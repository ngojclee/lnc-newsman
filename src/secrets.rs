//! Encrypted provider secrets (P2 foundation).
//!
//! Values are sealed with XChaCha20-Poly1305 under `LNC_MASTER_KEY`. Every write uses
//! a fresh 24-byte random nonce, and the secret *name* is bound as associated data so
//! a ciphertext cannot be moved to another name. The plaintext is never returned by
//! any endpoint, never logged and never rendered in the UI: the admin surface exposes
//! only the name, which key version sealed it, and when it changed.
//!
//! Key rotation: `LNC_MASTER_KEY_OLD` holds the previous key. `rewrap` decrypts with
//! whichever key sealed the row and re-encrypts under the current key with a **new**
//! nonce; plaintext only exists in memory for the duration of that call.

use crate::{SharedState, db, failure, internal_error};
use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use base64::{Engine, engine::general_purpose::STANDARD};
use chacha20poly1305::{
    AeadCore, KeyInit, XChaCha20Poly1305, XNonce,
    aead::{Aead, OsRng, Payload},
};
use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

const NONCE_BYTES: usize = 24;
const KEY_BYTES: usize = 32;
const MAX_SECRET_BYTES: usize = 8 * 1024;
const MAX_NAME: usize = 64;
pub const CURRENT_KEY_VERSION: i64 = 1;

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

/// The key ring. Only the current key is used for new writes; the old key can still
/// open rows that have not been rewrapped yet.
#[derive(Clone)]
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
    let decoded = STANDARD.decode(trimmed).ok()?;
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

    fn cipher(key: &[u8; KEY_BYTES]) -> XChaCha20Poly1305 {
        XChaCha20Poly1305::new(key.into())
    }

    fn seal(&self, name: &str, plaintext: &[u8]) -> Result<String, SecretError> {
        let nonce = XChaCha20Poly1305::generate_nonce(&mut OsRng);
        let sealed = Self::cipher(&self.current)
            .encrypt(
                &nonce,
                Payload {
                    msg: plaintext,
                    aad: name.as_bytes(),
                },
            )
            .map_err(|_| SecretError::Invalid)?;
        let mut blob = Vec::with_capacity(NONCE_BYTES + sealed.len());
        blob.extend_from_slice(&nonce);
        blob.extend_from_slice(&sealed);
        Ok(STANDARD.encode(blob))
    }

    fn open(&self, name: &str, encoded: &str) -> Result<String, SecretError> {
        let blob = STANDARD
            .decode(encoded)
            .map_err(|_| SecretError::Undecryptable)?;
        if blob.len() <= NONCE_BYTES {
            return Err(SecretError::Undecryptable);
        }
        let (nonce, body) = blob.split_at(NONCE_BYTES);
        let nonce = XNonce::from_slice(nonce);
        let mut keys = vec![self.current];
        if let Some(previous) = self.previous {
            keys.push(previous);
        }
        for key in keys {
            if let Ok(plain) = Self::cipher(&key).decrypt(
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
