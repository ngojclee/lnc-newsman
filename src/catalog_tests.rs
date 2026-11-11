use super::*;
use axum::{
    body::Body,
    http::{Request, header},
};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use tower::ServiceExt;

fn app() -> Router {
    router(Arc::new(AppState::new(
        config::Config {
            data_dir: ".".into(),
            regions_dir: ".".into(),
            bind_address: "127.0.0.1".parse().unwrap(),
            port: 8000,
            management_key: Some("test-management-key".into()),
            api_keys: vec!["test-api-key".into()],
            allowed_hosts: vec!["newsman.test".into()],
            trusted_proxies: Vec::new(),
            secure_cookie: true,
            version: "test",
        },
        db::Database::in_memory().unwrap(),
        fetch::Fetcher::public_only(),
    )))
}

async fn request(
    app: &Router,
    method: &str,
    path: &str,
    body: Option<Value>,
    cookie: Option<&str>,
    api_key: Option<&str>,
    csrf: bool,
) -> Response {
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
    app.clone()
        .oneshot(
            builder
                .body(Body::from(body.map_or_else(String::new, |v| v.to_string())))
                .unwrap(),
        )
        .await
        .unwrap()
}

async fn session(app: &Router) -> String {
    let response = request(
        app,
        "POST",
        "/api/auth/login",
        Some(json!({"key":"test-management-key"})),
        None,
        None,
        true,
    )
    .await;
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

async fn body(response: Response) -> Value {
    serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes()).unwrap()
}

fn topic(slug: &str, parent: Option<&str>, enabled: bool) -> Value {
    json!({"slug":slug,"label_vi":"Chủ đề thử","label_en":"Test topic","parent_slug":parent,"enabled":enabled})
}

fn source(url: &str, enabled: bool, topic_slug: Option<&str>) -> Value {
    json!({"slug":"test-source","name":"Test source","url":url,"tier":"T2","enabled":enabled,"language":"vi","region":"","topic_slug":topic_slug,"fetch_interval_minutes":60})
}

#[tokio::test]
async fn management_catalog_requires_session_and_csrf() {
    let app = app();
    for path in ["/api/admin/topics", "/api/admin/sources"] {
        let response = request(&app, "GET", path, None, None, Some("test-api-key"), false).await;
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }
    let response = request(
        &app,
        "POST",
        "/api/admin/topics",
        Some(topic("new-topic", None, true)),
        None,
        Some("test-api-key"),
        true,
    )
    .await;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    let cookie = session(&app).await;
    let response = request(
        &app,
        "POST",
        "/api/admin/topics",
        Some(topic("new-topic", None, true)),
        Some(&cookie),
        None,
        false,
    )
    .await;
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn catalog_create_edit_archive_and_reader_filter() {
    let app = app();
    let cookie = session(&app).await;
    let response = request(
        &app,
        "POST",
        "/api/admin/topics",
        Some(topic("central-banks", Some("finance"), true)),
        Some(&cookie),
        None,
        true,
    )
    .await;
    assert_eq!(response.status(), StatusCode::CREATED);
    let response = request(
        &app,
        "POST",
        "/api/admin/sources",
        Some(source(
            "https://example.invalid/feed.xml",
            true,
            Some("central-banks"),
        )),
        Some(&cookie),
        None,
        true,
    )
    .await;
    assert_eq!(response.status(), StatusCode::CREATED);
    let id = body(response).await["data"]["id"].as_i64().unwrap();
    let response = request(
        &app,
        "PUT",
        "/api/admin/topics/central-banks",
        Some(topic("central-banks", Some("finance"), false)),
        Some(&cookie),
        None,
        true,
    )
    .await;
    assert_eq!(response.status(), StatusCode::CONFLICT);
    assert_eq!(body(response).await["error"], "topic_in_use");
    let path = format!("/api/admin/sources/{id}");
    let response = request(
        &app,
        "PUT",
        &path,
        Some(source(
            "https://example.invalid/feed.xml",
            false,
            Some("central-banks"),
        )),
        Some(&cookie),
        None,
        true,
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let response = request(
        &app,
        "PUT",
        "/api/admin/topics/central-banks",
        Some(topic("central-banks", Some("finance"), false)),
        Some(&cookie),
        None,
        true,
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let response = request(
        &app,
        "GET",
        "/api/admin/topics",
        None,
        Some(&cookie),
        None,
        false,
    )
    .await;
    assert!(
        body(response).await["data"]
            .as_array()
            .unwrap()
            .iter()
            .any(|v| v["slug"] == "central-banks" && v["enabled"] == false)
    );
    let response = request(
        &app,
        "GET",
        "/api/v1/topics",
        None,
        None,
        Some("test-api-key"),
        false,
    )
    .await;
    assert!(
        !body(response).await["data"]
            .as_array()
            .unwrap()
            .iter()
            .any(|v| v["slug"] == "central-banks")
    );
}

#[tokio::test]
async fn catalog_rejects_cycles_duplicates_and_unsafe_urls() {
    let app = app();
    let cookie = session(&app).await;
    let response = request(
        &app,
        "POST",
        "/api/admin/topics",
        Some(topic("child", Some("finance"), true)),
        Some(&cookie),
        None,
        true,
    )
    .await;
    assert_eq!(response.status(), StatusCode::CREATED);
    let response = request(
        &app,
        "PUT",
        "/api/admin/topics/finance",
        Some(topic("finance", Some("child"), true)),
        Some(&cookie),
        None,
        true,
    )
    .await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert_eq!(body(response).await["error"], "topic_cycle");
    let response = request(
        &app,
        "POST",
        "/api/admin/topics",
        Some(topic("child", None, true)),
        Some(&cookie),
        None,
        true,
    )
    .await;
    assert_eq!(response.status(), StatusCode::CONFLICT);
    for url in [
        "file:///etc/passwd",
        "https://user:pass@example.invalid/feed",
        "https://example.invalid/feed?token=secret",
        "https://example.invalid/feed#fragment",
    ] {
        let response = request(
            &app,
            "POST",
            "/api/admin/sources",
            Some(source(url, false, None)),
            Some(&cookie),
            None,
            true,
        )
        .await;
        assert_eq!(response.status(), StatusCode::BAD_REQUEST, "accepted {url}");
    }
    let response = request(
        &app,
        "POST",
        "/api/admin/sources",
        Some(source("https://example.invalid/feed", false, None)),
        Some(&cookie),
        None,
        true,
    )
    .await;
    assert_eq!(response.status(), StatusCode::CREATED);
    let response = request(
        &app,
        "POST",
        "/api/admin/sources",
        Some(source("https://example.invalid/feed", false, None)),
        Some(&cookie),
        None,
        true,
    )
    .await;
    assert_eq!(response.status(), StatusCode::CONFLICT);
    let response = request(
        &app,
        "PUT",
        "/api/admin/sources/99999",
        Some(source("https://other.invalid/feed", false, None)),
        Some(&cookie),
        None,
        true,
    )
    .await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn disabled_child_remains_editable_when_parent_is_disabled() {
    let app = app();
    let cookie = session(&app).await;
    let response = request(
        &app,
        "POST",
        "/api/admin/topics",
        Some(topic("child", Some("finance"), false)),
        Some(&cookie),
        None,
        true,
    )
    .await;
    assert_eq!(response.status(), StatusCode::CREATED);
    let response = request(
        &app,
        "PUT",
        "/api/admin/topics/finance",
        Some(topic("finance", None, false)),
        Some(&cookie),
        None,
        true,
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let response = request(
        &app,
        "PUT",
        "/api/admin/topics/child",
        Some(topic("child", Some("finance"), false)),
        Some(&cookie),
        None,
        true,
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let response = request(
        &app,
        "PUT",
        "/api/admin/topics/child",
        Some(topic("child", Some("finance"), true)),
        Some(&cookie),
        None,
        true,
    )
    .await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert_eq!(body(response).await["error"], "invalid_parent");
}
