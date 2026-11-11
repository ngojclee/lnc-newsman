//! Time-series storage tests: revisions, as-of reads and region visibility.

use super::*;
use crate::{AppState, config, fetch, regions};
use axum::{
    body::Body,
    http::{Request, StatusCode, header},
};
use http_body_util::BodyExt;
use std::sync::Arc;
use tower::ServiceExt;

const DAY: i64 = 86_400;

fn state() -> SharedState {
    let dir = std::env::temp_dir().join(format!(
        "lnc-newsman-series-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).unwrap();
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
        fetch::Fetcher::public_only(),
    ))
}

fn seed_region(db: &db::Database, code: &str, parent: Option<&str>, enabled: bool) {
    db.conn
        .execute(
            "INSERT OR IGNORE INTO regions(code,parent_code,label_vi,label_en,enabled) VALUES (?1,?2,?1,?1,?3)",
            rusqlite::params![code, parent, enabled],
        )
        .unwrap();
}

fn seed_series(db: &db::Database, code: &str, region: &str) {
    upsert_series(
        db,
        &SeriesDef {
            code,
            provider: "mock",
            region,
            label_vi: "Chỉ số",
            label_en: "Index",
            unit: "index",
            frequency: "daily",
            attribution: "Source: mock",
        },
    )
    .unwrap();
    db.conn
        .execute("UPDATE series SET enabled=1 WHERE code=?1", [code])
        .unwrap();
}

#[test]
fn a_repeated_poll_is_idempotent_and_a_correction_adds_a_revision() {
    let state = state();
    let db = state.db.lock().unwrap();
    seed_region(&db, "us", None, true);
    seed_series(&db, "CPI", "us");
    let period = 1_800_000_000;
    assert_eq!(
        record(
            &db,
            &NewObservation::final_value("CPI", period, period + DAY, 3.1)
        )
        .unwrap(),
        1
    );
    assert_eq!(
        record(
            &db,
            &NewObservation::final_value("CPI", period, period + DAY, 3.1)
        )
        .unwrap(),
        0,
        "polling the same value again must not duplicate it"
    );
    assert_eq!(
        revise(&db, "CPI", period, period + 2 * DAY, 3.2, None).unwrap(),
        2,
        "a correction becomes revision 2"
    );
    assert_eq!(
        revise(&db, "CPI", period, period + 3 * DAY, 3.2, None).unwrap(),
        0,
        "an unchanged correction is a no-op"
    );
    let history: i64 = db
        .conn
        .query_row("SELECT COUNT(*) FROM observations", [], |r| r.get(0))
        .unwrap();
    assert_eq!(history, 2, "history keeps the superseded revision");
}

#[test]
fn as_of_reads_reproduce_what_was_known_then() {
    let state = state();
    let db = state.db.lock().unwrap();
    seed_region(&db, "us", None, true);
    seed_series(&db, "CPI", "us");
    let period = 1_800_000_000;
    record(
        &db,
        &NewObservation::final_value("CPI", period, period + DAY, 3.1),
    )
    .unwrap();
    revise(&db, "CPI", period, period + 5 * DAY, 3.4, None).unwrap();

    let now = observations(&db, "CPI", None, 10).unwrap();
    assert_eq!(now.len(), 1, "one row per period");
    assert_eq!((now[0].value, now[0].revision), (3.4, 2));

    let then = observations(&db, "CPI", Some(period + 2 * DAY), 10).unwrap();
    assert_eq!(
        (then[0].value, then[0].revision),
        (3.1, 1),
        "an as-of query before the correction must see the original value"
    );
}

#[test]
fn invalid_input_is_refused() {
    let state = state();
    let db = state.db.lock().unwrap();
    seed_region(&db, "us", None, true);
    seed_series(&db, "CPI", "us");
    assert!(
        record(
            &db,
            &NewObservation::final_value("CPI", 1_800_000_000, 1_700_000_000, 1.0)
        )
        .is_err(),
        "as_of before observed_at"
    );
    assert!(
        record(
            &db,
            &NewObservation::final_value("CPI", 1_800_000_000, 1_800_000_000, f64::NAN)
        )
        .is_err(),
        "NaN value"
    );
    assert!(
        record(
            &db,
            &NewObservation {
                quality: "bogus",
                ..NewObservation::final_value("CPI", 1_800_000_000, 1_800_000_000, 1.0)
            }
        )
        .is_err(),
        "unknown quality"
    );
    assert!(
        record(
            &db,
            &NewObservation::final_value("missing", 1_800_000_000, 1_800_000_000, 1.0)
        )
        .is_err()
    );
    let def = |region: &'static str, frequency: &'static str| SeriesDef {
        code: "X",
        provider: "p",
        region,
        label_vi: "a",
        label_en: "b",
        unit: "",
        frequency,
        attribution: "",
    };
    assert!(upsert_series(&db, &def("nowhere", "daily")).is_err());
    assert!(upsert_series(&db, &def("us", "hourly-ish")).is_err());
}

#[test]
fn events_are_deduplicated_and_validated() {
    let state = state();
    let db = state.db.lock().unwrap();
    seed_region(&db, "us", None, true);
    let start = 1_800_000_000;
    let event = |kind: &'static str| NewEvent {
        provider: "fred",
        region: "us",
        series_code: None,
        kind,
        title: "CPI",
        url: None,
        starts_at: start,
        attribution: "Source: mock",
    };
    assert_eq!(record_event(&db, &event("release")).unwrap(), 1);
    assert_eq!(record_event(&db, &event("release")).unwrap(), 0);
    assert!(record_event(&db, &event("party")).is_err());
    assert_eq!(events(&db, start - 1, start + 1, 10).unwrap().len(), 1);
}

#[tokio::test]
async fn the_read_api_hides_series_from_disabled_regions() {
    let state = state();
    {
        let db = state.db.lock().unwrap();
        seed_region(&db, "eu", None, true);
        seed_region(&db, "fr", Some("eu"), true);
        seed_region(&db, "us", None, false);
        seed_series(&db, "FR-INDEX", "fr");
        seed_series(&db, "US-INDEX", "us");
        record(
            &db,
            &NewObservation::final_value("FR-INDEX", 1_800_000_000, 1_800_000_000, 1.0),
        )
        .unwrap();
        record(
            &db,
            &NewObservation::final_value("US-INDEX", 1_800_000_000, 1_800_000_000, 2.0),
        )
        .unwrap();
    }
    let app = crate::router(state);
    let get = |path: &str| {
        app.clone().oneshot(
            Request::builder()
                .uri(path)
                .header(header::HOST, "newsman.test")
                .header(header::AUTHORIZATION, "Bearer test-api-key")
                .body(Body::empty())
                .unwrap(),
        )
    };
    let response = get("/api/v1/series").await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body: serde_json::Value =
        serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes()).unwrap();
    let codes: Vec<_> = body["data"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| row["code"].as_str().unwrap())
        .collect();
    assert_eq!(codes, ["FR-INDEX"], "the disabled US series must be hidden");
    assert_eq!(body["effective"], serde_json::json!(["eu", "fr", "global"]));

    assert_eq!(
        get("/api/v1/series/US-INDEX/observations")
            .await
            .unwrap()
            .status(),
        StatusCode::NOT_FOUND,
        "observations of a hidden series are not readable"
    );
    let response = get("/api/v1/series/FR-INDEX/observations?as_of=1800000000")
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body: serde_json::Value =
        serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(body["data"].as_array().unwrap().len(), 1);
    assert_eq!(body["series"]["region"], "fr");
    assert_eq!(
        get("/api/v1/series?region=us").await.unwrap().status(),
        StatusCode::BAD_REQUEST
    );
}

#[test]
fn series_are_global_by_default_in_regions_terms() {
    let state = state();
    let db = state.db.lock().unwrap();
    // `global` exists from migration 5, so a series with no explicit region still works.
    let row = upsert_series(
        &db,
        &SeriesDef {
            code: "BTC-USD",
            provider: "mock",
            region: "global",
            label_vi: "BTC",
            label_en: "BTC",
            unit: "USD",
            frequency: "hourly",
            attribution: "Source: mock",
        },
    )
    .unwrap();
    assert_eq!(row.region, "global");
    assert!(regions::region_enabled(&db, "global").unwrap());
    let _ = DAY;
}
