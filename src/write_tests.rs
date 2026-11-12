//! Write-tool tests. The important ones are negative: a caller without
//! `newsman:write` must reach nothing, an agent must not edit the owner's rows or
//! another agent's, and a citation must point at a stored article. The positive path
//! proves a write lands with the *delegated* client id as its author.

use super::*;
use crate::connector::{Caller, READ_SCOPE, WRITE_SCOPE};
use crate::SharedState;
use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use http_body_util::BodyExt;
use hmac::{Hmac, Mac};
use sha2::Sha256;
use tower::ServiceExt;

const BEARER: &str = "conn_write_test_bearer_abcdefghij";
const SECRET: &str = "write-test-delegation-secret";
const AUDIENCE: &str = "lnc-newsman";

fn state() -> SharedState {
    let dir = std::env::temp_dir().join(format!("lnc-newsman-write-{}", std::process::id()));
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
    for (id, title) in [(1i64, "A visible item"), (2, "Another item")] {
        db.conn
            .execute(
                "INSERT INTO articles(id,canonical_url,title,source_id,topic_slug,fetched_at,region_code)
                 VALUES (?1,?2,?3,?4,'crypto',0,'global')",
                rusqlite::params![id, format!("https://example.test/{id}"), title, source],
            )
            .unwrap();
    }
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
    state.connector = crate::connector::ConnectorConfig {
        bearer: Some(BEARER.into()),
        delegation_secret: Some(SECRET.into()),
        audience: Some(AUDIENCE.into()),
        max_scopes: None,
    };
    std::sync::Arc::new(state)
}

fn caller(client: &str, scopes: &[&str]) -> Caller {
    Caller {
        client_id: client.to_string(),
        app_id: None,
        scopes: scopes.iter().map(|s| s.to_string()).collect(),
    }
}

fn sign(client: &str, scopes: &[&str], nonce: &str, ts: u64) -> serde_json::Value {
    let joined = scopes.join(",");
    let payload = format!("v3\n{AUD}{nl}{client}{nl}{ts}{nl}{nonce}{nl}{joined}", AUD = AUDIENCE, nl = "\n");
    let mut mac = Hmac::<Sha256>::new_from_slice(SECRET.as_bytes()).unwrap();
    mac.update(payload.as_bytes());
    let signature: String = mac
        .finalize()
        .into_bytes()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    serde_json::json!({
        "client": client, "scopes": joined, "ts": ts, "nonce": nonce, "sig": signature
    })
}

async fn call(
    app: axum::Router,
    name: &str,
    arguments: serde_json::Value,
    signed: Option<serde_json::Value>,
) -> (StatusCode, serde_json::Value) {
    let body = serde_json::json!({
        "jsonrpc":"2.0","id":1,"method":"tools/call",
        "params":{"name": name, "arguments": arguments}
    });
    let mut builder = Request::builder()
        .method("POST")
        .uri("/mcp")
        .header(header::HOST, "newsman.test")
        .header(header::CONTENT_TYPE, "application/json");
    if let Some(s) = signed {
        builder = builder
            .header(header::AUTHORIZATION, format!("Bearer {BEARER}"))
            .header("x-mcp-hub-client-id", s["client"].as_str().unwrap())
            .header("x-mcp-hub-client-scopes", s["scopes"].as_str().unwrap())
            .header("x-mcp-hub-client-timestamp", s["ts"].to_string())
            .header("x-mcp-hub-client-nonce", s["nonce"].as_str().unwrap())
            .header("x-mcp-hub-client-signature", s["sig"].as_str().unwrap());
    } else {
        builder = builder.header("x-api-key", "test-api-key");
    }
    let response = app
        .oneshot(builder.body(Body::from(body.to_string())).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null),
    )
}

fn nonce(n: u32) -> String {
    format!("write-test-nonce-{n:016}")
}

#[tokio::test]
async fn a_caller_without_the_write_scope_cannot_reach_a_write_tool() {
    let state = state();
    let app = crate::router(state);
    let signed = sign("agent-reader", &[READ_SCOPE], &nonce(1), crate::now());
    let (status, body) = call(
        app,
        "note_create",
        serde_json::json!({"article_id": 1, "body": "hello"}),
        Some(signed),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let error = &body["error"];
    assert_eq!(error["code"], -32601, "{body}");
    assert_eq!(
        error["message"], crate::mcp::UNKNOWN_TOOL,
        "a missing write scope must look exactly like an unknown tool"
    );
}

#[tokio::test]
async fn a_write_caller_creates_a_note_attributed_to_itself() {
    let state = state();
    let app = crate::router(state.clone());
    let signed = sign(
        "agent-writer",
        &[READ_SCOPE, WRITE_SCOPE],
        &nonce(2),
        crate::now(),
    );
    let (status, body) = call(
        app,
        "note_create",
        serde_json::json!({"article_id": 1, "body": "First note"}),
        Some(signed),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["result"]["isError"], false, "{body}");

    let db = state.db.lock().unwrap();
    let (author, text): (Option<String>, String) = db
        .conn
        .query_row(
            "SELECT author_client,body FROM newsman_notes WHERE article_id=1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(author.as_deref(), Some("agent-writer"));
    assert_eq!(text, "First note");
    let logged: i64 = db
        .conn
        .query_row(
            "SELECT COUNT(*) FROM newsman_write_log WHERE tool='note_create' AND client_id='agent-writer'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(logged, 1, "every write is logged with the caller");
}

#[tokio::test]
async fn an_agent_cannot_touch_the_owners_note_or_another_agents() {
    let state = state();
    {
        let db = state.db.lock().unwrap();
        db.conn
            .execute(
                "INSERT INTO newsman_notes(id,article_id,body,author_client,created_at,updated_at)
                 VALUES (10,1,'owner note',NULL,0,0), (11,1,'other agent',  'agent-b',0,0)",
                [],
            )
            .unwrap();
    }
    let agent_a = caller("agent-a", &[READ_SCOPE, WRITE_SCOPE]);

    let owner_note = note_update(&state, &agent_a, &serde_json::json!({"id":10,"body":"hijacked"}))
        .expect_err("an owner note must not be editable");
    assert!(owner_note.contains("owner"), "{owner_note}");

    let foreign = note_delete(&state, &agent_a, &serde_json::json!({"id":11}))
        .expect_err("another agent's note must not be deletable");
    assert!(foreign.contains("another caller"), "{foreign}");

    // And their own works.
    let own = note_create(&state, &agent_a, &serde_json::json!({"article_id":2,"body":"mine"}))
        .expect("an agent may write its own note");
    let id = own["id"].as_i64().unwrap();
    note_update(&state, &agent_a, &serde_json::json!({"id":id,"body":"edited"}))
        .expect("an agent may edit its own note");
    note_delete(&state, &agent_a, &serde_json::json!({"id":id})).expect("and delete it");
}

#[test]
fn lengths_are_bounded() {
    let state = state();
    let agent = caller("agent-a", &[READ_SCOPE, WRITE_SCOPE]);
    let too_long = "x".repeat(MAX_NOTE_CHARS + 1);
    let error = note_create(
        &state,
        &agent,
        &serde_json::json!({"article_id": 1, "body": too_long}),
    )
    .err()
    .unwrap();
    assert_eq!(error, "too_long");

    let long_title = "t".repeat(MAX_TITLE_CHARS + 1);
    let error = analysis_create(
        &state,
        &agent,
        &serde_json::json!({"title": long_title, "body": "ok"}),
    )
    .err()
    .unwrap();
    assert_eq!(error, "too_long");

    let long_flag = "f".repeat(MAX_FLAG_CHARS + 1);
    let error = article_flag(
        &state,
        &agent,
        &serde_json::json!({"article_id": 1, "label": long_flag}),
    )
    .err()
    .unwrap();
    assert_eq!(error, "too_long");
}

#[test]
fn a_note_needs_exactly_one_target() {
    let state = state();
    let agent = caller("agent-a", &[READ_SCOPE, WRITE_SCOPE]);
    let neither = note_create(&state, &agent, &serde_json::json!({"body": "x"}))
        .err()
        .unwrap();
    assert!(neither.contains("exactly one"), "{neither}");
    let both = note_create(
        &state,
        &agent,
        &serde_json::json!({"article_id": 1, "document_id": 1, "body": "x"}),
    )
    .err()
    .unwrap();
    assert!(both.contains("exactly one"), "{both}");
    // An id that does not exist is refused rather than stored as a dangling note.
    let missing = note_create(
        &state,
        &agent,
        &serde_json::json!({"article_id": 9999, "body": "x"}),
    )
    .err()
    .unwrap();
    assert!(missing.contains("no article"), "{missing}");
}

#[test]
fn a_brief_can_only_cite_stored_articles() {
    let state = state();
    let agent = caller("agent-a", &[READ_SCOPE, WRITE_SCOPE]);
    let value = analysis_create(
        &state,
        &agent,
        &serde_json::json!({
            "title": "Weekly crypto brief",
            "body": "BTC moved.",
            "topic_slug": "crypto",
            "source_article_ids": [1, 2]
        }),
    )
    .expect("a brief citing stored articles is allowed");
    assert_eq!(value["sources"], 2);

    let error = analysis_create(
        &state,
        &agent,
        &serde_json::json!({
            "title": "Bad brief",
            "body": "x",
            "source_article_ids": [1, 424242]
        }),
    )
    .err()
    .unwrap();
    assert!(error.contains("no article"), "{error}");

    let error = analysis_create(
        &state,
        &agent,
        &serde_json::json!({"title": "Bad", "body": "x", "source_article_ids": "1,2"}),
    )
    .err()
    .unwrap();
    assert_eq!(error, "invalid_sources");
}

#[test]
fn a_brief_cannot_point_at_a_disabled_region_or_topic() {
    let state = state();
    let agent = caller("agent-a", &[READ_SCOPE, WRITE_SCOPE]);
    let error = analysis_create(
        &state,
        &agent,
        &serde_json::json!({"title": "x", "body": "y", "region_code": "us"}),
    )
    .err()
    .unwrap();
    assert!(error.contains("not enabled"), "{error}");

    let error = analysis_create(
        &state,
        &agent,
        &serde_json::json!({"title": "x", "body": "y", "topic_slug": "nope"}),
    )
    .err()
    .unwrap();
    assert!(error.contains("not enabled"), "{error}");
}

#[test]
fn flagging_twice_is_one_flag() {
    let state = state();
    let agent = caller("agent-a", &[READ_SCOPE, WRITE_SCOPE]);
    let first = article_flag(
        &state,
        &agent,
        &serde_json::json!({"article_id": 1, "label": "needs analysis"}),
    )
    .unwrap();
    assert_eq!(first["created"], true);
    let second = article_flag(
        &state,
        &agent,
        &serde_json::json!({"article_id": 1, "label": "needs analysis"}),
    )
    .unwrap();
    assert_eq!(second["created"], false);
    let db = state.db.lock().unwrap();
    let count: i64 = db
        .conn
        .query_row("SELECT COUNT(*) FROM newsman_article_flags", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(count, 1);
}

#[test]
fn an_unknown_argument_is_refused_so_author_cannot_be_forged() {
    // `deny_unknown_fields` on the tool schema is what stops a caller passing
    // `"author": "louis"`; this pins the schema rather than the dispatch.
    let tools = crate::mcp::tools();
    for tool in tools
        .iter()
        .filter(|tool| crate::mcp::is_write_tool(tool.name))
    {
        assert_eq!(
            tool.input["additionalProperties"], false,
            "{} must refuse unknown arguments",
            tool.name
        );
        let properties = tool.input["properties"].as_object().unwrap();
        assert!(
            !properties.contains_key("author") && !properties.contains_key("author_client"),
            "{} must not accept an author argument",
            tool.name
        );
    }
}

#[test]
fn the_write_log_is_appended_and_read_back() {
    let state = state();
    let agent = caller("agent-a", &[READ_SCOPE, WRITE_SCOPE]);
    article_flag(
        &state,
        &agent,
        &serde_json::json!({"article_id": 1, "label": "used in brief 7"}),
    )
    .unwrap();
    let db = state.db.lock().unwrap();
    let rows = write_log(&db, 10).unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["client_id"], "agent-a");
    assert_eq!(rows[0]["tool"], "article_flag");
    assert_eq!(rows[0]["record_kind"], "article");
    assert_eq!(rows[0]["note"], "used in brief 7");
}

#[test]
fn notes_and_analyses_are_readable_with_the_author_label() {
    let state = state();
    let agent = caller("agent-a", &[READ_SCOPE, WRITE_SCOPE]);
    note_create(&state, &agent, &serde_json::json!({"article_id": 1, "body": "one"})).unwrap();
    analysis_create(
        &state,
        &agent,
        &serde_json::json!({"title": "T", "body": "B", "source_article_ids": [1]}),
    )
    .unwrap();
    let db = state.db.lock().unwrap();
    let notes = notes_for(&db, Some(1), None, 10).unwrap();
    assert_eq!(notes.len(), 1);
    assert_eq!(notes[0]["author"], "agent-a");
    let analyses = analysis_list(&db, None, None, 10).unwrap();
    assert_eq!(analyses.len(), 1);
    assert_eq!(analyses[0]["author"], "agent-a");
    assert_eq!(analyses[0]["source_article_ids"][0], 1);
}

/// `additionalProperties: false` in a schema is a note to the client; nothing enforces
/// it. Ignoring an undeclared key lets a caller believe it set something, so the
/// server refuses one. `author` is the case that matters.
#[tokio::test]
async fn an_undeclared_argument_is_refused_not_ignored() {
    let state = state();
    let app = crate::router(state);
    let signed = sign(
        "agent-writer",
        &[READ_SCOPE, WRITE_SCOPE],
        &nonce(9),
        crate::now(),
    );
    let (status, body) = call(
        app,
        "note_create",
        serde_json::json!({"article_id": 1, "body": "x", "author": "louis"}),
        Some(signed),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["result"]["isError"], true, "{body}");
    let message = body["result"]["content"][0]["text"].as_str().unwrap();
    assert!(message.contains("unknown argument"), "{message}");
    assert!(message.contains("author"), "{message}");
}
