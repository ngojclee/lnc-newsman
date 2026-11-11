//! FRED — Federal Reserve Economic Data. Requires a free API key the owner creates.
//!
//! Terms (to be re-read at acceptance): a key is required; FRED and the original
//! sources must be credited; the terms can change without notice. This connector
//! therefore (a) never runs without the `fred-api-key` secret, (b) stores the FRED
//! credit on every series, (c) keeps a small budget, and (d) stores values verbatim.
//!
//! Endpoint: `/fred/series/observations?series_id={id}&api_key={key}&file_type=json&sort_order=desc&limit={n}`
//! The key travels in the query string because that is the only mechanism FRED offers;
//! it is never logged or stored (only the secret store holds it).

use crate::{
    connectors::{CollectOutcome, Connector, ConnectorContext, ConnectorError, ConnectorFuture},
    fetch::FetchOutcome,
    series::{self, NewObservation, SeriesDef},
};
use serde::Deserialize;

pub const ID: &str = "fred";
pub const SECRET: &str = "fred-api-key";
pub const ATTRIBUTION: &str = "Source: Federal Reserve Bank of St. Louis (FRED)";
const HOUR: i64 = 3_600;

#[derive(Debug, Deserialize)]
struct Envelope {
    #[serde(default)]
    observations: Vec<Point>,
    #[serde(default)]
    error_message: Option<String>,
    #[serde(default)]
    seriess: Vec<Meta>,
}

#[derive(Debug, Deserialize)]
struct Point {
    date: String,
    /// FRED uses "." for a missing value.
    value: String,
}

#[derive(Debug, Deserialize)]
struct Meta {
    #[allow(dead_code)]
    title: String,
    units: String,
    frequency: String,
}

/// What we subscribe to. Kept short on purpose: each entry is one request per run.
pub const WATCHLIST: &[(&str, &str, &str)] = &[(
    "FEDFUNDS",
    "Lãi suất liên bang hiệu lực (Fed funds)",
    "Effective federal funds rate",
)];

#[derive(Debug, Default)]
pub struct Fred;

impl Connector for Fred {
    fn id(&self) -> &'static str {
        ID
    }
    fn provider(&self) -> &'static str {
        "fred"
    }
    fn secret_name(&self) -> Option<&'static str> {
        Some(SECRET)
    }
    fn minimum_interval_seconds(&self) -> i64 {
        12 * HOUR
    }
    fn default_daily_quota(&self) -> i64 {
        6
    }
    fn base_url(&self) -> &'static str {
        "https://api.stlouisfed.org"
    }
    fn home_region(&self) -> &'static str {
        "us"
    }
    fn collect<'a>(&'a self, ctx: ConnectorContext<'a>) -> ConnectorFuture<'a> {
        Box::pin(async move {
            let Some(key) = ctx.secret.as_deref() else {
                // The gate already refuses this case; a connector must never run keyless.
                return Err(ConnectorError::Unauthorized);
            };
            let mut written = 0;
            for (series_id, vi, en) in WATCHLIST {
                let path = format!(
                    "/fred/series/observations?series_id={series_id}&api_key={key}&file_type=json&sort_order=desc&limit=24"
                );
                let envelope: Envelope = fetch_json(&ctx, &path).await?;
                if let Some(message) = &envelope.error_message {
                    // Contains provider wording only; never echo the key or the URL.
                    let _ = message;
                    return Err(ConnectorError::Unauthorized);
                }
                if envelope.observations.is_empty() {
                    return Err(ConnectorError::InvalidData);
                }
                let meta = envelope.seriess.first();
                if let Some(state) = ctx.state {
                    let db = state.db.lock().expect("database lock poisoned");
                    let unit = meta.map(|m| m.units.as_str()).unwrap_or("");
                    let frequency = match meta.map(|m| m.frequency.as_str()) {
                        Some(f) if f.contains("Monthly") => "monthly",
                        Some(f) if f.contains("Quarterly") => "quarterly",
                        Some(f) if f.contains("Annual") => "yearly",
                        _ => "daily",
                    };
                    series::upsert_series(
                        &db,
                        &SeriesDef {
                            code: series_id,
                            provider: "fred",
                            region: &ctx.region,
                            label_vi: vi,
                            label_en: en,
                            unit,
                            frequency,
                            attribution: ATTRIBUTION,
                        },
                    )
                    .map_err(|_| ConnectorError::InvalidData)?;
                    db.conn
                        .execute("UPDATE series SET enabled=1 WHERE code=?1", [series_id])
                        .map_err(|_| ConnectorError::InvalidData)?;
                    let as_of = crate::now() as i64;
                    for point in &envelope.observations {
                        // "." means FRED has no value for that period; skip, never fill.
                        let Ok(value) = point.value.parse::<f64>() else {
                            continue;
                        };
                        let Some(period) = month_start(&point.date) else {
                            continue;
                        };
                        series::record(
                            &db,
                            &NewObservation {
                                code: series_id,
                                observed_at: period,
                                as_of: as_of.max(period),
                                value,
                                quality: "final",
                                source_url: Some("https://fred.stlouisfed.org/"),
                                source_ref: None,
                            },
                        )
                        .map_err(|_| ConnectorError::InvalidData)?;
                        written += 1;
                    }
                }
            }
            Ok(CollectOutcome {
                observations: written,
                note: Some(ATTRIBUTION.into()),
                ..Default::default()
            })
        })
    }
}

/// FRED dates are `YYYY-MM-DD`; the stored period is the first day of that month for
/// monthly series and the date itself otherwise.
pub fn month_start(value: &str) -> Option<i64> {
    use chrono::{Datelike, TimeZone, Utc};
    let date = chrono::NaiveDate::parse_from_str(value, "%Y-%m-%d").ok()?;
    let start = chrono::NaiveDate::from_ymd_opt(date.year(), date.month(), 1)?;
    Some(
        Utc.from_utc_datetime(&start.and_hms_opt(0, 0, 0)?)
            .timestamp(),
    )
}

/// Redacts the key if a URL ever reaches a log line. Exposed for tests.
#[allow(dead_code)]
pub fn redact(url: &str) -> String {
    match url.split_once("api_key=") {
        Some((head, rest)) => {
            let suffix = rest.find('&').map(|index| &rest[index..]).unwrap_or("");
            format!("{head}api_key=[redacted]{suffix}")
        }
        None => url.to_string(),
    }
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
        Err(crate::fetch::FetchError::HttpStatus(401 | 403 | 400)) => {
            return Err(ConnectorError::Unauthorized);
        }
        Err(_) => return Err(ConnectorError::Fetch),
    };
    serde_json::from_slice(&body).map_err(|_| ConnectorError::Provider)
}

#[cfg(test)]
#[path = "fred_tests.rs"]
mod tests;
