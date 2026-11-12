//! Business MCP Hub connector boundary (P8, the managed-connector half).
//!
//! The Hub is MCP-native: it opens its own MCP session against our `POST /mcp` and
//! forwards `tools/call`. Two things arrive on that hop, and neither is the reader's
//! session cookie or a reader API key:
//!
//! * `Authorization: Bearer <connector token>` — an app-specific credential, distinct
//!   from every reader/management key. It proves "this caller is the Hub".
//! * `X-MCP-Hub-Client-{ID,Scopes,Timestamp,Nonce,Signature}` (+ optional `-App-ID`) —
//!   a signed statement of *which agent* is asking and *what it may do*. It proves
//!   "the Hub vouches for this caller and these scopes".
//!
//! The second half is the one that matters: the bearer alone would let the Hub call
//! anything with the connector's full rights, so every tool decision is made from the
//! **signed** scopes, never from the bearer.
//!
//! Everything here is a pure function of the request plus the resolved configuration,
//! so the reject paths are unit-testable without a socket. Both secrets are resolved
//! `env_or_cfg`: the environment wins when set, otherwise the value the owner pasted
//! into `/admin` (sealed in the existing secret store). The environment half is what
//! an operator sets at deploy time; the stored half lets the owner pair a connector
//! without a redeploy.
//!
//! The connector never carries a secret it can print: `Resolved` is `Debug`-safe and
//! no endpoint ever returns a value, only "set"/"unset".

use crate::SharedState;
use axum::{
    Json,
    extract::Request,
    http::{HeaderMap, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
};
use hmac::{Hmac, Mac};
use serde::Deserialize;
use sha2::Sha256;
use subtle::ConstantTimeEq;

type HmacSha256 = Hmac<Sha256>;

/// The `audience` the Hub signs for. Compiled in so a missing header cannot widen it.
pub const DEFAULT_AUDIENCE: &str = "lnc-newsman";

/// The one scope this app can grant. Every read tool requires it; there is no write
/// scope to grant, because there is no write tool.
pub const READ_SCOPE: &str = "newsman:read";

/// Names of the two sealed values in the existing secret store. They are ordinary
/// secret slugs, so `/api/admin/secrets` already lists them without revealing them.
pub const BEARER_SECRET: &str = "connector-bearer";
pub const DELEGATION_SECRET: &str = "delegation-secret";

/// Non-secret connector settings the owner may change from `/admin`.
pub const AUDIENCE_KEY: &str = "connector_audience";
pub const MAX_SCOPES_KEY: &str = "connector_max_scopes";

/// How far the Hub's clock may differ from ours, matching the Hub's own 90 s rule.
const MAX_SKEW_SECONDS: u64 = 90;
/// How long a nonce is remembered. Comfortably longer than the skew window, so a
/// captured request cannot be replayed after the window reopens.
const NONCE_TTL_SECONDS: u64 = 300;
/// A bounded replay table: a caller cannot grow our memory by inventing nonces.
const MAX_NONCES: usize = 10_000;

pub const DENIED_BEARER: &str = "connector_auth_required";
pub const DENIED_DELEGATION: &str = "delegation_denied";

/// The environment half of `env_or_cfg`. A value here wins over the stored one.
#[derive(Clone, Default)]
pub struct ConnectorConfig {
    pub bearer: Option<String>,
    pub delegation_secret: Option<String>,
    pub audience: Option<String>,
    pub max_scopes: Option<Vec<String>>,
}

impl std::fmt::Debug for ConnectorConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Never print the bearer or the delegation secret, not even a prefix.
        f.debug_struct("ConnectorConfig")
            .field("has_bearer", &self.bearer.is_some())
            .field("has_delegation_secret", &self.delegation_secret.is_some())
            .field("audience", &self.audience)
            .field("max_scopes", &self.max_scopes)
            .finish()
    }
}

fn env_value(names: &[&str]) -> Option<String> {
    // Legacy names are accepted so an existing app stack keeps working unchanged.
    names.iter().find_map(|name| {
        std::env::var(name)
            .ok()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
    })
}

fn split_scopes(value: &str) -> Vec<String> {
    let mut scopes: Vec<String> = value
        .split(',')
        .map(str::trim)
        .filter(|scope| !scope.is_empty())
        .map(ToOwned::to_owned)
        .collect();
    scopes.sort_unstable();
    scopes.dedup();
    scopes
}

impl ConnectorConfig {
    pub fn from_env() -> Self {
        Self {
            bearer: env_value(&["CONNECTOR_BEARER", "LNC_CONNECTOR_TOKEN"]),
            delegation_secret: env_value(&["DELEGATION_SECRET", "LNC_DELEGATION_SECRET"]),
            audience: env_value(&["CONNECTOR_AUDIENCE", "LNC_MCP_AUDIENCE"]),
            max_scopes: env_value(&["CONNECTOR_MAX_SCOPES", "LNC_MCP_MAX_SCOPES"])
                .map(|value| split_scopes(&value)),
        }
    }
}

/// Which half a value came from. Only ever reported as a word, never with the value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Source {
    Env,
    Admin,
    Unset,
}

/// Configuration actually used by one request. The secrets are present here but are
/// never serialised or logged; `Debug` prints presence only.
pub struct Resolved {
    pub bearer: Option<String>,
    pub delegation_secret: Option<String>,
    pub audience: String,
    pub max_scopes: Vec<String>,
    pub bearer_source: Source,
    pub secret_source: Source,
    pub audience_source: Source,
}

impl std::fmt::Debug for Resolved {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Resolved")
            .field("bearer_source", &self.bearer_source)
            .field("secret_source", &self.secret_source)
            .field("audience", &self.audience)
            .field("max_scopes", &self.max_scopes)
            .finish()
    }
}

/// Everything the tool gate and the admin card need, with no secret exposed.
#[derive(Debug, Clone, serde::Serialize)]
pub struct Status {
    pub enabled: bool,
    pub audience: String,
    pub max_scopes: Vec<String>,
    pub bearer: Source,
    pub delegation_secret: Source,
    pub audience_source: Source,
    /// False when `LNC_MASTER_KEY` is absent, so `/admin` cannot store a value.
    pub secret_store_open: bool,
}

/// Resolve one value from the vault, treating "store closed" and "not stored" alike:
/// from a request's point of view an absent value is absent either way.
fn stored(state: &SharedState, name: &str) -> Option<String> {
    let vault = state.secrets.as_ref()?;
    let db = state.db.lock().ok()?;
    crate::secrets::get(&db, vault, name).ok()
}

fn stored_setting(state: &SharedState, key: &str) -> Option<String> {
    let db = state.db.lock().ok()?;
    db.get_setting(key)
        .ok()
        .flatten()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

pub fn resolve(state: &SharedState) -> Resolved {
    let cfg = &state.connector;
    let (bearer, bearer_source) = match cfg.bearer.clone() {
        Some(value) => (Some(value), Source::Env),
        None => match stored(state, BEARER_SECRET) {
            Some(value) => (Some(value), Source::Admin),
            None => (None, Source::Unset),
        },
    };
    let (delegation_secret, secret_source) = match cfg.delegation_secret.clone() {
        Some(value) => (Some(value), Source::Env),
        None => match stored(state, DELEGATION_SECRET) {
            Some(value) => (Some(value), Source::Admin),
            None => (None, Source::Unset),
        },
    };
    let (audience, audience_source) = match cfg.audience.clone() {
        Some(value) => (value, Source::Env),
        None => match stored_setting(state, AUDIENCE_KEY) {
            Some(value) => (value, Source::Admin),
            None => (DEFAULT_AUDIENCE.to_string(), Source::Unset),
        },
    };
    // The ceiling can only ever narrow: an admin value cannot grant a scope this
    // build does not implement, because the tool table is the final word.
    let max_scopes = cfg
        .max_scopes
        .clone()
        .or_else(|| stored_setting(state, MAX_SCOPES_KEY).map(|value| split_scopes(&value)))
        .unwrap_or_else(|| vec![READ_SCOPE.to_string()]);
    Resolved {
        bearer,
        delegation_secret,
        audience,
        max_scopes,
        bearer_source,
        secret_source,
        audience_source,
    }
}

pub fn status(state: &SharedState) -> Status {
    let resolved = resolve(state);
    Status {
        enabled: resolved.bearer.is_some() && resolved.delegation_secret.is_some(),
        audience: resolved.audience.clone(),
        max_scopes: resolved.max_scopes.clone(),
        bearer: resolved.bearer_source,
        delegation_secret: resolved.secret_source,
        audience_source: resolved.audience_source,
        secret_store_open: state.secrets.is_some(),
    }
}

/// Why a connector request was refused. The code is stable and safe to show; the
/// message never quotes a header value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Denied {
    pub status: StatusCode,
    pub code: &'static str,
}

impl Denied {
    fn unauthorized() -> Self {
        Self {
            status: StatusCode::UNAUTHORIZED,
            code: DENIED_BEARER,
        }
    }
    fn delegation() -> Self {
        Self {
            status: StatusCode::FORBIDDEN,
            code: DENIED_DELEGATION,
        }
    }
    pub fn response(&self) -> Response {
        (
            self.status,
            Json(serde_json::json!({"ok": false, "error": self.code})),
        )
            .into_response()
    }
}

/// A verified Hub caller. Only ever built by [`authenticate`], which is why carrying
/// one is proof the signature was checked.
#[derive(Debug, Clone)]
pub struct Caller {
    pub client_id: String,
    /// Present only for a verified v4 assertion. Never taken from tool arguments.
    pub app_id: Option<String>,
    pub scopes: Vec<String>,
}

impl Caller {
    pub fn has_scope(&self, scope: &str) -> bool {
        self.scopes.iter().any(|granted| granted == scope)
    }
}

/// True when the request carries the `Bearer` credential the connector path uses.
/// This is what excludes a browser request from that path: a session or a reader key
/// may only arrive with `x-api-key`, never as a bearer, so a cookie request can not be
/// talked into being treated as a Hub call.
pub fn presents_connector_bearer(headers: &HeaderMap) -> bool {
    header_str(headers, "authorization")
        .is_some_and(|value| value.starts_with("Bearer "))
        && header_str(headers, "x-mcp-hub-client-signature").is_some()
}

fn header_str<'a>(headers: &'a HeaderMap, name: &str) -> Option<&'a str> {
    headers
        .get(name)
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

fn constant_time_eq(left: &str, right: &str) -> bool {
    bool::from(left.as_bytes().ct_eq(right.as_bytes()))
}

fn valid_client_id(value: &str) -> bool {
    let mut chars = value.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    value.len() <= 48
        && first.is_ascii_alphanumeric()
        && chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '-'))
}

fn valid_app_id(value: &str) -> bool {
    let mut chars = value.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    value.len() <= 64
        && first.is_ascii_alphanumeric()
        && chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '-'))
}

fn valid_scope(value: &str) -> bool {
    let mut chars = value.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    value.len() <= 64
        && first.is_ascii_alphanumeric()
        && chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, ':' | '.' | '_' | '*' | '-'))
}

fn valid_nonce(value: &str) -> bool {
    (20..=128).contains(&value.len())
        && value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-'))
}

/// The exact bytes the Hub signed. v4 is used when — and only when — an app id is
/// present, so a caller cannot drop or attach one after signing.
fn envelope(
    app_id: Option<&str>,
    audience: &str,
    client_id: &str,
    timestamp: &str,
    nonce: &str,
    scopes: &[String],
) -> String {
    let joined = scopes.join(",");
    match app_id {
        Some(app_id) => format!(
            "v4\n{audience}\n{client_id}\n{app_id}\n{timestamp}\n{nonce}\n{joined}"
        ),
        None => format!("v3\n{audience}\n{client_id}\n{timestamp}\n{nonce}\n{joined}"),
    }
}

fn sign(secret: &str, payload: &str) -> Option<String> {
    let mut mac = HmacSha256::new_from_slice(secret.as_bytes()).ok()?;
    mac.update(payload.as_bytes());
    Some(
        mac.finalize()
            .into_bytes()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect(),
    )
}

/// Claim a nonce once. `false` means it was already seen inside the window.
fn claim_nonce(state: &SharedState, nonce: &str, now: u64) -> bool {
    let Ok(mut seen) = state.connector_nonces.lock() else {
        return false;
    };
    seen.retain(|_, expires_at| *expires_at > now);
    if seen.contains_key(nonce) {
        return false;
    }
    if seen.len() >= MAX_NONCES {
        // Full of unexpired nonces: refuse rather than grow without bound. A healthy
        // Hub sends far fewer than this within five minutes.
        return false;
    }
    seen.insert(nonce.to_string(), now + NONCE_TTL_SECONDS);
    true
}

/// Verify the connector bearer and the delegation signature, in that order. Order
/// matters: the bearer is the cheap check, and failing it must not advertise whether
/// a signature would have been accepted.
pub fn authenticate(state: &SharedState, headers: &HeaderMap) -> Result<Caller, Denied> {
    let resolved = resolve(state);
    let (Some(expected_bearer), Some(secret)) =
        (resolved.bearer.as_deref(), resolved.delegation_secret.as_deref())
    else {
        // Not paired yet. Same answer as a wrong bearer: nothing is disclosed and a
        // caller cannot tell "misconfigured" from "wrong token".
        return Err(Denied::unauthorized());
    };
    let supplied = header_str(headers, "authorization")
        .and_then(|value| value.strip_prefix("Bearer "))
        .map(str::trim)
        .unwrap_or_default();
    if supplied.is_empty() || !constant_time_eq(supplied, expected_bearer) {
        return Err(Denied::unauthorized());
    }

    let client_id = header_str(headers, "x-mcp-hub-client-id").ok_or_else(Denied::delegation)?;
    let scopes_raw =
        header_str(headers, "x-mcp-hub-client-scopes").ok_or_else(Denied::delegation)?;
    let timestamp = header_str(headers, "x-mcp-hub-client-timestamp")
        .ok_or_else(Denied::delegation)?;
    let nonce = header_str(headers, "x-mcp-hub-client-nonce").ok_or_else(Denied::delegation)?;
    let signature =
        header_str(headers, "x-mcp-hub-client-signature").ok_or_else(Denied::delegation)?;
    let app_id = header_str(headers, "x-mcp-hub-app-id");

    if !valid_client_id(client_id) || app_id.is_some_and(|value| !valid_app_id(value)) {
        return Err(Denied::delegation());
    }
    let mut scopes = split_scopes(scopes_raw);
    if scopes.is_empty() || scopes.iter().any(|scope| !valid_scope(scope)) {
        return Err(Denied::delegation());
    }
    // A scope this build does not implement can never be granted, even if the Hub
    // signed it. Refuse the whole call rather than silently narrowing it: a caller
    // that asked for more than exists is confused, and a partial grant would hide that.
    if scopes
        .iter()
        .any(|scope| !resolved.max_scopes.iter().any(|max| max == scope))
    {
        return Err(Denied::delegation());
    }

    if !valid_nonce(nonce) {
        return Err(Denied::delegation());
    }
    let Ok(timestamp_value) = timestamp.parse::<u64>() else {
        return Err(Denied::delegation());
    };
    if crate::now().abs_diff(timestamp_value) > MAX_SKEW_SECONDS {
        return Err(Denied::delegation());
    }

    let payload = envelope(
        app_id,
        &resolved.audience,
        client_id,
        timestamp,
        nonce,
        &scopes,
    );
    let Some(expected) = sign(secret, &payload) else {
        return Err(Denied::delegation());
    };
    let supplied_signature = signature.to_ascii_lowercase();
    if !constant_time_eq(&expected, &supplied_signature) {
        return Err(Denied::delegation());
    }
    // Only after the signature holds: an unauthenticated caller must not be able to
    // burn a nonce that a real request is about to use.
    if !claim_nonce(state, nonce, crate::now()) {
        return Err(Denied::delegation());
    }
    scopes.shrink_to_fit();
    Ok(Caller {
        client_id: client_id.to_string(),
        app_id: app_id.map(str::to_string),
        scopes,
    })
}

#[cfg(test)]
#[path = "connector_tests.rs"]
mod tests;

// ---------- admin surface (labels and presence only) ----------

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AdminPatch {
    /// `null` clears the stored value and falls back to the compiled-in audience.
    pub audience: Option<String>,
    /// `null` (or an empty string) restores the built-in ceiling.
    pub max_scopes: Option<String>,
}

pub async fn admin_status(axum::extract::State(state): axum::extract::State<SharedState>) -> Response {
    Json(status(&state)).into_response()
}

/// Writes the two non-secret connector settings. The bearer and the delegation secret
/// are **not** accepted here: they are secrets, so they go through
/// `PUT /api/admin/secrets/{name}` like every other sealed value, and this endpoint can
/// never echo one back.
pub async fn admin_update(
    axum::extract::State(state): axum::extract::State<SharedState>,
    Json(patch): Json<AdminPatch>,
) -> Response {
    let mut writes: Vec<(&'static str, String)> = Vec::new();
    if let Some(audience) = patch.audience.as_ref() {
        let audience = audience.trim();
        // The Hub signs a plain audience string; anything with whitespace would be
        // impossible to reproduce on the Hub side, so refuse it here instead.
        if !audience.is_empty()
            && (audience.len() > 64
                || !audience
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-')))
        {
            return crate::failure(StatusCode::BAD_REQUEST, "invalid_connector_audience");
        }
        writes.push((AUDIENCE_KEY, audience.to_string()));
    }
    if let Some(raw) = patch.max_scopes.as_ref() {
        let scopes = split_scopes(raw);
        if scopes.iter().any(|scope| !valid_scope(scope)) {
            return crate::failure(StatusCode::BAD_REQUEST, "invalid_connector_scopes");
        }
        if scopes.iter().any(|scope| scope != READ_SCOPE) {
            // There is no write scope to grant, and silently dropping an unknown one
            // would make the ceiling look wider than it is.
            return crate::failure(StatusCode::BAD_REQUEST, "unsupported_connector_scope");
        }
        writes.push((MAX_SCOPES_KEY, scopes.join(",")));
    }
    if writes.is_empty() {
        return Json(status(&state)).into_response();
    }
    let owned: Vec<(&str, &str)> = writes
        .iter()
        .map(|(key, value)| (*key, value.as_str()))
        .collect();
    let result = state
        .db
        .lock()
        .expect("database lock poisoned")
        .set_settings(&owned);
    if let Err(err) = result {
        return crate::internal_error(err);
    }
    Json(status(&state)).into_response()
}

// ---------- request gate ----------

/// `POST /mcp` accepts two callers from the reader's side and one from the Hub's:
///
/// * a signed-in browser session, or a reader API key (`x-api-key`) — exactly the
///   credential `/api/v1/*` uses, unchanged;
/// * a Hub call — a `Bearer` connector token plus a delegation signature, scoped to
///   [`READ_SCOPE`].
///
/// The two are told apart by *how* the credential arrives, which is why a browser
/// request can never be mistaken for a Hub call: sessions travel as a cookie and reader
/// keys as `x-api-key`, so only the Hub path ever sees a `Bearer`.
///
/// A wrong or missing connector token is a 401; a valid token with a bad signature,
/// audience, timestamp, nonce or scope is a 403. Neither falls through to the reader
/// check — a caller that presented a bearer is judged on that bearer alone.
pub async fn require_mcp(
    axum::extract::State(state): axum::extract::State<SharedState>,
    req: Request,
    next: Next,
) -> Response {
    if presents_connector_bearer(req.headers()) {
        // The tool gate reads the verified caller from an extension, so a tool call
        // cannot run without one: if this insert is skipped, every call is refused.
        return match authenticate(&state, req.headers()) {
            Ok(caller) => {
                // Audit the delegated caller, not just "the Hub": with only the
                // connector token every request would look like one actor.
                tracing::debug!(
                    client_id = %caller.client_id,
                    app_id = caller.app_id.as_deref().unwrap_or(""),
                    scopes = %caller.scopes.join(","),
                    "mcp delegation accepted"
                );
                let mut req = req;
                req.extensions_mut().insert(caller);
                next.run(req).await
            }
            Err(denied) => {
                tracing::warn!(
                    code = denied.code,
                    connector = %header_str(req.headers(), "x-mcp-hub-client-id").unwrap_or(""),
                    "mcp delegation refused"
                );
                denied.response()
            }
        };
    }
    if crate::session_valid(&state, req.headers())
        || crate::auth::has_api_key(req.headers(), &state.cfg.api_keys)
    {
        return next.run(req).await;
    }
    // No credential at all. Answer as the reader API does, so an unauthenticated
    // probe learns nothing about which surfaces exist.
    crate::failure(StatusCode::UNAUTHORIZED, "unauthorized")
}

/// The Hub's scope gate for one tool. A reader session or key carries no delegation,
/// so it keeps the access it had: it sees the same data in the human UI already, and
/// the connector boundary is about what the *Hub* may do, not about narrowing a
/// logged-in human.
pub fn scope_allows(caller: Option<&Caller>, tool: &str) -> bool {
    match caller {
        None => true,
        Some(caller) => crate::mcp::tool_scope(tool)
            .map(|scope| caller.has_scope(scope))
            .unwrap_or(false),
    }
}
