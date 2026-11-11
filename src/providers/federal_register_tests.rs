//! Federal Register connector tests against a loopback server serving the documented
//! response shape. No external network.

use crate::{
    connectors::{self, ConnectorError, RunError, Skip},
    documents,
    fetch::Fetcher,
    providers::{
        federal_register::{ATTRIBUTION, FederalRegister},
        tests_support::{counting_server, enable, state_with},
    },
};

const PAGE: &str = r#"{
  "count": 2,
  "results": [
    {
      "document_number": "2026-24913",
      "title": "Air Plan Approval; Example State",
      "type": "Rule",
      "agencies": [{"name": "Environmental Protection Agency", "raw_name": "EPA"}],
      "publication_date": "2026-11-05",
      "effective_date": null,
      "effective_on": "2027-01-04",
      "html_url": "https://www.federalregister.gov/documents/2026/11/05/2026-24913/air-plan-approval",
      "abstract": "Publisher wording that must not be stored."
    },
    {
      "document_number": "2026-24920",
      "title": "Notice of Proposed Rulemaking",
      "type": "Proposed Rule",
      "agencies": [{"name": "Department of Energy"}],
      "publication_date": "2026-11-06",
      "effective_on": null,
      "html_url": "https://www.federalregister.gov/documents/2026/11/06/2026-24920/notice",
      "abstract": "Another publisher abstract."
    }
  ]
}"#;

fn routes() -> Vec<(&'static str, &'static str)> {
    vec![("/api/v1/documents.json", PAGE)]
}

fn use_region(db: &crate::db::Database, code: &str, enabled: bool) {
    db.conn
        .execute(
            "INSERT OR IGNORE INTO regions(code,parent_code,label_vi,label_en,enabled) VALUES (?1,NULL,?1,?1,?2)",
            rusqlite::params![code, enabled],
        )
        .unwrap();
}

#[tokio::test]
async fn stores_metadata_only_and_never_the_publisher_abstract() {
    let (base, hits) = counting_server(routes()).await;
    let state = state_with(Fetcher::allow_loopback_for_tests());
    {
        let db = state.db.lock().unwrap();
        use_region(&db, "us", true);
        enable(&db, &FederalRegister, "us");
    }
    let outcome = connectors::run_with(&state, &FederalRegister, &base)
        .await
        .expect("collects");
    assert_eq!(outcome.documents, 2);
    assert_eq!(outcome.note.as_deref(), Some(ATTRIBUTION));
    assert_eq!(
        hits.load(std::sync::atomic::Ordering::Relaxed),
        2,
        "the mock answers both pages; the connector stops when a page is empty"
    );
    let db = state.db.lock().unwrap();
    let rule = documents::get(&db, "federal-register", "2026-24913")
        .unwrap()
        .unwrap();
    assert_eq!(rule.effective_on.as_deref(), Some("2027-01-04"));
    assert_eq!(rule.published_on.as_deref(), Some("2026-11-05"));
    assert_eq!(rule.agency, "Environmental Protection Agency");
    assert_eq!(rule.doc_type, "rule");
    assert_eq!(rule.summary, None, "no publisher abstract is stored");
    let proposed = documents::get(&db, "federal-register", "2026-24920")
        .unwrap()
        .unwrap();
    assert_eq!(
        proposed.effective_on, None,
        "a null effective date stays null"
    );
    assert_eq!(proposed.doc_type, "proposed_rule");
    let leaked: i64 = db
        .conn
        .query_row(
            "SELECT COUNT(*) FROM documents WHERE title LIKE '%Publisher%' OR COALESCE(summary,'') LIKE '%publisher%'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(leaked, 0);
}

#[tokio::test]
async fn a_second_run_is_idempotent() {
    let (base, _hits) = counting_server(routes()).await;
    let state = state_with(Fetcher::allow_loopback_for_tests());
    {
        let db = state.db.lock().unwrap();
        use_region(&db, "us", true);
        enable(&db, &FederalRegister, "us");
    }
    connectors::run_with(&state, &FederalRegister, &base)
        .await
        .unwrap();
    state
        .db
        .lock()
        .unwrap()
        .conn
        .execute(
            "UPDATE connectors SET cache_seconds=0, next_run_at=NULL, last_run_at=NULL WHERE id=?1",
            ["federal-register"],
        )
        .unwrap();
    let again = connectors::run_with(&state, &FederalRegister, &base)
        .await
        .unwrap();
    assert_eq!(again.documents, 0, "nothing new on the second pass");
    assert_eq!(
        documents::count(&state.db.lock().unwrap(), "federal-register").unwrap(),
        2
    );
}

#[tokio::test]
async fn a_query_count_above_the_published_ceiling_is_refused() {
    let _ = ConnectorError::Provider;
    // The guard in the connector refuses any window reporting more than 10,000 hits.
    let body: &'static str = Box::leak(
        format!(
            r#"{{"count": 10001, "results": [{}]}}"#,
            r#"{"document_number":"1","title":"t","html_url":"https://x.test/1"}"#
        )
        .into_boxed_str(),
    );
    let (base, _hits) = counting_server(vec![("/api/v1/documents.json", body)]).await;
    let state = state_with(Fetcher::allow_loopback_for_tests());
    {
        let db = state.db.lock().unwrap();
        use_region(&db, "us", true);
        enable(&db, &FederalRegister, "us");
    }
    assert_eq!(
        connectors::run_with(&state, &FederalRegister, &base)
            .await
            .unwrap_err(),
        RunError::Failed(ConnectorError::InvalidData)
    );
    assert_eq!(
        documents::count(&state.db.lock().unwrap(), "federal-register").unwrap(),
        0
    );
}

#[tokio::test]
async fn a_disabled_region_never_calls_the_provider() {
    let (base, hits) = counting_server(routes()).await;
    let state = state_with(Fetcher::allow_loopback_for_tests());
    {
        let db = state.db.lock().unwrap();
        use_region(&db, "us", false);
        enable(&db, &FederalRegister, "us");
    }
    assert_eq!(
        connectors::run_with(&state, &FederalRegister, &base)
            .await
            .unwrap_err(),
        RunError::Skipped(Skip::RegionOff)
    );
    assert_eq!(hits.load(std::sync::atomic::Ordering::Relaxed), 0);
}
