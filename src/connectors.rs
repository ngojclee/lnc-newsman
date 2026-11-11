//! Connector framework (P2). Each provider adapter lives in `src/providers/` and is
//! wired through this module so every one of them obeys the same rules.
//!
//! A connector is a named provider adapter with an explicit contract: it declares
//! which secret it needs, how often it may poll, how much it may spend per UTC day,
//! and it *must* receive its HTTP client from the caller so every outbound request
//! goes through the same hardened [`crate::fetch::Fetcher`] (deny-list, pinned
//! addresses, hop-by-hop redirect validation). A connector never reads the network on
//! its own.
//!
//! Gating rules enforced here, not by each connector:
//! * the connector must be enabled, and its region (plus ancestors) must be enabled;
//! * the required secret must be present and decryptable;
//! * a configuration value the provider requires (for example a contact User-Agent)
//!   must be filled in;
//! * the request budget (daily quota, and a per-minute cap where the provider has one)
//!   must allow the run; the budget is charged per *request*, not per run;
//! * the schedule (`min_interval_seconds`) and the cache window must allow a run.

use crate::{SharedState, db, fetch::Fetcher, regions, secrets};
use axum::{
    Json,
    extract::State,
    response::{IntoResponse, Response},
};
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicU32, Ordering};

/// What a connector may return. Kept deliberately small so a new provider cannot
/// invent its own storage path.
#[derive(Debug, Default, Clone, serde::Serialize)]
pub struct CollectOutcome {
    /// Observations written (new or refreshed).
    pub observations: usize,
    /// Events written.
    pub events: usize,
    /// Documents written.
    pub documents: usize,
    /// Optional diagnostics for the health card. Must never contain secret values.
    pub note: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectorError {
    /// Provider rejected our credentials or quota: do not retry immediately.
    Unauthorized,
    /// Provider misbehaved (5xx, malformed payload).
    Provider,
    /// Our fetcher refused the request (SSRF guard, size, timeout).
    Fetch,
    /// Stored data failed validation before writing; nothing was written.
    InvalidData,
}

impl ConnectorError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::Unauthorized => "connector_unauthorized",
            Self::Provider => "connector_provider_error",
            Self::Fetch => "connector_fetch_failed",
            Self::InvalidData => "connector_invalid_data",
        }
    }
}

/// Counts requests a connector actually spent. A connector charges before every
/// request; `run` then records the real number against the day's budget.
#[derive(Default)]
pub struct Budget {
    spent: AtomicU32,
}

impl Budget {
    pub fn spent(&self) -> usize {
        self.spent.load(Ordering::Relaxed) as usize
    }
    pub fn charge(&self) {
        self.spent.fetch_add(1, Ordering::Relaxed);
    }
}

/// Everything a connector is allowed to touch. `fetcher` is the hardened client; a
/// connector must not build its own HTTP client.
pub struct ConnectorContext<'a> {
    pub fetcher: &'a Fetcher,
    /// Present for connector runs; `None` when a connector is exercised in isolation.
    pub state: Option<&'a SharedState>,
    /// Only present when the connector declared `secret_name` and it decrypted.
    /// The value is a credential: never log it, copy it into a note, or store it.
    pub secret: Option<String>,
    pub region: String,
    /// Provider root; tests point this at a loopback server.
    pub base_url: String,
    /// Configuration the owner must supply (contact User-Agent and similar). Never a
    /// credential; only values that identify the installation to the provider.
    pub config: &'a ConnectorConfig,
    pub budget: &'a Budget,
}

/// Operator-supplied settings some providers require. Present but empty when unset;
/// a connector that needs one refuses to run rather than inventing a value.
#[derive(Debug, Default, Clone)]
pub struct ConnectorConfig {
    /// `SEC_USER_AGENT` — must contain a name and a contact address per SEC policy.
    /// Consumed by the EDGAR connector through its environment variable, so this type
    /// only reports whether that provider is configured.
    pub sec_user_agent: Option<String>,
}

impl ConnectorConfig {
    pub fn from_env() -> Self {
        Self {
            sec_user_agent: std::env::var("SEC_USER_AGENT")
                .ok()
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty()),
        }
    }
}

pub type ConnectorFuture<'a> =
    Pin<Box<dyn Future<Output = Result<CollectOutcome, ConnectorError>> + Send + 'a>>;

pub trait Connector: Send + Sync {
    /// Stable id stored in `connectors.id`.
    fn id(&self) -> &'static str;
    fn provider(&self) -> &'static str;
    /// Secret name this connector needs, if any. The value is fetched by the caller.
    fn secret_name(&self) -> Option<&'static str> {
        None
    }
    /// Hard floor for `min_interval_seconds`, so a bad schedule cannot hammer a provider.
    fn minimum_interval_seconds(&self) -> i64 {
        300
    }
    /// Default daily request budget when the row does not override it.
    fn default_daily_quota(&self) -> i64 {
        48
    }
    /// Optional per-minute cap, for providers that publish one (CoinGecko: 30/min).
    fn max_requests_per_minute(&self) -> Option<i64> {
        None
    }
    /// Provider root. Tests override it to point at a loopback server.
    fn base_url(&self) -> &'static str;
    /// Region a freshly registered connector starts in. It still starts disabled.
    fn home_region(&self) -> &'static str {
        "global"
    }
    /// Extra check on operator configuration, e.g. a contact User-Agent.
    fn config_ready(&self, _config: &ConnectorConfig) -> bool {
        true
    }
    fn collect<'a>(&'a self, ctx: ConnectorContext<'a>) -> ConnectorFuture<'a>;
}

/// Why a scheduled run did not touch the network. Returned for the health card so the
/// admin can tell "off" from "broken".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Skip {
    Disabled,
    RegionOff,
    SecretMissing,
    SecretUnreadable,
    NotConfigured,
    NotDue,
    QuotaExhausted,
    Cached,
}

impl Skip {
    pub fn code(&self) -> &'static str {
        match self {
            Self::Disabled => "connector_disabled",
            Self::RegionOff => "connector_region_off",
            Self::SecretMissing => "connector_secret_missing",
            Self::SecretUnreadable => "connector_secret_unreadable",
            Self::NotConfigured => "connector_not_configured",
            Self::NotDue => "connector_not_due",
            Self::QuotaExhausted => "connector_quota_exhausted",
            Self::Cached => "connector_cache_fresh",
        }
    }
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct ConnectorRow {
    pub id: String,
    pub region: String,
    pub enabled: bool,
    pub secret_name: Option<String>,
    pub min_interval_seconds: i64,
    pub daily_quota: i64,
    pub cache_seconds: i64,
    pub last_run_at: Option<i64>,
    pub next_run_at: Option<i64>,
    pub requests_today: i64,
    pub quota_day: Option<String>,
    pub health: String,
    pub last_error: Option<String>,
}

pub fn day_key(now: i64) -> String {
    chrono::DateTime::from_timestamp(now, 0)
        .unwrap_or_default()
        .format("%Y-%m-%d")
        .to_string()
}

pub fn row(db: &db::Database, id: &str) -> rusqlite::Result<Option<ConnectorRow>> {
    db.conn
        .query_row(
            "SELECT id,region,enabled,secret_name,min_interval_seconds,daily_quota,cache_seconds,
                    last_run_at,next_run_at,requests_today,quota_day,health,last_error
             FROM connectors WHERE id=?1",
            [id],
            |r| {
                Ok(ConnectorRow {
                    id: r.get(0)?,
                    region: r.get(1)?,
                    enabled: r.get::<_, i64>(2)? != 0,
                    secret_name: r.get(3)?,
                    min_interval_seconds: r.get(4)?,
                    daily_quota: r.get(5)?,
                    cache_seconds: r.get(6)?,
                    last_run_at: r.get(7)?,
                    next_run_at: r.get(8)?,
                    requests_today: r.get(9)?,
                    quota_day: r.get(10)?,
                    health: r.get(11)?,
                    last_error: r.get(12)?,
                })
            },
        )
        .optional_row()
}

trait OptionalRow<T> {
    fn optional_row(self) -> rusqlite::Result<Option<T>>;
}
impl<T> OptionalRow<T> for rusqlite::Result<T> {
    fn optional_row(self) -> rusqlite::Result<Option<T>> {
        match self {
            Ok(row) => Ok(Some(row)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(err) => Err(err),
        }
    }
}

/// Register a connector with its provider defaults. New connectors start **disabled**:
/// the same "detection is not activation" rule as region packs.
pub fn upsert(db: &db::Database, connector: &dyn Connector, region: &str) -> anyhow::Result<()> {
    let exists = db.conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM regions WHERE code=?1)",
        [region],
        |r| r.get::<_, bool>(0),
    )?;
    if !exists {
        anyhow::bail!("invalid_region");
    }
    db.conn.execute(
        "INSERT INTO connectors(id,provider,region,enabled,secret_name,min_interval_seconds,daily_quota,cache_seconds)
         VALUES (?1,?2,?3,0,?4,?5,?6,?7)
         ON CONFLICT(id) DO UPDATE SET provider=excluded.provider,region=excluded.region",
        rusqlite::params![
            connector.id(),
            connector.provider(),
            region,
            connector.secret_name(),
            connector.minimum_interval_seconds(),
            connector.default_daily_quota(),
            900
        ],
    )?;
    Ok(())
}

/// Checks every gate before a connector is allowed to run. Pure database work, so it
/// can be tested without any network.
pub fn gate(
    db: &db::Database,
    vault: Option<&secrets::SecretBox>,
    config: &ConnectorConfig,
    connector: &dyn Connector,
    row: &ConnectorRow,
    now: i64,
) -> Result<(), Skip> {
    if !row.enabled {
        return Err(Skip::Disabled);
    }
    if !matches!(regions::region_enabled(db, &row.region), Ok(true)) {
        return Err(Skip::RegionOff);
    }
    if !connector.config_ready(config) {
        return Err(Skip::NotConfigured);
    }
    if let Some(name) = row.secret_name.as_deref() {
        let Some(vault) = vault else {
            return Err(Skip::SecretMissing);
        };
        match secrets::get(db, vault, name) {
            Ok(_) => {}
            Err(secrets::SecretError::Missing | secrets::SecretError::NotConfigured) => {
                return Err(Skip::SecretMissing);
            }
            Err(_) => return Err(Skip::SecretUnreadable),
        }
    }
    // Quota is checked before the schedule: an exhausted budget is the real reason a
    // run is refused, and the health card must say so rather than "not due".
    if row.quota_day.as_deref() == Some(day_key(now).as_str())
        && row.requests_today >= row.daily_quota
    {
        return Err(Skip::QuotaExhausted);
    }
    if let Some(last) = row.last_run_at
        && row.cache_seconds > 0
        && now - last < row.cache_seconds
    {
        return Err(Skip::Cached);
    }
    if row.next_run_at.is_some_and(|due| now < due) {
        return Err(Skip::NotDue);
    }
    Ok(())
}

/// Why a run ended without data. Separates "not allowed to run" from "ran and failed".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunError {
    Skipped(Skip),
    Failed(ConnectorError),
}

impl RunError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::Skipped(skip) => skip.code(),
            Self::Failed(err) => err.code(),
        }
    }
}

/// Runs one connector. All bookkeeping — quota, schedule, cache, health — is applied
/// here so no connector can forget it. The network call runs without any DB lock held.
pub async fn run(
    state: &SharedState,
    connector: &dyn Connector,
) -> Result<CollectOutcome, RunError> {
    let base = connector.base_url().to_string();
    run_with(state, connector, &base).await
}

/// Same as [`run`] with an explicit provider root, which tests point at loopback.
pub async fn run_with(
    state: &SharedState,
    connector: &dyn Connector,
    base_url: &str,
) -> Result<CollectOutcome, RunError> {
    let now = crate::now() as i64;
    let (row, secret) = {
        let db = state.db.lock().expect("database lock poisoned");
        let Some(row) = row(&db, connector.id()).map_err(|err| {
            tracing::error!(%err, connector = connector.id(), "connector row unreadable");
            RunError::Skipped(Skip::Disabled)
        })?
        else {
            return Err(RunError::Skipped(Skip::Disabled));
        };
        gate(
            &db,
            state.secrets.as_ref(),
            &state.connector_config,
            connector,
            &row,
            now,
        )
        .map_err(RunError::Skipped)?;
        let secret = match (connector.secret_name(), state.secrets.as_ref()) {
            (Some(name), Some(vault)) => secrets::get(&db, vault, name).ok(),
            _ => None,
        };
        (row, secret)
    };
    let budget = Budget::default();
    let started = crate::now() as i64;
    let outcome = connector
        .collect(ConnectorContext {
            fetcher: &state.fetcher,
            state: Some(state),
            secret,
            region: row.region.clone(),
            base_url: base_url.trim_end_matches('/').to_string(),
            config: &state.connector_config,
            budget: &budget,
        })
        .await;
    {
        let db = state.db.lock().expect("database lock poisoned");
        let day = day_key(started);
        let spent_before = if row.quota_day.as_deref() == Some(day.as_str()) {
            row.requests_today
        } else {
            0
        };
        // A run that made no request (cached provider answer) must not eat the budget,
        // but it still counts as a run for the schedule.
        let requests_today = (spent_before + budget.spent() as i64).min(row.daily_quota);
        let interval = row
            .min_interval_seconds
            .max(connector.minimum_interval_seconds())
            .max(60);
        let mut next = started + interval * 2;
        if let Some(cap) = connector.max_requests_per_minute()
            && cap > 0
        {
            // Spread the provider's per-minute cap: never schedule sooner than the gap
            // that cap implies.
            next = next.max(started + (3600 / cap).max(60));
        }
        let (health, last_error) = match &outcome {
            Ok(_) => ("healthy", None),
            Err(err) => ("degraded", Some(err.code())),
        };
        if let Err(err) = db.conn.execute(
            "UPDATE connectors SET last_run_at=?2, next_run_at=?3, requests_today=?4, quota_day=?5,
                    requests_last_run=?6, health=?7, last_error=?8,
                    consecutive_failures=CASE WHEN ?8 IS NULL THEN 0 ELSE consecutive_failures+1 END,
                    last_success_at=CASE WHEN ?8 IS NULL THEN ?2 ELSE last_success_at END
             WHERE id=?1",
            rusqlite::params![
                connector.id(),
                started,
                next,
                requests_today,
                day,
                budget.spent() as i64,
                health,
                last_error
            ],
        ) {
            tracing::error!(%err, connector = connector.id(), "connector bookkeeping failed");
        }
    }
    outcome.map_err(RunError::Failed)
}

/// Registers every shipped connector that is not registered yet, **disabled**. The
/// owner chooses the region by enabling it; a connector is never activated by start-up.
pub fn register_missing(db: &db::Database) -> anyhow::Result<usize> {
    let mut added = 0;
    for connector in crate::providers::all() {
        let exists: bool = db.conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM connectors WHERE id=?1)",
            [connector.id()],
            |r| r.get(0),
        )?;
        if !exists {
            // Each provider's data has a natural home region; the owner can move it.
            upsert(db, connector.as_ref(), connector.home_region())?;
            added += 1;
        }
    }
    Ok(added)
}

/// Manual run, so the approval script has a button and every live request is
/// deliberate. Same gates as a scheduled run; nothing runs while the connector is off.
pub async fn run_handler(
    State(state): State<SharedState>,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> Response {
    let Some(connector) = crate::providers::all()
        .into_iter()
        .find(|candidate| candidate.id() == id)
    else {
        return crate::failure(axum::http::StatusCode::NOT_FOUND, "not_found");
    };
    match run(&state, connector.as_ref()).await {
        Ok(outcome) => Json(serde_json::json!({ "data": outcome })).into_response(),
        Err(err) => {
            let status = match err {
                RunError::Skipped(_) => axum::http::StatusCode::CONFLICT,
                RunError::Failed(_) => axum::http::StatusCode::BAD_GATEWAY,
            };
            crate::failure(status, err.code())
        }
    }
}

/// Admin endpoint: connector health, quota and schedule. Never includes secrets.
pub async fn status_handler(State(state): State<SharedState>) -> Response {
    let db = state.db.lock().expect("database lock poisoned");
    match statuses(&db) {
        Ok(data) => Json(serde_json::json!({ "data": data })).into_response(),
        Err(err) => crate::internal_error(err),
    }
}

#[derive(Debug, serde::Serialize)]
pub struct ConnectorStatus {
    pub id: String,
    pub provider: String,
    pub region: String,
    pub enabled: bool,
    pub health: String,
    pub last_error: Option<String>,
    pub last_run_at: Option<i64>,
    pub next_run_at: Option<i64>,
    pub requests_today: i64,
    pub daily_quota: i64,
    pub requests_last_run: i64,
    pub needs_secret: bool,
}

pub fn statuses(db: &db::Database) -> rusqlite::Result<Vec<ConnectorStatus>> {
    let mut stmt = db.conn.prepare(
        "SELECT id,provider,region,enabled,health,last_error,last_run_at,next_run_at,
                requests_today,daily_quota,secret_name IS NOT NULL,requests_last_run
         FROM connectors ORDER BY id",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok(ConnectorStatus {
            id: r.get(0)?,
            provider: r.get(1)?,
            region: r.get(2)?,
            enabled: r.get::<_, i64>(3)? != 0,
            health: r.get(4)?,
            last_error: r.get(5)?,
            last_run_at: r.get(6)?,
            next_run_at: r.get(7)?,
            requests_today: r.get(8)?,
            daily_quota: r.get(9)?,
            needs_secret: r.get::<_, i64>(10)? != 0,
            requests_last_run: r.get(11)?,
        })
    })?;
    rows.collect()
}

#[cfg(test)]
#[path = "connectors_tests.rs"]
mod tests;
