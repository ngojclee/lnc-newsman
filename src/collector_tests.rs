//! Collector tests against a loopback feed server. No external network is used.

use super::*;
use axum::{
    body::Body,
    http::{HeaderMap as Headers, Request as HttpRequest},
    routing::get,
};
use http_body_util::BodyExt;
use rusqlite::params;
use serde_json::json;
use std::time::{Duration, Instant};
use tower::ServiceExt;

const RSS: &str = r#"<?xml version="1.0"?><rss version="2.0"><channel><title>Mock</title>
<item><title>Lãi suất giữ nguyên</title><link>https://news.example.org/a/1?utm_source=rss</link><guid>a-1</guid><pubDate>Tue, 10 Nov 2026 08:00:00 GMT</pubDate><description>publisher body</description></item>
<item><title>Second item</title><link>https://news.example.org/a/2</link><guid>a-2</guid></item>
</channel></rss>"#;

async fn feed_server() -> String {
    let app = Router::new()
        .route(
            "/feed.xml",
            get(|headers: Headers| async move {
                if headers.get("if-none-match").and_then(|v| v.to_str().ok()) == Some("\"v1\"") {
                    return (
                        StatusCode::NOT_MODIFIED,
                        [("etag", "\"v1\"")],
                        String::new(),
                    );
                }
                (StatusCode::OK, [("etag", "\"v1\"")], RSS.to_string())
            }),
        )
        .route(
            "/moved",
            get(|| async { (StatusCode::MOVED_PERMANENTLY, [("location", "/feed.xml")]) }),
        )
        .route(
            "/loop",
            get(|| async { (StatusCode::FOUND, [("location", "/loop")]) }),
        )
        .route(
            "/broken",
            get(|| async { StatusCode::INTERNAL_SERVER_ERROR }),
        )
        .route(
            "/page",
            get(|| async { "<html><body>not a feed</body></html>" }),
        )
        .route(
            "/huge",
            get(|| async { "x".repeat(fetch::MAX_BODY_BYTES + 1) }),
        )
        .route(
            "/redirect-to",
            get(
                |axum::extract::Query(params): axum::extract::Query<
                    std::collections::HashMap<String, String>,
                >| async move {
                    match params.get("to") {
                        Some(to) => (StatusCode::FOUND, [("location", to.clone())], String::new()),
                        None => (
                            StatusCode::BAD_REQUEST,
                            [("location", String::new())],
                            String::new(),
                        ),
                    }
                },
            ),
        )
        .route(
            "/conditional-probe",
            get(|headers: Headers| async move {
                let saw = headers.contains_key("if-none-match")
                    || headers.contains_key("if-modified-since");
                (
                    StatusCode::OK,
                    [("etag", "\"probe\"")],
                    if saw { "saw-validators" } else { "clean" },
                )
            }),
        )
        .route(
            "/slow",
            get(|| async {
                tokio::time::sleep(Duration::from_millis(1200)).await;
                RSS.to_string()
            }),
        );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    format!("http://{addr}")
}

/// Test state. The per-host gap is shortened so the suite does not spend seconds
/// waiting; `manual_runs_obey_the_same_per_host_spacing_as_the_scheduler` restores the
/// production value to prove the gate exists.
fn state(fetcher: fetch::Fetcher) -> SharedState {
    let mut state = AppState::new(
        config::Config {
            data_dir: ".".into(),
            regions_dir: ".".into(),
            bind_address: "127.0.0.1".parse().unwrap(),
            port: 8000,
            management_key: Some("test-management-key".into()),
            api_keys: vec!["test-api-key".into()],
            allowed_hosts: vec!["newsman.test".into()],
            secure_cookie: true,
            version: "test",
        },
        db::Database::in_memory().unwrap(),
        fetcher,
    );
    state.host_gap = Duration::from_millis(40);
    Arc::new(state)
}

fn add_source(state: &SharedState, slug: &str, url: &str, topic: Option<&str>) -> i64 {
    let db = state.db.lock().unwrap();
    db.conn
        .execute(
            "INSERT INTO sources(slug,name,url,tier,enabled,region,topic_slug,fetch_interval_minutes) VALUES (?1,?1,?2,'T2',1,'global',?3,60)",
            params![slug, url, topic],
        )
        .unwrap();
    db.conn.last_insert_rowid()
}

fn source_state(
    state: &SharedState,
    id: i64,
) -> (i64, Option<String>, Option<String>, Option<i64>) {
    state
        .db
        .lock()
        .unwrap()
        .conn
        .query_row(
            "SELECT consecutive_failures, paused_reason, last_error, next_fetch_at FROM sources WHERE id=?1",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .unwrap()
}

async fn get_json(
    app: &Router,
    path: &str,
    api_key: Option<&str>,
) -> (StatusCode, serde_json::Value) {
    let mut builder = HttpRequest::builder()
        .uri(path)
        .header(header::HOST, "newsman.test");
    if let Some(key) = api_key {
        builder = builder.header(header::AUTHORIZATION, format!("Bearer {key}"));
    }
    let response = app
        .clone()
        .oneshot(builder.body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (status, serde_json::from_slice(&bytes).unwrap_or_default())
}

#[tokio::test]
async fn collects_dedupes_uses_etag_and_filters_by_topic_tree() {
    let base = feed_server().await;
    let state = state(fetch::Fetcher::allow_loopback_for_tests());
    state
        .db
        .lock()
        .unwrap()
        .conn
        .execute(
            "INSERT INTO topics(slug,label_vi,label_en,parent_slug,enabled) VALUES ('rates','Lãi suất','Rates','finance',1)",
            [],
        )
        .unwrap();
    let id = add_source(&state, "mock", &format!("{base}/moved"), Some("rates"));

    let first = collector::collect(&state, id).await.unwrap();
    assert_eq!(
        (first.status, first.items_seen, first.items_new),
        ("ok", 2, 2)
    );
    let second = collector::collect(&state, id).await.unwrap();
    assert_eq!(
        second.status, "not_modified",
        "ETag must be sent on the next run"
    );

    // Clearing the validator forces a full body; rows must still not duplicate.
    state
        .db
        .lock()
        .unwrap()
        .conn
        .execute("UPDATE sources SET etag=NULL", [])
        .unwrap();
    let third = collector::collect(&state, id).await.unwrap();
    assert_eq!((third.status, third.items_new), ("ok", 0));

    let stored: (String, Option<String>, String) = state
        .db
        .lock()
        .unwrap()
        .conn
        .query_row(
            "SELECT canonical_url, published_at, topic_slug FROM articles WHERE guid='a-1'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .unwrap();
    assert_eq!(stored.0, "https://news.example.org/a/1");
    assert_eq!(stored.1.as_deref(), Some("2026-11-10T08:00:00Z"));
    assert_eq!(stored.2, "rates");
    let columns: String = state
        .db
        .lock()
        .unwrap()
        .conn
        .query_row(
            "SELECT COALESCE(summary,'') FROM articles WHERE guid='a-1'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(
        columns.is_empty(),
        "publisher description must not be stored"
    );

    let app = router(state.clone());
    let (status, body) =
        get_json(&app, "/api/v1/articles?topic=finance", Some("test-api-key")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        body["data"].as_array().unwrap().len(),
        2,
        "parent topic includes child topic"
    );
    // Undated items sort by fetch time, so the undated item (fetched now) comes first.
    assert_eq!(body["data"][0]["title"], "Second item");
    assert_eq!(body["data"][1]["title"], "Lãi suất giữ nguyên");
    assert_eq!(body["data"][1]["source_tier"], "T2");
    let (_, body) = get_json(&app, "/api/v1/articles?topic=world", Some("test-api-key")).await;
    assert!(body["data"].as_array().unwrap().is_empty());
    let (status, _) = get_json(&app, "/api/v1/articles", None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn failures_back_off_then_pause_and_resume_clears() {
    let base = feed_server().await;
    let state = state(fetch::Fetcher::allow_loopback_for_tests());
    let id = add_source(&state, "broken", &format!("{base}/broken"), None);
    let mut last_next = 0;
    for attempt in 1..=collector::PAUSE_AFTER_FAILURES {
        let run = collector::collect(&state, id).await.unwrap();
        assert_eq!(run.error.as_deref(), Some("http_500"));
        assert_eq!(run.paused, attempt == collector::PAUSE_AFTER_FAILURES);
        let (failures, _, _, next) = source_state(&state, id);
        assert_eq!(failures, attempt);
        assert!(next.unwrap() >= last_next, "back-off must not shrink");
        last_next = next.unwrap();
    }
    let (_, paused, error, _) = source_state(&state, id);
    assert_eq!(paused.as_deref(), Some("consecutive_failures"));
    assert_eq!(error.as_deref(), Some("http_500"));

    // The scheduler skips paused sources even when the collector is on and the source is due.
    {
        let mut db = state.db.lock().unwrap();
        db.set_settings(&[("collector_enabled", "true")]).unwrap();
        db.conn
            .execute("UPDATE sources SET next_fetch_at=0", [])
            .unwrap();
    }
    assert_eq!(collector::run_due_once(&state).await, 0);

    let app = router(state.clone());
    let cookie = login(&app).await;
    let response = post(
        &app,
        &format!("/api/admin/sources/{id}/resume"),
        "",
        &cookie,
    )
    .await;
    assert_eq!(response.0, StatusCode::OK);
    let (failures, paused, _, next) = source_state(&state, id);
    assert_eq!((failures, paused, next), (0, None, None));
    assert_eq!(
        collector::run_due_once(&state).await,
        1,
        "resumed source is due"
    );
}

#[tokio::test]
async fn scheduler_is_off_by_default_and_respects_disabled_sources() {
    let base = feed_server().await;
    let state = state(fetch::Fetcher::allow_loopback_for_tests());
    let on = add_source(&state, "on", &format!("{base}/feed.xml"), None);
    let off = add_source(&state, "off", &format!("{base}/feed.xml?copy=1"), None);
    state
        .db
        .lock()
        .unwrap()
        .conn
        .execute("UPDATE sources SET enabled=0 WHERE id=?1", [off])
        .unwrap();

    assert_eq!(
        collector::run_due_once(&state).await,
        0,
        "collector disabled by default"
    );
    state
        .db
        .lock()
        .unwrap()
        .set_settings(&[("collector_enabled", "true")])
        .unwrap();
    assert_eq!(collector::run_due_once(&state).await, 1);
    assert_eq!(source_state(&state, on).0, 0);
    assert_eq!(
        source_state(&state, off).3,
        None,
        "disabled source never ran"
    );
    assert_eq!(
        collector::run_due_once(&state).await,
        0,
        "not due again yet"
    );
}

#[tokio::test]
async fn rejects_pages_oversized_bodies_redirect_loops_and_private_targets() {
    let base = feed_server().await;
    let state = state(fetch::Fetcher::allow_loopback_for_tests());
    for (path, expected) in [
        ("/page", "not_a_feed"),
        ("/huge", "response_too_large"),
        ("/loop", "too_many_redirects"),
    ] {
        let id = add_source(
            &state,
            path.trim_start_matches('/'),
            &format!("{base}{path}"),
            None,
        );
        let run = collector::collect(&state, id).await.unwrap();
        assert_eq!(run.error.as_deref(), Some(expected), "{path}");
    }

    // The production fetcher refuses the loopback server before connecting.
    let public = state_with_public();
    let id = add_source(&public, "internal", "http://127.0.0.1:8080/feed.xml", None);
    let run = collector::collect(&public, id).await.unwrap();
    assert_eq!(run.error.as_deref(), Some("blocked_address"));
}

#[tokio::test]
async fn preview_reads_feed_without_storing_and_needs_a_session() {
    let base = feed_server().await;
    let state = state(fetch::Fetcher::allow_loopback_for_tests());
    let app = router(state.clone());
    let payload = format!(r#"{{"url":"{base}/feed.xml"}}"#);
    let unauthenticated = app
        .clone()
        .oneshot(
            HttpRequest::builder()
                .method("POST")
                .uri("/api/admin/sources/preview")
                .header(header::HOST, "newsman.test")
                .header(header::CONTENT_TYPE, "application/json")
                .header(header::AUTHORIZATION, "Bearer test-api-key")
                .header("x-requested-with", "XMLHttpRequest")
                .body(Body::from(payload.clone()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(unauthenticated.status(), StatusCode::UNAUTHORIZED);

    let cookie = login(&app).await;
    let (status, body) = post(&app, "/api/admin/sources/preview", &payload, &cookie).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["data"]["items_seen"], 2);
    assert_eq!(body["data"]["sample"][0]["title"], "Lãi suất giữ nguyên");
    let count: i64 = state
        .db
        .lock()
        .unwrap()
        .conn
        .query_row("SELECT COUNT(*) FROM articles", [], |r| r.get(0))
        .unwrap();
    assert_eq!(count, 0);

    let (status, body) = post(
        &app,
        "/api/admin/sources/preview",
        &format!(r#"{{"url":"{base}/broken"}}"#),
        &cookie,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_GATEWAY);
    assert_eq!(body["reason"], "http_500");
}

fn state_with_public() -> SharedState {
    state(fetch::Fetcher::public_only())
}

async fn login(app: &Router) -> String {
    let response = app
        .clone()
        .oneshot(
            HttpRequest::builder()
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
    response.headers()[header::SET_COOKIE]
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_string()
}

async fn post(
    app: &Router,
    path: &str,
    body: &str,
    cookie: &str,
) -> (StatusCode, serde_json::Value) {
    let response = app
        .clone()
        .oneshot(
            HttpRequest::builder()
                .method("POST")
                .uri(path)
                .header(header::HOST, "newsman.test")
                .header(header::CONTENT_TYPE, "application/json")
                .header(header::COOKIE, cookie)
                .header("x-requested-with", "XMLHttpRequest")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (status, serde_json::from_slice(&bytes).unwrap_or_default())
}

// ---------- A: safety review regression tests ----------

#[tokio::test]
async fn validators_are_not_replayed_to_a_redirected_origin() {
    let first = feed_server().await;
    let second = feed_server().await;
    let fetcher = fetch::Fetcher::allow_loopback_for_tests();
    let target = format!("{second}/conditional-probe");
    let encoded: String = url::form_urlencoded::byte_serialize(target.as_bytes()).collect();
    let start = format!("{first}/redirect-to?to={encoded}");
    let first_origin = url::Url::parse(&first)
        .unwrap()
        .origin()
        .ascii_serialization();

    let validators = fetch::Validators {
        etag: Some("\"first-origin\"".into()),
        last_modified: Some("Tue, 10 Nov 2026 08:00:00 GMT".into()),
        origin: Some(first_origin.clone()),
    };
    let body = match fetcher.get(&start, &validators).await.unwrap() {
        fetch::FetchOutcome::Body { bytes, .. } => String::from_utf8(bytes).unwrap(),
        other => panic!("unexpected outcome: {other:?}"),
    };
    assert_eq!(
        body, "clean",
        "a cross-origin redirect must not receive If-None-Match / If-Modified-Since"
    );

    // The same validators are still replayed to their own origin.
    let same_origin = format!("{first}/conditional-probe");
    let body = match fetcher.get(&same_origin, &validators).await.unwrap() {
        fetch::FetchOutcome::Body { bytes, .. } => String::from_utf8(bytes).unwrap(),
        other => panic!("unexpected outcome: {other:?}"),
    };
    assert_eq!(body, "saw-validators");
}

#[tokio::test]
async fn changing_the_url_clears_stored_validators() {
    let state = state(fetch::Fetcher::allow_loopback_for_tests());
    let id = add_source(&state, "swap", "http://127.0.0.1:9/old", None);
    state
        .db
        .lock()
        .unwrap()
        .conn
        .execute(
            "UPDATE sources SET etag='\"old\"', last_modified='Tue, 10 Nov 2026 08:00:00 GMT', validators_origin='http://127.0.0.1:9' WHERE id=?1",
            [id],
        )
        .unwrap();
    let row = json!({
        "slug": "swap", "name": "Swap", "url": "http://127.0.0.1:9/new", "tier": "T2",
        "enabled": false, "language": "en", "region": "global", "topic_slug": null,
        "fetch_interval_minutes": 60
    });
    let app = router(state.clone());
    let cookie = login_cookie_for(&app).await;
    let response = app
        .oneshot(
            HttpRequest::builder()
                .method("PUT")
                .uri(format!("/api/admin/sources/{id}"))
                .header(header::HOST, "newsman.test")
                .header(header::CONTENT_TYPE, "application/json")
                .header("x-requested-with", "XMLHttpRequest")
                .header(header::COOKIE, cookie)
                .body(Body::from(row.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let (etag, origin): (Option<String>, Option<String>) = state
        .db
        .lock()
        .unwrap()
        .conn
        .query_row(
            "SELECT etag, validators_origin FROM sources WHERE id=?1",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!(
        (etag, origin),
        (None, None),
        "validators of the old feed must not survive a URL change"
    );
}

async fn login_cookie_for(app: &Router) -> String {
    let response = app
        .clone()
        .oneshot(
            HttpRequest::builder()
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

#[tokio::test]
async fn manual_runs_obey_the_same_per_host_spacing_as_the_scheduler() {
    let base = feed_server().await;
    let state = state(fetch::Fetcher::allow_loopback_for_tests());
    let first = add_source(&state, "one", &format!("{base}/feed.xml?a"), None);
    let second = add_source(&state, "two", &format!("{base}/feed.xml?b"), None);
    let clock = Instant::now();
    collector::collect(&state, first).await.unwrap();
    let after_first = clock.elapsed();
    collector::collect(&state, second).await.unwrap();
    let total = clock.elapsed();
    assert!(
        after_first < Duration::from_secs(2),
        "the first request should not wait: {after_first:?}"
    );
    assert!(
        total >= Duration::from_millis(40),
        "'Fetch now' must wait for the per-host gap: {total:?}"
    );
}

#[tokio::test]
async fn a_run_discards_its_result_when_the_source_changes_mid_flight() {
    let base = feed_server().await;
    let state = state(fetch::Fetcher::allow_loopback_for_tests());
    let id = add_source(&state, "slow", &format!("{base}/slow"), None);
    let task = tokio::spawn({
        let state = state.clone();
        async move { collector::collect(&state, id).await }
    });
    tokio::time::sleep(Duration::from_millis(200)).await;
    state
        .db
        .lock()
        .unwrap()
        .conn
        .execute(
            "UPDATE sources SET url=?2 WHERE id=?1",
            params![id, format!("{base}/feed.xml")],
        )
        .unwrap();
    let summary = task.await.unwrap().unwrap();
    assert_eq!(summary.status, "discarded");
    assert_eq!(summary.error.as_deref(), Some("source_changed"));
    let articles: i64 = state
        .db
        .lock()
        .unwrap()
        .conn
        .query_row("SELECT COUNT(*) FROM articles", [], |r| r.get(0))
        .unwrap();
    assert_eq!(articles, 0, "a discarded run must not store anything");
}

#[tokio::test]
async fn a_run_discards_its_result_when_the_region_is_switched_mid_flight() {
    let base = feed_server().await;
    let state = state(fetch::Fetcher::allow_loopback_for_tests());
    let id = add_source(&state, "slow-region", &format!("{base}/slow"), None);
    let task = tokio::spawn({
        let state = state.clone();
        async move { collector::collect(&state, id).await }
    });
    tokio::time::sleep(Duration::from_millis(200)).await;
    // Simulates a pack reload that re-assigns the source while it is being fetched.
    state
        .db
        .lock()
        .unwrap()
        .conn
        .execute("UPDATE sources SET region='vn' WHERE id=?1", [id])
        .unwrap();
    let summary = task.await.unwrap().unwrap();
    assert_eq!(summary.status, "discarded");
    let articles: i64 = state
        .db
        .lock()
        .unwrap()
        .conn
        .query_row("SELECT COUNT(*) FROM articles", [], |r| r.get(0))
        .unwrap();
    assert_eq!(articles, 0);
}

#[tokio::test]
async fn a_cancelled_run_releases_its_in_flight_slot() {
    let base = feed_server().await;
    let state = state(fetch::Fetcher::allow_loopback_for_tests());
    let id = add_source(&state, "cancel", &format!("{base}/slow"), None);
    let task = tokio::spawn({
        let state = state.clone();
        async move { collector::collect(&state, id).await }
    });
    tokio::time::sleep(Duration::from_millis(150)).await;
    task.abort();
    let _ = task.await;
    assert!(
        state.in_flight.lock().unwrap().is_empty(),
        "an interrupted run must release the in-flight slot, as on shutdown"
    );
    let summary = collector::collect(&state, id).await.unwrap();
    assert_eq!(summary.status, "ok");
}
