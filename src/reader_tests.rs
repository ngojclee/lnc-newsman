//! Tests for reader-facing analyses, two-way owner feedback notes, preferences and
//! grounded Hub research (`POST /api/admin/research`).

use super::*;
use crate::{AppState, config::Config, connector::Caller, secrets::SecretBox};
use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode, header},
    routing::post,
};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use std::sync::{Arc, Mutex};
use tower::ServiceExt;

fn test_state(fetcher: crate::fetch::Fetcher) -> SharedState {
    let dir = std::env::temp_dir().join(format!(
        "lnc-newsman-reader-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let db = db::Database::in_memory().unwrap();
    db.conn
        .execute(
            "INSERT INTO sources(slug,name,url,tier,enabled,region)
             VALUES ('wire','Global Wire','https://example.invalid/rss','T1',1,'global')",
            [],
        )
        .unwrap();
    let source_id: i64 = db
        .conn
        .query_row("SELECT id FROM sources WHERE slug='wire'", [], |r| r.get(0))
        .unwrap();
    db.conn
        .execute(
            "INSERT INTO articles(canonical_url,title,source_id,topic_slug,published_at,fetched_at,region_code)
             VALUES ('https://example.invalid/1','Rate decision holds steady',?1,'finance','2026-11-12T08:00:00Z',100,'global')",
            [source_id],
        )
        .unwrap();
    Arc::new(AppState::new(
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
        fetcher,
    ))
}

async fn send(
    app: &Router,
    method: &str,
    path: &str,
    body: Option<Value>,
    cookie: Option<&str>,
    api_key: Option<&str>,
    csrf: bool,
) -> (StatusCode, Value) {
    let mut builder = Request::builder()
        .method(method)
        .uri(path)
        .header(header::HOST, "newsman.test")
        .header(header::CONTENT_TYPE, "application/json");
    if let Some(cookie) = cookie {
        builder = builder.header(header::COOKIE, cookie);
    }
    if let Some(key) = api_key {
        builder = builder.header(header::AUTHORIZATION, format!("Bearer {key}"));
    }
    if csrf {
        builder = builder.header("x-requested-with", "XMLHttpRequest");
    }
    let response = app
        .clone()
        .oneshot(
            builder
                .body(Body::from(body.map_or_else(String::new, |v| v.to_string())))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

async fn login(app: &Router) -> String {
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/auth/login")
                .header(header::HOST, "newsman.test")
                .header(header::CONTENT_TYPE, "application/json")
                .header("x-requested-with", "XMLHttpRequest")
                .body(Body::from(r#"{"key":"test-management-key"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    response
        .headers()
        .get(header::SET_COOKIE)
        .unwrap()
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_string()
}

#[test]
fn preferences_validation_accepts_valid_values_and_rejects_bad_clock_or_timezone() {
    let ok = validate_preferences_patch(
        Some("  Ưu tiên lãi suất FED, tỷ giá VND và thuế DN  "),
        Some("07:15"),
        Some(""),
        Some("Asia/Ho_Chi_Minh"),
    )
    .expect("valid preferences");
    assert_eq!(ok.len(), 4);
    assert_eq!(
        ok[0],
        (
            FOCUS_PROFILE_KEY,
            "Ưu tiên lãi suất FED, tỷ giá VND và thuế DN".to_string()
        )
    );
    assert_eq!(
        validate_preferences_patch(None, Some("25:00"), None, None),
        Err("invalid_brief_time")
    );
    assert_eq!(
        validate_preferences_patch(None, Some("7:30"), None, None),
        Err("invalid_brief_time")
    );
    assert_eq!(
        validate_preferences_patch(None, None, None, Some("Mars/Olympus")),
        Err("invalid_brief_timezone")
    );
    assert_eq!(
        validate_preferences_patch(Some("bad\u{0007}bell"), None, None, None),
        Err("invalid_focus_profile")
    );
}

#[tokio::test]
async fn owner_notes_are_protected_from_agent_edits_and_deletable_by_owner() {
    let state = test_state(crate::fetch::Fetcher::public_only());
    let agent = Caller {
        client_id: "analyst-bot".into(),
        app_id: None,
        scopes: vec![
            crate::connector::READ_SCOPE.into(),
            crate::connector::WRITE_SCOPE.into(),
        ],
        delegated_user: None,
        resource_ctx: None,
    };
    let created = write::analysis_create(
        &state,
        &agent,
        &json!({
            "title": "Morning Macro Brief",
            "body": "Rates stayed unchanged.",
            "kind": "brief",
            "topic_slug": "finance",
            "region_code": "global",
            "source_article_ids": [1]
        }),
    )
    .unwrap();
    let analysis_id = created["id"].as_i64().unwrap();

    let app = crate::router(state.clone());
    let cookie = login(&app).await;

    // Owner writes feedback on the brief from the web UI (`author_client = NULL`).
    let (status, payload) = send(
        &app,
        "POST",
        "/api/notes",
        Some(json!({
            "analysis_id": analysis_id,
            "body": "Cần theo dõi thêm tác động lên tỷ giá USD/VND tuần tới."
        })),
        Some(&cookie),
        None,
        true,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{payload}");
    let note_id = payload["data"]["id"].as_i64().unwrap();

    // Delegated MCP agent cannot overwrite or delete the owner's note.
    assert!(
        write::note_update(
            &state,
            &agent,
            &json!({"id": note_id, "body": "agent tamper"})
        )
        .is_err()
    );
    assert!(write::note_delete(&state, &agent, &json!({"id": note_id})).is_err());

    // Reader REST v1 returns the brief, its resolved cited article and the owner's note.
    let (status, detail) = send(
        &app,
        "GET",
        &format!("/api/v1/analyses/{analysis_id}"),
        None,
        None,
        Some("test-api-key"),
        false,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{detail}");
    assert_eq!(
        detail["data"]["stable_key"],
        format!("analysis-{analysis_id}")
    );
    assert_eq!(detail["cited_articles"][0]["id"], 1);
    assert_eq!(detail["notes"][0]["id"], note_id);
    assert!(detail["notes"][0]["author"].is_null());

    // `GET /api/v1/notes` also returns the note for `analysis_id`.
    let (status, notes_resp) = send(
        &app,
        "GET",
        &format!("/api/v1/notes?analysis_id={analysis_id}"),
        None,
        Some(&cookie),
        None,
        false,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(notes_resp["data"].as_array().unwrap().len(), 1);

    // Owner can delete the note from `/api/notes/{id}`.
    let (status, _) = send(
        &app,
        "DELETE",
        &format!("/api/notes/{note_id}"),
        None,
        Some(&cookie),
        None,
        true,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let (_, after) = send(
        &app,
        "GET",
        &format!("/api/v1/notes?analysis_id={analysis_id}"),
        None,
        Some(&cookie),
        None,
        false,
    )
    .await;
    assert!(after["data"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn research_endpoint_grounds_prompt_in_stored_articles_and_saves_brief_when_requested() {
    let prompts: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let p_chat = prompts.clone();
    let p_agent = prompts.clone();
    let hub_mock = Router::new()
        .route(
            "/chat/completions",
            post(move |body: String| {
                let p = p_chat.clone();
                async move {
                    p.lock().unwrap().push(format!("chat:{body}"));
                    (
                        StatusCode::OK,
                        axum::Json(json!({
                            "choices": [{"message": {"content": "## Tóm tắt\nLãi suất giữ nguyên theo [#1]."}}],
                            "usage": {"total_tokens": 88}
                        })),
                    )
                }
            }),
        )
        .route(
            "/agents/news-analyst-argus/run",
            post(move |body: String| {
                let p = p_agent.clone();
                async move {
                    p.lock().unwrap().push(format!("agent:{body}"));
                    (
                        StatusCode::OK,
                        axum::Json(json!({
                            "output": "## Phân tích sâu\nĐánh giá rủi ro tỷ giá từ [#1].",
                            "usage": {"total_tokens": 140}
                        })),
                    )
                }
            }),
        );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, hub_mock).await.unwrap() });

    let mut state = test_state(crate::fetch::Fetcher::allow_loopback_for_tests());
    {
        let inner = Arc::get_mut(&mut state).unwrap();
        inner.hub = hub::ProviderConfig {
            base_url: Some(format!("http://{addr}")),
            allowed_hosts: vec!["127.0.0.1".into()],
            model_classify: "cheap".into(),
            model_summarise: "default".into(),
            model_analyse: "strong".into(),
            agent_id: "news-analyst-argus".into(),
            timeout_seconds: 5,
        };
        let vault = SecretBox::from_bytes([9; 32], None);
        let mut db = inner.db.lock().unwrap();
        crate::secrets::set(&db, &vault, hub::SECRET_NAME, "hub-secret").unwrap();
        db.set_settings(&[(FOCUS_PROFILE_KEY, "Tập trung lãi suất FED và tỷ giá VND")])
            .unwrap();
        drop(db);
        inner.secrets = Some(vault);
    }

    let app = crate::router(state);
    let cookie = login(&app).await;

    let (status, resp) = send(
        &app,
        "POST",
        "/api/admin/research",
        Some(json!({
            "question": "Điểm chính hôm nay là gì?",
            "topic": "finance",
            "mode": "analyse",
            "save_as_brief": true,
            "title": "Bản tin Tài chính Sáng"
        })),
        Some(&cookie),
        None,
        true,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{resp}");
    assert!(resp["data"]["answer"].as_str().unwrap().contains("[#1]"));
    let saved_id = resp["data"]["saved_analysis_id"].as_i64().unwrap();
    assert_eq!(resp["data"]["stable_key"], format!("analysis-{saved_id}"));

    // Agent mode works too without saving when save_as_brief is omitted.
    let (status, agent_resp) = send(
        &app,
        "POST",
        "/api/admin/research",
        Some(json!({
            "question": "Phân tích sâu rủi ro tỷ giá",
            "mode": "agent"
        })),
        Some(&cookie),
        None,
        true,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{agent_resp}");
    assert!(agent_resp["data"]["saved_analysis_id"].is_null());
    assert_eq!(agent_resp["data"]["model"], "news-analyst-argus");

    {
        let recorded = prompts.lock().unwrap();
        assert_eq!(recorded.len(), 2);
        assert!(recorded[0].contains("Rate decision holds steady"));
        assert!(recorded[0].contains("Tập trung lãi suất FED và tỷ giá VND"));
    }

    // Saved brief appears on `GET /api/v1/analyses`.
    let (status, list_resp) = send(
        &app,
        "GET",
        "/api/v1/analyses?limit=5",
        None,
        Some(&cookie),
        None,
        false,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(list_resp["data"][0]["id"], saved_id);
    assert_eq!(list_resp["data"][0]["title"], "Bản tin Tài chính Sáng");
}
