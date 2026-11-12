//! Official documents kept as metadata (P2, Federal Register first).
//!
//! Only the identifier, title, effective date and original link are stored, plus a
//! summary we write ourselves. The publisher's abstract is never copied into
//! `summary`; it exists only as an input for a future drafting step owned by a human.

use crate::{SharedState, db, internal_error, regions};
use axum::{
    Json,
    extract::{Query, State},
    response::{IntoResponse, Response},
};
use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};

const MAX_TITLE: usize = 400;
const MAX_SUMMARY: usize = 2000;

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct DocumentRow {
    pub provider: String,
    pub region: String,
    pub doc_type: String,
    pub agency: String,
    pub number: String,
    pub title: String,
    pub effective_on: Option<String>,
    pub published_on: Option<String>,
    pub url: String,
    pub summary: Option<String>,
    /// Topic slug, e.g. `tax`. Empty when not classified.
    #[serde(default)]
    pub topic: String,
    /// The legal/tax disclaimer that travels with `summary`.
    #[serde(default)]
    pub disclaimer: String,
}

/// The only disclaimer allowed on a written summary. It is set by the server, never by
/// a client, so no API consumer can drop it.
pub const SUMMARY_DISCLAIMER: &str = "Không phải tư vấn pháp lý/thuế./Not legal or tax advice.";

/// Folds Vietnamese diacritics so a search for "thue" finds "thuế". This is applied to
/// the indexed copy and to the query, never to the displayed text.
pub fn fold_diacritics(value: &str) -> String {
    value
        .chars()
        .map(|ch| match ch {
            'à' | 'á' | 'ạ' | 'ả' | 'ã' | 'â' | 'ầ' | 'ấ' | 'ậ' | 'ẩ' | 'ẫ' | 'ă' | 'ằ' | 'ắ'
            | 'ặ' | 'ẳ' | 'ẵ' => 'a',
            'À' | 'Á' | 'Ạ' | 'Ả' | 'Ã' | 'Â' | 'Ầ' | 'Ấ' | 'Ậ' | 'Ẩ' | 'Ẫ' | 'Ă' | 'Ằ' | 'Ắ'
            | 'Ặ' | 'Ẳ' | 'Ẵ' => 'a',
            'è' | 'é' | 'ẹ' | 'ẻ' | 'ẽ' | 'ê' | 'ề' | 'ế' | 'ệ' | 'ể' | 'ễ' => {
                'e'
            }
            'È' | 'É' | 'Ẹ' | 'Ẻ' | 'Ẽ' | 'Ê' | 'Ề' | 'Ế' | 'Ệ' | 'Ể' | 'Ễ' => {
                'e'
            }
            'ì' | 'í' | 'ị' | 'ỉ' | 'ĩ' => 'i',
            'Ì' | 'Í' | 'Ị' | 'Ỉ' | 'Ĩ' => 'i',
            'ò' | 'ó' | 'ọ' | 'ỏ' | 'õ' | 'ô' | 'ồ' | 'ố' | 'ộ' | 'ổ' | 'ỗ' | 'ơ' | 'ờ' | 'ớ'
            | 'ợ' | 'ở' | 'ỡ' => 'o',
            'Ò' | 'Ó' | 'Ọ' | 'Ỏ' | 'Õ' | 'Ô' | 'Ồ' | 'Ố' | 'Ộ' | 'Ổ' | 'Ỗ' | 'Ơ' | 'Ờ' | 'Ớ'
            | 'Ợ' | 'Ở' | 'Ỡ' => 'o',
            'ù' | 'ú' | 'ụ' | 'ủ' | 'ũ' | 'ư' | 'ừ' | 'ứ' | 'ự' | 'ử' | 'ữ' => {
                'u'
            }
            'Ù' | 'Ú' | 'Ụ' | 'Ủ' | 'Ũ' | 'Ư' | 'Ừ' | 'Ứ' | 'Ự' | 'Ử' | 'Ữ' => {
                'u'
            }
            'ỳ' | 'ý' | 'ỵ' | 'ỷ' | 'ỹ' => 'y',
            'Ỳ' | 'Ý' | 'Ỵ' | 'Ỷ' | 'Ỹ' => 'y',
            'đ' => 'd',
            'Đ' => 'd',
            other => other,
        })
        .collect()
}

fn clean(value: &str, max: usize) -> bool {
    !value.trim().is_empty() && value.chars().count() <= max && !value.chars().any(char::is_control)
}

/// Insert or refresh a document without touching a summary a human already wrote.
/// Returns true when a row was inserted.
pub fn upsert(db: &db::Database, doc: &DocumentRow) -> anyhow::Result<bool> {
    if !clean(&doc.provider, 64)
        || !clean(&doc.number, 128)
        || !clean(&doc.title, MAX_TITLE)
        || !clean(&doc.url, 2048)
        || !doc.url.trim().starts_with("https://") && !doc.url.trim().starts_with("http://")
    {
        anyhow::bail!("invalid_document");
    }
    if doc
        .summary
        .as_deref()
        .is_some_and(|summary| summary.chars().count() > MAX_SUMMARY)
    {
        anyhow::bail!("invalid_summary");
    }
    let exists: bool = db.conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM regions WHERE code=?1)",
        [&doc.region],
        |r| r.get(0),
    )?;
    if !exists {
        anyhow::bail!("invalid_region");
    }
    let existed: bool = db.conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM documents WHERE provider=?1 AND number=?2)",
        params![doc.provider, doc.number],
        |r| r.get(0),
    )?;
    db.conn.execute(
        "INSERT INTO documents(provider,region,doc_type,agency,number,title,effective_on,published_on,url,summary,topic,disclaimer)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12)
         ON CONFLICT(provider,number) DO UPDATE SET
           region=excluded.region, doc_type=excluded.doc_type, agency=excluded.agency,
           title=excluded.title, effective_on=excluded.effective_on,
           published_on=excluded.published_on, url=excluded.url,
           summary=COALESCE(documents.summary, excluded.summary),
           topic=CASE WHEN excluded.topic='' THEN documents.topic ELSE excluded.topic END,
           disclaimer=CASE WHEN documents.summary IS NULL THEN excluded.disclaimer ELSE documents.disclaimer END,
           updated_at=CURRENT_TIMESTAMP",
        params![
            doc.provider,
            doc.region,
            doc.doc_type,
            doc.agency,
            doc.number,
            doc.title,
            doc.effective_on,
            doc.published_on,
            doc.url,
            doc.summary,
            doc.topic,
            if doc.summary.is_some() {
                SUMMARY_DISCLAIMER
            } else {
                ""
            }
        ],
    )?;
    // Keep the search index in step. The folded copy is what makes "thue" match "thuế".
    reindex(db, doc)?;
    Ok(!existed)
}

/// Rebuilds the search row for one document. Cheap enough to run on every upsert.
fn reindex(db: &db::Database, doc: &DocumentRow) -> anyhow::Result<()> {
    db.conn.execute(
        "DELETE FROM documents_fts WHERE provider=?1 AND number=?2",
        params![doc.provider, doc.number],
    )?;
    db.conn.execute(
        "INSERT INTO documents_fts(provider,number,title,agency,summary) VALUES (?1,?2,?3,?4,?5)",
        params![
            doc.provider,
            doc.number,
            fold_diacritics(&doc.title),
            fold_diacritics(&doc.agency),
            fold_diacritics(doc.summary.as_deref().unwrap_or(""))
        ],
    )?;
    Ok(())
}

/// Search by folded text. Returns provider/number pairs; the caller joins them back so
/// the result carries the region filter and the disclaimer unchanged.
pub fn search(
    db: &db::Database,
    query: &str,
    limit: i64,
) -> rusqlite::Result<Vec<(String, String)>> {
    let folded = fold_diacritics(query.trim());
    // FTS5 treats most punctuation as syntax; keep letters, digits and spaces only and
    // quote each token so a stray operator cannot change the meaning of the query.
    let tokens: Vec<String> = folded
        .split(|ch: char| !ch.is_alphanumeric())
        .filter(|token| !token.is_empty())
        .map(|token| format!("\"{token}\"*"))
        .collect();
    if tokens.is_empty() {
        return Ok(Vec::new());
    }
    let expression = tokens.join(" AND ");
    let mut stmt = db.conn.prepare(
        "SELECT provider,number FROM documents_fts WHERE documents_fts MATCH ?1 LIMIT ?2",
    )?;
    let rows = stmt.query_map(params![expression, limit], |r| Ok((r.get(0)?, r.get(1)?)))?;
    rows.collect()
}

#[cfg_attr(not(test), allow(dead_code))]
pub fn count(db: &db::Database, provider: &str) -> rusqlite::Result<i64> {
    db.conn.query_row(
        "SELECT COUNT(*) FROM documents WHERE provider=?1",
        [provider],
        |r| r.get(0),
    )
}

#[cfg_attr(not(test), allow(dead_code))]
pub fn get(
    db: &db::Database,
    provider: &str,
    number: &str,
) -> rusqlite::Result<Option<DocumentRow>> {
    db.conn
        .query_row(
            "SELECT provider,region,doc_type,agency,number,title,effective_on,published_on,url,summary,topic,disclaimer
             FROM documents WHERE provider=?1 AND number=?2",
            params![provider, number],
            |r| {
                Ok(DocumentRow {
                    provider: r.get(0)?,
                    region: r.get(1)?,
                    doc_type: r.get(2)?,
                    agency: r.get(3)?,
                    number: r.get(4)?,
                    title: r.get(5)?,
                    effective_on: r.get(6)?,
                    published_on: r.get(7)?,
                    url: r.get(8)?,
                    summary: r.get(9)?,
                    topic: r.get(10)?,
                    disclaimer: r.get(11)?,
                })
            },
        )
        .optional()
}

#[derive(Deserialize)]
pub struct DocumentQuery {
    region: Option<String>,
    limit: Option<i64>,
    /// Free text, matched with Vietnamese diacritics folded ("thue" finds "thuế").
    q: Option<String>,
    topic: Option<String>,
}

/// Reader API: documents in the effective region set only.
pub async fn list_documents(
    State(state): State<SharedState>,
    Query(q): Query<DocumentQuery>,
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
        return crate::failure(axum::http::StatusCode::BAD_REQUEST, "invalid_region");
    }
    let effective = match regions::effective(&db, region.as_deref()) {
        Ok(codes) => codes,
        Err(err) => return internal_error(err),
    };
    let query =
        q.q.as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty());
    let topic = q
        .topic
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    match list(&db, region.as_deref(), topic, query, limit) {
        Ok(data) => {
            Json(serde_json::json!({ "data": data, "effective": effective })).into_response()
        }
        Err(err) => internal_error(err),
    }
}

/// The listing both `/api/v1/documents` and the MCP tool use. `q` runs on the folded
/// FTS index first; hits are then filtered by the same region/topic rules, so a search
/// can never leak a row a plain listing would hide.
pub fn list(
    db: &db::Database,
    region: Option<&str>,
    topic: Option<&str>,
    query: Option<&str>,
    limit: i64,
) -> anyhow::Result<Vec<DocumentRow>> {
    let effective = regions::effective(db, region)?;
    (|| -> anyhow::Result<Vec<DocumentRow>> {
        // Search runs on the folded FTS index first; the identifiers it returns are then
        // filtered by the same region/topic rules as a plain listing, so a hidden region
        // can never leak through a search hit.
        let matched_keys: Vec<String> = match query {
            Some(text) => search(db, text, limit.saturating_mul(4).max(50))?
                .into_iter()
                .map(|(provider, number)| format!("{provider}\u{1}{number}"))
                .collect(),
            None => Vec::new(),
        };
        let mut stmt = db.conn.prepare(
            "SELECT provider,region,doc_type,agency,number,title,effective_on,published_on,url,summary,topic,disclaimer
             FROM documents
             WHERE region IN (SELECT value FROM json_each(?1))
               AND (?2 IS NULL OR topic=?2)
               AND (?3=0 OR (provider || char(31) || number) IN (SELECT value FROM json_each(?4)))
             ORDER BY COALESCE(effective_on, published_on) DESC, id DESC LIMIT ?5",
        )?;
        let rows = stmt.query_map(
            params![
                serde_json::to_string(&effective).unwrap_or_else(|_| "[]".into()),
                topic,
                i64::from(query.is_some()),
                serde_json::to_string(&matched_keys).unwrap_or_else(|_| "[]".into()),
                limit
            ],
            |r| {
                Ok(DocumentRow {
                    provider: r.get(0)?,
                    region: r.get(1)?,
                    doc_type: r.get(2)?,
                    agency: r.get(3)?,
                    number: r.get(4)?,
                    title: r.get(5)?,
                    effective_on: r.get(6)?,
                    published_on: r.get(7)?,
                    url: r.get(8)?,
                    summary: r.get(9)?,
                    topic: r.get(10)?,
                    disclaimer: r.get(11)?,
                })
            },
        )?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    })()
}

#[cfg(test)]
#[path = "documents_tests.rs"]
mod tests;
