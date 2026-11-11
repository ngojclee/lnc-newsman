//! Hub provider tests against a loopback mock. No real Hub is contacted.

use super::*;
use crate::providers::tests_support::state_with;
use axum::routing::{get, post};
use std::sync::Arc;

fn config(base: &str, hosts: Vec<&str>) -> ProviderConfig {
    ProviderConfig {
        base_url: Some(base.to_string()),
        allowed_hosts: hosts.into_iter().map(str::to_string).collect(),
        model_classify: "cheap".into(),
        model_summarise: "default".into(),
        model_analyse: "strong".into(),
        agent_id: "news-analyst-argus".into(),
        timeout_seconds: 5,
    }
}

/// Loopback Hub: chat completions, models and one agent. Counts each call.
async fn mock_hub() -> (String, Arc<std::sync::Mutex<Vec<String>>>) {
    let calls = Arc::new(std::sync::Mutex::new(Vec::<String>::new()));
    let c1 = calls.clone();
    let c2 = calls.clone();
    let c3 = calls.clone();
    let app = axum::Router::new()
        .route(
            "/chat/completions",
            post(move |body: String| {
                let calls = c1.clone();
                async move {
                    calls.lock().unwrap().push(format!("chat:{body}"));
                    (
                        axum::http::StatusCode::OK,
                        axum::Json(serde_json::json!({
                            "choices":[{"message":{"content":"answer text"}}],
                            "usage":{"total_tokens":42}
                        })),
                    )
                }
            }),
        )
        .route(
            "/models",
            get(move |headers: axum::http::HeaderMap| {
                let calls = c2.clone();
                async move {
                    calls.lock().unwrap().push(format!(
                        "models:{}",
                        headers
                            .get(axum::http::header::AUTHORIZATION)
                            .and_then(|v| v.to_str().ok())
                            .unwrap_or("none")
                    ));
                    axum::Json(serde_json::json!({"data":[{"id":"cheap"},{"id":"strong"}]}))
                }
            }),
        )
        .route(
            "/agents/news-analyst-argus/run",
            post(move |body: String| {
                let calls = c3.clone();
                async move {
                    calls.lock().unwrap().push(format!("agent:{body}"));
                    (
                        axum::http::StatusCode::OK,
                        axum::Json(serde_json::json!({
                            "output":"research result",
                            "usage":{"total_tokens":99}
                        })),
                    )
                }
            }),
        );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    (format!("http://{addr}"), calls)
}

fn state_with_hub(base: &str, hosts: Vec<&str>, with_secret: bool) -> SharedState {
    let state = state_with(crate::fetch::Fetcher::allow_loopback_for_tests());
    let mut state = state;
    let inner = Arc::get_mut(&mut state).expect("sole owner");
    inner.hub = config(base, hosts);
    if with_secret {
        let vault = SecretBox::from_bytes([21; 32], None);
        crate::secrets::set(
            &inner.db.lock().unwrap(),
            &vault,
            SECRET_NAME,
            "hub-key-value",
        )
        .unwrap();
        inner.secrets = Some(vault);
    }
    state
}

#[test]
fn a_provider_needs_both_a_base_url_and_an_allowlisted_host() {
    let state = state_with(crate::fetch::Fetcher::public_only());
    assert!(!configured(&state), "nothing configured by default");
    let mut state = state;
    Arc::get_mut(&mut state).unwrap().hub = config("http://127.0.0.1:9999", vec![]);
    assert!(
        !configured(&state),
        "a private host is not usable unless it is named in the allowlist"
    );
    Arc::get_mut(&mut state).unwrap().hub = config("http://127.0.0.1:9999", vec!["127.0.0.1"]);
    assert!(configured(&state), "an explicitly allowed host is usable");
    Arc::get_mut(&mut state).unwrap().hub = config("http://127.0.0.1:9999", vec!["other.host"]);
    assert!(
        !configured(&state),
        "a different host does not grant access"
    );
}

#[test]
fn the_per_host_permission_does_not_open_other_private_addresses() {
    let fetcher = crate::fetch::Fetcher::public_only().with_private_hosts(vec!["hub.lan".into()]);
    // A URL for the allowed host would be resolved by DNS, which these unit tests avoid;
    // the important property is that an unrelated literal stays blocked at the HTTP layer.
    assert!(!fetcher.host_allows_private(&url::Url::parse("http://other.lan/x").unwrap()));
    assert!(fetcher.host_allows_private(&url::Url::parse("http://hub.lan/x").unwrap()));
    assert!(!fetcher.host_allows_private(&url::Url::parse("http://127.0.0.1/x").unwrap()));
}

#[tokio::test]
async fn a_completion_is_billed_then_cached_and_uses_the_task_alias() {
    let (base, calls) = mock_hub().await;
    let state = state_with_hub(&base, vec!["127.0.0.1"], true);
    let first = complete(&state, Task::Classify, "hello").await.unwrap();
    assert_eq!(first.text, "answer text");
    assert_eq!(first.model, "cheap");
    assert!(!first.cached);
    assert_eq!(first.tokens, 42);
    let last = calls.lock().unwrap().last().cloned().unwrap();
    assert!(last.contains("\"model\":\"cheap\""), "{last}");
    let second = complete(&state, Task::Classify, "hello").await.unwrap();
    assert!(second.cached, "the same prompt is served from cache");
    assert_eq!(calls.lock().unwrap().len(), 1, "no second request");
    // A different task alias is a different cache entry.
    let analyse = complete(&state, Task::Analyse, "hello").await.unwrap();
    assert_eq!(analyse.model, "strong");
    assert!(!analyse.cached);
}

#[tokio::test]
async fn a_spent_budget_refuses_before_any_request() {
    let (base, calls) = mock_hub().await;
    let state = state_with_hub(&base, vec!["127.0.0.1"], true);
    {
        let mut db = state.db.lock().unwrap();
        db.conn
            .execute(
                "UPDATE settings SET value='10' WHERE key='llm_daily_token_budget'",
                [],
            )
            .ok();
        db.set_settings(&[("llm_daily_token_budget", "10")])
            .unwrap();
        db.conn
            .execute(
                "INSERT INTO llm_usage(day,tokens_used,calls) VALUES (?1,10,1)",
                [day_key(crate::now() as i64)],
            )
            .unwrap();
    }
    assert_eq!(
        complete(&state, Task::Summarise, "new prompt").await,
        Err(ProviderError::BudgetExhausted)
    );
    assert!(calls.lock().unwrap().is_empty());
}

#[tokio::test]
async fn a_missing_key_is_reported_and_never_requests() {
    let (base, calls) = mock_hub().await;
    let state = state_with_hub(&base, vec!["127.0.0.1"], false);
    assert_eq!(
        complete(&state, Task::Classify, "hello").await,
        Err(ProviderError::NoSecret)
    );
    assert!(calls.lock().unwrap().is_empty());
}

#[tokio::test]
async fn an_agent_run_is_capped_and_billed() {
    let (base, calls) = mock_hub().await;
    let state = state_with_hub(&base, vec!["127.0.0.1"], true);
    let run = run_agent(&state, "summarise the week").await.unwrap();
    assert_eq!(run.text, "research result");
    assert_eq!(run.agent, "news-analyst-argus");
    assert_eq!(run.tokens, 99);
    assert!(calls.lock().unwrap().last().unwrap().starts_with("agent:"));
    let (used, _, calls_today) = usage(&state.db.lock().unwrap(), crate::now() as i64);
    assert_eq!(used, 99);
    assert_eq!(calls_today, 1);
}

#[tokio::test]
async fn checking_the_connection_lists_models_only() {
    let (base, calls) = mock_hub().await;
    let state = state_with_hub(&base, vec!["127.0.0.1"], true);
    let models = list_models(&state).await.unwrap();
    assert_eq!(models, vec!["cheap".to_string(), "strong".to_string()]);
    let seen = calls.lock().unwrap().clone();
    assert_eq!(seen.len(), 1, "exactly one request");
    assert!(seen[0].starts_with("models:"), "{seen:?}");
    assert!(
        seen[0].contains("Bearer hub-key-value"),
        "the key travels as a bearer header"
    );
    let (used, _, _) = usage(&state.db.lock().unwrap(), crate::now() as i64);
    assert_eq!(used, 0, "listing models costs no tokens");
}

#[tokio::test]
async fn the_status_endpoint_never_returns_the_key_or_the_full_url() {
    let (base, _calls) = mock_hub().await;
    let state = state_with_hub(&base, vec!["127.0.0.1"], true);
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
                .uri("/api/admin/provider")
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
    assert!(!text.contains("hub-key-value"));
    assert!(!text.contains(&base), "the full base URL is not exposed");
    assert!(text.contains("\"base_host\":\"127.0.0.1\""));
    assert!(text.contains("has_secret"));
}
