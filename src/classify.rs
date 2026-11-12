//! Rule-based classification of articles (P5 foundation, no LLM required).
//!
//! Three independent axes, each decided by an inspectable rule so every label can be
//! explained and corrected:
//! * **region** — where the item belongs. A source's region wins; the title is only
//!   used to refine a `global` item to a specific region when a clear place name is
//!   present.
//! * **topic** — the editorial topic tree. Keyword rules, longest match wins; a tie
//!   keeps the order declared in [`TOPIC_RULES`], never a random pick.
//! * **entity** — tickers, currencies, institutions and assets, extracted by pattern.
//!
//! This module never calls the network. The LLM layer in [`crate::llm`] may add labels
//! on top, but the rule layer always runs first and its output is what "no-LLM mode"
//! means.

use crate::{SharedState, db, internal_error};
use axum::{Json, extract::State, response::IntoResponse};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

/// Topic rules, ordered: when two rules match equally well, the earlier one wins.
/// Slugs must exist in the `topics` table; a missing topic is skipped, not invented.
pub const TOPIC_RULES: &[(&str, &[&str])] = &[
    (
        "finance",
        &[
            "lãi suất",
            "lai suat",
            "interest rate",
            "inflation",
            "lạm phát",
            "lam phat",
            "cpi",
            "gdp",
            "ngân hàng",
            "ngan hang",
            "central bank",
            "fed",
            "ecb",
            "chứng khoán",
            "chung khoan",
            "stock",
            "market",
            "thị trường",
            "thi truong",
            "trái phiếu",
            "trai phieu",
            "bond",
            "revenue",
            "doanh thu",
            "earnings",
            "lợi nhuận",
            "loi nhuan",
            "usd",
            "eur",
            "vnd",
            "bitcoin",
            "btc",
            "ethereum",
            "etf",
            "cổ phiếu",
            "co phieu",
            "nasdaq",
            "nyse",
            "hose",
            "hnx",
            "thanh khoản",
            "thanh khoan",
            "đầu tư",
            "dau tu",
            "quỹ đầu tư",
            "ngoại hối",
            "ngoai hoi",
            "giá dầu",
            "gia dau",
        ],
    ),
    (
        "technology-ai",
        &[
            "ai",
            "trí tuệ nhân tạo",
            "tri tue nhan tao",
            "artificial intelligence",
            "openai",
            "anthropic",
            "google",
            "microsoft",
            "chip",
            "semiconductor",
            "bán dẫn",
            "ban dan",
            "cloud",
            "cloud computing",
            "algorithm",
            "thuật toán",
            "thuat toan",
            "robot",
            "ethereum",
            "công nghệ",
            "cong nghe",
            "mạng lưới",
            "mang luoi",
            "sa thải",
            "sa thai",
            "nhà máy",
            "nha may",
        ],
    ),
    (
        "politics",
        &[
            "quốc hội",
            "quoc hoi",
            "nghị viện",
            "nghi vien",
            "parliament",
            "senate",
            "congress",
            "bầu cử",
            "bau cu",
            "election",
            "chính phủ",
            "chinh phu",
            "government",
            "thủ tướng",
            "thu tuong",
            "tổng thống",
            "tong thong",
            "president",
            "policy",
            "chính sách",
            "chinh sach",
            "sanction",
            "trừng phạt",
            "trung phat",
            "thượng viện",
            "thuong vien",
            "hạ viện",
            "ha vien",
            "ngân sách",
            "ngan sach",
            "irs",
            "thuế",
            "thue",
            "luật",
            "luat",
            "nghị định",
            "nghi dinh",
            "bofip",
            "legifrance",
        ],
    ),
    (
        "ecommerce",
        &[
            "thương mại điện tử",
            "thuong mai dien tu",
            "e-commerce",
            "ecommerce",
            "shopee",
            "lazada",
            "amazon",
            "etsy",
            "tiktok shop",
            "seller",
            "người bán",
            "nguoi ban",
            "logistics",
            "vận chuyển",
            "van chuyen",
            "fulfilment",
            "tiktok shop",
            "phí người bán",
            "phi nguoi ban",
            "người bán",
            "nguoi ban",
        ],
    ),
    (
        "world",
        &[
            "thế giới",
            "the gioi",
            "world",
            "war",
            "chiến tranh",
            "chien tranh",
            "nato",
            "united nations",
            "liên hợp quốc",
            "lien hop quoc",
            "climate",
            "khí hậu",
            "khi hau",
            "xung đột",
            "xung dot",
            "trung đông",
            "trung dong",
        ],
    ),
];

/// Place names that let a `global` item be refined to a region. Longest first so
/// "việt nam" wins over "nam".
const REGION_HINTS: &[(&str, &[&str])] = &[
    (
        "fr",
        &[
            "pháp",
            "phap",
            "france",
            "french",
            "paris",
            "legifrance",
            "bofip",
        ],
    ),
    (
        "us",
        &[
            "mỹ",
            "hoa kỳ",
            "hoa ky",
            "united states",
            "u.s.",
            "usa",
            "washington",
            "irs",
            "federal reserve",
            "sec",
            "nasdaq",
            "nyse",
        ],
    ),
    (
        "vn",
        &[
            "việt nam",
            "viet nam",
            "vietnam",
            "hà nội",
            "ha noi",
            "tp.hcm",
            "hồ chí minh",
            "ho chi minh",
            "hose",
            "hnx",
            "ngân hàng nhà nước",
            "ngan hang nha nuoc",
        ],
    ),
    (
        "eu",
        &[
            "eu",
            "european union",
            "liên minh châu âu",
            "lien minh chau au",
            "brussels",
            "ecb",
        ],
    ),
];

/// Entity patterns. Each returns a normalised identifier when it matches.
pub fn extract_entities(title: &str) -> Vec<String> {
    let mut out = BTreeSet::new();
    let folded = crate::documents::fold_diacritics(title);
    let upper = folded.to_ascii_uppercase();
    // The folded title is what exchange and asset matching run on. Ticker detection is
    // different: it needs the *original* casing, because folding destroys the only
    // signal that separates a symbol from an ordinary word. "Public Law 119 - … Act of
    // 2026" is Title Case, so every word in it was an all-caps token once uppercased and
    // 100 real govinfo items came back labelled `TICKER:LAW`, `TICKER:ACT`,
    // `TICKER:OF`. A symbol is written in capitals *and* nothing else in its token is:
    // "lifts AAPL" and "AAPL tăng" qualify, "Public" and "Rural" do not.
    let original_tokens: Vec<&str> = folded
        .split(|ch: char| !ch.is_alphanumeric() && ch != ':' && ch != '.')
        .filter(|token| !token.is_empty())
        .collect();
    let tokens: Vec<&str> = upper
        .split(|ch: char| !ch.is_alphanumeric() && ch != ':' && ch != '.')
        .filter(|token| !token.is_empty())
        .collect();

    // If the whole title is already in capitals, capitalisation carries no information —
    // a feed that writes "PUBLIC LAW 119 … ACT OF 2025" or "OIL RISES" is not shouting
    // tickers, it simply has no mixed case. Guessing symbols from it produced
    // `TICKER:RURAL` and `TICKER:OIL` too. Rather than guess, take no tickers from such
    // a title; exchange-qualified and asset matching below still work on it.
    let all_caps_title = folded.chars().any(|ch| ch.is_alphabetic())
        && !folded.chars().any(|ch| ch.is_ascii_lowercase());
    if !all_caps_title {
        for (token, original) in tokens.iter().zip(&original_tokens) {
            // Ticker style: 2–5 uppercase letters with at least one vowel, not a common
            // word.
            let bare = token.trim_matches('.');
            let bare_original = original.trim_matches('.');
            if (2..=5).contains(&bare.len())
                && bare.chars().all(|ch| ch.is_ascii_uppercase())
                // Written in capitals in the source, not merely folded there.
                && !bare_original.is_empty()
                && bare_original.chars().all(|ch| ch.is_ascii_uppercase())
                && bare.chars().any(|ch| "AEIOU".contains(ch))
                && !STOP_WORDS.contains(&bare)
            {
                out.insert(format!("TICKER:{bare}"));
            }
        }
    }
    // Exchange-qualified tickers: EPA:MC, NASDAQ:AAPL, HOSE:VCB.
    for token in &tokens {
        if let Some((exchange, symbol)) = token.split_once(':')
            && matches!(
                exchange,
                "EPA" | "NASDAQ" | "NYSE" | "HOSE" | "HNX" | "EURONEXT"
            )
            && (1..=6).contains(&symbol.len())
            && symbol.chars().all(|ch| ch.is_ascii_alphanumeric())
        {
            out.insert(format!("{exchange}:{symbol}"));
        }
    }
    for (asset, needle) in [
        ("CRYPTO:BTC", "BITCOIN"),
        ("CRYPTO:BTC", "BTC"),
        ("CRYPTO:ETH", "ETHEREUM"),
        ("CRYPTO:ETH", "ETH"),
    ] {
        if upper
            .split(|ch: char| !ch.is_alphanumeric())
            .any(|w| w == needle)
        {
            out.insert(asset.to_string());
        }
    }
    for (currency, needle) in [
        ("CURRENCY:USD", "USD"),
        ("CURRENCY:EUR", "EUR"),
        ("CURRENCY:VND", "VND"),
        ("CURRENCY:GBP", "GBP"),
        ("CURRENCY:JPY", "JPY"),
    ] {
        if upper
            .split(|ch: char| !ch.is_alphanumeric())
            .any(|w| w == needle)
        {
            out.insert(currency.to_string());
        }
    }
    for (org, needles) in [
        ("ORG:FED", ["FEDERAL RESERVE", "THE FED", "FED "].as_slice()),
        ("ORG:ECB", ["EUROPEAN CENTRAL BANK", "ECB"].as_slice()),
        (
            "ORG:SEC",
            ["SECURITIES AND EXCHANGE COMMISSION", "SEC "].as_slice(),
        ),
        ("ORG:IRS", ["INTERNAL REVENUE SERVICE", "IRS"].as_slice()),
        (
            "ORG:SBV",
            ["STATE BANK OF VIETNAM", "NGAN HANG NHA NUOC"].as_slice(),
        ),
    ] {
        if needles.iter().any(|needle| upper.contains(needle)) {
            out.insert(org.to_string());
        }
    }
    out.into_iter().collect()
}

/// Words that look like tickers but are ordinary prose.
///
/// Ticker detection now requires the word to be genuinely capitalised in the source and
/// takes nothing from an all-caps title, so this list no longer has to enumerate every
/// ordinary English word. It keeps only what is ambiguous *even when genuinely
/// capitalised*: market abbreviations that are also words, and a handful of short
/// words a sports or legal headline can put in capitals.
const STOP_WORDS: &[&str] = &[
    "AI", "CEO", "CFO", "USA", "EU", "GDP", "CPI", "PMI", "IPO", "ETF", "API", "URL", "PDF",
    "HTML", "HTTP", "HTTPS", "ACT", "LAW", "BILL", "THE", "AND", "FOR", "NOT", "NEW", "ALL", "BUT",
    "ITS", "MAY", "CAN", "HAS", "ARE", "WAS", "WHO", "WHY", "HOW", "ONE", "TWO", "TOP", "NOW",
    "OUT", "DAY", "OFF", "PER", "PRO", "SUB", "VIA", "YOU", "OUR", "HIS", "HER", "HIM",
    // Roman numerals: "FISA Amendments Act … Title VII" is a section, not a listing.
    "II", "III", "IV", "VI", "VII", "VIII", "IX", "XI", "XII", "XIII", "XIV", "XV", "XVI",
];

/// Picks a topic by keyword score. Returns `(slug, matched_keyword, score)`.
pub fn classify_topic(title: &str) -> Option<(String, String, usize)> {
    let folded = crate::documents::fold_diacritics(title).to_lowercase();
    let mut best: Option<(String, String, usize)> = None;
    for (slug, keywords) in TOPIC_RULES {
        let mut score = 0;
        let mut matched = String::new();
        for keyword in *keywords {
            let needle = crate::documents::fold_diacritics(keyword).to_lowercase();
            if folded.contains(&needle) {
                score += 1;
                if matched.is_empty() || needle.len() > matched.len() {
                    matched = needle;
                }
            }
        }
        if score == 0 {
            continue;
        }
        // Strictly greater keeps the earlier rule on a tie: deterministic and explained.
        if best.as_ref().is_none_or(|(_, _, current)| score > *current) {
            best = Some(((*slug).into(), matched, score));
        }
    }
    best
}

/// Refines a region when the source is `global` but the title clearly names a region.
pub fn classify_region(source_region: &str, title: &str) -> Option<(String, String)> {
    if source_region != "global" {
        return None;
    }
    let folded = crate::documents::fold_diacritics(title).to_lowercase();
    let mut best: Option<(String, String)> = None;
    for (region, hints) in REGION_HINTS {
        for hint in *hints {
            let needle = crate::documents::fold_diacritics(hint).to_lowercase();
            if folded.contains(&needle)
                && best
                    .as_ref()
                    .is_none_or(|(_, current)| needle.len() > current.len())
            {
                best = Some(((*region).into(), needle));
            }
        }
    }
    best
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Classification {
    pub region: Option<RegionLabel>,
    pub topic: Option<TopicLabel>,
    pub entities: Vec<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct RegionLabel {
    pub value: String,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct TopicLabel {
    pub value: String,
    pub reason: String,
    pub score: usize,
}

/// Classifies one title. `source_region` is the region of the feed the item came from.
pub fn classify(source_region: &str, title: &str) -> Classification {
    let region = classify_region(source_region, title).map(|(value, hint)| RegionLabel {
        value,
        reason: format!("title mentions \"{hint}\""),
    });
    let topic = classify_topic(title).map(|(value, keyword, score)| TopicLabel {
        value,
        reason: format!("keyword \"{keyword}\""),
        score,
    });
    Classification {
        region,
        topic,
        entities: extract_entities(title),
    }
}

/// Stores a classification, replacing only rows of the same origin so a human edit is
/// never silently overwritten by a later rule pass.
pub fn save(db: &db::Database, article_id: i64, label: &Classification) -> anyhow::Result<usize> {
    save_in(&db.conn, article_id, label)
}

/// Same as [`save`] but on a connection or transaction, so collection can label the
/// rows it just inserted inside its own transaction.
pub fn save_in(
    conn: &rusqlite::Connection,
    article_id: i64,
    label: &Classification,
) -> anyhow::Result<usize> {
    // A human correction supersedes the rule's own value of the same kind: leaving both
    // would show two contradictory topics for one article.
    for kind in ["region", "topic"] {
        conn.execute(
            "DELETE FROM article_matches
              WHERE article_id=?1 AND kind=?2 AND origin='rule'
                AND EXISTS (SELECT 1 FROM article_matches h
                             WHERE h.article_id=?1 AND h.kind=?2 AND h.origin='human')",
            rusqlite::params![article_id, kind],
        )?;
    }
    let mut written = 0;
    if let Some(region) = &label.region {
        written += store(
            conn,
            article_id,
            "region",
            &region.value,
            "rule",
            &region.reason,
        )?;
    }
    if let Some(topic) = &label.topic {
        written += store(
            conn,
            article_id,
            "topic",
            &topic.value,
            "rule",
            &topic.reason,
        )?;
    }
    for entity in &label.entities {
        written += store(conn, article_id, "entity", entity, "rule", "pattern")?;
    }
    Ok(written)
}

fn store(
    conn: &rusqlite::Connection,
    article_id: i64,
    kind: &str,
    value: &str,
    origin: &str,
    reason: &str,
) -> anyhow::Result<usize> {
    // A human label for this kind wins outright: a rule pass must never re-add its own
    // value next to a corrected one, or the reader would show both.
    let human: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM article_matches WHERE article_id=?1 AND kind=?2 AND origin='human')",
        rusqlite::params![article_id, kind],
        |r| r.get(0),
    )?;
    if human {
        // Report "no write"; the caller counts rows written, not rows attempted.
        return Ok(0);
    }
    Ok(conn.execute(
        "INSERT INTO article_matches(article_id,kind,value,origin,reason) VALUES (?1,?2,?3,?4,?5)
         ON CONFLICT(article_id,kind,value) DO UPDATE SET origin=excluded.origin,reason=excluded.reason",
        rusqlite::params![article_id, kind, value, origin, reason],
    )?)
}

/// Labels for one article, for the reader. Used by tests today and by the article
/// endpoints once labels are surfaced in `/api/v1/articles`.
#[cfg_attr(not(test), allow(dead_code))]
pub fn labels(db: &db::Database, article_id: i64) -> rusqlite::Result<Vec<(String, String)>> {
    let mut stmt = db.conn.prepare(
        "SELECT kind,value FROM article_matches WHERE article_id=?1 ORDER BY kind,value",
    )?;
    let rows = stmt.query_map([article_id], |r| Ok((r.get(0)?, r.get(1)?)))?;
    rows.collect()
}

/// Re-runs the rule layer for every article. Cheap, deterministic and safe to repeat.
///
/// "Safe to repeat" only holds if a repeat pass *removes* what the previous pass added
/// and this one no longer produces. `store` upserts, so without the delete below a rule
/// change would leave the old labels in place forever: after the ticker fix, the 100
/// govinfo articles kept `TICKER:LAW`/`TICKER:ACT` until they were deleted here. Rule
/// labels for a kind are therefore cleared first; a `human` label is never touched.
pub fn reclassify_all(db: &db::Database) -> anyhow::Result<usize> {
    let rows: Vec<(i64, String, String)> = {
        let mut stmt = db.conn.prepare(
            "SELECT a.id, a.title, COALESCE(NULLIF(s.region,''),'global')
             FROM articles a LEFT JOIN sources s ON s.id=a.source_id",
        )?;
        stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
            .collect::<rusqlite::Result<Vec<_>>>()?
    };
    let mut written = 0;
    for (id, title, region) in rows {
        db.conn.execute(
            "DELETE FROM article_matches WHERE article_id=?1 AND origin='rule'",
            rusqlite::params![id],
        )?;
        written += save(db, id, &classify(&region, &title))?;
    }
    Ok(written)
}

#[derive(Deserialize)]
pub struct ClassifyInput {
    title: String,
    #[serde(default)]
    region: Option<String>,
}

#[derive(Serialize)]
pub struct ClassifyOutput {
    #[serde(flatten)]
    labels: Classification,
    llm: Option<crate::llm::LlmLabel>,
    mode: &'static str,
}

/// Admin helper: classify one title, with or without the LLM layer. Used to check a
/// rule before turning the model on, and by the 50-item sample measurement.
pub async fn classify_one(
    State(state): State<SharedState>,
    Json(input): Json<ClassifyInput>,
) -> impl IntoResponse {
    let region = input
        .region
        .as_deref()
        .map(|value| value.trim().to_ascii_lowercase())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "global".into());
    let labels = classify(&region, &input.title);
    if !crate::llm::enabled(&state) {
        return Json(ClassifyOutput {
            labels,
            llm: None,
            mode: "rules",
        })
        .into_response();
    }
    match crate::llm::classify(&state, &input.title, &labels).await {
        Some(label) => Json(ClassifyOutput {
            labels,
            llm: Some(label),
            mode: "rules+llm",
        })
        .into_response(),
        // Any failure leaves the rule labels untouched: the reader never depends on a model.
        None => Json(ClassifyOutput {
            labels,
            llm: None,
            mode: "rules_llm_unavailable",
        })
        .into_response(),
    }
}

pub async fn reclassify(State(state): State<SharedState>) -> impl IntoResponse {
    let result = reclassify_all(&state.db.lock().expect("database lock poisoned"));
    match result {
        Ok(written) => {
            Json(serde_json::json!({ "ok": true, "labels_written": written })).into_response()
        }
        Err(err) => internal_error(err).into_response(),
    }
}

#[cfg(test)]
#[path = "classify_tests.rs"]
mod tests;
