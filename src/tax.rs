//! Recurring tax deadlines derived from a region pack (P3 foundation).
//!
//! A rule is a *declaration*, not a legal fact: `verified = false` means the owner has
//! not checked it with the authority, and the reader shows that notice next to every
//! date. Nothing here decides what the law is; it only computes the next occurrence of
//! a declared rule in the region's own timezone.
//!
//! Weekend handling is deliberately rule-based rather than a holiday calendar: a pack
//! declares `next_business_day`, `previous_business_day` or `none`, and the engine
//! applies exactly that. Public holidays are NOT modelled, and the UI says so — a
//! wrong "holiday" answer would look more authoritative than no answer.
//!
//! No reminders are sent: P3 only exposes the calendar and an in-app "due soon" list.
//! Telegram and email belong to P7.

use crate::{SharedState, db, internal_error, regions};
use axum::{
    Json,
    extract::{Query, State},
    response::{IntoResponse, Response},
};
use chrono::{Datelike, Days, Months, NaiveDate, TimeZone, Utc, Weekday};
use rusqlite::OptionalExtension;
use serde::{Deserialize, Serialize};

pub const DISCLAIMER_VI: &str =
    "Không phải tư vấn pháp lý/thuế. Hạn nộp do pack khai báo; hãy kiểm tra lại với cơ quan thuế.";
pub const DISCLAIMER_EN: &str = "Not legal or tax advice. Deadlines come from a declared pack rule; verify them with the tax authority.";

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Deadline {
    pub code: String,
    pub region: String,
    pub name_vi: String,
    pub name_en: String,
    pub cadence: String,
    pub source_code: String,
    pub url: String,
    pub note: String,
    /// False when the pack rule has not been checked with the authority.
    pub verified: bool,
    pub lead_days: i64,
    /// The next occurrence in the region's own timezone, as `YYYY-MM-DD`.
    pub due_on: String,
    /// Whole days from "today in that timezone" to `due_on`; 0 means today.
    pub days_until: i64,
    pub due_soon: bool,
    /// Calendar date before any weekend shift was applied.
    pub nominal_on: String,
    pub weekend_rule: String,
}

#[derive(Debug, Clone)]
pub struct Rule {
    pub code: String,
    pub region: String,
    pub name_vi: String,
    pub name_en: String,
    pub cadence: String,
    pub month: Option<i64>,
    pub day: Option<i64>,
    pub weekend_rule: String,
    pub lead_days: i64,
    pub url: String,
    pub note: String,
    pub source_code: String,
    pub verified: bool,
}

/// Moves a calendar date according to the declared rule. Public holidays are not
/// modelled; `none` keeps the date, the other rules only skip Saturday/Sunday.
pub fn apply_weekend_rule(date: NaiveDate, rule: &str) -> NaiveDate {
    match rule {
        "next_business_day" => {
            let mut current = date;
            while matches!(current.weekday(), Weekday::Sat | Weekday::Sun) {
                current = current + Days::new(1);
            }
            current
        }
        "previous_business_day" => {
            let mut current = date;
            while matches!(current.weekday(), Weekday::Sat | Weekday::Sun) {
                current = current - Days::new(1);
            }
            current
        }
        // Unknown or `none`: keep the declared calendar date unchanged.
        _ => date,
    }
}

/// The nominal date of a rule in a given year, or `None` when the rule has no
/// occurrence that year (for example monthly rules with `month` set).
pub fn nominal_date(rule: &Rule, year: i32) -> Option<NaiveDate> {
    let day = rule.day?.clamp(1, 31) as u32;
    match rule.cadence.as_str() {
        "yearly" => NaiveDate::from_ymd_opt(year, rule.month? as u32, day),
        "monthly" | "quarterly" => None,
        _ => None,
    }
}

/// Next occurrence at or after `today`. Monthly and quarterly rules repeat every
/// month / every third month, so the search walks forward month by month, clamping the
/// day to the length of that month (a rule on day 31 fires on 28/29/30 February).
pub fn next_due(rule: &Rule, today: NaiveDate) -> Option<NaiveDate> {
    let day = rule.day? as u32;
    match rule.cadence.as_str() {
        "yearly" => {
            let month = rule.month? as u32;
            for year in today.year()..=today.year() + 1 {
                if let Some(date) = clamped_date(year, month, day) {
                    let shifted = apply_weekend_rule(date, &rule.weekend_rule);
                    if shifted >= today {
                        return Some(shifted);
                    }
                }
            }
            None
        }
        "monthly" | "quarterly" => {
            // Walk up to 14 months ahead; quarterly filters to months 1, 4, 7, 10.
            let mut month_cursor = NaiveDate::from_ymd_opt(today.year(), today.month(), 1)?;
            for _ in 0..14 {
                let quarter_ok = rule.cadence == "monthly" || (month_cursor.month() - 1) % 3 == 0;
                if quarter_ok
                    && let Some(date) = clamped_date(month_cursor.year(), month_cursor.month(), day)
                {
                    let shifted = apply_weekend_rule(date, &rule.weekend_rule);
                    if shifted >= today {
                        return Some(shifted);
                    }
                }
                month_cursor = month_cursor.checked_add_months(Months::new(1))?;
            }
            None
        }
        _ => None,
    }
}

fn clamped_date(year: i32, month: u32, day: u32) -> Option<NaiveDate> {
    if !(1..=12).contains(&month) {
        return None;
    }
    let first = NaiveDate::from_ymd_opt(year, month, 1)?;
    let last_day = first.checked_add_months(Months::new(1))? - Days::new(1);
    NaiveDate::from_ymd_opt(year, month, day.min(last_day.day()))
}

/// Today's date in the region's timezone. An unknown zone falls back to UTC and the
/// caller keeps the region's declared name, so a typo cannot shift every date silently.
pub fn today_in(timezone: &str, now: i64) -> NaiveDate {
    if let Ok(zone) = timezone.parse::<chrono_tz::Tz>() {
        let instant = Utc.timestamp_opt(now, 0).single().unwrap_or_default();
        return instant.with_timezone(&zone).date_naive();
    }
    Utc.timestamp_opt(now, 0)
        .single()
        .unwrap_or_default()
        .date_naive()
}

pub fn rules(db: &db::Database, region_filter: Option<&str>) -> rusqlite::Result<Vec<Rule>> {
    let effective = regions::effective(db, region_filter)?;
    let mut stmt = db.conn.prepare(
        "SELECT d.code,d.region,d.name_vi,d.name_en,d.cadence,d.month,d.day,d.weekend_rule,
                d.lead_days,d.url,d.note,d.source_code,d.verified
         FROM tax_deadlines d
         WHERE d.enabled=1 AND d.region IN (SELECT value FROM json_each(?1))
         ORDER BY d.region,d.code",
    )?;
    let rows = stmt.query_map(
        [serde_json::to_string(&effective).unwrap_or_else(|_| "[]".into())],
        |r| {
            Ok(Rule {
                code: r.get(0)?,
                region: r.get(1)?,
                name_vi: r.get(2)?,
                name_en: r.get(3)?,
                cadence: r.get(4)?,
                month: r.get(5)?,
                day: r.get(6)?,
                weekend_rule: r.get(7)?,
                lead_days: r.get(8)?,
                url: r.get(9)?,
                note: r.get(10)?,
                source_code: r.get(11)?,
                verified: r.get::<_, i64>(12)? != 0,
            })
        },
    )?;
    rows.collect()
}

/// Region timezone as declared in `region.toml`; UTC when absent.
fn timezone(db: &db::Database, code: &str) -> String {
    db.conn
        .query_row("SELECT timezone FROM regions WHERE code=?1", [code], |r| {
            r.get::<_, String>(0)
        })
        .unwrap_or_default()
}

/// Computes the calendar for the effective region set, sorted by due date.
pub fn calendar(
    db: &db::Database,
    region_filter: Option<&str>,
    now: i64,
    horizon_days: i64,
) -> rusqlite::Result<Vec<Deadline>> {
    let mut zones: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    let mut out = Vec::new();
    for rule in rules(db, region_filter)? {
        let zone = zones
            .entry(rule.region.clone())
            .or_insert_with(|| timezone(db, &rule.region))
            .clone();
        let today = today_in(&zone, now);
        let Some(due) = next_due(&rule, today) else {
            continue;
        };
        let days_until = (due - today).num_days();
        if days_until > horizon_days {
            continue;
        }
        let nominal = nominal_for(&rule, due).unwrap_or(due);
        out.push(Deadline {
            code: rule.code,
            region: rule.region,
            name_vi: rule.name_vi,
            name_en: rule.name_en,
            cadence: rule.cadence,
            source_code: rule.source_code,
            url: rule.url,
            note: rule.note,
            verified: rule.verified,
            lead_days: rule.lead_days,
            due_on: due.format("%Y-%m-%d").to_string(),
            days_until,
            due_soon: days_until <= rule.lead_days,
            nominal_on: nominal.format("%Y-%m-%d").to_string(),
            weekend_rule: rule.weekend_rule,
        });
    }
    out.sort_by(|a, b| a.due_on.cmp(&b.due_on).then(a.code.cmp(&b.code)));
    Ok(out)
}

/// The unshifted date that produced `due`, so the UI can explain a shift.
fn nominal_for(rule: &Rule, due: NaiveDate) -> Option<NaiveDate> {
    if rule.weekend_rule == "none" || rule.weekend_rule.is_empty() {
        return Some(due);
    }
    match rule.cadence.as_str() {
        "yearly" => nominal_date(rule, due.year()),
        "monthly" | "quarterly" => clamped_date(due.year(), due.month(), rule.day? as u32),
        _ => None,
    }
}

#[derive(Deserialize)]
pub struct CalendarQuery {
    region: Option<String>,
    /// How far ahead to look, in days. Defaults to 120 and is capped at a year.
    horizon_days: Option<i64>,
}

pub async fn list_calendar(
    State(state): State<SharedState>,
    Query(q): Query<CalendarQuery>,
) -> Response {
    let region = q
        .region
        .as_deref()
        .map(|code| code.trim().to_ascii_lowercase())
        .filter(|code| !code.is_empty());
    let horizon = q.horizon_days.unwrap_or(120).clamp(1, 366);
    let db = state.db.lock().expect("database lock poisoned");
    if let Some(code) = region.as_deref()
        && !matches!(regions::region_enabled(&db, code), Ok(true))
    {
        return crate::failure(axum::http::StatusCode::BAD_REQUEST, "invalid_region");
    }
    match calendar(&db, region.as_deref(), crate::now() as i64, horizon) {
        Ok(data) => Json(serde_json::json!({
            "data": data,
            "disclaimer_vi": DISCLAIMER_VI,
            "disclaimer_en": DISCLAIMER_EN,
            "holidays_modelled": false,
        }))
        .into_response(),
        Err(err) => internal_error(err),
    }
}

#[cfg(test)]
#[path = "tax_tests.rs"]
mod tests;

/// Admin view: every declared rule with the region's timezone, including disabled ones,
/// so the owner can see what a pack would contribute before turning it on.
#[derive(Debug, serde::Serialize)]
pub struct DeclaredRule {
    pub code: String,
    pub region: String,
    pub name_vi: String,
    pub name_en: String,
    pub cadence: String,
    pub day: Option<i64>,
    pub month: Option<i64>,
    pub weekend_rule: String,
    pub verified: bool,
    pub enabled: bool,
    pub note: String,
}

pub async fn declared_rules(State(state): State<SharedState>) -> Response {
    let db = state.db.lock().expect("database lock poisoned");
    let result = (|| -> rusqlite::Result<Vec<DeclaredRule>> {
        let mut stmt = db.conn.prepare(
            "SELECT code,region,name_vi,name_en,cadence,day,month,weekend_rule,verified,enabled,note
             FROM tax_deadlines ORDER BY region,code",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok(DeclaredRule {
                code: r.get(0)?,
                region: r.get(1)?,
                name_vi: r.get(2)?,
                name_en: r.get(3)?,
                cadence: r.get(4)?,
                day: r.get(5)?,
                month: r.get(6)?,
                weekend_rule: r.get(7)?,
                verified: r.get::<_, i64>(8)? != 0,
                enabled: r.get::<_, i64>(9)? != 0,
                note: r.get(10)?,
            })
        })?;
        rows.collect()
    })();
    match result {
        Ok(data) => Json(serde_json::json!({
            "data": data,
            "disclaimer_vi": DISCLAIMER_VI,
            "holidays_modelled": false,
        }))
        .into_response(),
        Err(err) => internal_error(err),
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RuleToggle {
    enabled: bool,
}

/// Enables or disables one declared rule. A rule is never enabled by discovery, and an
/// admin can only toggle it — the dates themselves come from the pack file.
pub async fn set_rule_enabled(
    State(state): State<SharedState>,
    axum::extract::Path(code): axum::extract::Path<String>,
    Json(input): Json<RuleToggle>,
) -> Response {
    let db = state.db.lock().expect("database lock poisoned");
    // A rule whose region is off would produce dates nobody can see; refuse it so the
    // admin is told to enable the region instead of wondering where the dates went.
    if input.enabled {
        let region: Option<String> = db
            .conn
            .query_row(
                "SELECT region FROM tax_deadlines WHERE code=?1",
                [&code],
                |r| r.get(0),
            )
            .optional()
            .ok()
            .flatten();
        let Some(region) = region else {
            return crate::failure(axum::http::StatusCode::NOT_FOUND, "not_found");
        };
        if !matches!(regions::region_enabled(&db, &region), Ok(true)) {
            return crate::failure(axum::http::StatusCode::CONFLICT, "invalid_region");
        }
    }
    match db.conn.execute(
        "UPDATE tax_deadlines SET enabled=?2 WHERE code=?1",
        rusqlite::params![code, input.enabled],
    ) {
        Ok(0) => crate::failure(axum::http::StatusCode::NOT_FOUND, "not_found"),
        Ok(_) => Json(serde_json::json!({ "ok": true })).into_response(),
        Err(err) => internal_error(err),
    }
}
