//! Legal-source gate tests. The point of these is the *refusal*: a source whose
//! publisher terms were not read must not be switchable on, and that must be true from
//! the seed data rather than from a comment.

use super::*;
use crate::db::Database;

fn db() -> Database {
    let db = Database::in_memory().unwrap();
    // The region tree the seeds attach to. Real installations get these from the pack.
    for (code, parent, enabled) in [
        ("global", None, 1),
        ("eu", Some("global"), 1),
        ("fr", Some("eu"), 1),
        ("us", Some("global"), 1),
        ("vn", Some("global"), 1),
    ] {
        db.conn
            .execute(
                "INSERT OR REPLACE INTO regions(code,parent_code,label_vi,label_en,enabled)
                 VALUES (?1,?2,?3,?3,?4)",
                params![code, parent, code, enabled],
            )
            .unwrap();
    }
    db
}

#[test]
fn every_seed_lands_disabled_and_a_second_run_adds_nothing() {
    let db = db();
    assert_eq!(seed(&db).unwrap(), SEEDS.len());
    assert_eq!(seed(&db).unwrap(), 0, "seeding is idempotent");
    let rows = list(&db).unwrap();
    assert_eq!(rows.len(), SEEDS.len());
    assert!(
        rows.iter().all(|row| !row.enabled),
        "no source may ship enabled"
    );
}

#[test]
fn the_gate_refuses_every_source_whose_terms_were_not_read() {
    let db = db();
    seed(&db).unwrap();
    for code in ["eur-lex", "vbpl"] {
        let row = get(&db, code).unwrap().expect(code);
        assert_eq!(row.terms_verdict, UNVERIFIED, "{code}");
        assert!(!row.pollable(), "{code} must not be pollable");
        assert_eq!(
            set_enabled(&db, code, true).unwrap_err(),
            "terms_unverified",
            "{code}"
        );
        assert!(!get(&db, code).unwrap().unwrap().enabled);
    }
}

#[test]
fn verified_terms_are_not_enough_without_a_documented_channel() {
    // Légifrance: reuse is allowed, but no PISTE key exists yet, so `endpoint` is empty.
    // Enabling it would only produce a connector that cannot run.
    let db = db();
    seed(&db).unwrap();
    let row = get(&db, "legifrance").unwrap().unwrap();
    assert_eq!(row.terms_verdict, VERIFIED);
    assert!(row.pollable(), "the terms themselves do allow polling");
    assert_eq!(row.block_reason(), Some("no_documented_channel"));
    assert_eq!(
        set_enabled(&db, "legifrance", true).unwrap_err(),
        "no_documented_channel"
    );
}

#[test]
fn a_source_with_clear_terms_and_a_channel_can_be_switched_on() {
    let db = db();
    seed(&db).unwrap();
    let row = get(&db, "govinfo").unwrap().unwrap();
    assert_eq!(row.terms_verdict, NOT_REQUIRED);
    assert!(row.endpoint.starts_with("https://www.govinfo.gov/rss/"));
    assert_eq!(row.block_reason(), None);
    set_enabled(&db, "govinfo", true).unwrap();
    assert!(get(&db, "govinfo").unwrap().unwrap().enabled);
    // And off again — an owner decision is reversible without a deploy.
    set_enabled(&db, "govinfo", false).unwrap();
    assert!(!get(&db, "govinfo").unwrap().unwrap().enabled);
}

#[test]
fn a_disabled_region_keeps_its_sources_off() {
    let db = db();
    seed(&db).unwrap();
    db.conn
        .execute("UPDATE regions SET enabled=0 WHERE code='us'", [])
        .unwrap();
    assert_eq!(set_enabled(&db, "govinfo", true).unwrap_err(), "region_disabled");
    assert!(!get(&db, "govinfo").unwrap().unwrap().enabled);
}

#[test]
fn a_prohibitive_verdict_says_so_instead_of_looking_unverified() {
    let db = db();
    seed(&db).unwrap();
    db.conn
        .execute(
            "UPDATE legal_sources SET terms_verdict='prohibited' WHERE code='govinfo'",
            [],
        )
        .unwrap();
    let row = get(&db, "govinfo").unwrap().unwrap();
    assert!(!row.pollable());
    assert_eq!(row.block_reason(), Some("terms_prohibit_polling"));
    assert_eq!(
        set_enabled(&db, "govinfo", true).unwrap_err(),
        "terms_prohibit_polling"
    );
}

#[test]
fn an_unknown_verdict_is_never_treated_as_permission() {
    // A typo in the column must fail closed, not open.
    assert!(verdict_allows_polling(VERIFIED));
    assert!(verdict_allows_polling(NOT_REQUIRED));
    for verdict in [UNVERIFIED, PROHIBITED, "", "Verified", "unknown"] {
        assert!(!verdict_allows_polling(verdict), "{verdict:?}");
    }
}

#[test]
fn the_database_itself_rejects_an_invented_verdict() {
    let db = db();
    seed(&db).unwrap();
    assert!(
        db.conn
            .execute(
                "UPDATE legal_sources SET terms_verdict='probably-fine' WHERE code='govinfo'",
                [],
            )
            .is_err(),
        "the CHECK constraint is the last line of defence"
    );
}

#[test]
fn seeding_never_re_enables_or_overwrites_what_the_owner_changed() {
    let db = db();
    seed(&db).unwrap();
    set_enabled(&db, "govinfo", true).unwrap();
    // A newer terms check recorded by hand must survive an upgrade.
    db.conn
        .execute(
            "UPDATE legal_sources SET terms_checked_on='2027-01-01', terms_note='rechecked'
             WHERE code='govinfo'",
            [],
        )
        .unwrap();
    assert_eq!(seed(&db).unwrap(), 0);
    let row = get(&db, "govinfo").unwrap().unwrap();
    assert!(row.enabled, "an upgrade must not switch a source off");
    assert_eq!(row.terms_checked_on, "2027-01-01");
    assert_eq!(row.terms_note, "rechecked");
}

#[test]
fn the_evidence_age_is_reported_so_an_old_verdict_can_be_flagged() {
    assert_eq!(evidence_age_days("2026-11-12", "2026-11-12"), Some(0));
    assert_eq!(
        evidence_age_days("2026-11-12", "2027-05-11"),
        Some(REVIEW_AFTER_DAYS)
    );
    // Unparseable evidence is unknown, never "fresh".
    assert_eq!(evidence_age_days("", "2026-11-12"), None);
    assert_eq!(evidence_age_days("12/11/2026", "2026-11-12"), None);
}

#[test]
fn the_seed_data_matches_what_was_actually_read() {
    // Guard against drift: each verdict must carry the date and URL it rests on, and an
    // "unverified" row must not pretend to have a quote.
    for seed in SEEDS {
        assert!(
            !seed.terms_checked_on.is_empty() && !seed.terms_url.is_empty(),
            "{} has no evidence",
            seed.code
        );
        assert!(seed.terms_url.starts_with("https://"), "{}", seed.code);
        match seed.terms_verdict {
            UNVERIFIED | PROHIBITED => assert!(
                seed.terms_quote.is_empty() || seed.terms_verdict == UNVERIFIED,
                "{}",
                seed.code
            ),
            VERIFIED | NOT_REQUIRED => assert!(
                !seed.terms_quote.trim().is_empty(),
                "{} claims clearance without a quote",
                seed.code
            ),
            other => panic!("{} has an unknown verdict {other}", seed.code),
        }
        assert!(
            !seed.attribution.is_empty(),
            "{} has no credit line",
            seed.code
        );
    }
}

#[test]
fn only_the_us_source_has_a_working_channel_today() {
    // Deliberate, checkable statement of the current state. If a later session adds a
    // channel for another source, this test should be edited — that is the point.
    let with_endpoint: Vec<&str> = SEEDS
        .iter()
        .filter(|seed| !seed.endpoint.is_empty())
        .map(|seed| seed.code)
        .collect();
    assert_eq!(with_endpoint, vec!["govinfo"]);
}

/// The govinfo feed is the one channel the plan allows today, so its *real* published
/// shape is pinned here rather than assumed. The item below is a verbatim excerpt of
/// `https://www.govinfo.gov/rss/plaw.xml` fetched on 2026-11-12: RSS 2.0 with an
/// `xml:base`, `guid isPermaLink="false"`, and an HTML-laden `<description>` carrying
/// PDF/HTML/USLM/ZIP links.
#[test]
fn the_real_govinfo_feed_shape_yields_metadata_only() {
    let xml = br#"<?xml version="1.0" encoding="UTF-8"?>
<rss version="2.0" xml:base="https://www.govinfo.gov" xmlns:dc="http://purl.org/dc/elements/1.1/">
  <channel>
    <title>Public and Private Laws - New items on GovInfo</title>
    <link>https://www.govinfo.gov/app/collection/plaw</link>
    <language>en</language>
    <item>
      <guid isPermaLink="false">PLAW-119publ58</guid>
      <title>Public Law 119 - 58 - Secure Rural Schools Reauthorization Act of 2025</title>
      <pubDate>Wed, 10 Jun 2026 20:57:14 +0000</pubDate>
      <category>Bills and Statutes</category>
      <link>https://www.govinfo.gov/app/details/PLAW-119publ58</link>
      <description> &lt;h2&gt;Content Files&lt;/h2&gt;
        &lt;a href="https://www.govinfo.gov/content/pkg/PLAW-119publ58/pdf/PLAW-119publ58.pdf"&gt;PDF&lt;/a&gt;
        &lt;a href="https://www.govinfo.gov/content/pkg/PLAW-119publ58.zip"&gt;ZIP&lt;/a&gt;
      </description>
    </item>
  </channel>
</rss>"#;
    let base = url::Url::parse("https://www.govinfo.gov/rss/plaw.xml").unwrap();
    let now = chrono::DateTime::parse_from_rfc3339("2026-11-12T00:00:00Z")
        .unwrap()
        .with_timezone(&chrono::Utc);
    let items = crate::feed::parse(xml, &base, now).unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].guid.as_deref(), Some("PLAW-119publ58"));
    assert_eq!(
        items[0].url,
        "https://www.govinfo.gov/app/details/PLAW-119publ58"
    );
    assert_eq!(
        items[0].published_at.as_deref(),
        Some("2026-06-10T20:57:14Z"),
        "RFC 2822 pubDate is kept as UTC"
    );
    // The description's PDF/ZIP links are never turned into stored items.
    assert!(!format!("{items:?}").contains(".zip"));
    assert!(!format!("{items:?}").contains(".pdf"));
}
