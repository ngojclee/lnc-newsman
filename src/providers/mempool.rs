//! mempool.space — Bitcoin network data. No key.
//!
//! Terms (as read on the public API docs, to be re-read before the live acceptance
//! run): the public API is free and rate limited; exceeding the limit returns HTTP 429
//! and repeated abuse can be banned. The published rate limit is not a fixed number, so
//! this connector keeps a deliberately small budget: one request every 10 minutes,
//! at most 144 per day, and it never retries inside a run.
//!
//! Endpoints used (documented shapes, mocked in tests):
//! * `/api/v1/fees/recommended` — recommended fee rates in sat/vB
//! * `/api/mempool` — transaction count and total vsize in the mempool
//! * `/api/v1/mining/hashrate/3d` — current hashrate and difficulty
//! * `/api/v1/blocks/tip/height` — current tip height

use crate::{
    connectors::{
        Budget, CollectOutcome, Connector, ConnectorContext, ConnectorError, ConnectorFuture,
    },
    fetch::FetchOutcome,
    series::{self, NewObservation, SeriesDef},
};
use serde::Deserialize;

pub const ID: &str = "mempool-space";
pub const ATTRIBUTION: &str = "Source: mempool.space";
const HOUR: i64 = 3_600;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RecommendedFees {
    fastest_fee: f64,
    #[serde(default, alias = "halfHourFee")]
    half_hour_fee: Option<f64>,
    economy_fee: f64,
}

/// mempool.space returns `vsize` in vBytes and `count` as integers.
#[derive(Debug, Deserialize)]
struct MempoolStats {
    count: i64,
    vsize: i64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Hashrate {
    /// Hashes per second.
    current_hashrate: f64,
    /// Difficulty as a plain number.
    #[serde(default)]
    current_difficulty: Option<f64>,
}

#[derive(Debug, Default)]
pub struct Mempool;

impl Connector for Mempool {
    fn id(&self) -> &'static str {
        ID
    }
    fn provider(&self) -> &'static str {
        "mempool.space"
    }
    fn minimum_interval_seconds(&self) -> i64 {
        600
    }
    fn default_daily_quota(&self) -> i64 {
        144
    }
    fn base_url(&self) -> &'static str {
        "https://mempool.space"
    }
    fn collect<'a>(&'a self, ctx: ConnectorContext<'a>) -> ConnectorFuture<'a> {
        Box::pin(async move {
            // Series are declared here so a fresh install shows them before the first
            // value arrives. Declaration is not activation: the reader still needs the
            // region on and the series enabled.
            let defs = [
                ("BTC-FEE-FASTEST", "Phí nhanh nhất", "Fastest fee", "sat/vB"),
                (
                    "BTC-FEE-HALF-HOUR",
                    "Phí trong 30 phút",
                    "Fee within 30 minutes",
                    "sat/vB",
                ),
                ("BTC-FEE-ECONOMY", "Phí tiết kiệm", "Economy fee", "sat/vB"),
                (
                    "BTC-MEMPOOL-TX",
                    "Giao dịch trong mempool",
                    "Transactions in mempool",
                    "count",
                ),
                (
                    "BTC-MEMPOOL-VSIZE",
                    "Dung lượng mempool",
                    "Mempool size",
                    "vB",
                ),
                ("BTC-HASHRATE", "Hashrate", "Hashrate", "H/s"),
                ("BTC-DIFFICULTY", "Độ khó", "Difficulty", ""),
                (
                    "BTC-TIP-HEIGHT",
                    "Chiều cao khối đỉnh",
                    "Tip height",
                    "block",
                ),
            ];
            if let Some(state) = ctx.state {
                let db = state.db.lock().expect("database lock poisoned");
                for (code, vi, en, unit) in defs {
                    series::upsert_series(
                        &db,
                        &SeriesDef {
                            code,
                            provider: "mempool.space",
                            region: &ctx.region,
                            label_vi: vi,
                            label_en: en,
                            unit,
                            frequency: "hourly",
                            attribution: ATTRIBUTION,
                        },
                    )
                    .map_err(|_| ConnectorError::InvalidData)?;
                    db.conn
                        .execute("UPDATE series SET enabled=1 WHERE code=?1", [code])
                        .map_err(|_| ConnectorError::InvalidData)?;
                }
            }
            let now = crate::now() as i64;
            let period = now - now % HOUR;
            let mut written = 0;
            let mut points: Vec<(&str, f64)> = Vec::new();

            let fees: RecommendedFees = fetch_json(&ctx, "/api/v1/fees/recommended").await?;
            points.push(("BTC-FEE-FASTEST", fees.fastest_fee));
            if let Some(half) = fees.half_hour_fee {
                points.push(("BTC-FEE-HALF-HOUR", half));
            }
            points.push(("BTC-FEE-ECONOMY", fees.economy_fee));

            let mempool: MempoolStats = fetch_json(&ctx, "/api/mempool").await?;
            points.push(("BTC-MEMPOOL-TX", mempool.count as f64));
            points.push(("BTC-MEMPOOL-VSIZE", mempool.vsize as f64));

            let hashrate: Hashrate = fetch_json(&ctx, "/api/v1/mining/hashrate/3d").await?;
            points.push(("BTC-HASHRATE", hashrate.current_hashrate));
            if let Some(difficulty) = hashrate.current_difficulty {
                points.push(("BTC-DIFFICULTY", difficulty));
            }

            let height: i64 = fetch_json(&ctx, "/api/v1/blocks/tip/height").await?;
            points.push(("BTC-TIP-HEIGHT", height as f64));

            if let Some(state) = ctx.state {
                let db = state.db.lock().expect("database lock poisoned");
                for (code, value) in &points {
                    if !value.is_finite() || *value < 0.0 {
                        return Err(ConnectorError::InvalidData);
                    }
                    // The provider publishes one current value per call; a repeated poll
                    // in the same hour is an idempotent write, not a correction.
                    series::record(
                        &db,
                        &NewObservation {
                            code,
                            observed_at: period,
                            as_of: now,
                            value: *value,
                            quality: "final",
                            source_url: Some("https://mempool.space"),
                            source_ref: None,
                        },
                    )
                    .map_err(|_| ConnectorError::InvalidData)?;
                    written += 1;
                }
            }
            Ok(CollectOutcome {
                observations: written,
                note: Some(format!("{} series from mempool.space", points.len())),
                ..Default::default()
            })
        })
    }
}

/// One GET through the shared fetcher, charged to the run's budget before it happens.
async fn fetch_json<T: for<'de> Deserialize<'de>>(
    ctx: &ConnectorContext<'_>,
    path: &str,
) -> Result<T, ConnectorError> {
    fetch_json_budget(ctx.fetcher, ctx.budget, &ctx.base_url, path).await
}

pub(crate) async fn fetch_json_budget<T: for<'de> Deserialize<'de>>(
    fetcher: &crate::fetch::Fetcher,
    budget: &Budget,
    base_url: &str,
    path: &str,
) -> Result<T, ConnectorError> {
    budget.charge();
    let url = format!("{base_url}{path}");
    let body = match fetcher.get(&url, &Default::default()).await {
        Ok(FetchOutcome::Body { bytes, .. }) => bytes,
        Ok(FetchOutcome::NotModified) => return Err(ConnectorError::Provider),
        Err(crate::fetch::FetchError::HttpStatus(401 | 403)) => {
            return Err(ConnectorError::Unauthorized);
        }
        Err(crate::fetch::FetchError::HttpStatus(429)) => {
            // The provider asked us to slow down: stop the run, do not retry.
            return Err(ConnectorError::Provider);
        }
        Err(_) => return Err(ConnectorError::Fetch),
    };
    serde_json::from_slice(&body).map_err(|_| ConnectorError::Provider)
}

#[cfg(test)]
#[path = "mempool_tests.rs"]
mod tests;
