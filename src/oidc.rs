use crate::{
    SharedState, admin_lifecycle, alerts, auth, db, failure,
    fetch::{FetchOutcome, Fetcher},
    internal_error, now, secrets, security,
};
use axum::{
    Json,
    body::Bytes,
    extract::{ConnectInfo, Query, State},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{IntoResponse, Redirect, Response},
};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use jsonwebtoken::{Algorithm, DecodingKey, Validation, decode, decode_header, jwk::JwkSet};
use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Deserializer, Serialize};
use sha2::{Digest, Sha256};
use std::{net::SocketAddr, sync::atomic::Ordering};
use subtle::ConstantTimeEq;
use url::Url;

const TX_TTL_SECONDS: u64 = 300;
const DISCOVERY_TTL_SECONDS: u64 = 300;
pub const DEFAULT_SCOPES: &str = "openid profile email";
const OIDC_SECRET_AAD: &str = "oidc:client_secret";
const MASKED_SECRET: &str = "********";

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct OidcConfig {
    pub enabled: bool,
    pub issuer_url: String,
    pub client_id: String,
    #[serde(skip)]
    pub client_secret: String,
    pub redirect_uri: String,
    #[serde(default = "default_scopes")]
    pub scopes: String,
    #[serde(default)]
    pub allowed_users: Vec<String>,
    #[serde(default)]
    pub allowed_groups: Vec<String>,
    #[serde(default)]
    pub allowed_hosts: Vec<String>,
    #[serde(default = "default_source")]
    pub source: String,
}

fn default_scopes() -> String {
    DEFAULT_SCOPES.to_string()
}

fn default_source() -> String {
    "none".to_string()
}

fn parse_csv_lower(raw: &str) -> Vec<String> {
    let mut out = Vec::new();
    for item in raw.split(',') {
        let clean = item.trim().to_ascii_lowercase();
        if !clean.is_empty() && !out.contains(&clean) {
            out.push(clean);
        }
    }
    out
}

impl OidcConfig {
    pub fn from_env() -> Self {
        let env_trim = |k: &str| {
            std::env::var(k)
                .ok()
                .map(|v| v.trim().to_string())
                .filter(|v| !v.is_empty())
        };
        let issuer_url = env_trim("OIDC_ISSUER_URL")
            .map(|v| v.trim_end_matches('/').to_string())
            .unwrap_or_default();
        let client_id = env_trim("OIDC_CLIENT_ID").unwrap_or_default();
        let client_secret = env_trim("OIDC_CLIENT_SECRET").unwrap_or_default();
        let redirect_uri = env_trim("OIDC_REDIRECT_URI").unwrap_or_default();
        let scopes = env_trim("OIDC_SCOPES").unwrap_or_else(default_scopes);
        let allowed_users =
            parse_csv_lower(&std::env::var("OIDC_ALLOWED_USERS").unwrap_or_default());
        let allowed_groups =
            parse_csv_lower(&std::env::var("OIDC_ALLOWED_GROUPS").unwrap_or_default());
        let allowed_hosts =
            parse_csv_lower(&std::env::var("OIDC_ALLOWED_HOSTS").unwrap_or_default());
        let explicit = env_trim("OIDC_ENABLED").map(|v| v.to_ascii_lowercase());
        let has_fields = !issuer_url.is_empty()
            && !client_id.is_empty()
            && !client_secret.is_empty()
            && (!allowed_users.is_empty() || !allowed_groups.is_empty());
        let enabled = match explicit.as_deref() {
            Some("true" | "1" | "yes") => has_fields,
            Some("false" | "0" | "no") => false,
            _ => has_fields,
        };
        let source = if issuer_url.is_empty() && client_id.is_empty() && client_secret.is_empty() {
            "none".to_string()
        } else {
            "env".to_string()
        };
        Self {
            enabled,
            issuer_url,
            client_id,
            client_secret,
            redirect_uri,
            scopes,
            allowed_users,
            allowed_groups,
            allowed_hosts,
            source,
        }
    }

    pub fn env_seed_digest(&self) -> String {
        let mut h = Sha256::new();
        h.update(if self.enabled { b"1|" } else { b"0|" });
        h.update(self.issuer_url.as_bytes());
        h.update(b"|");
        h.update(self.client_id.as_bytes());
        h.update(b"|");
        h.update(self.client_secret.as_bytes());
        h.update(b"|");
        h.update(self.redirect_uri.as_bytes());
        h.update(b"|");
        h.update(self.scopes.as_bytes());
        h.update(b"|");
        h.update(self.allowed_users.join(",").as_bytes());
        h.update(b"|");
        h.update(self.allowed_groups.join(",").as_bytes());
        h.update(b"|");
        h.update(self.allowed_hosts.join(",").as_bytes());
        h.finalize().iter().map(|b| format!("{b:02x}")).collect()
    }

    pub fn load_or_seed(db: &db::Database, secrets: Option<&secrets::SecretBox>) -> Self {
        let env_cfg = Self::from_env();
        let env_digest = env_cfg.env_seed_digest();
        let stored_json: Option<String> = db
            .conn
            .query_row(
                "SELECT value FROM settings WHERE key = 'oidc_config_json'",
                [],
                |r| r.get(0),
            )
            .optional()
            .ok()
            .flatten();
        let stored_enc: Option<String> = db
            .conn
            .query_row(
                "SELECT value FROM settings WHERE key = 'oidc_client_secret_enc'",
                [],
                |r| r.get(0),
            )
            .optional()
            .ok()
            .flatten();
        let prev_digest: Option<String> = db
            .conn
            .query_row(
                "SELECT value FROM settings WHERE key = 'seed_digest:oidc'",
                [],
                |r| r.get(0),
            )
            .optional()
            .ok()
            .flatten();

        if let Some(ref raw_json) = stored_json
            && let Ok(mut parsed) = serde_json::from_str::<OidcConfig>(raw_json)
        {
            let env_empty = env_cfg.issuer_url.is_empty()
                && env_cfg.client_id.is_empty()
                && env_cfg.client_secret.is_empty();
            if parsed.source == "runtime"
                || prev_digest.as_deref() == Some(env_digest.as_str())
                || env_empty
            {
                if let Some(ref enc) = stored_enc
                    && !enc.is_empty()
                    && let Some(vault) = secrets
                    && let Ok(plain) = vault.open_envelope(OIDC_SECRET_AAD, enc)
                {
                    parsed.client_secret = plain;
                } else if parsed.source == "env" && stored_enc.as_deref().unwrap_or("").is_empty() {
                    parsed.client_secret = env_cfg.client_secret.clone();
                }
                if parsed.scopes.trim().is_empty() {
                    parsed.scopes = default_scopes();
                }
                return parsed;
            }
        }

        // Seed from env once per digest
        if env_cfg.source == "env" {
            let _ = persist_oidc_to_db(db, secrets, &env_cfg, Some(&env_digest));
        }
        env_cfg
    }

    pub fn has_wildcard(&self) -> bool {
        self.redirect_uri.contains('*')
            || self.allowed_users.iter().any(|u| u == "*" || u.contains('*'))
            || self
                .allowed_groups
                .iter()
                .any(|g| g == "*" || g.contains('*'))
            || self.allowed_hosts.iter().any(|h| h == "*")
    }

    pub fn is_active(&self) -> bool {
        self.enabled
            && !self.issuer_url.is_empty()
            && !self.client_id.is_empty()
            && !self.client_secret.is_empty()
            && (!self.allowed_users.is_empty() || !self.allowed_groups.is_empty())
            && !self.has_wildcard()
    }

    pub fn effective_scopes(&self) -> &str {
        let trimmed = self.scopes.trim();
        if trimmed.is_empty() {
            DEFAULT_SCOPES
        } else {
            trimmed
        }
    }

    pub fn user_allowed(&self, candidates: &[Option<&str>]) -> Option<String> {
        if self.allowed_users.is_empty() || self.has_wildcard() {
            // Fail closed when OIDC_ALLOWED_USERS is empty or contains wildcard.
            return None;
        }
        for candidate in candidates.iter().flatten() {
            let clean = candidate.trim();
            if clean.is_empty() {
                continue;
            }
            let lower = clean.to_ascii_lowercase();
            if self.allowed_users.iter().any(|u| u == &lower) {
                return Some(clean.to_string());
            }
        }
        None
    }

    pub fn authorize_claims(&self, claims: &IdClaims) -> Option<String> {
        if self.has_wildcard() || (self.allowed_users.is_empty() && self.allowed_groups.is_empty()) {
            return None;
        }
        let candidates = [
            claims.preferred_username.as_deref(),
            claims.email.as_deref(),
            claims.sub.as_deref(),
        ];
        if let Some(matched) = self.user_allowed(&candidates) {
            return Some(matched);
        }
        if !self.allowed_groups.is_empty() {
            let claim_groups = claims.collect_groups();
            let group_ok = claim_groups.iter().any(|cg| {
                let low = cg.trim().trim_start_matches('/').to_ascii_lowercase();
                !low.is_empty()
                    && self
                        .allowed_groups
                        .iter()
                        .any(|ag| ag.trim_start_matches('/') == low)
            });
            if group_ok {
                return candidates
                    .into_iter()
                    .flatten()
                    .map(str::trim)
                    .find(|s| !s.is_empty())
                    .map(str::to_string);
            }
        }
        None
    }
}

pub fn persist_oidc_to_db(
    db: &db::Database,
    secrets: Option<&secrets::SecretBox>,
    cfg: &OidcConfig,
    seed_digest: Option<&str>,
) -> Result<(), secrets::SecretError> {
    let json_str = serde_json::to_string(cfg).map_err(|_| secrets::SecretError::Invalid)?;
    db.conn
        .execute(
            "INSERT INTO settings(key, value) VALUES ('oidc_config_json', ?1)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![json_str],
        )
        .map_err(|_| secrets::SecretError::Invalid)?;
    if cfg.client_secret.is_empty() {
        let _ = db.conn.execute(
            "INSERT INTO settings(key, value) VALUES ('oidc_client_secret_enc', '')
             ON CONFLICT(key) DO UPDATE SET value = ''",
            [],
        );
    } else if let Some(vault) = secrets {
        let enc = vault.seal_envelope(OIDC_SECRET_AAD, &cfg.client_secret)?;
        db.conn
            .execute(
                "INSERT INTO settings(key, value) VALUES ('oidc_client_secret_enc', ?1)
                 ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                params![enc],
            )
            .map_err(|_| secrets::SecretError::Invalid)?;
    }
    if let Some(digest) = seed_digest {
        let _ = db.conn.execute(
            "INSERT INTO settings(key, value) VALUES ('seed_digest:oidc', ?1)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![digest],
        );
    }
    Ok(())
}

#[derive(Debug, Clone, Deserialize)]
pub struct DiscoveryDoc {
    pub issuer: String,
    pub authorization_endpoint: String,
    pub token_endpoint: String,
    pub jwks_uri: String,
}

#[derive(Default)]
pub struct OidcCache {
    pub discovery: Option<(u64, DiscoveryDoc)>,
    pub jwks: Option<(u64, JwkSet)>,
}

#[derive(Serialize, Deserialize)]
struct OidcTx {
    state: String,
    nonce: String,
    code_verifier: String,
    redirect_uri: String,
    created_at: u64,
}

#[derive(Deserialize)]
pub struct CallbackQuery {
    code: Option<String>,
    state: Option<String>,
    error: Option<String>,
}

#[derive(Deserialize)]
struct TokenResponse {
    id_token: Option<String>,
}

#[derive(Default, Deserialize)]
pub struct RealmAccess {
    #[serde(default)]
    pub roles: Vec<String>,
}

#[derive(Deserialize)]
pub struct IdClaims {
    #[serde(default)]
    nonce: Option<String>,
    #[serde(default)]
    preferred_username: Option<String>,
    #[serde(default)]
    email: Option<String>,
    #[serde(default)]
    sub: Option<String>,
    #[serde(default)]
    groups: Vec<String>,
    #[serde(default)]
    roles: Vec<String>,
    #[serde(default)]
    realm_access: Option<RealmAccess>,
}

impl IdClaims {
    pub fn collect_groups(&self) -> Vec<String> {
        let mut out = Vec::new();
        for g in self.groups.iter().chain(self.roles.iter()) {
            out.push(g.clone());
        }
        if let Some(ref ra) = self.realm_access {
            for r in &ra.roles {
                out.push(r.clone());
            }
        }
        out
    }
}

fn oidc_fetcher_with_hosts(state: &SharedState, extra_hosts: &[String]) -> Fetcher {
    let mut hosts = state.security.oidc_cfg().allowed_hosts;
    for h in extra_hosts
        .iter()
        .chain(state.cfg.allowed_hosts.iter())
        .chain(state.agent_bridge.config().allowed_hosts.iter())
    {
        let clean = h.trim().to_ascii_lowercase();
        if !clean.is_empty() && clean != "*" && !hosts.contains(&clean) {
            hosts.push(clean);
        }
    }
    if hosts.is_empty() {
        return state
            .fetcher
            .clone()
            .with_timeout(std::time::Duration::from_secs(10));
    }
    hosts.sort();
    hosts.dedup();
    state
        .fetcher
        .clone()
        .with_private_hosts(hosts)
        .with_timeout(std::time::Duration::from_secs(10))
}

fn effective_redirect_uri(
    state: &SharedState,
    cfg: &OidcConfig,
    headers: &HeaderMap,
) -> Result<String, &'static str> {
    if !cfg.redirect_uri.is_empty() {
        return Ok(cfg.redirect_uri.clone());
    }
    if let Some(ref pub_url) = state.security.public_base_url {
        return Ok(format!(
            "{}/api/auth/oidc/callback",
            pub_url.trim_end_matches('/')
        ));
    }
    let host = headers
        .get(header::HOST)
        .and_then(|v| v.to_str().ok())
        .ok_or("oidc_redirect_unavailable")?;
    let scheme = if state.security.is_cookie_secure(headers) {
        "https"
    } else {
        "http"
    };
    Ok(format!("{scheme}://{host}/api/auth/oidc/callback"))
}

fn random_b64url(bytes: usize) -> Result<String, getrandom::Error> {
    let mut buf = vec![0u8; bytes];
    getrandom::fill(&mut buf)?;
    Ok(URL_SAFE_NO_PAD.encode(buf))
}

fn pkce_challenge_s256(verifier: &str) -> String {
    URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()))
}

async fn load_discovery(state: &SharedState, cfg: &OidcConfig) -> Result<DiscoveryDoc, &'static str> {
    let now_ts = now();
    {
        let guard = state
            .security
            .oidc_cache
            .lock()
            .expect("oidc cache lock poisoned");
        if let Some((fetched_at, ref doc)) = guard.discovery
            && now_ts.saturating_sub(fetched_at) < DISCOVERY_TTL_SECONDS
        {
            return Ok(doc.clone());
        }
    }
    let url = format!(
        "{}/.well-known/openid-configuration",
        cfg.issuer_url.trim_end_matches('/')
    );
    let outcome = oidc_fetcher_with_hosts(state, &cfg.allowed_hosts)
        .get_with_headers(&url, &[])
        .await
        .map_err(|_| "oidc_discovery_failed")?;
    let bytes = match outcome {
        FetchOutcome::Body { bytes, .. } => bytes,
        FetchOutcome::NotModified => return Err("oidc_discovery_failed"),
    };
    let doc: DiscoveryDoc =
        serde_json::from_slice(&bytes).map_err(|_| "oidc_discovery_invalid")?;
    let mut guard = state
        .security
        .oidc_cache
        .lock()
        .expect("oidc cache lock poisoned");
    guard.discovery = Some((now_ts, doc.clone()));
    Ok(doc)
}

async fn load_jwks(
    state: &SharedState,
    cfg: &OidcConfig,
    jwks_uri: &str,
    force: bool,
) -> Result<JwkSet, &'static str> {
    let now_ts = now();
    if !force {
        let guard = state
            .security
            .oidc_cache
            .lock()
            .expect("oidc cache lock poisoned");
        if let Some((fetched_at, ref set)) = guard.jwks
            && now_ts.saturating_sub(fetched_at) < DISCOVERY_TTL_SECONDS
        {
            return Ok(set.clone());
        }
    }
    let outcome = oidc_fetcher_with_hosts(state, &cfg.allowed_hosts)
        .get_with_headers(jwks_uri, &[])
        .await
        .map_err(|_| "oidc_jwks_failed")?;
    let bytes = match outcome {
        FetchOutcome::Body { bytes, .. } => bytes,
        FetchOutcome::NotModified => return Err("oidc_jwks_failed"),
    };
    let set: JwkSet = serde_json::from_slice(&bytes).map_err(|_| "oidc_jwks_invalid")?;
    let mut guard = state
        .security
        .oidc_cache
        .lock()
        .expect("oidc cache lock poisoned");
    guard.jwks = Some((now_ts, set.clone()));
    Ok(set)
}

pub fn verify_id_token_claims(
    id_token: &str,
    jwks: &JwkSet,
    expected_issuers: &[&str],
    client_id: &str,
    expected_nonce: &str,
) -> Result<IdClaims, &'static str> {
    let header = decode_header(id_token).map_err(|_| "oidc_invalid_token_header")?;
    let allowed_alg = matches!(
        header.alg,
        Algorithm::RS256
            | Algorithm::RS384
            | Algorithm::RS512
            | Algorithm::ES256
            | Algorithm::ES384
            | Algorithm::EdDSA
    );
    if !allowed_alg {
        return Err("oidc_disallowed_alg");
    }
    let jwk = match header.kid.as_deref() {
        Some(kid) => jwks.find(kid).ok_or("oidc_unknown_kid")?,
        None if jwks.keys.len() == 1 => &jwks.keys[0],
        None => return Err("oidc_missing_kid"),
    };
    let decoding_key = DecodingKey::from_jwk(jwk).map_err(|_| "oidc_invalid_jwk")?;
    let mut validation = Validation::new(header.alg);
    validation.set_issuer(expected_issuers);
    validation.set_audience(&[client_id]);
    validation.validate_exp = true;
    validation.leeway = 60;
    let decoded = decode::<IdClaims>(id_token, &decoding_key, &validation)
        .map_err(|_| "oidc_token_verification_failed")?;
    let claims = decoded.claims;
    let token_nonce = claims.nonce.as_deref().unwrap_or("");
    if token_nonce.is_empty()
        || !bool::from(token_nonce.as_bytes().ct_eq(expected_nonce.as_bytes()))
    {
        return Err("oidc_nonce_mismatch");
    }
    Ok(claims)
}

#[allow(dead_code)]
pub fn verify_id_token_with_jwks(
    id_token: &str,
    jwks: &JwkSet,
    expected_issuers: &[&str],
    client_id: &str,
    expected_nonce: &str,
) -> Result<String, &'static str> {
    let claims =
        verify_id_token_claims(id_token, jwks, expected_issuers, client_id, expected_nonce)?;
    Ok(claims
        .preferred_username
        .or(claims.email)
        .or(claims.sub)
        .unwrap_or_default())
}

pub async fn oidc_login(
    State(state): State<SharedState>,
    peer: Option<axum::Extension<ConnectInfo<SocketAddr>>>,
    headers: HeaderMap,
) -> Response {
    let cfg = state.security.oidc_cfg();
    if !cfg.is_active() {
        return failure(StatusCode::SERVICE_UNAVAILABLE, "oidc_not_configured");
    }
    let client_ip = security::request_client_ip(&state, peer.as_ref(), &headers);
    if !state
        .security
        .oidc_limiter
        .lock()
        .expect("oidc limiter poisoned")
        .allow_and_record(&client_ip, now())
    {
        return failure(StatusCode::TOO_MANY_REQUESTS, "oidc_rate_limited");
    }
    let doc = match load_discovery(&state, &cfg).await {
        Ok(d) => d,
        Err(code) => return failure(StatusCode::BAD_GATEWAY, code),
    };
    let redirect_uri = match effective_redirect_uri(&state, &cfg, &headers) {
        Ok(u) => u,
        Err(code) => return failure(StatusCode::BAD_REQUEST, code),
    };
    let (Ok(state_val), Ok(nonce_val), Ok(code_verifier)) =
        (random_b64url(24), random_b64url(24), random_b64url(48))
    else {
        return internal_error("getrandom failed");
    };
    let challenge = pkce_challenge_s256(&code_verifier);
    let tx = OidcTx {
        state: state_val.clone(),
        nonce: nonce_val.clone(),
        code_verifier,
        redirect_uri: redirect_uri.clone(),
        created_at: now(),
    };
    let tx_json = match serde_json::to_vec(&tx) {
        Ok(b) => b,
        Err(err) => return internal_error(err),
    };
    let tx_key = state.security.oidc_tx_key(state.secrets.as_ref());
    let sealed_tx = match secrets::seal_with_key(&tx_key, b"oidc_tx:v1", &tx_json) {
        Ok(s) => s,
        Err(err) => return internal_error(err),
    };

    let Ok(mut auth_url) = Url::parse(&doc.authorization_endpoint) else {
        return failure(StatusCode::BAD_GATEWAY, "oidc_invalid_auth_endpoint");
    };
    auth_url
        .query_pairs_mut()
        .append_pair("response_type", "code")
        .append_pair("client_id", &cfg.client_id)
        .append_pair("redirect_uri", &redirect_uri)
        .append_pair("scope", cfg.effective_scopes())
        .append_pair("state", &state_val)
        .append_pair("nonce", &nonce_val)
        .append_pair("code_challenge", &challenge)
        .append_pair("code_challenge_method", "S256");

    let secure = state.security.is_cookie_secure(&headers);
    let cookie = security::build_cookie_header(
        auth::OIDC_TX_COOKIE_NAME,
        &sealed_tx,
        "/api/auth/oidc",
        TX_TTL_SECONDS,
        secure,
    );
    let mut response = Redirect::to(auth_url.as_str()).into_response();
    response.headers_mut().insert(header::SET_COOKIE, cookie);
    response
}

fn redirect_login_error(code: &str, secure: bool) -> Response {
    let clear = security::clear_cookie_header(auth::OIDC_TX_COOKIE_NAME, "/api/auth/oidc", secure);
    let mut response = Redirect::to(&format!("/login.html?oidc_error={code}")).into_response();
    response.headers_mut().insert(header::SET_COOKIE, clear);
    response
}

pub async fn oidc_callback(
    State(state): State<SharedState>,
    peer: Option<axum::Extension<ConnectInfo<SocketAddr>>>,
    headers: HeaderMap,
    Query(query): Query<CallbackQuery>,
) -> Response {
    let secure = state.security.is_cookie_secure(&headers);
    let cfg = state.security.oidc_cfg();
    if !cfg.is_active() {
        return redirect_login_error("oidc_not_configured", secure);
    }
    let client_ip = security::request_client_ip(&state, peer.as_ref(), &headers);
    if !state
        .security
        .oidc_limiter
        .lock()
        .expect("oidc limiter poisoned")
        .allow_and_record(&client_ip, now())
    {
        return failure(StatusCode::TOO_MANY_REQUESTS, "oidc_rate_limited");
    }
    if query.error.is_some() {
        return redirect_login_error("oidc_denied", secure);
    }
    let (Some(code), Some(incoming_state), Some(tx_cookie)) = (
        query.code.as_deref().filter(|s| !s.is_empty()),
        query.state.as_deref().filter(|s| !s.is_empty()),
        auth::cookie_named(&headers, auth::OIDC_TX_COOKIE_NAME),
    ) else {
        return redirect_login_error("oidc_invalid_callback", secure);
    };
    let tx_key = state.security.oidc_tx_key(state.secrets.as_ref());
    let Ok(tx_bytes) = secrets::open_with_key(&tx_key, b"oidc_tx:v1", tx_cookie) else {
        return redirect_login_error("oidc_tx_invalid", secure);
    };
    let Ok(tx) = serde_json::from_slice::<OidcTx>(&tx_bytes) else {
        return redirect_login_error("oidc_tx_invalid", secure);
    };
    if now().saturating_sub(tx.created_at) > TX_TTL_SECONDS
        || !bool::from(tx.state.as_bytes().ct_eq(incoming_state.as_bytes()))
    {
        return redirect_login_error("oidc_state_mismatch", secure);
    }

    let Ok(doc) = load_discovery(&state, &cfg).await else {
        return redirect_login_error("oidc_discovery_failed", secure);
    };
    let form = url::form_urlencoded::Serializer::new(String::new())
        .append_pair("grant_type", "authorization_code")
        .append_pair("code", code)
        .append_pair("redirect_uri", &tx.redirect_uri)
        .append_pair("client_id", &cfg.client_id)
        .append_pair("client_secret", &cfg.client_secret)
        .append_pair("code_verifier", &tx.code_verifier)
        .finish();
    let outcome = match oidc_fetcher_with_hosts(&state, &cfg.allowed_hosts)
        .post_form(&doc.token_endpoint, &form, &[])
        .await
    {
        Ok(FetchOutcome::Body { bytes, .. }) => bytes,
        _ => return redirect_login_error("oidc_token_exchange_failed", secure),
    };
    let Ok(tok) = serde_json::from_slice::<TokenResponse>(&outcome) else {
        return redirect_login_error("oidc_token_invalid", secure);
    };
    let Some(id_token) = tok.id_token.as_deref().filter(|s| !s.is_empty()) else {
        return redirect_login_error("oidc_missing_id_token", secure);
    };

    let Ok(jwks) = load_jwks(&state, &cfg, &doc.jwks_uri, false).await else {
        return redirect_login_error("oidc_jwks_failed", secure);
    };
    let configured_iss = cfg.issuer_url.trim_end_matches('/');
    let doc_iss = doc.issuer.trim_end_matches('/');
    let issuers = [configured_iss, doc_iss];
    let claims = match verify_id_token_claims(
        id_token,
        &jwks,
        &issuers,
        &cfg.client_id,
        &tx.nonce,
    ) {
        Ok(c) => c,
        Err("oidc_unknown_kid") => match load_jwks(&state, &cfg, &doc.jwks_uri, true).await {
            Ok(fresh_jwks) => match verify_id_token_claims(
                id_token,
                &fresh_jwks,
                &issuers,
                &cfg.client_id,
                &tx.nonce,
            ) {
                Ok(c) => c,
                Err(code) => return redirect_login_error(code, secure),
            },
            Err(code) => return redirect_login_error(code, secure),
        },
        Err(code) => return redirect_login_error(code, secure),
    };

    let Some(username) = cfg.authorize_claims(&claims) else {
        let denied_actor = claims
            .preferred_username
            .as_deref()
            .or(claims.email.as_deref())
            .or(claims.sub.as_deref())
            .unwrap_or("unknown");
        let db = state.db.lock().expect("database lock poisoned");
        let _ = db.record_audit_event(
            now(),
            denied_actor,
            "oidc",
            &client_ip,
            "auth.oidc_denied",
            "oidc",
            &cfg.client_id,
            &serde_json::json!({ "reason": "user_not_allowed" }),
        );
        return redirect_login_error("oidc_user_not_allowed", secure);
    };

    let sess_cookie =
        match security::issue_session_cookie(&state, &headers, "oidc", &username, &client_ip) {
            Ok(c) => c,
            Err(err) => return internal_error(err),
        };
    if !state.alerts_running.swap(true, Ordering::SeqCst) {
        alerts::spawn_sweeper(state.clone());
    }
    let clear_tx = security::clear_cookie_header(auth::OIDC_TX_COOKIE_NAME, "/api/auth/oidc", secure);
    let mut response = Redirect::to("/admin").into_response();
    response.headers_mut().append(header::SET_COOKIE, clear_tx);
    response.headers_mut().append(header::SET_COOKIE, sess_cookie);
    response
}

// ===========================================================================
// Editable cfg_or_env Keycloak OIDC Admin Endpoints (Blueprint v3.0)
// ===========================================================================

fn no_store_json<T: Serialize>(status: StatusCode, payload: T) -> Response {
    let mut res = (status, Json(payload)).into_response();
    res.headers_mut().insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("no-store, private"),
    );
    res
}

fn oidc_view_json(state: &SharedState, cfg: &OidcConfig, headers: &HeaderMap) -> serde_json::Value {
    let default_redir = effective_redirect_uri(state, cfg, headers).unwrap_or_default();
    let has_secret = !cfg.client_secret.is_empty();
    serde_json::json!({
        "ok": true,
        "enabled": cfg.enabled,
        "active": cfg.is_active(),
        "source": if cfg.source.is_empty() { "none" } else { cfg.source.as_str() },
        "env_managed": false,
        "issuer_url": cfg.issuer_url,
        "client_id": cfg.client_id,
        "has_client_secret": has_secret,
        "client_secret": if has_secret { MASKED_SECRET } else { "" },
        "client_secret_masked": if has_secret { MASKED_SECRET } else { "" },
        "redirect_uri": cfg.redirect_uri,
        "default_redirect_uri": default_redir,
        "scopes": cfg.effective_scopes(),
        "allowed_users": cfg.allowed_users,
        "allowed_groups": cfg.allowed_groups,
        "allowed_hosts": cfg.allowed_hosts,
    })
}

fn deserialize_flexible_list<'de, D>(deserializer: D) -> Result<Option<Vec<String>>, D::Error>
where
    D: Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum StringOrVec {
        Vec(Vec<String>),
        Str(String),
    }
    match Option::<StringOrVec>::deserialize(deserializer)? {
        None => Ok(None),
        Some(StringOrVec::Vec(items)) => Ok(Some(
            items
                .into_iter()
                .flat_map(|s| {
                    s.split(',')
                        .map(|p| p.trim().to_ascii_lowercase())
                        .filter(|p| !p.is_empty())
                        .collect::<Vec<_>>()
                })
                .collect(),
        )),
        Some(StringOrVec::Str(raw)) => Ok(Some(parse_csv_lower(&raw))),
    }
}

fn is_masked_or_blank(val: Option<&str>) -> bool {
    match val {
        None => true,
        Some(s) => {
            let t = s.trim();
            t.is_empty()
                || t == MASKED_SECRET
                || (t.chars().count() >= 4 && t.chars().all(|c| c == '*' || c == '•'))
        }
    }
}

#[derive(Deserialize, Default)]
pub struct OidcUpdateInput {
    pub enabled: Option<bool>,
    pub issuer_url: Option<String>,
    pub client_id: Option<String>,
    pub client_secret: Option<String>,
    pub redirect_uri: Option<String>,
    pub scopes: Option<String>,
    #[serde(default, deserialize_with = "deserialize_flexible_list")]
    pub allowed_users: Option<Vec<String>>,
    #[serde(default, deserialize_with = "deserialize_flexible_list")]
    pub allowed_groups: Option<Vec<String>>,
    #[serde(default, deserialize_with = "deserialize_flexible_list")]
    pub allowed_hosts: Option<Vec<String>>,
}

pub async fn admin_get_oidc(State(state): State<SharedState>, headers: HeaderMap) -> Response {
    let cfg = state.security.oidc_cfg();
    no_store_json(StatusCode::OK, oidc_view_json(&state, &cfg, &headers))
}

pub async fn admin_put_oidc(
    State(state): State<SharedState>,
    peer: Option<axum::Extension<ConnectInfo<SocketAddr>>>,
    headers: HeaderMap,
    Json(input): Json<OidcUpdateInput>,
) -> Response {
    let current = state.security.oidc_cfg();
    let enabled = input.enabled.unwrap_or(current.enabled);
    let issuer_raw = input
        .issuer_url
        .as_deref()
        .unwrap_or(&current.issuer_url)
        .trim()
        .trim_end_matches('/')
        .to_string();
    let client_id = input
        .client_id
        .as_deref()
        .unwrap_or(&current.client_id)
        .trim()
        .to_string();
    let redirect_uri = input
        .redirect_uri
        .as_deref()
        .unwrap_or(&current.redirect_uri)
        .trim()
        .to_string();
    let scopes = input
        .scopes
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or(current.effective_scopes())
        .to_string();
    let allowed_users = input.allowed_users.unwrap_or(current.allowed_users.clone());
    let allowed_groups = input
        .allowed_groups
        .unwrap_or(current.allowed_groups.clone());
    let allowed_hosts = input.allowed_hosts.unwrap_or(current.allowed_hosts.clone());

    if redirect_uri.contains('*')
        || allowed_users.iter().any(|u| u == "*" || u.contains('*'))
        || allowed_groups.iter().any(|g| g == "*" || g.contains('*'))
        || allowed_hosts.iter().any(|h| h == "*")
    {
        return failure(StatusCode::BAD_REQUEST, "oidc_wildcard_not_allowed");
    }

    let issuer_url = if !issuer_raw.is_empty() {
        match admin_lifecycle::validate_outbound_url_with_hosts(&state, &issuer_raw, &allowed_hosts)
        {
            Ok(clean) => clean,
            Err(resp) => return resp,
        }
    } else {
        String::new()
    };

    if !redirect_uri.is_empty() {
        let Ok(parsed_redir) = Url::parse(&redirect_uri) else {
            return failure(StatusCode::BAD_REQUEST, "invalid_redirect_uri");
        };
        if !matches!(parsed_redir.scheme(), "http" | "https") || parsed_redir.host_str().is_none() {
            return failure(StatusCode::BAD_REQUEST, "invalid_redirect_uri");
        }
    }

    let client_secret = if is_masked_or_blank(input.client_secret.as_deref()) {
        current.client_secret.clone()
    } else {
        if state.secrets.is_none() {
            return failure(
                StatusCode::SERVICE_UNAVAILABLE,
                "secret_store_not_configured",
            );
        }
        input
            .client_secret
            .as_deref()
            .unwrap_or("")
            .trim()
            .to_string()
    };

    if enabled
        && (issuer_url.is_empty()
            || client_id.is_empty()
            || client_secret.is_empty()
            || (allowed_users.is_empty() && allowed_groups.is_empty()))
    {
        return failure(
            StatusCode::BAD_REQUEST,
            "oidc_incomplete_or_empty_allowlist",
        );
    }

    let next_cfg = OidcConfig {
        enabled,
        issuer_url,
        client_id,
        client_secret,
        redirect_uri,
        scopes,
        allowed_users,
        allowed_groups,
        allowed_hosts,
        source: "runtime".to_string(),
    };

    let (auth_method, actor) = security::validate_session_identity(&state, &headers)
        .unwrap_or_else(|| ("session".into(), "owner".into()));
    let client_ip = security::request_client_ip(&state, peer.as_ref(), &headers);

    {
        let db = state.db.lock().expect("database lock poisoned");
        if let Err(err) = persist_oidc_to_db(&db, state.secrets.as_ref(), &next_cfg, None) {
            return failure(StatusCode::BAD_REQUEST, err.code());
        }
        let _ = db.record_audit_event(
            now(),
            &actor,
            &auth_method,
            &client_ip,
            "oidc.updated",
            "oidc",
            &next_cfg.client_id,
            &serde_json::json!({
                "enabled": next_cfg.enabled,
                "issuer_url": next_cfg.issuer_url,
                "allowed_users_count": next_cfg.allowed_users.len(),
                "allowed_groups_count": next_cfg.allowed_groups.len(),
            }),
        );
    }

    state.security.set_oidc_cfg(next_cfg.clone());
    no_store_json(StatusCode::OK, oidc_view_json(&state, &next_cfg, &headers))
}

#[derive(Deserialize, Default)]
struct OidcTestInput {
    issuer_url: Option<String>,
    #[serde(default, deserialize_with = "deserialize_flexible_list")]
    allowed_hosts: Option<Vec<String>>,
}

pub async fn admin_test_oidc(
    State(state): State<SharedState>,
    _headers: HeaderMap,
    body: Bytes,
) -> Response {
    let input: OidcTestInput = if body.is_empty() {
        OidcTestInput::default()
    } else {
        serde_json::from_slice(&body).unwrap_or_default()
    };
    let current = state.security.oidc_cfg();
    let issuer_raw = input
        .issuer_url
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or(&current.issuer_url)
        .trim_end_matches('/')
        .to_string();
    if issuer_raw.is_empty() {
        return failure(StatusCode::BAD_REQUEST, "oidc_issuer_required");
    }
    let allowed_hosts = input.allowed_hosts.unwrap_or(current.allowed_hosts);
    if allowed_hosts.iter().any(|h| h == "*") {
        return failure(StatusCode::BAD_REQUEST, "oidc_wildcard_not_allowed");
    }
    let clean_issuer =
        match admin_lifecycle::validate_outbound_url_with_hosts(&state, &issuer_raw, &allowed_hosts)
        {
            Ok(u) => u,
            Err(resp) => return resp,
        };

    let fetcher = oidc_fetcher_with_hosts(&state, &allowed_hosts);
    let disc_url = format!("{clean_issuer}/.well-known/openid-configuration");
    let disc_bytes = match fetcher.get_with_headers(&disc_url, &[]).await {
        Ok(FetchOutcome::Body { bytes, .. }) => bytes,
        _ => return failure(StatusCode::BAD_GATEWAY, "oidc_discovery_failed"),
    };
    let Ok(doc) = serde_json::from_slice::<DiscoveryDoc>(&disc_bytes) else {
        return failure(StatusCode::BAD_GATEWAY, "oidc_discovery_invalid");
    };
    let jwks_bytes = match fetcher.get_with_headers(&doc.jwks_uri, &[]).await {
        Ok(FetchOutcome::Body { bytes, .. }) => bytes,
        _ => return failure(StatusCode::BAD_GATEWAY, "oidc_jwks_failed"),
    };
    let Ok(jwks) = serde_json::from_slice::<JwkSet>(&jwks_bytes) else {
        return failure(StatusCode::BAD_GATEWAY, "oidc_jwks_invalid");
    };

    no_store_json(
        StatusCode::OK,
        serde_json::json!({
            "ok": true,
            "issuer": doc.issuer,
            "authorization_endpoint": doc.authorization_endpoint,
            "token_endpoint": doc.token_endpoint,
            "jwks_uri": doc.jwks_uri,
            "jwks_keys": jwks.keys.len(),
        }),
    )
}

pub async fn admin_reset_oidc(
    State(state): State<SharedState>,
    peer: Option<axum::Extension<ConnectInfo<SocketAddr>>>,
    headers: HeaderMap,
) -> Response {
    let env_cfg = OidcConfig::from_env();
    let digest = env_cfg.env_seed_digest();
    let (auth_method, actor) = security::validate_session_identity(&state, &headers)
        .unwrap_or_else(|| ("session".into(), "owner".into()));
    let client_ip = security::request_client_ip(&state, peer.as_ref(), &headers);

    {
        let db = state.db.lock().expect("database lock poisoned");
        let _ = db.conn.execute(
            "DELETE FROM settings WHERE key IN ('oidc_config_json', 'oidc_client_secret_enc')",
            [],
        );
        if env_cfg.source == "env" {
            let _ = persist_oidc_to_db(&db, state.secrets.as_ref(), &env_cfg, Some(&digest));
        }
        let _ = db.record_audit_event(
            now(),
            &actor,
            &auth_method,
            &client_ip,
            "oidc.reset",
            "oidc",
            &env_cfg.client_id,
            &serde_json::json!({ "source": env_cfg.source, "enabled": env_cfg.enabled }),
        );
    }

    state.security.set_oidc_cfg(env_cfg.clone());
    no_store_json(StatusCode::OK, oidc_view_json(&state, &env_cfg, &headers))
}
