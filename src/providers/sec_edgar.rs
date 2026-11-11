//! SEC EDGAR — US filings. No key, but the SEC requires a **declared User-Agent with a
//! contact address**; requests without one are blocked.
//!
//! The connector never invents that address. `SEC_USER_AGENT` is owner configuration,
//! and when it is empty the connector refuses to run (`connector_not_configured`).
//!
//! Rate policy: the SEC allows up to 10 requests/second. This connector uses at most
//! **2 requests/second** (a 1-second gap between calls, enforced by the framework's
//! per-minute spacing) and one request per run, so a misconfiguration cannot burst.
//!
//! Endpoint: `/cgi-bin/browse-edgar?action=getcurrent&type=8-K&dateb=&owner=include&count=10&output=atom`

use crate::{
    connectors::{
        CollectOutcome, Connector, ConnectorConfig, ConnectorContext, ConnectorError,
        ConnectorFuture,
    },
    fetch::FetchOutcome,
};
use quick_xml::{Reader, events::Event};

pub const ID: &str = "sec-edgar";
pub const ATTRIBUTION: &str = "Source: SEC EDGAR (sec.gov)";
pub const MAX_PER_MINUTE: i64 = 60;

/// A User-Agent is usable only when it names something and contains a contact address.
/// The check is deliberately strict: the SEC blocks anonymous clients.
pub fn user_agent_ok(value: &str) -> bool {
    let value = value.trim();
    value.len() >= 8
        && value.contains('@')
        && value
            .split('@')
            .nth(1)
            .is_some_and(|rest| rest.contains('.'))
}

#[derive(Debug, Default)]
pub struct SecEdgar;

impl Connector for SecEdgar {
    fn id(&self) -> &'static str {
        ID
    }
    fn provider(&self) -> &'static str {
        "sec-edgar"
    }
    fn minimum_interval_seconds(&self) -> i64 {
        60
    }
    fn default_daily_quota(&self) -> i64 {
        60
    }
    fn max_requests_per_minute(&self) -> Option<i64> {
        Some(MAX_PER_MINUTE)
    }
    fn base_url(&self) -> &'static str {
        "https://www.sec.gov"
    }
    fn home_region(&self) -> &'static str {
        "us"
    }
    fn config_ready(&self, config: &ConnectorConfig) -> bool {
        config.sec_user_agent.as_deref().is_some_and(user_agent_ok)
    }
    fn collect<'a>(&'a self, ctx: ConnectorContext<'a>) -> ConnectorFuture<'a> {
        Box::pin(async move {
            let user_agent = ctx
                .config
                .sec_user_agent
                .as_deref()
                .filter(|value| user_agent_ok(value))
                .ok_or(ConnectorError::Unauthorized)?;
            ctx.budget.charge();
            let url = format!(
                "{}/cgi-bin/browse-edgar?action=getcurrent&type=8-K&owner=include&count=10&output=atom",
                ctx.base_url
            );
            let client = ctx.fetcher.clone().with_user_agent(user_agent);
            let body = client.get(&url, &Default::default()).await;
            let bytes = match body {
                Ok(FetchOutcome::Body { bytes, .. }) => bytes,
                Ok(FetchOutcome::NotModified) => return Err(ConnectorError::Provider),
                Err(crate::fetch::FetchError::HttpStatus(403 | 401)) => {
                    return Err(ConnectorError::Unauthorized);
                }
                Err(_) => return Err(ConnectorError::Fetch),
            };
            let entries = parse_atom(&bytes).map_err(|_| ConnectorError::Provider)?;
            Ok(CollectOutcome {
                documents: 0,
                note: Some(format!(
                    "{entries} filings seen ({ATTRIBUTION}); storage lands with the filings phase"
                )),
                ..Default::default()
            })
        })
    }
}

/// Counts the entries in the Atom feed. This phase only proves the transport,
/// User-Agent and parse path; storing filings is a later, separately approved step.
pub fn parse_atom(bytes: &[u8]) -> Result<usize, ()> {
    let mut reader = Reader::from_reader(bytes);
    let config = reader.config_mut();
    config.trim_text(false);
    config.check_end_names = false;
    let mut buf = Vec::new();
    let mut entries = 0;
    let mut saw_feed = false;
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(ref start)) | Ok(Event::Empty(ref start)) => {
                let name = start.local_name();
                if name.as_ref() == b"feed" {
                    saw_feed = true;
                }
                if name.as_ref() == b"entry" {
                    entries += 1;
                }
            }
            Ok(Event::Eof) => break,
            Err(_) => break,
            _ => {}
        }
        buf.clear();
    }
    if !saw_feed {
        return Err(());
    }
    Ok(entries)
}

#[cfg(test)]
#[path = "sec_edgar_tests.rs"]
mod tests;
