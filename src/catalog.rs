use crate::{SharedState, db, failure, internal_error};
use axum::{
    Json,
    extract::{Path, State},
    http::{StatusCode, Uri},
    response::{IntoResponse, Response},
};
use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TopicInput {
    slug: String,
    label_vi: String,
    label_en: String,
    parent_slug: Option<String>,
    enabled: bool,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SourceInput {
    slug: String,
    name: String,
    url: String,
    tier: String,
    enabled: bool,
    language: String,
    region: String,
    topic_slug: Option<String>,
    fetch_interval_minutes: i64,
}

#[derive(Debug, Serialize)]
struct Source {
    id: i64,
    slug: String,
    name: String,
    url: String,
    tier: String,
    enabled: bool,
    language: String,
    region: String,
    topic_slug: Option<String>,
    fetch_interval_minutes: i64,
    validators_origin: Option<String>,
    last_checked_at: Option<String>,
    last_success_at: Option<String>,
    last_error: Option<String>,
    consecutive_failures: i64,
    paused_reason: Option<String>,
    next_fetch_at: Option<i64>,
    article_count: i64,
    pack_key: Option<String>,
}

enum CatalogError {
    Invalid(&'static str),
    Conflict(&'static str),
    NotFound,
    Database(rusqlite::Error),
}
impl From<rusqlite::Error> for CatalogError {
    fn from(value: rusqlite::Error) -> Self {
        Self::Database(value)
    }
}
impl CatalogError {
    fn response(self) -> Response {
        match self {
            Self::Invalid(code) => failure(StatusCode::BAD_REQUEST, code),
            Self::Conflict(code) => failure(StatusCode::CONFLICT, code),
            Self::NotFound => failure(StatusCode::NOT_FOUND, "not_found"),
            Self::Database(error) => internal_error(error),
        }
    }
}
type CatalogResult<T> = Result<T, CatalogError>;

fn clean_text(value: &mut String, max: usize, allow_empty: bool) -> bool {
    *value = value.trim().to_string();
    (allow_empty || !value.is_empty())
        && value.chars().count() <= max
        && !value.chars().any(char::is_control)
}

pub(crate) fn valid_slug(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && !value.starts_with('-')
        && !value.ends_with('-')
        && !value.contains("--")
        && value
            .bytes()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'-')
}

fn normalize_optional(value: &mut Option<String>) {
    if let Some(text) = value {
        *text = text.trim().to_string();
        if text.is_empty() {
            *value = None;
        }
    }
}

fn validate_topic(input: &mut TopicInput) -> CatalogResult<()> {
    input.slug = input.slug.trim().to_string();
    normalize_optional(&mut input.parent_slug);
    if !valid_slug(&input.slug)
        || !clean_text(&mut input.label_vi, 120, false)
        || !clean_text(&mut input.label_en, 120, false)
    {
        return Err(CatalogError::Invalid("invalid_topic"));
    }
    Ok(())
}

fn validate_parent(
    db: &db::Database,
    slug: &str,
    parent: Option<&str>,
    require_enabled: bool,
) -> CatalogResult<()> {
    let mut next = parent.map(str::to_string);
    let mut visited = HashSet::new();
    while let Some(current) = next {
        if current == slug || !visited.insert(current.clone()) {
            return Err(CatalogError::Invalid("topic_cycle"));
        }
        let parent_row: Option<(Option<String>, bool)> = db
            .conn
            .query_row(
                "SELECT parent_slug, enabled FROM topics WHERE slug=?1",
                [&current],
                |row| Ok((row.get(0)?, row.get::<_, i64>(1)? != 0)),
            )
            .optional()?;
        let Some((ancestor, enabled)) = parent_row else {
            return Err(CatalogError::Invalid("invalid_parent"));
        };
        if require_enabled && !enabled {
            return Err(CatalogError::Invalid("invalid_parent"));
        }
        next = ancestor;
    }
    Ok(())
}

pub(crate) fn valid_source_url(value: &str) -> bool {
    if value.len() > 2048 || value.contains('#') || value.chars().any(char::is_control) {
        return false;
    }
    let Ok(uri) = value.parse::<Uri>() else {
        return false;
    };
    if !matches!(uri.scheme_str(), Some("http" | "https")) {
        return false;
    }
    let Some(authority) = uri.authority() else {
        return false;
    };
    if authority.host().is_empty() || authority.as_str().contains('@') {
        return false;
    }
    // A future fetcher must still validate resolved IPs and every redirect to prevent SSRF.
    !uri.query().is_some_and(|query| {
        query.split('&').any(|part| {
            let name = part.split('=').next().unwrap_or("").to_ascii_lowercase();
            matches!(
                name.as_str(),
                "key" | "api_key" | "apikey" | "token" | "access_token" | "password" | "secret"
            )
        })
    })
}

fn validate_source(db: &db::Database, input: &mut SourceInput) -> CatalogResult<()> {
    input.slug = input.slug.trim().to_string();
    input.url = input.url.trim().to_string();
    input.tier = input.tier.trim().to_ascii_uppercase();
    input.region = input.region.trim().to_ascii_lowercase();
    if input.region.is_empty() {
        input.region = "global".into();
    }
    normalize_optional(&mut input.topic_slug);
    if !valid_slug(&input.slug)
        || !clean_text(&mut input.name, 120, false)
        || !clean_text(&mut input.language, 16, true)
        || !clean_text(&mut input.region, 80, true)
        || !valid_source_url(&input.url)
        || !matches!(input.tier.as_str(), "T1" | "T2" | "T3" | "T4")
        || !(5..=10_080).contains(&input.fetch_interval_minutes)
    {
        return Err(CatalogError::Invalid("invalid_source"));
    }
    if let Some(topic) = &input.topic_slug {
        let enabled: Option<bool> = db
            .conn
            .query_row("SELECT enabled FROM topics WHERE slug=?1", [topic], |r| {
                Ok(r.get::<_, i64>(0)? != 0)
            })
            .optional()?;
        if enabled.is_none() || (input.enabled && enabled == Some(false)) {
            return Err(CatalogError::Invalid("invalid_topic"));
        }
    }
    // Hand-added sources may target any existing region; enabling one requires that
    // region (and therefore its ancestors) to be enabled.
    let region_enabled: Option<bool> = db
        .conn
        .query_row(
            "SELECT enabled FROM regions WHERE code=?1",
            [&input.region],
            |r| Ok(r.get::<_, i64>(0)? != 0),
        )
        .optional()?;
    if region_enabled.is_none() || (input.enabled && region_enabled == Some(false)) {
        return Err(CatalogError::Invalid("invalid_region"));
    }
    Ok(())
}

fn topic_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<db::Topic> {
    Ok(db::Topic {
        slug: row.get(0)?,
        label_vi: row.get(1)?,
        label_en: row.get(2)?,
        parent_slug: row.get(3)?,
        enabled: row.get::<_, i64>(4)? != 0,
    })
}

fn source_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Source> {
    Ok(Source {
        id: row.get(0)?,
        slug: row.get(1)?,
        name: row.get(2)?,
        url: row.get(3)?,
        tier: row.get(4)?,
        enabled: row.get::<_, i64>(5)? != 0,
        language: row.get(6)?,
        region: row.get(7)?,
        topic_slug: row.get(8)?,
        fetch_interval_minutes: row.get(9)?,
        last_checked_at: row.get(10)?,
        last_success_at: row.get(11)?,
        last_error: row.get(12)?,
        consecutive_failures: row.get(13)?,
        paused_reason: row.get(14)?,
        next_fetch_at: row.get(15)?,
        article_count: row.get(16)?,
        pack_key: row.get(17)?,
        validators_origin: row.get(18)?,
    })
}

fn all_topics(db: &db::Database) -> CatalogResult<Vec<db::Topic>> {
    let mut stmt = db
        .conn
        .prepare("SELECT slug,label_vi,label_en,parent_slug,enabled FROM topics ORDER BY slug")?;
    let rows = stmt.query_map([], topic_row)?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

fn all_sources(db: &db::Database) -> CatalogResult<Vec<Source>> {
    let mut stmt = db.conn.prepare("SELECT id,slug,name,url,tier,enabled,language,region,topic_slug,fetch_interval_minutes,last_checked_at,last_success_at,last_error,consecutive_failures,paused_reason,next_fetch_at,(SELECT COUNT(*) FROM articles a WHERE a.source_id=sources.id),pack_key,validators_origin FROM sources ORDER BY name COLLATE NOCASE, id")?;
    let rows = stmt.query_map([], source_row)?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

fn source_by_id(db: &db::Database, id: i64) -> CatalogResult<Source> {
    db.conn.query_row("SELECT id,slug,name,url,tier,enabled,language,region,topic_slug,fetch_interval_minutes,last_checked_at,last_success_at,last_error,consecutive_failures,paused_reason,next_fetch_at,(SELECT COUNT(*) FROM articles a WHERE a.source_id=sources.id),pack_key,validators_origin FROM sources WHERE id=?1", [id], source_row)
        .optional()?.ok_or(CatalogError::NotFound)
}

fn save_topic(
    db: &db::Database,
    mut input: TopicInput,
    updating: bool,
) -> CatalogResult<TopicInput> {
    validate_topic(&mut input)?;
    let exists: bool = db.conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM topics WHERE slug=?1)",
        [&input.slug],
        |r| r.get(0),
    )?;
    if updating && !exists {
        return Err(CatalogError::NotFound);
    }
    if !updating && exists {
        return Err(CatalogError::Conflict("duplicate_slug"));
    }
    validate_parent(db, &input.slug, input.parent_slug.as_deref(), input.enabled)?;
    if updating && !input.enabled {
        let used: bool = db.conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM topics WHERE parent_slug=?1 AND enabled=1 UNION ALL SELECT 1 FROM sources WHERE topic_slug=?1 AND enabled=1)",
            [&input.slug], |r| r.get(0),
        )?;
        if used {
            return Err(CatalogError::Conflict("topic_in_use"));
        }
    }
    if updating {
        db.conn.execute(
            "UPDATE topics SET label_vi=?2,label_en=?3,parent_slug=?4,enabled=?5 WHERE slug=?1",
            params![
                input.slug,
                input.label_vi,
                input.label_en,
                input.parent_slug,
                input.enabled
            ],
        )?;
    } else {
        db.conn.execute("INSERT INTO topics(slug,label_vi,label_en,parent_slug,enabled) VALUES (?1,?2,?3,?4,?5)", params![input.slug,input.label_vi,input.label_en,input.parent_slug,input.enabled])?;
    }
    Ok(input)
}

fn save_source(
    db: &db::Database,
    mut input: SourceInput,
    id: Option<i64>,
) -> CatalogResult<Source> {
    validate_source(db, &mut input)?;
    if let Some(id) = id {
        let existing = source_by_id(db, id)?;
        // A pack-managed source is content-managed by its file: the UI may only
        // flip `enabled`, so the row cannot drift from the pack.
        if existing.pack_key.is_some()
            && (existing.slug != input.slug
                || existing.name != input.name
                || existing.url != input.url
                || existing.tier != input.tier
                || existing.language != input.language
                || existing.region != input.region
                || existing.topic_slug != input.topic_slug
                || existing.fetch_interval_minutes != input.fetch_interval_minutes)
        {
            return Err(CatalogError::Conflict("source_managed_by_pack"));
        }
    }
    let duplicate: bool = db.conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM sources WHERE (slug=?1 OR url=?2) AND id<>?3)",
        params![input.slug, input.url, id.unwrap_or(-1)],
        |r| r.get(0),
    )?;
    if duplicate {
        return Err(CatalogError::Conflict("duplicate_source"));
    }
    let id = if let Some(id) = id {
        // A new URL means the old validators belong to a different feed; clearing them
        // with their origin also means they can never be replayed to the new host.
        db.conn.execute("UPDATE sources SET slug=?1,name=?2,url=?3,tier=?4,enabled=?5,language=?6,region=?7,topic_slug=?8,fetch_interval_minutes=?9, etag=CASE WHEN url=?3 THEN etag ELSE NULL END, last_modified=CASE WHEN url=?3 THEN last_modified ELSE NULL END, validators_origin=CASE WHEN url=?3 THEN validators_origin ELSE NULL END WHERE id=?10", params![input.slug,input.name,input.url,input.tier,input.enabled,input.language,input.region,input.topic_slug,input.fetch_interval_minutes,id])?;
        id
    } else {
        db.conn.execute("INSERT INTO sources(slug,name,url,tier,enabled,language,region,topic_slug,fetch_interval_minutes) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9)", params![input.slug,input.name,input.url,input.tier,input.enabled,input.language,input.region,input.topic_slug,input.fetch_interval_minutes])?;
        db.conn.last_insert_rowid()
    };
    source_by_id(db, id)
}

pub async fn list_topics(State(state): State<SharedState>) -> Response {
    let result = all_topics(&state.db.lock().expect("database lock poisoned"));
    match result {
        Ok(data) => Json(serde_json::json!({"data":data})).into_response(),
        Err(e) => e.response(),
    }
}
pub async fn create_topic(
    State(state): State<SharedState>,
    Json(input): Json<TopicInput>,
) -> Response {
    let result = save_topic(
        &state.db.lock().expect("database lock poisoned"),
        input,
        false,
    );
    match result {
        Ok(data) => (StatusCode::CREATED, Json(serde_json::json!({"data":data}))).into_response(),
        Err(e) => e.response(),
    }
}
pub async fn update_topic(
    State(state): State<SharedState>,
    Path(slug): Path<String>,
    Json(input): Json<TopicInput>,
) -> Response {
    if slug != input.slug {
        return failure(StatusCode::BAD_REQUEST, "immutable_slug");
    }
    let result = save_topic(
        &state.db.lock().expect("database lock poisoned"),
        input,
        true,
    );
    match result {
        Ok(data) => Json(serde_json::json!({"data":data})).into_response(),
        Err(e) => e.response(),
    }
}
pub async fn list_sources(State(state): State<SharedState>) -> Response {
    let result = all_sources(&state.db.lock().expect("database lock poisoned"));
    match result {
        Ok(data) => Json(serde_json::json!({"data":data})).into_response(),
        Err(e) => e.response(),
    }
}
pub async fn create_source(
    State(state): State<SharedState>,
    Json(input): Json<SourceInput>,
) -> Response {
    let result = save_source(
        &state.db.lock().expect("database lock poisoned"),
        input,
        None,
    );
    match result {
        Ok(data) => (StatusCode::CREATED, Json(serde_json::json!({"data":data}))).into_response(),
        Err(e) => e.response(),
    }
}
pub async fn update_source(
    State(state): State<SharedState>,
    Path(id): Path<i64>,
    Json(input): Json<SourceInput>,
) -> Response {
    let result = save_source(
        &state.db.lock().expect("database lock poisoned"),
        input,
        Some(id),
    );
    match result {
        Ok(data) => Json(serde_json::json!({"data":data})).into_response(),
        Err(e) => e.response(),
    }
}
