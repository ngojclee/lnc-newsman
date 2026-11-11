//! ECB Data Portal — SDMX 2.1 REST. No key.
//!
//! Reuse rule (secondary sources, to be re-read on the ECB site before the live run):
//! data may be reused with the credit "Source: ECB statistics" and without altering
//! the figures. This connector therefore stores the values exactly as returned (no
//! rounding, no unit conversion) and writes that exact attribution on the series.
//!
//! Endpoint: `/service/data/{flow}/{key}?format=jsondata&lastNObservations={n}`
//! Response shape mocked in tests follows the documented SDMX-JSON 1.0 structure.

use crate::{
    connectors::{CollectOutcome, Connector, ConnectorContext, ConnectorError, ConnectorFuture},
    providers::mempool::fetch_json_budget,
    series::{self, NewObservation, SeriesDef},
};
use serde::Deserialize;

pub const ID: &str = "ecb-sdmx";
/// Exact credit required when ECB data is reused.
pub const ATTRIBUTION: &str = "Source: ECB statistics";

/// One subscribed flow. Times are the SDMX `TIME_PERIOD` values (monthly `2026-09`,
/// daily `2026-11-10`), converted to Unix seconds at the start of that period.
#[derive(Debug, Clone, Copy)]
pub struct Flow {
    pub code: &'static str,
    pub label_vi: &'static str,
    pub label_en: &'static str,
    pub unit: &'static str,
    /// SDMX data flow, e.g. `EXR`.
    pub flow: &'static str,
    /// SDMX key, e.g. `M.USD.EUR.SP00.A`.
    pub key: &'static str,
    pub frequency: &'static str,
    pub observations: i64,
}

pub const FLOWS: &[Flow] = &[Flow {
    code: "ECB-EUR-USD",
    label_vi: "Tỷ giá tham chiếu EUR/USD",
    label_en: "EUR/USD reference rate",
    unit: "USD per EUR",
    flow: "EXR",
    key: "D.USD.EUR.SP00.A",
    frequency: "daily",
    observations: 30,
}];

#[derive(Debug, Deserialize)]
pub(crate) struct Sdmx {
    data: SdmxData,
}

#[derive(Debug, Deserialize)]
pub(crate) struct SdmxData {
    structure: SdmxStructure,
    data_sets: Vec<SdmxDataSet>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct SdmxStructure {
    dimensions: SdmxDimensions,
}

#[derive(Debug, Deserialize)]
pub(crate) struct SdmxDimensions {
    observation: Vec<SdmxDimension>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct SdmxDimension {
    values: Vec<SdmxDimensionValue>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct SdmxDimensionValue {
    id: String,
}

#[derive(Debug, Deserialize)]
pub(crate) struct SdmxDataSet {
    series: std::collections::BTreeMap<String, SdmxSeries>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct SdmxSeries {
    #[serde(default)]
    observations: std::collections::BTreeMap<String, Vec<serde_json::Value>>,
}

#[derive(Debug, Default)]
pub struct Ecb;

impl Connector for Ecb {
    fn id(&self) -> &'static str {
        ID
    }
    fn provider(&self) -> &'static str {
        "ecb"
    }
    fn minimum_interval_seconds(&self) -> i64 {
        6 * 3_600
    }
    fn default_daily_quota(&self) -> i64 {
        12
    }
    fn base_url(&self) -> &'static str {
        "https://data-api.ecb.europa.eu"
    }
    fn home_region(&self) -> &'static str {
        "eu"
    }
    fn collect<'a>(&'a self, ctx: ConnectorContext<'a>) -> ConnectorFuture<'a> {
        Box::pin(async move {
            let mut written = 0;
            for flow in FLOWS {
                let path = format!(
                    "/service/data/{}/{}?format=jsondata&lastNObservations={}",
                    flow.flow, flow.key, flow.observations
                );
                let doc: Sdmx = fetch_json_budget(ctx.fetcher, ctx.budget, &ctx.base_url, &path)
                    .await
                    .map_err(|err| {
                        // An empty flow is a data problem, not a transport one.
                        if err == ConnectorError::Provider {
                            ConnectorError::InvalidData
                        } else {
                            err
                        }
                    })?;
                let parsed = parse_doc(&doc, flow).map_err(|_| ConnectorError::InvalidData)?;
                if let Some(state) = ctx.state {
                    let db = state.db.lock().expect("database lock poisoned");
                    series::upsert_series(
                        &db,
                        &SeriesDef {
                            code: flow.code,
                            provider: "ecb",
                            region: &ctx.region,
                            label_vi: flow.label_vi,
                            label_en: flow.label_en,
                            unit: flow.unit,
                            frequency: flow.frequency,
                            attribution: ATTRIBUTION,
                        },
                    )
                    .map_err(|_| ConnectorError::InvalidData)?;
                    db.conn
                        .execute("UPDATE series SET enabled=1 WHERE code=?1", [flow.code])
                        .map_err(|_| ConnectorError::InvalidData)?;
                    let as_of = crate::now() as i64;
                    for (period, value) in parsed {
                        series::record(
                            &db,
                            &NewObservation {
                                code: flow.code,
                                observed_at: period,
                                // `as_of` is when we learned it; the value is not altered.
                                as_of: as_of.max(period),
                                value,
                                quality: "final",
                                source_url: Some("https://data.ecb.europa.eu/data/datasets/EXR"),
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

/// Turns the SDMX-JSON body into `(period_start_unix, value)` pairs.
///
/// The observation dimension carries the periods as strings; the series map keys are
/// `"0:0:0:..."` indexes into them. Missing observations are skipped, never filled:
/// inventing a value would violate "do not alter the data".
pub(crate) fn parse_doc(doc: &Sdmx, flow: &Flow) -> Result<Vec<(i64, f64)>, ()> {
    let periods = doc
        .data
        .structure
        .dimensions
        .observation
        .first()
        .ok_or(())?;
    let mut out = Vec::new();
    for series in doc
        .data
        .data_sets
        .iter()
        .flat_map(|set| set.series.values())
    {
        for (position, values) in &series.observations {
            let index: usize = position.parse().map_err(|_| ())?;
            let label = periods.values.get(index).ok_or(())?;
            let Some(first) = values.first() else {
                continue;
            };
            let Some(value) = first.as_f64() else {
                continue;
            };
            let Some(period) = period_start(&label.id, flow.frequency) else {
                continue;
            };
            out.push((period, value));
        }
    }
    if out.is_empty() {
        return Err(());
    }
    Ok(out)
}

/// SDMX periods are coarse labels; the start of the period is stored so the series
/// stays sortable and comparable. Nothing is inferred beyond the label itself.
pub fn period_start(label: &str, frequency: &str) -> Option<i64> {
    use chrono::{NaiveDate, TimeZone, Utc};
    let date = match frequency {
        "daily" => NaiveDate::parse_from_str(label, "%Y-%m-%d").ok(),
        "monthly" => {
            let (year, month) = label.split_once('-')?;
            NaiveDate::from_ymd_opt(year.parse().ok()?, month.parse().ok()?, 1)
        }
        "quarterly" => {
            let (year, quarter) = label.split_once('-')?;
            let month = (quarter.trim_start_matches('Q').parse::<u32>().ok()? - 1) * 3 + 1;
            NaiveDate::from_ymd_opt(year.parse().ok()?, month, 1)
        }
        "yearly" => NaiveDate::from_ymd_opt(label.parse().ok()?, 1, 1),
        _ => None,
    }?;
    Some(
        Utc.from_utc_datetime(&date.and_hms_opt(0, 0, 0)?)
            .timestamp(),
    )
}

/// The credit line shown under any ECB value.
#[cfg_attr(not(test), allow(dead_code))]
pub fn attribution() -> &'static str {
    ATTRIBUTION
}

/// A dimension name, used only for diagnostics; never copied into stored text.
#[cfg(test)]
#[path = "ecb_tests.rs"]
mod tests;
