//! Alert rules and the in-app notification inbox (P7 foundation, offline).
//!
//! A rule is a *declaration* the admin makes, not a guess: it names one match
//! (`region`, `topic`, `entity` or `keyword`) and fires when a stored article matches.
//! Three properties are enforced here and each is covered by a test:
//!
//! * **Exactly once per event.** `alert_deliveries` is UNIQUE on (rule, article), so a
//!   second sweep of the same article inserts nothing, no matter how often it runs.
//! * **Cooldown.** A rule that already fired inside `cooldown_seconds` records a
//!   `cooldown` skip instead of delivering, and can fire again once the window passes.
//! * **Quiet hours.** A rule with a quiet window is silent inside it — but *silent is not
//!   "lost"*: the sweep records the skip and never marks the article as delivered, so it
//!   fires after the window ends.
//!
//! The in-app inbox is written synchronously here. Telegram is *queued* here and sent by
//! [`crate::telegram::flush`] once the database lock is released, so network I/O never
//! runs under the lock; email and webhooks are still declared but unimplemented.

use crate::{SharedState, db, internal_error};
use axum::{
    Json,
    extract::{Path, Query, State},
    response::{IntoResponse, Response},
};
use chrono::{NaiveTime, TimeZone, Timelike, Utc};
use rusqlite::{OptionalExtension, named_params, params};
use serde::{Deserialize, Serialize};

pub const CHANNEL_IN_APP: &str = "in-app";
/// Channels named by the plan but not implemented yet. `telegram` is *not* here: it is
/// implemented and accepted while `telegram_mode` is on (see [`crate::telegram`]).
pub const UNIMPLEMENTED_CHANNELS: &[&str] = &["email", "webhook", "imap", "slack"];

/// Channels a rule may name right now: the in-app inbox always, plus Telegram while it
/// is switched on (mode `relay` or `direct`). Returned to the admin surface so the UI
/// offers exactly what works.
pub fn available_channels(telegram_on: bool) -> Vec<&'static str> {
    let mut channels = vec![CHANNEL_IN_APP];
    if telegram_on {
        channels.push(crate::telegram::CHANNEL);
    }
    channels
}

#[derive(Debug, Clone, Serialize)]
pub struct Rule {
    pub id: i64,
    pub slug: String,
    pub label_vi: String,
    pub label_en: String,
    pub enabled: bool,
    pub match_kind: String,
    pub match_value: String,
    pub level: String,
    pub cooldown_seconds: i64,
    pub quiet_from: String,
    pub quiet_to: String,
    pub timezone: String,
    pub channels: String,
    pub last_fired_at: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Delivery {
    pub id: i64,
    pub rule_slug: String,
    pub rule_label_vi: String,
    pub rule_label_en: String,
    pub article_id: i64,
    pub article_title: String,
    pub article_url: String,
    pub level: String,
    pub channel: String,
    pub title: String,
    pub body: String,
    pub read: bool,
    pub created_at: i64,
}

/// What one article means for one rule.
#[derive(Debug, PartialEq)]
pub enum Outcome {
    Fire,
    Skip(&'static str),
}

/// Parse `HH:MM`. Empty means "no boundary" and is reported as `None` by the caller.
fn parse_clock(value: &str) -> Option<NaiveTime> {
    NaiveTime::parse_from_str(value.trim(), "%H:%M").ok()
}

/// Minutes since midnight in the rule's own timezone; unknown zone falls back to UTC
/// and the caller keeps the declared name, the same way the tax engine does.
pub fn minutes_of_day(timezone: &str, now: i64) -> u32 {
    let utc = Utc.timestamp_opt(now, 0).single().unwrap_or_default();
    let (hour, minute) = match timezone.parse::<chrono_tz::Tz>() {
        Ok(zone) => {
            let local = utc.with_timezone(&zone);
            (local.hour(), local.minute())
        }
        Err(_) => (utc.hour(), utc.minute()),
    };
    hour * 60 + minute
}

/// True when the local time (minutes since local midnight) is inside [from, to). A
/// window that wraps midnight (`22:00`–`07:00`) is honoured: quiet when after the start
/// **or** before the end.
pub fn in_quiet_hours(quiet_from: &str, quiet_to: &str, now: u32) -> bool {
    let (Some(from), Some(to)) = (parse_clock(quiet_from), parse_clock(quiet_to)) else {
        return false;
    };
    let start = from.hour() * 60 + from.minute();
    let end = to.hour() * 60 + to.minute();
    if start == end {
        return false;
    }
    if start < end {
        now >= start && now < end
    } else {
        now >= start || now < end
    }
}

/// Decide what a rule does with an article. `last_fired` is the rule's own timestamp,
/// `already_delivered` comes from the UNIQUE delivery row.
pub fn decide(rule: &Rule, now: i64, in_quiet: bool, already_delivered: bool) -> Outcome {
    if !rule.enabled {
        return Outcome::Skip("disabled");
    }
    if already_delivered {
        return Outcome::Skip("already_delivered");
    }
    if in_quiet {
        return Outcome::Skip("quiet_hours");
    }
    if let Some(last) = rule.last_fired_at
        && now.saturating_sub(last) < rule.cooldown_seconds
    {
        return Outcome::Skip("cooldown");
    }
    Outcome::Fire
}

pub fn rules(db: &db::Database) -> rusqlite::Result<Vec<Rule>> {
    let mut stmt = db.conn.prepare(
        "SELECT id,slug,label_vi,label_en,enabled,match_kind,match_value,level,
                cooldown_seconds,quiet_from,quiet_to,timezone,channels,last_fired_at
         FROM alert_rules ORDER BY slug",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok(Rule {
            id: r.get(0)?,
            slug: r.get(1)?,
            label_vi: r.get(2)?,
            label_en: r.get(3)?,
            enabled: r.get::<_, i64>(4)? != 0,
            match_kind: r.get(5)?,
            match_value: r.get(6)?,
            level: r.get(7)?,
            cooldown_seconds: r.get(8)?,
            quiet_from: r.get(9)?,
            quiet_to: r.get(10)?,
            timezone: r.get(11)?,
            channels: r.get(12)?,
            last_fired_at: r.get(13)?,
        })
    })?;
    rows.collect()
}

/// The SQL predicate that decides whether an article matches a rule. Kept as a
/// fragment so the sweep and any preview answer the same question.
fn match_clause(kind: &str) -> Option<&'static str> {
    match kind {
        "region" => Some(
            "EXISTS (SELECT 1 FROM article_matches m
                      WHERE m.article_id=a.id AND m.kind='region' AND m.value=:mv)",
        ),
        "topic" => Some("a.topic_slug = :mv"),
        "entity" => Some(
            "EXISTS (SELECT 1 FROM article_matches m
                      WHERE m.article_id=a.id AND m.kind='entity' AND m.value=:mv)",
        ),
        "keyword" => Some("instr(lower(a.title), lower(:mv)) > 0"),
        _ => None,
    }
}

/// Articles a rule matches, only within the effective region set and only from a topic
/// that is enabled — the same visibility rules the reader uses, so a rule can never
/// notify about something the reader would hide.
pub fn matching_articles(
    db: &db::Database,
    rule: &Rule,
    limit: i64,
) -> rusqlite::Result<Vec<(i64, String, String)>> {
    let Some(clause) = match_clause(&rule.match_kind) else {
        return Ok(Vec::new());
    };
    let sql = format!(
        "WITH RECURSIVE up(code) AS (
           SELECT code FROM regions WHERE enabled=1
           UNION SELECT r.parent_code FROM regions r JOIN up ON r.code=up.code
                 WHERE r.parent_code IS NOT NULL)
         SELECT a.id, a.title, a.canonical_url
         FROM articles a LEFT JOIN topics t ON t.slug=a.topic_slug
         WHERE (a.topic_slug IS NULL OR t.enabled=1)
           AND COALESCE(NULLIF(a.region_code,''),'global') IN (SELECT code FROM up)
           AND {clause}
         ORDER BY a.id DESC LIMIT :lim"
    );
    let mut stmt = db.conn.prepare(&sql)?;
    let rows = stmt.query_map(
        named_params! {":mv": rule.match_value, ":lim": limit},
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
    )?;
    rows.collect()
}

/// Run every enabled rule once. Returns how many notifications were created, how many
/// (rule, article) pairs were skipped this pass keyed by reason, and which deliveries
/// ask for Telegram (`true`) — those are queued once the caller owns the network step.
pub fn sweep(db: &db::Database, now: i64) -> anyhow::Result<SweepOutcome> {
    let all = rules(db)?;
    let mut created = 0usize;
    let mut pushed: Vec<(i64, bool)> = Vec::new();
    let mut skips: std::collections::BTreeMap<String, i64> = Default::default();
    for mut rule in all {
        if !rule.enabled {
            continue;
        }
        // Re-read the rule's own firing time per article: once it fires, the cooldown
        // starts, so a single sweep cannot deliver a burst from one rule.
        let quiet = in_quiet_hours(
            &rule.quiet_from,
            &rule.quiet_to,
            minutes_of_day(&rule.timezone, now),
        );
        for (article_id, title, url) in matching_articles(db, &rule, 200)? {
            let delivered: bool = db
                .conn
                .query_row(
                    "SELECT 1 FROM alert_deliveries WHERE rule_id=?1 AND article_id=?2",
                    params![rule.id, article_id],
                    |_| Ok(true),
                )
                .optional()?
                .unwrap_or(false);
            match decide(&rule, now, quiet, delivered) {
                Outcome::Fire => {
                    let wants_telegram = rule
                        .channels
                        .split(',')
                        .map(str::trim)
                        .any(|channel| channel == crate::telegram::CHANNEL);
                    db.conn.execute(
                        "INSERT INTO alert_deliveries(rule_id,article_id,level,channel,title,body,created_at)
                         VALUES (?1,?2,?3,?4,?5,?6,?7)",
                        params![rule.id, article_id, rule.level, CHANNEL_IN_APP, title, url, now],
                    )?;
                    pushed.push((db.conn.last_insert_rowid(), wants_telegram));
                    db.conn.execute(
                        "UPDATE alert_rules SET last_fired_at=?2, updated_at=CURRENT_TIMESTAMP WHERE id=?1",
                        params![rule.id, now],
                    )?;
                    rule.last_fired_at = Some(now);
                    db.conn.execute(
                        "DELETE FROM alert_skip_log WHERE rule_id=?1 AND article_id=?2",
                        params![rule.id, article_id],
                    )?;
                    created += 1;
                }
                Outcome::Skip(reason) => {
                    if reason != "disabled" {
                        *skips.entry(reason.to_string()).or_default() += 1;
                        db.conn.execute(
                            "INSERT OR IGNORE INTO alert_skip_log(rule_id,article_id,reason,created_at)
                             VALUES (?1,?2,?3,?4)",
                            params![rule.id, article_id, reason, now],
                        )?;
                    }
                }
            }
        }
    }
    Ok((created, skips.into_iter().collect(), pushed))
}

/// What one sweep produced: how many notifications were written, why the rest were
/// skipped, and which deliveries ask for Telegram (delivery id, wants it).
pub type SweepOutcome = (usize, Vec<(String, i64)>, Vec<(i64, bool)>);

/// Queue one delivery for Telegram. Sending happens later, outside the database lock,
/// so the sweep only writes this row. Returns true when a row was queued.
pub fn queue_telegram(db: &db::Database, delivery_id: i64, now: i64) -> bool {
    crate::telegram::enqueue(db, delivery_id, now).is_ok()
}

pub fn inbox(db: &db::Database, unread_only: bool, limit: i64) -> rusqlite::Result<Vec<Delivery>> {
    let mut stmt = db.conn.prepare(
        "SELECT d.id, r.slug, r.label_vi, r.label_en, d.article_id, a.title, a.canonical_url,
                d.level, d.channel, d.title, d.body, d.read_at, d.created_at
         FROM alert_deliveries d
         JOIN alert_rules r ON r.id=d.rule_id
         JOIN articles a ON a.id=d.article_id
         WHERE (?1=0 OR d.read_at IS NULL)
         ORDER BY d.created_at DESC, d.id DESC LIMIT ?2",
    )?;
    let rows = stmt.query_map(params![unread_only as i64, limit], |r| {
        Ok(Delivery {
            id: r.get(0)?,
            rule_slug: r.get(1)?,
            rule_label_vi: r.get(2)?,
            rule_label_en: r.get(3)?,
            article_id: r.get(4)?,
            article_title: r.get(5)?,
            article_url: r.get(6)?,
            level: r.get(7)?,
            channel: r.get(8)?,
            title: r.get(9)?,
            body: r.get(10)?,
            read: r.get::<_, Option<i64>>(11)?.is_some(),
            created_at: r.get(12)?,
        })
    })?;
    rows.collect()
}

pub fn unread_count(db: &db::Database) -> rusqlite::Result<i64> {
    db.conn.query_row(
        "SELECT COUNT(*) FROM alert_deliveries WHERE read_at IS NULL",
        [],
        |r| r.get(0),
    )
}

pub fn mark_read(db: &db::Database, id: i64, now: i64) -> rusqlite::Result<bool> {
    let changed = db.conn.execute(
        "UPDATE alert_deliveries SET read_at=?2 WHERE id=?1 AND read_at IS NULL",
        params![id, now],
    )?;
    Ok(changed > 0)
}

pub fn mark_all_read(db: &db::Database, now: i64) -> rusqlite::Result<usize> {
    db.conn.execute(
        "UPDATE alert_deliveries SET read_at=?1 WHERE read_at IS NULL",
        params![now],
    )
}

#[derive(Deserialize)]
pub struct RuleInput {
    pub slug: String,
    pub label_vi: String,
    pub label_en: String,
    #[serde(default)]
    pub enabled: bool,
    pub match_kind: String,
    pub match_value: String,
    #[serde(default = "default_level")]
    pub level: String,
    #[serde(default = "default_cooldown")]
    pub cooldown_seconds: i64,
    #[serde(default)]
    pub quiet_from: String,
    #[serde(default)]
    pub quiet_to: String,
    #[serde(default = "default_tz")]
    pub timezone: String,
    #[serde(default = "default_channels")]
    pub channels: String,
}
fn default_level() -> String {
    "info".into()
}
fn default_cooldown() -> i64 {
    21_600
}
fn default_tz() -> String {
    "UTC".into()
}
fn default_channels() -> String {
    CHANNEL_IN_APP.into()
}

/// Validation is separate from storage so the API can answer with a precise code and
/// the tests can call it directly without a database. `telegram_ok` says whether
/// Telegram is switched on, which is what decides if the `telegram` channel exists.
pub fn validate(input: &RuleInput, telegram_ok: bool) -> Result<(), &'static str> {
    if input.slug.trim().is_empty() || input.slug.len() > 60 {
        return Err("invalid_slug");
    }
    if !input
        .slug
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
    {
        return Err("invalid_slug");
    }
    if input.label_vi.trim().is_empty() || input.label_en.trim().is_empty() {
        return Err("label_required");
    }
    if match_clause(&input.match_kind).is_none() {
        return Err("invalid_match_kind");
    }
    if input.match_value.trim().is_empty() {
        return Err("match_value_required");
    }
    if !["info", "important", "urgent"].contains(&input.level.as_str()) {
        return Err("invalid_level");
    }
    if input.cooldown_seconds < 0 || input.cooldown_seconds > 30 * 24 * 3600 {
        return Err("invalid_cooldown");
    }
    for (value, label) in [
        (input.quiet_from.as_str(), "quiet_from"),
        (input.quiet_to.as_str(), "quiet_to"),
    ] {
        if !value.trim().is_empty() && parse_clock(value).is_none() {
            return Err(if label == "quiet_from" {
                "invalid_quiet_from"
            } else {
                "invalid_quiet_to"
            });
        }
    }
    if input.quiet_from.trim().is_empty() != input.quiet_to.trim().is_empty() {
        return Err("quiet_needs_both_bounds");
    }
    if input.timezone.parse::<chrono_tz::Tz>().is_err() {
        return Err("invalid_timezone");
    }
    // Only channels that can actually deliver are accepted. A stored channel nothing
    // sends to would look like a working notification while sending nothing, so it is
    // refused outright; `telegram_ok` is true exactly while Telegram is switched on.
    for channel in input
        .channels
        .split(',')
        .map(str::trim)
        .filter(|c| !c.is_empty())
    {
        let known = channel == CHANNEL_IN_APP
            || (channel == crate::telegram::CHANNEL && telegram_ok);
        if !known {
            return Err("channel_not_implemented");
        }
    }
    Ok(())
}

pub fn upsert(db: &db::Database, input: &RuleInput) -> anyhow::Result<()> {
    db.conn.execute(
        "INSERT INTO alert_rules(slug,label_vi,label_en,enabled,match_kind,match_value,level,
                                 cooldown_seconds,quiet_from,quiet_to,timezone,channels,updated_at)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,CURRENT_TIMESTAMP)
         ON CONFLICT(slug) DO UPDATE SET
           label_vi=excluded.label_vi, label_en=excluded.label_en, enabled=excluded.enabled,
           match_kind=excluded.match_kind, match_value=excluded.match_value, level=excluded.level,
           cooldown_seconds=excluded.cooldown_seconds, quiet_from=excluded.quiet_from,
           quiet_to=excluded.quiet_to, timezone=excluded.timezone, channels=excluded.channels,
           updated_at=CURRENT_TIMESTAMP",
        params![
            input.slug.trim(),
            input.label_vi.trim(),
            input.label_en.trim(),
            input.enabled as i64,
            input.match_kind,
            input.match_value.trim(),
            input.level,
            input.cooldown_seconds,
            input.quiet_from.trim(),
            input.quiet_to.trim(),
            input.timezone,
            input.channels,
        ],
    )?;
    Ok(())
}

pub fn set_enabled(db: &db::Database, slug: &str, enabled: bool) -> rusqlite::Result<bool> {
    let changed = db.conn.execute(
        "UPDATE alert_rules SET enabled=?2, updated_at=CURRENT_TIMESTAMP WHERE slug=?1",
        params![slug, enabled as i64],
    )?;
    Ok(changed > 0)
}

/// How often the background sweep looks for new matches.
pub const SWEEP_EVERY: std::time::Duration = std::time::Duration::from_secs(120);
/// How often queued Telegram alerts are flushed. Shorter than the sweep so a queued
/// alert leaves within a minute of being created.
pub const FLUSH_EVERY: std::time::Duration = std::time::Duration::from_secs(30);

/// Start the background sweep once. Every firing is still guarded by the delivery
/// UNIQUE row and the cooldown, so an interval that runs while nothing changed writes
/// nothing. This reads the database only; it never opens a connection.
pub fn spawn_sweeper(state: crate::SharedState) {
    // The flusher runs on its own clock so a queued alert leaves within `FLUSH_EVERY`
    // of being created, instead of waiting for the next two-minute sweep.
    let flushing = state.clone();
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(FLUSH_EVERY).await;
            crate::telegram::flush(flushing.clone()).await;
        }
    });
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(SWEEP_EVERY).await;
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs() as i64;
            let outcome = {
                let db = match state.db.lock() {
                    Ok(db) => db,
                    Err(_) => continue,
                };
                sweep(&db, now).map(|(created, skips, pushed)| {
                    // Queueing is a local write; the send itself stays outside the lock.
                    let telegram_ok = crate::telegram::can_send(
                        &db,
                        state.secrets.as_ref(),
                        &state.telegram,
                    );
                    if telegram_ok {
                        for (id, _) in pushed.iter().filter(|(_, wants)| *wants) {
                            queue_telegram(&db, *id, now);
                        }
                    }
                    (created, skips)
                })
            };
            match outcome {
                Ok((created, _)) if created > 0 => {
                    tracing::info!(created, "alert sweep delivered notifications");
                }
                Ok(_) => {}
                Err(err) => tracing::warn!(%err, "alert sweep failed"),
            }
        }
    });
}

// ---------------------------------------------------------------- handlers

#[derive(Deserialize)]
pub struct InboxQuery {
    unread: Option<u8>,
    limit: Option<i64>,
}

pub async fn list_inbox(
    State(state): State<SharedState>,
    Query(query): Query<InboxQuery>,
) -> Response {
    let limit = query.limit.unwrap_or(50).clamp(1, 200);
    let db = state.db.lock().expect("database lock poisoned");
    match (
        inbox(&db, query.unread.unwrap_or(0) == 1, limit),
        unread_count(&db),
    ) {
        (Ok(data), Ok(unread)) => {
            Json(serde_json::json!({"data": data, "unread": unread})).into_response()
        }
        (Err(err), _) | (_, Err(err)) => internal_error(err),
    }
}

pub async fn read_one(State(state): State<SharedState>, Path(id): Path<i64>) -> Response {
    let db = state.db.lock().expect("database lock poisoned");
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;
    match mark_read(&db, id, now) {
        Ok(true) => Json(serde_json::json!({"ok": true})).into_response(),
        Ok(false) => crate::failure(axum::http::StatusCode::NOT_FOUND, "not_found"),
        Err(err) => internal_error(err),
    }
}

pub async fn read_all(State(state): State<SharedState>) -> Response {
    let db = state.db.lock().expect("database lock poisoned");
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;
    match mark_all_read(&db, now) {
        Ok(count) => Json(serde_json::json!({"ok": true, "marked": count})).into_response(),
        Err(err) => internal_error(err),
    }
}

pub async fn list_rules(State(state): State<SharedState>) -> Response {
    let db = state.db.lock().expect("database lock poisoned");
    let telegram_on = crate::telegram::is_on(&db, &state.telegram);
    match rules(&db) {
        Ok(data) => Json(serde_json::json!({
            "data": data,
            "channels": available_channels(telegram_on),
            "unimplemented_channels": UNIMPLEMENTED_CHANNELS,
        }))
        .into_response(),
        Err(err) => internal_error(err),
    }
}

pub async fn put_rule(State(state): State<SharedState>, Json(input): Json<RuleInput>) -> Response {
    let db = state.db.lock().expect("database lock poisoned");
    let telegram_on = crate::telegram::is_on(&db, &state.telegram);
    if let Err(code) = validate(&input, telegram_on) {
        return crate::failure(axum::http::StatusCode::BAD_REQUEST, code);
    }
    match upsert(&db, &input) {
        Ok(()) => Json(serde_json::json!({"ok": true, "slug": input.slug.trim()})).into_response(),
        Err(err) => internal_error(err),
    }
}

#[derive(Deserialize)]
pub struct Toggle {
    enabled: bool,
}

pub async fn toggle_rule(
    State(state): State<SharedState>,
    Path(slug): Path<String>,
    Json(input): Json<Toggle>,
) -> Response {
    let db = state.db.lock().expect("database lock poisoned");
    match set_enabled(&db, &slug, input.enabled) {
        Ok(true) => Json(serde_json::json!({"ok": true})).into_response(),
        Ok(false) => crate::failure(axum::http::StatusCode::NOT_FOUND, "not_found"),
        Err(err) => internal_error(err),
    }
}

/// Manual sweep. The scheduler-free equivalent of "check now"; the same function the
/// background interval calls, plus the Telegram queue it leaves behind.
pub async fn run_sweep(State(state): State<SharedState>) -> Response {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;
    let outcome = {
        let db = state.db.lock().expect("database lock poisoned");
        sweep(&db, now).map(|(created, skips, pushed)| {
            let telegram_ok =
                crate::telegram::can_send(&db, state.secrets.as_ref(), &state.telegram);
            let mut queued = 0usize;
            if telegram_ok {
                for (id, _) in pushed.iter().filter(|(_, wants)| *wants) {
                    if queue_telegram(&db, *id, now) {
                        queued += 1;
                    }
                }
            }
            (created, skips, queued)
        })
    };
    match outcome {
        Ok((created, skips, queued)) => {
            let flushed = crate::telegram::flush(state.clone()).await;
            Json(serde_json::json!({
                "ok": true,
                "created": created,
                "queued_telegram": queued,
                "telegram_sent": flushed.sent,
                "telegram_failed": flushed.failed,
                "skipped": skips.into_iter().map(|(reason, count)| serde_json::json!({"reason": reason, "count": count})).collect::<Vec<_>>(),
            }))
            .into_response()
        }
        Err(err) => internal_error(err),
    }
}

#[cfg(test)]
#[path = "alerts_tests.rs"]
mod db_tests;

#[cfg(test)]
mod tests {
    use super::*;

    fn rule(enabled: bool, last: Option<i64>, cooldown: i64) -> Rule {
        Rule {
            id: 1,
            slug: "btc".into(),
            label_vi: "Bitcoin".into(),
            label_en: "Bitcoin".into(),
            enabled,
            match_kind: "entity".into(),
            match_value: "BTC".into(),
            level: "important".into(),
            cooldown_seconds: cooldown,
            quiet_from: String::new(),
            quiet_to: String::new(),
            timezone: "UTC".into(),
            channels: CHANNEL_IN_APP.into(),
            last_fired_at: last,
        }
    }

    #[test]
    fn a_disabled_rule_never_fires() {
        assert_eq!(
            decide(&rule(false, None, 0), 100, false, false),
            Outcome::Skip("disabled")
        );
    }

    #[test]
    fn an_event_delivered_once_never_fires_again() {
        assert_eq!(
            decide(&rule(true, None, 0), 100, false, true),
            Outcome::Skip("already_delivered")
        );
    }

    #[test]
    fn the_cooldown_blocks_a_second_event_then_expires() {
        let r = rule(true, Some(1_000), 600);
        assert_eq!(decide(&r, 1_500, false, false), Outcome::Skip("cooldown"));
        assert_eq!(decide(&r, 1_600, false, false), Outcome::Fire);
    }

    #[test]
    fn quiet_hours_block_and_a_plain_window_does_not_wrap() {
        // 22:00-07:00 wraps midnight; 09:00-17:00 does not.
        assert!(in_quiet_hours("22:00", "07:00", 23 * 60));
        assert!(in_quiet_hours("22:00", "07:00", 3 * 60));
        assert!(!in_quiet_hours("22:00", "07:00", 12 * 60));
        assert!(in_quiet_hours("09:00", "17:00", 9 * 60));
        assert!(!in_quiet_hours("09:00", "17:00", 17 * 60));
        // A malformed or empty window is "no window", never "always quiet".
        assert!(!in_quiet_hours("", "", 0));
        assert!(!in_quiet_hours("25:99", "07:00", 60));
        assert!(!in_quiet_hours("09:00", "09:00", 9 * 60));
    }

    #[test]
    fn quiet_hours_outrank_the_cooldown_without_marking_delivery() {
        let r = rule(true, Some(1_000), 600);
        assert_eq!(
            decide(&r, 10_000, true, false),
            Outcome::Skip("quiet_hours")
        );
    }

    #[test]
    fn timezone_shifts_the_local_minute() {
        // 2026-01-01T00:30:00Z is 07:30 in Vietnam.
        let t = 1_767_227_400; // 2026-01-01T00:30:00Z
        assert!(in_quiet_hours(
            "07:00",
            "08:00",
            minutes_of_day("Asia/Ho_Chi_Minh", t)
        ));
        assert!(!in_quiet_hours("07:00", "08:00", minutes_of_day("UTC", t)));
    }

    #[test]
    fn validation_refuses_channels_that_cannot_deliver() {
        let mut input = RuleInput {
            slug: "btc".into(),
            label_vi: "Bitcoin".into(),
            label_en: "Bitcoin".into(),
            enabled: true,
            match_kind: "entity".into(),
            match_value: "BTC".into(),
            level: "info".into(),
            cooldown_seconds: 60,
            quiet_from: String::new(),
            quiet_to: String::new(),
            timezone: "UTC".into(),
            channels: "in-app".into(),
        };
        assert_eq!(validate(&input, false), Ok(()));
        input.channels = "telegram".into();
        assert_eq!(validate(&input, false), Err("channel_not_implemented"));
        input.channels = "in-app".into();
        input.match_kind = "sql".into();
        assert_eq!(validate(&input, false), Err("invalid_match_kind"));
        input.match_kind = "entity".into();
        input.quiet_from = "22:00".into();
        assert_eq!(validate(&input, false), Err("quiet_needs_both_bounds"));
        input.quiet_to = "07:00".into();
        assert_eq!(validate(&input, false), Ok(()));
        input.timezone = "Mars/Base".into();
        assert_eq!(validate(&input, false), Err("invalid_timezone"));
        input.timezone = "UTC".into();
        input.slug = "BTC Room".into();
        assert_eq!(validate(&input, false), Err("invalid_slug"));
    }
}
