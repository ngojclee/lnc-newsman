use crate::{
    SharedState, alerts, auth, failure,
    fetch::{FetchOutcome, Fetcher},
    internal_error, now, secrets, security,
};
use axum::{
    extract::{ConnectInfo, Query, State},
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Redirect, Response},
};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use jsonwebtoken::{Algorithm, DecodingKey, Validation, decode, decode_header, jwk::JwkSet};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{net::SocketAddr, sync::atomic::Ordering};
use subtle::ConstantTimeEq;
use url::Url;

const TX_TTL_SECONDS: u64 = 300;
const DISCOVERY_TTL_SECONDS: u64 = 300;

#[derive(Clone, Debug, Default)]
pub struct OidcConfig {
    pub enabled: bool,
    pub issuer_url: String,
    pub client_id: String,
    pub client_secret: String,
    pub redirect_uri: String,
    pub allowed_users: Vec<String>,
    pub allowed_hosts: Vec<String>,
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
        let allowed_users: Vec<String> = std::env::var("OIDC_ALLOWED_USERS")
            .unwrap_or_default()
            .split(',')
            .map(|s| s.trim().to_ascii_lowercase())
            .filter(|s| !s.is_empty())
            .collect();
        let allowed_hosts: Vec<String> = std::env::var("OIDC_ALLOWED_HOSTS")
            .unwrap_or_default()
            .split(',')
            .map(|s| s.trim().to_ascii_lowercase())
            .filter(|s| !s.is_empty())
            .collect();
        let explicit = env_trim("OIDC_ENABLED").map(|v| v.to_ascii_lowercase());
        let has_fields = !issuer_url.is_empty() && !client_id.is_empty() && !client_secret.is_empty();
        let enabled = match explicit.as_deref() {
            Some("true" | "1" | "yes") => has_fields,
            Some("false" | "0" | "no") => false,
            _ => has_fields,
        };
        Self {
            enabled,
            issuer_url,
            client_id,
            client_secret,
            redirect_uri,
            allowed_users,
            allowed_hosts,
        }
    }

    pub fn is_active(&self) -> bool {
        self.enabled
            && !self.issuer_url.is_empty()
            && !self.client_id.is_empty()
            && !self.client_secret.is_empty()
    }

    pub fn user_allowed(&self, candidates: &[Option<&str>]) -> Option<String> {
        if self.allowed_users.is_empty() {
            // Fail closed when OIDC_ALLOWED_USERS is empty.
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
}

fn oidc_fetcher(state: &SharedState) -> Fetcher {
    let mut hosts = state.security.oidc.allowed_hosts.clone();
    if hosts.is_empty() {
        return state.fetcher.clone();
    }
    hosts.sort();
    hosts.dedup();
    state
        .fetcher
        .clone()
        .with_private_hosts(hosts)
        .with_timeout(std::time::Duration::from_secs(10))
}

fn effective_redirect_uri(state: &SharedState, headers: &HeaderMap) -> Result<String, &'static str> {
    if !state.security.oidc.redirect_uri.is_empty() {
        return Ok(state.security.oidc.redirect_uri.clone());
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

async fn load_discovery(state: &SharedState) -> Result<DiscoveryDoc, &'static str> {
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
        state.security.oidc.issuer_url.trim_end_matches('/')
    );
    let outcome = oidc_fetcher(state)
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

async fn load_jwks(state: &SharedState, jwks_uri: &str, force: bool) -> Result<JwkSet, &'static str> {
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
    let outcome = oidc_fetcher(state)
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
    if token_nonce.is_empty() || !bool::from(token_nonce.as_bytes().ct_eq(expected_nonce.as_bytes()))
    {
        return Err("oidc_nonce_mismatch");
    }
    Ok(claims)
}

pub fn verify_id_token_with_jwks(
    id_token: &str,
    jwks: &JwkSet,
    expected_issuers: &[&str],
    client_id: &str,
    expected_nonce: &str,
) -> Result<String, &'static str> {
    let claims = verify_id_token_claims(id_token, jwks, expected_issuers, client_id, expected_nonce)?;
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
    if !state.security.oidc.is_active() {
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
    let doc = match load_discovery(&state).await {
        Ok(d) => d,
        Err(code) => return failure(StatusCode::BAD_GATEWAY, code),
    };
    let redirect_uri = match effective_redirect_uri(&state, &headers) {
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
        .append_pair("client_id", &state.security.oidc.client_id)
        .append_pair("redirect_uri", &redirect_uri)
        .append_pair("scope", "openid profile email")
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
    let mut response = Redirect::to(&format!("/login.html?error={code}")).into_response();
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
    if !state.security.oidc.is_active() {
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

    let Ok(doc) = load_discovery(&state).await else {
        return redirect_login_error("oidc_discovery_failed", secure);
    };
    let form = url::form_urlencoded::Serializer::new(String::new())
        .append_pair("grant_type", "authorization_code")
        .append_pair("code", code)
        .append_pair("redirect_uri", &tx.redirect_uri)
        .append_pair("client_id", &state.security.oidc.client_id)
        .append_pair("client_secret", &state.security.oidc.client_secret)
        .append_pair("code_verifier", &tx.code_verifier)
        .finish();
    let outcome = match oidc_fetcher(&state)
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

    let Ok(jwks) = load_jwks(&state, &doc.jwks_uri, false).await else {
        return redirect_login_error("oidc_jwks_failed", secure);
    };
    let configured_iss = state.security.oidc.issuer_url.trim_end_matches('/');
    let doc_iss = doc.issuer.trim_end_matches('/');
    let issuers = [configured_iss, doc_iss];
    let claims = match verify_id_token_claims(
        id_token,
        &jwks,
        &issuers,
        &state.security.oidc.client_id,
        &tx.nonce,
    ) {
        Ok(c) => c,
        Err("oidc_unknown_kid") => match load_jwks(&state, &doc.jwks_uri, true).await {
            Ok(fresh_jwks) => match verify_id_token_claims(
                id_token,
                &fresh_jwks,
                &issuers,
                &state.security.oidc.client_id,
                &tx.nonce,
            ) {
                Ok(c) => c,
                Err(code) => return redirect_login_error(code, secure),
            },
            Err(code) => return redirect_login_error(code, secure),
        },
        Err(code) => return redirect_login_error(code, secure),
    };

    let allowed_user = state.security.oidc.user_allowed(&[
        claims.preferred_username.as_deref(),
        claims.email.as_deref(),
        claims.sub.as_deref(),
    ]);
    let Some(username) = allowed_user else {
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
            &state.security.oidc.client_id,
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
