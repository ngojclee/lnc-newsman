//! mempool.space connector tests against a loopback server serving the documented
//! response shapes. No external network.

use super::*;
use crate::{
    connectors::{self, ConnectorConfig},
    fetch::Fetcher,
    providers::tests_support::{counting_server, enable, state_with},
};

const FEES: &str = r#"{"fastestFee":12,"halfHourFee":9,"hourFee":6,"economyFee":3,"minimumFee":1}"#;
const MEMPOOL: &str = r#"{"count":48213,"vsize":78432112,"total_fee":51301234,"fee_histogram":[]}"#;
const HASHRATE: &str = r#"{"hashrates":[{"timestamp":"2026-11-10T00:00:00.000Z","avgHashrate":7.0e20}],"difficulty":[{"time":1794000000,"height":900000,"difficulty":1.1e14,"adjustment":1.5}],"currentHashrate":7.1e20,"currentDifficulty":1.1e14}"#;
const HEIGHT: &str = "900123";

fn routes() -> Vec<(&'static str, &'static str)> {
    vec![
        ("/api/v1/fees/recommended", FEES),
        ("/api/mempool", MEMPOOL),
        ("/api/v1/mining/hashrate/3d", HASHRATE),
        ("/api/v1/blocks/tip/height", HEIGHT),
    ]
}

#[tokio::test]
async fn writes_each_series_with_attribution_and_no_altered_values() {
    let (base, hits) = counting_server(routes()).await;
    let state = state_with(Fetcher::allow_loopback_for_tests());
    {
        let db = state.db.lock().unwrap();
        enable(&db, &Mempool, "global");
    }
    let outcome = match connectors::run_with(&state, &Mempool, &base).await {
        Ok(outcome) => outcome,
        Err(err) => panic!("collects failed: {err:?} base={base}"),
    };
    assert_eq!(outcome.observations, 8);
    assert_eq!(
        hits.load(std::sync::atomic::Ordering::Relaxed),
        4,
        "exactly the four documented endpoints"
    );
    let db = state.db.lock().unwrap();
    let fee = crate::series::observations(&db, "BTC-FEE-FASTEST", None, 5).unwrap();
    assert_eq!(fee.len(), 1);
    assert_eq!(fee[0].value, 12.0, "the provider value is stored unchanged");
    assert_eq!(fee[0].quality, "final");
    let stored = crate::series::series(&db, "BTC-FEE-FASTEST")
        .unwrap()
        .unwrap();
    assert_eq!(stored.attribution, "Source: mempool.space");
    assert_eq!(stored.unit, "sat/vB");
    let height = crate::series::observations(&db, "BTC-TIP-HEIGHT", None, 5).unwrap();
    assert_eq!(height[0].value, 900_123.0);
    assert_eq!(
        crate::series::series(&db, "BTC-MEMPOOL-TX")
            .unwrap()
            .unwrap()
            .attribution,
        ATTRIBUTION
    );
}

#[tokio::test]
async fn a_repeat_poll_in_the_same_hour_does_not_duplicate() {
    let (base, _hits) = counting_server(routes()).await;
    let state = state_with(Fetcher::allow_loopback_for_tests());
    {
        let db = state.db.lock().unwrap();
        enable(&db, &Mempool, "global");
    }
    connectors::run_with(&state, &Mempool, &base).await.unwrap();
    state
        .db
        .lock()
        .unwrap()
        .conn
        .execute(
            "UPDATE connectors SET cache_seconds=0, next_run_at=NULL, last_run_at=NULL WHERE id=?1",
            [ID],
        )
        .unwrap();
    connectors::run_with(&state, &Mempool, &base).await.unwrap();
    let db = state.db.lock().unwrap();
    let rows: i64 = db
        .conn
        .query_row(
            "SELECT COUNT(*) FROM observations o JOIN series s ON s.id=o.series_id WHERE s.code='BTC-FEE-FASTEST'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(rows, 1, "the same hour stores one observation");
}

#[tokio::test]
async fn a_429_stops_the_run_without_retrying() {
    let app = axum::Router::new().route(
        "/api/v1/fees/recommended",
        axum::routing::get(|| async { axum::http::StatusCode::TOO_MANY_REQUESTS }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let state = state_with(Fetcher::allow_loopback_for_tests());
    {
        let db = state.db.lock().unwrap();
        enable(&db, &Mempool, "global");
    }
    assert_eq!(
        connectors::run_with(&state, &Mempool, &format!("http://{addr}"))
            .await
            .unwrap_err(),
        connectors::RunError::Failed(crate::connectors::ConnectorError::Provider)
    );
    let db = state.db.lock().unwrap();
    let spent: i64 = db
        .conn
        .query_row(
            "SELECT requests_today FROM connectors WHERE id=?1",
            [ID],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(spent, 1, "one request, no retry after 429");
}

#[tokio::test]
async fn a_disabled_region_never_calls_the_provider() {
    let (base, hits) = counting_server(routes()).await;
    let state = state_with(Fetcher::allow_loopback_for_tests());
    {
        let db = state.db.lock().unwrap();
        db.conn
            .execute(
                "INSERT INTO regions(code,parent_code,label_vi,label_en,enabled) VALUES ('vn',NULL,'VN','VN',0)",
                [],
            )
            .unwrap();
        enable(&db, &Mempool, "vn");
    }
    assert_eq!(
        connectors::run_with(&state, &Mempool, &base)
            .await
            .unwrap_err(),
        connectors::RunError::Skipped(connectors::Skip::RegionOff)
    );
    assert_eq!(hits.load(std::sync::atomic::Ordering::Relaxed), 0);
    let _ = ConnectorConfig::default();
}

#[test]
fn the_documented_bodies_match_the_connector_structs() {
    // If any of these fail to deserialize, the connector would report a provider error
    // against a healthy provider — the most likely test failure to misdiagnose.
    let _: RecommendedFees = serde_json::from_str(FEES).unwrap();
    let _: MempoolStats = serde_json::from_str(MEMPOOL).unwrap();
    let _: Hashrate = serde_json::from_str(HASHRATE).unwrap();
    let _: i64 = serde_json::from_str(HEIGHT).unwrap();
}
