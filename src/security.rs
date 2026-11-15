use crate::{
    SharedState, auth, db, failure, now,
    oidc::{OidcCache, OidcConfig},
    secrets::{self, SecretBox},
};
use axum::http::{HeaderMap, HeaderValue, Method, Request, StatusCode, header, uri::Authority};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use hkdf::Hkdf;
use hmac::{Hmac, Mac};
use rusqlite::{OptionalExtension, params};
use sha2::{Digest, Sha256};
use std::sync::{Mutex, RwLock};
use subtle::ConstantTimeEq;
use url::Url;
use zeroize::{Zeroize, ZeroizeOnDrop};

type HmacSha256 = Hmac<Sha256>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CookieSecureMode {
    Always,
    Never,
    Auto,
}

impl CookieSecureMode {
    pub fn from_env() -> Self {
        match std::env::var("COOKIE_SECURE")
            .unwrap_or_else(|_| "auto".to_string())
            .trim()
            .to_ascii_lowercase()
            .as_str()
        {
            "true" | "1" => Self::Always,
            "false" | "0" => Self::Never,
            _ => Self::Auto,
        }
    }
}

fn parse_ttl_env(name: &str, default_secs: u64, min_secs: u64, max_secs: u64) -> u64 {
    std::env::var(name)
        .ok()
        .and_then(|v| v.trim().parse::<u64>().ok())
        .map(|v| v.clamp(min_secs, max_secs))
        .unwrap_or(default_secs)
}

pub fn sha256_hex(input: &str) -> String {
    Sha256::digest(input.as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// Parses an entry from `LNC_API_KEYS`: accepts either `label:secret`, `label=secret`,
/// or bare `secret`.
pub fn parse_env_api_key_entry(raw_entry: &str, index: usize) -> Option<(String, String)> {
    let trimmed = raw_entry.trim();
    if trimmed.is_empty() {
        return None;
    }
    for sep in [':', '='] {
        if let Some((left, right)) = trimmed.split_once(sep) {
            let lbl = left.trim();
            let sec = right.trim();
            if !lbl.is_empty()
                && lbl.len() <= 48
                && sec.len() >= 6
                && !lbl.starts_with("nm_")
                && !lbl.starts_with("sk-")
            {
                return Some((lbl.to_string(), sec.to_string()));
            }
        }
    }
    Some((
        format!("Bootstrap Key #{}", index + 1),
        trimmed.to_string(),
    ))
}

fn first_nonempty_env(keys: &[&str]) -> Option<String> {
    for k in keys {
        if let Ok(val) = std::env::var(k) {
            let trimmed = val.trim();
            if !trimmed.is_empty() {
                return Some(trimmed.to_string());
            }
        }
    }
    None
}

pub fn hub_url_from_env() -> Option<String> {
    first_nonempty_env(&[
        "LNC_AGENTS_HUB_URL",
        "LNC_AGENT_HUB_URL",
        "AGENT_HUB_URL",
        "HUB_BASE_URL",
    ])
    .map(|u| u.trim_end_matches('/').to_string())
}

pub fn hub_token_from_env() -> Option<String> {
    first_nonempty_env(&[
        "LNC_AGENTS_HUB_TOKEN",
        "LNC_AGENT_HUB_TOKEN",
        "LNC_AGENT_HUB_API_KEY",
        "AGENT_HUB_TOKEN",
        "LNC_HUB_TOKEN",
    ])
}

/// One-time `cfg_or_env` seeding with SHA-256 digests so operator UI edits, rotations,
/// revocations, and deletions persist across restarts without env resurrection.
pub fn seed_bootstrap_env(
    db: &db::Database,
    cfg: &crate::config::Config,
    secrets: Option<&SecretBox>,
) {
    let Some(vault) = secrets else {
        return;
    };
    let ts_iso = chrono::Utc::now().to_rfc3339();
    let ts_unix = now() as i64;

    // 1. Seed LNC_API_KEYS once per secret digest
    for (idx, raw_entry) in cfg.api_keys.iter().enumerate() {
        let Some((label, raw_secret)) = parse_env_api_key_entry(raw_entry, idx) else {
            continue;
        };
        if raw_secret.len() < 8 {
            continue;
        }
        let digest = sha256_hex(&raw_secret);
        let digest_key = format!("seed_digest:api_key:{digest}");
        let already: Option<String> = db
            .conn
            .query_row(
                "SELECT value FROM settings WHERE key = ?1",
                [&digest_key],
                |r| r.get(0),
            )
            .optional()
            .ok()
            .flatten();
        if already.as_deref() == Some("1") {
            continue;
        }
        let id = format!("ak-seed-{}", &digest[..12]);
        let prefix: String = raw_secret.chars().take(8).collect();
        let last4: String = raw_secret
            .chars()
            .rev()
            .take(4)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect();
        let hmac_val = vault.hmac_api_key(&raw_secret);
        let Ok(enc) = vault.seal_envelope(&format!("api_key:{id}"), &raw_secret) else {
            continue;
        };
        let _ = db.conn.execute(
            "INSERT OR IGNORE INTO api_keys
                (id, name, prefix, last4, key_hash_hmac, secret_enc, copy_available, scopes_json, status, source, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, 1, '[\"newsman:read\",\"newsman:write\"]', 'active', 'env_seed', ?7, ?7)",
            params![id, label, prefix, last4, hmac_val, enc, ts_unix],
        );
        let _ = db.conn.execute(
            "INSERT INTO settings(key, value) VALUES (?1, '1')
             ON CONFLICT(key) DO UPDATE SET value = '1'",
            [&digest_key],
        );
    }

    // 2. Seed Rule T2 LNC-Agent-Hub URL + Token once per digest
    let env_hub_url = hub_url_from_env().unwrap_or_default();
    let env_hub_tok = hub_token_from_env().unwrap_or_default();
    if !env_hub_url.is_empty() || !env_hub_tok.is_empty() {
        let hub_digest = sha256_hex(&format!("{env_hub_url}|{env_hub_tok}"));
        let prev_hub: Option<String> = db
            .conn
            .query_row(
                "SELECT value FROM settings WHERE key = 'seed_digest:hub'",
                [],
                |r| r.get(0),
            )
            .optional()
            .ok()
            .flatten();
        if prev_hub.as_deref() != Some(hub_digest.as_str()) {
            if !env_hub_url.is_empty() {
                let _ = db.conn.execute(
                    "UPDATE app_agent_bridge_config SET hub_url = ?1, updated_at = ?2
                     WHERE id = 1 AND trim(hub_url) = ''",
                    params![env_hub_url, ts_iso],
                );
            }
            if !env_hub_tok.is_empty() {
                let has_bridge_tok: i64 = db
                    .conn
                    .query_row(
                        "SELECT COUNT(*) FROM secrets WHERE name = ?1",
                        [crate::agent_bridge::SEC_HUB_TOKEN],
                        |r| r.get(0),
                    )
                    .unwrap_or(0);
                if has_bridge_tok == 0 {
                    let _ = secrets::set(db, vault, crate::agent_bridge::SEC_HUB_TOKEN, &env_hub_tok);
                    let _ = secrets::set(db, vault, crate::hub::SECRET_NAME, &env_hub_tok);
                }
            }
            let _ = db.conn.execute(
                "INSERT INTO settings(key, value) VALUES ('seed_digest:hub', ?1)
                 ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                [&hub_digest],
            );
        }
    }

    // 3. Seed Inbound MCP Connector secrets (CONNECTOR_BEARER / DELEGATION_SECRET) once per digest
    let env_cb = first_nonempty_env(&["CONNECTOR_BEARER"]).unwrap_or_default();
    let env_ds = first_nonempty_env(&["DELEGATION_SECRET"]).unwrap_or_default();
    if !env_cb.is_empty() || !env_ds.is_empty() {
        let conn_digest = sha256_hex(&format!("{env_cb}|{env_ds}"));
        let prev_conn: Option<String> = db
            .conn
            .query_row(
                "SELECT value FROM settings WHERE key = 'seed_digest:connector'",
                [],
                |r| r.get(0),
            )
            .optional()
            .ok()
            .flatten();
        if prev_conn.as_deref() != Some(conn_digest.as_str()) {
            if !env_cb.is_empty() && secrets::get(db, vault, crate::connector::BEARER_SECRET).is_err() {
                let _ = secrets::set(db, vault, crate::connector::BEARER_SECRET, &env_cb);
            }
            if !env_ds.is_empty() && secrets::get(db, vault, crate::connector::DELEGATION_SECRET).is_err() {
                let _ = secrets::set(db, vault, crate::connector::DELEGATION_SECRET, &env_ds);
            }
            let _ = db.conn.execute(
                "INSERT INTO settings(key, value) VALUES ('seed_digest:connector', ?1)
                 ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                [&conn_digest],
            );
        }
    }
}

#[derive(Zeroize, ZeroizeOnDrop)]
struct EphemeralKey([u8; 32]);

pub struct SecurityRuntime {
    ephemeral: EphemeralKey,
    pub cookie_secure_mode: CookieSecureMode,
    pub session_idle_secs: u64,
    pub session_max_secs: u64,
    pub public_base_url: Option<String>,
    pub oidc: RwLock<OidcConfig>,
    pub oidc_cache: Mutex<OidcCache>,
    pub oidc_limiter: Mutex<auth::WindowLimiter>,
    pub copy_limiter: Mutex<auth::WindowLimiter>,
}

impl SecurityRuntime {
    pub fn from_env(
        db: &db::Database,
        cfg: &crate::config::Config,
        secrets: Option<&SecretBox>,
    ) -> Self {
        let mut ephemeral = [0u8; 32];
        let _ = getrandom::fill(&mut ephemeral);
        if let Err(err) = db.ensure_bootstrap_management_key(cfg.management_key.as_deref(), now()) {
            tracing::warn!(%err, "failed to bootstrap management key hash");
        }
        if let Some(vault) = secrets {
            match secrets::boot_rotate_vault_and_keys(db, vault) {
                Ok(rotated) if rotated > 0 => {
                    tracing::info!(
                        rotated,
                        "upgraded/rotated stored vault envelopes & HMAC digests to $enc:1:"
                    );
                }
                Ok(_) => {}
                Err(err) => {
                    tracing::warn!(%err, "boot vault envelope rotation encountered an error");
                }
            }
            seed_bootstrap_env(db, cfg, Some(vault));
            let checked = vault.verify_gcm_records(db);
            tracing::info!(checked, "verified stored AES-256-GCM records on startup");
        }
        let oidc_cfg = OidcConfig::load_or_seed(db, secrets);
        let public_base_url = std::env::var("PUBLIC_BASE_URL")
            .ok()
            .map(|v| v.trim().trim_end_matches('/').to_string())
            .filter(|v| !v.is_empty());
        Self {
            ephemeral: EphemeralKey(ephemeral),
            cookie_secure_mode: CookieSecureMode::from_env(),
            session_idle_secs: parse_ttl_env(
                "SESSION_IDLE_TTL_SECONDS",
                auth::SESSION_IDLE_SECONDS,
                60,
                2_592_000,
            ),
            session_max_secs: parse_ttl_env(
                "SESSION_MAX_TTL_SECONDS",
                auth::SESSION_MAX_SECONDS,
                300,
                7_776_000,
            ),
            public_base_url,
            oidc: RwLock::new(oidc_cfg),
            oidc_cache: Mutex::new(OidcCache::default()),
            oidc_limiter: Mutex::new(auth::oidc_limiter()),
            copy_limiter: Mutex::new(auth::copy_limiter()),
        }
    }

    pub fn for_test(cfg: &crate::config::Config) -> Self {
        let mut ephemeral = [0u8; 32];
        let _ = getrandom::fill(&mut ephemeral);
        let cookie_secure_mode = if cfg.secure_cookie {
            CookieSecureMode::Always
        } else {
            CookieSecureMode::Never
        };
        Self {
            ephemeral: EphemeralKey(ephemeral),
            cookie_secure_mode,
            session_idle_secs: auth::SESSION_IDLE_SECONDS,
            session_max_secs: auth::SESSION_MAX_SECONDS,
            public_base_url: None,
            oidc: RwLock::new(OidcConfig::default()),
            oidc_cache: Mutex::new(OidcCache::default()),
            oidc_limiter: Mutex::new(auth::oidc_limiter()),
            copy_limiter: Mutex::new(auth::copy_limiter()),
        }
    }

    pub fn oidc_cfg(&self) -> OidcConfig {
        self.oidc
            .read()
            .expect("oidc config lock poisoned")
            .clone()
    }

    pub fn set_oidc_cfg(&self, next: OidcConfig) {
        *self.oidc.write().expect("oidc config lock poisoned") = next;
        let mut cache = self.oidc_cache.lock().expect("oidc cache lock poisoned");
        cache.discovery = None;
        cache.jwks = None;
    }

    pub fn is_cookie_secure(&self, headers: &HeaderMap) -> bool {
        match self.cookie_secure_mode {
            CookieSecureMode::Always => true,
            CookieSecureMode::Never => false,
            CookieSecureMode::Auto => {
                if headers
                    .get("x-forwarded-proto")
                    .and_then(|v| v.to_str().ok())
                    .is_some_and(|v| {
                        v.split(',')
                            .next()
                            .unwrap_or("")
                            .trim()
                            .eq_ignore_ascii_case("https")
                    })
                {
                    return true;
                }
                headers
                    .get(header::ORIGIN)
                    .or_else(|| headers.get(header::REFERER))
                    .and_then(|v| v.to_str().ok())
                    .is_some_and(|v| v.trim().to_ascii_lowercase().starts_with("https://"))
            }
        }
    }

    pub fn session_hmac_key(
        &self,
        secrets: Option<&SecretBox>,
        mgmt_key: Option<&str>,
    ) -> [u8; 32] {
        if let Some(vault) = secrets {
            return vault.derived_current().session_hmac;
        }
        let seed = mgmt_key
            .map(str::as_bytes)
            .filter(|b| !b.is_empty())
            .unwrap_or(&self.ephemeral.0);
        let hk = Hkdf::<Sha256>::new(Some(b"lnc-newsman-session"), seed);
        let mut out = [0u8; 32];
        hk.expand(b"session-hmac", &mut out)
            .expect("32-byte HKDF output valid");
        out
    }

    pub fn oidc_tx_key(&self, secrets: Option<&SecretBox>) -> [u8; 32] {
        if let Some(vault) = secrets {
            return vault.derived_current().oidc_tx;
        }
        self.ephemeral.0
    }

    pub fn api_key_pepper(&self, secrets: Option<&SecretBox>) -> [u8; 32] {
        if let Some(vault) = secrets {
            return vault.derived_current().api_key_pepper;
        }
        self.ephemeral.0
    }
}

pub fn hmac_token_with_pepper(pepper: &[u8; 32], token: &str) -> String {
    let mut mac = HmacSha256::new_from_slice(pepper).expect("HMAC accepts 32-byte key");
    mac.update(token.as_bytes());
    URL_SAFE_NO_PAD.encode(mac.finalize().into_bytes())
}

pub fn build_cookie_header(
    name: &str,
    value: &str,
    path: &str,
    max_age_secs: u64,
    secure: bool,
) -> HeaderValue {
    let raw = format!(
        "{name}={value}; Path={path}; HttpOnly; SameSite=Lax; Max-Age={max_age_secs}{}",
        if secure { "; Secure" } else { "" }
    );
    HeaderValue::from_str(&raw).expect("valid Set-Cookie header")
}

pub fn clear_cookie_header(name: &str, path: &str, secure: bool) -> HeaderValue {
    build_cookie_header(name, "", path, 0, secure)
}

pub fn request_client_ip(
    state: &SharedState,
    peer: Option<&axum::Extension<axum::extract::ConnectInfo<std::net::SocketAddr>>>,
    headers: &HeaderMap,
) -> String {
    let peer_ip = peer
        .map(|axum::Extension(axum::extract::ConnectInfo(addr))| addr.ip().to_string())
        .unwrap_or_else(|| "unknown".into());
    let forwarded_for = headers.get("x-forwarded-for").and_then(|v| v.to_str().ok());
    auth::client_key(&peer_ip, forwarded_for, &state.cfg.trusted_proxies)
}

pub fn issue_session_cookie(
    state: &SharedState,
    headers: &HeaderMap,
    auth_method: &str,
    username: &str,
    client_ip: &str,
) -> anyhow::Result<HeaderValue> {
    let key = state
        .security
        .session_hmac_key(state.secrets.as_ref(), state.cfg.management_key.as_deref());
    let token = auth::new_token_bytes(&key)?;
    let timestamp = now();
    let idle_ttl = state.security.session_idle_secs;
    let max_ttl = state.security.session_max_secs;
    let db = state.db.lock().expect("database lock poisoned");
    let epoch = db.current_session_epoch().unwrap_or(1);
    db.create_blueprint_session(
        &auth::token_hash(&token),
        timestamp + idle_ttl,
        timestamp + max_ttl,
        epoch,
        auth_method,
        username,
        timestamp,
    )?;
    let _ = db.record_audit_event(
        timestamp,
        username,
        auth_method,
        client_ip,
        "auth.session_created",
        "session",
        "",
        &serde_json::json!({ "epoch": epoch }),
    );
    let secure = state.security.is_cookie_secure(headers);
    Ok(build_cookie_header(
        auth::COOKIE_NAME,
        &token,
        "/",
        max_ttl,
        secure,
    ))
}

/// Validates session cookie signature, idle/max TTL, and `session_epoch`.
/// Returns `(auth_method, username)` when valid.
pub fn validate_session_identity(
    state: &SharedState,
    headers: &HeaderMap,
) -> Option<(String, String)> {
    let token = auth::cookie_value(headers)?;
    let hmac_key = state
        .security
        .session_hmac_key(state.secrets.as_ref(), state.cfg.management_key.as_deref());
    let sig_ok = auth::valid_signature_bytes(token, &hmac_key)
        || state
            .cfg
            .management_key
            .as_deref()
            .is_some_and(|k| auth::valid_signature(token, k));
    if !sig_ok {
        return None;
    }
    let db = state.db.lock().expect("database lock poisoned");
    db.validate_and_touch_session(
        &auth::token_hash(token),
        now(),
        state.security.session_idle_secs,
    )
    .ok()
    .flatten()
}

pub fn validate_api_key_request(state: &SharedState, headers: &HeaderMap) -> bool {
    let Some(supplied) = auth::extract_api_key(headers) else {
        return false;
    };
    let pepper = state.security.api_key_pepper(state.secrets.as_ref());
    let hash_hex = hmac_token_with_pepper(&pepper, supplied);
    let db = state.db.lock().expect("database lock poisoned");
    let row: Option<(String, String)> = db
        .conn
        .query_row(
            "SELECT id, key_hash_hmac FROM api_keys WHERE key_hash_hmac = ?1 AND status = 'active' LIMIT 1",
            [&hash_hex],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()
        .ok()
        .flatten();
    if let Some((id, stored_hash)) = row
        && bool::from(stored_hash.as_bytes().ct_eq(hash_hex.as_bytes()))
    {
        let _ = db.conn.execute(
            "UPDATE api_keys SET last_used_at = ?2 WHERE id = ?1",
            params![id, now()],
        );
        return true;
    }

    // Fallback to raw cfg.api_keys ONLY when that bootstrap key has not been seeded into
    // `api_keys` (e.g. legacy in-memory tests running without a vault). Once seeded,
    // rotation/revocation/deletion in `api_keys` is authoritative and non-resurrecting.
    let digest_key = format!("seed_digest:api_key:{}", sha256_hex(supplied));
    let was_seeded: Option<String> = db
        .conn
        .query_row(
            "SELECT value FROM settings WHERE key = ?1",
            [&digest_key],
            |r| r.get(0),
        )
        .optional()
        .ok()
        .flatten();
    if was_seeded.as_deref() == Some("1") {
        return false;
    }
    let clean_keys: Vec<String> = state
        .cfg
        .api_keys
        .iter()
        .enumerate()
        .filter_map(|(i, e)| parse_env_api_key_entry(e, i).map(|(_, s)| s))
        .collect();
    auth::has_api_key(headers, &clean_keys)
}

fn origin_matches_host(candidate_origin: &str, allowed_hosts: &[String]) -> bool {
    let Ok(parsed) = Url::parse(candidate_origin.trim()) else {
        return false;
    };
    if !matches!(parsed.scheme(), "http" | "https") {
        return false;
    }
    let Some(host) = parsed.host_str() else {
        return false;
    };
    let host_lower = host.trim_matches(['[', ']']).to_ascii_lowercase();
    let host_port = match parsed.port() {
        Some(port) => format!("{host_lower}:{port}"),
        None => host_lower.clone(),
    };
    allowed_hosts.iter().any(|allowed| {
        let a = allowed.trim_matches(['[', ']']).to_ascii_lowercase();
        a == host_lower || a == host_port
    })
}

/// Strict CSRF guard: enforces `X-Requested-With: XMLHttpRequest`, rejects `cross-site`
/// `Sec-Fetch-Site`, and checks `Origin` / `Referer` strictly against validated `Host` /
/// `PUBLIC_BASE_URL` / `ALLOWED_HOSTS` while ignoring `X-Forwarded-Host` spoofing.
#[allow(clippy::result_large_err)]
pub fn enforce_csrf(
    state: &SharedState,
    req: &Request<axum::body::Body>,
) -> Result<(), axum::response::Response> {
    if matches!(*req.method(), Method::GET | Method::HEAD | Method::OPTIONS) {
        return Ok(());
    }
    let headers = req.headers();
    if let Some(sfs) = headers.get("sec-fetch-site").and_then(|v| v.to_str().ok()) {
        let lower = sfs.trim().to_ascii_lowercase();
        if !matches!(lower.as_str(), "same-origin" | "same-site" | "none") {
            return Err(failure(StatusCode::FORBIDDEN, "csrf_origin_mismatch"));
        }
    }
    let mut allowed_origins = Vec::new();
    if let Some(host_val) = headers.get(header::HOST).and_then(|v| v.to_str().ok())
        && let Ok(auth) = host_val.parse::<Authority>()
    {
        let h = auth.host().trim_matches(['[', ']']).to_ascii_lowercase();
        allowed_origins.push(h.clone());
        if let Some(p) = auth.port_u16() {
            allowed_origins.push(format!("{h}:{p}"));
        }
    }
    if let Some(ref pub_url) = state.security.public_base_url
        && let Ok(parsed) = Url::parse(pub_url)
        && let Some(h) = parsed.host_str()
    {
        let h_low = h.trim_matches(['[', ']']).to_ascii_lowercase();
        allowed_origins.push(h_low.clone());
        if let Some(p) = parsed.port() {
            allowed_origins.push(format!("{h_low}:{p}"));
        }
    }
    for h in &state.cfg.allowed_hosts {
        let clean = h.trim().trim_matches(['[', ']']).to_ascii_lowercase();
        if !clean.is_empty() && clean != "*" {
            allowed_origins.push(clean);
        }
    }
    if let Some(origin) = headers.get(header::ORIGIN).and_then(|v| v.to_str().ok()) {
        if origin.trim() == "null" || !origin_matches_host(origin, &allowed_origins) {
            return Err(failure(StatusCode::FORBIDDEN, "csrf_origin_mismatch"));
        }
    } else if let Some(referer) = headers.get(header::REFERER).and_then(|v| v.to_str().ok())
        && !origin_matches_host(referer, &allowed_origins)
    {
        return Err(failure(StatusCode::FORBIDDEN, "csrf_origin_mismatch"));
    }
    if headers
        .get("x-requested-with")
        .and_then(|v| v.to_str().ok())
        != Some("XMLHttpRequest")
    {
        return Err(failure(StatusCode::FORBIDDEN, "csrf_header_required"));
    }
    Ok(())
}
