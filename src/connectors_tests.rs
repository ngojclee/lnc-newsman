//! Connector framework tests: gates, quota, cache, health and request accounting.
//! No real provider is contacted.

use super::*;
use crate::{AppState, config, secrets::SecretBox};
use axum::routing::get;
use std::sync::Arc;

fn temp_dir() -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "lnc-newsman-connectors-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

pub(crate) fn state_with(fetcher: crate::fetch::Fetcher) -> SharedState {
    let dir = temp_dir();
    Arc::new(AppState::new(
        config::Config {
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
        db::Database::in_memory().unwrap(),
        fetcher,
    ))
}

/// A loopback server whose routes are given as (path, body) pairs.
pub(crate) async fn mock_server(routes: Vec<(&'static str, &'static str)>) -> String {
    let mut app = axum::Router::new();
    for (path, body) in routes {
        app = app.route(
            path,
            get(move || async move { (axum::http::StatusCode::OK, body) }),
        );
    }
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    format!("http://{addr}")
}

/// The number of requests the mock observed, so a test can prove a connector stops.
pub(crate) type Counter = std::sync::Arc<std::sync::atomic::AtomicUsize>;

pub(crate) async fn counting_server(
    routes: Vec<(&'static str, &'static str)>,
) -> (String, Counter) {
    let counter: Counter = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let mut app = axum::Router::new();
    for (path, body) in routes {
        let counter = counter.clone();
        app = app.route(
            path,
            get(move || {
                let counter = counter.clone();
                async move {
                    counter.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    (axum::http::StatusCode::OK, body)
                }
            }),
        );
    }
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    (format!("http://{addr}"), counter)
}

/// A minimal connector living in the test, so the framework's rules are tested
/// independently of any provider adapter.
pub(crate) struct Harness {
    pub needs_secret: bool,
    pub config_required: bool,
    pub per_minute: Option<i64>,
    pub writes: bool,
}

impl Harness {
    pub fn plain() -> Self {
        Self {
            needs_secret: false,
            config_required: false,
            per_minute: None,
            writes: false,
        }
    }
}

impl Connector for Harness {
    fn id(&self) -> &'static str {
        "harness"
    }
    fn provider(&self) -> &'static str {
        "test"
    }
    fn secret_name(&self) -> Option<&'static str> {
        self.needs_secret.then_some("harness-key")
    }
    fn minimum_interval_seconds(&self) -> i64 {
        0
    }
    fn default_daily_quota(&self) -> i64 {
        10
    }
    fn max_requests_per_minute(&self) -> Option<i64> {
        self.per_minute
    }
    fn base_url(&self) -> &'static str {
        // Deliberately unreachable: tests always pass an explicit loopback base.
        "https://harness.invalid"
    }
    fn config_ready(&self, config: &ConnectorConfig) -> bool {
        !self.config_required || config.sec_user_agent.is_some()
    }
    fn collect<'a>(&'a self, ctx: ConnectorContext<'a>) -> ConnectorFuture<'a> {
        Box::pin(async move {
            ctx.budget.charge();
            let url = format!("{}/value.json", ctx.base_url);
            let body = match ctx.fetcher.get(&url, &Default::default()).await {
                Ok(crate::fetch::FetchOutcome::Body { bytes, .. }) => bytes,
                Ok(_) => return Err(ConnectorError::Provider),
                Err(_) => return Err(ConnectorError::Fetch),
            };
            let parsed: serde_json::Value =
                serde_json::from_slice(&body).map_err(|_| ConnectorError::Provider)?;
            let value = parsed["value"]
                .as_f64()
                .ok_or(ConnectorError::InvalidData)?;
            if self.writes
                && let Some(state) = ctx.state
            {
                let db = state.db.lock().expect("database lock poisoned");
                db.conn
                    .execute(
                        "INSERT INTO articles(canonical_url,title,source_id,fetched_at) VALUES (?1,?1,NULL,0)",
                        [format!("https://example.invalid/{value}")],
                    )
                    .map_err(|_| ConnectorError::InvalidData)?;
            }
            Ok(CollectOutcome {
                observations: 1,
                ..Default::default()
            })
        })
    }
}

pub(crate) fn enable(db: &db::Database, connector: &dyn Connector, region: &str) {
    upsert(db, connector, region).unwrap();
    db.conn
        .execute(
            "UPDATE connectors SET enabled=1, cache_seconds=0 WHERE id=?1",
            [connector.id()],
        )
        .unwrap();
}

#[test]
fn every_gate_is_checked_without_network() {
    let state = state_with(crate::fetch::Fetcher::public_only());
    let db = state.db.lock().unwrap();
    db.conn
        .execute(
            "INSERT INTO regions(code,parent_code,label_vi,label_en,enabled) VALUES ('us',NULL,'US','US',0)",
            [],
        )
        .unwrap();
    let connector = Harness {
        needs_secret: true,
        ..Harness::plain()
    };
    upsert(&db, &connector, "us").unwrap();
    let mut row = row(&db, "harness").unwrap().unwrap();
    let now = crate::now() as i64;
    let config = ConnectorConfig::default();
    let vault = SecretBox::from_bytes([1; 32], None);

    assert_eq!(
        gate(&db, Some(&vault), &config, &connector, &row, now),
        Err(Skip::Disabled)
    );
    row.enabled = true;
    assert_eq!(
        gate(&db, Some(&vault), &config, &connector, &row, now),
        Err(Skip::RegionOff)
    );
    db.conn
        .execute("UPDATE regions SET enabled=1 WHERE code='us'", [])
        .unwrap();

    assert_eq!(
        gate(&db, Some(&vault), &config, &connector, &row, now),
        Err(Skip::SecretMissing)
    );
    assert_eq!(
        gate(&db, None, &config, &connector, &row, now),
        Err(Skip::SecretMissing)
    );
    secrets::set(&db, &vault, "harness-key", "x").unwrap();
    assert_eq!(
        gate(&db, Some(&vault), &config, &connector, &row, now),
        Ok(())
    );
    let other = SecretBox::from_bytes([2; 32], None);
    assert_eq!(
        gate(&db, Some(&other), &config, &connector, &row, now),
        Err(Skip::SecretUnreadable)
    );

    let needs_config = Harness {
        config_required: true,
        ..Harness::plain()
    };
    assert_eq!(
        gate(&db, Some(&vault), &config, &needs_config, &row, now),
        Err(Skip::NotConfigured)
    );
    let configured = ConnectorConfig {
        sec_user_agent: Some("Example Bot contact@example.invalid".into()),
    };
    assert_eq!(
        gate(&db, Some(&vault), &configured, &needs_config, &row, now),
        Ok(())
    );

    let simple = Harness::plain();
    row.next_run_at = Some(now + 600);
    assert_eq!(
        gate(&db, Some(&vault), &config, &simple, &row, now),
        Err(Skip::NotDue)
    );
    row.next_run_at = None;
    row.quota_day = Some(day_key(now));
    row.requests_today = 10;
    row.daily_quota = 10;
    assert_eq!(
        gate(&db, Some(&vault), &config, &simple, &row, now),
        Err(Skip::QuotaExhausted)
    );
    row.requests_today = 0;
    row.last_run_at = Some(now - 10);
    row.cache_seconds = 900;
    assert_eq!(
        gate(&db, Some(&vault), &config, &simple, &row, now),
        Err(Skip::Cached)
    );
    row.cache_seconds = 0;
    assert_eq!(gate(&db, Some(&vault), &config, &simple, &row, now), Ok(()));
}

#[test]
fn new_connectors_start_disabled() {
    let state = state_with(crate::fetch::Fetcher::public_only());
    let db = state.db.lock().unwrap();
    let connector = Harness::plain();
    upsert(&db, &connector, "global").unwrap();
    assert!(!row(&db, "harness").unwrap().unwrap().enabled);
    assert!(upsert(&db, &connector, "nowhere").is_err());
}

#[tokio::test]
async fn a_successful_run_records_health_and_the_requests_it_spent() {
    let base = mock_server(vec![("/value.json", r#"{"value":123.5}"#)]).await;
    let state = state_with(crate::fetch::Fetcher::allow_loopback_for_tests());
    let connector = Harness::plain();
    {
        let db = state.db.lock().unwrap();
        enable(&db, &connector, "global");
    }
    let outcome = run_with(&state, &connector, &base).await.expect("runs");
    assert_eq!(outcome.observations, 1);
    let saved = row(&state.db.lock().unwrap(), "harness").unwrap().unwrap();
    assert_eq!(saved.health, "healthy");
    assert_eq!(saved.requests_today, 1, "one HTTP request spent one unit");

    // With a cache window, the next run is served without a request.
    state
        .db
        .lock()
        .unwrap()
        .conn
        .execute(
            "UPDATE connectors SET cache_seconds=900 WHERE id='harness'",
            [],
        )
        .unwrap();
    assert_eq!(
        run_with(&state, &connector, &base).await.unwrap_err(),
        RunError::Skipped(Skip::Cached)
    );
    let saved = row(&state.db.lock().unwrap(), "harness").unwrap().unwrap();
    assert_eq!(saved.requests_today, 1, "a cached run spends nothing");
}

#[tokio::test]
async fn a_disabled_region_stops_the_connector_before_any_request() {
    let (base, hits) = counting_server(vec![("/value.json", r#"{"value":1}"#)]).await;
    let state = state_with(crate::fetch::Fetcher::allow_loopback_for_tests());
    let connector = Harness::plain();
    {
        let db = state.db.lock().unwrap();
        db.conn
            .execute(
                "INSERT INTO regions(code,parent_code,label_vi,label_en,enabled) VALUES ('us',NULL,'US','US',0)",
                [],
            )
            .unwrap();
        enable(&db, &connector, "us");
    }
    assert_eq!(
        run_with(&state, &connector, &base).await.unwrap_err(),
        RunError::Skipped(Skip::RegionOff)
    );
    assert_eq!(hits.load(std::sync::atomic::Ordering::Relaxed), 0);
    let spent: i64 = state
        .db
        .lock()
        .unwrap()
        .conn
        .query_row(
            "SELECT requests_today FROM connectors WHERE id='harness'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(spent, 0);
}

#[tokio::test]
async fn provider_errors_degrade_health_and_keep_the_budget_honest() {
    let base = mock_server(vec![("/value.json", "not json")]).await;
    let state = state_with(crate::fetch::Fetcher::allow_loopback_for_tests());
    let connector = Harness::plain();
    {
        let db = state.db.lock().unwrap();
        enable(&db, &connector, "global");
    }
    assert_eq!(
        run_with(&state, &connector, &base).await.unwrap_err(),
        RunError::Failed(ConnectorError::Provider)
    );
    let saved = row(&state.db.lock().unwrap(), "harness").unwrap().unwrap();
    assert_eq!(saved.health, "degraded");
    assert_eq!(
        saved.last_error.as_deref(),
        Some("connector_provider_error")
    );
    assert_eq!(
        saved.requests_today, 1,
        "a failed attempt still spent the request"
    );
}

#[tokio::test]
async fn the_daily_quota_is_charged_per_request_and_refuses_the_next_run() {
    let base = mock_server(vec![("/value.json", r#"{"value":1}"#)]).await;
    let state = state_with(crate::fetch::Fetcher::allow_loopback_for_tests());
    let connector = Harness::plain();
    {
        let db = state.db.lock().unwrap();
        enable(&db, &connector, "global");
        db.conn
            .execute("UPDATE connectors SET daily_quota=1 WHERE id='harness'", [])
            .unwrap();
    }
    assert!(run_with(&state, &connector, &base).await.is_ok());
    state
        .db
        .lock()
        .unwrap()
        .conn
        .execute(
            "UPDATE connectors SET next_run_at=NULL, last_run_at=NULL, cache_seconds=0 WHERE id='harness'",
            [],
        )
        .unwrap();
    assert_eq!(
        run_with(&state, &connector, &base).await.unwrap_err(),
        RunError::Skipped(Skip::QuotaExhausted)
    );
}

#[tokio::test]
async fn a_per_minute_cap_pushes_the_next_run_out() {
    let base = mock_server(vec![("/value.json", r#"{"value":1}"#)]).await;
    let state = state_with(crate::fetch::Fetcher::allow_loopback_for_tests());
    let connector = Harness {
        per_minute: Some(30),
        ..Harness::plain()
    };
    {
        let db = state.db.lock().unwrap();
        enable(&db, &connector, "global");
    }
    let before = crate::now() as i64;
    run_with(&state, &connector, &base).await.unwrap();
    let saved = row(&state.db.lock().unwrap(), "harness").unwrap().unwrap();
    let next = saved.next_run_at.unwrap();
    assert!(
        next >= before + 120,
        "30/min must space runs by at least two minutes, got {}",
        next - before
    );
}

#[tokio::test]
async fn a_missing_secret_blocks_the_run() {
    let (base, hits) = counting_server(vec![("/value.json", r#"{"value":1}"#)]).await;
    let state = state_with(crate::fetch::Fetcher::allow_loopback_for_tests());
    let connector = Harness {
        needs_secret: true,
        ..Harness::plain()
    };
    {
        let db = state.db.lock().unwrap();
        enable(&db, &connector, "global");
    }
    assert_eq!(
        run_with(&state, &connector, &base).await.unwrap_err(),
        RunError::Skipped(Skip::SecretMissing)
    );
    assert_eq!(hits.load(std::sync::atomic::Ordering::Relaxed), 0);
}
