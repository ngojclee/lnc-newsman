//! The write surface a delegated Hub agent may use (P8, `newsman:write`).
//!
//! Scope, deliberately: an agent may add **notes on a stored article or document**, write
//! an **analysis/brief** that cites stored article ids, and **flag an article**. It may
//! not touch an article, a source, a region, a connector, a secret or alert delivery —
//! those stay on `/admin`, behind a signed-in owner session. The boundary is enforced by
//! *shape* (every tool below writes to one of the three tables migration 18 adds) rather
//! than by a list of forbidden operations, so adding a tool means adding a table.
//!
//! Three rules hold for every tool here:
//!
//! * **The author is the verified caller.** `author_client` comes from the delegation
//!   the boundary already checked, never from a tool argument. `additionalProperties:
//!   false` in the schema is only a note to the client, so `crate::mcp` refuses an
//!   undeclared key itself — otherwise `{"author": "louis"}` would be silently dropped
//!   and the agent would believe it had set an author.
//! * **Every write is logged** with the client id, the tool and the record id, in a
//!   table no tool can write to except this path.
//! * **Article content is data, never instruction.** Text stored by a feed is quoted
//!   back into a summary and nothing else; nothing here interprets it.
//!
//! Ids are always resolved against what the app can already show, so a write cannot
//! reference a row the reader cannot see (a disabled region, a hidden article).

use crate::connector::Caller;
use crate::{SharedState, db};
use rusqlite::{OptionalExtension, params};
use serde_json::{Value, json};

/// Longest note body. A note is a comment on an item, not a place to paste an article.
pub const MAX_NOTE_CHARS: usize = 4_000;
/// Longest analysis body. Long enough for a real brief, short enough to stay a summary.
pub const MAX_ANALYSIS_CHARS: usize = 20_000;
/// Most article ids an analysis may cite in one write.
pub const MAX_SOURCES: usize = 200;
pub const MAX_TITLE_CHARS: usize = 300;
pub const MAX_FLAG_CHARS: usize = 60;

/// A refusal that is the caller's fault. Shown as a tool error, never a transport one.
#[derive(Debug)]
pub struct WriteError(pub &'static str);

impl WriteError {
    fn new(code: &'static str) -> Self {
        Self(code)
    }
}

impl std::fmt::Display for WriteError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.0)
    }
}

type Result<T> = std::result::Result<T, WriteError>;

fn text(arguments: &Value, key: &str) -> Option<String> {
    arguments
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn required_text(arguments: &Value, key: &str, max: usize) -> Result<String> {
    let value = text(arguments, key).ok_or(WriteError::new("missing_field"))?;
    if value.chars().count() > max {
        return Err(WriteError::new("too_long"));
    }
    Ok(value)
}

fn id(arguments: &Value, key: &str) -> Result<i64> {
    arguments
        .get(key)
        .and_then(Value::as_i64)
        .filter(|value| *value > 0)
        .ok_or(WriteError::new("invalid_id"))
}

fn optional_id(arguments: &Value, key: &str) -> Result<Option<i64>> {
    match arguments.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(_) => id(arguments, key).map(Some),
    }
}

/// The cited article ids: a JSON array of positive integers, bounded, at most
/// [`MAX_SOURCES`] entries. Anything else is refused rather than silently dropped,
/// because a citation list that quietly lost an entry would make a summary unreliable.
fn source_ids(arguments: &Value) -> Result<Vec<i64>> {
    let Some(value) = arguments.get("source_article_ids") else {
        return Ok(Vec::new());
    };
    let Some(items) = value.as_array() else {
        return Err(WriteError::new("invalid_sources"));
    };
    if items.len() > MAX_SOURCES {
        return Err(WriteError::new("too_many_sources"));
    }
    let mut ids = Vec::with_capacity(items.len());
    for item in items {
        let id = item
            .as_i64()
            .filter(|value| *value > 0)
            .ok_or(WriteError::new("invalid_sources"))?;
        if !ids.contains(&id) {
            ids.push(id);
        }
    }
    Ok(ids)
}

fn article_exists(db: &db::Database, id: i64) -> Result<bool> {
    db.conn
        .query_row("SELECT 1 FROM articles WHERE id=?1", [id], |_| Ok(true))
        .optional()
        .map(|found| found.unwrap_or(false))
        .map_err(|_| WriteError::new("storage_error"))
}

fn document_exists(db: &db::Database, id: i64) -> Result<bool> {
    db.conn
        .query_row("SELECT 1 FROM documents WHERE id=?1", [id], |_| Ok(true))
        .optional()
        .map(|found| found.unwrap_or(false))
        .map_err(|_| WriteError::new("storage_error"))
}

/// Which record a tool touched, for the audit row.
fn log_write(
    db: &db::Database,
    caller: &Caller,
    tool: &str,
    record_kind: &str,
    record_id: Option<i64>,
    note: Option<&str>,
) {
    // A failed log write must not fail the write it describes, but it must be visible.
    if let Err(err) = db.conn.execute(
        "INSERT INTO newsman_write_log(at,client_id,app_id,tool,record_kind,record_id,outcome,note)
         VALUES (?1,?2,?3,?4,?5,?6,'ok',?7)",
        params![
            crate::now() as i64,
            caller.client_id,
            caller.app_id,
            tool,
            record_kind,
            record_id,
            note
        ],
    ) {
        tracing::error!(%err, tool, "write audit row could not be stored");
    }
}

// ---------- notes ----------

pub fn note_create(
    state: &SharedState,
    caller: &Caller,
    arguments: &Value,
) -> std::result::Result<Value, String> {
    let body = required_text(arguments, "body", MAX_NOTE_CHARS).map_err(|e| e.0.to_string())?;
    let article_id = optional_id(arguments, "article_id").map_err(|e| e.0.to_string())?;
    let document_id = optional_id(arguments, "document_id").map_err(|e| e.0.to_string())?;
    // Exactly one target: the CHECK constraint would reject both-or-neither anyway, but
    // saying so here gives the caller a usable message instead of a storage error.
    if article_id.is_some() == document_id.is_some() {
        return Err("provide exactly one of `article_id` or `document_id`".into());
    }
    let db = state.db.lock().map_err(|_| "database lock poisoned".to_string())?;
    if let Some(id) = article_id
        && !article_exists(&db, id).map_err(|e| e.0.to_string())?
    {
        return Err(format!("no article `{id}`"));
    }
    if let Some(id) = document_id
        && !document_exists(&db, id).map_err(|e| e.0.to_string())?
    {
        return Err(format!("no document `{id}`"));
    }
    let now = crate::now() as i64;
    db.conn
        .execute(
            "INSERT INTO newsman_notes(article_id,document_id,body,author_client,created_at,updated_at)
             VALUES (?1,?2,?3,?4,?5,?5)",
            params![article_id, document_id, body, caller.client_id, now],
        )
        .map_err(|err| err.to_string())?;
    let id = db.conn.last_insert_rowid();
    log_write(&db, caller, "note_create", "note", Some(id), None);
    Ok(json!({"id": id, "created": true}))
}

/// Update is scoped to the author's own note. A note the owner wrote in the web UI
/// (`author_client` NULL) is not editable through this boundary at all, so an agent
/// cannot rewrite the owner's words even if it guesses the id.
pub fn note_update(
    state: &SharedState,
    caller: &Caller,
    arguments: &Value,
) -> std::result::Result<Value, String> {
    let id = id(arguments, "id").map_err(|e| e.0.to_string())?;
    let body = required_text(arguments, "body", MAX_NOTE_CHARS).map_err(|e| e.0.to_string())?;
    let db = state.db.lock().map_err(|_| "database lock poisoned".to_string())?;
    let owner: Option<Option<String>> = db
        .conn
        .query_row(
            "SELECT author_client FROM newsman_notes WHERE id=?1",
            [id],
            |row| row.get(0),
        )
        .optional()
        .map_err(|err| err.to_string())?;
    match owner {
        None => return Err(format!("no note `{id}`")),
        Some(None) => return Err("that note was written by the owner and cannot be changed".into()),
        Some(Some(author)) if author != caller.client_id => {
            return Err("that note belongs to another caller".into());
        }
        Some(Some(_)) => {}
    }
    db.conn
        .execute(
            "UPDATE newsman_notes SET body=?2, updated_at=?3 WHERE id=?1",
            params![id, body, crate::now() as i64],
        )
        .map_err(|err| err.to_string())?;
    log_write(&db, caller, "note_update", "note", Some(id), None);
    Ok(json!({"id": id, "updated": true}))
}

pub fn note_delete(
    state: &SharedState,
    caller: &Caller,
    arguments: &Value,
) -> std::result::Result<Value, String> {
    let id = id(arguments, "id").map_err(|e| e.0.to_string())?;
    let db = state.db.lock().map_err(|_| "database lock poisoned".to_string())?;
    let owner: Option<Option<String>> = db
        .conn
        .query_row(
            "SELECT author_client FROM newsman_notes WHERE id=?1",
            [id],
            |row| row.get(0),
        )
        .optional()
        .map_err(|err| err.to_string())?;
    match owner {
        None => return Err(format!("no note `{id}`")),
        Some(None) => return Err("that note was written by the owner and cannot be deleted".into()),
        Some(Some(author)) if author != caller.client_id => {
            return Err("that note belongs to another caller".into());
        }
        Some(Some(_)) => {}
    }
    db.conn
        .execute("DELETE FROM newsman_notes WHERE id=?1", [id])
        .map_err(|err| err.to_string())?;
    log_write(&db, caller, "note_delete", "note", Some(id), None);
    Ok(json!({"id": id, "deleted": true}))
}

/// Notes on one item, newest first. Read side of the note tools, open to `newsman:read`
/// because a note is commentary the reader is meant to see next to the article.
pub fn notes_for(
    db: &db::Database,
    article_id: Option<i64>,
    document_id: Option<i64>,
    limit: i64,
) -> anyhow::Result<Vec<Value>> {
    let mut stmt = db.conn.prepare(
        "SELECT id,article_id,document_id,body,author_client,created_at,updated_at
         FROM newsman_notes
         WHERE (?1 IS NOT NULL AND article_id=?1) OR (?2 IS NOT NULL AND document_id=?2)
         ORDER BY created_at DESC, id DESC LIMIT ?3",
    )?;
    let rows = stmt
        .query_map(params![article_id, document_id, limit], |row| {
            Ok(json!({
                "id": row.get::<_, i64>(0)?,
                "article_id": row.get::<_, Option<i64>>(1)?,
                "document_id": row.get::<_, Option<i64>>(2)?,
                "body": row.get::<_, String>(3)?,
                "author": row.get::<_, Option<String>>(4)?,
                "created_at": row.get::<_, i64>(5)?,
                "updated_at": row.get::<_, i64>(6)?,
            }))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

// ---------- analyses ----------

pub fn analysis_create(
    state: &SharedState,
    caller: &Caller,
    arguments: &Value,
) -> std::result::Result<Value, String> {
    let title = required_text(arguments, "title", MAX_TITLE_CHARS).map_err(|e| e.0.to_string())?;
    let body =
        required_text(arguments, "body", MAX_ANALYSIS_CHARS).map_err(|e| e.0.to_string())?;
    let kind = match text(arguments, "kind").as_deref() {
        None | Some("brief") => "brief",
        Some("summary") => "summary",
        Some(_) => return Err("`kind` must be brief or summary".into()),
    };
    let topic = text(arguments, "topic_slug");
    let region = text(arguments, "region_code");
    let sources = source_ids(arguments).map_err(|e| e.0.to_string())?;
    let db = state.db.lock().map_err(|_| "database lock poisoned".to_string())?;
    // A region or topic the reader cannot see must not appear on a brief either.
    if let Some(code) = region.as_deref()
        && !matches!(crate::regions::region_enabled(&db, code), Ok(true))
    {
        return Err(format!("region `{code}` is not enabled"));
    }
    if let Some(slug) = topic.as_deref() {
        let known: Option<i64> = db
            .conn
            .query_row(
                "SELECT 1 FROM topics WHERE slug=?1 AND enabled=1",
                [slug],
                |row| row.get(0),
            )
            .optional()
            .map_err(|err| err.to_string())?;
        if known.is_none() {
            return Err(format!("topic `{slug}` is not enabled"));
        }
    }
    // Every citation must resolve to a stored article, so a reader can click through.
    for id in &sources {
        if !article_exists(&db, *id).map_err(|e| e.0.to_string())? {
            return Err(format!("no article `{id}` to cite"));
        }
    }
    let now = crate::now() as i64;
    db.conn
        .execute(
            "INSERT INTO newsman_analyses(kind,title,topic_slug,region_code,body,source_article_ids,author_client,created_at,updated_at)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?8)",
            params![
                kind,
                title,
                topic,
                region,
                body,
                serde_json::to_string(&sources).unwrap_or_else(|_| "[]".into()),
                caller.client_id,
                now
            ],
        )
        .map_err(|err| err.to_string())?;
    let id = db.conn.last_insert_rowid();
    log_write(&db, caller, "analysis_create", "analysis", Some(id), None);
    Ok(json!({"id": id, "created": true, "sources": sources.len()}))
}

pub fn analysis_update(
    state: &SharedState,
    caller: &Caller,
    arguments: &Value,
) -> std::result::Result<Value, String> {
    let id = id(arguments, "id").map_err(|e| e.0.to_string())?;
    let db = state.db.lock().map_err(|_| "database lock poisoned".to_string())?;
    let owner: Option<Option<String>> = db
        .conn
        .query_row(
            "SELECT author_client FROM newsman_analyses WHERE id=?1",
            [id],
            |row| row.get(0),
        )
        .optional()
        .map_err(|err| err.to_string())?;
    match owner {
        None => return Err(format!("no analysis `{id}`")),
        Some(None) => return Err("that analysis was written by the owner".into()),
        Some(Some(author)) if author != caller.client_id => {
            return Err("that analysis belongs to another caller".into());
        }
        Some(Some(_)) => {}
    }
    // Replace whatever was supplied; an omitted field keeps its stored value, so an
    // agent can append a correction without resending the whole brief.
    let mut sets: Vec<String> = Vec::new();
    let mut values: Vec<rusqlite::types::Value> = Vec::new();
    if let Some(title) = arguments.get("title").and_then(Value::as_str) {
        let title = title.trim();
        if title.is_empty() || title.chars().count() > MAX_TITLE_CHARS {
            return Err("`title` must be 1..=300 characters".into());
        }
        sets.push(format!("title=?{}", sets.len() + 2));
        values.push(title.to_string().into());
    }
    if let Some(body) = arguments.get("body").and_then(Value::as_str) {
        let body = body.trim();
        if body.is_empty() || body.chars().count() > MAX_ANALYSIS_CHARS {
            return Err("`body` must be 1..=20000 characters".into());
        }
        sets.push(format!("body=?{}", sets.len() + 2));
        values.push(body.to_string().into());
    }
    if arguments.get("source_article_ids").is_some() {
        let sources = source_ids(arguments).map_err(|e| e.0.to_string())?;
        for source in &sources {
            if !article_exists(&db, *source).map_err(|e| e.0.to_string())? {
                return Err(format!("no article `{source}` to cite"));
            }
        }
        sets.push(format!("source_article_ids=?{}", sets.len() + 2));
        values.push(serde_json::to_string(&sources).unwrap_or_else(|_| "[]".into()).into());
    }
    if sets.is_empty() {
        return Err("nothing to update".into());
    }
    sets.push(format!("updated_at=?{}", sets.len() + 2));
    values.push((crate::now() as i64).into());
    let sql = format!("UPDATE newsman_analyses SET {} WHERE id=?1", sets.join(", "));
    let mut bound: Vec<&dyn rusqlite::ToSql> = vec![&id];
    for value in &values {
        bound.push(value);
    }
    db.conn
        .execute(&sql, bound.as_slice())
        .map_err(|err| err.to_string())?;
    log_write(&db, caller, "analysis_update", "analysis", Some(id), None);
    Ok(json!({"id": id, "updated": true}))
}

/// Stored analyses, newest first, with the author label the UI shows.
pub fn analysis_list(
    db: &db::Database,
    topic: Option<&str>,
    region: Option<&str>,
    limit: i64,
) -> anyhow::Result<Vec<Value>> {
    let mut stmt = db.conn.prepare(
        "SELECT id,kind,title,topic_slug,region_code,body,source_article_ids,author_client,created_at,updated_at
         FROM newsman_analyses
         WHERE (?1 IS NULL OR topic_slug=?1) AND (?2 IS NULL OR region_code=?2)
         ORDER BY updated_at DESC, id DESC LIMIT ?3",
    )?;
    let rows = stmt
        .query_map(params![topic, region, limit], |row| {
            let cited: String = row.get(6)?;
            Ok(json!({
                "id": row.get::<_, i64>(0)?,
                "kind": row.get::<_, String>(1)?,
                "title": row.get::<_, String>(2)?,
                "topic_slug": row.get::<_, Option<String>>(3)?,
                "region_code": row.get::<_, Option<String>>(4)?,
                "body": row.get::<_, String>(5)?,
                "source_article_ids": serde_json::from_str::<Value>(&cited).unwrap_or(json!([])),
                "author": row.get::<_, Option<String>>(7)?,
                "created_at": row.get::<_, i64>(8)?,
                "updated_at": row.get::<_, i64>(9)?,
            }))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

// ---------- article flags ----------

pub fn article_flag(
    state: &SharedState,
    caller: &Caller,
    arguments: &Value,
) -> std::result::Result<Value, String> {
    let article_id = id(arguments, "article_id").map_err(|e| e.0.to_string())?;
    let label = required_text(arguments, "label", MAX_FLAG_CHARS).map_err(|e| e.0.to_string())?;
    let db = state.db.lock().map_err(|_| "database lock poisoned".to_string())?;
    if !article_exists(&db, article_id).map_err(|e| e.0.to_string())? {
        return Err(format!("no article `{article_id}`"));
    }
    // Idempotent: flagging twice is the same fact, not two rows.
    let inserted = db
        .conn
        .execute(
            "INSERT INTO newsman_article_flags(article_id,label,author_client,created_at)
             VALUES (?1,?2,?3,?4)
             ON CONFLICT(article_id,label) DO NOTHING",
            params![article_id, label, caller.client_id, crate::now() as i64],
        )
        .map_err(|err| err.to_string())?;
    log_write(
        &db,
        caller,
        "article_flag",
        "article",
        Some(article_id),
        Some(&label),
    );
    Ok(json!({"article_id": article_id, "label": label, "created": inserted > 0}))
}

pub fn flags_for(db: &db::Database, article_id: i64) -> anyhow::Result<Vec<Value>> {
    let mut stmt = db.conn.prepare(
        "SELECT label,author_client,created_at FROM newsman_article_flags
         WHERE article_id=?1 ORDER BY created_at",
    )?;
    let rows = stmt
        .query_map([article_id], |row| {
            Ok(json!({
                "label": row.get::<_, String>(0)?,
                "author": row.get::<_, Option<String>>(1)?,
                "created_at": row.get::<_, i64>(2)?,
            }))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

/// The audit trail, newest first. Read-only here: no tool inserts into it except
/// [`log_write`], so what the owner sees is exactly what happened.
pub fn write_log(db: &db::Database, limit: i64) -> anyhow::Result<Vec<Value>> {
    let mut stmt = db.conn.prepare(
        "SELECT id,at,client_id,app_id,tool,record_kind,record_id,outcome,note
         FROM newsman_write_log ORDER BY id DESC LIMIT ?1",
    )?;
    let rows = stmt
        .query_map([limit], |row| {
            Ok(json!({
                "id": row.get::<_, i64>(0)?,
                "at": row.get::<_, i64>(1)?,
                "client_id": row.get::<_, Option<String>>(2)?,
                "app_id": row.get::<_, Option<String>>(3)?,
                "tool": row.get::<_, String>(4)?,
                "record_kind": row.get::<_, String>(5)?,
                "record_id": row.get::<_, Option<i64>>(6)?,
                "outcome": row.get::<_, String>(7)?,
                "note": row.get::<_, Option<String>>(8)?,
            }))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

#[cfg(test)]
#[path = "write_tests.rs"]
mod tests;
