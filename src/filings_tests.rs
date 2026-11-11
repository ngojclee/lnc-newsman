//! Filings store tests: idempotence, revisions, region visibility, disclaimer.

use super::*;
use crate::{AppState, config, fetch};
use axum::{
    body::Body,
    http::{Request, StatusCode, header},
};
use http_body_util::BodyExt;
use std::sync::Arc;
use tower::ServiceExt;

fn state() -> SharedState {
    let dir = std::env::temp_dir().join(format!(
        "lnc-newsman-filings-{}-{}",
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
            secure_cookie: true,
            version: "test",
        },
        db::Database::in_memory().unwrap(),
        fetch::Fetcher::public_only(),
    ))
}

/// Tests that store US companies need the region to exist. It is created disabled, and
/// the tests that need it enabled do so explicitly.
fn with_us(state: &SharedState) {
    state
        .db
        .lock()
        .unwrap()
        .conn
        .execute(
            "INSERT OR IGNORE INTO regions(code,parent_code,label_vi,label_en,enabled) VALUES ('us',NULL,'US','US',0)",
            [],
        )
        .unwrap();
}

fn filing(accession: &str, form: &str) -> NewFiling<'static> {
    NewFiling {
        provider: "sec-edgar",
        accession: Box::leak(accession.to_string().into_boxed_str()),
        form_type: Box::leak(form.to_string().into_boxed_str()),
        period_end: Some("2026-09-30"),
        filed_on: "2026-11-01",
        url: "https://www.sec.gov/Archives/edgar/data/320193/000032019326000001/aapl.htm",
        title: "Apple Inc.",
    }
}

#[test]
fn forms_map_to_the_expected_kind() {
    assert_eq!(filing_kind("10-K"), "annual");
    assert_eq!(filing_kind("10-Q"), "quarterly");
    assert_eq!(filing_kind("8-K"), "current");
    assert_eq!(filing_kind("S-1"), "registration");
    assert_eq!(filing_kind("424B4"), "registration");
    assert_eq!(filing_kind("SC 13G"), "other");
}

#[test]
fn only_known_concepts_become_metrics() {
    assert_eq!(normalise_concept("us-gaap:Revenues"), Some("revenue"));
    assert_eq!(
        normalise_concept("us-gaap:RevenueFromContractWithCustomerExcludingAssessedTax"),
        Some("revenue")
    );
    assert_eq!(normalise_concept("us-gaap:Assets"), Some("total_assets"));
    assert_eq!(
        normalise_concept("us-gaap:SomeCustomThing"),
        None,
        "an unknown concept is never stored under a guessed metric"
    );
}

#[test]
fn a_repeat_filing_is_ignored_and_a_bad_one_is_refused() {
    let state = state();
    with_us(&state);
    let db = state.db.lock().unwrap();
    let id = upsert_company(
        &db,
        "sec-edgar",
        "0000320193",
        "us",
        "Apple Inc.",
        Some("AAPL"),
        Some("Nasdaq"),
    )
    .unwrap();
    assert!(insert_filing(&db, id, &filing("0000320193-26-000001", "10-K")).unwrap());
    assert!(
        !insert_filing(&db, id, &filing("0000320193-26-000001", "10-K")).unwrap(),
        "the same accession is not stored twice"
    );
    let mut bad = filing("0000320193-26-000002", "10-Q");
    bad.url = "javascript:alert(1)";
    assert!(insert_filing(&db, id, &bad).is_err());
    let mut bad_date = filing("0000320193-26-000003", "10-Q");
    bad_date.filed_on = "01/11/2026";
    assert!(insert_filing(&db, id, &bad_date).is_err());
}

#[test]
fn a_restatement_becomes_a_new_revision() {
    let state = state();
    with_us(&state);
    let db = state.db.lock().unwrap();
    let id = upsert_company(
        &db,
        "sec-edgar",
        "0000320193",
        "us",
        "Apple Inc.",
        None,
        None,
    )
    .unwrap();
    let fact = |value: f64, as_of: i64| NewFact {
        provider: "sec-edgar",
        metric: "revenue",
        concept: "us-gaap:Revenues",
        period_start: Some("2025-10-01"),
        period_end: "2026-09-30",
        period_kind: "duration",
        fiscal_period: "FY",
        fiscal_year: Some(2026),
        value,
        unit: "USD",
        currency: "USD",
        as_of,
        quality: "final",
        source_url: "https://www.sec.gov/edgar/sec-api-documentation",
    };
    assert_eq!(
        insert_fact(&db, id, &fact(391_035.0, 1_800_000_000)).unwrap(),
        1
    );
    assert_eq!(
        insert_fact(&db, id, &fact(391_035.0, 1_800_000_100)).unwrap(),
        1,
        "the same number is not a new revision"
    );
    assert_eq!(
        insert_fact(&db, id, &fact(391_040.0, 1_800_000_200)).unwrap(),
        2,
        "a corrected number becomes revision 2"
    );
    let rows = facts_for(&db, id, Some("revenue"), 10).unwrap();
    assert_eq!(rows.len(), 1, "one row per period, the latest revision");
    assert_eq!(rows[0]["value"], 391_040.0);
    assert_eq!(rows[0]["revision"], 2);
    let history: i64 = db
        .conn
        .query_row("SELECT COUNT(*) FROM financial_facts", [], |r| r.get(0))
        .unwrap();
    assert_eq!(history, 2, "history is kept");
}

#[test]
fn a_fact_with_an_unknown_metric_or_a_non_finite_value_is_refused() {
    let state = state();
    with_us(&state);
    with_us(&state);
    let db = state.db.lock().unwrap();
    let id = upsert_company(&db, "sec-edgar", "1", "us", "X", None, None).unwrap();
    let base = NewFact {
        provider: "sec-edgar",
        metric: "mystery",
        concept: "us-gaap:Mystery",
        period_start: None,
        period_end: "2026-09-30",
        period_kind: "instant",
        fiscal_period: "",
        fiscal_year: None,
        value: 1.0,
        unit: "USD",
        currency: "USD",
        as_of: 1_800_000_000,
        quality: "final",
        source_url: "",
    };
    assert!(insert_fact(&db, id, &base).is_err());
    let nan = NewFact {
        metric: "revenue",
        concept: "us-gaap:Revenues",
        value: f64::NAN,
        ..base
    };
    assert!(insert_fact(&db, id, &nan).is_err());
}

#[tokio::test]
async fn the_reader_hides_companies_of_disabled_regions() {
    let state = state();
    with_us(&state);
    {
        let db = state.db.lock().unwrap();
        let us = upsert_company(&db, "sec-edgar", "1", "us", "US Co", None, None).unwrap();
        let global =
            upsert_company(&db, "sec-edgar", "2", "global", "Global Co", None, None).unwrap();
        insert_filing(&db, us, &filing("a-1", "10-K")).unwrap();
        insert_filing(&db, global, &filing("a-2", "10-K")).unwrap();
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
    let response = get("/api/v1/companies").await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body: serde_json::Value =
        serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes()).unwrap();
    let names: Vec<_> = body["data"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| row["name"].as_str().unwrap())
        .collect();
    assert_eq!(
        names,
        ["Global Co"],
        "a company in a disabled region stays hidden"
    );
    assert!(
        body["disclaimer"]
            .as_str()
            .unwrap()
            .contains("Not investment")
    );
}
