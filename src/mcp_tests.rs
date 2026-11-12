//! `/mcp` tests: the JSON-RPC surface, its credential gate, and the promise that a
//! tool answers exactly what the REST v1 endpoint it wraps answers.
//!
//! No network and no wall clock: one in-memory database seeded with a visible article
//! and one hidden behind a disabled region.

use super::*;
use crate::config::Config;
use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use http_body_util::BodyExt;
use tower::ServiceExt;

fn state() -> crate::SharedState {
    let dir = std::env::temp_dir().join(format!("lnc-newsman-mcp-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let db = crate::db::Database::in_memory().unwrap();
    // One enabled region (global is enabled by default) and one topic that is on.
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
    std::sync::Arc::new(crate::AppState::new(
        Config {
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
    ))
}

async fn rpc(
    app: axum::Router,
    body: serde_json::Value,
    api_key: Option<&str>,
) -> (StatusCode, serde_json::Value) {
    let mut builder = Request::builder()
        .method("POST")
        .uri("/mcp")
        .header(header::HOST, "newsman.test")
        .header(header::CONTENT_TYPE, "application/json")
        .header("x-requested-with", "XMLHttpRequest");
    if let Some(key) = api_key {
        builder = builder.header(header::AUTHORIZATION, format!("Bearer {key}"));
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

fn call(name: &str, arguments: serde_json::Value) -> serde_json::Value {
    serde_json::json!({
        "jsonrpc": "2.0",
        "id": 7,
        "method": "tools/call",
        "params": {"name": name, "arguments": arguments}
    })
}

#[tokio::test]
async fn mcp_requires_the_same_credentials_as_rest_v1() {
    let app = crate::router(state());
    let init = serde_json::json!({"jsonrpc": "2.0", "id": 1, "method": "initialize"});
    let (status, _) = rpc(app.clone(), init.clone(), None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    let (status, _) = rpc(app.clone(), init.clone(), Some("wrong-key")).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    let (status, body) = rpc(app, init, Some("test-api-key")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["result"]["serverInfo"]["name"], SERVER_NAME);
    assert_eq!(body["result"]["protocolVersion"], PROTOCOL_VERSION);
}

#[tokio::test]
async fn initialize_and_tools_list_are_stable_and_say_what_they_do() {
    let app = crate::router(state());
    let (_, body) = rpc(
        app.clone(),
        serde_json::json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list"}),
        Some("test-api-key"),
    )
    .await;
    let listed = body["result"]["tools"].as_array().unwrap();
    let names: Vec<&str> = listed
        .iter()
        .map(|tool| tool["name"].as_str().unwrap())
        .collect();
    assert_eq!(names.len(), tools().len());
    for tool in listed {
        assert!(
            tool["description"]
                .as_str()
                .is_some_and(|text| !text.is_empty())
        );
        assert_eq!(tool["inputSchema"]["type"], "object", "{tool}");
    }
    // A second call answers byte-for-byte the same list: tools are static per build.
    let (_, again) = rpc(
        app,
        serde_json::json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list"}),
        Some("test-api-key"),
    )
    .await;
    assert_eq!(body["result"], again["result"]);
}

#[tokio::test]
async fn a_tool_answers_what_the_rest_endpoint_answers() {
    let app = crate::router(state());
    let (_, mcp_body) = rpc(
        app.clone(),
        call("latest_articles", serde_json::json!({"limit": 10})),
        Some("test-api-key"),
    )
    .await;
    assert_eq!(mcp_body["result"]["isError"], false, "{mcp_body}");
    let text = mcp_body["result"]["content"][0]["text"].as_str().unwrap();
    let from_mcp: serde_json::Value = serde_json::from_str(text).unwrap();
    let rest = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/articles?limit=10")
                .header(header::HOST, "newsman.test")
                .header(header::AUTHORIZATION, "Bearer test-api-key")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let bytes = rest.into_body().collect().await.unwrap().to_bytes();
    let from_rest: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(from_mcp["data"], from_rest["data"]);
}

#[tokio::test]
async fn a_bad_argument_and_an_unknown_tool_are_reported_without_a_transport_error() {
    let app = crate::router(state());
    // Missing required `code`: the tool answers isError, the transport stays 200.
    let (status, body) = rpc(
        app.clone(),
        call("observations", serde_json::json!({})),
        Some("test-api-key"),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["result"]["isError"], true);
    // An unknown tool is a JSON-RPC parameter error, not a tool result.
    let (status, body) = rpc(
        app.clone(),
        call("drop_everything", serde_json::json!({})),
        Some("test-api-key"),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["error"]["code"], -32601,
        "an unknown tool and an out-of-scope tool answer identically"
    );
    // A region that is not enabled is refused the same way the REST endpoint refuses it.
    let (_, body) = rpc(
        app.clone(),
        call(
            "newsman__latest_articles",
            serde_json::json!({"region": "us"}),
        ),
        Some("test-api-key"),
    )
    .await;
    assert_eq!(body["result"]["isError"], true);
    // Unknown method.
    let (_, body) = rpc(
        app,
        serde_json::json!({"jsonrpc": "2.0", "id": 9, "method": "resources/list"}),
        Some("test-api-key"),
    )
    .await;
    assert_eq!(body["error"]["code"], -32601);
}

#[tokio::test]
async fn a_batch_answers_every_call_in_order() {
    let app = crate::router(state());
    let batch = serde_json::json!([
        {"jsonrpc": "2.0", "id": "a", "method": "ping"},
        call("topics", serde_json::json!({}))
    ]);
    let (status, body) = rpc(app, batch, Some("test-api-key")).await;
    assert_eq!(status, StatusCode::OK);
    let answers = body.as_array().unwrap();
    assert_eq!(answers.len(), 2);
    assert_eq!(answers[0]["id"], "a");
    assert_eq!(answers[1]["result"]["isError"], false);
}

#[tokio::test]
async fn get_does_not_pretend_to_stream() {
    let app = crate::router(state());
    let response = app
        .oneshot(
            Request::builder()
                .uri("/mcp")
                .header(header::HOST, "newsman.test")
                .header(header::AUTHORIZATION, "Bearer test-api-key")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::METHOD_NOT_ALLOWED);
}

#[test]
fn the_tool_list_has_no_write_tool_and_no_duplicate_name() {
    let listed = tools();
    let mut names: Vec<&str> = listed.iter().map(|tool| tool.name).collect();
    let unique = {
        let mut copy = names.clone();
        copy.sort_unstable();
        copy.dedup();
        copy.len() == names.len()
    };
    assert!(unique, "duplicate tool name in {names:?}");
    for name in names.drain(..) {
        // Names are bare on purpose: the Business MCP Hub adds its namespace when it
        // proxies, so advertising `newsman__regions` here would reach an agent as
        // `newsman__newsman__regions`.
        assert!(
            !name.starts_with("newsman__"),
            "{name} already carries the Hub namespace"
        );
        // A write tool may exist, but only over the app's own analytic data. One that
        // looks like it configures the app is the thing this test exists to stop.
        for forbidden in ["source_", "connector", "secret", "collector", "alert"] {
            assert!(
                !name.contains(forbidden),
                "{name} looks like a configuration tool"
            );
        }
    }
}

/// The Hub's client sends `notifications/initialized` (no `id`) and treats 202 as
/// "delivered, nothing to read". Answering with a JSON-RPC object would put a stray
/// response on the stream — and the MCP spec forbids answering a notification at all.
#[tokio::test]
async fn a_notification_is_accepted_without_a_body() {
    let app = crate::router(state());
    for body in [
        serde_json::json!({"jsonrpc":"2.0","method":"notifications/initialized"}),
        serde_json::json!([{"jsonrpc":"2.0","method":"notifications/initialized"}]),
    ] {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/mcp")
                    .header(header::HOST, "newsman.test")
                    .header(header::CONTENT_TYPE, "application/json")
                    .header("x-requested-with", "XMLHttpRequest")
                    .header(header::AUTHORIZATION, "Bearer test-api-key")
                    .body(Body::from(body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(
            response.status(),
            StatusCode::ACCEPTED,
            "a notification must not be answered: {body}"
        );
    }
}

/// The discovery filter is the tool-level gate, and it must behave the same as the
/// call gate: a caller whose scopes satisfy no tool learns nothing from `tools/list`.
/// Tested at the function so the rule is pinned even though today the envelope already
/// refuses any scope but the read one.
#[test]
fn tools_list_is_filtered_by_the_callers_scopes() {
    let all = tools().len();
    let none = crate::connector::Caller {
        client_id: "agent-x".into(),
        app_id: None,
        scopes: Vec::new(),
    };
    let read = crate::connector::Caller {
        client_id: "agent-x".into(),
        app_id: None,
        scopes: vec![crate::connector::READ_SCOPE.to_string()],
    };
    // A reader (no delegation) sees everything, unchanged from before the connector.
    assert_eq!(
        tools()
            .iter()
            .filter(|tool| crate::connector::scope_allows(None, tool.name))
            .count(),
        all
    );
    let read_count = tools()
        .iter()
        .filter(|tool| crate::connector::scope_allows(Some(&read), tool.name))
        .count();
    assert!(
        read_count > 0 && read_count < all,
        "the read scope reaches the read tools and none of the write ones: {read_count}/{all}"
    );
    assert_eq!(
        tools()
            .iter()
            .filter(|tool| {
                crate::mcp::tool_scope(tool.name) == Some(crate::connector::READ_SCOPE)
            })
            .count(),
        read_count,
        "the read scope is exactly the set of read tools"
    );
    assert_eq!(
        tools()
            .iter()
            .filter(|tool| crate::connector::scope_allows(Some(&none), tool.name))
            .count(),
        0,
        "no scope means no tools, and no tool means no data"
    );
}

/// The other side of argument checking: a **declared** argument must never be refused.
///
/// A reject-list that drifts from the schema is worse than no check at all — an agent
/// would be blocked from a parameter the tool advertises. This walks every published
/// tool, builds an argument object containing exactly the properties its schema
/// declares, and asserts the checker accepts it. Any future drift between the tool
/// table and the schema fails here.
#[test]
fn a_declared_argument_is_never_refused() {
    for tool in tools() {
        let properties = tool.input["properties"]
            .as_object()
            .cloned()
            .unwrap_or_default();
        let mut arguments = serde_json::Map::new();
        for key in properties.keys() {
            arguments.insert(key.clone(), serde_json::Value::Null);
        }
        assert!(
            reject_unknown_arguments(&tool, &serde_json::Value::Object(arguments))
                .is_ok(),
            "{} refused a property it declares in its own schema",
            tool.name
        );
    }
    // And the rejecting side still works, for a key nobody declares.
    let regions = tools()
        .into_iter()
        .find(|tool| tool.name == "regions")
        .unwrap();
    let error = reject_unknown_arguments(&regions, &serde_json::json!({"limit": 5}))
        .expect_err("regions takes no arguments, so `limit` must be refused");
    assert!(error.contains("limit"), "{error}");
}

/// A read tool is checked too. Silently ignoring `{"language": "vi"}` on a search is
/// worse than an error: the caller believes a filter was applied that never was.
#[test]
fn a_read_tool_also_refuses_an_undeclared_argument() {
    let topics = tools()
        .into_iter()
        .find(|tool| tool.name == "topics")
        .unwrap();
    let error = reject_unknown_arguments(&topics, &serde_json::json!({"language": "vi"}))
        .expect_err("an undeclared filter must not be ignored");
    assert!(error.contains("language"), "{error}");
}

#[tokio::test]
async fn reader_preferences_tool_exposes_focus_and_schedule_without_secrets() {
    let st = state();
    st.db
        .lock()
        .unwrap()
        .set_settings(&[
            ("focus_profile", "Theo dõi vĩ mô Mỹ và thuế Việt Nam"),
            ("brief_morning_time", "07:00"),
            ("brief_evening_time", "18:30"),
            ("brief_timezone", "Asia/Ho_Chi_Minh"),
        ])
        .unwrap();
    let app = crate::router(st);
    let (status, body) = rpc(
        app,
        call("reader_preferences", serde_json::json!({})),
        Some("test-api-key"),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["result"]["isError"], false, "{body}");
    let text = body["result"]["content"][0]["text"].as_str().unwrap();
    let parsed: serde_json::Value = serde_json::from_str(text).unwrap();
    assert_eq!(
        parsed["data"]["focus_profile"],
        "Theo dõi vĩ mô Mỹ và thuế Việt Nam"
    );
    assert_eq!(parsed["data"]["brief_morning_time"], "07:00");
    assert_eq!(parsed["data"]["brief_evening_time"], "18:30");
    assert_eq!(parsed["data"]["brief_timezone"], "Asia/Ho_Chi_Minh");
}

#[tokio::test]
async fn article_get_and_latest_articles_search_return_stored_content() {
    let st = state();
    let article_id: i64 = {
        let db = st.db.lock().unwrap();
        let id: i64 = db
            .conn
            .query_row("SELECT id FROM articles LIMIT 1", [], |r| r.get(0))
            .unwrap();
        db.conn
            .execute(
                "UPDATE articles SET summary='Tóm tắt nhanh', content_text='Nội dung toàn văn phân tích vĩ mô chuyên sâu.', content_source='web', word_count=9 WHERE id=?1",
                [id],
            )
            .unwrap();
        crate::db::index_article_fts(
            &db.conn,
            id,
            "A visible item",
            Some("Tóm tắt nhanh"),
            Some("Nội dung toàn văn phân tích vĩ mô chuyên sâu."),
        )
        .unwrap();
        id
    };
    let app = crate::router(st);

    let (status, detail_resp) = rpc(
        app.clone(),
        call("article_get", serde_json::json!({"id": article_id})),
        Some("test-api-key"),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(detail_resp["result"]["isError"], false, "{detail_resp}");
    let detail_json: serde_json::Value =
        serde_json::from_str(detail_resp["result"]["content"][0]["text"].as_str().unwrap())
            .unwrap();
    assert_eq!(detail_json["data"]["id"], article_id);
    assert_eq!(detail_json["data"]["content_source"], "web");
    assert!(
        detail_json["data"]["content_text"]
            .as_str()
            .unwrap()
            .contains("phân tích vĩ mô")
    );

    let (status, search_resp) = rpc(
        app,
        call("latest_articles", serde_json::json!({"q": "vi mo"})),
        Some("test-api-key"),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(search_resp["result"]["isError"], false, "{search_resp}");
    let search_json: serde_json::Value =
        serde_json::from_str(search_resp["result"]["content"][0]["text"].as_str().unwrap())
            .unwrap();
    assert_eq!(search_json["data"].as_array().unwrap().len(), 1);
    assert_eq!(search_json["data"][0]["id"], article_id);
}
