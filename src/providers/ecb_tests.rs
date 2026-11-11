//! ECB SDMX connector tests against a loopback server serving a documented-shaped
//! SDMX-JSON body. No external network.

use super::*;
use crate::{
    connectors::{self, ConnectorError, RunError, Skip},
    fetch::Fetcher,
    providers::tests_support::{counting_server, enable, state_with},
};

const EXR: &str = r#"{
  "header": {"id": "mock", "prepared": "2026-11-11T08:00:00Z"},
  "data": {
    "structure": {
      "dimensions": {
        "observation": [
          {"id": "TIME_PERIOD", "values": [{"id": "2026-11-07"}, {"id": "2026-11-10"}]}
        ]
      }
    },
    "data_sets": [
      {"series": {"0:0:0:0:0": {"observations": {"0": [1.0712], "1": [1.0734]}}}}
    ]
  }
}"#;

#[tokio::test]
async fn stores_values_verbatim_with_the_required_credit() {
    let (base, hits) = counting_server(vec![("/service/data/EXR/D.USD.EUR.SP00.A", EXR)]).await;
    let state = state_with(Fetcher::allow_loopback_for_tests());
    {
        let db = state.db.lock().unwrap();
        db.conn
            .execute(
                "INSERT INTO regions(code,parent_code,label_vi,label_en,enabled) VALUES ('eu',NULL,'EU','EU',1)",
                [],
            )
            .unwrap();
        enable(&db, &Ecb, "eu");
    }
    let outcome = connectors::run_with(&state, &Ecb, &base)
        .await
        .expect("collects");
    assert_eq!(outcome.observations, 2);
    assert_eq!(
        hits.load(std::sync::atomic::Ordering::Relaxed),
        1,
        "one request per flow"
    );
    let db = state.db.lock().unwrap();
    let saved = crate::series::series(&db, "ECB-EUR-USD").unwrap().unwrap();
    assert_eq!(saved.attribution, "Source: ECB statistics");
    assert_eq!(saved.unit, "USD per EUR");
    assert_eq!(saved.region, "eu");
    let points = crate::series::observations(&db, "ECB-EUR-USD", None, 10).unwrap();
    assert_eq!(points.len(), 2);
    assert_eq!(points[0].value, 1.0734, "value stored as published");
    assert_eq!(points[1].value, 1.0712);
}

#[test]
fn the_query_uses_the_documented_path_and_keeps_the_credit_line() {
    let flow = FLOWS[0];
    let path = format!(
        "/service/data/{}/{}?format=jsondata&lastNObservations={}",
        flow.flow, flow.key, flow.observations
    );
    assert_eq!(
        path,
        "/service/data/EXR/D.USD.EUR.SP00.A?format=jsondata&lastNObservations=30"
    );
    assert_eq!(attribution(), "Source: ECB statistics");
    assert_eq!(ATTRIBUTION, "Source: ECB statistics");
}

#[test]
fn sdmx_bodies_parse_into_periods_and_values() {
    let doc: Sdmx = serde_json::from_str(EXR).unwrap();
    let periods = parse_doc(&doc, &FLOWS[0]).unwrap();
    assert_eq!(periods.len(), 2);
    assert_eq!(periods[0].1, 1.0712, "no rounding, no conversion");
    assert_eq!(periods[1].1, 1.0734);
    let first = period_start("2026-11-07", "daily").unwrap();
    assert_eq!(
        chrono::DateTime::from_timestamp(first, 0)
            .unwrap()
            .format("%Y-%m-%d")
            .to_string(),
        "2026-11-07"
    );
    assert_eq!(period_start("2026-11-07", "daily"), Some(first));
    assert_eq!(period_start("not-a-date", "daily"), None);
}

#[test]
fn empty_or_missing_observations_are_refused_not_filled() {
    let empty: Sdmx = serde_json::from_str(
        r#"{"data":{"structure":{"dimensions":{"observation":[{"id":"TIME_PERIOD","values":[]}]}},"data_sets":[{"series":{}}]}}"#,
    )
    .unwrap();
    assert!(parse_doc(&empty, &FLOWS[0]).is_err());

    // A missing value (`null`) is skipped, never replaced by zero or carried forward.
    let with_gap: Sdmx = serde_json::from_str(
        r#"{"data":{"structure":{"dimensions":{"observation":[{"id":"TIME_PERIOD","values":[{"id":"2026-11-07"},{"id":"2026-11-10"}]}]}},"data_sets":[{"series":{"0":{ "observations": {"0":[null],"1":[1.0734]} }}}]}}"#,
    )
    .unwrap();
    let periods = parse_doc(&with_gap, &FLOWS[0]).unwrap();
    assert_eq!(periods.len(), 1, "the gap is not invented");
    assert_eq!(periods[0].1, 1.0734);
}

#[test]
fn monthly_and_quarterly_labels_map_to_the_start_of_the_period() {
    assert!(period_start("2026-09", "monthly").is_some());
    assert!(period_start("2026-Q3", "quarterly").is_some());
    assert!(period_start("2026", "yearly").is_some());
    assert_eq!(period_start("2026-09", "fortnightly"), None);
}

#[tokio::test]
async fn a_missing_route_surfaces_as_a_fetch_failure_not_as_data() {
    let (base, _hits) = counting_server(vec![("/service/data/EXR/D.USD.EUR.SP00.A", EXR)]).await;
    let state = state_with(Fetcher::allow_loopback_for_tests());
    {
        let db = state.db.lock().unwrap();
        db.conn
            .execute(
                "INSERT INTO regions(code,parent_code,label_vi,label_en,enabled) VALUES ('eu',NULL,'EU','EU',1)",
                [],
            )
            .unwrap();
        enable(&db, &Ecb, "eu");
    }
    // A wrong root answers 404 (axum's fallback), which must surface as a fetch failure.
    let wrong = format!("{base}/nope");
    assert!(matches!(
        connectors::run_with(&state, &Ecb, &wrong).await,
        Err(RunError::Failed(ConnectorError::Fetch))
    ));
}

#[tokio::test]
async fn a_disabled_region_never_calls_the_provider() {
    let (base, hits) = counting_server(vec![("/service/data/EXR/D.USD.EUR.SP00.A", EXR)]).await;
    let state = state_with(Fetcher::allow_loopback_for_tests());
    {
        let db = state.db.lock().unwrap();
        db.conn
            .execute(
                "INSERT INTO regions(code,parent_code,label_vi,label_en,enabled) VALUES ('eu',NULL,'EU','EU',0)",
                [],
            )
            .unwrap();
        enable(&db, &Ecb, "eu");
    }
    assert_eq!(
        connectors::run_with(&state, &Ecb, &base).await.unwrap_err(),
        RunError::Skipped(Skip::RegionOff)
    );
    assert_eq!(hits.load(std::sync::atomic::Ordering::Relaxed), 0);
}
