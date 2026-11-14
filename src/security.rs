use crate::{
    SharedState, auth, db, failure, now,
    oidc::{OidcCache, OidcConfig},
    secrets::SecretBox,
};
use axum::http::{HeaderMap, HeaderValue, Method, Request, StatusCode, header, uri::Authority};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use hkdf::Hkdf;
use hmac::{Hmac, Mac};
use rusqlite::{OptionalExtension, params};
use sha2::Sha256;
use std::sync::Mutex;
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

#[derive(Zeroize, ZeroizeOnDrop)]
struct EphemeralKey([u8; 32]);

pub struct SecurityRuntime {
    ephemeral: EphemeralKey,
    pub cookie_secure_mode: CookieSecureMode,
    pub public_base_url: Option<String>,
    pub oidc: OidcConfig,
    pub oidc_cache: Mutex<OidcCache>,
    pub oidc_limiter: Mutex<auth::WindowLimiter>,
    pub copy_limiter: Mutex<auth::WindowLimiter>,
}

impl SecurityRuntime {
    pub fn from_env(db: &db::Database, cfg: &crate::config::Config, secrets: Option<&SecretBox>) -> Self {
        let mut ephemeral = [0u8; 32];
        let _ = getrandom::fill(&mut ephemeral);
        if let Err(err) = db.ensure_bootstrap_management_key(cfg.management_key.as_deref(), now()) {
            tracing::warn!(%err, "failed to bootstrap management key hash");
        }
        if let Some(vault) = secrets {
            let checked = vault.verify_gcm_records(db);
            tracing::info!(checked, "verified stored AES-256-GCM records on startup");
        }
        let public_base_url = std::env::var("PUBLIC_BASE_URL")
            .ok()
            .map(|v| v.trim().trim_end_matches('/').to_string())
            .filter(|v| !v.is_empty());
        Self {
            ephemeral: EphemeralKey(ephemeral),
            cookie_secure_mode: CookieSecureMode::from_env(),
            public_base_url,
            oidc: OidcConfig::from_env(),
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
            public_base_url: None,
            oidc: OidcConfig::default(),
            oidc_cache: Mutex::new(OidcCache::default()),
            oidc_limiter: Mutex::new(auth::oidc_limiter()),
            copy_limiter: Mutex::new(auth::copy_limiter()),
        }
    }

    pub fn is_cookie_secure(&self, headers: &HeaderMap) -> bool {
        match self.cookie_secure_mode {
            CookieSecureMode::Always => true,
            CookieSecureMode::Never => false,
            CookieSecureMode::Auto => {
                if headers
                    .get("x-forwarded-proto")
                    .and_then(|v| v.to_str().ok())
                    .is_some_and(|v| v.split(',').next().unwrap_or("").trim().eq_ignore_ascii_case("https"))
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

    pub fn session_hmac_key(&self, secrets: Option<&SecretBox>, mgmt_key: Option<&str>) -> [u8; 32] {
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
    let db = state.db.lock().expect("database lock poisoned");
    let epoch = db.current_session_epoch().unwrap_or(1);
    db.create_blueprint_session(
        &auth::token_hash(&token),
        timestamp + auth::SESSION_IDLE_SECONDS,
        timestamp + auth::SESSION_MAX_SECONDS,
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
        auth::SESSION_MAX_SECONDS,
        secure,
    ))
}

/// Validates session cookie signature, idle/max TTL, and `session_epoch`.
/// Returns `(auth_method, username)` when valid.
pub fn validate_session_identity(state: &SharedState, headers: &HeaderMap) -> Option<(String, String)> {
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
    db.validate_and_touch_session(&auth::token_hash(token), now(), auth::SESSION_IDLE_SECONDS)
        .ok()
        .flatten()
}

pub fn validate_api_key_request(state: &SharedState, headers: &HeaderMap) -> bool {
    if auth::has_api_key(headers, &state.cfg.api_keys) {
        return true;
    }
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
    let Some((id, stored_hash)) = row else {
        return false;
    };
    if !bool::from(stored_hash.as_bytes().ct_eq(hash_hex.as_bytes())) {
        return false;
    }
    let _ = db.conn.execute(
        "UPDATE api_keys SET last_used_at = ?2 WHERE id = ?1",
        params![id, now()],
    );
    true
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
pub fn enforce_csrf(state: &SharedState, req: &Request<axum::body::Body>) -> Result<(), axum::response::Response> {
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
    if let Some(host_val) = headers.get(header::HOST).and_then(|v| v.to_str().ok()) {
        if let Ok(auth) = host_val.parse::<Authority>() {
            let h = auth.host().trim_matches(['[', ']']).to_ascii_lowercase();
            allowed_origins.push(h.clone());
            if let Some(p) = auth.port_u16() {
                allowed_origins.push(format!("{h}:{p}"));
            }
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
    } else if let Some(referer) = headers.get(header::REFERER).and_then(|v| v.to_str().ok()) {
        if !origin_matches_host(referer, &allowed_origins) {
            return Err(failure(StatusCode::FORBIDDEN, "csrf_origin_mismatch"));
        }
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
