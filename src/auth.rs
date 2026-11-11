use axum::http::{HeaderMap, header};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use hmac::{Hmac, Mac};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use subtle::ConstantTimeEq;

pub const COOKIE_NAME: &str = "lnc_newsman_session";
pub const SESSION_SECONDS: u64 = 12 * 60 * 60;
const LIMIT_WINDOW: u64 = 300;
const MAX_ATTEMPTS: usize = 8;
type HmacSha256 = Hmac<Sha256>;

/// A signed, random token. Only its hash is stored server-side, so logout can revoke it.
pub fn new_token(key: &str) -> Result<String, getrandom::Error> {
    let mut nonce = [0u8; 32];
    getrandom::fill(&mut nonce)?;
    let body = URL_SAFE_NO_PAD.encode(nonce);
    let mut mac = HmacSha256::new_from_slice(key.as_bytes()).expect("HMAC accepts any key size");
    mac.update(body.as_bytes());
    Ok(format!(
        "{}.{}",
        body,
        URL_SAFE_NO_PAD.encode(mac.finalize().into_bytes())
    ))
}

pub fn valid_signature(token: &str, key: &str) -> bool {
    let Some((body, signature)) = token.split_once('.') else {
        return false;
    };
    let Ok(signature) = URL_SAFE_NO_PAD.decode(signature) else {
        return false;
    };
    let Ok(mut mac) = HmacSha256::new_from_slice(key.as_bytes()) else {
        return false;
    };
    mac.update(body.as_bytes());
    mac.verify_slice(&signature).is_ok()
}

pub fn token_hash(token: &str) -> String {
    URL_SAFE_NO_PAD.encode(Sha256::digest(token.as_bytes()))
}

pub fn cookie_value(headers: &HeaderMap) -> Option<&str> {
    let prefix = format!("{}=", COOKIE_NAME);
    headers
        .get(header::COOKIE)?
        .to_str()
        .ok()?
        .split(';')
        .map(str::trim)
        .find_map(|part| part.strip_prefix(&prefix))
}

pub fn valid_key(supplied: &str, expected: &str) -> bool {
    bool::from(supplied.as_bytes().ct_eq(expected.as_bytes()))
}

pub fn has_api_key(headers: &HeaderMap, configured: &[String]) -> bool {
    let bearer = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "));
    let supplied = bearer.or_else(|| headers.get("x-api-key").and_then(|v| v.to_str().ok()));
    supplied.is_some_and(|value| {
        configured
            .iter()
            .any(|candidate| valid_key(value, candidate))
    })
}

/// Per socket peer, deliberately ignoring spoofable X-Forwarded-For headers.
/// The reverse-proxy IP is shared unless a trusted proxy integration is added later.
#[derive(Default)]
pub struct LoginLimiter {
    attempts: HashMap<String, Vec<u64>>,
}
impl LoginLimiter {
    pub fn allow_and_record(&mut self, peer: &str, now: u64) -> bool {
        self.attempts.retain(|_, times| {
            times.retain(|t| now.saturating_sub(*t) < LIMIT_WINDOW);
            !times.is_empty()
        });
        if !self.attempts.contains_key(peer) && self.attempts.len() >= 10_000 {
            return false;
        }
        let times = self.attempts.entry(peer.to_string()).or_default();
        if times.len() >= MAX_ATTEMPTS {
            return false;
        }
        times.push(now);
        true
    }
    pub fn clear(&mut self, peer: &str) {
        self.attempts.remove(peer);
    }
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
}
