//! CoinGecko connector tests: key required, header never logged, attribution present,
//! conservative budget. Loopback only.

use super::*;
use crate::{
    connectors::{self, RunError, Skip},
    fetch::Fetcher,
    providers::tests_support::{counting_server, enable, state_with},
    secrets::SecretBox,
};

const BODY: &str = r#"{"bitcoin":{"usd":68234.12,"eur":63100.5,"vnd":1730000000}}"#;

fn with_key() -> (std::sync::Arc<crate::AppState>, SecretBox) {
    let vault = SecretBox::from_bytes([9; 32], None);
    let mut state = state_with(Fetcher::allow_loopback_for_tests());
    std::sync::Arc::get_mut(&mut state)
        .expect("sole owner")
        .secrets = Some(vault.clone());
    (state, vault)
}

#[tokio::test]
async fn without_a_key_nothing_is_requested() {
    let (base, hits) = counting_server(vec![("/api/v3/simple/price", BODY)]).await;
    let state = state_with(Fetcher::allow_loopback_for_tests());
    {
        let db = state.db.lock().unwrap();
        enable(&db, &CoinGecko, "global");
    }
    assert_eq!(
        connectors::run_with(&state, &CoinGecko, &base)
            .await
            .unwrap_err(),
        RunError::Skipped(Skip::SecretMissing)
    );
    assert_eq!(hits.load(std::sync::atomic::Ordering::Relaxed), 0);
}

#[tokio::test]
async fn a_key_writes_prices_with_mandatory_attribution() {
    let (base, _hits) = counting_server(vec![("/api/v3/simple/price", BODY)]).await;
    let (state, vault) = with_key();
    {
        let db = state.db.lock().unwrap();
        enable(&db, &CoinGecko, "global");
        crate::secrets::set(&db, &vault, SECRET, "demo-key-value").unwrap();
    }
    let outcome = connectors::run_with(&state, &CoinGecko, &base)
        .await
        .expect("collects");
    assert_eq!(outcome.observations, 3);
    assert_eq!(outcome.note.as_deref(), Some(ATTRIBUTION));
    let db = state.db.lock().unwrap();
    for code in ["BTC-USD-CG", "BTC-EUR-CG", "BTC-VND-CG"] {
        let saved = crate::series::series(&db, code).unwrap().unwrap();
        assert!(saved.attribution.contains("CoinGecko"), "{code}");
    }
    let points = crate::series::observations(&db, "BTC-USD-CG", None, 5).unwrap();
    assert_eq!(points[0].value, 68_234.12);
    // The key value never appears in stored data.
    let leak: i64 = db
        .conn
        .query_row(
            "SELECT COUNT(*) FROM series WHERE attribution LIKE '%demo-key-value%'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(leak, 0);
}

#[test]
fn the_connector_declares_conservative_limits() {
    assert_eq!(CoinGecko.max_requests_per_minute(), Some(2));
    assert!(CoinGecko.default_daily_quota() <= 30);
    const { assert!(MONTH_STOP_AT < MONTHLY_BUDGET, "stop before the hard edge") };
}

#[tokio::test]
async fn a_401_from_the_provider_is_reported_as_unauthorized() {
    let counter = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let handler_counter = counter.clone();
    let app = axum::Router::new().route(
        "/api/v3/simple/price",
        axum::routing::get(move || {
            let counter = handler_counter.clone();
            async move {
                counter.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                axum::http::StatusCode::UNAUTHORIZED
            }
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let (state, vault) = with_key();
    {
        let db = state.db.lock().unwrap();
        enable(&db, &CoinGecko, "global");
        crate::secrets::set(&db, &vault, SECRET, "bad").unwrap();
    }
    assert_eq!(
        connectors::run_with(&state, &CoinGecko, &format!("http://{addr}"))
            .await
            .unwrap_err(),
        RunError::Failed(crate::connectors::ConnectorError::Unauthorized)
    );
    assert_eq!(counter.load(std::sync::atomic::Ordering::Relaxed), 1);
}
