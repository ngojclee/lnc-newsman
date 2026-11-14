use anyhow::{Context, Result};
use rusqlite::{Connection, OptionalExtension, params};
use serde::Serialize;
use std::path::Path;

pub struct Database {
    pub(crate) conn: Connection,
}

#[derive(Debug, Serialize)]
pub struct Topic {
    pub slug: String,
    pub label_vi: String,
    pub label_en: String,
    pub parent_slug: Option<String>,
    pub enabled: bool,
}

impl Database {
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("create data dir {}", parent.display()))?;
        }
        let conn =
            Connection::open(path).with_context(|| format!("open database {}", path.display()))?;
        Self::initialize(conn)
    }

    #[cfg(test)]
    pub fn in_memory() -> Result<Self> {
        Self::initialize(Connection::open_in_memory()?)
    }

    fn initialize(mut conn: Connection) -> Result<Self> {
        conn.execute_batch(include_str!("../migrations/0001_init.sql"))
            .context("run initial migration")?;
        conn.execute_batch(include_str!("../migrations/0002_sessions.sql"))
            .context("run session migration")?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS schema_migrations (version INTEGER PRIMARY KEY, applied_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP)",
        )?;
        for (version, sql) in [
            (3, include_str!("../migrations/0003_source_catalog.sql")),
            (4, include_str!("../migrations/0004_collector.sql")),
            (5, include_str!("../migrations/0005_regions.sql")),
            (6, include_str!("../migrations/0006_validator_origin.sql")),
            (7, include_str!("../migrations/0007_secrets.sql")),
            (8, include_str!("../migrations/0008_series.sql")),
            (
                9,
                include_str!("../migrations/0009_provider_connectors.sql"),
            ),
            (
                10,
                include_str!("../migrations/0010_connector_requests.sql"),
            ),
            (
                11,
                include_str!("../migrations/0011_source_attribution.sql"),
            ),
            (12, include_str!("../migrations/0012_official_tax.sql")),
            (13, include_str!("../migrations/0013_classifier_llm.sql")),
            (14, include_str!("../migrations/0014_filings.sql")),
            (15, include_str!("../migrations/0015_alerts.sql")),
            (
                16,
                include_str!("../migrations/0016_alert_telegram.sql"),
            ),
            (17, include_str!("../migrations/0017_legal_sources.sql")),
            (18, include_str!("../migrations/0018_agent_write.sql")),
            (19, include_str!("../migrations/0019_note_analysis.sql")),
            (20, include_str!("../migrations/0020_article_content.sql")),
            (21, crate::agent_bridge::MIGRATION_0021_SQL),
            (22, crate::agent_bridge::MIGRATION_0022_SQL),
            (23, crate::agent_bridge::MIGRATION_0023_SQL),
            (24, crate::agent_bridge::MIGRATION_0024_SQL),
        ] {
            let applied: bool = conn.query_row(
                "SELECT EXISTS(SELECT 1 FROM schema_migrations WHERE version = ?1)",
                [version],
                |row| row.get(0),
            )?;
            if !applied {
                let tx = conn.transaction()?;
                tx.execute_batch(sql)
                    .with_context(|| format!("run migration {version}"))?;
                tx.execute(
                    "INSERT INTO schema_migrations(version) VALUES (?1)",
                    [version],
                )?;
                tx.commit()?;
            }
        }
        let db = Self { conn };
        for (slug, vi, en) in [
            ("finance", "Tài chính", "Finance"),
            ("technology-ai", "Công nghệ & AI", "Technology & AI"),
            ("politics", "Chính trị & chính sách", "Politics & Policy"),
            ("ecommerce", "Thương mại điện tử", "E-commerce"),
            ("world", "Thế giới", "World"),
        ] {
            db.conn.execute(
                "INSERT OR IGNORE INTO topics (slug,label_vi,label_en,enabled) VALUES (?1,?2,?3,1)",
                params![slug, vi, en],
            )?;
        }
        crate::agent_bridge::seed_defaults(&db.conn)?;
        db.sync_missing_articles_fts()?;
        Ok(db)
    }

    /// Backfills `articles_fts` for any rows present in `articles` that are not yet in the
    /// folded FTS5 index (for example, rows stored before migration 20).
    pub fn sync_missing_articles_fts(&self) -> Result<()> {
        let mut stmt = self.conn.prepare(
            "SELECT a.id, a.title, a.summary, a.content_text
             FROM articles a
             WHERE NOT EXISTS (SELECT 1 FROM articles_fts f WHERE CAST(f.article_id AS INTEGER) = a.id)",
        )?;
        let missing: Vec<(i64, String, Option<String>, Option<String>)> = stmt
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?
            .collect::<rusqlite::Result<_>>()?;
        for (id, title, summary, content_text) in missing {
            index_article_fts(
                &self.conn,
                id,
                &title,
                summary.as_deref(),
                content_text.as_deref(),
            )?;
        }
        Ok(())
    }

    pub fn topics(&self) -> Result<Vec<Topic>> {
        let mut stmt = self.conn.prepare("SELECT slug,label_vi,label_en,parent_slug,enabled FROM topics WHERE enabled=1 ORDER BY slug")?;
        let rows = stmt.query_map([], |r| {
            Ok(Topic {
                slug: r.get(0)?,
                label_vi: r.get(1)?,
                label_en: r.get(2)?,
                parent_slug: r.get(3)?,
                enabled: r.get::<_, i64>(4)? != 0,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    pub fn get_setting(&self, key: &str) -> Result<Option<String>> {
        Ok(self
            .conn
            .query_row("SELECT value FROM settings WHERE key=?1", [key], |r| {
                r.get(0)
            })
            .optional()?)
    }

    pub fn set_settings(&mut self, writes: &[(&str, &str)]) -> Result<()> {
        let tx = self.conn.transaction()?;
        for (key, value) in writes {
            tx.execute("INSERT INTO settings(key,value,updated_at) VALUES (?1,?2,CURRENT_TIMESTAMP) ON CONFLICT(key) DO UPDATE SET value=excluded.value,updated_at=excluded.updated_at", params![key, value])?;
        }
        tx.commit()?;
        Ok(())
    }

    pub fn create_session(&self, hash: &str, expires: u64, now: u64) -> Result<()> {
        self.conn.execute(
            "DELETE FROM management_sessions WHERE expires_at <= ?1",
            [now],
        )?;
        self.conn.execute(
            "INSERT INTO management_sessions(token_hash,expires_at) VALUES (?1,?2)",
            params![hash, expires],
        )?;
        Ok(())
    }
    pub fn session_exists(&self, hash: &str, now: u64) -> Result<bool> {
        Ok(self.conn.query_row("SELECT EXISTS(SELECT 1 FROM management_sessions WHERE token_hash=?1 AND expires_at > ?2)", params![hash, now], |r| r.get(0))?)
    }
    pub fn revoke_session(&self, hash: &str) -> Result<()> {
        self.conn.execute(
            "DELETE FROM management_sessions WHERE token_hash=?1",
            [hash],
        )?;
        Ok(())
    }
}

/// Updates the folded FTS5 search row for a single article.
pub fn index_article_fts(
    conn: &Connection,
    article_id: i64,
    title: &str,
    summary: Option<&str>,
    content_text: Option<&str>,
) -> rusqlite::Result<()> {
    let id_str = article_id.to_string();
    conn.execute("DELETE FROM articles_fts WHERE article_id=?1", [&id_str])?;
    conn.execute(
        "INSERT INTO articles_fts(article_id,title,summary,content_text) VALUES (?1,?2,?3,?4)",
        params![
            id_str,
            crate::documents::fold_diacritics(title),
            crate::documents::fold_diacritics(summary.unwrap_or("")),
            crate::documents::fold_diacritics(content_text.unwrap_or("")),
        ],
    )?;
    Ok(())
}

#[cfg(test)]
#[path = "db_tests.rs"]
mod tests;
