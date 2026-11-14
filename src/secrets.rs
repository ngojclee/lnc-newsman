//! Encrypted provider secrets & Blueprint envelope vault (P2 + Blueprint v2).
//!
//! All newly written values are sealed with AES-256-GCM using a per-domain key derived
//! via HKDF-SHA256 from `LNC_MASTER_KEY` (`vault-kek`, `session-hmac`, `oidc-tx`,
//! `api-key-pepper`). Every write uses a fresh 12-byte random nonce and binds record AAD
//! so ciphertexts cannot be moved across records.
//!
//! Zero-loss compatibility: existing `secrets` rows written prior to v2 (`XChaCha20-Poly1305`
//! with raw `LNC_MASTER_KEY` + AAD `name`) continue opening transparently and upgrade to
//! `aesgcm:v1:...` on overwrite or `rewrap`.

use crate::{SharedState, db, failure, internal_error};
use aes_gcm::{Aes256Gcm, Nonce as AesNonce};
use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use base64::{
    Engine,
    engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD},
};
use chacha20poly1305::{
    KeyInit, XChaCha20Poly1305, XNonce,
    aead::{Aead, Payload},
};
use hkdf::Hkdf;
use hmac::{Hmac, Mac};
use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use zeroize::{Zeroize, ZeroizeOnDrop, Zeroizing};

const LEGACY_NONCE_BYTES: usize = 24;
const GCM_NONCE_BYTES: usize = 12;
const GCM_PREFIX: &str = "aesgcm:v1:";
const HKDF_SALT: &[u8] = b"lnc-v1";
const KEY_BYTES: usize = 32;
const MAX_SECRET_BYTES: usize = 8 * 1024;
const MAX_NAME: usize = 64;
pub const CURRENT_KEY_VERSION: i64 = 2;

type HmacSha256 = Hmac<Sha256>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SecretError {
    /// `LNC_MASTER_KEY` is missing or malformed.
    NotConfigured,
    /// The stored value cannot be decrypted with the available keys.
    Undecryptable,
    /// The name is not a lowercase slug or the value is too large.
    Invalid,
    Missing,
}

impl std::fmt::Display for SecretError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.code())
    }
}
impl std::error::Error for SecretError {}

impl SecretError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::NotConfigured => "secret_store_not_configured",
            Self::Undecryptable => "secret_undecryptable",
            Self::Invalid => "invalid_secret",
            Self::Missing => "not_found",
        }
    }
}

/// Domain-separated subkeys derived from a 32-byte master key via HKDF-SHA256.
#[derive(Clone, Zeroize, ZeroizeOnDrop)]
pub struct DerivedKeys {
    pub vault_kek: [u8; KEY_BYTES],
    pub session_hmac: [u8; KEY_BYTES],
    pub oidc_tx: [u8; KEY_BYTES],
    pub api_key_pepper: [u8; KEY_BYTES],
}

impl std::fmt::Debug for DerivedKeys {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("DerivedKeys{..}")
    }
}

pub fn derive_keys_from_master(master: &[u8; KEY_BYTES]) -> DerivedKeys {
    let hk = Hkdf::<Sha256>::new(Some(HKDF_SALT), master);
    let mut vault_kek = [0u8; KEY_BYTES];
    let mut session_hmac = [0u8; KEY_BYTES];
    let mut oidc_tx = [0u8; KEY_BYTES];
    let mut api_key_pepper = [0u8; KEY_BYTES];
    hk.expand(b"vault-kek", &mut vault_kek)
        .expect("32-byte HKDF output valid");
    hk.expand(b"session-hmac", &mut session_hmac)
        .expect("32-byte HKDF output valid");
    hk.expand(b"oidc-tx", &mut oidc_tx)
        .expect("32-byte HKDF output valid");
    hk.expand(b"api-key-pepper", &mut api_key_pepper)
        .expect("32-byte HKDF output valid");
    DerivedKeys {
        vault_kek,
        session_hmac,
        oidc_tx,
        api_key_pepper,
    }
}

/// Encrypt bytes using AES-256-GCM with a random 12-byte nonce and record AAD.
pub fn seal_with_key(
    key: &[u8; KEY_BYTES],
    aad: &[u8],
    plaintext: &[u8],
) -> Result<String, SecretError> {
    let mut nonce_bytes = [0u8; GCM_NONCE_BYTES];
    getrandom::fill(&mut nonce_bytes).map_err(|_| SecretError::Invalid)?;
    let cipher = Aes256Gcm::new(key.into());
    let sealed = cipher
        .encrypt(
            AesNonce::from_slice(&nonce_bytes),
            Payload {
                msg: plaintext,
                aad,
            },
        )
        .map_err(|_| SecretError::Invalid)?;
    let mut blob = Vec::with_capacity(GCM_NONCE_BYTES + sealed.len());
    blob.extend_from_slice(&nonce_bytes);
    blob.extend_from_slice(&sealed);
    Ok(format!("{GCM_PREFIX}{}", URL_SAFE_NO_PAD.encode(blob)))
}

/// Decrypt bytes sealed by `seal_with_key` verifying record AAD.
pub fn open_with_key(
    key: &[u8; KEY_BYTES],
    aad: &[u8],
    encoded: &str,
) -> Result<Vec<u8>, SecretError> {
    let rest = encoded
        .strip_prefix(GCM_PREFIX)
        .ok_or(SecretError::Undecryptable)?;
    let blob = URL_SAFE_NO_PAD
        .decode(rest)
        .or_else(|_| STANDARD.decode(rest))
        .map_err(|_| SecretError::Undecryptable)?;
    if blob.len() <= GCM_NONCE_BYTES {
        return Err(SecretError::Undecryptable);
    }
    let (nonce_bytes, body) = blob.split_at(GCM_NONCE_BYTES);
    let cipher = Aes256Gcm::new(key.into());
    cipher
        .decrypt(
            AesNonce::from_slice(nonce_bytes),
            Payload { msg: body, aad },
        )
        .map_err(|_| SecretError::Undecryptable)
}

/// The key ring. Only the current key is used for new writes; the old key can still
/// open rows that have not been rewrapped yet.
#[derive(Clone, Zeroize, ZeroizeOnDrop)]
pub struct SecretBox {
    current: [u8; KEY_BYTES],
    previous: Option<[u8; KEY_BYTES]>,
}

impl std::fmt::Debug for SecretBox {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Never print key material, not even derived hints.
        f.write_str("SecretBox{..}")
    }
}

/// Accepts 64 hex characters or 44-character base64. Anything else is refused rather
/// than silently stretched, so a weak or mistyped key cannot be used by accident.
fn parse_key(value: &str) -> Option<[u8; KEY_BYTES]> {
    let trimmed = value.trim();
    if trimmed.len() == KEY_BYTES * 2 && trimmed.bytes().all(|b| b.is_ascii_hexdigit()) {
        let mut key = [0u8; KEY_BYTES];
        for (index, byte) in key.iter_mut().enumerate() {
            *byte = u8::from_str_radix(&trimmed[index * 2..index * 2 + 2], 16).ok()?;
        }
        return Some(key);
    }
    let decoded = STANDARD
        .decode(trimmed)
        .or_else(|_| URL_SAFE_NO_PAD.decode(trimmed))
        .ok()?;
    decoded.try_into().ok()
}

impl SecretBox {
    /// `None` when `LNC_MASTER_KEY` is absent: the store stays closed rather than
    /// falling back to plaintext.
    pub fn from_env() -> Option<Self> {
        let current = parse_key(&std::env::var("LNC_MASTER_KEY").ok()?)?;
        let previous = std::env::var("LNC_MASTER_KEY_OLD")
            .ok()
            .and_then(|value| parse_key(&value));
        Some(Self { current, previous })
    }

    /// Explicit key material, for tests and for the connector framework tests.
    /// Production code uses [`SecretBox::from_env`].
    #[cfg(test)]
    pub fn from_bytes(current: [u8; KEY_BYTES], previous: Option<[u8; KEY_BYTES]>) -> Self {
        Self { current, previous }
    }

    pub fn derived_current(&self) -> DerivedKeys {
        derive_keys_from_master(&self.current)
    }

    pub fn derived_previous(&self) -> Option<DerivedKeys> {
        self.previous.as_ref().map(derive_keys_from_master)
    }

    pub fn hmac_api_key(&self, token: &str) -> String {
        let derived = self.derived_current();
        let mut mac = <HmacSha256 as hmac::Mac>::new_from_slice(&derived.api_key_pepper)
            .expect("HMAC accepts 32-byte key");
        mac.update(token.as_bytes());
        URL_SAFE_NO_PAD.encode(mac.finalize().into_bytes())
    }

    /// Seal a generic Blueprint secret envelope (`api_keys`, `ai_providers`, `mcp_connectors`)
    /// using HKDF-derived `vault_kek` + AES-256-GCM + record AAD.
    pub fn seal_envelope(&self, aad: &str, plaintext: &str) -> Result<String, SecretError> {
        if plaintext.is_empty() || plaintext.len() > MAX_SECRET_BYTES {
            return Err(SecretError::Invalid);
        }
        let derived = self.derived_current();
        seal_with_key(&derived.vault_kek, aad.as_bytes(), plaintext.as_bytes())
    }

    /// Open a Blueprint AES-256-GCM envelope using current or previous HKDF `vault_kek`.
    pub fn open_envelope(&self, aad: &str, encoded: &str) -> Result<String, SecretError> {
        let derived_curr = self.derived_current();
        if let Ok(plain) = open_with_key(&derived_curr.vault_kek, aad.as_bytes(), encoded) {
            let text = Zeroizing::new(
                String::from_utf8(plain).map_err(|_| SecretError::Undecryptable)?,
            );
            return Ok(text.to_string());
        }
        if let Some(derived_prev) = self.derived_previous()
            && let Ok(plain) = open_with_key(&derived_prev.vault_kek, aad.as_bytes(), encoded)
        {
            let text = Zeroizing::new(
                String::from_utf8(plain).map_err(|_| SecretError::Undecryptable)?,
            );
            return Ok(text.to_string());
        }
        Err(SecretError::Undecryptable)
    }

    fn legacy_cipher(key: &[u8; KEY_BYTES]) -> XChaCha20Poly1305 {
        XChaCha20Poly1305::new(key.into())
    }

    fn seal(&self, name: &str, plaintext: &[u8]) -> Result<String, SecretError> {
        let derived = self.derived_current();
        let aad = format!("secret:{name}");
        seal_with_key(&derived.vault_kek, aad.as_bytes(), plaintext)
    }

    fn open(&self, name: &str, encoded: &str) -> Result<String, SecretError> {
        if encoded.starts_with(GCM_PREFIX) {
            let aad_v2 = format!("secret:{name}");
            let curr = self.derived_current();
            for aad in [aad_v2.as_bytes(), name.as_bytes()] {
                if let Ok(plain) = open_with_key(&curr.vault_kek, aad, encoded) {
                    return String::from_utf8(plain).map_err(|_| SecretError::Undecryptable);
                }
            }
            if let Some(prev) = self.derived_previous() {
                for aad in [aad_v2.as_bytes(), name.as_bytes()] {
                    if let Ok(plain) = open_with_key(&prev.vault_kek, aad, encoded) {
                        return String::from_utf8(plain).map_err(|_| SecretError::Undecryptable);
                    }
                }
            }
            return Err(SecretError::Undecryptable);
        }

        // Legacy XChaCha20-Poly1305 zero-loss compatibility for pre-v2 DB rows.
        let blob = STANDARD
            .decode(encoded)
            .map_err(|_| SecretError::Undecryptable)?;
        if blob.len() <= LEGACY_NONCE_BYTES {
            return Err(SecretError::Undecryptable);
        }
        let (nonce, body) = blob.split_at(LEGACY_NONCE_BYTES);
        let nonce = XNonce::from_slice(nonce);
        let mut keys = vec![self.current];
        if let Some(previous) = self.previous {
            keys.push(previous);
        }
        for key in keys {
            if let Ok(plain) = Self::legacy_cipher(&key).decrypt(
                nonce,
                Payload {
                    msg: body,
                    aad: name.as_bytes(),
                },
            ) {
                return String::from_utf8(plain).map_err(|_| SecretError::Undecryptable);
            }
        }
        Err(SecretError::Undecryptable)
    }

    /// Startup self-verification over stored `aesgcm:v1:` records across vault tables.
    /// Reports how many records verified cleanly; logs errors without revealing secrets.
    pub fn verify_gcm_records(&self, db: &db::Database) -> usize {
        let mut checked = 0usize;
        if let Ok(mut stmt) = db
            .conn
            .prepare("SELECT name, ciphertext FROM secrets WHERE ciphertext LIKE 'aesgcm:v1:%'")
            && let Ok(rows) =
                stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))
        {
            for row in rows.flatten() {
                checked += 1;
                if self.open(&row.0, &row.1).is_err() {
                    tracing::error!(secret = %row.0, "boot AES-256-GCM envelope verification failed");
                }
            }
        }
        for (sql, prefix) in [
            ("SELECT id, secret_enc FROM api_keys WHERE secret_enc LIKE 'aesgcm:v1:%'", "api_key"),
            ("SELECT id, secret_enc FROM ai_providers WHERE secret_enc LIKE 'aesgcm:v1:%'", "provider"),
            ("SELECT id, secret_enc FROM mcp_connectors WHERE secret_enc LIKE 'aesgcm:v1:%'", "mcp_connector"),
        ] {
            if let Ok(mut stmt) = db.conn.prepare(sql)
                && let Ok(rows) =
                    stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))
            {
                for row in rows.flatten() {
                    checked += 1;
                    let aad = format!("{prefix}:{}", row.0);
                    if self.open_envelope(&aad, &row.1).is_err() {
                        tracing::error!(record = %aad, "boot AES-256-GCM envelope verification failed");
                    }
                }
            }
        }
        checked
    }

    /// A short, non-reversible fingerprint so an operator can tell two keys apart
    /// without seeing either of them.
    pub fn fingerprint(&self) -> String {
        let digest = Sha256::digest(self.current);
        digest[..4].iter().map(|b| format!("{b:02x}")).collect()
    }
}

fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= MAX_NAME
        && !name.starts_with('-')
        && !name.ends_with('-')
        && !name.contains("--")
        && name
            .bytes()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'-')
}

fn vault(state: &SharedState) -> Result<&SecretBox, SecretError> {
    state.secrets.as_ref().ok_or(SecretError::NotConfigured)
}

pub fn set(
    db: &db::Database,
    vault: &SecretBox,
    name: &str,
    value: &str,
) -> Result<(), SecretError> {
    if !valid_name(name) || value.is_empty() || value.len() > MAX_SECRET_BYTES {
        return Err(SecretError::Invalid);
    }
    let sealed = vault.seal(name, value.as_bytes())?;
    db.conn
        .execute(
            "INSERT INTO secrets(name,ciphertext,key_version) VALUES (?1,?2,?3)
             ON CONFLICT(name) DO UPDATE SET ciphertext=excluded.ciphertext,key_version=excluded.key_version,updated_at=CURRENT_TIMESTAMP",
            params![name, sealed, CURRENT_KEY_VERSION],
        )
        .map_err(|_| SecretError::Invalid)?;
    Ok(())
}

/// Only connectors may call this; no HTTP handler returns the value.
#[allow(dead_code)]
pub fn get(db: &db::Database, vault: &SecretBox, name: &str) -> Result<String, SecretError> {
    let encoded: Option<String> = db
        .conn
        .query_row(
            "SELECT ciphertext FROM secrets WHERE name=?1",
            [name],
            |r| r.get(0),
        )
        .optional()
        .map_err(|_| SecretError::Undecryptable)?;
    vault.open(name, &encoded.ok_or(SecretError::Missing)?)
}

pub fn delete(db: &db::Database, name: &str) -> Result<bool, SecretError> {
    let removed = db
        .conn
        .execute("DELETE FROM secrets WHERE name=?1", [name])
        .map_err(|_| SecretError::Invalid)?;
    Ok(removed > 0)
}

/// Re-seals every row that the old key sealed under the current key, with a new nonce.
/// Returns how many rows were rewrapped. A row that neither key can open aborts the
/// whole operation and is left untouched, so a wrong `LNC_MASTER_KEY` cannot destroy
/// existing secrets.
pub fn rewrap(db: &mut db::Database, vault: &SecretBox) -> anyhow::Result<usize> {
    let rows: Vec<(String, String, i64)> = {
        let mut stmt = db
            .conn
            .prepare("SELECT name,ciphertext,key_version FROM secrets ORDER BY name")?;
        stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
            .collect::<rusqlite::Result<Vec<_>>>()?
    };
    // Decrypt everything first: if any row fails, nothing is written.
    let mut rewrapped = Vec::new();
    for (name, ciphertext, _version) in &rows {
        let plain = vault.open(name, ciphertext).map_err(|_| {
            anyhow::anyhow!("secret {name} cannot be opened with the current or previous key")
        })?;
        rewrapped.push((name.clone(), vault.seal(name, plain.as_bytes())?));
    }
    let tx = db.conn.transaction()?;
    for (name, ciphertext) in &rewrapped {
        tx.execute(
            "UPDATE secrets SET ciphertext=?2, key_version=?3, updated_at=CURRENT_TIMESTAMP WHERE name=?1",
            params![name, ciphertext, CURRENT_KEY_VERSION],
        )?;
    }
    tx.commit()?;
    Ok(rewrapped.len())
}

// ---------- admin endpoints (metadata only) ----------

#[derive(Debug, Serialize)]
pub struct SecretMeta {
    name: String,
    key_version: i64,
    updated_at: String,
}

fn metadata(db: &db::Database) -> rusqlite::Result<Vec<SecretMeta>> {
    let mut stmt = db
        .conn
        .prepare("SELECT name,key_version,updated_at FROM secrets ORDER BY name")?;
    let rows = stmt.query_map([], |r| {
        Ok(SecretMeta {
            name: r.get(0)?,
            key_version: r.get(1)?,
            updated_at: r.get(2)?,
        })
    })?;
    rows.collect()
}

fn secret_response(err: SecretError) -> Response {
    let status = match err {
        SecretError::NotConfigured => StatusCode::SERVICE_UNAVAILABLE,
        SecretError::Missing => StatusCode::NOT_FOUND,
        SecretError::Invalid => StatusCode::BAD_REQUEST,
        SecretError::Undecryptable => StatusCode::CONFLICT,
    };
    failure(status, err.code())
}

/// Names and key versions only. There is deliberately no "reveal" action.
pub async fn list(State(state): State<SharedState>) -> Response {
    let Ok(_vault) = vault(&state) else {
        return failure(
            StatusCode::SERVICE_UNAVAILABLE,
            "secret_store_not_configured",
        );
    };
    let db = state.db.lock().expect("database lock poisoned");
    match metadata(&db) {
        Ok(data) => Json(serde_json::json!({
            "data": data,
            "key_fingerprint": vault(&state).map(SecretBox::fingerprint).ok(),
        }))
        .into_response(),
        Err(err) => internal_error(err),
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SecretInput {
    value: String,
}

pub async fn put(
    State(state): State<SharedState>,
    Path(name): Path<String>,
    Json(input): Json<SecretInput>,
) -> Response {
    let Ok(vault) = vault(&state) else {
        return failure(
            StatusCode::SERVICE_UNAVAILABLE,
            "secret_store_not_configured",
        );
    };
    let result = set(
        &state.db.lock().expect("database lock poisoned"),
        vault,
        &name,
        &input.value,
    );
    match result {
        Ok(()) => Json(serde_json::json!({ "ok": true, "name": name })).into_response(),
        Err(err) => secret_response(err),
    }
}

pub async fn remove(State(state): State<SharedState>, Path(name): Path<String>) -> Response {
    let result = delete(&state.db.lock().expect("database lock poisoned"), &name);
    match result {
        Ok(true) => Json(serde_json::json!({ "ok": true })).into_response(),
        Ok(false) => failure(StatusCode::NOT_FOUND, "not_found"),
        Err(err) => secret_response(err),
    }
}

pub async fn rewrap_handler(State(state): State<SharedState>) -> Response {
    let Ok(vault) = vault(&state) else {
        return failure(
            StatusCode::SERVICE_UNAVAILABLE,
            "secret_store_not_configured",
        );
    };
    let result = rewrap(&mut state.db.lock().expect("database lock poisoned"), vault);
    match result {
        Ok(rewrapped) => {
            Json(serde_json::json!({ "ok": true, "rewrapped": rewrapped })).into_response()
        }
        Err(err) => {
            // Details are logged, never returned: they may mention secret names.
            tracing::error!(%err, "secret rewrap failed");
            failure(StatusCode::CONFLICT, "secret_rewrap_failed")
        }
    }
}

#[cfg(test)]
#[path = "secrets_tests.rs"]
mod tests;
