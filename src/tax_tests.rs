//! Tax calendar engine tests: weekend shifting, short months, timezones, region
//! visibility and the "unverified" flag. No network, no clock dependency.

use super::*;
use crate::regions::tests::TempDir as TestDir;
use rusqlite::params;

fn rule(code: &str, cadence: &str, month: Option<i64>, day: Option<i64>) -> Rule {
    Rule {
        code: code.into(),
        region: "global".into(),
        name_vi: "Hạn mẫu".into(),
        name_en: "Sample due".into(),
        cadence: cadence.into(),
        month,
        day,
        weekend_rule: "none".into(),
        lead_days: 10,
        url: String::new(),
        note: String::new(),
        source_code: "src".into(),
        verified: false,
    }
}

#[test]
fn a_weekend_date_moves_to_the_next_business_day() {
    // 2026-11-21 is a Saturday; the next business day is Monday the 23rd.
    let saturday = NaiveDate::from_ymd_opt(2026, 11, 21).unwrap();
    assert_eq!(
        apply_weekend_rule(saturday, "next_business_day"),
        NaiveDate::from_ymd_opt(2026, 11, 23).unwrap()
    );
    // Sunday 22 November 2026 as well.
    let sunday = NaiveDate::from_ymd_opt(2026, 11, 22).unwrap();
    assert_eq!(
        apply_weekend_rule(sunday, "next_business_day"),
        NaiveDate::from_ymd_opt(2026, 11, 23).unwrap()
    );
}

#[test]
fn a_weekend_date_can_move_backwards() {
    // Saturday 2026-05-30 is the nominal date; previous business day is Friday the 29th.
    let saturday = NaiveDate::from_ymd_opt(2026, 5, 30).unwrap();
    assert_eq!(
        apply_weekend_rule(saturday, "previous_business_day"),
        NaiveDate::from_ymd_opt(2026, 5, 29).unwrap()
    );
    // A weekday is untouched by either rule.
    let thursday = NaiveDate::from_ymd_opt(2026, 11, 19).unwrap();
    for mode in [
        "none",
        "next_business_day",
        "previous_business_day",
        "bogus",
    ] {
        assert_eq!(apply_weekend_rule(thursday, mode), thursday, "{mode}");
    }
}

#[test]
fn a_day_31_rule_lands_on_the_last_day_of_short_months() {
    let mut monthly = rule("m", "monthly", None, Some(31));
    monthly.weekend_rule = "none".into();
    let february = NaiveDate::from_ymd_opt(2026, 2, 1).unwrap();
    let due = next_due(&monthly, february).unwrap();
    assert_eq!(
        due,
        NaiveDate::from_ymd_opt(2026, 2, 28).unwrap(),
        "February 2026 has 28 days"
    );
    // A leap year gets the 29th.
    let leap = NaiveDate::from_ymd_opt(2028, 2, 1).unwrap();
    assert_eq!(
        next_due(&monthly, leap).unwrap(),
        NaiveDate::from_ymd_opt(2028, 2, 29).unwrap()
    );
    let april = NaiveDate::from_ymd_opt(2026, 4, 1).unwrap();
    assert_eq!(
        next_due(&monthly, april).unwrap(),
        NaiveDate::from_ymd_opt(2026, 4, 30).unwrap()
    );
}

#[test]
fn a_same_day_deadline_is_reported_today_not_next_year() {
    let yearly = rule("y", "yearly", Some(11), Some(11));
    let today = NaiveDate::from_ymd_opt(2026, 11, 11).unwrap();
    assert_eq!(next_due(&yearly, today), Some(today));
    let day_after = NaiveDate::from_ymd_opt(2026, 11, 12).unwrap();
    assert_eq!(
        next_due(&yearly, day_after),
        NaiveDate::from_ymd_opt(2027, 11, 11),
        "a passed date rolls to next year"
    );
}

#[test]
fn quarterly_rules_only_fire_in_quarter_months() {
    let quarterly = rule("q", "quarterly", None, Some(15));
    let january = NaiveDate::from_ymd_opt(2026, 1, 20).unwrap();
    assert_eq!(
        next_due(&quarterly, january),
        NaiveDate::from_ymd_opt(2026, 4, 15),
        "January is a quarter month, so the next one is April"
    );
    let february = NaiveDate::from_ymd_opt(2026, 2, 1).unwrap();
    assert_eq!(
        next_due(&quarterly, february),
        NaiveDate::from_ymd_opt(2026, 4, 15),
        "February is not a quarter month"
    );
    let december = NaiveDate::from_ymd_opt(2026, 12, 20).unwrap();
    assert_eq!(
        next_due(&quarterly, december),
        NaiveDate::from_ymd_opt(2027, 1, 15),
        "the search crosses the year boundary"
    );
}

#[test]
fn the_region_timezone_decides_what_today_means() {
    // 2026-11-11T22:30Z is 12 November in Vietnam (UTC+7, 05:30) but still the 11th
    // in Paris (UTC+1 in November, 23:30).
    let instant = Utc
        .with_ymd_and_hms(2026, 11, 11, 22, 30, 0)
        .unwrap()
        .timestamp();
    assert_eq!(
        today_in("Asia/Ho_Chi_Minh", instant)
            .format("%Y-%m-%d")
            .to_string(),
        "2026-11-12"
    );
    assert_eq!(
        today_in("Europe/Paris", instant)
            .format("%Y-%m-%d")
            .to_string(),
        "2026-11-11"
    );
    // An unknown zone falls back to UTC instead of silently shifting dates.
    assert_eq!(
        today_in("Not/AZone", instant)
            .format("%Y-%m-%d")
            .to_string(),
        "2026-11-11",
        "UTC fallback: 22:30Z is still the 11th"
    );
}

fn seed_region(db: &db::Database, code: &str, timezone: &str, enabled: bool) {
    db.conn
        .execute(
            "INSERT OR IGNORE INTO regions(code,parent_code,label_vi,label_en,enabled,timezone) VALUES (?1,NULL,?1,?1,?2,?3)",
            params![code, enabled, timezone],
        )
        .unwrap();
}

fn seed_deadline(db: &db::Database, region: &str, code: &str, enabled: bool, verified: bool) {
    db.conn
        .execute(
            "INSERT INTO tax_deadlines(pack_key,region,source_code,code,name_vi,name_en,cadence,day,weekend_rule,lead_days,verified,enabled)
             VALUES (?1,?2,'src',?3,'Hạn mẫu','Sample','monthly',20,'next_business_day',10,?4,?5)",
            params![format!("{region}/{code}"), region, code, verified, enabled],
        )
        .unwrap();
}

#[test]
fn only_enabled_rules_of_enabled_regions_are_listed() {
    let state = crate::providers::tests_support::state_with(crate::fetch::Fetcher::public_only());
    let db = state.db.lock().unwrap();
    seed_region(&db, "vn", "Asia/Ho_Chi_Minh", true);
    seed_region(&db, "us", "America/New_York", false);
    seed_deadline(&db, "vn", "on-enabled-region", true, false);
    seed_deadline(&db, "vn", "off-rule", false, false);
    seed_deadline(&db, "us", "on-disabled-region", true, false);
    let now = Utc
        .with_ymd_and_hms(2026, 11, 1, 0, 0, 0)
        .unwrap()
        .timestamp();
    let calendar = calendar(&db, None, now, 400).unwrap();
    let codes: Vec<_> = calendar.iter().map(|d| d.code.as_str()).collect();
    assert_eq!(
        codes,
        ["on-enabled-region"],
        "a disabled rule and a rule of a disabled region must not appear"
    );
    assert!(!calendar[0].verified, "unverified rules stay flagged");
}

#[test]
fn the_disclaimer_ships_with_every_calendar() {
    assert!(DISCLAIMER_VI.contains("Không phải tư vấn pháp lý/thuế"));
    assert!(DISCLAIMER_EN.contains("Not legal or tax advice"));
}

#[tokio::test]
async fn the_calendar_endpoint_requires_a_session_and_validates_the_region() {
    let state = crate::providers::tests_support::state_with(crate::fetch::Fetcher::public_only());
    seed_region(&state.db.lock().unwrap(), "us", "America/New_York", false);
    let app = crate::router(state);
    let request = |path: &str, key: Option<&str>| {
        let mut builder = axum::http::Request::builder()
            .uri(path)
            .header(axum::http::header::HOST, "newsman.test");
        if let Some(key) = key {
            builder = builder.header(axum::http::header::AUTHORIZATION, format!("Bearer {key}"));
        }
        app.clone()
            .oneshot(builder.body(axum::body::Body::empty()).unwrap())
    };
    use tower::ServiceExt;
    assert_eq!(
        request("/api/v1/tax-calendar", None)
            .await
            .unwrap()
            .status(),
        axum::http::StatusCode::UNAUTHORIZED
    );
    let ok = request("/api/v1/tax-calendar", Some("test-api-key"))
        .await
        .unwrap();
    assert_eq!(ok.status(), axum::http::StatusCode::OK);
    let body: serde_json::Value = serde_json::from_slice(
        &http_body_util::BodyExt::collect(ok.into_body())
            .await
            .unwrap()
            .to_bytes(),
    )
    .unwrap();
    assert!(body["disclaimer_vi"].as_str().unwrap().contains("thuế"));
    assert_eq!(body["holidays_modelled"], false);
    assert_eq!(
        request("/api/v1/tax-calendar?region=us", Some("test-api-key"))
            .await
            .unwrap()
            .status(),
        axum::http::StatusCode::BAD_REQUEST,
        "a disabled region is not a valid filter"
    );
}

#[test]
fn a_pack_declares_deadlines_but_they_start_disabled_and_unverified() {
    let dir = pack_with_tax();
    let state = crate::providers::tests_support::state_with(crate::fetch::Fetcher::public_only());
    let status = crate::regions::reload_with_dir(&state, dir.path());
    assert!(status.issues.is_empty(), "{:?}", status.issues);
    let db = state.db.lock().unwrap();
    let (enabled, verified): (i64, i64) = db
        .conn
        .query_row(
            "SELECT enabled,verified FROM tax_deadlines WHERE code='sample-monthly'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!(enabled, 0, "discovery never enables a rule");
    assert_eq!(verified, 0);
}

#[test]
fn a_broken_tax_file_is_reported_and_the_rest_of_the_pack_still_loads() {
    let dir = pack_with_tax();
    dir.write("tax.toml", "[[deadline]]\ncode = ");
    let state = crate::providers::tests_support::state_with(crate::fetch::Fetcher::public_only());
    let status = crate::regions::reload_with_dir(&state, dir.path());
    assert!(status.issues.iter().any(|i| i.code == "syntax_error"));
    let db = state.db.lock().unwrap();
    let regions: i64 = db
        .conn
        .query_row("SELECT COUNT(*) FROM regions", [], |r| r.get(0))
        .unwrap();
    assert!(
        regions >= 1,
        "a broken tax file must not break region loading"
    );
    let deadlines: i64 = db
        .conn
        .query_row("SELECT COUNT(*) FROM tax_deadlines", [], |r| r.get(0))
        .unwrap();
    assert_eq!(deadlines, 0);
}

#[test]
fn invalid_deadline_rules_are_refused_one_by_one() {
    let dir = pack_with_tax();
    dir.write(
        "tax.toml",
        r#"
[[source]]
code = "src"
name_vi = "N"
name_en = "N"
url = "https://example.invalid/tax"
kind = "authority"

[[deadline]]
code = "monthly-with-month"
source = "src"
name_vi = "a"
name_en = "b"
cadence = "monthly"
month = 5
day = 10

[[deadline]]
code = "unknown-source"
source = "nope"
name_vi = "a"
name_en = "b"
cadence = "monthly"
day = 10

[[deadline]]
code = "bad-cadence"
source = "src"
name_vi = "a"
name_en = "b"
cadence = "weekly"
day = 10

[[deadline]]
code = "yearly-without-month"
source = "src"
name_vi = "a"
name_en = "b"
cadence = "yearly"
day = 10
"#,
    );
    let state = crate::providers::tests_support::state_with(crate::fetch::Fetcher::public_only());
    let status = crate::regions::reload_with_dir(&state, dir.path());
    let codes: std::collections::HashSet<_> =
        status.issues.iter().map(|i| i.code.as_str()).collect();
    assert!(codes.contains("invalid_deadline"), "{codes:?}");
    assert!(codes.contains("unknown_tax_source"), "{codes:?}");
    let db = state.db.lock().unwrap();
    let loaded: i64 = db
        .conn
        .query_row("SELECT COUNT(*) FROM tax_deadlines", [], |r| r.get(0))
        .unwrap();
    assert_eq!(loaded, 0, "every malformed rule is refused");
}

fn pack_with_tax() -> TestDir {
    let dir = TestDir::new("tax-pack");
    dir.write(
        "region.toml",
        "code = \"global\"\nname_vi = \"Toàn cầu\"\nname_en = \"Global\"\ntimezone = \"UTC\"\n",
    );
    dir.write(
        "tax.toml",
        r#"
[[source]]
code = "src"
name_vi = "Cơ quan thuế"
name_en = "Tax authority"
url = "https://example.invalid/tax"
kind = "authority"
attribution = "Source: example"

[[deadline]]
code = "sample-monthly"
source = "src"
name_vi = "Khai tháng (mẫu)"
name_en = "Monthly (sample)"
cadence = "monthly"
day = 20
weekend_rule = "next_business_day"
lead_days = 10
verified = false
"#,
    );
    dir
}

#[tokio::test]
async fn a_rule_can_only_be_enabled_when_its_region_is_on() {
    let state = crate::providers::tests_support::state_with(crate::fetch::Fetcher::public_only());
    {
        let db = state.db.lock().unwrap();
        seed_region(&db, "vn", "Asia/Ho_Chi_Minh", false);
        seed_deadline(&db, "vn", "sample", false, false);
    }
    let app = crate::router(state);
    let cookie = {
        let response = app
            .clone()
            .oneshot(
                axum::http::Request::builder()
                    .method("POST")
                    .uri("/api/auth/login")
                    .header(axum::http::header::HOST, "newsman.test")
                    .header(axum::http::header::CONTENT_TYPE, "application/json")
                    .header("x-requested-with", "XMLHttpRequest")
                    .body(axum::body::Body::from(r#"{"key":"test-management-key"}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        response
            .headers()
            .get(axum::http::header::SET_COOKIE)
            .unwrap()
            .to_str()
            .unwrap()
            .split(';')
            .next()
            .unwrap()
            .to_string()
    };
    use tower::ServiceExt;
    let put = |body: &'static str| {
        app.clone().oneshot(
            axum::http::Request::builder()
                .method("PUT")
                .uri("/api/admin/tax-rules/sample")
                .header(axum::http::header::HOST, "newsman.test")
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .header("x-requested-with", "XMLHttpRequest")
                .header(axum::http::header::COOKIE, cookie.clone())
                .body(axum::body::Body::from(body))
                .unwrap(),
        )
    };
    assert_eq!(
        put(r#"{"enabled":true}"#).await.unwrap().status(),
        axum::http::StatusCode::CONFLICT,
        "a rule cannot be enabled while its region is off"
    );
    assert_eq!(
        put(r#"{"enabled":false}"#).await.unwrap().status(),
        axum::http::StatusCode::OK,
        "disabling is always allowed"
    );
}
