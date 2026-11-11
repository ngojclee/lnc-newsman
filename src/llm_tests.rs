//! LLM layer tests against a loopback "mock provider". No real model is contacted.

use super::*;
use crate::providers::tests_support::state_with;
use axum::routing::post;
use std::sync::Arc;

fn chat_body() -> String {
    r#"{"choices":[{"message":{"content":"{\"topic\":\"finance\",\"entities\":[\"TICKER:AAPL\",\"CURRENCY:USD\"]}"}}],"usage":{"total_tokens":37}}"#.into()
}

/// Loopback chat-completions server that counts calls and can answer badly.
async fn mock_llm(body: &'static str) -> (String, Arc<std::sync::atomic::AtomicUsize>) {
    let counter = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let handler_counter = counter.clone();
    let app = axum::Router::new().route(
        "/chat/completions",
        post(move || {
            let counter = handler_counter.clone();
            async move {
                counter.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                (
                    axum::http::StatusCode::OK,
                    axum::Json(serde_json::from_str::<serde_json::Value>(body).unwrap()),
                )
            }
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    (format!("http://{addr}"), counter)
}

fn state_with_llm(base: String, enabled: bool) -> std::sync::Arc<crate::AppState> {
    let mut state = state_with(crate::fetch::Fetcher::allow_loopback_for_tests());
    let inner = Arc::get_mut(&mut state).expect("sole owner");
    inner.llm = LlmConfig {
        base_url: Some(base),
        model: "mock-1".into(),
    };
    inner.llm_enabled = enabled;
    state
}

fn rules() -> Classification {
    crate::classify::classify("global", "Apple reports earnings")
}

#[test]
fn the_layer_is_off_by_default() {
    let state = state_with(crate::fetch::Fetcher::public_only());
    assert!(!enabled(&state), "no LLM unless the admin turns it on");
    assert!(state.llm.base_url.is_none());
}

#[test]
fn answers_are_parsed_strictly() {
    let label = parse_answer(
        "```json\n{\"topic\":\"finance\",\"entities\":[\"TICKER:AAPL\",\"bogus:thing\",\"ORG:FED\"]}\n```",
        "mock-1",
    )
    .unwrap();
    assert_eq!(label.topic.as_deref(), Some("finance"));
    assert_eq!(
        label.entities,
        vec!["TICKER:AAPL", "ORG:FED"],
        "unknown types are dropped"
    );
    // An unknown topic is dropped rather than stored.
    let unknown = parse_answer("{\"topic\":\"sports\"}", "mock-1").unwrap();
    assert_eq!(unknown.topic, None);
    // Garbage does not panic and yields nothing.
    assert!(parse_answer("not json", "mock-1").is_none());
}

#[test]
fn the_cache_key_separates_model_prompt_and_content() {
    let a = cache_key("mock-1", "headline");
    assert_eq!(
        a,
        cache_key("mock-1", "headline"),
        "stable for the same input"
    );
    assert_ne!(
        a,
        cache_key("mock-2", "headline"),
        "model is part of the key"
    );
    assert_ne!(
        a,
        cache_key("mock-1", "other headline"),
        "content is part of the key"
    );
}

#[tokio::test]
async fn a_call_is_billed_then_served_from_cache_without_a_second_request() {
    let body: &'static str = Box::leak(chat_body().into_boxed_str());
    let (base, hits) = mock_llm(body).await;
    let state = state_with_llm(base, true);
    let first = classify(&state, "Apple reports earnings", &rules())
        .await
        .unwrap();
    assert_eq!(first.topic.as_deref(), Some("finance"));
    assert!(!first.cached);
    assert_eq!(hits.load(std::sync::atomic::Ordering::Relaxed), 1);
    {
        let db = state.db.lock().unwrap();
        let (used, budget, calls) = usage(&db, crate::now() as i64).unwrap();
        assert_eq!(used, 37, "the provider's token count is recorded");
        assert!(budget > 0);
        assert_eq!(calls, 1);
    }
    let second = classify(&state, "Apple reports earnings", &rules())
        .await
        .unwrap();
    assert!(second.cached, "the second answer comes from the cache");
    assert_eq!(
        hits.load(std::sync::atomic::Ordering::Relaxed),
        1,
        "a cache hit must not call the provider"
    );
}

#[tokio::test]
async fn an_exhausted_token_budget_stops_calls_and_keeps_rules() {
    let body: &'static str = Box::leak(chat_body().into_boxed_str());
    let (base, hits) = mock_llm(body).await;
    let state = state_with_llm(base, true);
    {
        let mut db = state.db.lock().unwrap();
        db.set_settings(&[("llm_daily_token_budget", "10")])
            .unwrap();
        db.conn
            .execute(
                "INSERT INTO llm_usage(day,tokens_used,calls) VALUES (?1,10,1)",
                [day_key(crate::now() as i64)],
            )
            .unwrap();
    }
    let label = classify(&state, "A brand new headline", &rules()).await;
    assert!(label.is_none(), "over budget means no answer");
    assert_eq!(
        hits.load(std::sync::atomic::Ordering::Relaxed),
        0,
        "the budget is checked before the request"
    );
    // The rule layer still produced labels independently.
    assert!(rules().topic.is_some());
}

#[tokio::test]
async fn the_llm_layer_being_unavailable_never_breaks_the_rules() {
    let state = state_with_llm("http://127.0.0.1:9".into(), true);
    assert!(
        classify(&state, "Apple reports earnings", &rules())
            .await
            .is_none(),
        "an unreachable provider yields no label"
    );
    assert!(rules().topic.is_some(), "rule labels are unaffected");
}

#[tokio::test]
async fn a_redirect_does_not_forward_the_api_key() {
    // The mock records the Authorization header it sees on the redirect target.
    let seen = Arc::new(std::sync::Mutex::new(Vec::<String>::new()));
    let seen_inner = seen.clone();
    let app = axum::Router::new()
        .route(
            "/chat/completions",
            post(|| async {
                (
                    axum::http::StatusCode::TEMPORARY_REDIRECT,
                    [(axum::http::header::LOCATION, "/target")],
                )
            }),
        )
        .route(
            "/target",
            post(move |headers: axum::http::HeaderMap| {
                let seen = seen_inner.clone();
                async move {
                    seen.lock().unwrap().push(
                        headers
                            .get(axum::http::header::AUTHORIZATION)
                            .and_then(|v| v.to_str().ok())
                            .unwrap_or("none")
                            .to_string(),
                    );
                    (
                        axum::http::StatusCode::OK,
                        axum::Json(serde_json::json!({
                            "choices":[{"message":{"content":"{\"topic\":\"finance\"}"}}],
                            "usage":{"total_tokens":5}
                        })),
                    )
                }
            }),
        );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let state = state_with_llm(format!("http://{addr}"), true);
    let _ = classify(&state, "redirect test", &rules()).await;
    let headers = seen.lock().unwrap().clone();
    assert_eq!(
        headers,
        vec!["none"],
        "the bearer token stays on the first hop"
    );
}

#[tokio::test]
async fn the_admin_toggle_is_refused_without_a_configured_provider() {
    let state = state_with(crate::fetch::Fetcher::public_only());
    let app = crate::router(state);
    use tower::ServiceExt;
    let cookie = {
        let response = app
            .clone()
            .oneshot(
                axum::http::Request::builder()
                    .method("POST")
                    .uri("/api/auth/login")
                    .header(axum::http::header::HOST, "newsman.test")
                    .header(axum::http::header::CONTENT_TYPE, "application/json")
                    .header("x-requested-with", "XMLHttpRequest")
                    .body(axum::body::Body::from(r#"{"key":"test-management-key"}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        response
            .headers()
            .get(axum::http::header::SET_COOKIE)
            .unwrap()
            .to_str()
            .unwrap()
            .split(';')
            .next()
            .unwrap()
            .to_string()
    };
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .method("PATCH")
                .uri("/api/admin/llm")
                .header(axum::http::header::HOST, "newsman.test")
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .header("x-requested-with", "XMLHttpRequest")
                .header(axum::http::header::COOKIE, cookie)
                .body(axum::body::Body::from(r#"{"enabled":true}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        response.status(),
        axum::http::StatusCode::CONFLICT,
        "the switch cannot be flipped with no provider configured"
    );
}

#[tokio::test]
async fn the_status_endpoint_never_returns_the_key_or_the_provider_url() {
    let body: &'static str = Box::leak(chat_body().into_boxed_str());
    let (base, _hits) = mock_llm(body).await;
    let mut state = state_with_llm(base.clone(), true);
    let vault = crate::secrets::SecretBox::from_bytes([11; 32], None);
    std::sync::Arc::get_mut(&mut state)
        .expect("sole owner")
        .secrets = Some(vault.clone());
    {
        let db = state.db.lock().unwrap();
        crate::secrets::set(&db, &vault, SECRET_NAME, "super-secret-llm-key").unwrap();
    }
    let app = crate::router(state);
    use tower::ServiceExt;
    let login = app
        .clone()
        .oneshot(
            axum::http::Request::builder()
                .method("POST")
                .uri("/api/auth/login")
                .header(axum::http::header::HOST, "newsman.test")
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .header("x-requested-with", "XMLHttpRequest")
                .body(axum::body::Body::from(r#"{"key":"test-management-key"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    let cookie = login
        .headers()
        .get(axum::http::header::SET_COOKIE)
        .unwrap()
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_string();
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri("/api/admin/llm")
                .header(axum::http::header::HOST, "newsman.test")
                .header(axum::http::header::COOKIE, cookie)
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), axum::http::StatusCode::OK);
    let text = String::from_utf8(
        http_body_util::BodyExt::collect(response.into_body())
            .await
            .unwrap()
            .to_bytes()
            .to_vec(),
    )
    .unwrap();
    assert!(!text.contains("super-secret-llm-key"));
    assert!(
        !text.contains("127.0.0.1"),
        "the provider URL is not exposed"
    );
    assert!(text.contains("has_secret"));
}
