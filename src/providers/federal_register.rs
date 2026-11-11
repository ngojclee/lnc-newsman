//! Federal Register API — US official documents. No key.
//!
//! Documented rules taken as given (re-read before the live run):
//! * no API key required;
//! * a query returns at most 10,000 documents, so the connector pages in time windows
//!   and stops well below that ceiling;
//! * the filter parameter is `effective_date` while the response field is
//!   `effective_on` — the connector uses each name on its own side;
//! * requests should identify the client.
//!
//! Only metadata is stored: document number, title, effective date, publication date,
//! agency and the original link. The provider's `abstract` is deliberately ignored;
//! `summary` stays empty until a human writes one.

use crate::{
    connectors::{CollectOutcome, Connector, ConnectorContext, ConnectorError, ConnectorFuture},
    documents::{self, DocumentRow},
    fetch::FetchOutcome,
};
use serde::Deserialize;

pub const ID: &str = "federal-register";
pub const ATTRIBUTION: &str = "Source: Federal Register (federalregister.gov)";
/// The API caps a single query at 10,000 documents; stay far below it.
const PER_PAGE: i64 = 100;
const MAX_PAGES_PER_RUN: i64 = 2;
const LOOKBACK_DAYS: i64 = 7;

#[derive(Debug, Deserialize)]
struct Envelope {
    results: Vec<Hit>,
    #[serde(default)]
    count: i64,
}

#[derive(Debug, Deserialize)]
pub(crate) struct Hit {
    /// Document number, e.g. `2026-12345`.
    document_number: String,
    title: String,
    #[serde(default)]
    r#type: Option<String>,
    #[serde(default)]
    agencies: Vec<Agency>,
    #[serde(default)]
    publication_date: Option<String>,
    /// NOTE: the filter is `effective_date`, the returned field is `effective_on`.
    #[serde(default)]
    effective_on: Option<String>,
    #[serde(default)]
    html_url: Option<String>,
    #[serde(default)]
    raw_text_url: Option<String>,
    /// The publisher abstract arrives in the response. It is deserialized only so a
    /// test can prove it never reaches storage; nothing reads it in production.
    #[serde(default, rename = "abstract")]
    #[allow(dead_code)]
    publisher_abstract: Option<String>,
}

#[derive(Debug, Deserialize)]
struct Agency {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    raw_name: Option<String>,
}

#[derive(Debug, Default)]
pub struct FederalRegister;

impl Connector for FederalRegister {
    fn id(&self) -> &'static str {
        ID
    }
    fn provider(&self) -> &'static str {
        "federal-register"
    }
    fn minimum_interval_seconds(&self) -> i64 {
        6 * 3_600
    }
    fn default_daily_quota(&self) -> i64 {
        6
    }
    fn base_url(&self) -> &'static str {
        "https://www.federalregister.gov"
    }
    fn home_region(&self) -> &'static str {
        "us"
    }
    fn collect<'a>(&'a self, ctx: ConnectorContext<'a>) -> ConnectorFuture<'a> {
        Box::pin(async move {
            // Start of the window, as `YYYY-MM-DD`; never a range wider than the cap.
            let today = chrono::DateTime::from_timestamp(crate::now() as i64, 0)
                .unwrap_or_default()
                .date_naive();
            let since = today - chrono::Duration::days(LOOKBACK_DAYS);
            let mut stored = 0;
            for page in 1..=MAX_PAGES_PER_RUN {
                let path = format!(
                    "/api/v1/documents.json?conditions[effective_date][gte]={}&per_page={}&page={}&order=newest",
                    since.format("%Y-%m-%d"),
                    PER_PAGE,
                    page
                );
                let envelope: Envelope = fetch_json(&ctx, &path).await?;
                if envelope.count > 10_000 {
                    // Should be impossible for a 7-day window, but never trust a count.
                    return Err(ConnectorError::InvalidData);
                }
                let empty = envelope.results.is_empty();
                if let Some(state) = ctx.state {
                    let db = state.db.lock().expect("database lock poisoned");
                    for hit in envelope.results {
                        let row = to_document(&hit, &ctx.region)
                            .map_err(|_| ConnectorError::InvalidData)?;
                        if documents::upsert(&db, &row).map_err(|_| ConnectorError::InvalidData)? {
                            stored += 1;
                        }
                    }
                }
                if empty {
                    break;
                }
            }
            Ok(CollectOutcome {
                documents: stored,
                note: Some(ATTRIBUTION.into()),
                ..Default::default()
            })
        })
    }
}

/// Map one hit to our metadata record. `effective_on` is copied verbatim when present;
/// nothing is derived from the publisher's abstract.
pub(crate) fn to_document(hit: &Hit, region: &str) -> Result<DocumentRow, ()> {
    let number = hit.document_number.trim();
    let title = hit.title.trim();
    if number.is_empty() || title.is_empty() {
        return Err(());
    }
    let url = hit
        .html_url
        .as_deref()
        .or(hit.raw_text_url.as_deref())
        .map(str::trim)
        .filter(|value| value.starts_with("http"))
        .ok_or(())?;
    let agency = hit
        .agencies
        .iter()
        .filter_map(|agency| agency.name.as_deref().or(agency.raw_name.as_deref()))
        .map(str::trim)
        .find(|value| !value.is_empty())
        .unwrap_or("")
        .to_string();
    let doc_type = match hit.r#type.as_deref() {
        Some("Rule") => "rule",
        Some("Proposed Rule") => "proposed_rule",
        Some("Notice") => "notice",
        Some("Presidential Document") => "notice",
        _ => "notice",
    };
    Ok(DocumentRow {
        provider: "federal-register".into(),
        region: region.into(),
        doc_type: doc_type.into(),
        agency,
        number: number.into(),
        title: title.chars().take(400).collect(),
        effective_on: hit
            .effective_on
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string),
        published_on: hit
            .publication_date
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string),
        url: url.to_string(),
        summary: None,
        // Left empty until a human writes the summary; classification is manual in P3.
        topic: String::new(),
        disclaimer: String::new(),
    })
}

async fn fetch_json<T: for<'de> Deserialize<'de>>(
    ctx: &ConnectorContext<'_>,
    path: &str,
) -> Result<T, ConnectorError> {
    ctx.budget.charge();
    let url = format!("{}{}", ctx.base_url, path);
    let body = match ctx.fetcher.get(&url, &Default::default()).await {
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
mod tests {
    use super::*;

    #[test]
    fn filter_and_response_field_names_are_kept_apart() {
        // The request uses `effective_date`; the response field is `effective_on`.
        let request = format!(
            "/api/v1/documents.json?conditions[effective_date][gte]={}&per_page=100",
            "2026-11-01"
        );
        assert!(request.contains("[effective_date]"));
        assert!(!request.contains("effective_on="));
        let hit: Hit = serde_json::from_value(serde_json::json!({
            "document_number": "2026-12345",
            "title": "Example rule",
            "type": "Rule",
            "agencies": [{"name": "Environmental Protection Agency"}],
            "publication_date": "2026-12-01",
            "effective_on": "2027-01-15",
            "html_url": "https://www.federalregister.gov/d/2026-12345",
            "abstract": "Publisher wording that must not be stored."
        }))
        .unwrap();
        let row = to_document(&hit, "us").unwrap();
        assert_eq!(row.effective_on.as_deref(), Some("2027-01-15"));
        assert_eq!(row.doc_type, "rule");
        assert_eq!(row.agency, "Environmental Protection Agency");
        assert_eq!(row.summary, None, "the abstract is never copied");
        assert!(!format!("{row:?}").contains("Publisher wording"));
    }

    #[test]
    fn hits_without_number_title_or_link_are_refused() {
        let base = serde_json::json!({
            "document_number": "2026-1",
            "title": "T",
            "html_url": "https://www.federalregister.gov/d/2026-1"
        });
        let hit: Hit = serde_json::from_value(base.clone()).unwrap();
        assert!(to_document(&hit, "us").is_ok());
        for (field, value) in [
            ("document_number", serde_json::json!("")),
            ("title", serde_json::json!("  ")),
            ("html_url", serde_json::json!("javascript:alert(1)")),
        ] {
            let mut broken = base.clone();
            broken[field] = value;
            let hit: Hit = serde_json::from_value(broken).unwrap();
            assert!(to_document(&hit, "us").is_err(), "{field}");
        }
    }
}

#[cfg(test)]
#[path = "federal_register_tests.rs"]
mod http_tests;
