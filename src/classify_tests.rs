//! Classification tests, including the 50-item sample that defines the P5 acceptance
//! metric. The sample is built by hand (no network) and the measurement prints the
//! exact numbers so the report can quote them.

use super::*;

/// 50 hand-built headlines: **25 dev items** used to tune the rule keywords, and
/// **25 holdout items** that were written before tuning and are never used to change a
/// rule. Reporting the two numbers separately is what stops an inflated score: tuning
/// on the same items you measure on is overfitting, and the report says so.
pub const DEV: &[(&str, &str, &str)] = &[
    (
        "Fed giữ nguyên lãi suất, thị trường chứng khoán tăng nhẹ",
        "finance",
        "us",
    ),
    ("ECB cắt lãi suất lần đầu sau hai năm", "finance", "eu"),
    (
        "Ngân hàng Nhà nước điều chỉnh lãi suất điều hành",
        "finance",
        "vn",
    ),
    (
        "Lạm phát Mỹ hạ nhiệt, CPI tháng 10 tăng iki",
        "finance",
        "us",
    ),
    (
        "Doanh thu quý 3 của các ngân hàng lớn tăng",
        "finance",
        "vn",
    ),
    (
        "Trái phiếu chính phủ kỳ hạn 10 năm tăng giá",
        "finance",
        "us",
    ),
    (
        "OpenAI ra mắt mô hình AI mới cho doanh nghiệp",
        "technology-ai",
        "us",
    ),
    (
        "Anthropic công bố cải tiến an toàn cho AI",
        "technology-ai",
        "us",
    ),
    (
        "Chip bán dẫn của Microsoft giảm giá thành sản xuất",
        "technology-ai",
        "us",
    ),
    (
        "Google mở rộng hạ tầng cloud tại châu Á",
        "technology-ai",
        "global",
    ),
    ("Quốc hội thông qua luật mới về dữ liệu", "politics", "vn"),
    ("Thượng viện Mỹ bỏ phiếu về ngân sách", "politics", "us"),
    ("Bầu cử Nghị viện EU diễn ra vào tháng 6", "politics", "eu"),
    (
        "Chính phủ Pháp công bố chính sách thuế mới",
        "politics",
        "fr",
    ),
    (
        "Lệnh trừng phạt mới của phương Tây với Nga",
        "politics",
        "global",
    ),
    ("Shopee tăng phí người bán từ tháng 12", "ecommerce", "vn"),
    (
        "Amazon mở trung tâm logistics mới tại Đức",
        "ecommerce",
        "global",
    ),
    ("Etsy thay đổi chính sách phí cho seller", "ecommerce", "us"),
    ("TikTok Shop cán mốc doanh thu mới", "ecommerce", "global"),
    (
        "Lazada giảm phí vận chuyển mùa khuyến mãi",
        "ecommerce",
        "vn",
    ),
    (
        "Liên Hợp Quốc kêu gọi hành động về khí hậu",
        "world",
        "global",
    ),
    ("NATO họp khẩn về chiến tranh ở Đông Âu", "world", "global"),
    ("Thế giới ghi nhận năm nóng nhất lịch sử", "world", "global"),
    ("Hội nghị thượng đỉnh về khí hậu tại Paris", "world", "fr"),
    ("Xung đột ở Trung Đông ảnh hưởng giá dầu", "world", "global"),
];

/// Holdout: written as a separate batch, never inspected while editing `TOPIC_RULES`.
pub const HOLDOUT: &[(&str, &str, &str)] = &[
    ("USD mạnh lên, EUR giảm so với USD", "finance", "global"),
    (
        "Bitcoin vượt 70.000 USD, dòng tiền ETF tăng",
        "finance",
        "global",
    ),
    (
        "Ethereum nâng cấp mạng lưới, phí giao dịch giảm",
        "technology-ai",
        "global",
    ),
    ("VND ổn định, NHNN bơm thanh khoản", "finance", "vn"),
    ("HOSE ghi nhận thanh khoản cao nhất quý", "finance", "vn"),
    (
        "Nasdaq lập đỉnh mới nhờ cổ phiếu công nghệ",
        "finance",
        "us",
    ),
    ("Cổ phiếu AAPL tăng sau báo cáo lợi nhuận", "finance", "us"),
    ("EPA:MC công bố kết quả kinh doanh quý", "finance", "fr"),
    ("IRS công bố hướng dẫn khai thuế mới", "politics", "us"),
    ("Cục Thuế hướng dẫn quyết toán thuế năm", "politics", "vn"),
    ("BOFiP cập nhật học thuyết thuế VAT", "politics", "fr"),
    (
        "Nghị định mới về thuế thương mại điện tử",
        "ecommerce",
        "vn",
    ),
    ("Luật thuế tối thiểu toàn cầu có hiệu lực", "politics", "eu"),
    (
        "Công ty công nghệ sa thải nhân viên hàng loạt",
        "technology-ai",
        "global",
    ),
    (
        "Robot trong nhà máy thay thế lao động",
        "technology-ai",
        "global",
    ),
    ("Quốc hội thảo luận ngân sách quốc gia", "politics", "vn"),
    (
        "Thống đốc ngân hàng trung ương cảnh báo rủi ro",
        "finance",
        "global",
    ),
    ("Giá vàng tăng khi lạm phát dai dẳng", "finance", "global"),
    (
        "Cổ phiếu ngân hàng châu Âu giảm sau tin ECB",
        "finance",
        "eu",
    ),
    (
        "Amazon ra mắt dịch vụ AI cho người bán",
        "technology-ai",
        "us",
    ),
    (
        "Thương mại điện tử Việt Nam tăng trưởng 25%",
        "ecommerce",
        "vn",
    ),
    (
        "Chính phủ Mỹ đóng cửa một phần do bất đồng ngân sách",
        "politics",
        "us",
    ),
    (
        "Vận chuyển toàn cầu bị ảnh hưởng bởi chiến tranh",
        "world",
        "global",
    ),
    (
        "Công nghệ AI trong y tế được cấp phép",
        "technology-ai",
        "global",
    ),
    (
        "Thị trường chứng khoán châu Á giảm điểm",
        "finance",
        "global",
    ),
];

fn measure(sample: &[(&str, &str, &str)]) -> (usize, Vec<(String, String, String)>) {
    let mut correct = 0;
    let mut misses = Vec::new();
    for (title, expected, region) in sample {
        let labels = classify(region, title);
        let actual = labels.topic.as_ref().map(|topic| topic.value.clone());
        match actual.as_deref() {
            Some(found) if found == *expected => correct += 1,
            other => misses.push((
                (*title).to_string(),
                (*expected).to_string(),
                other.unwrap_or("none").to_string(),
            )),
        }
    }
    (correct, misses)
}

/// The P5 acceptance metric, measured honestly: the dev set is what the rules were
/// tuned on, so only the holdout number is evidence of generalisation. Both are
/// printed and asserted against the 80% threshold; a failure here is a real failure,
/// not something to paper over by editing the holdout.
#[test]
fn the_rule_classifier_is_measured_on_dev_and_holdout() {
    assert_eq!(
        DEV.len() + HOLDOUT.len(),
        50,
        "the combined sample is exactly 50"
    );
    let (dev_correct, dev_misses) = measure(DEV);
    let (holdout_correct, holdout_misses) = measure(HOLDOUT);
    let dev_rate = dev_correct * 100 / DEV.len();
    let holdout_rate = holdout_correct * 100 / HOLDOUT.len();
    eprintln!(
        "rules dev: {dev_correct}/{} = {dev_rate}% | holdout: {holdout_correct}/{} = {holdout_rate}%",
        DEV.len(),
        HOLDOUT.len()
    );
    for (title, expected, actual) in &dev_misses {
        eprintln!("dev miss: {actual:?} (want {expected}) <- {title}");
    }
    for (title, expected, actual) in &holdout_misses {
        eprintln!("holdout miss: {actual:?} (want {expected}) <- {title}");
    }
    assert!(
        dev_rate >= 80,
        "dev accuracy {dev_rate}% is below 80%; the rules are under-tuned"
    );
    assert!(
        holdout_rate >= 80,
        "holdout accuracy {holdout_rate}% is below 80%; this is the real acceptance metric"
    );
}

#[test]
fn the_rule_pass_is_deterministic() {
    let first: Vec<_> = DEV
        .iter()
        .chain(HOLDOUT)
        .map(|(title, _, region)| classify(region, title))
        .collect();
    let second: Vec<_> = DEV
        .iter()
        .chain(HOLDOUT)
        .map(|(title, _, region)| classify(region, title))
        .collect();
    assert_eq!(
        first, second,
        "the same input must always give the same labels"
    );
}

#[test]
fn a_global_item_is_refined_to_a_named_region() {
    let label = classify("global", "Chính phủ Pháp công bố chính sách thuế mới");
    assert_eq!(label.region.as_ref().map(|r| r.value.as_str()), Some("fr"));
    // A non-global source keeps its own region and is never overridden.
    let label = classify("vn", "Chính phủ Pháp công bố chính sách thuế mới");
    assert!(label.region.is_none(), "the source region wins");
}

#[test]
fn entities_are_extracted_with_types() {
    let label = classify(
        "us",
        "Cổ phiếu AAPL và NASDAQ:AAPL tăng, USD mạnh, Bitcoin vượt đỉnh",
    );
    for expected in ["TICKER:AAPL", "NASDAQ:AAPL", "CURRENCY:USD", "CRYPTO:BTC"] {
        assert!(
            label.entities.iter().any(|entity| entity == expected),
            "missing {expected} in {:?}",
            label.entities
        );
    }
    // Ordinary prose is not a ticker.
    let plain = classify("global", "THE NEW RULES ARE OUT");
    assert!(
        plain
            .entities
            .iter()
            .all(|entity| !entity.starts_with("TICKER:THE")),
        "{:?}",
        plain.entities
    );
}

/// Regression: the 100 real govinfo public-law items fetched on production came back
/// with `TICKER:LAW`, `TICKER:ACT`, `TICKER:OF` and `TICKER:ON` on almost every row,
/// because folding a Title-Case headline to uppercase turns every ordinary word into an
/// all-caps token. These are the exact titles that exposed it.
#[test]
fn a_title_case_headline_yields_no_tickers_from_ordinary_words() {
    // A place name in a headline is capitalised like a ticker too. It is not one, and
    // with no country entity type available the honest answer is no entity at all.
    for title in [
        "Public Law 119 - 137 - United States Commission on International Religious Freedom Reauthorization Act of 2026",
        "Public Law 119 - 111 - Lindsey O. Graham Sanctioning Russia and Iran Act of 2025",
        "Public Law 119 - 58 - Secure Rural Schools Reauthorization Act of 2025",
        "Public Law 119 - 3 - Joint Resolution Providing for Congressional Disapproval",
    ] {
        let label = classify("us", title);
        assert!(
            label.entities.is_empty(),
            "{title} produced {:?}",
            label.entities
        );
    }
    // A real ticker in a headline still survives alongside the ordinary words.
    let mixed = classify("us", "Public Law 119 - Act of 2026 lifts AAPL");
    assert!(
        mixed.entities.iter().any(|entity| entity == "TICKER:AAPL"),
        "{:?}",
        mixed.entities
    );
    assert!(
        mixed.entities.iter().all(|entity| entity != "TICKER:LAW"),
        "{:?}",
        mixed.entities
    );
}

/// A headline with no mixed case at all says nothing about which words are symbols.
/// Guessing produced `TICKER:RURAL` and `TICKER:OIL`; taking none is the honest answer.
#[test]
fn an_all_caps_headline_yields_no_guessed_tickers() {
    for title in [
        "PUBLIC LAW 119 - 58 - SECURE RURAL SCHOOLS REAUTHORIZATION ACT OF 2025",
        "US AND IRAN TALKS RESUME AS OIL RISES",
    ] {
        let label = classify("us", title);
        assert!(
            label.entities.iter().all(|e| !e.starts_with("TICKER:")),
            "{title} produced {:?}",
            label.entities
        );
    }
    // The lowercase equivalent behaves the same way, which is the point.
    let lower = classify("us", "oil rises as talks resume");
    assert!(
        lower.entities.iter().all(|e| !e.starts_with("TICKER:")),
        "{:?}",
        lower.entities
    );
}

#[test]
fn ties_keep_the_declared_rule_order() {
    // "market" (finance) and "world" both appear once; finance is declared first.
    let label = classify("global", "market world").topic.unwrap();
    assert_eq!(label.value, "finance");
}

#[test]
fn a_human_label_survives_a_later_rule_pass() {
    // Each step is printed so a failure shows which write slipped through.

    let state = crate::providers::tests_support::state_with(crate::fetch::Fetcher::public_only());
    let db = state.db.lock().unwrap();
    db.conn
        .execute(
            "INSERT INTO articles(id,canonical_url,title,source_id) VALUES (1,'https://example.invalid/a','Fed keeps rates',NULL)",
            [],
        )
        .unwrap();
    let labels = classify("global", "Fed keeps rates");
    save(&db, 1, &labels).unwrap();
    // A human overrides the topic.
    db.conn
        .execute(
            "INSERT OR REPLACE INTO article_matches(article_id,kind,value,origin,reason)
             VALUES (1,'topic','politics','human','editor')",
            [],
        )
        .unwrap();
    save(&db, 1, &labels).unwrap();
    let stored = labels_for(&db, 1);
    assert!(
        stored.contains(&("topic".into(), "politics".into())),
        "human label kept: {stored:?}"
    );
    assert!(
        !stored.contains(&("topic".into(), "finance".into())),
        "the rule must not re-add the overridden value"
    );
}

fn labels_for(db: &db::Database, id: i64) -> Vec<(String, String)> {
    labels(db, id).unwrap()
}

#[test]
fn saving_one_classification_never_crosses_kinds() {
    let state = crate::providers::tests_support::state_with(crate::fetch::Fetcher::public_only());
    let db = state.db.lock().unwrap();
    db.conn
        .execute(
            "INSERT INTO articles(id,canonical_url,title,source_id) VALUES (2,'https://example.invalid/b','Bitcoin jumps as Fed holds',NULL)",
            [],
        )
        .unwrap();
    let classification = classify("global", "Bitcoin jumps as Fed holds");
    assert!(
        classification.region.is_none(),
        "a global source with no place hint stays global"
    );
    save(&db, 2, &classification).unwrap();
    let stored = labels(&db, 2).unwrap();
    assert!(
        stored
            .iter()
            .any(|(kind, value)| kind == "topic" && value == "finance")
    );
    assert!(
        stored
            .iter()
            .any(|(kind, value)| kind == "entity" && value == "CRYPTO:BTC")
    );
    assert!(stored.iter().all(|(kind, _)| kind != "region"));
}

#[test]
fn reclassifying_the_whole_table_is_safe_to_repeat() {
    let state = crate::providers::tests_support::state_with(crate::fetch::Fetcher::public_only());
    let db = state.db.lock().unwrap();
    for (id, title) in [
        (1, "Fed holds rates"),
        (2, "Shopee raises seller fees"),
        (3, "Bitcoin hits a record"),
    ] {
        db.conn
            .execute(
                "INSERT INTO articles(id,canonical_url,title,source_id) VALUES (?1,?2,?3,NULL)",
                rusqlite::params![id, format!("https://example.invalid/{id}"), title],
            )
            .unwrap();
    }
    let first = reclassify_all(&db).unwrap();
    let second = reclassify_all(&db).unwrap();
    assert!(first > 0);
    assert_eq!(
        first, second,
        "the same rules produce the same number of writes on a repeat pass"
    );
    let count: i64 = db
        .conn
        .query_row("SELECT COUNT(*) FROM article_matches", [], |r| r.get(0))
        .unwrap();
    assert_eq!(
        count as usize, first,
        "labels are upserted, never duplicated"
    );
}

/// A repeat pass must also *remove* labels the current rules no longer produce.
/// `store` alone only upserts, which is why the 100 govinfo articles kept
/// `TICKER:LAW`/`TICKER:ACT` after the ticker rule was fixed.
#[test]
fn reclassifying_removes_a_rule_label_that_no_longer_applies() {
    let state = crate::providers::tests_support::state_with(crate::fetch::Fetcher::public_only());
    let db = state.db.lock().unwrap();
    db.conn
        .execute(
            "INSERT INTO articles(id,canonical_url,title,source_id)
             VALUES (1,'https://example.invalid/1','Public Law 119 - Act of 2026',NULL)",
            [],
        )
        .unwrap();
    // What the old rule would have written.
    for stale in ["TICKER:LAW", "TICKER:ACT"] {
        store(&db.conn, 1, "entity", stale, "rule", "pattern").unwrap();
    }
    // A human label must survive the sweep.
    store(&db.conn, 1, "topic", "politics", "human", "owner").unwrap();

    reclassify_all(&db).unwrap();

    let stored = labels(&db, 1).unwrap();
    assert!(
        stored
            .iter()
            .all(|(kind, value)| !(kind == "entity"
                && (value == "TICKER:LAW" || value == "TICKER:ACT"))),
        "stale rule entities were not removed: {stored:?}"
    );
    assert!(
        stored
            .iter()
            .any(|(kind, value)| kind == "topic" && value == "politics"),
        "a human label must survive: {stored:?}"
    );
}
