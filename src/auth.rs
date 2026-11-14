use argon2::{
    Algorithm, Argon2, Params, Version,
    password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
};
use axum::http::{HeaderMap, header};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use hmac::{Hmac, Mac};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use subtle::ConstantTimeEq;

pub const COOKIE_NAME: &str = "lnc_newsman_session";
pub const OIDC_TX_COOKIE_NAME: &str = "lnc_newsman_oidc_tx";
pub const SESSION_SECONDS: u64 = 12 * 60 * 60;
pub const SESSION_IDLE_SECONDS: u64 = 12 * 60 * 60;
pub const SESSION_MAX_SECONDS: u64 = 7 * 24 * 60 * 60;
const LIMIT_WINDOW: u64 = 300;
const MAX_ATTEMPTS: usize = 8;
pub const COPY_LIMIT_WINDOW: u64 = 300;
pub const COPY_MAX_ATTEMPTS: usize = 15;
pub const OIDC_LIMIT_WINDOW: u64 = 300;
pub const OIDC_MAX_ATTEMPTS: usize = 20;
type HmacSha256 = Hmac<Sha256>;

pub fn new_token_bytes(key: &[u8]) -> Result<String, getrandom::Error> {
    let mut nonce = [0u8; 32];
    getrandom::fill(&mut nonce)?;
    let body = URL_SAFE_NO_PAD.encode(nonce);
    let mut mac = HmacSha256::new_from_slice(key).expect("HMAC accepts any key size");
    mac.update(body.as_bytes());
    Ok(format!(
        "{}.{}",
        body,
        URL_SAFE_NO_PAD.encode(mac.finalize().into_bytes())
    ))
}

/// A signed, random token. Only its hash is stored server-side, so logout can revoke it.
pub fn new_token(key: &str) -> Result<String, getrandom::Error> {
    new_token_bytes(key.as_bytes())
}

pub fn valid_signature_bytes(token: &str, key: &[u8]) -> bool {
    let Some((body, signature)) = token.split_once('.') else {
        return false;
    };
    let Ok(signature) = URL_SAFE_NO_PAD.decode(signature) else {
        return false;
    };
    let Ok(mut mac) = HmacSha256::new_from_slice(key) else {
        return false;
    };
    mac.update(body.as_bytes());
    mac.verify_slice(&signature).is_ok()
}

pub fn valid_signature(token: &str, key: &str) -> bool {
    valid_signature_bytes(token, key.as_bytes())
}

pub fn token_hash(token: &str) -> String {
    URL_SAFE_NO_PAD.encode(Sha256::digest(token.as_bytes()))
}

pub fn cookie_named<'a>(headers: &'a HeaderMap, cookie_name: &str) -> Option<&'a str> {
    let prefix = format!("{cookie_name}=");
    headers
        .get(header::COOKIE)?
        .to_str()
        .ok()?
        .split(';')
        .map(str::trim)
        .find_map(|part| part.strip_prefix(&prefix))
}

pub fn cookie_value(headers: &HeaderMap) -> Option<&str> {
    cookie_named(headers, COOKIE_NAME)
}

pub fn valid_key(supplied: &str, expected: &str) -> bool {
    bool::from(supplied.as_bytes().ct_eq(expected.as_bytes()))
}

pub fn extract_api_key(headers: &HeaderMap) -> Option<&str> {
    let bearer = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "));
    bearer.or_else(|| headers.get("x-api-key").and_then(|v| v.to_str().ok()))
}

pub fn has_api_key(headers: &HeaderMap, configured: &[String]) -> bool {
    extract_api_key(headers).is_some_and(|value| {
        configured
            .iter()
            .any(|candidate| valid_key(value, candidate))
    })
}

pub fn argon2id_instance() -> Argon2<'static> {
    let params = Params::new(65_536, 3, 4, None).expect("Argon2id parameters valid");
    Argon2::new(Algorithm::Argon2id, Version::V0x13, params)
}

pub fn hash_password_argon2id(password: &str) -> anyhow::Result<String> {
    let mut salt_bytes = [0u8; 16];
    getrandom::fill(&mut salt_bytes)
        .map_err(|err| anyhow::anyhow!("getrandom salt failed: {err}"))?;
    let salt = SaltString::encode_b64(&salt_bytes)
        .map_err(|err| anyhow::anyhow!("encode salt failed: {err}"))?;
    argon2id_instance()
        .hash_password(password.as_bytes(), &salt)
        .map(|h| h.to_string())
        .map_err(|err| anyhow::anyhow!("argon2id hash failed: {err}"))
}

pub fn verify_password_argon2id(password: &str, phc_hash: &str) -> bool {
    let Ok(parsed) = PasswordHash::new(phc_hash) else {
        return false;
    };
    argon2id_instance()
        .verify_password(password.as_bytes(), &parsed)
        .is_ok()
}

/// Verify a management key against DB runtime hash first; once rotated (`mgmt_source = 'runtime'`),
/// older env keys are strictly refused.
pub fn verify_management_key(
    db: &crate::db::Database,
    env_key: Option<&str>,
    supplied: &str,
) -> bool {
    if supplied.is_empty() {
        return false;
    }
    if let Ok(rt) = db.runtime_auth_state() {
        if rt.mgmt_source == "runtime" {
            return !rt.mgmt_key_hash.is_empty()
                && verify_password_argon2id(supplied, &rt.mgmt_key_hash);
        }
        if !rt.mgmt_key_hash.is_empty() {
            return verify_password_argon2id(supplied, &rt.mgmt_key_hash);
        }
    }
    env_key.is_some_and(|expected| valid_key(supplied, expected))
}

pub fn management_key_configured(db: &crate::db::Database, env_key: Option<&str>) -> bool {
    if let Ok(rt) = db.runtime_auth_state()
        && !rt.mgmt_key_hash.is_empty()
    {
        return true;
    }
    env_key.is_some_and(|k| !k.trim().is_empty())
}

/// Where a login attempt is counted from: the socket peer, or the client IP a
/// trusted proxy reported. `X-Forwarded-For` is only consulted when the socket
/// peer is in `TRUSTED_PROXIES`, and then only its right-most entry, which is the
/// address the trusted proxy appended itself. Anything further left was supplied
/// by the caller and is not believed.
pub fn client_key(peer: &str, forwarded_for: Option<&str>, trusted: &[String]) -> String {
    if trusted.is_empty() || !trusted.iter().any(|t| t.eq_ignore_ascii_case(peer)) {
        return peer.to_string();
    }
    let Some(chain) = forwarded_for else {
        return peer.to_string();
    };
    match chain.rsplit(',').map(str::trim).find(|v| !v.is_empty()) {
        Some(last) if last.parse::<std::net::IpAddr>().is_ok() => last.to_string(),
        // A present but unparsable chain is a caller trying to influence the key;
        // count it against the proxy itself rather than trusting the text.
        Some(_) | None => peer.to_string(),
    }
}

/// Per client key sliding-window rate limiter shared across login (`8/5m`),
/// secret copy (`15/5m`), and OIDC endpoints (`20/5m`).
pub struct WindowLimiter {
    window_secs: u64,
    max_attempts: usize,
    attempts: HashMap<String, Vec<u64>>,
}

impl Default for WindowLimiter {
    fn default() -> Self {
        Self::new(LIMIT_WINDOW, MAX_ATTEMPTS)
    }
}

impl WindowLimiter {
    pub fn new(window_secs: u64, max_attempts: usize) -> Self {
        Self {
            window_secs,
            max_attempts,
            attempts: HashMap::new(),
        }
    }

    pub fn allow_and_record(&mut self, peer: &str, now: u64) -> bool {
        let window = self.window_secs;
        self.attempts.retain(|_, times| {
            times.retain(|t| now.saturating_sub(*t) < window);
            !times.is_empty()
        });
        if !self.attempts.contains_key(peer) && self.attempts.len() >= 10_000 {
            return false;
        }
        let times = self.attempts.entry(peer.to_string()).or_default();
        if times.len() >= self.max_attempts {
            return false;
        }
        times.push(now);
        true
    }

    pub fn clear(&mut self, peer: &str) {
        self.attempts.remove(peer);
    }
}

pub type LoginLimiter = WindowLimiter;

pub fn copy_limiter() -> WindowLimiter {
    WindowLimiter::new(COPY_LIMIT_WINDOW, COPY_MAX_ATTEMPTS)
}

pub fn oidc_limiter() -> WindowLimiter {
    WindowLimiter::new(OIDC_LIMIT_WINDOW, OIDC_MAX_ATTEMPTS)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn random_tokens_are_signed_and_key_specific() {
        let a = new_token("first-key").unwrap();
        let b = new_token("first-key").unwrap();
        assert_ne!(a, b);
        assert!(valid_signature(&a, "first-key"));
        assert!(!valid_signature(&a, "other-key"));
        assert!(!valid_signature("bad", "first-key"));
    }
    #[test]
    fn limiter_blocks_ninth_attempt_and_resets_after_window() {
        let mut limiter = LoginLimiter::default();
        for _ in 0..8 {
            assert!(limiter.allow_and_record("peer", 100));
        }
        assert!(!limiter.allow_and_record("peer", 101));
        assert!(limiter.allow_and_record("peer", 400));
    }
    fn trusted(list: &[&str]) -> Vec<String> {
        list.iter().map(|v| v.to_string()).collect()
    }
    #[test]
    fn forwarded_for_is_ignored_without_a_trusted_proxy() {
        assert_eq!(
            client_key("10.20.0.50", Some("203.0.113.9"), &[]),
            "10.20.0.50"
        );
        // A proxy we did not name is just a peer: its header does not move the key.
        assert_eq!(
            client_key("10.20.0.50", Some("203.0.113.9"), &trusted(&["10.20.0.3"])),
            "10.20.0.50"
        );
    }
    #[test]
    fn a_trusted_proxy_moves_the_key_to_the_client_it_reported() {
        let t = trusted(&["10.20.0.3"]);
        assert_eq!(
            client_key("10.20.0.3", Some("10.20.1.44"), &t),
            "10.20.1.44"
        );
        // Only the right-most entry is the proxy's own observation; a caller that
        // prepends its own X-Forwarded-For must not be able to choose the key.
        assert_eq!(
            client_key("10.20.0.3", Some("1.2.3.4, 10.20.1.44"), &t),
            "10.20.1.44"
        );
        assert_eq!(
            client_key("10.20.0.3", Some("1.2.3.4, 10.20.0.3"), &t),
            "10.20.0.3"
        );
    }
    #[test]
    fn a_trusted_proxy_with_junk_still_counts_against_itself() {
        let t = trusted(&["10.20.0.3"]);
        assert_eq!(client_key("10.20.0.3", Some("not-an-ip"), &t), "10.20.0.3");
        assert_eq!(client_key("10.20.0.3", Some("  ,  "), &t), "10.20.0.3");
        assert_eq!(client_key("10.20.0.3", None, &t), "10.20.0.3");
    }
    #[test]
    fn separate_clients_behind_one_proxy_do_not_share_a_bucket() {
        let t = trusted(&["10.20.0.3"]);
        let mut limiter = LoginLimiter::default();
        let a = client_key("10.20.0.3", Some("10.20.1.44"), &t);
        let b = client_key("10.20.0.3", Some("10.20.1.45"), &t);
        for _ in 0..MAX_ATTEMPTS {
            assert!(limiter.allow_and_record(&a, 100));
        }
        assert!(!limiter.allow_and_record(&a, 101));
        assert!(limiter.allow_and_record(&b, 101));
    }
}
