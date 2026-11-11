//! Time series, observations and events (P2).
//!
//! An observation records the period it describes (`observed_at`), when we learned it
//! (`as_of`), and a revision number. Corrections are new revisions, never in-place
//! updates, so "what did we know at time T" stays answerable. Every series carries the
//! attribution its provider's terms require, and it is returned with every value.

use crate::{SharedState, db, internal_error, regions};
use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};

const MAX_CODE: usize = 80;
const MAX_LABEL: usize = 160;

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct SeriesRow {
    pub code: String,
    pub provider: String,
    pub region: String,
    pub label_vi: String,
    pub label_en: String,
    pub unit: String,
    pub frequency: String,
    /// Provider credit line, e.g. `Source: ECB statistics`. Required by several
    /// provider terms; shown wherever the series is displayed.
    pub attribution: String,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ObservationRow {
    pub observed_at: i64,
    pub as_of: i64,
    pub revision: i64,
    pub value: f64,
    pub quality: String,
    pub source_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct EventRow {
    pub kind: String,
    pub title: String,
    pub url: Option<String>,
    pub starts_at: i64,
    pub series_code: Option<String>,
    pub attribution: String,
}

fn clean(value: &str, max: usize) -> bool {
    let value = value.trim();
    !value.is_empty() && value.chars().count() <= max && !value.chars().any(char::is_control)
}

pub fn valid_frequency(value: &str) -> bool {
    matches!(
        value,
        "hourly" | "daily" | "weekly" | "monthly" | "quarterly" | "yearly"
    )
}

/// A period is a Unix second timestamp; anything before 1970 or after 2200 is a
/// provider bug rather than data, and is refused instead of polluting the series.
pub fn valid_timestamp(value: i64) -> bool {
    (0..=7_258_118_400).contains(&value)
}

/// Resolve the region for storage: it must exist; a disabled region is still valid for
/// writing (a connector collects for its own region and the reader hides it anyway).
fn region_exists(db: &db::Database, code: &str) -> rusqlite::Result<bool> {
    let exists: bool = db.conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM regions WHERE code=?1)",
        [code],
        |r| r.get(0),
    )?;
    Ok(exists)
}

/// Series definition. A struct rather than a long parameter list, because these fields
/// are easy to mix up and they travel together.
pub struct SeriesDef<'a> {
    pub code: &'a str,
    pub provider: &'a str,
    pub region: &'a str,
    pub label_vi: &'a str,
    pub label_en: &'a str,
    pub unit: &'a str,
    pub frequency: &'a str,
    pub attribution: &'a str,
}

pub fn upsert_series(db: &db::Database, def: &SeriesDef<'_>) -> anyhow::Result<SeriesRow> {
    let SeriesDef {
        code,
        provider,
        region,
        label_vi,
        label_en,
        unit,
        frequency,
        attribution,
    } = *def;
    let (code, provider, region) = (code.trim(), provider.trim(), region.trim());
    if !clean(code, MAX_CODE)
        || !clean(provider, 64)
        || !clean(region, 64)
        || !clean(label_vi, MAX_LABEL)
        || !clean(label_en, MAX_LABEL)
        || unit.chars().count() > 32
        || !valid_frequency(frequency)
        || attribution.chars().count() > 200
        || attribution.chars().any(char::is_control)
    {
        anyhow::bail!("invalid_series");
    }
    if !region_exists(db, region)? {
        anyhow::bail!("invalid_region");
    }
    db.conn.execute(
        "INSERT INTO series(code,provider,region,label_vi,label_en,unit,frequency,attribution)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8)
         ON CONFLICT(code) DO UPDATE SET provider=excluded.provider,region=excluded.region,
           label_vi=excluded.label_vi,label_en=excluded.label_en,unit=excluded.unit,
           frequency=excluded.frequency,attribution=excluded.attribution,updated_at=CURRENT_TIMESTAMP",
        params![code, provider, region, label_vi, label_en, unit, frequency, attribution],
    )?;
    series(db, code)?.ok_or_else(|| anyhow::anyhow!("invalid_series"))
}

pub fn series(db: &db::Database, code: &str) -> anyhow::Result<Option<SeriesRow>> {
    Ok(db
        .conn
        .query_row(
            "SELECT code,provider,region,label_vi,label_en,unit,frequency,attribution FROM series WHERE code=?1",
            [code],
            |r| {
                Ok(SeriesRow {
                    code: r.get(0)?,
                    provider: r.get(1)?,
                    region: r.get(2)?,
                    label_vi: r.get(3)?,
                    label_en: r.get(4)?,
                    unit: r.get(5)?,
                    frequency: r.get(6)?,
                    attribution: r.get(7)?,
                })
            },
        )
        .optional()?)
}

/// Writes one observation as a new revision of `(series, observed_at)`.
///
/// Returns the number of rows inserted: 0 when the same value at the same revision
/// already exists (a repeated poll), 1 otherwise. A correction for an already-known
/// period must use [`revise`], which bumps the revision and keeps history.
/// One observation to write. A struct rather than a long parameter list: the fields
/// are easy to mix up, and they travel together everywhere.
pub struct NewObservation<'a> {
    pub code: &'a str,
    pub observed_at: i64,
    pub as_of: i64,
    pub value: f64,
    pub quality: &'a str,
    pub source_url: Option<&'a str>,
    pub source_ref: Option<&'a str>,
}

impl<'a> NewObservation<'a> {
    /// A complete final value, the common case for a provider response.
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn final_value(code: &'a str, observed_at: i64, as_of: i64, value: f64) -> Self {
        Self {
            code,
            observed_at,
            as_of,
            value,
            quality: "final",
            source_url: None,
            source_ref: None,
        }
    }
}

pub fn record(db: &db::Database, input: &NewObservation<'_>) -> anyhow::Result<i64> {
    let NewObservation {
        code,
        observed_at,
        as_of,
        value,
        quality,
        source_url,
        source_ref,
    } = *input;
    if !valid_timestamp(observed_at) || !valid_timestamp(as_of) || as_of < observed_at {
        anyhow::bail!("invalid_time");
    }
    if !value.is_finite() {
        anyhow::bail!("invalid_value");
    }
    if !matches!(quality, "preliminary" | "final" | "revised") {
        anyhow::bail!("invalid_quality");
    }
    let series_id: i64 = db
        .conn
        .query_row("SELECT id FROM series WHERE code=?1", [code], |r| r.get(0))
        .optional()?
        .ok_or_else(|| anyhow::anyhow!("unknown_series"))?;
    let inserted = db.conn.execute(
        "INSERT OR IGNORE INTO observations(series_id,observed_at,as_of,revision,value,quality,source_url,source_ref)
         VALUES (?1,?2,?3,1,?4,?5,?6,?7)",
        params![series_id, observed_at, as_of, value, quality, source_url, source_ref],
    )?;
    Ok(inserted as i64)
}

/// Adds a correction: the revision fingerprint changes whenever a provider publishes a
/// new value for an existing period. The previous revision stays queryable.
#[cfg_attr(not(test), allow(dead_code))]
pub fn revise(
    db: &db::Database,
    code: &str,
    observed_at: i64,
    as_of: i64,
    value: f64,
    source_url: Option<&str>,
) -> anyhow::Result<i64> {
    if !valid_timestamp(observed_at) || !valid_timestamp(as_of) || as_of < observed_at {
        anyhow::bail!("invalid_time");
    }
    if !value.is_finite() {
        anyhow::bail!("invalid_value");
    }
    let series_id: i64 = db
        .conn
        .query_row("SELECT id FROM series WHERE code=?1", [code], |r| r.get(0))
        .optional()?
        .ok_or_else(|| anyhow::anyhow!("unknown_series"))?;
    let latest: Option<(i64, f64)> = db
        .conn
        .query_row(
            "SELECT revision,value FROM observations WHERE series_id=?1 AND observed_at=?2
             ORDER BY revision DESC LIMIT 1",
            params![series_id, observed_at],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;
    let Some((revision, previous)) = latest else {
        anyhow::bail!("unknown_period");
    };
    if (previous - value).abs() < f64::EPSILON {
        return Ok(0);
    }
    db.conn.execute(
        "INSERT INTO observations(series_id,observed_at,as_of,revision,value,quality,source_url)
         VALUES (?1,?2,?3,?4,?5,'revised',?6)",
        params![
            series_id,
            observed_at,
            as_of,
            revision + 1,
            value,
            source_url
        ],
    )?;
    Ok(revision + 1)
}

/// Newest observations of one series, one row per period: the revision known as of
/// `as_of` (or the latest when `as_of` is `None`).
pub fn observations(
    db: &db::Database,
    code: &str,
    as_of: Option<i64>,
    limit: i64,
) -> anyhow::Result<Vec<ObservationRow>> {
    let mut stmt = db.conn.prepare(
        "WITH picked AS (
           SELECT o.observed_at, MAX(o.as_of) AS as_of
           FROM observations o JOIN series s ON s.id=o.series_id
           WHERE s.code=?1 AND (?2 IS NULL OR o.as_of <= ?2)
           GROUP BY o.observed_at
           ORDER BY o.observed_at DESC LIMIT ?3)
         SELECT o.observed_at, o.as_of, o.revision, o.value, o.quality, o.source_url
         FROM observations o JOIN series s ON s.id=o.series_id
         JOIN picked p ON p.observed_at=o.observed_at AND p.as_of=o.as_of
         WHERE s.code=?1
         ORDER BY o.observed_at DESC",
    )?;
    let rows = stmt.query_map(params![code, as_of, limit], |r| {
        Ok(ObservationRow {
            observed_at: r.get(0)?,
            as_of: r.get(1)?,
            revision: r.get(2)?,
            value: r.get(3)?,
            quality: r.get(4)?,
            source_url: r.get(5)?,
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

#[cfg_attr(not(test), allow(dead_code))]
pub fn events(db: &db::Database, from: i64, to: i64, limit: i64) -> anyhow::Result<Vec<EventRow>> {
    let mut stmt = db.conn.prepare(
        "SELECT kind,title,url,starts_at,series_code,attribution FROM series_events
         WHERE starts_at >= ?1 AND starts_at <= ?2 ORDER BY starts_at LIMIT ?3",
    )?;
    let rows = stmt.query_map(params![from, to, limit], |r| {
        Ok(EventRow {
            kind: r.get(0)?,
            title: r.get(1)?,
            url: r.get(2)?,
            starts_at: r.get(3)?,
            series_code: r.get(4)?,
            attribution: r.get(5)?,
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

#[cfg_attr(not(test), allow(dead_code))]
pub struct NewEvent<'a> {
    pub provider: &'a str,
    pub region: &'a str,
    pub series_code: Option<&'a str>,
    pub kind: &'a str,
    pub title: &'a str,
    pub url: Option<&'a str>,
    pub starts_at: i64,
    pub attribution: &'a str,
}

#[cfg_attr(not(test), allow(dead_code))]
pub fn record_event(db: &db::Database, event: &NewEvent<'_>) -> anyhow::Result<i64> {
    let NewEvent {
        provider,
        region,
        series_code,
        kind,
        title,
        url,
        starts_at,
        attribution,
    } = *event;
    if !clean(provider, 64) || !clean(title, MAX_LABEL) || !valid_timestamp(starts_at) {
        anyhow::bail!("invalid_event");
    }
    if !region_exists(db, region)? {
        anyhow::bail!("invalid_region");
    }
    if !matches!(kind, "release" | "holiday" | "filing" | "other") {
        anyhow::bail!("invalid_event_kind");
    }
    let inserted = db.conn.execute(
        "INSERT OR IGNORE INTO series_events(provider,region,series_code,kind,title,url,starts_at,attribution)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8)",
        params![provider, region, series_code, kind, title, url, starts_at, attribution],
    )?;
    Ok(inserted as i64)
}

// ---------- read API ----------

#[derive(Deserialize)]
pub struct SeriesQuery {
    region: Option<String>,
    limit: Option<i64>,
}

#[derive(Deserialize)]
pub struct ObservationQuery {
    as_of: Option<i64>,
    limit: Option<i64>,
}

#[derive(Deserialize)]
pub struct EventQuery {
    from: Option<i64>,
    to: Option<i64>,
    limit: Option<i64>,
}

fn bad(code: &'static str) -> Response {
    crate::failure(StatusCode::BAD_REQUEST, code)
}

/// Series visible to the reader: those whose region is in the effective set.
pub async fn list_series(
    State(state): State<SharedState>,
    Query(q): Query<SeriesQuery>,
) -> Response {
    let region = q
        .region
        .as_deref()
        .map(|code| code.trim().to_ascii_lowercase())
        .filter(|code| !code.is_empty());
    let limit = q.limit.unwrap_or(100).clamp(1, 500);
    let db = state.db.lock().expect("database lock poisoned");
    if let Some(code) = region.as_deref()
        && !matches!(regions::region_enabled(&db, code), Ok(true))
    {
        return bad("invalid_region");
    }
    let effective = match regions::effective(&db, region.as_deref()) {
        Ok(codes) => codes,
        Err(err) => return internal_error(err),
    };
    let result = (|| -> rusqlite::Result<Vec<SeriesRow>> {
        let mut stmt = db.conn.prepare(
            "SELECT code,provider,region,label_vi,label_en,unit,frequency,attribution FROM series
             WHERE enabled=1 AND region IN (SELECT value FROM json_each(?1))
             ORDER BY region,code LIMIT ?2",
        )?;
        let rows = stmt.query_map(
            params![
                serde_json::to_string(&effective).unwrap_or_else(|_| "[]".into()),
                limit
            ],
            |r| {
                Ok(SeriesRow {
                    code: r.get(0)?,
                    provider: r.get(1)?,
                    region: r.get(2)?,
                    label_vi: r.get(3)?,
                    label_en: r.get(4)?,
                    unit: r.get(5)?,
                    frequency: r.get(6)?,
                    attribution: r.get(7)?,
                })
            },
        )?;
        rows.collect()
    })();
    match result {
        Ok(data) => {
            Json(serde_json::json!({ "data": data, "effective": effective })).into_response()
        }
        Err(err) => internal_error(err),
    }
}

pub async fn list_observations(
    State(state): State<SharedState>,
    Path(code): Path<String>,
    Query(q): Query<ObservationQuery>,
) -> Response {
    if q.as_of.is_some_and(|value| !valid_timestamp(value)) {
        return bad("invalid_time");
    }
    let limit = q.limit.unwrap_or(200).clamp(1, 2000);
    let db = state.db.lock().expect("database lock poisoned");
    let stored = match series(&db, &code) {
        Ok(Some(row)) => row,
        Ok(None) => return crate::failure(StatusCode::NOT_FOUND, "not_found"),
        Err(err) => return internal_error(err),
    };
    if !matches!(regions::region_enabled(&db, &stored.region), Ok(true)) {
        return crate::failure(StatusCode::NOT_FOUND, "not_found");
    }
    match observations(&db, &code, q.as_of, limit) {
        Ok(data) => Json(serde_json::json!({ "data": data, "series": stored })).into_response(),
        Err(err) => internal_error(err),
    }
}

pub async fn list_events(
    State(state): State<SharedState>,
    Query(q): Query<EventQuery>,
) -> Response {
    let limit = q.limit.unwrap_or(200).clamp(1, 1000);
    let db = state.db.lock().expect("database lock poisoned");
    let effective = match regions::effective(&db, None) {
        Ok(codes) => codes,
        Err(err) => return internal_error(err),
    };
    let result = (|| -> anyhow::Result<Vec<EventRow>> {
        let mut stmt = db.conn.prepare(
            "SELECT kind,title,url,starts_at,series_code,attribution FROM series_events
             WHERE starts_at >= ?1 AND starts_at <= ?2 AND region IN (SELECT value FROM json_each(?3))
             ORDER BY starts_at LIMIT ?4",
        )?;
        let rows = stmt.query_map(
            params![
                q.from.unwrap_or(0),
                q.to.unwrap_or(i64::MAX / 2),
                serde_json::to_string(&effective).unwrap_or_else(|_| "[]".into()),
                limit
            ],
            |r| {
                Ok(EventRow {
                    kind: r.get(0)?,
                    title: r.get(1)?,
                    url: r.get(2)?,
                    starts_at: r.get(3)?,
                    series_code: r.get(4)?,
                    attribution: r.get(5)?,
                })
            },
        )?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    })();
    match result {
        Ok(data) => {
            Json(serde_json::json!({ "data": data, "effective": effective })).into_response()
        }
        Err(err) => internal_error(err),
    }
}

#[cfg(test)]
#[path = "series_tests.rs"]
mod tests;
