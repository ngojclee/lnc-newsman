//! Canonical Blueprint v2 Admin Lifecycle APIs:
//! - Management key rotation (`Argon2id`) & instant `session_epoch` revocation
//! - Inbound API keys (`Copy · Rotate · Revoke · Remove`, HMAC-SHA256 + AES-256-GCM dual store)
//! - Unified AI Provider Hub (`LNC Agents Hub` + custom OpenAI-compat providers, catalog, 2 model roles)
//! - MCP Connectors (Inbound `/mcp` pairing + custom outbound MCP connectors with live `tools/list` test)
//! - Immutable security audit trail & SSRF-safe outbound validation

use crate::{
    SharedState, agent_bridge, auth, connector, db, failure,
    fetch::{self, FetchOutcome, Fetcher},
    hub, internal_error, mcp, now, secrets, security,
};
use axum::{
    Extension, Json,
    extract::{ConnectInfo, Path, Query, State},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use chrono::Utc;
use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{net::IpAddr, net::SocketAddr};
use url::Url;

const MASKED_SECRET: &str = "********";
const MIN_SECRET_LEN: usize = 16;

fn no_store_response(status: StatusCode, body: Value) -> Response {
    let mut res = (status, Json(body)).into_response();
    res.headers_mut().insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("no-store, private"),
    );
    res
}

fn ok_no_store(body: Value) -> Response {
    no_store_response(StatusCode::OK, body)
}

fn bad_request_msg(msg: impl Into<String>) -> Response {
    no_store_response(
        StatusCode::BAD_REQUEST,
        json!({ "ok": false, "error": "bad_request", "message": msg.into() }),
    )
}

fn check_copy_rate_limit(
    state: &SharedState,
    peer: Option<&Extension<ConnectInfo<SocketAddr>>>,
    headers: &HeaderMap,
) -> Result<String, Response> {
    let ip = security::request_client_ip(state, peer, headers);
    let allowed = state
        .security
        .copy_limiter
        .lock()
        .expect("limiter lock poisoned")
        .allow_and_record(&ip, now());
    if !allowed {
        return Err(failure(StatusCode::TOO_MANY_REQUESTS, "copy_rate_limited"));
    }
    Ok(ip)
}

fn actor_identity(state: &SharedState, headers: &HeaderMap) -> (String, String) {
    security::validate_session_identity(state, headers)
        .unwrap_or_else(|| ("management_key".to_string(), "admin".to_string()))
}

fn mint_token(prefix: &str, random_bytes: usize) -> Result<String, Response> {
    let mut buf = vec![0u8; random_bytes];
    if getrandom::fill(&mut buf).is_err() {
        return Err(failure(StatusCode::INTERNAL_SERVER_ERROR, "rng_failed"));
    }
    Ok(format!("{prefix}{}", URL_SAFE_NO_PAD.encode(buf)))
}

fn token_prefix_and_last4(token: &str) -> (String, String) {
    let chars: Vec<char> = token.chars().collect();
    let prefix: String = chars.iter().take(10).collect();
    let last4: String = if chars.len() >= 4 {
        chars[chars.len() - 4..].iter().collect()
    } else {
        token.to_string()
    };
    (prefix, last4)
}

fn sanitize_slug(raw: &str, fallback: &str) -> String {
    let mut out = String::new();
    let mut prev_dash = false;
    for ch in raw.trim().to_ascii_lowercase().chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch);
            prev_dash = false;
        } else if !prev_dash && !out.is_empty() {
            out.push('-');
            prev_dash = true;
        }
    }
    while out.ends_with('-') {
        out.pop();
    }
    if out.is_empty() {
        fallback.to_string()
    } else {
        out.chars().take(48).collect()
    }
}

fn outbound_fetcher(state: &SharedState, extra_allowed_hosts: &[String]) -> Fetcher {
    let cfg = state.agent_bridge.config();
    let mut hosts: Vec<String> = Vec::new();
    for h in state
        .hub
        .allowed_hosts
        .iter()
        .chain(state.telegram.allowed_hosts.iter())
        .chain(cfg.allowed_hosts.iter())
        .chain(extra_allowed_hosts.iter())
    {
        let clean = h.trim().to_ascii_lowercase();
        if !clean.is_empty() && !hosts.contains(&clean) {
            hosts.push(clean);
        }
    }
    for env_key in [
        "APP_AGENTS_ALLOWED_HOSTS",
        "ASTRAEA_ALLOWED_HOSTS",
        "MCP_ALLOWED_HOSTS",
    ] {
        if let Ok(raw) = std::env::var(env_key) {
            for part in raw.split(',') {
                let clean = part.trim().to_ascii_lowercase();
                if !clean.is_empty() && !hosts.contains(&clean) {
                    hosts.push(clean);
                }
            }
        }
    }
    state
        .fetcher
        .clone()
        .with_private_hosts(hosts)
        .with_timeout(std::time::Duration::from_secs(30))
}

fn validate_outbound_url(state: &SharedState, raw: &str) -> Result<String, Response> {
    let parsed = fetch::parse_target(raw.trim())
        .map_err(|e| bad_request_msg(format!("invalid url: {}", e.code())))?;
    let fetcher = outbound_fetcher(state, &[]);
    let allow_priv = fetcher.host_allows_private(&parsed);
    match parsed.host() {
        Some(url::Host::Ipv4(v4)) if !fetch::is_public_ip(IpAddr::V4(v4)) && !allow_priv => {
            return Err(failure(StatusCode::BAD_REQUEST, "ssrf_blocked_private_url"));
        }
        Some(url::Host::Ipv6(v6)) if !fetch::is_public_ip(IpAddr::V6(v6)) && !allow_priv => {
            return Err(failure(StatusCode::BAD_REQUEST, "ssrf_blocked_private_url"));
        }
        Some(url::Host::Domain(dom)) => {
            let low = dom.trim_end_matches('.').to_ascii_lowercase();
            if !allow_priv
                && (low == "localhost"
                    || low.ends_with(".localhost")
                    || low.ends_with(".local")
                    || low.ends_with(".internal"))
            {
                return Err(failure(StatusCode::BAD_REQUEST, "ssrf_blocked_private_url"));
            }
        }
        _ => {}
    }
    Ok(parsed.to_string().trim_end_matches('/').to_string())
}

fn secret_for_copy(
    state: &SharedState,
    aad: &str,
    encoded: &str,
    fallback_secret_name: Option<&str>,
) -> Result<String, Response> {
    let Some(vault) = state.secrets.as_ref() else {
        return Err(failure(
            StatusCode::SERVICE_UNAVAILABLE,
            "secret_store_not_configured",
        ));
    };
    if !encoded.trim().is_empty() {
        return vault
            .open_envelope(aad, encoded)
            .map_err(|_| failure(StatusCode::CONFLICT, "copy_unavailable_rotate_required"));
    }
    if let Some(sname) = fallback_secret_name.filter(|s| !s.is_empty()) {
        let db = state.db.lock().expect("database lock poisoned");
        if let Ok(val) = secrets::get(&db, vault, sname) {
            return Ok(val);
        }
    }
    Err(failure(
        StatusCode::CONFLICT,
        "copy_unavailable_rotate_required",
    ))
}

fn is_blank_or_mask(val: Option<&str>) -> bool {
    match val {
        None => true,
        Some(s) => {
            let t = s.trim();
            t.is_empty() || t == MASKED_SECRET
        }
    }
}

// ===========================================================================
// 1. Inbound API Keys + Management Key Rotation + Session Epoch Revocation
// ===========================================================================

#[derive(Serialize)]
struct ApiKeyView {
    id: String,
    name: String,
    label: String,
    prefix: String,
    last4: String,
    scopes: Vec<String>,
    status: String,
    source: String,
    env_managed: bool,
    copy_available: bool,
    created_at: u64,
    updated_at: u64,
    last_used_at: u64,
    revoked_at: u64,
}

fn list_api_keys_data(state: &SharedState) -> anyhow::Result<(Vec<ApiKeyView>, Value)> {
    let db = state.db.lock().expect("database lock poisoned");
    let mut st = db.conn.prepare(
        "SELECT id, name, prefix, last4, scopes_json, status, source, copy_available,
                created_at, updated_at, last_used_at, revoked_at
         FROM api_keys
         ORDER BY created_at DESC, id DESC",
    )?;
    let mut items: Vec<ApiKeyView> = st
        .query_map([], |r| {
            let id: String = r.get(0)?;
            let name: String = r.get(1)?;
            let scopes_raw: String = r.get(4)?;
            let source: String = r.get(6)?;
            let copy_flag: i64 = r.get(7)?;
            Ok(ApiKeyView {
                id,
                name: name.clone(),
                label: name,
                prefix: r.get(2)?,
                last4: r.get(3)?,
                scopes: serde_json::from_str(&scopes_raw)
                    .unwrap_or_else(|_| vec!["newsman:read".to_string()]),
                status: r.get(5)?,
                env_managed: source == "env",
                source,
                copy_available: copy_flag != 0 && state.secrets.is_some(),
                created_at: r.get::<_, i64>(8)?.max(0) as u64,
                updated_at: r.get::<_, i64>(9)?.max(0) as u64,
                last_used_at: r.get::<_, i64>(10)?.max(0) as u64,
                revoked_at: r.get::<_, i64>(11)?.max(0) as u64,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    for (idx, raw) in state.cfg.api_keys.iter().enumerate() {
        let (prefix, last4) = token_prefix_and_last4(raw);
        items.push(ApiKeyView {
            id: format!("env-key-{}", idx + 1),
            name: format!("LNC_API_KEYS #{}", idx + 1),
            label: format!("LNC_API_KEYS #{}", idx + 1),
            prefix,
            last4,
            scopes: vec!["newsman:read".to_string(), "newsman:write".to_string()],
            status: "active".to_string(),
            source: "env".to_string(),
            env_managed: true,
            copy_available: false,
            created_at: 0,
            updated_at: 0,
            last_used_at: 0,
            revoked_at: 0,
        });
    }

    let rt = db.runtime_auth_state()?;
    let configured = auth::management_key_configured(&db, state.cfg.management_key.as_deref());
    let mgmt = json!({
        "configured": configured,
        "source": if !rt.mgmt_key_hash.is_empty() { rt.mgmt_source.clone() } else if state.cfg.management_key.is_some() { "env".to_string() } else { "unset".to_string() },
        "hash_algo": "argon2id",
        "session_epoch": rt.session_epoch,
        "oidc_enabled": state.security.oidc.is_active(),
        "vault_configured": state.secrets.is_some(),
        "env_pinned": rt.mgmt_source == "env" && state.cfg.management_key.is_some(),
    });
    Ok((items, mgmt))
}

pub async fn get_api_keys(State(state): State<SharedState>) -> Response {
    match list_api_keys_data(&state) {
        Ok((items, mgmt)) => ok_no_store(json!({ "ok": true, "items": items, "mgmt": mgmt })),
        Err(err) => internal_error(err),
    }
}

#[derive(Deserialize)]
pub struct CreateApiKeyInput {
    label: Option<String>,
    name: Option<String>,
    scopes: Option<Vec<String>>,
    custom_secret: Option<String>,
}

pub async fn post_api_key(
    State(state): State<SharedState>,
    peer: Option<Extension<ConnectInfo<SocketAddr>>>,
    headers: HeaderMap,
    Json(input): Json<CreateApiKeyInput>,
) -> Response {
    let label = input
        .label
        .or(input.name)
        .unwrap_or_default()
        .trim()
        .to_string();
    if label.is_empty() || label.len() > 80 {
        return bad_request_msg("API key label must be 1..80 characters");
    }
    let scopes: Vec<String> = input
        .scopes
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| vec!["newsman:read".to_string()]);
    if scopes.iter().any(|s| s != "newsman:read" && s != "newsman:write") {
        return bad_request_msg("invalid scope (allowed: newsman:read, newsman:write)");
    }
    let raw_token = match input
        .custom_secret
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        Some(custom) => {
            if custom.len() < MIN_SECRET_LEN {
                return bad_request_msg("custom_secret must be at least 16 characters");
            }
            custom.to_string()
        }
        None => match mint_token("nm_", 24) {
            Ok(t) => t,
            Err(res) => return res,
        },
    };

    let id = match mint_token("key_", 9) {
        Ok(id) => sanitize_slug(&id, "key-item"),
        Err(res) => return res,
    };
    let (prefix, last4) = token_prefix_and_last4(&raw_token);
    let pepper = state.security.api_key_pepper(state.secrets.as_ref());
    let hmac_hex = security::hmac_token_with_pepper(&pepper, &raw_token);
    let (secret_enc, copy_available) = if let Some(vault) = state.secrets.as_ref() {
        match vault.seal_envelope(&format!("api_key:{id}"), &raw_token) {
            Ok(enc) => (enc, 1_i64),
            Err(err) => return failure(StatusCode::BAD_REQUEST, err.code()),
        }
    } else {
        (String::new(), 0_i64)
    };
    let ts = now();
    let scopes_json = serde_json::to_string(&scopes).unwrap_or_else(|_| "[\"newsman:read\"]".into());
    let (auth_method, actor) = actor_identity(&state, &headers);
    let ip = security::request_client_ip(&state, peer.as_ref(), &headers);

    {
        let db = state.db.lock().expect("database lock poisoned");
        if let Err(err) = db.conn.execute(
            "INSERT INTO api_keys(id, name, prefix, last4, key_hash_hmac, secret_enc, copy_available, scopes_json, status, source, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 'active', 'runtime', ?9, ?9)",
            params![id, label, prefix, last4, hmac_hex, secret_enc, copy_available, scopes_json, ts],
        ) {
            return internal_error(err);
        }
        let _ = db.record_audit_event(
            ts,
            &actor,
            &auth_method,
            &ip,
            "api_key.created",
            "api_key",
            &id,
            &json!({ "name": label, "copy_available": copy_available != 0 }),
        );
    }

    match list_api_keys_data(&state) {
        Ok((items, mgmt)) => {
            let item = items.iter().find(|k| k.id == id);
            no_store_response(
                StatusCode::CREATED,
                json!({
                    "ok": true,
                    "id": id,
                    "secret": raw_token,
                    "key": raw_token,
                    "item": item,
                    "items": items,
                    "mgmt": mgmt,
                }),
            )
        }
        Err(err) => internal_error(err),
    }
}

#[derive(Deserialize)]
pub struct PatchApiKeyInput {
    label: Option<String>,
    name: Option<String>,
    scopes: Option<Vec<String>>,
    status: Option<String>,
}

pub async fn patch_api_key(
    State(state): State<SharedState>,
    peer: Option<Extension<ConnectInfo<SocketAddr>>>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(input): Json<PatchApiKeyInput>,
) -> Response {
    if id.starts_with("env-key-") {
        return failure(StatusCode::CONFLICT, "env_managed_immutable");
    }
    let ts = now();
    let (auth_method, actor) = actor_identity(&state, &headers);
    let ip = security::request_client_ip(&state, peer.as_ref(), &headers);

    {
        let db = state.db.lock().expect("database lock poisoned");
        let existing: Option<(String, String, String)> = db
            .conn
            .query_row(
                "SELECT name, scopes_json, status FROM api_keys WHERE id = ?1",
                [&id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .optional()
            .ok()
            .flatten();
        let Some((cur_name, cur_scopes, cur_status)) = existing else {
            return failure(StatusCode::NOT_FOUND, "not_found");
        };
        let next_name = input
            .label
            .or(input.name)
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .unwrap_or(cur_name);
        let next_scopes = match input.scopes {
            Some(s) if !s.is_empty() => {
                if s.iter().any(|x| x != "newsman:read" && x != "newsman:write") {
                    return bad_request_msg("invalid scope");
                }
                serde_json::to_string(&s).unwrap_or(cur_scopes)
            }
            _ => cur_scopes,
        };
        let next_status = match input.status.as_deref() {
            Some("active") => "active".to_string(),
            Some("revoked") => "revoked".to_string(),
            None => cur_status,
            Some(_) => return bad_request_msg("status must be active or revoked"),
        };
        let revoked_at = if next_status == "revoked" { ts } else { 0 };
        if let Err(err) = db.conn.execute(
            "UPDATE api_keys SET name = ?2, scopes_json = ?3, status = ?4, revoked_at = CASE WHEN ?4 = 'revoked' THEN ?5 ELSE 0 END, updated_at = ?5 WHERE id = ?1",
            params![id, next_name, next_scopes, next_status, ts],
        ) {
            return internal_error(err);
        }
        let _ = revoked_at;
        let _ = db.record_audit_event(
            ts,
            &actor,
            &auth_method,
            &ip,
            "api_key.updated",
            "api_key",
            &id,
            &json!({ "status": next_status }),
        );
    }

    match list_api_keys_data(&state) {
        Ok((items, mgmt)) => ok_no_store(json!({ "ok": true, "items": items, "mgmt": mgmt })),
        Err(err) => internal_error(err),
    }
}

pub async fn copy_api_key(
    State(state): State<SharedState>,
    peer: Option<Extension<ConnectInfo<SocketAddr>>>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Response {
    let ip = match check_copy_rate_limit(&state, peer.as_ref(), &headers) {
        Ok(ip) => ip,
        Err(res) => return res,
    };
    if id.starts_with("env-key-") {
        return failure(StatusCode::CONFLICT, "env_managed_immutable");
    }
    let (auth_method, actor) = actor_identity(&state, &headers);
    let (secret_enc, copy_flag): (String, i64) = {
        let db = state.db.lock().expect("database lock poisoned");
        let row: Option<(String, i64)> = db
            .conn
            .query_row(
                "SELECT secret_enc, copy_available FROM api_keys WHERE id = ?1",
                [&id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()
            .ok()
            .flatten();
        match row {
            Some(r) => r,
            None => return failure(StatusCode::NOT_FOUND, "not_found"),
        }
    };
    if copy_flag == 0 || secret_enc.is_empty() {
        return failure(StatusCode::CONFLICT, "copy_unavailable_rotate_required");
    }
    let plain = match secret_for_copy(&state, &format!("api_key:{id}"), &secret_enc, None) {
        Ok(p) => p,
        Err(res) => return res,
    };
    {
        let db = state.db.lock().expect("database lock poisoned");
        let _ = db.record_audit_event(
            now(),
            &actor,
            &auth_method,
            &ip,
            "secret.copy",
            "api_key",
            &id,
            &json!({ "copied_field": "api_key" }),
        );
    }
    ok_no_store(json!({ "ok": true, "secret": plain, "copied_field": "api_key" }))
}

#[derive(Deserialize, Default)]
pub struct RotateSecretInput {
    custom_secret: Option<String>,
    api_key: Option<String>,
    secret: Option<String>,
}

pub async fn rotate_api_key(
    State(state): State<SharedState>,
    peer: Option<Extension<ConnectInfo<SocketAddr>>>,
    headers: HeaderMap,
    Path(id): Path<String>,
    input: Option<Json<RotateSecretInput>>,
) -> Response {
    if id.starts_with("env-key-") {
        return failure(StatusCode::CONFLICT, "env_managed_immutable");
    }
    let body = input.map(|j| j.0).unwrap_or_default();
    let supplied = body
        .custom_secret
        .or(body.api_key)
        .or(body.secret)
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    let new_token = match supplied {
        Some(custom) => {
            if custom.len() < MIN_SECRET_LEN {
                return bad_request_msg("rotated key must be at least 16 characters");
            }
            custom
        }
        None => match mint_token("nm_", 24) {
            Ok(t) => t,
            Err(res) => return res,
        },
    };
    let (prefix, last4) = token_prefix_and_last4(&new_token);
    let pepper = state.security.api_key_pepper(state.secrets.as_ref());
    let hmac_hex = security::hmac_token_with_pepper(&pepper, &new_token);
    let (secret_enc, copy_available) = if let Some(vault) = state.secrets.as_ref() {
        match vault.seal_envelope(&format!("api_key:{id}"), &new_token) {
            Ok(enc) => (enc, 1_i64),
            Err(err) => return failure(StatusCode::BAD_REQUEST, err.code()),
        }
    } else {
        (String::new(), 0_i64)
    };
    let ts = now();
    let (auth_method, actor) = actor_identity(&state, &headers);
    let ip = security::request_client_ip(&state, peer.as_ref(), &headers);

    {
        let db = state.db.lock().expect("database lock poisoned");
        let changed = db
            .conn
            .execute(
                "UPDATE api_keys
                 SET prefix = ?2, last4 = ?3, key_hash_hmac = ?4, secret_enc = ?5,
                     copy_available = ?6, status = 'active', revoked_at = 0, updated_at = ?7
                 WHERE id = ?1",
                params![id, prefix, last4, hmac_hex, secret_enc, copy_available, ts],
            )
            .unwrap_or(0);
        if changed == 0 {
            return failure(StatusCode::NOT_FOUND, "not_found");
        }
        let _ = db.record_audit_event(
            ts,
            &actor,
            &auth_method,
            &ip,
            "api_key.rotated",
            "api_key",
            &id,
            &json!({ "prefix": prefix, "last4": last4 }),
        );
    }

    match list_api_keys_data(&state) {
        Ok((items, mgmt)) => {
            let item = items.iter().find(|k| k.id == id);
            ok_no_store(json!({
                "ok": true,
                "id": id,
                "secret": new_token,
                "key": new_token,
                "item": item,
                "items": items,
                "mgmt": mgmt,
            }))
        }
        Err(err) => internal_error(err),
    }
}

pub async fn revoke_api_key(
    State(state): State<SharedState>,
    peer: Option<Extension<ConnectInfo<SocketAddr>>>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Response {
    if id.starts_with("env-key-") {
        return failure(StatusCode::CONFLICT, "env_managed_immutable");
    }
    let ts = now();
    let (auth_method, actor) = actor_identity(&state, &headers);
    let ip = security::request_client_ip(&state, peer.as_ref(), &headers);
    {
        let db = state.db.lock().expect("database lock poisoned");
        let changed = db
            .conn
            .execute(
                "UPDATE api_keys SET status = 'revoked', revoked_at = ?2, updated_at = ?2 WHERE id = ?1",
                params![id, ts],
            )
            .unwrap_or(0);
        if changed == 0 {
            return failure(StatusCode::NOT_FOUND, "not_found");
        }
        let _ = db.record_audit_event(
            ts,
            &actor,
            &auth_method,
            &ip,
            "api_key.revoked",
            "api_key",
            &id,
            &json!({}),
        );
    }
    match list_api_keys_data(&state) {
        Ok((items, mgmt)) => ok_no_store(json!({ "ok": true, "items": items, "mgmt": mgmt })),
        Err(err) => internal_error(err),
    }
}

pub async fn delete_api_key(
    State(state): State<SharedState>,
    peer: Option<Extension<ConnectInfo<SocketAddr>>>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Response {
    if id.starts_with("env-key-") {
        return failure(StatusCode::CONFLICT, "env_managed_immutable");
    }
    let (auth_method, actor) = actor_identity(&state, &headers);
    let ip = security::request_client_ip(&state, peer.as_ref(), &headers);
    {
        let db = state.db.lock().expect("database lock poisoned");
        let removed = db
            .conn
            .execute("DELETE FROM api_keys WHERE id = ?1", [&id])
            .unwrap_or(0);
        if removed == 0 {
            return failure(StatusCode::NOT_FOUND, "not_found");
        }
        let _ = db.record_audit_event(
            now(),
            &actor,
            &auth_method,
            &ip,
            "api_key.removed",
            "api_key",
            &id,
            &json!({}),
        );
    }
    match list_api_keys_data(&state) {
        Ok((items, mgmt)) => ok_no_store(json!({ "ok": true, "deleted_id": id, "items": items, "mgmt": mgmt })),
        Err(err) => internal_error(err),
    }
}

#[derive(Deserialize, Default)]
pub struct RotateMgmtInput {
    new_key: Option<String>,
}

pub async fn rotate_management_key(
    State(state): State<SharedState>,
    peer: Option<Extension<ConnectInfo<SocketAddr>>>,
    headers: HeaderMap,
    input: Option<Json<RotateMgmtInput>>,
) -> Response {
    let body = input.map(|j| j.0).unwrap_or_default();
    let next_key = match body.new_key.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        Some(k) => {
            if k.len() < MIN_SECRET_LEN {
                return bad_request_msg("new_key must be at least 16 characters");
            }
            k.to_string()
        }
        None => match mint_token("nm_mgmt_", 24) {
            Ok(t) => t,
            Err(res) => return res,
        },
    };
    let (auth_method, actor) = actor_identity(&state, &headers);
    let ip = security::request_client_ip(&state, peer.as_ref(), &headers);
    let ts = now();
    let epoch = {
        let db = state.db.lock().expect("database lock poisoned");
        match db.rotate_management_key(&next_key, ts) {
            Ok(ep) => {
                let _ = db.record_audit_event(
                    ts,
                    &actor,
                    &auth_method,
                    &ip,
                    "mgmt_key.rotated",
                    "management_key",
                    "runtime",
                    &json!({ "session_epoch": ep }),
                );
                ep
            }
            Err(err) => return internal_error(err),
        }
    };
    let cookie = match security::issue_session_cookie(&state, &headers, "management_key", &actor, &ip) {
        Ok(c) => c,
        Err(err) => return internal_error(err),
    };
    let mut res = ok_no_store(json!({
        "ok": true,
        "secret": next_key,
        "session_epoch": epoch,
        "source": "runtime",
    }));
    res.headers_mut().insert(header::SET_COOKIE, cookie);
    res
}

pub async fn revoke_all_sessions(
    State(state): State<SharedState>,
    peer: Option<Extension<ConnectInfo<SocketAddr>>>,
    headers: HeaderMap,
) -> Response {
    let (auth_method, actor) = actor_identity(&state, &headers);
    let ip = security::request_client_ip(&state, peer.as_ref(), &headers);
    let ts = now();
    let epoch = {
        let db = state.db.lock().expect("database lock poisoned");
        match db.revoke_all_sessions(ts) {
            Ok(ep) => {
                let _ = db.record_audit_event(
                    ts,
                    &actor,
                    &auth_method,
                    &ip,
                    "sessions.revoke_all",
                    "session",
                    "*",
                    &json!({ "session_epoch": ep }),
                );
                ep
            }
            Err(err) => return internal_error(err),
        }
    };
    let mut res = ok_no_store(json!({
        "ok": true,
        "revoked_all": true,
        "session_epoch": epoch,
    }));
    res.headers_mut().insert(
        header::SET_COOKIE,
        security::clear_cookie_header(auth::COOKIE_NAME, "/", state.security.is_cookie_secure(&headers)),
    );
    res
}

pub async fn get_audit_events(State(state): State<SharedState>) -> Response {
    let db = state.db.lock().expect("database lock poisoned");
    let mut st = match db.conn.prepare(
        "SELECT id, at, actor, auth_method, client_ip, action, target_type, target_id, detail_json
         FROM audit_events
         ORDER BY id DESC
         LIMIT 100",
    ) {
        Ok(s) => s,
        Err(err) => return internal_error(err),
    };
    let items: Vec<Value> = match st.query_map([], |r| {
        let detail_raw: String = r.get(8)?;
        let detail: Value = serde_json::from_str(&detail_raw).unwrap_or_else(|_| json!({}));
        Ok(json!({
            "id": r.get::<_, i64>(0)?,
            "at": r.get::<_, u64>(1)?,
            "actor": r.get::<_, String>(2)?,
            "auth_method": r.get::<_, String>(3)?,
            "client_ip": r.get::<_, String>(4)?,
            "action": r.get::<_, String>(5)?,
            "target_type": r.get::<_, String>(6)?,
            "target_id": r.get::<_, String>(7)?,
            "detail": detail,
        }))
    }) {
        Ok(rows) => rows.flatten().collect(),
        Err(err) => return internal_error(err),
    };
    ok_no_store(json!({ "ok": true, "items": items }))
}

// ===========================================================================
// 2. Unified LLM Provider Hub (`ai_providers` + `ai_model_catalog` + bindings)
// ===========================================================================

const HUB_PROVIDER_ID: &str = "lnc-agents-hub";

#[derive(Serialize)]
struct ProviderView {
    id: String,
    name: String,
    kind: String,
    base_url: String,
    capabilities: Vec<String>,
    enabled: bool,
    status: String,
    is_default: bool,
    configured: bool,
    has_secret: bool,
    key_last4: String,
    copy_available: bool,
    source: String,
    env_managed: bool,
    model_count: usize,
    consumer_refs: usize,
    created_at: String,
    updated_at: String,
}

fn hub_key_info(state: &SharedState) -> (bool, String, bool, String) {
    for name in [agent_bridge::SEC_HUB_TOKEN, hub::SECRET_NAME] {
        if let Some(val) = agent_bridge::read_vault_secret(state, name) {
            let (_, last4) = token_prefix_and_last4(&val);
            return (true, last4, true, "runtime".to_string());
        }
    }
    for env_key in ["AGENT_HUB_TOKEN", "LNC_HUB_TOKEN"] {
        if let Ok(val) = std::env::var(env_key) {
            let trimmed = val.trim();
            if !trimmed.is_empty() {
                let (_, last4) = token_prefix_and_last4(trimmed);
                return (true, last4, false, "env".to_string());
            }
        }
    }
    (false, String::new(), false, "unset".to_string())
}

fn sync_custom_providers_from_legacy(db: &db::Database) {
    let _ = db.conn.execute(
        "INSERT OR IGNORE INTO ai_providers (
            id, name, kind, base_url, secret_name, capabilities_json, enabled, status, is_default, created_at, updated_at
         )
         SELECT id, name, 'openai_compat', base_url, secret_name, capabilities_json, enabled,
                CASE WHEN enabled != 0 THEN 'active' ELSE 'revoked' END, 0, created_at, updated_at
         FROM app_agent_custom_providers",
        [],
    );
}

fn build_providers_snapshot(state: &SharedState) -> anyhow::Result<Value> {
    let cfg = state.agent_bridge.config();
    let (hub_has_secret, hub_last4, hub_copy_avail, hub_source) = hub_key_info(state);
    let db = state.db.lock().expect("database lock poisoned");
    sync_custom_providers_from_legacy(&db);

    let hub_url = agent_bridge::effective_hub_base(state, &cfg);
    let hub_disabled = db
        .get_setting("hub_provider_revoked")
        .ok()
        .flatten()
        .as_deref()
        == Some("1");
    let hub_model_count: i64 = db
        .conn
        .query_row(
            "SELECT COUNT(*) FROM app_agent_model_catalog WHERE source IN ('hub', 'hub_bundle', 'llm', 'config')",
            [],
            |r| r.get(0),
        )
        .unwrap_or(0);

    let mut providers = vec![ProviderView {
        id: HUB_PROVIDER_ID.to_string(),
        name: "LNC Agents Hub".to_string(),
        kind: "lnc_hub".to_string(),
        base_url: hub_url.clone(),
        capabilities: vec!["text".into(), "tools".into(), "vision".into()],
        enabled: !hub_disabled,
        status: if hub_disabled { "revoked".into() } else { "active".into() },
        is_default: true,
        configured: !hub_url.is_empty() && hub_has_secret,
        has_secret: hub_has_secret,
        key_last4: hub_last4,
        copy_available: hub_copy_avail,
        env_managed: hub_source == "env",
        source: hub_source,
        model_count: hub_model_count.max(0) as usize,
        consumer_refs: 2,
        created_at: String::new(),
        updated_at: String::new(),
    }];

    let mut st = db.conn.prepare(
        "SELECT id, name, kind, base_url, secret_name, secret_enc, key_last4, copy_available,
                enabled, status, is_default, capabilities_json, created_at, updated_at
         FROM ai_providers
         WHERE id != 'lnc-agents-hub'
         ORDER BY created_at DESC, name ASC",
    )?;
    let raw_rows: Vec<(
        String,
        String,
        String,
        String,
        String,
        String,
        String,
        i64,
        i64,
        String,
        i64,
        String,
        String,
        String,
    )> = st
        .query_map([], |r| {
            Ok((
                r.get(0)?,
                r.get(1)?,
                r.get(2)?,
                r.get(3)?,
                r.get(4)?,
                r.get(5)?,
                r.get(6)?,
                r.get(7)?,
                r.get(8)?,
                r.get(9)?,
                r.get(10)?,
                r.get(11)?,
                r.get(12)?,
                r.get(13)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    drop(st);

    for (
        id,
        name,
        kind,
        base_url,
        secret_name,
        secret_enc,
        key_last4,
        copy_flag,
        enabled,
        status,
        is_default,
        caps_raw,
        created_at,
        updated_at,
    ) in raw_rows
    {
        let has_legacy_secret = !secret_name.is_empty()
            && db
                .conn
                .query_row("SELECT 1 FROM secrets WHERE name = ?1 LIMIT 1", [&secret_name], |_| Ok(()))
                .optional()?
                .is_some();
        let has_secret = !secret_enc.is_empty() || has_legacy_secret;
        let copy_available = state.secrets.is_some()
            && ((copy_flag != 0 && !secret_enc.is_empty()) || has_legacy_secret);
        let mcount: i64 = db
            .conn
            .query_row(
                "SELECT COUNT(*) FROM ai_model_catalog WHERE provider_id = ?1",
                [&id],
                |r| r.get(0),
            )
            .unwrap_or(0);
        let refs: i64 = db
            .conn
            .query_row(
                "SELECT COUNT(*) FROM ai_consumer_bindings WHERE provider_id = ?1",
                [&id],
                |r| r.get(0),
            )
            .unwrap_or(0);
        providers.push(ProviderView {
            id,
            name,
            kind,
            base_url,
            capabilities: serde_json::from_str(&caps_raw).unwrap_or_else(|_| vec!["text".into()]),
            enabled: enabled != 0 && status == "active",
            status,
            is_default: is_default != 0,
            configured: true,
            has_secret,
            key_last4,
            copy_available,
            source: "runtime".into(),
            env_managed: false,
            model_count: mcount.max(0) as usize,
            consumer_refs: refs.max(0) as usize,
            created_at,
            updated_at,
        });
    }

    let mut bst = db.conn.prepare(
        "SELECT role_key, COALESCE(provider_id, ''), model_id, updated_at FROM ai_consumer_bindings ORDER BY role_key",
    )?;
    let bindings: Vec<Value> = bst
        .query_map([], |r| {
            Ok(json!({
                "role_key": r.get::<_, String>(0)?,
                "provider_id": r.get::<_, String>(1)?,
                "model_id": r.get::<_, String>(2)?,
                "updated_at": r.get::<_, String>(3)?,
            }))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    Ok(json!({
        "ok": true,
        "items": providers,
        "roles": {
            "default_smart_model": agent_bridge::resolve_smart_model(&cfg),
            "fast_background_model": cfg.fast_background_model,
            "resolved_fast_model": agent_bridge::resolve_fast_model(&cfg),
        },
        "bindings": bindings,
    }))
}

pub async fn get_providers(State(state): State<SharedState>) -> Response {
    match build_providers_snapshot(&state) {
        Ok(v) => ok_no_store(v),
        Err(err) => internal_error(err),
    }
}

#[derive(Deserialize)]
pub struct UpsertProviderInput {
    id: Option<String>,
    name: Option<String>,
    kind: Option<String>,
    base_url: Option<String>,
    api_key: Option<String>,
    capabilities: Option<Vec<String>>,
    enabled: Option<bool>,
}

pub async fn post_provider(
    State(state): State<SharedState>,
    peer: Option<Extension<ConnectInfo<SocketAddr>>>,
    headers: HeaderMap,
    Json(input): Json<UpsertProviderInput>,
) -> Response {
    let target_id = input
        .id
        .as_deref()
        .map(|s| sanitize_slug(s, ""))
        .filter(|s| !s.is_empty());
    upsert_provider_inner(state, peer, headers, target_id, input).await
}

pub async fn put_provider(
    State(state): State<SharedState>,
    peer: Option<Extension<ConnectInfo<SocketAddr>>>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(input): Json<UpsertProviderInput>,
) -> Response {
    upsert_provider_inner(state, peer, headers, Some(id), input).await
}

async fn upsert_provider_inner(
    state: SharedState,
    peer: Option<Extension<ConnectInfo<SocketAddr>>>,
    headers: HeaderMap,
    id_opt: Option<String>,
    input: UpsertProviderInput,
) -> Response {
    let ts_str = Utc::now().to_rfc3339();
    let (auth_method, actor) = actor_identity(&state, &headers);
    let ip = security::request_client_ip(&state, peer.as_ref(), &headers);

    // Built-in LNC Agents Hub hero card update
    if id_opt.as_deref() == Some(HUB_PROVIDER_ID) {
        let mut cfg = state.agent_bridge.config();
        if let Some(ref url) = input.base_url {
            let t = url.trim();
            if !t.is_empty() {
                match validate_outbound_url(&state, t) {
                    Ok(clean) => cfg.hub_url = clean,
                    Err(res) => return res,
                }
            } else {
                cfg.hub_url.clear();
            }
        }
        if let Err(err) = agent_bridge::persist_bridge_config_row(&state, cfg) {
            return internal_error(err);
        }
        if !is_blank_or_mask(input.api_key.as_deref()) {
            let raw_key = input.api_key.as_deref().unwrap_or("").trim();
            let Some(vault) = state.secrets.as_ref() else {
                return failure(
                    StatusCode::SERVICE_UNAVAILABLE,
                    "secret_store_not_configured",
                );
            };
            let db = state.db.lock().expect("database lock poisoned");
            if let Err(err) = secrets::set(&db, vault, agent_bridge::SEC_HUB_TOKEN, raw_key)
                .and_then(|_| secrets::set(&db, vault, hub::SECRET_NAME, raw_key))
            {
                return failure(StatusCode::BAD_REQUEST, err.code());
            }
        }
        if let Some(en) = input.enabled {
            let db = state.db.lock().expect("database lock poisoned");
            let _ = db.set_setting("hub_provider_revoked", if en { "0" } else { "1" });
        }
        {
            let db = state.db.lock().expect("database lock poisoned");
            let _ = db.record_audit_event(
                now(),
                &actor,
                &auth_method,
                &ip,
                "provider.updated",
                "provider",
                HUB_PROVIDER_ID,
                &json!({}),
            );
        }
        return match build_providers_snapshot(&state) {
            Ok(v) => ok_no_store(v),
            Err(err) => internal_error(err),
        };
    }

    let id = id_opt.unwrap_or_else(|| {
        let base = input.name.as_deref().unwrap_or("provider");
        let suffix = agent_bridge::new_id("p").replace('_', "-");
        sanitize_slug(&format!("{base}-{}", &suffix[suffix.len().saturating_sub(6)..]), "provider")
    });
    let clean_url = match input.base_url.as_deref().map(str::trim) {
        Some(u) if !u.is_empty() => match validate_outbound_url(&state, u) {
            Ok(c) => Some(c),
            Err(res) => return res,
        },
        _ => None,
    };

    {
        let db = state.db.lock().expect("database lock poisoned");
        sync_custom_providers_from_legacy(&db);
        let existing: Option<(String, String, String, String, String, String, i64, i64, String)> = db
            .conn
            .query_row(
                "SELECT name, kind, base_url, secret_name, secret_enc, key_last4, copy_available, enabled, capabilities_json
                 FROM ai_providers WHERE id = ?1",
                [&id],
                |r| {
                    Ok((
                        r.get(0)?,
                        r.get(1)?,
                        r.get(2)?,
                        r.get(3)?,
                        r.get(4)?,
                        r.get(5)?,
                        r.get(6)?,
                        r.get(7)?,
                        r.get(8)?,
                    ))
                },
            )
            .optional()
            .ok()
            .flatten();

        let (
            cur_name,
            cur_kind,
            cur_url,
            cur_sname,
            cur_enc,
            cur_last4,
            cur_copy,
            cur_enabled,
            cur_caps,
        ) = existing.unwrap_or_else(|| {
            (
                String::new(),
                "openai_compat".to_string(),
                String::new(),
                format!("provider-{id}"),
                String::new(),
                String::new(),
                0,
                1,
                "[\"text\"]".to_string(),
            )
        });

        let name = input
            .name
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(ToOwned::to_owned)
            .unwrap_or(cur_name);
        if name.is_empty() {
            return bad_request_msg("provider name is required");
        }
        let final_url = clean_url.unwrap_or(cur_url);
        if final_url.is_empty() {
            return bad_request_msg("provider base_url is required");
        }
        let kind = input
            .kind
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .unwrap_or(&cur_kind)
            .to_string();
        let caps_json = input
            .capabilities
            .as_ref()
            .filter(|c| !c.is_empty())
            .and_then(|c| serde_json::to_string(c).ok())
            .unwrap_or(cur_caps);
        let enabled = input.enabled.unwrap_or(cur_enabled != 0);
        let status = if enabled { "active" } else { "revoked" };

        let secret_name = if cur_sname.is_empty() {
            let candidate = format!("provider-{id}");
            candidate.chars().take(63).collect::<String>().trim_end_matches('-').to_string()
        } else {
            cur_sname
        };

        let (secret_enc, key_last4, copy_available) = if !is_blank_or_mask(input.api_key.as_deref()) {
            let raw_key = input.api_key.as_deref().unwrap_or("").trim();
            let Some(vault) = state.secrets.as_ref() else {
                return failure(
                    StatusCode::SERVICE_UNAVAILABLE,
                    "secret_store_not_configured",
                );
            };
            let enc = match vault.seal_envelope(&format!("provider:{id}"), raw_key) {
                Ok(e) => e,
                Err(err) => return failure(StatusCode::BAD_REQUEST, err.code()),
            };
            let _ = secrets::set(&db, vault, &secret_name, raw_key);
            let (_, last4) = token_prefix_and_last4(raw_key);
            (enc, last4, 1_i64)
        } else {
            // omitBlankSecrets: preserve existing sealed ciphertext cleanly
            (cur_enc, cur_last4, cur_copy)
        };

        if let Err(err) = db.conn.execute(
            "INSERT INTO ai_providers
                (id, name, kind, base_url, secret_name, secret_enc, key_last4, copy_available,
                 enabled, status, is_default, capabilities_json, created_at, updated_at, revoked_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, 0, ?11, ?12, ?12, '')
             ON CONFLICT(id) DO UPDATE SET
                name = excluded.name,
                kind = excluded.kind,
                base_url = excluded.base_url,
                secret_name = excluded.secret_name,
                secret_enc = excluded.secret_enc,
                key_last4 = excluded.key_last4,
                copy_available = excluded.copy_available,
                enabled = excluded.enabled,
                status = excluded.status,
                capabilities_json = excluded.capabilities_json,
                updated_at = excluded.updated_at",
            params![
                id,
                name,
                kind,
                final_url,
                secret_name,
                secret_enc,
                key_last4,
                copy_available,
                enabled as i64,
                status,
                caps_json,
                ts_str
            ],
        ) {
            return internal_error(err);
        }

        let _ = db.conn.execute(
            "INSERT INTO app_agent_custom_providers
                (id, org_id, name, base_url, secret_name, capabilities_json, enabled, created_at, updated_at)
             VALUES (?1, 'default', ?2, ?3, ?4, ?5, ?6, ?7, ?7)
             ON CONFLICT(id) DO UPDATE SET
                name = excluded.name,
                base_url = excluded.base_url,
                secret_name = excluded.secret_name,
                capabilities_json = excluded.capabilities_json,
                enabled = excluded.enabled,
                updated_at = excluded.updated_at",
            params![id, name, final_url, secret_name, caps_json, enabled as i64, ts_str],
        );

        let _ = db.record_audit_event(
            now(),
            &actor,
            &auth_method,
            &ip,
            "provider.upserted",
            "provider",
            &id,
            &json!({ "name": name }),
        );
    }

    match build_providers_snapshot(&state) {
        Ok(mut v) => {
            if let Some(obj) = v.as_object_mut() {
                obj.insert("id".into(), json!(id));
            }
            ok_no_store(v)
        }
        Err(err) => internal_error(err),
    }
}

pub async fn copy_provider_secret(
    State(state): State<SharedState>,
    peer: Option<Extension<ConnectInfo<SocketAddr>>>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Response {
    let ip = match check_copy_rate_limit(&state, peer.as_ref(), &headers) {
        Ok(ip) => ip,
        Err(res) => return res,
    };
    let (auth_method, actor) = actor_identity(&state, &headers);
    let plain = if id == HUB_PROVIDER_ID {
        let from_vault = agent_bridge::read_vault_secret(&state, agent_bridge::SEC_HUB_TOKEN)
            .or_else(|| agent_bridge::read_vault_secret(&state, hub::SECRET_NAME));
        match from_vault {
            Some(v) => v,
            None => return failure(StatusCode::CONFLICT, "copy_unavailable_rotate_required"),
        }
    } else {
        let row: Option<(String, String)> = {
            let db = state.db.lock().expect("database lock poisoned");
            db.conn
                .query_row(
                    "SELECT secret_enc, secret_name FROM ai_providers WHERE id = ?1",
                    [&id],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
                .optional()
                .ok()
                .flatten()
        };
        let Some((enc, sname)) = row else {
            return failure(StatusCode::NOT_FOUND, "not_found");
        };
        match secret_for_copy(&state, &format!("provider:{id}"), &enc, Some(&sname)) {
            Ok(p) => p,
            Err(res) => return res,
        }
    };
    {
        let db = state.db.lock().expect("database lock poisoned");
        let _ = db.record_audit_event(
            now(),
            &actor,
            &auth_method,
            &ip,
            "secret.copy",
            "provider",
            &id,
            &json!({ "copied_field": "api_key" }),
        );
    }
    ok_no_store(json!({ "ok": true, "secret": plain, "copied_field": "api_key" }))
}

pub async fn rotate_provider_secret(
    State(state): State<SharedState>,
    peer: Option<Extension<ConnectInfo<SocketAddr>>>,
    headers: HeaderMap,
    Path(id): Path<String>,
    input: Option<Json<RotateSecretInput>>,
) -> Response {
    let body = input.map(|j| j.0).unwrap_or_default();
    let supplied = body
        .api_key
        .or(body.custom_secret)
        .or(body.secret)
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    let new_key = match supplied {
        Some(k) => {
            if k.len() < 8 {
                return bad_request_msg("provider api_key must be at least 8 characters");
            }
            k
        }
        None => match mint_token("sk_lnc_", 24) {
            Ok(t) => t,
            Err(res) => return res,
        },
    };
    let Some(vault) = state.secrets.as_ref() else {
        return failure(
            StatusCode::SERVICE_UNAVAILABLE,
            "secret_store_not_configured",
        );
    };
    let (auth_method, actor) = actor_identity(&state, &headers);
    let ip = security::request_client_ip(&state, peer.as_ref(), &headers);
    let ts_str = Utc::now().to_rfc3339();
    let (_, last4) = token_prefix_and_last4(&new_key);

    if id == HUB_PROVIDER_ID {
        let db = state.db.lock().expect("database lock poisoned");
        if let Err(err) = secrets::set(&db, vault, agent_bridge::SEC_HUB_TOKEN, &new_key)
            .and_then(|_| secrets::set(&db, vault, hub::SECRET_NAME, &new_key))
        {
            return failure(StatusCode::BAD_REQUEST, err.code());
        }
        let _ = db.set_setting("hub_provider_revoked", "0");
        let _ = db.record_audit_event(
            now(),
            &actor,
            &auth_method,
            &ip,
            "provider.rotated",
            "provider",
            &id,
            &json!({ "last4": last4 }),
        );
    } else {
        let db = state.db.lock().expect("database lock poisoned");
        let sname: Option<String> = db
            .conn
            .query_row("SELECT secret_name FROM ai_providers WHERE id = ?1", [&id], |r| r.get(0))
            .optional()
            .ok()
            .flatten();
        let Some(mut secret_name) = sname else {
            return failure(StatusCode::NOT_FOUND, "not_found");
        };
        if secret_name.is_empty() {
            secret_name = format!("provider-{id}");
        }
        let enc = match vault.seal_envelope(&format!("provider:{id}"), &new_key) {
            Ok(e) => e,
            Err(err) => return failure(StatusCode::BAD_REQUEST, err.code()),
        };
        let _ = secrets::set(&db, vault, &secret_name, &new_key);
        let _ = db.conn.execute(
            "UPDATE ai_providers
             SET secret_name = ?2, secret_enc = ?3, key_last4 = ?4, copy_available = 1,
                 enabled = 1, status = 'active', revoked_at = '', updated_at = ?5
             WHERE id = ?1",
            params![id, secret_name, enc, last4, ts_str],
        );
        let _ = db.conn.execute(
            "UPDATE app_agent_custom_providers SET enabled = 1, updated_at = ?2 WHERE id = ?1",
            params![id, ts_str],
        );
        let _ = db.record_audit_event(
            now(),
            &actor,
            &auth_method,
            &ip,
            "provider.rotated",
            "provider",
            &id,
            &json!({ "last4": last4 }),
        );
    }

    match build_providers_snapshot(&state) {
        Ok(mut v) => {
            if let Some(obj) = v.as_object_mut() {
                obj.insert("secret".into(), json!(new_key));
            }
            ok_no_store(v)
        }
        Err(err) => internal_error(err),
    }
}

pub async fn revoke_provider(
    State(state): State<SharedState>,
    peer: Option<Extension<ConnectInfo<SocketAddr>>>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Response {
    let ts_str = Utc::now().to_rfc3339();
    let (auth_method, actor) = actor_identity(&state, &headers);
    let ip = security::request_client_ip(&state, peer.as_ref(), &headers);

    {
        let db = state.db.lock().expect("database lock poisoned");
        if id == HUB_PROVIDER_ID {
            let _ = db.set_setting("hub_provider_revoked", "1");
        } else {
            let changed = db
                .conn
                .execute(
                    "UPDATE ai_providers SET enabled = 0, status = 'revoked', revoked_at = ?2, updated_at = ?2 WHERE id = ?1",
                    params![id, ts_str],
                )
                .unwrap_or(0);
            if changed == 0 {
                return failure(StatusCode::NOT_FOUND, "not_found");
            }
            let _ = db.conn.execute(
                "UPDATE app_agent_custom_providers SET enabled = 0, updated_at = ?2 WHERE id = ?1",
                params![id, ts_str],
            );
            let _ = db.conn.execute(
                "UPDATE ai_model_catalog SET enabled = 0 WHERE provider_id = ?1",
                [&id],
            );
            let _ = db.conn.execute(
                "UPDATE app_agent_model_catalog SET enabled = 0 WHERE source = ?1",
                [format!("custom:{id}")],
            );
        }
        let _ = db.record_audit_event(
            now(),
            &actor,
            &auth_method,
            &ip,
            "provider.revoked",
            "provider",
            &id,
            &json!({}),
        );
    }

    match build_providers_snapshot(&state) {
        Ok(mut v) => {
            if let Some(obj) = v.as_object_mut() {
                obj.insert("id".into(), json!(id));
            }
            ok_no_store(v)
        }
        Err(err) => internal_error(err),
    }
}

pub async fn delete_provider(
    State(state): State<SharedState>,
    peer: Option<Extension<ConnectInfo<SocketAddr>>>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Response {
    if id == HUB_PROVIDER_ID {
        return failure(StatusCode::CONFLICT, "builtin_hub_cannot_be_removed");
    }
    let (auth_method, actor) = actor_identity(&state, &headers);
    let ip = security::request_client_ip(&state, peer.as_ref(), &headers);
    let sname: Option<String>;
    let affected_bindings: i64;
    {
        let db = state.db.lock().expect("database lock poisoned");
        sname = db
            .conn
            .query_row("SELECT secret_name FROM ai_providers WHERE id = ?1", [&id], |r| r.get(0))
            .optional()
            .ok()
            .flatten();
        affected_bindings = db
            .conn
            .query_row(
                "SELECT COUNT(*) FROM ai_consumer_bindings WHERE provider_id = ?1",
                [&id],
                |r| r.get(0),
            )
            .unwrap_or(0);
        let removed = db
            .conn
            .execute("DELETE FROM ai_providers WHERE id = ?1", [&id])
            .unwrap_or(0);
        if removed == 0 {
            return failure(StatusCode::NOT_FOUND, "not_found");
        }
        let _ = db
            .conn
            .execute("DELETE FROM app_agent_custom_providers WHERE id = ?1", [&id]);
        let _ = db.conn.execute(
            "DELETE FROM app_agent_model_catalog WHERE source = ?1",
            [format!("custom:{id}")],
        );
        if let Some(ref sn) = sname
            && !sn.is_empty()
        {
            let _ = secrets::delete(&db, sn);
        }
        let _ = db.record_audit_event(
            now(),
            &actor,
            &auth_method,
            &ip,
            "provider.removed",
            "provider",
            &id,
            &json!({ "affected_bindings": affected_bindings }),
        );
    }
    match build_providers_snapshot(&state) {
        Ok(mut v) => {
            if let Some(obj) = v.as_object_mut() {
                obj.insert("deleted_id".into(), json!(id));
                obj.insert("affected_bindings".into(), json!(affected_bindings));
            }
            ok_no_store(v)
        }
        Err(err) => internal_error(err),
    }
}

fn parse_models_payload(val: &Value, default_provider: &str) -> Vec<(String, String, Vec<String>, Option<i64>)> {
    let mut out = Vec::new();
    let arr = val
        .get("data")
        .and_then(Value::as_array)
        .or_else(|| val.get("models").and_then(Value::as_array))
        .or_else(|| val.as_array());
    let Some(items) = arr else {
        return out;
    };
    for item in items {
        let id = item
            .get("id")
            .or_else(|| item.get("model_id"))
            .or_else(|| item.get("name"))
            .and_then(Value::as_str)
            .map(str::trim)
            .unwrap_or("");
        if id.is_empty() {
            continue;
        }
        let display = item
            .get("display_name")
            .or_else(|| item.get("name"))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .unwrap_or(id)
            .to_string();
        let _ = default_provider;
        let caps = item
            .get("capabilities")
            .and_then(Value::as_array)
            .map(|c| {
                c.iter()
                    .filter_map(Value::as_str)
                    .map(str::to_string)
                    .collect::<Vec<_>>()
            })
            .filter(|c: &Vec<String>| !c.is_empty())
            .unwrap_or_else(|| vec!["text".into()]);
        let ctx_win = item.get("context_window").and_then(Value::as_i64);
        out.push((id.to_string(), display, caps, ctx_win));
    }
    out
}

pub async fn fetch_provider_models(
    State(state): State<SharedState>,
    Path(id): Path<String>,
) -> Response {
    let (pname, base_url, api_key, source_tag) = if id == HUB_PROVIDER_ID {
        let cfg = state.agent_bridge.config();
        let base = agent_bridge::effective_hub_base(&state, &cfg);
        if base.is_empty() {
            return bad_request_msg("LNC Agents Hub URL is not configured");
        }
        (
            "hub".to_string(),
            base,
            agent_bridge::resolve_hub_token(&state),
            "hub".to_string(),
        )
    } else {
        let row: Option<(String, String, String, String)> = {
            let db = state.db.lock().expect("database lock poisoned");
            db.conn
                .query_row(
                    "SELECT name, base_url, secret_enc, secret_name FROM ai_providers WHERE id = ?1",
                    [&id],
                    |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
                )
                .optional()
                .ok()
                .flatten()
        };
        let Some((name, base_url, enc, sname)) = row else {
            return failure(StatusCode::NOT_FOUND, "not_found");
        };
        let key = secret_for_copy(&state, &format!("provider:{id}"), &enc, Some(&sname))
            .unwrap_or_default();
        (name, base_url, key, format!("custom:{id}"))
    };

    let url = agent_bridge::models_list_url(&base_url);
    let parsed_host = Url::parse(&base_url)
        .ok()
        .and_then(|u| u.host_str().map(|s| s.to_ascii_lowercase()))
        .into_iter()
        .collect::<Vec<_>>();
    let _ = parsed_host;
    // Enforce outbound SSRF policy: private IPs require explicit allowlist in env/bridge.
    let fetcher = outbound_fetcher(&state, &[]);
    let auth_header = (!api_key.trim().is_empty()).then(|| format!("Bearer {}", api_key.trim()));
    let mut hdrs: Vec<(&str, &str)> = vec![("X-LNC-App-Id", "lnc-newsman")];
    if let Some(ref a) = auth_header {
        hdrs.push(("Authorization", a.as_str()));
    }
    let outcome = match fetcher.get_with_headers(&url, &hdrs).await {
        Ok(o) => o,
        Err(err) => {
            return no_store_response(
                StatusCode::BAD_GATEWAY,
                json!({ "ok": false, "error": "provider_probe_failed", "detail": err.code() }),
            );
        }
    };
    let val = match agent_bridge::outcome_json(outcome) {
        Ok(v) => v,
        Err(err) => {
            return no_store_response(
                StatusCode::BAD_GATEWAY,
                json!({ "ok": false, "error": "provider_probe_failed", "detail": err.to_string() }),
            );
        }
    };
    let models = parse_models_payload(&val, &pname);
    let ts_str = Utc::now().to_rfc3339();
    {
        let db = state.db.lock().expect("database lock poisoned");
        let had_enabled: i64 = db
            .conn
            .query_row(
                "SELECT COUNT(*) FROM app_agent_model_catalog WHERE enabled = 1",
                [],
                |r| r.get(0),
            )
            .unwrap_or(0);
        let init_en = if had_enabled == 0 { 1_i64 } else { 0_i64 };
        for (model_id, display, caps, ctx_win) in &models {
            let caps_json = serde_json::to_string(caps).unwrap_or_else(|_| "[\"text\"]".into());
            let _ = db.conn.execute(
                "INSERT INTO ai_model_catalog
                    (provider_id, model_id, display_name, capabilities_json, context_window, enabled, last_fetched_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
                 ON CONFLICT(provider_id, model_id) DO UPDATE SET
                    display_name = excluded.display_name,
                    capabilities_json = excluded.capabilities_json,
                    context_window = COALESCE(excluded.context_window, ai_model_catalog.context_window),
                    last_fetched_at = excluded.last_fetched_at",
                params![id, model_id, display, caps_json, ctx_win, init_en, ts_str],
            );
            let _ = db.conn.execute(
                "INSERT INTO app_agent_model_catalog
                    (model_id, display_name, provider, capabilities_json, context_window, enabled, source, last_fetched_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
                 ON CONFLICT(model_id) DO UPDATE SET
                    display_name = excluded.display_name,
                    provider = excluded.provider,
                    capabilities_json = excluded.capabilities_json,
                    context_window = COALESCE(excluded.context_window, app_agent_model_catalog.context_window),
                    source = excluded.source,
                    last_fetched_at = excluded.last_fetched_at",
                params![model_id, display, pname, caps_json, ctx_win, init_en, source_tag, ts_str],
            );
        }
    }

    ok_no_store(json!({
        "ok": true,
        "provider_id": id,
        "fetched_count": models.len(),
        "models": models.iter().map(|m| json!({"model_id": m.0, "display_name": m.1, "capabilities": m.2, "context_window": m.3})).collect::<Vec<_>>(),
    }))
}

#[derive(Deserialize)]
pub struct ToggleCatalogItem {
    model_id: String,
    enabled: bool,
}

#[derive(Deserialize)]
pub struct PatchModelsCatalogInput {
    items: Vec<ToggleCatalogItem>,
}

pub async fn patch_models_catalog(
    State(state): State<SharedState>,
    Json(input): Json<PatchModelsCatalogInput>,
) -> Response {
    {
        let db = state.db.lock().expect("database lock poisoned");
        for item in input.items {
            let trimmed = item.model_id.trim();
            if trimmed.is_empty() {
                continue;
            }
            let _ = db.conn.execute(
                "UPDATE app_agent_model_catalog SET enabled = ?2 WHERE model_id = ?1",
                params![trimmed, item.enabled as i64],
            );
            let _ = db.conn.execute(
                "UPDATE ai_model_catalog SET enabled = ?2 WHERE model_id = ?1",
                params![trimmed, item.enabled as i64],
            );
        }
    }
    crate::agent_v2::get_models_catalog(State(state)).await
}

#[derive(Deserialize)]
pub struct ModelBindingsPatch {
    default_smart_model: Option<String>,
    fast_background_model: Option<String>,
}

pub async fn put_models_bindings(
    State(state): State<SharedState>,
    Json(input): Json<ModelBindingsPatch>,
) -> Response {
    let mut cfg = state.agent_bridge.config();
    if let Some(ref smart) = input.default_smart_model {
        let t = smart.trim().to_string();
        cfg.default_smart_model = t.clone();
        if !t.is_empty() {
            cfg.default_model = t;
        }
    }
    if let Some(ref fast) = input.fast_background_model {
        cfg.fast_background_model = fast.trim().to_string();
    }
    let saved = match agent_bridge::persist_bridge_config_row(&state, cfg) {
        Ok(c) => c,
        Err(err) => return internal_error(err),
    };
    let ts_str = Utc::now().to_rfc3339();
    {
        let db = state.db.lock().expect("database lock poisoned");
        let _ = db.conn.execute(
            "INSERT INTO ai_consumer_bindings(role_key, provider_id, model_id, updated_at)
             VALUES ('default_smart_model', NULL, ?1, ?2)
             ON CONFLICT(role_key) DO UPDATE SET model_id = excluded.model_id, updated_at = excluded.updated_at",
            params![agent_bridge::resolve_smart_model(&saved), ts_str],
        );
        let _ = db.conn.execute(
            "INSERT INTO ai_consumer_bindings(role_key, provider_id, model_id, updated_at)
             VALUES ('fast_background_model', NULL, ?1, ?2)
             ON CONFLICT(role_key) DO UPDATE SET model_id = excluded.model_id, updated_at = excluded.updated_at",
            params![saved.fast_background_model, ts_str],
        );
    }
    ok_no_store(json!({
        "ok": true,
        "default_smart_model": agent_bridge::resolve_smart_model(&saved),
        "fast_background_model": saved.fast_background_model,
        "resolved_fast_model": agent_bridge::resolve_fast_model(&saved),
    }))
}

// ===========================================================================
// 3. MCP Connectors (Inbound `/mcp` + Custom Outbound MCP Servers)
// ===========================================================================

const INBOUND_MCP_ID: &str = "inbound-mcp";

fn source_label(src: connector::Source) -> &'static str {
    match src {
        connector::Source::Env => "env",
        connector::Source::Admin => "runtime",
        connector::Source::Unset => "unset",
    }
}

fn public_mcp_url(state: &SharedState, headers: &HeaderMap) -> String {
    if let Some(ref base) = state.security.public_base_url {
        return format!("{}/mcp", base.trim_end_matches('/'));
    }
    if let Some(host) = headers.get(header::HOST).and_then(|v| v.to_str().ok()) {
        let proto = headers
            .get("x-forwarded-proto")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.split(',').next())
            .map(str::trim)
            .filter(|v| *v == "https" || *v == "http")
            .unwrap_or("https");
        return format!("{proto}://{host}/mcp");
    }
    "/mcp".to_string()
}

fn build_mcp_connectors_snapshot(state: &SharedState, headers: &HeaderMap) -> anyhow::Result<Value> {
    let resolved = connector::resolve(state);
    let bearer_source = source_label(resolved.bearer_source);
    let secret_source = source_label(resolved.secret_source);
    let env_managed = resolved.bearer_source == connector::Source::Env
        && resolved.secret_source == connector::Source::Env;
    let db = state.db.lock().expect("database lock poisoned");
    let revoked = db
        .get_setting("connector_revoked")
        .ok()
        .flatten()
        .as_deref()
        == Some("1");
    let paired = resolved.bearer.is_some() && resolved.delegation_secret.is_some();
    let bearer_copy_avail = state.secrets.is_some()
        && resolved.bearer_source == connector::Source::Admin
        && secrets::get(&db, state.secrets.as_ref().expect("checked"), connector::BEARER_SECRET).is_ok();
    let delegation_copy_avail = state.secrets.is_some()
        && resolved.secret_source == connector::Source::Admin
        && secrets::get(&db, state.secrets.as_ref().expect("checked"), connector::DELEGATION_SECRET).is_ok();

    let mut items = vec![json!({
        "id": INBOUND_MCP_ID,
        "name": "Inbound /mcp Endpoint (Newsman MCP)",
        "direction": "inbound",
        "endpoint_url": public_mcp_url(state, headers),
        "audience": resolved.audience,
        "scopes": resolved.max_scopes,
        "enabled": paired && !revoked,
        "status": if revoked { "revoked" } else if paired { "active" } else { "unpaired" },
        "bearer_configured": resolved.bearer.is_some(),
        "bearer_source": bearer_source,
        "bearer_copy_available": bearer_copy_avail,
        "delegation_configured": resolved.delegation_secret.is_some(),
        "delegation_source": secret_source,
        "delegation_copy_available": delegation_copy_avail,
        "copy_available": bearer_copy_avail || delegation_copy_avail,
        "env_managed": env_managed,
        "source": if env_managed { "env" } else if bearer_source == "runtime" || secret_source == "runtime" { "runtime" } else { "unset" },
        "tool_count": mcp::tools().len(),
    })];

    let mut st = db.conn.prepare(
        "SELECT id, name, direction, endpoint_url, secret_enc, key_last4, copy_available,
                scopes_json, audience, enabled, status, created_at, updated_at, revoked_at
         FROM mcp_connectors
         ORDER BY created_at DESC, name ASC",
    )?;
    let rows = st.query_map([], |r| {
        let scopes_raw: String = r.get(7)?;
        let secret_enc: String = r.get(4)?;
        let copy_flag: i64 = r.get(6)?;
        let enabled_flag: i64 = r.get(9)?;
        let status: String = r.get(10)?;
        Ok(json!({
            "id": r.get::<_, String>(0)?,
            "name": r.get::<_, String>(1)?,
            "direction": r.get::<_, String>(2)?,
            "endpoint_url": r.get::<_, String>(3)?,
            "has_secret": !secret_enc.is_empty(),
            "key_last4": r.get::<_, String>(5)?,
            "copy_available": copy_flag != 0 && !secret_enc.is_empty() && state.secrets.is_some(),
            "scopes": serde_json::from_str::<Vec<String>>(&scopes_raw).unwrap_or_default(),
            "audience": r.get::<_, String>(8)?,
            "enabled": enabled_flag != 0 && status == "active",
            "status": status,
            "env_managed": false,
            "source": "runtime",
            "created_at": r.get::<_, u64>(11)?,
            "updated_at": r.get::<_, u64>(12)?,
            "revoked_at": r.get::<_, u64>(13)?,
        }))
    })?;
    for row in rows.flatten() {
        items.push(row);
    }

    Ok(json!({ "ok": true, "items": items }))
}

pub async fn get_mcp_connectors(State(state): State<SharedState>, headers: HeaderMap) -> Response {
    match build_mcp_connectors_snapshot(&state, &headers) {
        Ok(v) => ok_no_store(v),
        Err(err) => internal_error(err),
    }
}

#[derive(Deserialize)]
pub struct UpsertMcpConnectorInput {
    id: Option<String>,
    name: Option<String>,
    endpoint_url: Option<String>,
    audience: Option<String>,
    scopes: Option<Vec<String>>,
    bearer_token: Option<String>,
    delegation_secret: Option<String>,
    secret: Option<String>,
    enabled: Option<bool>,
}

pub async fn post_mcp_connector(
    State(state): State<SharedState>,
    peer: Option<Extension<ConnectInfo<SocketAddr>>>,
    headers: HeaderMap,
    Json(input): Json<UpsertMcpConnectorInput>,
) -> Response {
    let id = input
        .id
        .as_deref()
        .map(|s| sanitize_slug(s, ""))
        .filter(|s| !s.is_empty());
    upsert_mcp_connector_inner(state, peer, headers, id, input).await
}

pub async fn put_mcp_connector(
    State(state): State<SharedState>,
    peer: Option<Extension<ConnectInfo<SocketAddr>>>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(input): Json<UpsertMcpConnectorInput>,
) -> Response {
    upsert_mcp_connector_inner(state, peer, headers, Some(id), input).await
}

async fn upsert_mcp_connector_inner(
    state: SharedState,
    peer: Option<Extension<ConnectInfo<SocketAddr>>>,
    headers: HeaderMap,
    id_opt: Option<String>,
    input: UpsertMcpConnectorInput,
) -> Response {
    let (auth_method, actor) = actor_identity(&state, &headers);
    let ip = security::request_client_ip(&state, peer.as_ref(), &headers);

    if id_opt.as_deref() == Some(INBOUND_MCP_ID) {
        let db = state.db.lock().expect("database lock poisoned");
        if let Some(ref aud) = input.audience {
            let trimmed = aud.trim();
            if !trimmed.is_empty()
                && (trimmed.len() > 64
                    || !trimmed
                        .chars()
                        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-')))
            {
                return bad_request_msg("invalid connector audience");
            }
            let _ = db.set_setting(connector::AUDIENCE_KEY, trimmed);
        }
        if let Some(ref scopes) = input.scopes {
            if scopes.iter().any(|s| s != "newsman:read" && s != "newsman:write") {
                return bad_request_msg("unsupported connector scope");
            }
            let _ = db.set_setting(connector::MAX_SCOPES_KEY, &scopes.join(","));
        }
        if !is_blank_or_mask(input.bearer_token.as_deref()) {
            if state.connector.bearer.is_some() {
                return failure(StatusCode::CONFLICT, "env_managed_immutable");
            }
            let Some(vault) = state.secrets.as_ref() else {
                return failure(
                    StatusCode::SERVICE_UNAVAILABLE,
                    "secret_store_not_configured",
                );
            };
            if let Err(err) = secrets::set(
                &db,
                vault,
                connector::BEARER_SECRET,
                input.bearer_token.as_deref().unwrap_or("").trim(),
            ) {
                return failure(StatusCode::BAD_REQUEST, err.code());
            }
        }
        if !is_blank_or_mask(input.delegation_secret.as_deref()) {
            if state.connector.delegation_secret.is_some() {
                return failure(StatusCode::CONFLICT, "env_managed_immutable");
            }
            let Some(vault) = state.secrets.as_ref() else {
                return failure(
                    StatusCode::SERVICE_UNAVAILABLE,
                    "secret_store_not_configured",
                );
            };
            if let Err(err) = secrets::set(
                &db,
                vault,
                connector::DELEGATION_SECRET,
                input.delegation_secret.as_deref().unwrap_or("").trim(),
            ) {
                return failure(StatusCode::BAD_REQUEST, err.code());
            }
        }
        if let Some(en) = input.enabled {
            let _ = db.set_setting("connector_revoked", if en { "0" } else { "1" });
        }
        let _ = db.record_audit_event(
            now(),
            &actor,
            &auth_method,
            &ip,
            "mcp_connector.updated",
            "mcp_connector",
            INBOUND_MCP_ID,
            &json!({}),
        );
        drop(db);
        return match build_mcp_connectors_snapshot(&state, &headers) {
            Ok(v) => ok_no_store(v),
            Err(err) => internal_error(err),
        };
    }

    let id = id_opt.unwrap_or_else(|| {
        let base = input.name.as_deref().unwrap_or("mcp");
        let suffix = agent_bridge::new_id("mc").replace('_', "-");
        sanitize_slug(&format!("{base}-{}", &suffix[suffix.len().saturating_sub(6)..]), "mcp-conn")
    });
    let clean_url = match input.endpoint_url.as_deref().map(str::trim) {
        Some(u) if !u.is_empty() => match validate_outbound_url(&state, u) {
            Ok(c) => Some(c),
            Err(res) => return res,
        },
        _ => None,
    };
    let incoming_secret = input.bearer_token.or(input.secret);
    let ts = now();
    {
        let db = state.db.lock().expect("database lock poisoned");
        let existing: Option<(String, String, String, String, i64, String, String, i64)> = db
            .conn
            .query_row(
                "SELECT name, endpoint_url, secret_enc, key_last4, copy_available, scopes_json, audience, enabled
                 FROM mcp_connectors WHERE id = ?1",
                [&id],
                |r| {
                    Ok((
                        r.get(0)?,
                        r.get(1)?,
                        r.get(2)?,
                        r.get(3)?,
                        r.get(4)?,
                        r.get(5)?,
                        r.get(6)?,
                        r.get(7)?,
                    ))
                },
            )
            .optional()
            .ok()
            .flatten();
        let (cur_name, cur_url, cur_enc, cur_last4, cur_copy, cur_scopes, cur_aud, cur_en) =
            existing.unwrap_or_else(|| {
                (
                    String::new(),
                    String::new(),
                    String::new(),
                    String::new(),
                    0,
                    "[\"newsman:read\"]".into(),
                    String::new(),
                    1,
                )
            });

        let name = input
            .name
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(ToOwned::to_owned)
            .unwrap_or(cur_name);
        if name.is_empty() {
            return bad_request_msg("connector name is required");
        }
        let endpoint_url = clean_url.unwrap_or(cur_url);
        if endpoint_url.is_empty() {
            return bad_request_msg("connector endpoint_url is required");
        }
        let scopes_json = input
            .scopes
            .as_ref()
            .and_then(|s| serde_json::to_string(s).ok())
            .unwrap_or(cur_scopes);
        let audience = input
            .audience
            .as_deref()
            .map(str::trim)
            .map(ToOwned::to_owned)
            .unwrap_or(cur_aud);
        let enabled = input.enabled.unwrap_or(cur_en != 0);
        let status = if enabled { "active" } else { "revoked" };

        let (secret_enc, key_last4, copy_available) = if !is_blank_or_mask(incoming_secret.as_deref())
        {
            let raw_sec = incoming_secret.as_deref().unwrap_or("").trim();
            let Some(vault) = state.secrets.as_ref() else {
                return failure(
                    StatusCode::SERVICE_UNAVAILABLE,
                    "secret_store_not_configured",
                );
            };
            let enc = match vault.seal_envelope(&format!("mcp_connector:{id}"), raw_sec) {
                Ok(e) => e,
                Err(err) => return failure(StatusCode::BAD_REQUEST, err.code()),
            };
            let (_, last4) = token_prefix_and_last4(raw_sec);
            (enc, last4, 1_i64)
        } else {
            (cur_enc, cur_last4, cur_copy)
        };

        if let Err(err) = db.conn.execute(
            "INSERT INTO mcp_connectors
                (id, name, direction, endpoint_url, secret_name, secret_enc, key_last4, copy_available,
                 scopes_json, audience, enabled, status, created_at, updated_at, revoked_at)
             VALUES (?1, ?2, 'outbound', ?3, '', ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?11, 0)
             ON CONFLICT(id) DO UPDATE SET
                name = excluded.name,
                endpoint_url = excluded.endpoint_url,
                secret_enc = excluded.secret_enc,
                key_last4 = excluded.key_last4,
                copy_available = excluded.copy_available,
                scopes_json = excluded.scopes_json,
                audience = excluded.audience,
                enabled = excluded.enabled,
                status = excluded.status,
                updated_at = excluded.updated_at",
            params![
                id,
                name,
                endpoint_url,
                secret_enc,
                key_last4,
                copy_available,
                scopes_json,
                audience,
                enabled as i64,
                status,
                ts
            ],
        ) {
            return internal_error(err);
        }
        let _ = db.record_audit_event(
            ts,
            &actor,
            &auth_method,
            &ip,
            "mcp_connector.upserted",
            "mcp_connector",
            &id,
            &json!({ "name": name }),
        );
    }

    match build_mcp_connectors_snapshot(&state, &headers) {
        Ok(mut v) => {
            if let Some(obj) = v.as_object_mut() {
                obj.insert("id".into(), json!(id));
            }
            ok_no_store(v)
        }
        Err(err) => internal_error(err),
    }
}

#[derive(Deserialize, Default)]
pub struct CopyFieldQuery {
    field: Option<String>,
}

pub async fn copy_mcp_connector(
    State(state): State<SharedState>,
    peer: Option<Extension<ConnectInfo<SocketAddr>>>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Query(query): Query<CopyFieldQuery>,
) -> Response {
    let ip = match check_copy_rate_limit(&state, peer.as_ref(), &headers) {
        Ok(ip) => ip,
        Err(res) => return res,
    };
    let (auth_method, actor) = actor_identity(&state, &headers);
    let field = query.field.unwrap_or_else(|| "bearer".to_string());

    let (secret_val, copied_field) = if id == INBOUND_MCP_ID {
        let Some(vault) = state.secrets.as_ref() else {
            return failure(
                StatusCode::SERVICE_UNAVAILABLE,
                "secret_store_not_configured",
            );
        };
        let (target_name, label, env_pinned) = if field == "delegation" {
            (
                connector::DELEGATION_SECRET,
                "delegation_secret",
                state.connector.delegation_secret.is_some(),
            )
        } else {
            (
                connector::BEARER_SECRET,
                "bearer_token",
                state.connector.bearer.is_some(),
            )
        };
        if env_pinned {
            return failure(StatusCode::CONFLICT, "env_managed_immutable");
        }
        let db = state.db.lock().expect("database lock poisoned");
        let val = match secrets::get(&db, vault, target_name) {
            Ok(v) => v,
            Err(_) => return failure(StatusCode::CONFLICT, "copy_unavailable_rotate_required"),
        };
        (val, label)
    } else {
        let enc: Option<String> = {
            let db = state.db.lock().expect("database lock poisoned");
            db.conn
                .query_row(
                    "SELECT secret_enc FROM mcp_connectors WHERE id = ?1",
                    [&id],
                    |r| r.get(0),
                )
                .optional()
                .ok()
                .flatten()
        };
        let Some(enc) = enc else {
            return failure(StatusCode::NOT_FOUND, "not_found");
        };
        match secret_for_copy(&state, &format!("mcp_connector:{id}"), &enc, None) {
            Ok(p) => (p, "bearer_token"),
            Err(res) => return res,
        }
    };

    {
        let db = state.db.lock().expect("database lock poisoned");
        let _ = db.record_audit_event(
            now(),
            &actor,
            &auth_method,
            &ip,
            "secret.copy",
            "mcp_connector",
            &id,
            &json!({ "copied_field": copied_field }),
        );
    }
    ok_no_store(json!({ "ok": true, "secret": secret_val, "copied_field": copied_field }))
}

#[derive(Deserialize, Default)]
pub struct RotateMcpInput {
    field: Option<String>,
    secret: Option<String>,
}

pub async fn rotate_mcp_connector(
    State(state): State<SharedState>,
    peer: Option<Extension<ConnectInfo<SocketAddr>>>,
    headers: HeaderMap,
    Path(id): Path<String>,
    input: Option<Json<RotateMcpInput>>,
) -> Response {
    let body = input.map(|j| j.0).unwrap_or_default();
    let Some(vault) = state.secrets.as_ref() else {
        return failure(
            StatusCode::SERVICE_UNAVAILABLE,
            "secret_store_not_configured",
        );
    };
    let (auth_method, actor) = actor_identity(&state, &headers);
    let ip = security::request_client_ip(&state, peer.as_ref(), &headers);
    let ts = now();

    if id == INBOUND_MCP_ID {
        let field = body.field.unwrap_or_else(|| "bearer".to_string());
        if (field == "bearer" && state.connector.bearer.is_some())
            || (field == "delegation" && state.connector.delegation_secret.is_some())
        {
            return failure(StatusCode::CONFLICT, "env_managed_immutable");
        }
        let new_val = match body.secret.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
            Some(s) => s.to_string(),
            None => match mint_token(if field == "delegation" { "nmd_" } else { "nmc_" }, 24) {
                Ok(t) => t,
                Err(res) => return res,
            },
        };
        let target_name = if field == "delegation" {
            connector::DELEGATION_SECRET
        } else {
            connector::BEARER_SECRET
        };
        {
            let db = state.db.lock().expect("database lock poisoned");
            if let Err(err) = secrets::set(&db, vault, target_name, &new_val) {
                return failure(StatusCode::BAD_REQUEST, err.code());
            }
            let _ = db.set_setting("connector_revoked", "0");
            let _ = db.record_audit_event(
                ts,
                &actor,
                &auth_method,
                &ip,
                "mcp_connector.rotated",
                "mcp_connector",
                &id,
                &json!({ "field": field }),
            );
        }
        return match build_mcp_connectors_snapshot(&state, &headers) {
            Ok(mut v) => {
                if let Some(obj) = v.as_object_mut() {
                    obj.insert("secret".into(), json!(new_val));
                    obj.insert("copied_field".into(), json!(field));
                }
                ok_no_store(v)
            }
            Err(err) => internal_error(err),
        };
    }

    let new_sec = match body.secret.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        Some(s) => s.to_string(),
        None => match mint_token("nm_mcp_", 24) {
            Ok(t) => t,
            Err(res) => return res,
        },
    };
    let (_, last4) = token_prefix_and_last4(&new_sec);
    let enc = match vault.seal_envelope(&format!("mcp_connector:{id}"), &new_sec) {
        Ok(e) => e,
        Err(err) => return failure(StatusCode::BAD_REQUEST, err.code()),
    };
    {
        let db = state.db.lock().expect("database lock poisoned");
        let changed = db
            .conn
            .execute(
                "UPDATE mcp_connectors
                 SET secret_enc = ?2, key_last4 = ?3, copy_available = 1, enabled = 1,
                     status = 'active', revoked_at = 0, updated_at = ?4
                 WHERE id = ?1",
                params![id, enc, last4, ts],
            )
            .unwrap_or(0);
        if changed == 0 {
            return failure(StatusCode::NOT_FOUND, "not_found");
        }
        let _ = db.record_audit_event(
            ts,
            &actor,
            &auth_method,
            &ip,
            "mcp_connector.rotated",
            "mcp_connector",
            &id,
            &json!({ "last4": last4 }),
        );
    }
    match build_mcp_connectors_snapshot(&state, &headers) {
        Ok(mut v) => {
            if let Some(obj) = v.as_object_mut() {
                obj.insert("secret".into(), json!(new_sec));
            }
            ok_no_store(v)
        }
        Err(err) => internal_error(err),
    }
}

pub async fn revoke_mcp_connector(
    State(state): State<SharedState>,
    peer: Option<Extension<ConnectInfo<SocketAddr>>>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Response {
    let (auth_method, actor) = actor_identity(&state, &headers);
    let ip = security::request_client_ip(&state, peer.as_ref(), &headers);
    let ts = now();
    {
        let db = state.db.lock().expect("database lock poisoned");
        if id == INBOUND_MCP_ID {
            if state.connector.bearer.is_some() && state.connector.delegation_secret.is_some() {
                return failure(StatusCode::CONFLICT, "env_managed_immutable");
            }
            let _ = db.set_setting("connector_revoked", "1");
        } else {
            let changed = db
                .conn
                .execute(
                    "UPDATE mcp_connectors SET enabled = 0, status = 'revoked', revoked_at = ?2, updated_at = ?2 WHERE id = ?1",
                    params![id, ts],
                )
                .unwrap_or(0);
            if changed == 0 {
                return failure(StatusCode::NOT_FOUND, "not_found");
            }
        }
        let _ = db.record_audit_event(
            ts,
            &actor,
            &auth_method,
            &ip,
            "mcp_connector.revoked",
            "mcp_connector",
            &id,
            &json!({}),
        );
    }
    match build_mcp_connectors_snapshot(&state, &headers) {
        Ok(mut v) => {
            if let Some(obj) = v.as_object_mut() {
                obj.insert("id".into(), json!(id));
            }
            ok_no_store(v)
        }
        Err(err) => internal_error(err),
    }
}

pub async fn delete_mcp_connector(
    State(state): State<SharedState>,
    peer: Option<Extension<ConnectInfo<SocketAddr>>>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Response {
    let (auth_method, actor) = actor_identity(&state, &headers);
    let ip = security::request_client_ip(&state, peer.as_ref(), &headers);
    {
        let db = state.db.lock().expect("database lock poisoned");
        if id == INBOUND_MCP_ID {
            if state.connector.bearer.is_some() && state.connector.delegation_secret.is_some() {
                return failure(StatusCode::CONFLICT, "env_managed_immutable");
            }
            let _ = secrets::delete(&db, connector::BEARER_SECRET);
            let _ = secrets::delete(&db, connector::DELEGATION_SECRET);
            let _ = db.set_setting("connector_revoked", "0");
        } else {
            let removed = db
                .conn
                .execute("DELETE FROM mcp_connectors WHERE id = ?1", [&id])
                .unwrap_or(0);
            if removed == 0 {
                return failure(StatusCode::NOT_FOUND, "not_found");
            }
        }
        let _ = db.record_audit_event(
            now(),
            &actor,
            &auth_method,
            &ip,
            "mcp_connector.removed",
            "mcp_connector",
            &id,
            &json!({}),
        );
    }
    match build_mcp_connectors_snapshot(&state, &headers) {
        Ok(mut v) => {
            if let Some(obj) = v.as_object_mut() {
                obj.insert("deleted_id".into(), json!(id));
            }
            ok_no_store(v)
        }
        Err(err) => internal_error(err),
    }
}

pub async fn test_mcp_connector(
    State(state): State<SharedState>,
    Path(id): Path<String>,
) -> Response {
    if id == INBOUND_MCP_ID {
        let resolved = connector::resolve(&state);
        let tools = mcp::tools();
        return ok_no_store(json!({
            "ok": true,
            "id": id,
            "paired": resolved.bearer.is_some() && resolved.delegation_secret.is_some(),
            "tool_count": tools.len(),
            "tools": tools.iter().map(|t| t.name).collect::<Vec<_>>(),
        }));
    }

    let row: Option<(String, String)> = {
        let db = state.db.lock().expect("database lock poisoned");
        db.conn
            .query_row(
                "SELECT endpoint_url, secret_enc FROM mcp_connectors WHERE id = ?1",
                [&id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()
            .ok()
            .flatten()
    };
    let Some((endpoint_url, secret_enc)) = row else {
        return failure(StatusCode::NOT_FOUND, "not_found");
    };
    let bearer = if secret_enc.is_empty() {
        String::new()
    } else {
        secret_for_copy(&state, &format!("mcp_connector:{id}"), &secret_enc, None)
            .unwrap_or_default()
    };
    let fetcher = outbound_fetcher(&state, &[]);
    let auth_hdr = (!bearer.trim().is_empty()).then(|| format!("Bearer {}", bearer.trim()));
    let mut hdrs: Vec<(&str, &str)> = vec![("Accept", "application/json")];
    if let Some(ref a) = auth_hdr {
        hdrs.push(("Authorization", a.as_str()));
    }
    let init_body = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "protocolVersion": "2025-06-18",
            "capabilities": {},
            "clientInfo": { "name": "lnc-newsman", "version": state.cfg.version }
        }
    })
    .to_string();
    if let Err(err) = fetcher
        .request_json(reqwest::Method::POST, &endpoint_url, Some(&init_body), &hdrs)
        .await
    {
        return no_store_response(
            StatusCode::BAD_GATEWAY,
            json!({ "ok": false, "error": "mcp_probe_failed", "detail": err.code() }),
        );
    }
    let list_body = json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/list",
        "params": {}
    })
    .to_string();
    match fetcher
        .request_json(reqwest::Method::POST, &endpoint_url, Some(&list_body), &hdrs)
        .await
    {
        Ok(FetchOutcome::Body { bytes, .. }) => {
            let v: Value = serde_json::from_slice(&bytes).unwrap_or_else(|_| json!({}));
            let tool_count = v
                .pointer("/result/tools")
                .and_then(Value::as_array)
                .map(|a| a.len())
                .unwrap_or(0);
            ok_no_store(json!({
                "ok": true,
                "id": id,
                "tool_count": tool_count,
            }))
        }
        Ok(_) => ok_no_store(json!({ "ok": true, "id": id, "tool_count": 0 })),
        Err(err) => no_store_response(
            StatusCode::BAD_GATEWAY,
            json!({ "ok": false, "error": "mcp_probe_failed", "detail": err.code() }),
        ),
    }
}
