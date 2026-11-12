//! Legal-text sources: a registry that records what each publisher's terms actually
//! say, and a gate that refuses to poll anything whose terms were not read.
//!
//! This is deliberately **not** a set of connectors yet. The plan
//! (`.docs/legal-sources.md` in `lnc-library-ui`) requires every source's terms to be
//! verified against the publisher's own page before Newsman may retrieve anything, and
//! the four sources it names are not all clear:
//!
//! | source | verdict | why |
//! |---|---|---|
//! | govinfo (US) | `not_required` | GPO states public documents may be reprinted without legal restriction and customary credit should be given; feeds are keyless |
//! | Légifrance (FR) | `verified` | licence ouverte 2.0 + PISTE CGU + stated quotas; free after registration |
//! | EUR-Lex (EU) | `unverified` | reuse notice unreadable at check time (HTTP 202, empty body) |
//! | vbpl.vn (VN) | `unverified` | no terms-of-use page published; `robots.txt` disallows `/api/` and `/Pages/` |
//!
//! The registry therefore **ships every source disabled**, and `set_enabled` refuses to
//! switch on a row whose verdict is `unverified` or `prohibited`. That is the whole
//! point: the refusal lives in the database and in one function, not in a comment.
//!
//! What is implemented here is the part that needs no new network code: govinfo
//! announces new content through per-collection RSS feeds the GPO documents, and
//! [`crate::feed::parse`] already reads them while keeping metadata only. Adding a
//! govinfo collection is a *source* row; enabling it is the owner's decision.

use crate::{SharedState, db, failure, internal_error, now, regions};
use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};

/// Verdicts, in the order the gate accepts them.
pub const VERIFIED: &str = "verified";
pub const NOT_REQUIRED: &str = "not_required";
pub const UNVERIFIED: &str = "unverified";
pub const PROHIBITED: &str = "prohibited";

/// True only for a verdict that permits automated retrieval. One list, used by the
/// gate, the API and the tests, so a new verdict cannot be allowed by accident.
pub fn verdict_allows_polling(verdict: &str) -> bool {
    matches!(verdict, VERIFIED | NOT_REQUIRED)
}

/// Why a source that ships in the pack cannot be polled. Recorded per row so the admin
/// surface can explain a refusal without anyone re-reading a chat log.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct LegalSource {
    pub code: String,
    pub region: String,
    pub name_vi: String,
    pub name_en: String,
    pub access_kind: String,
    pub endpoint: String,
    pub access_note: String,
    pub licence: String,
    pub attribution: String,
    pub terms_verdict: String,
    pub terms_checked_on: String,
    pub terms_url: String,
    pub terms_quote: String,
    pub terms_note: String,
    pub rate_limit_per_day: i64,
    pub enabled: bool,
}

impl LegalSource {
    /// What an admin sees next to the switch. Computed, never stored, so it cannot go
    /// stale relative to the verdict.
    pub fn pollable(&self) -> bool {
        verdict_allows_polling(&self.terms_verdict)
    }
    pub fn block_reason(&self) -> Option<&'static str> {
        match self.terms_verdict.as_str() {
            PROHIBITED => Some("terms_prohibit_polling"),
            UNVERIFIED => Some("terms_unverified"),
            // Verified terms are not enough on their own: a source with no documented
            // channel, or one whose region is off, still cannot run.
            _ if self.endpoint.trim().is_empty() => Some("no_documented_channel"),
            _ => None,
        }
    }
}

/// The sources the plan names, with the evidence collected on 12/11/2026. Kept in code
/// rather than only in the database so a fresh installation starts from the same
/// audited state, and so the tests can assert the gate against real rows.
pub struct Seed {
    pub code: &'static str,
    pub region: &'static str,
    pub name_vi: &'static str,
    pub name_en: &'static str,
    pub access_kind: &'static str,
    pub endpoint: &'static str,
    pub access_note: &'static str,
    pub licence: &'static str,
    pub attribution: &'static str,
    pub terms_verdict: &'static str,
    pub terms_checked_on: &'static str,
    pub terms_url: &'static str,
    pub terms_quote: &'static str,
    pub terms_note: &'static str,
    pub rate_limit_per_day: i64,
}

/// One entry per source in the plan. `endpoint` is filled only where the publisher
/// documents a channel we can use; an empty endpoint means "no documented channel",
/// which is itself a reason the gate stays closed.
pub const SEEDS: &[Seed] = &[
    Seed {
        code: "govinfo",
        region: "us",
        name_vi: "GovInfo — Ấn phẩm chính thức của Chính phủ Hoa Kỳ",
        name_en: "GovInfo — US Government Publishing Office",
        access_kind: "feed",
        // Documented per-collection feeds, linked from govinfo.gov/feeds. The connector
        // reads the collection the owner names; these are the ones the plan cares about.
        endpoint: "https://www.govinfo.gov/rss/plaw.xml",
        access_note: "RSS theo bộ sưu tập, không cần khóa (govinfo.gov/feeds)",
        licence: "US Government work — 17 U.S.C. §105",
        attribution: "Source: U.S. Government Publishing Office (govinfo.gov)",
        terms_verdict: NOT_REQUIRED,
        terms_checked_on: "2026-11-12",
        terms_url: "https://www.govinfo.gov/about/policies",
        terms_quote: "public documents can generally be reprinted without legal restriction; customary credit should be given to the Government department or agency which prepared the material.",
        terms_note: "Cùng trang cảnh báo ấn phẩm chính phủ có thể chứa tài liệu có bản quyền của bên thứ ba; chỉ lưu số hiệu, tiêu đề, ngày và liên kết.",
        rate_limit_per_day: 24,
    },
    Seed {
        code: "legifrance",
        region: "fr",
        name_vi: "Légifrance — Công báo pháp luật Pháp",
        name_en: "Légifrance — French official legal gazette",
        access_kind: "api",
        endpoint: "",
        access_note: "API qua cổng PISTE, miễn phí sau khi đăng ký; cần tài khoản + khóa OAuth2",
        licence: "Licence Ouverte 2.0",
        attribution: "Source : Légifrance (DILA)",
        terms_verdict: VERIFIED,
        terms_checked_on: "2026-11-12",
        terms_url: "https://www.legifrance.gouv.fr/contenu/pied-de-page/open-data-et-api",
        terms_quote: "Les données de Légifrance sont mises à disposition pour une réutilisation gratuite comme disposé par l'Arrêté du 24 juin 2014 […] L'utilisation de ces données est soumise : à la licence ouverte 2.0 ; aux conditions générales d'utilisation de PISTE ; aux conditions générales d'utilisation de l'API Légifrance ; à des quotas détaillés sur le portail PISTE.",
        terms_note: "Điều khoản cho phép dùng lại, nhưng CHƯA có khóa PISTE nên endpoint để trống: không gọi được gì cho tới khi chủ dự án đăng ký. Số hạn mức chưa công bố công khai (trên portal PISTE).",
        rate_limit_per_day: 0,
    },
    Seed {
        code: "eur-lex",
        region: "eu",
        name_vi: "EUR-Lex — Pháp luật Liên minh châu Âu",
        name_en: "EUR-Lex — European Union law",
        access_kind: "feed",
        endpoint: "",
        access_note: "Chưa có kênh chính thức kiểm chứng được: trang điều khoản trả về rỗng khi kiểm",
        licence: "Chưa xác nhận cho EUR-Lex (trang thông báo không đọc được)",
        attribution: "Source: EUR-Lex (European Union)",
        terms_verdict: UNVERIFIED,
        terms_checked_on: "2026-11-12",
        terms_url: "https://eur-lex.europa.eu/content/legal-notice/legal-notice.html",
        terms_quote: "",
        terms_note: "Trang điều khoản của EUR-Lex trả HTTP 202 với nội dung rỗng khi kiểm 12/11/2026; feed RSS thử được trả 'Service temporarily not available'. Trang thông báo pháp lý của Ủy ban châu Âu nói nội dung của EU theo CC BY 4.0, nhưng đó không phải điều khoản riêng của EUR-Lex. Chưa đủ căn cứ để bật.",
        rate_limit_per_day: 0,
    },
    Seed {
        code: "vbpl",
        region: "vn",
        name_vi: "Cơ sở dữ liệu quốc gia về pháp luật (Bộ Tư pháp)",
        name_en: "Vietnam national legal database (Ministry of Justice)",
        access_kind: "feed",
        endpoint: "",
        access_note: "Chưa có kênh công bố; robots.txt chặn /api/ và /Pages/",
        licence: "Chưa rõ — chưa tìm thấy trang điều khoản sử dụng",
        attribution: "Nguồn: vbpl.vn (Bộ Tư pháp)",
        terms_verdict: UNVERIFIED,
        terms_checked_on: "2026-11-12",
        terms_url: "https://vbpl.vn/robots.txt",
        terms_quote: "User-Agent: *\nAllow: /\nDisallow: /api/\nDisallow: /Pages/",
        terms_note: "Không tìm thấy trang điều khoản sử dụng nào trên cổng (trang tải bằng JavaScript, footer không có liên kết điều khoản). robots.txt cho phép phần còn lại nhưng chặn /api/ và /Pages/ — tức đường tra cứu. Văn bản pháp luật Việt Nam không thuộc đối tượng bảo hộ quyền tác giả (Điều 15 khoản 2), nhưng điều khoản của trang thì chưa biết. Cần hỏi Bộ Tư pháp hoặc chờ kênh công bố chính thức.",
        rate_limit_per_day: 0,
    },
];

/// Stable key so a re-run never duplicates and never overwrites the owner's switch.
pub fn pack_key(region: &str, code: &str) -> String {
    format!("legal/{region}/{code}")
}

/// Inserts missing seeds. An existing row keeps its `enabled` value and its verdict
/// evidence, exactly like a region pack: re-running an upgrade must not silently
/// re-enable something the owner turned off, nor overwrite a newer terms check.
pub fn seed(db: &db::Database) -> anyhow::Result<usize> {
    let mut added = 0;
    for seed in SEEDS {
        let exists: bool = db.conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM regions WHERE code=?1)",
            [seed.region],
            |r| r.get(0),
        )?;
        if !exists {
            // Region pack not loaded yet; the next reload will pick it up.
            continue;
        }
        let inserted = db.conn.execute(
            "INSERT INTO legal_sources(pack_key,region,code,name_vi,name_en,access_kind,endpoint,
                 access_note,licence,attribution,terms_verdict,terms_checked_on,terms_url,
                 terms_quote,terms_note,rate_limit_per_day,enabled)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,0)
             ON CONFLICT(pack_key) DO NOTHING",
            params![
                pack_key(seed.region, seed.code),
                seed.region,
                seed.code,
                seed.name_vi,
                seed.name_en,
                seed.access_kind,
                seed.endpoint,
                seed.access_note,
                seed.licence,
                seed.attribution,
                seed.terms_verdict,
                seed.terms_checked_on,
                seed.terms_url,
                seed.terms_quote,
                seed.terms_note,
                seed.rate_limit_per_day,
            ],
        )?;
        added += inserted;
    }
    Ok(added)
}

fn row_from(r: &rusqlite::Row<'_>) -> rusqlite::Result<LegalSource> {
    Ok(LegalSource {
        code: r.get(0)?,
        region: r.get(1)?,
        name_vi: r.get(2)?,
        name_en: r.get(3)?,
        access_kind: r.get(4)?,
        endpoint: r.get(5)?,
        access_note: r.get(6)?,
        licence: r.get(7)?,
        attribution: r.get(8)?,
        terms_verdict: r.get(9)?,
        terms_checked_on: r.get(10)?,
        terms_url: r.get(11)?,
        terms_quote: r.get(12)?,
        terms_note: r.get(13)?,
        rate_limit_per_day: r.get(14)?,
        enabled: r.get::<_, i64>(15)? != 0,
    })
}

const SELECT: &str = "SELECT code,region,name_vi,name_en,access_kind,endpoint,access_note,licence,
    attribution,terms_verdict,terms_checked_on,terms_url,terms_quote,terms_note,rate_limit_per_day,enabled
    FROM legal_sources";

pub fn list(db: &db::Database) -> rusqlite::Result<Vec<LegalSource>> {
    let mut stmt = db
        .conn
        .prepare(&format!("{SELECT} ORDER BY region, code"))?;
    let rows = stmt.query_map([], row_from)?;
    rows.collect()
}

pub fn get(db: &db::Database, code: &str) -> rusqlite::Result<Option<LegalSource>> {
    db.conn
        .query_row(&format!("{SELECT} WHERE code=?1"), [code], row_from)
        .optional()
}

/// The only way to switch a legal source on. Refuses a row whose terms were not
/// verified, and refuses to enable a source in a disabled region — mirroring the two
/// gates feeds already have, plus the legal one.
pub fn set_enabled(db: &db::Database, code: &str, enabled: bool) -> Result<(), &'static str> {
    let Some(row) = get(db, code).map_err(|_| "internal")? else {
        return Err("not_found");
    };
    if enabled {
        if !row.pollable() {
            return Err(match row.terms_verdict.as_str() {
                PROHIBITED => "terms_prohibit_polling",
                _ => "terms_unverified",
            });
        }
        if row.endpoint.trim().is_empty() {
            return Err("no_documented_channel");
        }
        if !matches!(regions::region_enabled(db, &row.region), Ok(true)) {
            return Err("region_disabled");
        }
    }
    db.conn
        .execute(
            "UPDATE legal_sources SET enabled=?2, updated_at=CURRENT_TIMESTAMP WHERE code=?1",
            params![code, i64::from(enabled)],
        )
        .map_err(|_| "internal")?;
    // A feed source is only useful if the collector knows about it. The row is created
    // as `legal-<code>` and follows the switch, so enabling one button does the whole
    // job: registry verdict + region gate + collector source + alert matching.
    if row.access_kind == "feed" {
        sync_feed_source(db, &row, enabled)?;
    }
    Ok(())
}

/// Mirror a legal source into the collector's `sources` table. Marked pack-managed with
/// the same key as the registry row, so the catalog UI keeps it read-only apart from the
/// switch.
///
/// INSERT-then-UPDATE rather than one `ON CONFLICT`: `sources.pack_key` is covered by a
/// *partial* unique index (`WHERE pack_key IS NOT NULL`), and SQLite refuses a conflict
/// target that does not repeat that predicate.
fn sync_feed_source(db: &db::Database, row: &LegalSource, enabled: bool) -> Result<(), &'static str> {
    let slug = format!("legal-{}", row.code);
    if !crate::catalog::valid_slug(&slug) || !crate::catalog::valid_source_url(&row.endpoint) {
        return Err("invalid_endpoint");
    }
    let key = pack_key(&row.region, &row.code);
    let language = if row.region == "vn" { "vi" } else { "en" };
    let existing: Option<i64> = db
        .conn
        .query_row("SELECT id FROM sources WHERE pack_key=?1", [&key], |r| r.get(0))
        .optional()
        .map_err(|_| "internal")?;
    match existing {
        Some(id) => {
            db.conn
                .execute(
                    "UPDATE sources SET name=?2, url=?3, enabled=?4, language=?5, region=?6,
                            attribution=?7 WHERE id=?1",
                    params![
                        id,
                        row.name_en,
                        row.endpoint,
                        i64::from(enabled),
                        language,
                        row.region,
                        row.attribution
                    ],
                )
                .map_err(|_| "internal")?;
        }
        None => {
            db.conn
                .execute(
                    "INSERT INTO sources(slug,name,url,tier,enabled,language,region,topic_slug,
                         fetch_interval_minutes,pack_key,attribution)
                     VALUES (?1,?2,?3,'T1',?4,?5,?6,'politics',720,?7,?8)",
                    params![
                        slug,
                        row.name_en,
                        row.endpoint,
                        i64::from(enabled),
                        language,
                        row.region,
                        key,
                        row.attribution
                    ],
                )
                .map_err(|_| "internal")?;
        }
    }
    Ok(())
}

#[derive(Serialize)]
struct Status {
    code: String,
    region: String,
    name_vi: String,
    name_en: String,
    access_kind: String,
    endpoint: String,
    access_note: String,
    licence: String,
    attribution: String,
    terms_verdict: String,
    terms_checked_on: String,
    terms_url: String,
    terms_quote: String,
    terms_note: String,
    rate_limit_per_day: i64,
    enabled: bool,
    pollable: bool,
    block_reason: Option<&'static str>,
    /// Days since the terms were read, and whether that is old enough to re-check. The
    /// admin card shows this so a remembered verdict never looks like a fresh one.
    evidence_age_days: Option<i64>,
    evidence_stale: bool,
}

impl From<LegalSource> for Status {
    fn from(row: LegalSource) -> Self {
        let age = evidence_age_days(&row.terms_checked_on, &today());
        Self {
            pollable: row.pollable(),
            block_reason: row.block_reason(),
            evidence_age_days: age,
            evidence_stale: age.is_some_and(|days| days > REVIEW_AFTER_DAYS),
            code: row.code,
            region: row.region,
            name_vi: row.name_vi,
            name_en: row.name_en,
            access_kind: row.access_kind,
            endpoint: row.endpoint,
            access_note: row.access_note,
            licence: row.licence,
            attribution: row.attribution,
            terms_verdict: row.terms_verdict,
            terms_checked_on: row.terms_checked_on,
            terms_url: row.terms_url,
            terms_quote: row.terms_quote,
            terms_note: row.terms_note,
            rate_limit_per_day: row.rate_limit_per_day,
            enabled: row.enabled,
        }
    }
}

pub async fn list_handler(State(state): State<SharedState>) -> Response {
    let db = state.db.lock().expect("database lock poisoned");
    match list(&db) {
        Ok(rows) => Json(serde_json::json!({ "data": rows.into_iter().map(Status::from).collect::<Vec<_>>() }))
            .into_response(),
        Err(err) => internal_error(err),
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ToggleInput {
    enabled: bool,
}

pub async fn toggle_handler(
    State(state): State<SharedState>,
    Path(code): Path<String>,
    Json(input): Json<ToggleInput>,
) -> Response {
    let db = state.db.lock().expect("database lock poisoned");
    match set_enabled(&db, &code, input.enabled) {
        Ok(()) => Json(serde_json::json!({ "ok": true, "enabled": input.enabled })).into_response(),
        Err("not_found") => failure(StatusCode::NOT_FOUND, "not_found"),
        Err("internal") => failure(StatusCode::INTERNAL_SERVER_ERROR, "internal"),
        Err(code) => failure(StatusCode::CONFLICT, code),
    }
}

/// Honestly reports how stale the evidence is, so nobody mistakes a six-month-old
/// verdict for a current one. `now` is injected so the rule is testable.
pub fn evidence_age_days(checked_on: &str, today: &str) -> Option<i64> {
    let a = chrono::NaiveDate::parse_from_str(checked_on.trim(), "%Y-%m-%d").ok()?;
    let b = chrono::NaiveDate::parse_from_str(today.trim(), "%Y-%m-%d").ok()?;
    Some((b - a).num_days())
}

/// Re-check reminder window. Terms change; an unreviewed verdict older than this is
/// reported as stale on the admin card rather than quietly trusted.
pub const REVIEW_AFTER_DAYS: i64 = 180;

pub fn today() -> String {
    chrono::DateTime::from_timestamp(now() as i64, 0)
        .unwrap_or_default()
        .format("%Y-%m-%d")
        .to_string()
}

#[cfg(test)]
#[path = "legal_tests.rs"]
mod tests;
