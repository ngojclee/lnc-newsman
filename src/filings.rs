//! Company filings and structured financial facts (P4 foundation).
//!
//! Only metadata, structured numbers and the original link are stored. A filing's body
//! is never copied, and a written summary is ours, carries the "not investment advice"
//! notice and never overwrites another human's summary by accident.
//!
//! Facts keep the period they describe and the time we learned them, with a revision
//! number, so a restatement becomes a new revision instead of erasing history.

use crate::{SharedState, db, internal_error, regions};
use axum::{
    Json,
    extract::{Path, Query, State},
    response::{IntoResponse, Response},
};
use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};

/// The only disclaimer allowed on a filing summary or a fact view.
pub const DISCLAIMER: &str =
    "Không phải tư vấn đầu tư hay tư vấn tài chính./Not investment or financial advice.";

/// Metrics we normalise. Anything else from a provider is ignored rather than stored
/// under a guessed name, because a wrong metric is worse than a missing one.
pub const METRICS: &[(&str, &[&str])] = &[
    (
        "revenue",
        &[
            "Revenues",
            "RevenueFromContractWithCustomerExcludingAssessedTax",
            "SalesRevenueNet",
        ],
    ),
    ("net_income", &["NetIncomeLoss", "ProfitLoss"]),
    ("total_assets", &["Assets"]),
    ("total_liabilities", &["Liabilities"]),
    (
        "equity",
        &[
            "StockholdersEquity",
            "StockholdersEquityIncludingPortionAttributableToNoncontrollingInterest",
        ],
    ),
    ("operating_income", &["OperatingIncomeLoss"]),
    ("cash", &["CashAndCashEquivalentsAtCarryingValue"]),
    ("eps_basic", &["EarningsPerShareBasic"]),
];

/// Maps a provider concept (e.g. `us-gaap:Revenues`) to our metric key.
pub fn normalise_concept(concept: &str) -> Option<&'static str> {
    let local = concept.rsplit(':').next().unwrap_or(concept);
    METRICS
        .iter()
        .find(|(_, concepts)| concepts.contains(&local))
        .map(|(metric, _)| *metric)
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Company {
    pub id: i64,
    pub provider: String,
    pub external_id: String,
    pub region: String,
    pub name: String,
    pub ticker: Option<String>,
    pub exchange: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Filing {
    pub id: i64,
    pub company_id: i64,
    pub form_type: String,
    pub accession: String,
    pub period_end: Option<String>,
    pub filed_on: String,
    pub filing_kind: String,
    pub url: String,
    pub title: String,
    pub summary: Option<String>,
    pub disclaimer: String,
}

#[derive(Debug, Clone)]
pub struct NewFiling<'a> {
    pub provider: &'a str,
    pub accession: &'a str,
    pub form_type: &'a str,
    pub period_end: Option<&'a str>,
    pub filed_on: &'a str,
    pub url: &'a str,
    pub title: &'a str,
}

#[derive(Debug, Clone)]
pub struct NewFact<'a> {
    pub provider: &'a str,
    pub metric: &'a str,
    pub concept: &'a str,
    pub period_start: Option<&'a str>,
    pub period_end: &'a str,
    pub period_kind: &'a str,
    pub fiscal_period: &'a str,
    pub fiscal_year: Option<i64>,
    pub value: f64,
    pub unit: &'a str,
    pub currency: &'a str,
    pub as_of: i64,
    pub quality: &'a str,
    pub source_url: &'a str,
}

fn clean(value: &str, max: usize) -> bool {
    !value.trim().is_empty() && value.chars().count() <= max && !value.chars().any(char::is_control)
}

fn valid_date(value: &str) -> bool {
    chrono::NaiveDate::parse_from_str(value.trim(), "%Y-%m-%d").is_ok()
}

/// Filing forms we recognise, mapped to a coarse kind. S-1 and 424B are registration
/// documents (IPO and follow-on offerings), not reports.
pub fn filing_kind(form_type: &str) -> &'static str {
    match form_type.trim() {
        "10-K" | "10-K/A" | "20-F" | "40-F" => "annual",
        "10-Q" | "10-Q/A" => "quarterly",
        "8-K" | "6-K" => "current",
        "S-1" | "S-1/A" | "424B4" | "424B3" | "424B5" | "424A" => "registration",
        _ => "other",
    }
}

pub fn upsert_company(
    db: &db::Database,
    provider: &str,
    external_id: &str,
    region: &str,
    name: &str,
    ticker: Option<&str>,
    exchange: Option<&str>,
) -> anyhow::Result<i64> {
    if !clean(provider, 32) || !clean(external_id, 32) || !clean(name, 240) {
        anyhow::bail!("invalid_company");
    }
    let exists: bool = db.conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM regions WHERE code=?1)",
        [region],
        |r| r.get(0),
    )?;
    if !exists {
        anyhow::bail!("invalid_region");
    }
    db.conn.execute(
        "INSERT INTO companies(external_id,provider,region,name,ticker,exchange) VALUES (?1,?2,?3,?4,?5,?6)
         ON CONFLICT(provider,external_id) DO UPDATE SET name=excluded.name,ticker=excluded.ticker,
           exchange=excluded.exchange,region=excluded.region,updated_at=CURRENT_TIMESTAMP",
        params![external_id, provider, region, name, ticker, exchange],
    )?;
    Ok(db.conn.query_row(
        "SELECT id FROM companies WHERE provider=?1 AND external_id=?2",
        params![provider, external_id],
        |r| r.get(0),
    )?)
}

/// Lookup used by tests and by the filing connector's own bookkeeping.
#[cfg_attr(not(test), allow(dead_code))]
pub fn company(
    db: &db::Database,
    provider: &str,
    external_id: &str,
) -> anyhow::Result<Option<Company>> {
    Ok(db
        .conn
        .query_row(
            "SELECT id,provider,external_id,region,name,ticker,exchange FROM companies WHERE provider=?1 AND external_id=?2",
            params![provider, external_id],
            |r| {
                Ok(Company {
                    id: r.get(0)?,
                    provider: r.get(1)?,
                    external_id: r.get(2)?,
                    region: r.get(3)?,
                    name: r.get(4)?,
                    ticker: r.get(5)?,
                    exchange: r.get(6)?,
                })
            },
        )
        .optional()?)
}

/// Inserts a filing, ignoring a repeat. Returns true when it was new.
pub fn insert_filing(
    db: &db::Database,
    company_id: i64,
    filing: &NewFiling<'_>,
) -> anyhow::Result<bool> {
    if !clean(filing.accession, 64)
        || !clean(filing.form_type, 24)
        || !clean(filing.url, 2048)
        || !valid_date(filing.filed_on)
        || filing.period_end.is_some_and(|date| !valid_date(date))
    {
        anyhow::bail!("invalid_filing");
    }
    let url_ok = filing.url.starts_with("https://") || filing.url.starts_with("http://");
    if !url_ok {
        anyhow::bail!("invalid_filing_url");
    }
    let inserted = db.conn.execute(
        "INSERT OR IGNORE INTO filings(company_id,provider,form_type,accession,period_end,filed_on,filing_kind,url,title)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9)",
        params![
            company_id,
            filing.provider,
            filing.form_type,
            filing.accession,
            filing.period_end,
            filing.filed_on,
            filing_kind(filing.form_type),
            filing.url,
            filing.title
        ],
    )?;
    Ok(inserted > 0)
}

/// Writes a fact. A repeat of the same company/metric/period/value is a no-op; a
/// different value for a known period is a restatement and becomes revision N+1.
pub fn insert_fact(db: &db::Database, company_id: i64, fact: &NewFact<'_>) -> anyhow::Result<i64> {
    if normalise_concept(fact.concept).is_none()
        && !METRICS.iter().any(|(metric, _)| *metric == fact.metric)
    {
        anyhow::bail!("unknown_metric");
    }
    if !valid_date(fact.period_end) || fact.period_start.is_some_and(|date| !valid_date(date)) {
        anyhow::bail!("invalid_period");
    }
    if !fact.value.is_finite() {
        anyhow::bail!("invalid_value");
    }
    if !matches!(fact.period_kind, "duration" | "instant") {
        anyhow::bail!("invalid_period_kind");
    }
    if !matches!(fact.quality, "preliminary" | "final" | "restated") {
        anyhow::bail!("invalid_quality");
    }
    if !clean(fact.unit, 24) || !clean(fact.currency, 8) {
        anyhow::bail!("invalid_unit");
    }
    let latest: Option<(i64, f64)> = db
        .conn
        .query_row(
            "SELECT revision,value FROM financial_facts
              WHERE company_id=?1 AND metric=?2 AND period_end=?3 ORDER BY revision DESC LIMIT 1",
            params![company_id, fact.metric, fact.period_end],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;
    let revision = match latest {
        None => 1,
        Some((revision, value)) if (value - fact.value).abs() < f64::EPSILON => {
            return Ok(revision);
        }
        Some((revision, _)) => revision + 1,
    };
    db.conn.execute(
        "INSERT INTO financial_facts(company_id,provider,metric,concept,period_start,period_end,period_kind,
             fiscal_period,fiscal_year,value,unit,currency,as_of,revision,quality,source_url)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16)",
        params![
            company_id,
            fact.provider,
            fact.metric,
            fact.concept,
            fact.period_start,
            fact.period_end,
            fact.period_kind,
            fact.fiscal_period,
            fact.fiscal_year,
            fact.value,
            fact.unit,
            fact.currency,
            fact.as_of,
            revision,
            fact.quality,
            fact.source_url
        ],
    )?;
    Ok(revision)
}

/// Latest revision of every metric for one period, newest period first.
pub fn facts_for(
    db: &db::Database,
    company_id: i64,
    metric: Option<&str>,
    limit: i64,
) -> anyhow::Result<Vec<serde_json::Value>> {
    let mut stmt = db.conn.prepare(
        "WITH latest AS (
           SELECT metric,period_end,MAX(revision) AS revision
           FROM financial_facts WHERE company_id=?1 AND (?2 IS NULL OR metric=?2)
           GROUP BY metric,period_end)
         SELECT f.metric,f.concept,f.period_start,f.period_end,f.period_kind,f.fiscal_period,
                f.fiscal_year,f.value,f.unit,f.currency,f.as_of,f.revision,f.quality,f.source_url
         FROM financial_facts f JOIN latest l
           ON l.metric=f.metric AND l.period_end=f.period_end AND l.revision=f.revision
         WHERE f.company_id=?1
         ORDER BY f.period_end DESC, f.metric LIMIT ?3",
    )?;
    let rows = stmt.query_map(params![company_id, metric, limit], |r| {
        Ok(serde_json::json!({
            "metric": r.get::<_, String>(0)?,
            "concept": r.get::<_, String>(1)?,
            "period_start": r.get::<_, Option<String>>(2)?,
            "period_end": r.get::<_, String>(3)?,
            "period_kind": r.get::<_, String>(4)?,
            "fiscal_period": r.get::<_, String>(5)?,
            "fiscal_year": r.get::<_, Option<i64>>(6)?,
            "value": r.get::<_, f64>(7)?,
            "unit": r.get::<_, String>(8)?,
            "currency": r.get::<_, String>(9)?,
            "as_of": r.get::<_, i64>(10)?,
            "revision": r.get::<_, i64>(11)?,
            "quality": r.get::<_, String>(12)?,
            "source_url": r.get::<_, String>(13)?,
        }))
    })?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

// ---------- read API ----------

#[derive(Deserialize)]
pub struct CompanyQuery {
    region: Option<String>,
    limit: Option<i64>,
}

#[derive(Deserialize)]
pub struct FilingQuery {
    form: Option<String>,
    limit: Option<i64>,
}

#[derive(Deserialize)]
pub struct FactQuery {
    metric: Option<String>,
    limit: Option<i64>,
}

#[allow(clippy::result_large_err)]
fn region_filter(db: &db::Database, region: Option<&str>) -> Result<Vec<String>, Response> {
    if let Some(code) = region
        && !matches!(regions::region_enabled(db, code), Ok(true))
    {
        return Err(crate::failure(
            axum::http::StatusCode::BAD_REQUEST,
            "invalid_region",
        ));
    }
    regions::effective(db, region).map_err(internal_error)
}

/// Companies that have at least one filing in the effective region set.
pub async fn list_companies(
    State(state): State<SharedState>,
    Query(q): Query<CompanyQuery>,
) -> Response {
    let region = q
        .region
        .as_deref()
        .map(|code| code.trim().to_ascii_lowercase())
        .filter(|code| !code.is_empty());
    let limit = q.limit.unwrap_or(100).clamp(1, 500);
    let db = state.db.lock().expect("database lock poisoned");
    let effective = match region_filter(&db, region.as_deref()) {
        Ok(codes) => codes,
        Err(response) => return response,
    };
    let result = (|| -> rusqlite::Result<Vec<Company>> {
        let mut stmt = db.conn.prepare(
            "SELECT id,provider,external_id,region,name,ticker,exchange FROM companies
             WHERE region IN (SELECT value FROM json_each(?1)) ORDER BY name COLLATE NOCASE LIMIT ?2",
        )?;
        let rows = stmt.query_map(
            params![
                serde_json::to_string(&effective).unwrap_or_else(|_| "[]".into()),
                limit
            ],
            |r| {
                Ok(Company {
                    id: r.get(0)?,
                    provider: r.get(1)?,
                    external_id: r.get(2)?,
                    region: r.get(3)?,
                    name: r.get(4)?,
                    ticker: r.get(5)?,
                    exchange: r.get(6)?,
                })
            },
        )?;
        rows.collect()
    })();
    match result {
        Ok(data) => {
            Json(serde_json::json!({ "data": data, "disclaimer": DISCLAIMER })).into_response()
        }
        Err(err) => internal_error(err),
    }
}

pub async fn list_filings(
    State(state): State<SharedState>,
    Path(company_id): Path<i64>,
    Query(q): Query<FilingQuery>,
) -> Response {
    let limit = q.limit.unwrap_or(100).clamp(1, 500);
    let form = q
        .form
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let db = state.db.lock().expect("database lock poisoned");
    let region: Option<String> = db
        .conn
        .query_row(
            "SELECT region FROM companies WHERE id=?1",
            [company_id],
            |r| r.get(0),
        )
        .optional()
        .unwrap_or(None);
    let Some(region) = region else {
        return crate::failure(axum::http::StatusCode::NOT_FOUND, "not_found");
    };
    if !matches!(regions::region_enabled(&db, &region), Ok(true)) {
        return crate::failure(axum::http::StatusCode::NOT_FOUND, "not_found");
    }
    let result = (|| -> rusqlite::Result<Vec<Filing>> {
        let mut stmt = db.conn.prepare(
            "SELECT id,company_id,form_type,accession,period_end,filed_on,filing_kind,url,title,summary,disclaimer
             FROM filings WHERE company_id=?1 AND (?2 IS NULL OR form_type=?2)
             ORDER BY filed_on DESC, id DESC LIMIT ?3",
        )?;
        let rows = stmt.query_map(params![company_id, form, limit], |r| {
            Ok(Filing {
                id: r.get(0)?,
                company_id: r.get(1)?,
                form_type: r.get(2)?,
                accession: r.get(3)?,
                period_end: r.get(4)?,
                filed_on: r.get(5)?,
                filing_kind: r.get(6)?,
                url: r.get(7)?,
                title: r.get(8)?,
                summary: r.get(9)?,
                disclaimer: r.get(10)?,
            })
        })?;
        rows.collect()
    })();
    match result {
        Ok(data) => {
            Json(serde_json::json!({ "data": data, "disclaimer": DISCLAIMER })).into_response()
        }
        Err(err) => internal_error(err),
    }
}

pub async fn list_facts(
    State(state): State<SharedState>,
    Path(company_id): Path<i64>,
    Query(q): Query<FactQuery>,
) -> Response {
    let limit = q.limit.unwrap_or(200).clamp(1, 1000);
    let metric = q
        .metric
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let db = state.db.lock().expect("database lock poisoned");
    let region: Option<String> = db
        .conn
        .query_row(
            "SELECT region FROM companies WHERE id=?1",
            [company_id],
            |r| r.get(0),
        )
        .optional()
        .unwrap_or(None);
    let Some(region) = region else {
        return crate::failure(axum::http::StatusCode::NOT_FOUND, "not_found");
    };
    if !matches!(regions::region_enabled(&db, &region), Ok(true)) {
        return crate::failure(axum::http::StatusCode::NOT_FOUND, "not_found");
    }
    match facts_for(&db, company_id, metric, limit) {
        Ok(data) => Json(serde_json::json!({
            "data": data,
            "disclaimer": DISCLAIMER,
            "note": "Values are as published by the provider, in the provider's currency; no ratios are computed here.",
        }))
        .into_response(),
        Err(err) => internal_error(err),
    }
}

#[cfg(test)]
#[path = "filings_tests.rs"]
mod tests;
