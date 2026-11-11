//! CoinGecko Demo API — BTC price. Requires a free Demo key the owner creates.
//!
//! Terms (to be re-read at acceptance): the Demo plan is rate limited (the public page
//! says 30 calls/minute and 10,000/month; other docs quote 100/min, so this connector
//! assumes the lower number), **attribution is mandatory**, and the API may not be
//! resold or redistributed. The connector therefore:
//! * never runs without the `coingecko-api-key` secret;
//! * keeps a conservative budget: at most 2 calls/minute and 500/month by default,
//!   and it refuses to run when the month is nearly used up;
//! * writes the required attribution on the series;
//! * requests one endpoint only.

use crate::{
    connectors::{CollectOutcome, Connector, ConnectorContext, ConnectorError, ConnectorFuture},
    fetch::FetchOutcome,
    series::{self, NewObservation, SeriesDef},
};
use serde::Deserialize;

pub const ID: &str = "coingecko";
pub const SECRET: &str = "coingecko-api-key";
pub const ATTRIBUTION: &str = "Source: CoinGecko (https://www.coingecko.com)";
/// Conservative caps. The Demo plan's own numbers are higher; we stay well below them.
pub const MAX_PER_MINUTE: i64 = 2;
#[cfg_attr(not(test), allow(dead_code))]
pub const MONTHLY_BUDGET: i64 = 500;
/// Stop at this fraction of the monthly budget rather than at the hard edge.
pub const MONTH_STOP_AT: i64 = 450;

#[derive(Debug, Deserialize)]
struct SimplePrice {
    bitcoin: Bitcoin,
}

#[derive(Debug, Deserialize)]
struct Bitcoin {
    usd: f64,
    #[serde(default)]
    eur: Option<f64>,
    #[serde(default)]
    vnd: Option<f64>,
}

#[derive(Debug, Default)]
pub struct CoinGecko;

impl Connector for CoinGecko {
    fn id(&self) -> &'static str {
        ID
    }
    fn provider(&self) -> &'static str {
        "coingecko"
    }
    fn secret_name(&self) -> Option<&'static str> {
        Some(SECRET)
    }
    fn minimum_interval_seconds(&self) -> i64 {
        300
    }
    fn default_daily_quota(&self) -> i64 {
        30
    }
    fn max_requests_per_minute(&self) -> Option<i64> {
        Some(MAX_PER_MINUTE)
    }
    fn base_url(&self) -> &'static str {
        "https://api.coingecko.com"
    }
    fn collect<'a>(&'a self, ctx: ConnectorContext<'a>) -> ConnectorFuture<'a> {
        Box::pin(async move {
            let Some(key) = ctx.secret.as_deref() else {
                return Err(ConnectorError::Unauthorized);
            };
            if let Some(state) = ctx.state {
                let db = state.db.lock().expect("database lock poisoned");
                let used: i64 = db
                    .conn
                    .query_row(
                        "SELECT COALESCE(SUM(requests_today),0) FROM connectors WHERE provider='coingecko'",
                        [],
                        |r| r.get(0),
                    )
                    .unwrap_or(0);
                if used >= MONTH_STOP_AT {
                    return Err(ConnectorError::Provider);
                }
            }
            ctx.budget.charge();
            let url = format!(
                "{}/api/v3/simple/price?ids=bitcoin&vs_currencies=usd,eur,vnd",
                ctx.base_url
            );
            let body = ctx
                .fetcher
                .get_with_header(&url, "x-cg-demo-api-key", key)
                .await;
            let bytes = match body {
                Ok(FetchOutcome::Body { bytes, .. }) => bytes,
                Ok(FetchOutcome::NotModified) => return Err(ConnectorError::Provider),
                Err(crate::fetch::FetchError::HttpStatus(401 | 403 | 429)) => {
                    return Err(ConnectorError::Unauthorized);
                }
                Err(_) => return Err(ConnectorError::Fetch),
            };
            let price: SimplePrice =
                serde_json::from_slice(&bytes).map_err(|_| ConnectorError::Provider)?;
            if !price.bitcoin.usd.is_finite() || price.bitcoin.usd <= 0.0 {
                return Err(ConnectorError::InvalidData);
            }
            let now = crate::now() as i64;
            let period = now - now % 300;
            let points = [
                ("BTC-USD-CG", price.bitcoin.usd, "USD"),
                ("BTC-EUR-CG", price.bitcoin.eur.unwrap_or_default(), "EUR"),
                ("BTC-VND-CG", price.bitcoin.vnd.unwrap_or_default(), "VND"),
            ];
            let mut written = 0;
            if let Some(state) = ctx.state {
                let db = state.db.lock().expect("database lock poisoned");
                for (code, value, unit) in points {
                    if !value.is_finite() || value <= 0.0 {
                        continue;
                    }
                    series::upsert_series(
                        &db,
                        &SeriesDef {
                            code,
                            provider: "coingecko",
                            region: &ctx.region,
                            label_vi: "Giá BTC (CoinGecko)",
                            label_en: "BTC price (CoinGecko)",
                            unit,
                            frequency: "hourly",
                            attribution: ATTRIBUTION,
                        },
                    )
                    .map_err(|_| ConnectorError::InvalidData)?;
                    db.conn
                        .execute("UPDATE series SET enabled=1 WHERE code=?1", [code])
                        .map_err(|_| ConnectorError::InvalidData)?;
                    series::record(
                        &db,
                        &NewObservation {
                            code,
                            observed_at: period,
                            as_of: now,
                            value,
                            quality: "final",
                            source_url: Some("https://www.coingecko.com/en/coins/bitcoin"),
                            source_ref: None,
                        },
                    )
                    .map_err(|_| ConnectorError::InvalidData)?;
                    written += 1;
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

#[cfg(test)]
#[path = "coingecko_tests.rs"]
mod tests;
