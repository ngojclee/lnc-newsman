//! Feed collection: one run per source, scheduling, back-off, auto-pause and the
//! reader/admin endpoints built on top of the stored article metadata.

use crate::{
    SharedState, catalog, failure, feed,
    fetch::{FetchOutcome, Validators},
    internal_error, now, regions,
};
use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    time::{Duration, Instant},
};

/// Consecutive failures before a source is paused and needs an admin to resume it.
pub const PAUSE_AFTER_FAILURES: i64 = 5;
const MAX_BACKOFF_SECONDS: i64 = 24 * 60 * 60;
pub(crate) const HOST_GAP: Duration = Duration::from_secs(5);
const SCHEDULER_TICK: Duration = Duration::from_secs(30);
const MAX_SOURCES_PER_TICK: i64 = 20;
const RUNS_KEPT_PER_SOURCE: i64 = 50;

#[derive(Debug, Serialize, PartialEq)]
pub struct RunSummary {
    pub status: &'static str,
    pub items_seen: usize,
    pub items_new: usize,
    pub error: Option<String>,
    pub paused: bool,
}

#[derive(Debug)]
pub enum CollectError {
    NotFound,
    Busy,
    Database(rusqlite::Error),
}
impl From<rusqlite::Error> for CollectError {
    fn from(value: rusqlite::Error) -> Self {
        Self::Database(value)
    }
}
impl CollectError {
    fn response(self) -> Response {
        match self {
            Self::NotFound => failure(StatusCode::NOT_FOUND, "not_found"),
            Self::Busy => failure(StatusCode::CONFLICT, "fetch_in_progress"),
            Self::Database(err) => internal_error(err),
        }
    }
}

struct Job {
    url: String,
    topic_slug: Option<String>,
    region_code: String,
    interval_minutes: i64,
    failures: i64,
    validators: Validators,
}

/// Host of a URL, used to space out requests to the same server.
fn host_of(url: &str) -> String {
    url::Url::parse(url)
        .ok()
        .and_then(|parsed| parsed.host_str().map(str::to_ascii_lowercase))
        .unwrap_or_default()
}

/// Waits until `HOST_GAP` has passed since the last request to `host`, then records
/// this request. Shared through `AppState`, so the scheduler, "Fetch now" and feed
/// preview all obey the same per-host spacing.
async fn host_slot(state: &SharedState, host: &str) {
    if host.is_empty() {
        return;
    }
    let wait = {
        let mut gates = state.host_gates.lock().expect("host gate lock poisoned");
        gates.retain(|_, at| at.elapsed() < Duration::from_secs(60));
        let wait = gates
            .get(host)
            .map(|at| state.host_gap.saturating_sub(at.elapsed()))
            .unwrap_or_default();
        gates.insert(host.to_string(), Instant::now() + wait);
        wait
    };
    if !wait.is_zero() {
        tokio::time::sleep(wait).await;
    }
}

/// Re-reads a source just before its result is applied. The URL, the region and the
/// `enabled` flag may all have changed while the request was in flight; a run that
/// started under the old settings must not write anything.
enum Recheck {
    Gone,
    Changed,
    RegionOff,
    Current,
}

fn recheck(db: &crate::db::Database, id: i64, job: &Job) -> rusqlite::Result<Recheck> {
    let row: Option<(String, String, i64)> = db
        .conn
        .query_row(
            "SELECT url, COALESCE(NULLIF(region,''),'global'), enabled FROM sources WHERE id=?1",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .optional()?;
    match row {
        None => Ok(Recheck::Gone),
        Some((url, _, _)) if url != job.url => Ok(Recheck::Changed),
        Some((_, _, 0)) => Ok(Recheck::RegionOff),
        Some((_, region, _)) if region != job.region_code => Ok(Recheck::RegionOff),
        Some(_) => Ok(Recheck::Current),
    }
}

struct InFlight<'a> {
    state: &'a SharedState,
    id: i64,
}
impl Drop for InFlight<'_> {
    fn drop(&mut self) {
        self.state
            .in_flight
            .lock()
            .expect("in-flight lock poisoned")
            .remove(&self.id);
    }
}

fn rfc3339(ts: i64) -> String {
    chrono::DateTime::from_timestamp(ts, 0)
        .unwrap_or_default()
        .format("%Y-%m-%dT%H:%M:%SZ")
        .to_string()
}

/// Fetch one source now and record the outcome. Network work happens without the DB lock.
pub async fn collect(state: &SharedState, id: i64) -> Result<RunSummary, CollectError> {
    let job = {
        let db = state.db.lock().expect("database lock poisoned");
        db.conn
            .query_row(
                "SELECT url, topic_slug, fetch_interval_minutes, consecutive_failures, etag, last_modified,
                        COALESCE(NULLIF(region,''), 'global'), validators_origin FROM sources WHERE id=?1",
                [id],
                |r| {
                    Ok(Job {
                        url: r.get(0)?,
                        topic_slug: r.get(1)?,
                        interval_minutes: r.get(2)?,
                        failures: r.get(3)?,
                        validators: Validators {
                            etag: r.get(4)?,
                            last_modified: r.get(5)?,
                            origin: r.get(7)?,
                        },
                        region_code: r.get(6)?,
                    })
                },
            )
            .optional()?
            .ok_or(CollectError::NotFound)?
    };
    if !state
        .in_flight
        .lock()
        .expect("in-flight lock poisoned")
        .insert(id)
    {
        return Err(CollectError::Busy);
    }
    let _guard = InFlight { state, id };
    host_slot(state, &host_of(&job.url)).await;
    let started = now() as i64;
    let clock = Instant::now();
    let outcome = match state.fetcher.get(&job.url, &job.validators).await {
        Ok(FetchOutcome::NotModified) => Ok(None),
        Ok(FetchOutcome::Body {
            bytes,
            final_url,
            validators,
        }) => feed::parse(&bytes, &final_url, chrono::Utc::now())
            .map(|items| Some((items, validators)))
            .map_err(|err| err.code().to_string()),
        Err(err) => Err(err.code()),
    };
    let duration_ms = clock.elapsed().as_millis() as i64;
    let mut db = state.db.lock().expect("database lock poisoned");
    // Settings changed mid-flight: drop the result instead of mixing it with the new
    // configuration. Nothing is stored and the failure/back-off state is untouched.
    match recheck(&db, id, &job)? {
        Recheck::Gone => return Err(CollectError::NotFound),
        Recheck::Changed | Recheck::RegionOff => {
            tracing::info!(
                source_id = id,
                "result discarded: source or region changed during the run"
            );
            return Ok(RunSummary {
                status: "discarded",
                items_seen: 0,
                items_new: 0,
                error: Some("source_changed".into()),
                paused: false,
            });
        }
        Recheck::Current => {}
    }
    let tx = db.conn.transaction()?;
    let base = job.interval_minutes.max(5) * 60;
    let summary = match outcome {
        Ok(parsed) => {
            let (seen, new) = match &parsed {
                Some((items, _)) => {
                    let mut new = 0;
                    let mut new_ids = Vec::new();
                    for item in items {
                        let inserted = tx.execute(
                            "INSERT OR IGNORE INTO articles(canonical_url,title,source_id,published_at,guid,topic_slug,fetched_at,region_code) VALUES (?1,?2,?3,?4,?5,?6,?7,?8)",
                            params![item.url, item.title, id, item.published_at, item.guid, job.topic_slug, started, job.region_code],
                        )?;
                        new += inserted;
                        if inserted > 0 {
                            new_ids.push((tx.last_insert_rowid(), item.title.clone()));
                        }
                    }
                    // Rule labels only: no model is called during collection, so a fetch
                    // never depends on an LLM being reachable or affordable.
                    for (article_id, title) in new_ids {
                        // A labelling failure must not lose the article: the title is
                        // already stored and labelling can be re-run at any time.
                        if let Err(err) = crate::classify::save_in(
                            &tx,
                            article_id,
                            &crate::classify::classify(&job.region_code, &title),
                        ) {
                            tracing::warn!(%err, article_id, "could not label an article");
                        }
                    }
                    (items.len(), new)
                }
                None => (0, 0),
            };
            let jitter = (id * 7919 + started) % (base / 10).max(1);
            tx.execute(
                "UPDATE sources SET last_checked_at=?2, last_success_at=?2, last_error=NULL, consecutive_failures=0, paused_reason=NULL, next_fetch_at=?3 WHERE id=?1",
                params![id, rfc3339(started), started + base + jitter],
            )?;
            if let Some((_, validators)) = &parsed {
                tx.execute(
                    "UPDATE sources SET etag=?2, last_modified=?3, validators_origin=?4 WHERE id=?1",
                    params![id, validators.etag, validators.last_modified, validators.origin],
                )?;
            }
            RunSummary {
                status: if parsed.is_some() {
                    "ok"
                } else {
                    "not_modified"
                },
                items_seen: seen,
                items_new: new,
                error: None,
                paused: false,
            }
        }
        Err(code) => {
            let failures = job.failures + 1;
            let paused = failures >= PAUSE_AFTER_FAILURES;
            let backoff = (base << failures.min(6)).min(MAX_BACKOFF_SECONDS).max(base);
            tx.execute(
                "UPDATE sources SET last_checked_at=?2, last_error=?3, consecutive_failures=?4, next_fetch_at=?5, paused_reason=CASE WHEN ?6 THEN 'consecutive_failures' ELSE paused_reason END WHERE id=?1",
                params![id, rfc3339(started), code, failures, started + backoff, paused],
            )?;
            RunSummary {
                status: "error",
                items_seen: 0,
                items_new: 0,
                error: Some(code),
                paused,
            }
        }
    };
    tx.execute(
        "INSERT INTO fetch_runs(source_id,started_at,duration_ms,status,items_seen,items_new,error) VALUES (?1,?2,?3,?4,?5,?6,?7)",
        params![id, started, duration_ms, summary.status, summary.items_seen as i64, summary.items_new as i64, summary.error],
    )?;
    tx.execute(
        "DELETE FROM fetch_runs WHERE source_id=?1 AND id NOT IN (SELECT id FROM fetch_runs WHERE source_id=?1 ORDER BY id DESC LIMIT ?2)",
        params![id, RUNS_KEPT_PER_SOURCE],
    )?;
    tx.commit()?;
    Ok(summary)
}

pub fn collector_enabled(db: &crate::db::Database) -> bool {
    db.get_setting("collector_enabled")
        .ok()
        .flatten()
        .as_deref()
        == Some("true")
}

/// Sources that are enabled, not paused, under an enabled (or no) topic, in an
/// enabled region (or one of its ancestors) and due now.
pub(crate) fn due_sources(state: &SharedState) -> rusqlite::Result<Vec<(i64, String)>> {
    let db = state.db.lock().expect("database lock poisoned");
    if !collector_enabled(&db) {
        return Ok(Vec::new());
    }
    // Paused or disabled regions never run: a source only runs when its region is
    // enabled (the ancestor union keeps children of an enabled region eligible).
    let mut stmt = db.conn.prepare(
        "WITH RECURSIVE active(code) AS (
           SELECT code FROM regions WHERE enabled=1
           UNION SELECT r.parent_code FROM regions r JOIN active ON r.code=active.code WHERE r.parent_code IS NOT NULL
         )
         SELECT s.id, s.url FROM sources s LEFT JOIN topics t ON t.slug=s.topic_slug
         WHERE s.enabled=1 AND s.paused_reason IS NULL AND (s.topic_slug IS NULL OR t.enabled=1)
           AND COALESCE(NULLIF(s.region,''),'global') IN (SELECT code FROM active)
           AND (s.next_fetch_at IS NULL OR s.next_fetch_at <= ?1)
         ORDER BY COALESCE(s.next_fetch_at, 0), s.id LIMIT ?2",
    )?;
    let rows = stmt.query_map(params![now() as i64, MAX_SOURCES_PER_TICK], |r| {
        Ok((r.get(0)?, r.get(1)?))
    })?;
    rows.collect()
}

/// One scheduler pass. Per-host spacing is applied inside `collect` through the
/// shared gate, so manual runs and previews cannot jump the queue either.
pub async fn run_due_once(state: &SharedState) -> usize {
    let due = match due_sources(state) {
        Ok(due) => due,
        Err(err) => {
            tracing::warn!(%err, "could not list due sources");
            return 0;
        }
    };
    let mut ran = 0;
    for (id, _url) in due {
        match collect(state, id).await {
            Ok(summary) => {
                ran += 1;
                tracing::info!(source_id = id, status = summary.status, new = summary.items_new, error = ?summary.error, "source collected");
            }
            Err(CollectError::Busy) => {}
            Err(err) => tracing::warn!(source_id = id, ?err, "collection failed"),
        }
    }
    ran
}

pub fn spawn_scheduler(state: SharedState) {
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(SCHEDULER_TICK);
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            ticker.tick().await;
            run_due_once(&state).await;
        }
    });
}

// ---------- admin endpoints ----------

pub async fn fetch_now(State(state): State<SharedState>, Path(id): Path<i64>) -> Response {
    match collect(&state, id).await {
        Ok(summary) => Json(serde_json::json!({ "data": summary })).into_response(),
        Err(err) => err.response(),
    }
}

pub async fn resume(State(state): State<SharedState>, Path(id): Path<i64>) -> Response {
    let result = state
        .db
        .lock()
        .expect("database lock poisoned")
        .conn
        .execute(
            "UPDATE sources SET paused_reason=NULL, consecutive_failures=0, next_fetch_at=NULL WHERE id=?1",
            [id],
        );
    match result {
        Ok(0) => failure(StatusCode::NOT_FOUND, "not_found"),
        Ok(_) => Json(serde_json::json!({ "ok": true })).into_response(),
        Err(err) => internal_error(err),
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PreviewInput {
    url: String,
}

/// Fetch and parse without storing anything, so an admin can check a feed before saving it.
pub async fn preview(
    State(state): State<SharedState>,
    Json(input): Json<PreviewInput>,
) -> Response {
    let url = input.url.trim();
    if !catalog::valid_source_url(url) {
        return failure(StatusCode::BAD_REQUEST, "invalid_source");
    }
    host_slot(&state, &host_of(url)).await;
    let reason = match state.fetcher.get(url, &Validators::default()).await {
        Ok(FetchOutcome::Body {
            bytes, final_url, ..
        }) => match feed::parse(&bytes, &final_url, chrono::Utc::now()) {
            Ok(items) => {
                let sample: Vec<_> = items
                    .iter()
                    .take(5)
                    .map(|i| serde_json::json!({ "title": i.title, "url": i.url, "published_at": i.published_at }))
                    .collect();
                return Json(serde_json::json!({ "data": {
                    "final_url": final_url.as_str(),
                    "items_seen": items.len(),
                    "sample": sample,
                }}))
                .into_response();
            }
            Err(err) => err.code().to_string(),
        },
        Ok(FetchOutcome::NotModified) => "not_modified".into(),
        Err(err) => err.code(),
    };
    (
        StatusCode::BAD_GATEWAY,
        Json(serde_json::json!({ "ok": false, "error": "preview_failed", "reason": reason })),
    )
        .into_response()
}

pub async fn runs(State(state): State<SharedState>, Path(id): Path<i64>) -> Response {
    let db = state.db.lock().expect("database lock poisoned");
    let result = db
        .conn
        .prepare("SELECT started_at,duration_ms,status,items_seen,items_new,error FROM fetch_runs WHERE source_id=?1 ORDER BY id DESC LIMIT 20")
        .and_then(|mut stmt| {
            stmt.query_map([id], |r| {
                Ok(serde_json::json!({
                    "started_at": rfc3339(r.get(0)?),
                    "duration_ms": r.get::<_, i64>(1)?,
                    "status": r.get::<_, String>(2)?,
                    "items_seen": r.get::<_, i64>(3)?,
                    "items_new": r.get::<_, i64>(4)?,
                    "error": r.get::<_, Option<String>>(5)?,
                }))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()
        });
    match result {
        Ok(data) => Json(serde_json::json!({ "data": data })).into_response(),
        Err(err) => internal_error(err),
    }
}

// ---------- reader endpoints ----------

#[derive(Deserialize)]
pub struct ArticleQuery {
    topic: Option<String>,
    region: Option<String>,
    /// Source slug, so the desktop browse list can narrow the reader to one feed.
    source: Option<String>,
    limit: Option<i64>,
    offset: Option<i64>,
}

#[derive(Serialize)]
pub struct Article {
    pub(crate) id: i64,
    pub(crate) title: String,
    pub(crate) url: String,
    pub(crate) published_at: Option<String>,
    pub(crate) fetched_at: Option<String>,
    pub(crate) topic_slug: Option<String>,
    pub region_code: Option<String>,
    pub(crate) source_name: String,
    pub(crate) source_tier: String,
    /// Human-first labels from `article_matches` (P5). Two readers of the same article
    /// see the same labels because they are read here, not recomputed per request.
    pub labels: Labels,
}

/// `topic` is the article's own topic plus anything a human classified it as; `entities`
/// are the tickers/assets/institutions the rules recognised. A human answer always wins
/// over a rule answer for the same kind, which is what [`crate::classify::save_in`]
/// enforces on write, so this struct does not have to resolve conflicts.
#[derive(Serialize, Default, Clone, Debug, PartialEq)]
pub struct Labels {
    pub topics: Vec<String>,
    pub entities: Vec<String>,
}

/// Newest first. A topic filter includes its descendants and a region filter
/// includes that region's ancestors (`fr` also returns `eu` and `global`). Disabled
/// topics and items outside the effective region set are hidden. `source` narrows to
/// one feed and is applied on top of the same visibility rules, so a source filter can
/// only ever hide items the reader already shows.
pub fn list_articles(
    db: &crate::db::Database,
    topic: Option<&str>,
    region: Option<&str>,
    source: Option<&str>,
    limit: i64,
    offset: i64,
) -> rusqlite::Result<Vec<Article>> {
    let mut stmt = db.conn.prepare(
        "WITH RECURSIVE tree(slug) AS (
           SELECT ?1 WHERE ?1 IS NOT NULL
           UNION SELECT c.slug FROM topics c JOIN tree ON c.parent_slug=tree.slug),
         branch(code) AS (
           SELECT sel.code FROM regions sel WHERE ?2 IS NOT NULL AND sel.code=?2 AND sel.enabled=1
           UNION SELECT r.parent_code FROM regions r JOIN branch ON r.code=branch.code WHERE r.parent_code IS NOT NULL),
         up(code) AS (
           SELECT code FROM regions WHERE enabled=1
           UNION SELECT r.parent_code FROM regions r JOIN up ON r.code=up.code WHERE r.parent_code IS NOT NULL)
         SELECT a.id, a.title, a.canonical_url, a.published_at, a.fetched_at, a.topic_slug, s.name, s.tier, a.region_code
         FROM articles a JOIN sources s ON s.id=a.source_id LEFT JOIN topics t ON t.slug=a.topic_slug
         WHERE (a.topic_slug IS NULL OR t.enabled=1)
           AND (?1 IS NULL OR a.topic_slug IN (SELECT slug FROM tree))
           AND COALESCE(NULLIF(a.region_code,''),'global') IN (SELECT code FROM up)
           AND (?2 IS NULL OR COALESCE(NULLIF(a.region_code,''),'global') IN (SELECT code FROM branch))
           AND (?5 IS NULL OR s.slug=?5)
         ORDER BY COALESCE(a.published_at, strftime('%Y-%m-%dT%H:%M:%SZ', a.fetched_at, 'unixepoch')) DESC, a.id DESC
         LIMIT ?3 OFFSET ?4",
    )?;
    let rows = stmt.query_map(params![topic, region, limit, offset, source], |r| {
        Ok(Article {
            id: r.get(0)?,
            title: r.get(1)?,
            url: r.get(2)?,
            published_at: r.get(3)?,
            fetched_at: r.get::<_, Option<i64>>(4)?.map(rfc3339),
            topic_slug: r.get(5)?,
            region_code: r.get(8)?,
            source_name: r.get(6)?,
            source_tier: r.get(7)?,
            labels: Labels::default(),
        })
    })?;
    let mut items: Vec<Article> = rows.collect::<rusqlite::Result<_>>()?;
    let mut labels = labels_for(db, &items)?;
    for item in &mut items {
        item.labels = labels.remove(&item.id).unwrap_or_default();
    }
    Ok(items)
}

/// Labels for a page of articles in one query, so listing 50 items costs one statement
/// rather than 50. Topics fall back to the article's own `topic_slug` when the
/// classifier stored no row for it.
fn labels_for(
    db: &crate::db::Database,
    items: &[Article],
) -> rusqlite::Result<std::collections::HashMap<i64, Labels>> {
    let ids: Vec<i64> = items.iter().map(|a| a.id).collect();
    let mut out: std::collections::HashMap<i64, Labels> = std::collections::HashMap::new();
    for item in items {
        let mut set = Labels::default();
        if let Some(slug) = &item.topic_slug {
            set.topics.push(slug.clone());
        }
        out.insert(item.id, set);
    }
    if ids.is_empty() {
        return Ok(out);
    }
    let json = serde_json::to_string(&ids).unwrap_or_else(|_| "[]".into());
    let mut stmt = db.conn.prepare(
        "SELECT article_id, kind, value FROM article_matches
          WHERE article_id IN (SELECT value FROM json_each(?1))
            AND kind IN ('topic','entity')
          ORDER BY CASE origin WHEN 'human' THEN 0 WHEN 'llm' THEN 1 ELSE 2 END, value",
    )?;
    let rows = stmt.query_map([json], |r| {
        Ok((
            r.get::<_, i64>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, String>(2)?,
        ))
    })?;
    for row in rows {
        let (id, kind, value) = row?;
        let Some(labels) = out.get_mut(&id) else {
            continue;
        };
        let bucket = if kind == "topic" {
            &mut labels.topics
        } else {
            &mut labels.entities
        };
        if !bucket.contains(&value) {
            bucket.push(value);
        }
    }
    Ok(out)
}

pub async fn articles(
    State(state): State<SharedState>,
    Query(query): Query<ArticleQuery>,
) -> Response {
    let topic = query
        .topic
        .as_deref()
        .map(str::trim)
        .filter(|t| !t.is_empty());
    let limit = query.limit.unwrap_or(50).clamp(1, 100);
    let offset = query.offset.unwrap_or(0).clamp(0, 10_000);
    let region = query
        .region
        .as_deref()
        .map(|code| code.trim().to_ascii_lowercase())
        .filter(|code| !code.is_empty());
    let source = query
        .source
        .as_deref()
        .map(str::trim)
        .filter(|slug| !slug.is_empty());
    let result = {
        let db = state.db.lock().expect("database lock poisoned");
        match region.as_deref() {
            Some(code) if !matches!(regions::region_enabled(&db, code), Ok(true)) => {
                return failure(StatusCode::BAD_REQUEST, "invalid_region");
            }
            _ => list_articles(&db, topic, region.as_deref(), source, limit, offset),
        }
    };
    match result {
        Ok(data) => Json(serde_json::json!({ "data": data, "limit": limit, "offset": offset }))
            .into_response(),
        Err(err) => internal_error(err),
    }
}

#[derive(Serialize, Default)]
pub struct Stats {
    pub collector_enabled: bool,
    pub articles_total: i64,
    pub articles_24h: i64,
    pub sources_enabled: i64,
    pub sources_paused: i64,
    pub topic_counts: HashMap<String, i64>,
}

/// One entry of the desktop sidebar's source list. The count follows the same
/// visibility rules as the reader (enabled topic, effective region set), so a number
/// here is exactly how many items that filter will show.
#[derive(Serialize)]
pub struct SourceFacet {
    pub slug: String,
    pub name: String,
    pub tier: String,
    pub region: String,
    pub articles: i64,
}

pub fn source_facets(
    db: &crate::db::Database,
    region: Option<&str>,
) -> rusqlite::Result<Vec<SourceFacet>> {
    let mut stmt = db.conn.prepare(
        "WITH RECURSIVE up(code) AS (
           SELECT code FROM regions
            WHERE (?1 IS NULL AND enabled=1)
               OR (?1 IS NOT NULL AND code=?1 AND enabled=1)
           UNION SELECT r.parent_code FROM regions r JOIN up ON r.code=up.code WHERE r.parent_code IS NOT NULL)
         SELECT s.slug, s.name, s.tier, COALESCE(NULLIF(s.region,''),'global'),
                COUNT(a.id)
         FROM sources s
         LEFT JOIN articles a ON a.source_id=s.id
          AND (a.topic_slug IS NULL OR a.topic_slug IN (SELECT slug FROM topics WHERE enabled=1))
          AND COALESCE(NULLIF(a.region_code,''),'global') IN (SELECT code FROM up)
         WHERE s.enabled=1
         GROUP BY s.id
         ORDER BY COUNT(a.id) DESC, s.name COLLATE NOCASE",
    )?;
    let rows = stmt.query_map([region], |r| {
        Ok(SourceFacet {
            slug: r.get(0)?,
            name: r.get(1)?,
            tier: r.get(2)?,
            region: r.get(3)?,
            articles: r.get(4)?,
        })
    })?;
    rows.collect()
}

/// Article counters use the same effective region set and topic visibility as
/// `list_articles`, so the dashboard can never report items the reader hides. With
/// `region`, counters follow the selected region and its ancestors, like the list.
/// Source counters stay global on purpose: a source may be enabled while its region
/// is off (the source list warns about it), and hiding it would understate reality.
pub fn stats(db: &crate::db::Database, region: Option<&str>) -> rusqlite::Result<Stats> {
    let since = now() as i64 - 24 * 60 * 60;
    let (articles_total, articles_24h): (i64, i64) = db.conn.query_row(
        "WITH RECURSIVE up(code) AS (
           SELECT code FROM regions
            WHERE (?2 IS NULL AND enabled=1)
               OR (?2 IS NOT NULL AND code=?2 AND enabled=1)
           UNION SELECT r.parent_code FROM regions r JOIN up ON r.code=up.code WHERE r.parent_code IS NOT NULL)
         SELECT COUNT(*), COALESCE(SUM(a.fetched_at >= ?1), 0)
         FROM articles a LEFT JOIN topics t ON t.slug=a.topic_slug
         WHERE (a.topic_slug IS NULL OR t.enabled=1)
           AND COALESCE(NULLIF(a.region_code,''),'global') IN (SELECT code FROM up)",
        params![since, region],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    let (sources_enabled, sources_paused): (i64, i64) = db.conn.query_row(
        "SELECT COALESCE(SUM(enabled=1), 0), COALESCE(SUM(enabled=1 AND paused_reason IS NOT NULL), 0) FROM sources",
        [],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    let mut stmt = db.conn.prepare(
        "WITH RECURSIVE up(code) AS (
           SELECT code FROM regions
            WHERE (?1 IS NULL AND enabled=1)
               OR (?1 IS NOT NULL AND code=?1 AND enabled=1)
           UNION SELECT r.parent_code FROM regions r JOIN up ON r.code=up.code WHERE r.parent_code IS NOT NULL)
         SELECT a.topic_slug, COUNT(*) FROM articles a LEFT JOIN topics t ON t.slug=a.topic_slug
         WHERE a.topic_slug IS NOT NULL AND (t.enabled=1 OR t.slug IS NULL)
           AND COALESCE(NULLIF(a.region_code,''),'global') IN (SELECT code FROM up)
         GROUP BY a.topic_slug",
    )?;
    let topic_counts = stmt
        .query_map([region], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<rusqlite::Result<HashMap<String, i64>>>()?;
    Ok(Stats {
        collector_enabled: collector_enabled(db),
        articles_total,
        articles_24h,
        sources_enabled,
        sources_paused,
        topic_counts,
    })
}
