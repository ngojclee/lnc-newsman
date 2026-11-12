//! Connector boundary tests: the delegation envelope the Hub actually signs, and the
//! reject paths the integration standard requires. Everything here is offline; a
//! signature is produced by the same HMAC the app verifies, from the envelope bytes
//! written out by hand, so a change to the envelope cannot pass silently.
//!
//! The tests build requests as raw header maps and drive the real router, so the CSRF
//! exemption and the credential discrimination are exercised, not just the verifier.

use super::*;
use crate::SharedState;
use crate::connector::{ALL_SCOPES, READ_SCOPE, WRITE_SCOPE};
use crate::mcp::{UNKNOWN_TOOL, tool_scope, tools};
use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use http_body_util::BodyExt;
use tower::ServiceExt;

const BEARER: &str = "conn_test_bearer_abcdefghijklmnop";
const SECRET: &str = "test-delegation-secret-hex";

fn state_with_connector() -> SharedState {
    let dir = std::env::temp_dir().join(format!("lnc-newsman-conn-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let db = crate::db::Database::in_memory().unwrap();
    db.conn
        .execute(
            "INSERT INTO topics(slug,label_vi,label_en,enabled) VALUES ('crypto','Tiền mã hóa','Crypto',1)
             ON CONFLICT(slug) DO NOTHING",
            [],
        )
        .unwrap();
    db.conn
        .execute(
            "INSERT INTO sources(slug,name,url,tier,enabled,region) VALUES ('s','Wire','https://example.test/feed','T2',1,'global')
             ON CONFLICT(slug) DO NOTHING",
            [],
        )
        .unwrap();
    let source: i64 = db
        .conn
        .query_row("SELECT id FROM sources WHERE slug='s'", [], |r| r.get(0))
        .unwrap();
    db.conn
        .execute(
            "INSERT INTO articles(canonical_url,title,source_id,topic_slug,fetched_at,region_code)
             VALUES ('https://example.test/a','A visible item',?1,'crypto',0,'global')",
            [source],
        )
        .unwrap();
    let mut state = crate::AppState::new(
        crate::config::Config {
            data_dir: dir.clone(),
            regions_dir: dir.join("regions"),
            bind_address: "127.0.0.1".parse().unwrap(),
            port: 8000,
            management_key: Some("test-management-key".into()),
            api_keys: vec!["test-api-key".into()],
            allowed_hosts: vec!["newsman.test".into()],
            trusted_proxies: Vec::new(),
            secure_cookie: true,
            version: "test",
        },
        db,
        crate::fetch::Fetcher::public_only(),
    );
    state.connector = ConnectorConfig {
        bearer: Some(BEARER.into()),
        delegation_secret: Some(SECRET.into()),
        audience: Some(DEFAULT_AUDIENCE.into()),
        max_scopes: None,
    };
    std::sync::Arc::new(state)
}

/// The exact bytes the Hub signs, spelled out rather than reused from the module, so a
/// change to `envelope` shows up as a failing test rather than a shared mistake.
fn hub_signature(
    secret: &str,
    app_id: Option<&str>,
    audience: &str,
    client_id: &str,
    timestamp: u64,
    nonce: &str,
    scopes: &[&str],
) -> String {
    let joined = scopes.join(",");
    let payload = match app_id {
        Some(app_id) => format!(
            "v4\n{audience}\n{client_id}\n{app_id}\n{timestamp}\n{nonce}\n{joined}"
        ),
        None => format!("v3\n{audience}\n{client_id}\n{timestamp}\n{nonce}\n{joined}"),
    };
    let mut mac = HmacSha256::new_from_slice(secret.as_bytes()).unwrap();
    mac.update(payload.as_bytes());
    mac.finalize()
        .into_bytes()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

struct Signed {
    timestamp: u64,
    nonce: String,
    signature: String,
    scopes: String,
    client_id: String,
}

fn signed(
    secret: &str,
    audience: &str,
    client_id: &str,
    scopes: &[&str],
    nonce: &str,
    timestamp: u64,
    app_id: Option<&str>,
) -> Signed {
    Signed {
        timestamp,
        nonce: nonce.to_string(),
        signature: hub_signature(secret, app_id, audience, client_id, timestamp, nonce, scopes),
        scopes: scopes.join(","),
        client_id: client_id.to_string(),
    }
}

fn now() -> u64 {
    crate::now()
}

/// A request carrying the full Hub credential set and no CSRF header — which is the
/// point: the Hub cannot send one.
fn hub_request(signed: &Signed, body: serde_json::Value) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri("/mcp")
        .header(header::HOST, "newsman.test")
        .header(header::CONTENT_TYPE, "application/json")
        .header(header::AUTHORIZATION, format!("Bearer {BEARER}"))
        .header("x-mcp-hub-client-id", &signed.client_id)
        .header("x-mcp-hub-client-scopes", &signed.scopes)
        .header(
            "x-mcp-hub-client-timestamp",
            signed.timestamp.to_string(),
        )
        .header("x-mcp-hub-client-nonce", &signed.nonce)
        .header("x-mcp-hub-client-signature", &signed.signature)
        .body(Body::from(body.to_string()))
        .unwrap()
}

async fn send(app: axum::Router, request: Request<Body>) -> (StatusCode, serde_json::Value) {
    let response = app.oneshot(request).await.unwrap();
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null),
    )
}

fn tools_list() -> serde_json::Value {
    serde_json::json!({"jsonrpc":"2.0","id":1,"method":"tools/list"})
}

fn call(name: &str) -> serde_json::Value {
    serde_json::json!({
        "jsonrpc":"2.0","id":2,"method":"tools/call",
        "params":{"name":name,"arguments":{"limit":5}}
    })
}

fn names(value: &serde_json::Value) -> Vec<String> {
    value["result"]["tools"]
        .as_array()
        .map(|tools| {
            tools
                .iter()
                .filter_map(|tool| tool["name"].as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

// ---------- the happy path ----------

#[test]
fn a_correctly_signed_read_call_is_accepted() {
    let state = state_with_connector();
    let sh = signed(
        SECRET,
        DEFAULT_AUDIENCE,
        "agent-argus",
        &[READ_SCOPE],
        "nonce-happy-path-0000000001",
        now(),
        None,
    );
    let app = crate::router(state);
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let (status, body) = runtime.block_on(async move {
        let (list_status, list) = send(app.clone(), hub_request(&sh, tools_list())).await;
        assert_eq!(list_status, StatusCode::OK, "{list}");
        assert_eq!(
            names(&list),
            vec![
                "regions",
                "topics",
                "reader_preferences",
                "latest_articles",
                "documents",
                "tax_calendar",
                "series",
                "observations",
                "notes",
                "analyses",
                "write_log"
            ],
            "a read caller sees the read tools, with no prefix: the Hub adds the namespace.              No write tool is advertised to a read-only grant."
        );
        (list_status, list)
    });
    assert_eq!(status, StatusCode::OK);

    // A second call must use a fresh nonce: replay defence is per request.
    let state2 = state_with_connector();
    let signed2 = signed(
        SECRET,
        DEFAULT_AUDIENCE,
        "agent-argus",
        &[READ_SCOPE],
        "nonce-happy-path-0000000002",
        now(),
        None,
    );
    let app2 = crate::router(state2);
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let (call_status, call_body) = runtime.block_on(async move {
        send(app2, hub_request(&signed2, call("latest_articles"))).await
    });
    assert_eq!(call_status, StatusCode::OK);
    assert_eq!(
        call_body["result"]["isError"], false,
        "a signed read must actually return data: {call_body}"
    );
    assert!(
        call_body["result"]["content"][0]["text"]
            .as_str()
            .is_some_and(|text| text.contains("A visible item")),
        "the tool must return the stored article: {call_body}"
    );
    assert!(body["result"]["tools"].is_array());
}

#[test]
fn a_v4_call_signs_the_app_id_it_asserts() {
    let state = state_with_connector();
    let sh = signed(
        SECRET,
        DEFAULT_AUDIENCE,
        "agent-argus",
        &[READ_SCOPE],
        "nonce-v4-envelope-000000001",
        now(),
        Some("lnc-newsman"),
    );
    let mut request = hub_request(&sh, tools_list());
    request
        .headers_mut()
        .insert("x-mcp-hub-app-id", "lnc-newsman".parse().unwrap());
    let app = crate::router(state);
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let (status, body) = runtime.block_on(async move { send(app, request).await });
    assert_eq!(status, StatusCode::OK, "v4 envelope accepted: {body}");
    assert!(names(&body).contains(&"regions".to_string()));
}

// ---------- reject paths required by the integration standard ----------

#[test]
fn a_missing_or_wrong_bearer_is_unauthorized() {
    let state = state_with_connector();
    let sh = signed(
        SECRET,
        DEFAULT_AUDIENCE,
        "agent-argus",
        &[READ_SCOPE],
        "nonce-bearer-reject-00000001",
        now(),
        None,
    );
    let runtime = tokio::runtime::Runtime::new().unwrap();
    // Two different answers on purpose: no `Authorization` at all is a plain reader
    // probe (401 `unauthorized`, as `/api/v1/*` answers), while a bearer of the wrong
    // value is judged as a connector and gets the connector code. Neither reveals
    // whether a connector is paired.
    for (bearer, expected) in [
        (None, "unauthorized"),
        (Some("conn_wrong_bearer_aaaaaaaaaaaa"), DENIED_BEARER),
    ] {
        let app = crate::router(state.clone());
        let mut request = hub_request(&sh, tools_list());
        match bearer {
            None => {
                request.headers_mut().remove(header::AUTHORIZATION);
            }
            Some(value) => {
                request.headers_mut().insert(
                    header::AUTHORIZATION,
                    format!("Bearer {value}").parse().unwrap(),
                );
            }
        }
        let (status, body) = runtime.block_on(async move { send(app, request).await });
        assert_eq!(status, StatusCode::UNAUTHORIZED, "{body}");
        assert_eq!(body["error"], expected, "{body}");
    }
}

#[test]
fn a_wrong_signature_is_forbidden() {
    let state = state_with_connector();
    // Signed with a different secret — the shape a rotated or leaked-and-changed
    // secret produces.
    let mut sh = signed(
        "a-different-secret",
        DEFAULT_AUDIENCE,
        "agent-argus",
        &[READ_SCOPE],
        "nonce-bad-signature-00000001",
        now(),
        None,
    );
    sh.signature = sh.signature.to_uppercase();
    let app = crate::router(state);
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let (status, body) =
        runtime.block_on(async move { send(app, hub_request(&sh, tools_list())).await });
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    assert_eq!(body["error"], DENIED_DELEGATION);
}

#[test]
fn a_signature_for_another_audience_is_forbidden() {
    let state = state_with_connector();
    let sh = signed(
        SECRET,
        "lnc-library",
        "agent-argus",
        &[READ_SCOPE],
        "nonce-wrong-audience-0000001",
        now(),
        None,
    );
    let app = crate::router(state);
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let (status, body) =
        runtime.block_on(async move { send(app, hub_request(&sh, tools_list())).await });
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    assert_eq!(body["error"], DENIED_DELEGATION);
}

#[test]
fn an_expired_timestamp_is_forbidden() {
    let state = state_with_connector();
    let sh = signed(
        SECRET,
        DEFAULT_AUDIENCE,
        "agent-argus",
        &[READ_SCOPE],
        "nonce-expired-timestamp-000001",
        now().saturating_sub(120),
        None,
    );
    let app = crate::router(state);
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let (status, body) =
        runtime.block_on(async move { send(app, hub_request(&sh, tools_list())).await });
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    assert_eq!(body["error"], DENIED_DELEGATION);
}

#[test]
fn a_replayed_nonce_is_forbidden() {
    let state = state_with_connector();
    let nonce = "nonce-replay-attempt-0000000001";
    let runtime = tokio::runtime::Runtime::new().unwrap();
    // Each attempt is freshly signed, so only the nonce differs from the first.
    let mut statuses = Vec::new();
    for attempt in 0..2 {
        let sh = signed(
            SECRET,
            DEFAULT_AUDIENCE,
            "agent-argus",
            &[READ_SCOPE],
            nonce,
            now(),
            None,
        );
        let app = crate::router(state.clone());
        let (status, _) = runtime.block_on(async move {
            send(app, hub_request(&sh, tools_list())).await
        });
        statuses.push((attempt, status));
    }
    assert_eq!(statuses[0].1, StatusCode::OK);
    assert_eq!(statuses[1].1, StatusCode::FORBIDDEN, "{statuses:?}");
}

#[test]
fn a_scope_outside_the_ceiling_is_forbidden() {
    let state = state_with_connector();
    // A scope this build does not implement: the Hub signing it must not make it real.
    let sh = signed(
        SECRET,
        DEFAULT_AUDIENCE,
        "agent-argus",
        &[READ_SCOPE, "newsman:admin"],
        "nonce-extra-scope-00000000001",
        now(),
        None,
    );
    let app = crate::router(state);
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let (status, body) =
        runtime.block_on(async move { send(app, hub_request(&sh, tools_list())).await });
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    assert_eq!(body["error"], DENIED_DELEGATION);
}

/// With a single supported scope, a caller granted any *other* scope is refused at
/// the envelope: the ceiling is the read scope and nothing else, so "granted something
/// this app cannot use" is exactly "scope outside the ceiling". The tool-level filter
/// still matters for a future build that adds a second scope; today this test pins
/// the shape of that refusal so the two gates cannot silently diverge.
#[test]
fn a_signed_caller_without_the_read_scope_is_refused() {
    let state = state_with_connector();
    let sh = signed(
        SECRET,
        DEFAULT_AUDIENCE,
        "agent-argus",
        &["other:read"],
        "nonce-no-read-scope-000000001",
        now(),
        None,
    );
    let app = crate::router(state);
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let (status, body) =
        runtime.block_on(async move { send(app, hub_request(&sh, tools_list())).await });
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    assert_eq!(body["error"], DENIED_DELEGATION);
    // And with no caller at all (a reader key), every tool is still visible.
    assert!(crate::connector::scope_allows(None, "regions"));
}

#[test]
fn an_unknown_tool_answers_the_same_as_an_unauthorized_one() {
    let state = state_with_connector();
    let sh = signed(
        SECRET,
        DEFAULT_AUDIENCE,
        "agent-argus",
        &[READ_SCOPE],
        "nonce-unknown-tool-0000000001",
        now(),
        None,
    );
    let app = crate::router(state);
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let (status, body) =
        runtime.block_on(async move { send(app, hub_request(&sh, call("delete_everything"))).await });
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["error"]["code"], -32601);
    assert_eq!(body["error"]["message"], UNKNOWN_TOOL);
}

/// Every published tool must declare exactly one scope, and the two scopes must be the
/// only ones a ceiling can hold. A tool with no entry in the table would be unreachable
/// by design, which is easy to mistake for "no tool" — so it fails here instead.
#[test]
fn every_tool_declares_a_known_scope() {
    for tool in tools() {
        let scope = tool_scope(tool.name).unwrap_or_else(|| panic!("{} has no scope", tool.name));
        assert!(
            ALL_SCOPES.contains(&scope),
            "{} declares {scope}, which is not a scope this build implements",
            tool.name
        );
    }
    // The write tools are exactly the ones this app may let an agent change; none of
    // them is named for configuration.
    for tool in tools().iter().filter(|tool| tool_scope(tool.name) == Some(WRITE_SCOPE)) {
        for forbidden in ["source", "region", "connector", "secret", "collector", "alert"] {
            assert!(
                !tool.name.contains(forbidden),
                "{} looks like a configuration write, which this boundary must not grant",
                tool.name
            );
        }
    }
}

// ---------- the reader's existing credentials still work ----------

#[test]
fn a_reader_api_key_still_works_without_any_delegation() {
    let state = state_with_connector();
    let app = crate::router(state);
    let request = Request::builder()
        .method("POST")
        .uri("/mcp")
        .header(header::HOST, "newsman.test")
        .header(header::CONTENT_TYPE, "application/json")
        .header("x-requested-with", "XMLHttpRequest")
        .header("x-api-key", "test-api-key")
        .body(Body::from(tools_list().to_string()))
        .unwrap();
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let (status, body) = runtime.block_on(async move { send(app, request).await });
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        names(&body).len(),
        tools().len(),
        "a reader key sees every tool, read and write alike"
    );
}

#[test]
fn no_credential_at_all_is_unauthorized() {
    let state = state_with_connector();
    let app = crate::router(state);
    let request = Request::builder()
        .method("POST")
        .uri("/mcp")
        .header(header::HOST, "newsman.test")
        .header(header::CONTENT_TYPE, "application/json")
        .header("x-requested-with", "XMLHttpRequest")
        .body(Body::from(tools_list().to_string()))
        .unwrap();
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let (status, body) = runtime.block_on(async move { send(app, request).await });
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(body["error"], "unauthorized");
}

// ---------- configuration resolution ----------

#[test]
fn the_prefixed_tool_name_is_still_accepted() {
    let state = state_with_connector();
    let sh = signed(
        SECRET,
        DEFAULT_AUDIENCE,
        "agent-argus",
        &[READ_SCOPE],
        "nonce-legacy-name-00000000001",
        now(),
        None,
    );
    let app = crate::router(state);
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let (status, body) = runtime.block_on(async move {
        send(app, hub_request(&sh, call("newsman__latest_articles"))).await
    });
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        body["result"]["isError"], false,
        "a client written against 0.4.3 keeps working: {body}"
    );
}

#[test]
fn the_audience_falls_back_to_the_compiled_default() {
    let mut state = state_with_connector();
    let inner = std::sync::Arc::get_mut(&mut state).expect("not shared");
    inner.connector.audience = None;
    let resolved = resolve(&state);
    assert_eq!(resolved.audience, DEFAULT_AUDIENCE);
    assert_eq!(resolved.audience_source, Source::Unset);
    // And the ceiling is the single read scope, never wider.
    assert_eq!(
        resolved.max_scopes,
        ALL_SCOPES.iter().map(|s| s.to_string()).collect::<Vec<_>>()
    );
}

#[test]
fn the_status_never_carries_a_secret_value() {
    let state = state_with_connector();
    let rendered = serde_json::to_string(&status(&state)).unwrap();
    assert!(!rendered.contains(BEARER), "{rendered}");
    assert!(!rendered.contains(SECRET), "{rendered}");
    assert!(rendered.contains("env"), "{rendered}");
    let debug_config = format!("{:?}", state.connector);
    assert!(!debug_config.contains(BEARER), "{debug_config}");
    assert!(!debug_config.contains(SECRET), "{debug_config}");
}

#[test]
fn an_unpaired_connector_refuses_every_hub_call() {
    let mut state = state_with_connector();
    std::sync::Arc::get_mut(&mut state).expect("not shared").connector =
        ConnectorConfig::default();
    // No stored values either, so the connector is unpaired.
    let sh = signed(
        SECRET,
        DEFAULT_AUDIENCE,
        "agent-argus",
        &[READ_SCOPE],
        "nonce-unpaired-connector-000001",
        now(),
        None,
    );
    let app = crate::router(state);
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let (status, body) =
        runtime.block_on(async move { send(app, hub_request(&sh, tools_list())).await });
    assert_eq!(
        status,
        StatusCode::UNAUTHORIZED,
        "an unpaired connector is indistinguishable from a wrong token: {body}"
    );
}

/// An operator typing a bad value into `CONNECTOR_MAX_SCOPES` must not silently make
/// every delegated call fail: the ceiling is intersected with what this build
/// implements. The admin endpoint refuses the same value outright, because there a
/// person is watching and the mistake should be visible.
#[test]
fn a_nonsense_env_ceiling_narrows_to_the_real_scope() {
    for (ceiling, expected) in [
        (
            vec!["fleet:read".to_string()],
            ALL_SCOPES.iter().map(|s| s.to_string()).collect::<Vec<_>>(),
        ),
        (
            vec!["newsman:write".to_string(), "fleet:read".to_string()],
            vec![WRITE_SCOPE.to_string()],
        ),
        (
            vec![READ_SCOPE.to_string(), "other:read".to_string()],
            vec![READ_SCOPE.to_string()],
        ),
        (
            Vec::new(),
            ALL_SCOPES.iter().map(|s| s.to_string()).collect::<Vec<_>>(),
        ),
    ] {
        let mut state = state_with_connector();
        std::sync::Arc::get_mut(&mut state)
            .expect("not shared")
            .connector
            .max_scopes = Some(ceiling.clone());
        assert_eq!(
            resolve(&state).max_scopes,
            expected,
            "{ceiling:?} must narrow to the scopes this build implements"
        );
    }
}

// ---------- the admin card's own validation ----------

fn patch(state: &SharedState, body: serde_json::Value) -> (StatusCode, serde_json::Value) {
    let app = crate::router(state.clone());
    let runtime = tokio::runtime::Runtime::new().unwrap();
    runtime.block_on(async move {
        // A signed-in session is required for the admin routes; the test signs in
        // first and reuses the cookie, exactly as the browser does.
        let login = Request::builder()
            .method("POST")
            .uri("/api/auth/login")
            .header(header::HOST, "newsman.test")
            .header(header::CONTENT_TYPE, "application/json")
            .header("x-requested-with", "XMLHttpRequest")
            .body(Body::from(r#"{"key":"test-management-key"}"#))
            .unwrap();
        let response = app.clone().oneshot(login).await.unwrap();
        let cookie = response
            .headers()
            .get(header::SET_COOKIE)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.split(';').next())
            .map(str::to_string)
            .unwrap_or_default();
        let request = Request::builder()
            .method("PATCH")
            .uri("/api/admin/connector")
            .header(header::HOST, "newsman.test")
            .header(header::CONTENT_TYPE, "application/json")
            .header("x-requested-with", "XMLHttpRequest")
            .header(header::COOKIE, cookie)
            .body(Body::from(body.to_string()))
            .unwrap();
        send(app, request).await
    })
}

#[test]
fn the_admin_card_refuses_what_it_cannot_honour() {
    let state = state_with_connector();
    for (body, code) in [
        (
            serde_json::json!({"max_scopes": "newsman:admin"}),
            "unsupported_connector_scope",
        ),
        (
            serde_json::json!({"max_scopes": "not a scope"}),
            "invalid_connector_scopes",
        ),
        (
            serde_json::json!({"audience": "has space"}),
            "invalid_connector_audience",
        ),
    ] {
        let (status, reply) = patch(&state, body.clone());
        assert_eq!(status, StatusCode::BAD_REQUEST, "{body} -> {reply}");
        assert_eq!(reply["error"], code, "{body}");
    }
    // The honest values are accepted, and the reply never carries a secret.
    let (status, reply) = patch(
        &state,
        serde_json::json!({"audience": "lnc-newsman", "max_scopes": "newsman:read"}),
    );
    assert_eq!(status, StatusCode::OK, "{reply}");
    assert_eq!(reply["audience"], "lnc-newsman");
    let rendered = reply.to_string();
    assert!(!rendered.contains(BEARER), "{rendered}");
    assert!(!rendered.contains(SECRET), "{rendered}");
}

/// The ceiling must come from the build, not from whatever a deployment file happens to
/// set. An empty `CONNECTOR_MAX_SCOPES` therefore means "both scopes this build
/// implements" — a compose default that narrowed it would be a ceiling nobody declared.
#[test]
fn an_empty_ceiling_means_every_scope_this_build_has() {
    let mut state = state_with_connector();
    std::sync::Arc::get_mut(&mut state)
        .expect("not shared")
        .connector
        .max_scopes = None;
    assert_eq!(
        resolve(&state).max_scopes,
        ALL_SCOPES.iter().map(|s| s.to_string()).collect::<Vec<_>>()
    );
}
