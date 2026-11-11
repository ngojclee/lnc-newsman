//! FRED connector tests: key required, key never stored/logged, values verbatim.
//! Loopback server only.

use super::*;
use crate::{
    connectors::{self, RunError, Skip},
    fetch::Fetcher,
    providers::tests_support::{counting_server, enable, state_with},
    secrets::SecretBox,
};

const BODY: &str = r#"{
  "realtime_start": "2026-11-01",
  "realtime_end": "2026-11-01",
  "observation_start": "1999-01-01",
  "seriess": [{"id": "FEDFUNDS", "title": "Federal Funds Effective Rate", "units": "Percent", "frequency": "Monthly"}],
  "observations": [
    {"realtime_start": "2026-11-01", "realtime_end": "2026-11-01", "date": "2026-09-01", "value": "4.83"},
    {"date": "2026-08-01", "value": "4.33"},
    {"date": "2026-07-01", "value": "."}
  ]
}"#;

fn state_with_key() -> (std::sync::Arc<crate::AppState>, SecretBox) {
    let vault = SecretBox::from_bytes([7; 32], None);
    let mut state = state_with(Fetcher::allow_loopback_for_tests());
    std::sync::Arc::get_mut(&mut state)
        .expect("sole owner")
        .secrets = Some(vault.clone());
    (state, vault)
}

#[tokio::test]
async fn without_a_key_the_connector_never_runs() {
    let (base, hits) = counting_server(vec![("/fred/series/observations", BODY)]).await;
    let state = state_with(Fetcher::allow_loopback_for_tests());
    {
        let db = state.db.lock().unwrap();
        db.conn
            .execute(
                "INSERT OR IGNORE INTO regions(code,parent_code,label_vi,label_en,enabled) VALUES ('us',NULL,'US','US',1)",
                [],
            )
            .unwrap();
        enable(&db, &Fred, "us");
    }
    assert_eq!(
        connectors::run_with(&state, &Fred, &base)
            .await
            .unwrap_err(),
        RunError::Skipped(Skip::SecretMissing)
    );
    assert_eq!(hits.load(std::sync::atomic::Ordering::Relaxed), 0);
}

#[tokio::test]
async fn a_configured_key_writes_values_verbatim_and_the_credit_line() {
    let (base, _hits) = counting_server(vec![("/fred/series/observations", BODY)]).await;
    let (state, vault) = state_with_key();
    {
        let db = state.db.lock().unwrap();
        db.conn
            .execute(
                "INSERT OR IGNORE INTO regions(code,parent_code,label_vi,label_en,enabled) VALUES ('us',NULL,'US','US',1)",
                [],
            )
            .unwrap();
        enable(&db, &Fred, "us");
        crate::secrets::set(&db, &vault, SECRET, "test-key-value").unwrap();
    }
    let outcome = connectors::run_with(&state, &Fred, &base)
        .await
        .expect("collects");
    assert_eq!(
        outcome.observations, 2,
        "the '.' placeholder is skipped, never filled"
    );
    let db = state.db.lock().unwrap();
    let saved = crate::series::series(&db, "FEDFUNDS").unwrap().unwrap();
    assert_eq!(saved.attribution, ATTRIBUTION);
    assert_eq!(saved.unit, "Percent");
    assert_eq!(saved.frequency, "monthly");
    let points = crate::series::observations(&db, "FEDFUNDS", None, 10).unwrap();
    assert_eq!(points.len(), 2);
    assert!(points.iter().all(|p| p.value != 0.0));
    let leak: i64 = db
        .conn
        .query_row(
            "SELECT COUNT(*) FROM series WHERE code LIKE '%test-key-value%' OR attribution LIKE '%test-key-value%'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(leak, 0, "the key is never stored as data");
}

#[test]
fn a_key_in_a_url_is_redacted_before_any_log_line() {
    let redacted = redact(
        "https://api.stlouisfed.org/fred/series/observations?series_id=FEDFUNDS&api_key=abc123&file_type=json",
    );
    assert!(!redacted.contains("abc123"));
    assert!(redacted.contains("api_key=[redacted]"));
    assert!(redacted.contains("file_type=json"));
}

#[test]
fn missing_values_are_skipped_not_filled() {
    assert_eq!(month_start("2026-09-15"), month_start("2026-09-01"));
    assert_eq!(month_start("bad"), None);
}
