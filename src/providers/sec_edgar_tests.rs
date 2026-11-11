//! SEC EDGAR connector tests: the contact User-Agent is mandatory, never invented, and
//! the provider is never called when it is missing. Loopback only.

use super::*;
use crate::{
    connectors::{self, ConnectorConfig, RunError, Skip},
    fetch::Fetcher,
    providers::tests_support::{enable, state_with},
};

const FEED: &str = r#"<?xml version="1.0" encoding="ISO-8859-1"?>
<feed xmlns="http://www.w3.org/2005/Atom">
  <title>Latest Filings</title>
  <entry><title>8-K - EXAMPLE CORP (0000000001) (Filer)</title><updated>2026-11-11T10:00:00-05:00</updated></entry>
  <entry><title>8-K - SECOND CORP (0000000002) (Filer)</title><updated>2026-11-11T10:01:00-05:00</updated></entry>
</feed>"#;

#[test]
fn a_user_agent_needs_a_contact_address() {
    for good in [
        "Example Newsman admin@example.invalid",
        "LNC Newsman (ops@example.org)",
    ] {
        assert!(user_agent_ok(good), "{good}");
    }
    for bad in ["", "Example Newsman", "bot", "no-at-sign.example", "a@b"] {
        assert!(!user_agent_ok(bad), "{bad:?}");
    }
}

#[test]
fn the_connector_is_unconfigured_without_the_setting() {
    let connector = SecEdgar;
    assert!(!connector.config_ready(&ConnectorConfig::default()));
    assert!(!connector.config_ready(&ConnectorConfig {
        sec_user_agent: Some("no address".into())
    }));
    assert!(connector.config_ready(&ConnectorConfig {
        sec_user_agent: Some("Example admin@example.invalid".into())
    }));
}

#[tokio::test]
async fn without_a_configured_user_agent_nothing_is_requested() {
    let (base, hits) =
        crate::providers::tests_support::counting_server(vec![("/cgi-bin/browse-edgar", FEED)])
            .await;
    let state = state_with(Fetcher::allow_loopback_for_tests());
    {
        let db = state.db.lock().unwrap();
        db.conn
            .execute(
                "INSERT OR IGNORE INTO regions(code,parent_code,label_vi,label_en,enabled) VALUES ('us',NULL,'US','US',1)",
                [],
            )
            .unwrap();
        enable(&db, &SecEdgar, "us");
    }
    assert_eq!(
        connectors::run_with(&state, &SecEdgar, &base)
            .await
            .unwrap_err(),
        RunError::Skipped(Skip::NotConfigured)
    );
    assert_eq!(hits.load(std::sync::atomic::Ordering::Relaxed), 0);
}

#[test]
fn the_atom_feed_is_parsed_without_a_feed_root_it_is_refused() {
    assert_eq!(parse_atom(FEED.as_bytes()).unwrap(), 2);
    assert_eq!(parse_atom(b"<html><entry></entry></html>"), Err(()));
}

#[tokio::test]
async fn the_rate_limit_is_well_below_the_published_ceiling() {
    // The SEC allows 10 requests/second; this connector caps at one per second.
    assert_eq!(SecEdgar.max_requests_per_minute(), Some(60));
    assert!(SecEdgar.minimum_interval_seconds() >= 60);
    assert_eq!(SecEdgar.default_daily_quota(), 60);
}
