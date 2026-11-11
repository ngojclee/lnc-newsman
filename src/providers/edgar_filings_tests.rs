//! EDGAR filings/XBRL connector tests against a loopback server serving documented
//! response shapes. No external network.

use crate::{
    connectors::{self, ConnectorConfig, ConnectorError, RunError, Skip},
    fetch::Fetcher,
    filings,
    providers::{
        edgar_filings::{EdgarFilings, ID},
        tests_support::{counting_server, enable, state_with},
    },
};

const SUBMISSIONS: &str = r#"{
  "cik": "0000320193",
  "name": "Apple Inc.",
  "tickers": ["AAPL"],
  "exchanges": ["Nasdaq"],
  "filings": {
    "recent": {
      "form": ["10-K", "10-Q", "8-K", "S-1", "SC 13G"],
      "accessionNumber": ["0000320193-26-000010", "0000320193-26-000011", "0000320193-26-000012", "0000320193-26-000013", "0000320193-26-000014"],
      "filingDate": ["2026-11-01", "2026-08-01", "2026-07-15", "2026-06-01", "2026-05-01"],
      "reportDate": ["2026-09-30", "2026-06-30", "", "2026-05-30", ""],
      "primaryDocument": ["aapl-20260930.htm", "aapl-20260630.htm", "aapl-8k.htm", "s1.htm", "sc13g.htm"]
    }
  }
}"#;

const FACTS: &str = r#"{
  "cik": 320193,
  "facts": {
    "us-gaap": {
      "Revenues": {
        "label": "Revenues",
        "units": {
          "USD": [
            {"start": "2025-10-01", "end": "2026-09-30", "val": 391035000000, "fy": 2026, "fp": "FY", "form": "10-K", "filed": "2026-11-01"}
          ]
        }
      },
      "Assets": {
        "label": "Assets",
        "units": {
          "USD": [
            {"end": "2026-09-30", "val": 364980000000, "fy": 2026, "fp": "FY", "form": "10-K", "filed": "2026-11-01"}
          ]
        }
      },
      "SomeCustomThing": {
        "label": "Custom",
        "units": {"USD": [{"end": "2026-09-30", "val": 1.0}]}
      }
    }
  }
}"#;

fn use_region(db: &crate::db::Database, code: &str, enabled: bool) {
    db.conn
        .execute(
            "INSERT OR IGNORE INTO regions(code,parent_code,label_vi,label_en,enabled) VALUES (?1,NULL,?1,?1,?2)",
            rusqlite::params![code, enabled],
        )
        .unwrap();
}

fn configured() -> ConnectorConfig {
    ConnectorConfig {
        sec_user_agent: Some("Example Newsman admin@example.invalid".into()),
    }
}

#[tokio::test]
async fn without_a_user_agent_nothing_is_requested() {
    let (base, hits) = counting_server(vec![
        ("/submissions/CIK0000320193.json", SUBMISSIONS),
        ("/api/xbrl/companyfacts/CIK0000320193.json", FACTS),
    ])
    .await;
    let state = state_with(Fetcher::allow_loopback_for_tests());
    {
        let db = state.db.lock().unwrap();
        use_region(&db, "us", true);
        enable(&db, &EdgarFilings, "us");
    }
    assert_eq!(
        connectors::run_with(&state, &EdgarFilings, &base)
            .await
            .unwrap_err(),
        RunError::Skipped(Skip::NotConfigured)
    );
    assert_eq!(hits.load(std::sync::atomic::Ordering::Relaxed), 0);
}

#[tokio::test]
async fn filings_and_facts_are_stored_without_any_filing_body() {
    let (base, hits) = counting_server(vec![
        ("/submissions/CIK0000320193.json", SUBMISSIONS),
        ("/api/xbrl/companyfacts/CIK0000320193.json", FACTS),
    ])
    .await;
    let state = state_with(Fetcher::allow_loopback_for_tests());
    {
        let db = state.db.lock().unwrap();
        use_region(&db, "us", true);
        enable(&db, &EdgarFilings, "us");
    }
    // Inject the owner-supplied contact User-Agent without touching the environment.
    let state = {
        let mut state = state;
        std::sync::Arc::get_mut(&mut state)
            .unwrap()
            .connector_config = configured();
        state
    };
    let outcome = match connectors::run_with(&state, &EdgarFilings, &base).await {
        Ok(outcome) => outcome,
        Err(err) => panic!("collect failed: {err:?}"),
    };
    assert_eq!(
        hits.load(std::sync::atomic::Ordering::Relaxed),
        2,
        "one submissions call and one XBRL call, nothing else"
    );
    assert_eq!(
        outcome.documents, 4,
        "four recognised forms; SC 13G is ignored"
    );
    let db = state.db.lock().unwrap();
    let company = filings::company(&db, "sec-edgar", "0000320193")
        .unwrap()
        .unwrap();
    assert_eq!(company.name, "Apple Inc.");
    assert_eq!(company.ticker.as_deref(), Some("AAPL"));
    assert_eq!(company.region, "us");
    let filings_count: i64 = db
        .conn
        .query_row("SELECT COUNT(*) FROM filings", [], |r| r.get(0))
        .unwrap();
    assert_eq!(filings_count, 4);
    let registration: i64 = db
        .conn
        .query_row(
            "SELECT COUNT(*) FROM filings WHERE filing_kind='registration'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(registration, 1, "S-1 is kept as an IPO registration");
    // Facts: revenue and assets normalised; the custom concept is dropped.
    let metrics: Vec<String> = {
        let mut stmt = db
            .conn
            .prepare("SELECT DISTINCT metric FROM financial_facts ORDER BY metric")
            .unwrap();
        stmt.query_map([], |r| r.get(0))
            .unwrap()
            .collect::<rusqlite::Result<Vec<_>>>()
            .unwrap()
    };
    assert_eq!(metrics, vec!["revenue", "total_assets"]);
    let revenue = filings::facts_for(&db, company.id, Some("revenue"), 5).unwrap();
    assert_eq!(revenue[0]["value"], 391_035_000_000.0);
    assert_eq!(revenue[0]["currency"], "USD");
    assert_eq!(revenue[0]["period_kind"], "duration");
    let assets = filings::facts_for(&db, company.id, Some("total_assets"), 5).unwrap();
    assert_eq!(assets[0]["period_kind"], "instant");
    // No filing body is ever stored: the only text is title/summary, and summary is null.
    let summaries: i64 = db
        .conn
        .query_row(
            "SELECT COUNT(*) FROM filings WHERE summary IS NOT NULL",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(summaries, 0);
}

#[tokio::test]
async fn a_second_run_is_idempotent() {
    let (base, _hits) = counting_server(vec![
        ("/submissions/CIK0000320193.json", SUBMISSIONS),
        ("/api/xbrl/companyfacts/CIK0000320193.json", FACTS),
    ])
    .await;
    let state = state_with(Fetcher::allow_loopback_for_tests());
    let state = {
        let mut state = state;
        let inner = std::sync::Arc::get_mut(&mut state).unwrap();
        inner.connector_config = configured();
        state
    };
    {
        let db = state.db.lock().unwrap();
        use_region(&db, "us", true);
        enable(&db, &EdgarFilings, "us");
    }
    connectors::run_with(&state, &EdgarFilings, &base)
        .await
        .unwrap();
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
    let again = connectors::run_with(&state, &EdgarFilings, &base)
        .await
        .unwrap();
    assert_eq!(again.documents, 0, "no new filings on the second pass");
    let db = state.db.lock().unwrap();
    let facts: i64 = db
        .conn
        .query_row("SELECT COUNT(*) FROM financial_facts", [], |r| r.get(0))
        .unwrap();
    assert_eq!(facts, 2, "facts are not duplicated either");
}

#[tokio::test]
async fn a_403_from_the_sec_is_reported_as_unauthorized() {
    let (base, _hits) = counting_server(vec![]).await;
    let state = state_with(Fetcher::allow_loopback_for_tests());
    let state = {
        let mut state = state;
        std::sync::Arc::get_mut(&mut state)
            .unwrap()
            .connector_config = configured();
        state
    };
    {
        let db = state.db.lock().unwrap();
        use_region(&db, "us", true);
        enable(&db, &EdgarFilings, "us");
    }
    // An unknown route answers 404, which must surface as a fetch failure.
    assert!(matches!(
        connectors::run_with(&state, &EdgarFilings, &base).await,
        Err(RunError::Failed(ConnectorError::Fetch))
    ));
}
