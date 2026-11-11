//! SEC EDGAR filings and XBRL facts (P4 foundation). Requires the owner's
//! `SEC_USER_AGENT` with a contact address, exactly like the news connector.
//!
//! Terms (read on sec.gov, 11/11/2026, confidence A): no key; **10 requests/second**
//! maximum; a declared User-Agent with a name and contact address; content may be
//! redistributed with citation to the SEC. This connector stays far below the limit:
//! two calls per run, one company at a time, and the framework adds at least a
//! 1-second gap.
//!
//! Endpoints used (documented shapes, mocked in tests):
//! * `/submissions/CIK##########.json` — company identity + recent filings
//! * `/api/xbrl/companyfacts/CIK##########.json` — structured XBRL facts
//!
//! Only metadata and normalised numbers are stored; filing bodies are never fetched.

use crate::{
    connectors::{
        CollectOutcome, Connector, ConnectorConfig, ConnectorContext, ConnectorError,
        ConnectorFuture,
    },
    fetch::FetchOutcome,
    filings::{self, NewFact, NewFiling},
    providers::sec_edgar::user_agent_ok,
};
use serde::Deserialize;
use std::collections::BTreeMap;

pub const ID: &str = "sec-edgar-filings";
pub const ATTRIBUTION: &str = "Source: SEC EDGAR (sec.gov)";
/// Stay well under the SEC's 10 requests/second; the framework also spaces runs.
pub const MAX_PER_MINUTE: i64 = 30;

/// Companies to track. The CIK is the only identifier EDGAR needs.
pub const COMPANIES: &[(&str, &str, &str)] = &[
    // (CIK, ticker, display name) — samples for the test and the acceptance script.
    ("0000320193", "AAPL", "Apple Inc."),
];

#[derive(Debug, Deserialize)]
struct Submissions {
    name: String,
    #[serde(default)]
    tickers: Vec<String>,
    #[serde(default)]
    exchanges: Vec<String>,
    filings: FilingsWrapper,
}

/// The documented shape nests the arrays under `filings.recent`.
#[derive(Debug, Deserialize)]
struct FilingsWrapper {
    recent: Recent,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Recent {
    #[serde(default)]
    form: Vec<String>,
    #[serde(default)]
    accession_number: Vec<String>,
    #[serde(default)]
    filing_date: Vec<String>,
    #[serde(default)]
    report_date: Vec<String>,
    #[serde(default)]
    primary_document: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct CompanyFacts {
    #[serde(default)]
    facts: BTreeMap<String, BTreeMap<String, FactEntry>>,
}

#[derive(Debug, Deserialize)]
struct FactEntry {
    #[serde(default)]
    units: BTreeMap<String, Vec<FactUnit>>,
}

#[derive(Debug, Deserialize)]
struct FactUnit {
    #[serde(default)]
    start: Option<String>,
    end: String,
    val: f64,
    #[serde(default)]
    fy: Option<i64>,
    #[serde(default)]
    fp: Option<String>,
    #[serde(default)]
    #[allow(dead_code)]
    form: Option<String>,
    #[serde(default)]
    #[allow(dead_code)]
    filed: Option<String>,
}

#[derive(Debug, Default)]
pub struct EdgarFilings;

impl Connector for EdgarFilings {
    fn id(&self) -> &'static str {
        ID
    }
    fn provider(&self) -> &'static str {
        "sec-edgar"
    }
    fn minimum_interval_seconds(&self) -> i64 {
        12 * 3_600
    }
    fn default_daily_quota(&self) -> i64 {
        10
    }
    fn max_requests_per_minute(&self) -> Option<i64> {
        Some(MAX_PER_MINUTE)
    }
    fn base_url(&self) -> &'static str {
        "https://data.sec.gov"
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
                .ok_or(ConnectorError::Unauthorized)?
                .to_string();
            let client = ctx.fetcher.clone().with_user_agent(user_agent);
            let mut filings_written = 0;
            let mut facts_written = 0;
            for (cik, ticker, name) in COMPANIES {
                let submissions_url = format!("{}/submissions/CIK{cik}.json", ctx.base_url);
                ctx.budget.charge();
                let submissions: Submissions = fetch_json(&client, &submissions_url).await?;
                let company_name = if submissions.name.trim().is_empty() {
                    (*name).to_string()
                } else {
                    submissions.name.clone()
                };
                let exchange = submissions.exchanges.first().cloned();
                let ticker_value = submissions
                    .tickers
                    .first()
                    .cloned()
                    .or_else(|| Some((*ticker).to_string()));
                let company_id = {
                    let Some(state) = ctx.state else {
                        continue;
                    };
                    let db = state.db.lock().expect("database lock poisoned");
                    filings::upsert_company(
                        &db,
                        "sec-edgar",
                        cik,
                        &ctx.region,
                        &company_name,
                        ticker_value.as_deref(),
                        exchange.as_deref(),
                    )
                    .map_err(|_| ConnectorError::InvalidData)?
                };
                // Recent filings arrive as four parallel arrays. Zip by index and stop at
                // the shortest one rather than trusting the lengths to agree.
                let recent = &submissions.filings.recent;
                let count = recent
                    .form
                    .len()
                    .min(recent.accession_number.len())
                    .min(recent.filing_date.len());
                let mut new_filings: Vec<(String, String, String, String, String)> = Vec::new();
                for index in 0..count {
                    let form = recent.form[index].as_str();
                    let accession = recent.accession_number[index].trim();
                    let filed_on = recent.filing_date[index].trim();
                    if !matches!(
                        filings::filing_kind(form),
                        "annual" | "quarterly" | "current" | "registration"
                    ) {
                        continue;
                    }
                    let period_end = recent
                        .report_date
                        .get(index)
                        .map(|value| value.trim().to_string())
                        .filter(|value| !value.is_empty());
                    let document = recent
                        .primary_document
                        .get(index)
                        .map(String::as_str)
                        .unwrap_or("");
                    let url = format!(
                        "https://www.sec.gov/Archives/edgar/data/{cik}/{}/{document}",
                        accession.replace('-', "")
                    );
                    new_filings.push((
                        accession.to_string(),
                        form.to_string(),
                        period_end.unwrap_or_default(),
                        filed_on.to_string(),
                        url,
                    ));
                }
                if let Some(state) = ctx.state {
                    let db = state.db.lock().expect("database lock poisoned");
                    for (accession, form, period_end, filed_on, url) in &new_filings {
                        let row = NewFiling {
                            provider: "sec-edgar",
                            accession,
                            form_type: form,
                            period_end: (!period_end.is_empty()).then_some(period_end.as_str()),
                            filed_on,
                            url,
                            title: &company_name,
                        };
                        if filings::insert_filing(&db, company_id, &row)
                            .map_err(|_| ConnectorError::InvalidData)?
                        {
                            filings_written += 1;
                        }
                    }
                }
                // XBRL facts: the second and last request for this company.
                let facts_url = format!("{}/api/xbrl/companyfacts/CIK{cik}.json", ctx.base_url);
                ctx.budget.charge();
                let facts: CompanyFacts = fetch_json(&client, &facts_url).await?;
                let now = crate::now() as i64;
                let mut prepared: Vec<PreparedFact> = Vec::new();
                for (taxonomy, concepts) in &facts.facts {
                    for (concept, entry) in concepts {
                        let Some(metric) = filings::normalise_concept(concept) else {
                            continue;
                        };
                        let qualified = format!("{taxonomy}:{concept}");
                        for (unit, values) in &entry.units {
                            // One row per period; a later filing for the same period with a
                            // different number becomes a new revision in the store.
                            let mut by_period: BTreeMap<&str, &FactUnit> = BTreeMap::new();
                            for value in values {
                                if value.val.is_finite() {
                                    by_period.insert(value.end.as_str(), value);
                                }
                            }
                            for (period_end, value) in by_period {
                                let currency = if unit.starts_with("USD") { "USD" } else { unit };
                                prepared.push(PreparedFact {
                                    metric: metric.to_string(),
                                    concept: qualified.clone(),
                                    period_start: value.start.clone(),
                                    period_end: period_end.to_string(),
                                    period_kind: if value.start.is_some() {
                                        "duration"
                                    } else {
                                        "instant"
                                    },
                                    fiscal_period: value.fp.clone().unwrap_or_default(),
                                    fiscal_year: value.fy,
                                    value: value.val,
                                    unit: unit.clone(),
                                    currency: currency.to_string(),
                                    as_of: now,
                                    // A later filing date with the same form family means the
                                    // provider itself has superseded the earlier number.
                                    quality_restated: false,
                                });
                            }
                        }
                    }
                }
                if let Some(state) = ctx.state {
                    let db = state.db.lock().expect("database lock poisoned");
                    for fact in &prepared {
                        let row = NewFact {
                            provider: "sec-edgar",
                            metric: &fact.metric,
                            concept: &fact.concept,
                            period_start: fact.period_start.as_deref(),
                            period_end: &fact.period_end,
                            period_kind: fact.period_kind,
                            fiscal_period: &fact.fiscal_period,
                            fiscal_year: fact.fiscal_year,
                            value: fact.value,
                            unit: &fact.unit,
                            currency: &fact.currency,
                            as_of: fact.as_of,
                            // EDGAR restates by filing a later document; the store turns a
                            // changed value for a known period into a revision.
                            quality: if fact.quality_restated {
                                "restated"
                            } else {
                                "final"
                            },
                            source_url: "https://www.sec.gov/edgar/sec-api-documentation",
                        };
                        // A fact the normaliser accepted but the store rejects is skipped
                        // rather than failing the run: one odd unit must not stop a company.
                        if filings::insert_fact(&db, company_id, &row).is_ok() {
                            facts_written += 1;
                        }
                    }
                }
            }
            Ok(CollectOutcome {
                documents: filings_written,
                observations: facts_written,
                note: Some(format!(
                    "{ATTRIBUTION}; {filings_written} filings, {facts_written} facts"
                )),
                ..Default::default()
            })
        })
    }
}

/// An owned fact ready to be written, so nothing borrows from a leak.
struct PreparedFact {
    metric: String,
    concept: String,
    period_start: Option<String>,
    period_end: String,
    period_kind: &'static str,
    fiscal_period: String,
    fiscal_year: Option<i64>,
    value: f64,
    unit: String,
    currency: String,
    as_of: i64,
    quality_restated: bool,
}

async fn fetch_json<T: for<'de> Deserialize<'de>>(
    client: &crate::fetch::Fetcher,
    url: &str,
) -> Result<T, ConnectorError> {
    let body = match client.get(url, &Default::default()).await {
        Ok(FetchOutcome::Body { bytes, .. }) => bytes,
        Ok(FetchOutcome::NotModified) => return Err(ConnectorError::Provider),
        Err(crate::fetch::FetchError::HttpStatus(401 | 403)) => {
            return Err(ConnectorError::Unauthorized);
        }
        Err(_) => return Err(ConnectorError::Fetch),
    };
    serde_json::from_slice(&body).map_err(|_| ConnectorError::Provider)
}

#[cfg(test)]
#[path = "edgar_filings_tests.rs"]
mod tests;
