//! Region tree and pack tests. Everything runs against temporary directories and an
//! in-memory database; no network access and no live feeds.

use super::*;
use crate::{AppState, config, fetch};
use axum::{
    body::Body,
    http::{Request, StatusCode, header},
};
use http_body_util::BodyExt;
use rusqlite::params;
use std::path::Path as FsPath;
use std::sync::Arc;
use tower::ServiceExt;

pub(crate) struct TempDir(pub(crate) std::path::PathBuf);
impl TempDir {
    pub(crate) fn new(name: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "lnc-newsman-test-{}-{name}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&path).unwrap();
        Self(path)
    }
    pub(crate) fn path(&self) -> &FsPath {
        &self.0
    }
    pub(crate) fn write(&self, relative: &str, contents: &str) {
        let path = self.0.join(relative);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, contents).unwrap();
    }
}
impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn region_toml(code: &str, vi: &str, en: &str) -> String {
    format!(
        "code = \"{code}\"\nname_vi = \"{vi}\"\nname_en = \"{en}\"\ncurrency = \"EUR\"\ntimezone = \"Europe/Paris\"\nlanguage = \"fr\"\n"
    )
}

fn sources_toml(slug: &str) -> String {
    format!(
        "[[source]]\nslug = \"{slug}\"\nname = \"Mock {slug}\"\nurl = \"https://example.invalid/{slug}.xml\"\ntier = \"T3\"\nlanguage = \"fr\"\ntopic = \"finance\"\ninterval_minutes = 60\n"
    )
}

fn pack() -> TempDir {
    let dir = TempDir::new("pack");
    dir.write("region.toml", &region_toml("global", "Toàn cầu", "Global"));
    dir.write("sources.toml", &sources_toml("world"));
    dir.write("eu/region.toml", &region_toml("eu", "EU", "European Union"));
    dir.write("eu/sources.toml", &sources_toml("eu-wire"));
    dir.write("eu/fr/region.toml", &region_toml("fr", "Pháp", "France"));
    dir.write("eu/fr/sources.toml", &sources_toml("fr-wire"));
    dir.write(
        "us/region.toml",
        &region_toml("us", "Hoa Kỳ", "United States"),
    );
    dir
}

fn state_with(dir: &FsPath) -> SharedState {
    Arc::new(AppState::new(
        config::Config {
            data_dir: dir.to_path_buf(),
            regions_dir: dir.to_path_buf(),
            bind_address: "127.0.0.1".parse().unwrap(),
            port: 8000,
            management_key: Some("test-management-key".into()),
            api_keys: vec!["test-api-key".into()],
            allowed_hosts: vec!["newsman.test".into()],
            trusted_proxies: Vec::new(),
            secure_cookie: true,
            version: "test",
        },
        db::Database::in_memory().unwrap(),
        fetch::Fetcher::public_only(),
    ))
}

fn region(db: &db::Database, code: &str) -> (Option<String>, i64) {
    db.conn
        .query_row(
            "SELECT parent_code, enabled FROM regions WHERE code=?1",
            [code],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap()
}

fn set_region(db: &db::Database, code: &str, enabled: bool) {
    db.conn
        .execute(
            "UPDATE regions SET enabled=?2 WHERE code=?1",
            params![code, enabled],
        )
        .unwrap();
}

#[test]
fn scan_builds_the_tree_and_registers_everything_disabled() {
    let dir = pack();
    let scan = scan(dir.path());
    assert!(
        scan.issues.is_empty(),
        "unexpected issues: {:?}",
        scan.issues
    );
    let codes: Vec<_> = scan.regions.iter().map(|r| r.code.as_str()).collect();
    assert_eq!(codes, ["global", "eu", "fr", "us"]);
    assert_eq!(scan.regions[1].parent, "global");
    assert_eq!(scan.regions[2].parent, "eu");
    assert_eq!(scan.regions[2].label_vi, "Pháp");
    assert_eq!(scan.regions[2].sources[0].slug, "fr-wire");

    let state = state_with(dir.path());
    let status = reload(&state);
    assert!(status.issues.is_empty(), "{:?}", status.issues);
    let db = state.db.lock().unwrap();
    assert_eq!(region(&db, "global"), (None, 1), "global is always on");
    assert_eq!(region(&db, "eu"), (Some("global".into()), 0));
    assert_eq!(region(&db, "fr"), (Some("eu".into()), 0));
    let enabled_sources: i64 = db
        .conn
        .query_row("SELECT COUNT(*) FROM sources WHERE enabled=1", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(enabled_sources, 0, "detection must not activate sources");
    let pack_sources: i64 = db
        .conn
        .query_row(
            "SELECT COUNT(*) FROM sources WHERE pack_key IS NOT NULL AND region='fr'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(pack_sources, 1);
}

#[test]
fn a_source_pointing_at_an_unknown_region_falls_back_to_global() {
    let dir = pack();
    let state = state_with(dir.path());
    reload(&state);
    let db = state.db.lock().unwrap();
    // Simulates a hand edit (or an old P1 row) naming a region that does not exist.
    db.conn
        .execute(
            "INSERT INTO sources(slug,name,url,tier,enabled,region) VALUES ('stray','Stray','https://example.invalid/stray','T2',1,'nowhere')",
            [],
        )
        .unwrap();
    let id = db.conn.last_insert_rowid();
    db.conn
        .execute(
            "INSERT INTO articles(canonical_url,title,source_id,region_code,fetched_at) VALUES ('https://example.invalid/stray','Stray item',?1,'nowhere',0)",
            [id],
        )
        .unwrap();
    drop(db);
    reload(&state);
    let db = state.db.lock().unwrap();
    let region: String = db
        .conn
        .query_row("SELECT region FROM sources WHERE slug='stray'", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(region, "global");
    let visible = crate::collector::list_articles(&db, None, None, None, 50, 0).unwrap();
    assert_eq!(
        visible.len(),
        1,
        "the item must not disappear from the reader"
    );
}

#[test]
fn a_pack_source_keeps_its_required_credit_line_and_starts_disabled() {
    let dir = pack();
    dir.write("vn/region.toml", &region_toml("vn", "Việt Nam", "Vietnam"));
    dir.write(
        "vn/sources.toml",
        "[[source]]
slug = \"vnexpress\"
name = \"VnExpress\"
url = \"https://vnexpress.net/rss/kinh-doanh.rss\"
tier = \"T3\"
language = \"vi\"
topic = \"finance\"
interval_minutes = 60
attribution = \"Nguồn: VnExpress\"
",
    );
    let state = state_with(dir.path());
    let status = reload(&state);
    assert!(status.issues.is_empty(), "{:?}", status.issues);
    let db = state.db.lock().unwrap();
    let (enabled, attribution, region): (i64, String, String) = db
        .conn
        .query_row(
            "SELECT enabled,attribution,region FROM sources WHERE url LIKE '%vnexpress%'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .unwrap();
    assert_eq!(enabled, 0, "a pack source is never activated by discovery");
    assert_eq!(attribution, "Nguồn: VnExpress");
    assert_eq!(region, "vn");
}

#[test]
fn the_builtin_vn_pack_ships_only_disabled_feeds_and_records_the_terms() {
    let sources = include_str!("../assets/regions/vn/sources.toml");
    // Terms must be recorded next to the feed, so an owner can re-read them before
    // enabling it. The pack format has no "enabled" key at all: every pack source is
    // registered off, so a feed can only be switched on from the UI.
    assert!(sources.contains("VnExpress"));
    assert!(sources.contains("Terms that must be re-read"));
    assert!(sources.contains("vnexpress.net/rss/"));
    // A guessed URL must never ship for a publisher whose documentation is unconfirmed.
    assert!(
        !sources.contains("vneconomy") && !sources.contains("thanhnien"),
        "unconfirmed Vietnamese feeds stay out of the pack"
    );
    let dir = pack();
    dir.write("vn/sources.toml", sources);
    let scan = scan(dir.path());
    let vn = scan.regions.iter().find(|r| r.code == "vn");
    assert!(vn.is_none_or(|r| r.sources.iter().all(|s| !s.attribution.is_empty())));
}

#[test]
fn effective_set_is_enabled_regions_plus_all_ancestors() {
    let dir = pack();
    let state = state_with(dir.path());
    reload(&state);
    {
        let db = state.db.lock().unwrap();
        assert_eq!(effective(&db, None).unwrap(), vec!["global"]);
        set_region(&db, "eu", true);
        assert_eq!(effective(&db, None).unwrap(), vec!["eu", "global"]);
        set_region(&db, "fr", true);
        assert_eq!(effective(&db, None).unwrap(), vec!["eu", "fr", "global"]);
        assert_eq!(
            effective(&db, Some("fr")).unwrap(),
            vec!["eu", "fr", "global"],
            "choosing fr must include eu and global"
        );
        assert_eq!(effective(&db, Some("us")).unwrap(), vec!["global", "us"]);
    }
}

#[test]
fn a_new_nested_directory_is_detected_but_stays_disabled() {
    let dir = pack();
    let state = state_with(dir.path());
    reload(&state);
    {
        let db = state.db.lock().unwrap();
        set_region(&db, "eu", true);
    }
    dir.write("asia/region.toml", &region_toml("asia", "Châu Á", "Asia"));
    dir.write(
        "asia/sg/region.toml",
        &region_toml("sg", "Singapore", "Singapore"),
    );
    dir.write("asia/sg/sources.toml", &sources_toml("sg-wire"));
    let status = reload(&state);
    assert!(status.issues.is_empty(), "{:?}", status.issues);
    let db = state.db.lock().unwrap();
    assert_eq!(region(&db, "asia"), (Some("global".into()), 0));
    assert_eq!(
        region(&db, "sg"),
        (Some("asia".into()), 0),
        "sg is a child of asia, not of global"
    );
    let (enabled, pack_key): (i64, String) = db
        .conn
        .query_row(
            "SELECT enabled,pack_key FROM sources WHERE url LIKE '%sg-wire%'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!((enabled, pack_key.as_str()), (0, "sg/sg-wire"));
}

#[test]
fn bad_files_are_skipped_and_do_not_disturb_other_regions() {
    let dir = pack();
    // Broken TOML, a code that does not match its folder, an invalid URL, and an
    // orphan region whose parent folder has no region.toml.
    dir.write("eu/fr/region.toml", "code = \"fr\"\nname_vi = ");
    dir.write(
        "us/region.toml",
        &region_toml("usa", "Hoa Kỳ", "United States"),
    );
    dir.write("vn/region.toml", &region_toml("vn", "Việt Nam", "Vietnam"));
    dir.write(
        "vn/sources.toml",
        "[[source]]\nslug = \"bad\"\nname = \"Bad\"\nurl = \"ftp://example.invalid/x\"\ntier = \"T2\"\ninterval_minutes = 60\n",
    );
    dir.write(
        "latam/region.toml",
        "code = \"latam\"
name_vi = ",
    );
    dir.write("asia/sources.toml", &sources_toml("orphan-wire"));
    dir.write(
        "asia/sea/region.toml",
        &region_toml("sea", "Đông Nam Á", "SE Asia"),
    );
    let state = state_with(dir.path());
    let status = reload(&state);
    let codes: HashSet<_> = status.issues.iter().map(|i| i.code.as_str()).collect();
    for expected in [
        "syntax_error",
        "code_mismatch",
        "invalid_source",
        "missing_region_file",
        "region_orphan",
    ] {
        assert!(codes.contains(expected), "missing {expected}: {codes:?}");
    }
    let db = state.db.lock().unwrap();
    assert_eq!(region(&db, "eu").0, Some("global".into()));
    assert!(
        !db.conn
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM regions WHERE code='us')",
                [],
                |r| r.get::<_, bool>(0)
            )
            .unwrap(),
        "a region whose declared code does not match its folder is rejected"
    );
    assert_eq!(
        region(&db, "vn").0,
        Some("global".into()),
        "vn region itself is valid even when its sources file is not"
    );
    assert!(
        db.conn
            .query_row(
                "SELECT COUNT(*) FROM regions WHERE code IN ('fr','usa','sea','latam')",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap()
            == 0,
        "rejected regions must not be registered"
    );
}

#[test]
fn duplicate_codes_and_cycles_are_rejected() {
    let dir = pack();
    dir.write("us2/region.toml", &region_toml("eu", "Trùng", "Duplicate"));
    let state = state_with(dir.path());
    reload(&state);
    assert!(
        state
            .pack
            .lock()
            .unwrap()
            .issues
            .iter()
            .any(|i| i.code == "code_mismatch")
    );

    // A cycle can only come from an earlier database state (for example a hand edit),
    // so plant one directly and verify that the whole apply is refused.
    {
        let db = state.db.lock().unwrap();
        db.conn
            .execute(
                "INSERT INTO regions(code,parent_code,label_vi,label_en,enabled) VALUES ('a','b','A','A',0),('b','a','B','B',0)",
                [],
            )
            .unwrap();
    }
    let status = reload(&state);
    assert!(status.issues.iter().any(|i| i.code == "region_cycle"));
    assert_eq!(
        status.regions_registered, 0,
        "a cycle must abort the whole apply, not partially register"
    );
}

#[test]
fn a_failed_reload_keeps_the_previous_valid_configuration() {
    let dir = pack();
    let state = state_with(dir.path());
    reload(&state);
    {
        let db = state.db.lock().unwrap();
        set_region(&db, "eu", true);
        set_region(&db, "fr", true);
        db.conn
            .execute("UPDATE sources SET enabled=1 WHERE region='fr'", [])
            .unwrap();
    }
    dir.write("eu/fr/region.toml", "this is not toml =");
    let status = reload(&state);
    assert!(status.issues.iter().any(|i| i.code == "syntax_error"));
    let db = state.db.lock().unwrap();
    assert_eq!(
        region(&db, "fr"),
        (Some("eu".into()), 1),
        "an unreadable region keeps its last valid row and enabled state"
    );
    let enabled: i64 = db
        .conn
        .query_row(
            "SELECT COUNT(*) FROM sources WHERE region='fr' AND enabled=1",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(enabled, 1);
}

#[test]
fn symlinked_directories_are_skipped() {
    let dir = pack();
    let outside = TempDir::new("outside");
    outside.write("region.toml", &region_toml("evil", "Evil", "Evil"));
    outside.write("sources.toml", &sources_toml("evil-wire"));
    let link = dir.path().join("evil");
    #[cfg(windows)]
    let created = std::os::windows::fs::symlink_dir(outside.path(), &link).is_ok();
    #[cfg(not(windows))]
    let created = std::os::unix::fs::symlink(outside.path(), &link).is_ok();
    if !created {
        // Windows only allows directory symlinks with developer mode or elevation.
        return;
    }
    let state = state_with(dir.path());
    let status = reload(&state);
    assert!(
        status.issues.iter().any(|i| i.code == "symlink_skipped"),
        "{:?}",
        status.issues
    );
    let db = state.db.lock().unwrap();
    let evil: i64 = db
        .conn
        .query_row("SELECT COUNT(*) FROM regions WHERE code='evil'", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(evil, 0, "a symlink must never become a region");
}

#[test]
fn reader_region_filter_includes_ancestors_and_hides_disabled_regions() {
    let dir = pack();
    let state = state_with(dir.path());
    reload(&state);
    {
        let db = state.db.lock().unwrap();
        set_region(&db, "eu", true);
        set_region(&db, "fr", true);
        for (url, source_slug, region_code) in [
            ("https://example.invalid/eu", "eu-src", "eu"),
            ("https://example.invalid/fr", "fr-src", "fr"),
            ("https://example.invalid/us", "us-src", "us"),
            ("https://example.invalid/w", "w-src", "global"),
        ] {
            db.conn
                .execute(
                    "INSERT INTO sources(slug,name,url,tier,enabled,region) VALUES (?1,?1,?2,'T2',1,?3)",
                    params![source_slug, url, region_code],
                )
                .unwrap();
            let id = db.conn.last_insert_rowid();
            db.conn
                .execute(
                    "INSERT INTO articles(canonical_url,title,source_id,region_code,fetched_at) VALUES (?1,?1,?2,?3,0)",
                    params![url, id, region_code],
                )
                .unwrap();
        }
        let articles = crate::collector::list_articles(&db, None, None, None, 50, 0).unwrap();
        let regions: HashSet<_> = articles
            .iter()
            .map(|a| a.region_code.clone().unwrap())
            .collect();
        assert_eq!(
            regions,
            HashSet::from(["global".to_string(), "eu".into(), "fr".into()]),
            "items from the disabled us region must be hidden"
        );
        let fr = crate::collector::list_articles(&db, None, Some("fr"), None, 50, 0).unwrap();
        assert_eq!(fr.len(), 3, "fr also returns eu and global items");
        let us = crate::collector::list_articles(&db, None, Some("us"), None, 50, 0).unwrap();
        assert!(
            us.is_empty(),
            "a disabled region is not a valid filter target"
        );
    }
}

#[test]
fn dashboard_counters_match_the_region_filtered_reader() {
    let dir = pack();
    let state = state_with(dir.path());
    reload(&state);
    let db = state.db.lock().unwrap();
    set_region(&db, "eu", true);
    set_region(&db, "fr", true);
    db.conn
        .execute(
            "INSERT INTO topics(slug,label_vi,label_en,enabled) VALUES ('archive','Lưu trữ','Archive',0)",
            [],
        )
        .unwrap();
    let now = crate::now() as i64;
    for (index, region_code, topic) in [
        (0, "global", "finance"),
        (1, "eu", "finance"),
        (2, "fr", "finance"),
        (3, "us", "finance"),
        (4, "vn", "finance"),
        (5, "fr", "archive"),
    ] {
        db.conn
            .execute(
                "INSERT INTO sources(slug,name,url,tier,enabled,region) VALUES (?1,?1,?2,'T2',1,?3)",
                params![format!("src-{index}"), format!("https://example.invalid/{index}"), region_code],
            )
            .unwrap();
        let source_id = db.conn.last_insert_rowid();
        db.conn
            .execute(
                "INSERT INTO articles(canonical_url,title,source_id,region_code,topic_slug,fetched_at)
                 VALUES (?1,?1,?2,?3,?4,?5)",
                params![
                    format!("https://example.invalid/item-{index}"),
                    source_id,
                    region_code,
                    topic,
                    now
                ],
            )
            .unwrap();
    }
    let counters = crate::collector::stats(&db, None).unwrap();
    let listed = crate::collector::list_articles(&db, None, None, None, 100, 0).unwrap();
    assert_eq!(
        counters.articles_total,
        listed.len() as i64,
        "the dashboard total must equal what the reader shows"
    );
    assert_eq!(counters.articles_total, 3, "eu, fr and global only");
    assert_eq!(counters.articles_24h, 3);
    assert_eq!(counters.topic_counts.get("finance"), Some(&3));
    assert!(
        !counters.topic_counts.contains_key("archive"),
        "items under a disabled topic are hidden from both list and counts"
    );
    // Source counters stay global: all six hand-added sources are enabled even though
    // two of their regions are off (the source table warns about each of them).
    assert_eq!(counters.sources_enabled, 6);
    drop(db);
    // The API refuses to disable a parent while a child is on, but the counters must
    // still follow the effective set if the rows are changed directly.
    set_region(&state.db.lock().unwrap(), "fr", false);
    let db = state.db.lock().unwrap();
    assert_eq!(
        crate::collector::stats(&db, None).unwrap().articles_total,
        2
    );
    drop(db);
    set_region(&state.db.lock().unwrap(), "eu", false);
    let db = state.db.lock().unwrap();
    let counters = crate::collector::stats(&db, None).unwrap();
    assert_eq!(counters.articles_total, 1, "only the global item remains");
    assert_eq!(counters.topic_counts.get("finance"), Some(&1));
    assert_eq!(
        crate::collector::list_articles(&db, None, None, None, 100, 0)
            .unwrap()
            .len(),
        1,
        "counters and the reader always agree"
    );
}

#[tokio::test]
async fn dashboard_region_query_follows_the_picker() {
    let dir = pack();
    let state = state_with(dir.path());
    reload(&state);
    {
        let db = state.db.lock().unwrap();
        set_region(&db, "eu", true);
        set_region(&db, "fr", true);
    }
    let app = crate::router(state);
    let cookie = login_cookie(app.clone()).await;
    let get = |path: &str| {
        app.clone().oneshot(
            Request::builder()
                .uri(path)
                .header(header::HOST, "newsman.test")
                .header(header::COOKIE, cookie.as_str())
                .body(Body::empty())
                .unwrap(),
        )
    };
    let all = get("/api/dashboard").await.unwrap();
    assert_eq!(all.status(), StatusCode::OK);
    let value: serde_json::Value =
        serde_json::from_slice(&all.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(value["stats"]["articles_total"], 0);
    let fr = get("/api/dashboard?region=fr").await.unwrap();
    assert_eq!(fr.status(), StatusCode::OK);
    let value: serde_json::Value =
        serde_json::from_slice(&fr.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert!(value["stats"].get("topic_counts").is_some());
    assert_eq!(
        get("/api/dashboard?region=us").await.unwrap().status(),
        StatusCode::BAD_REQUEST,
        "a disabled region is not a valid counter filter"
    );
    assert_eq!(
        get("/api/dashboard?region=FR").await.unwrap().status(),
        StatusCode::OK,
        "the region code is case-insensitive"
    );
    assert_eq!(
        get("/api/dashboard?region=US").await.unwrap().status(),
        StatusCode::BAD_REQUEST,
        "case folding must not make a disabled region valid"
    );
}

#[test]
fn scheduler_skips_sources_in_disabled_regions() {
    let dir = pack();
    let state = state_with(dir.path());
    reload(&state);
    state
        .db
        .lock()
        .unwrap()
        .set_settings(&[("collector_enabled", "true")])
        .unwrap();
    assert!(
        crate::collector::due_sources(&state).unwrap().is_empty(),
        "pack sources are registered disabled and no region is on"
    );
    state
        .db
        .lock()
        .unwrap()
        .conn
        .execute(
            "UPDATE sources SET enabled=1, next_fetch_at=NULL WHERE region='fr'",
            [],
        )
        .unwrap();
    assert!(
        crate::collector::due_sources(&state).unwrap().is_empty(),
        "an enabled source in a disabled region must not be fetched"
    );
    set_region(&state.db.lock().unwrap(), "eu", true);
    assert!(
        crate::collector::due_sources(&state).unwrap().is_empty(),
        "enabling the parent alone does not enable its children"
    );
    set_region(&state.db.lock().unwrap(), "fr", true);
    let due = crate::collector::due_sources(&state).unwrap();
    assert_eq!(due.len(), 1, "fr is enabled, so its source becomes due");
}

#[tokio::test]
async fn region_administration_requires_an_admin_session() {
    let dir = pack();
    let state = state_with(dir.path());
    reload(&state);
    let app = crate::router(state);
    let request =
        |method: &str, path: &str, body: &str, key: Option<&str>, cookie: Option<&str>| {
            let mut builder = Request::builder()
                .method(method)
                .uri(path)
                .header(header::HOST, "newsman.test")
                .header(header::CONTENT_TYPE, "application/json")
                .header("x-requested-with", "XMLHttpRequest");
            if let Some(key) = key {
                builder = builder.header(header::AUTHORIZATION, format!("Bearer {key}"));
            }
            if let Some(cookie) = cookie {
                builder = builder.header(header::COOKIE, cookie);
            }
            app.clone()
                .oneshot(builder.body(Body::from(body.to_string())).unwrap())
        };
    assert_eq!(
        request("GET", "/api/admin/regions", "", Some("test-api-key"), None)
            .await
            .unwrap()
            .status(),
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        request(
            "POST",
            "/api/admin/regions/reload",
            "",
            Some("test-api-key"),
            None
        )
        .await
        .unwrap()
        .status(),
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        request(
            "PUT",
            "/api/admin/regions/eu",
            r#"{"enabled":true}"#,
            Some("test-api-key"),
            None
        )
        .await
        .unwrap()
        .status(),
        StatusCode::UNAUTHORIZED
    );

    let login = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/auth/login")
                .header(header::HOST, "newsman.test")
                .header(header::CONTENT_TYPE, "application/json")
                .header("x-requested-with", "XMLHttpRequest")
                .body(Body::from(r#"{"key":"test-management-key"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(login.status(), StatusCode::OK);
    let cookie = login
        .headers()
        .get(header::SET_COOKIE)
        .unwrap()
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_string();
    let response = request(
        "PUT",
        "/api/admin/regions/fr",
        r#"{"enabled":true}"#,
        None,
        Some(&cookie),
    )
    .await
    .unwrap();
    assert_eq!(
        response.status(),
        StatusCode::CONFLICT,
        "fr cannot be enabled before eu"
    );
    assert_eq!(
        request(
            "PUT",
            "/api/admin/regions/eu",
            r#"{"enabled":true}"#,
            None,
            Some(&cookie)
        )
        .await
        .unwrap()
        .status(),
        StatusCode::OK
    );
    assert_eq!(
        request(
            "PUT",
            "/api/admin/regions/fr",
            r#"{"enabled":true}"#,
            None,
            Some(&cookie)
        )
        .await
        .unwrap()
        .status(),
        StatusCode::OK
    );
    let response = request(
        "PUT",
        "/api/admin/regions/eu",
        r#"{"enabled":false}"#,
        None,
        Some(&cookie),
    )
    .await
    .unwrap();
    assert_eq!(
        response.status(),
        StatusCode::CONFLICT,
        "eu cannot be disabled while fr is on"
    );
    let response = request("POST", "/api/admin/regions/reload", "", None, Some(&cookie))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let value: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(value["data"]["issues"].as_array().unwrap().len(), 0);
    let response = request(
        "PUT",
        "/api/admin/regions/global",
        r#"{"enabled":false}"#,
        None,
        Some(&cookie),
    )
    .await
    .unwrap();
    assert_eq!(response.status(), StatusCode::CONFLICT);
}

#[tokio::test]
async fn api_key_reads_only_enabled_regions() {
    let dir = pack();
    let state = state_with(dir.path());
    reload(&state);
    set_region(&state.db.lock().unwrap(), "eu", true);
    let app = crate::router(state);
    let get = |path: &str, key: Option<&str>| {
        let mut builder = Request::builder()
            .uri(path)
            .header(header::HOST, "newsman.test");
        if let Some(key) = key {
            builder = builder.header(header::AUTHORIZATION, format!("Bearer {key}"));
        }
        app.clone().oneshot(builder.body(Body::empty()).unwrap())
    };
    assert_eq!(
        get("/api/v1/regions", None).await.unwrap().status(),
        StatusCode::UNAUTHORIZED
    );
    let response = get("/api/v1/regions", Some("test-api-key")).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let value: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let codes: Vec<_> = value["data"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| row["code"].as_str().unwrap())
        .collect();
    assert_eq!(
        codes,
        ["global", "eu"],
        "disabled regions must stay hidden; the list follows the tree order"
    );
    assert_eq!(value["effective"], serde_json::json!(["eu", "global"]));
    assert!(
        value.get("pack").is_none(),
        "pack issues are admin-only and must not leak to API consumers"
    );
}

#[tokio::test]
async fn pack_sources_cannot_be_edited_only_toggled() {
    let dir = pack();
    let state = state_with(dir.path());
    reload(&state);
    let id: i64 = state
        .db
        .lock()
        .unwrap()
        .conn
        .query_row(
            "SELECT id FROM sources WHERE pack_key='fr/fr-wire'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    let app = crate::router(state.clone());
    let cookie = login_cookie(app.clone()).await;
    let row = serde_json::json!({
        "slug": "pack-fr-fr-wire", "name": "Renamed", "url": "https://example.invalid/fr-wire.xml",
        "tier": "T3", "enabled": false, "language": "fr", "region": "fr",
        "topic_slug": "finance", "fetch_interval_minutes": 60
    });
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri(format!("/api/admin/sources/{id}"))
                .header(header::HOST, "newsman.test")
                .header(header::CONTENT_TYPE, "application/json")
                .header("x-requested-with", "XMLHttpRequest")
                .header(header::COOKIE, cookie.as_str())
                .body(Body::from(row.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::CONFLICT);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let value: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(value["error"], "source_managed_by_pack");
}

async fn login_cookie(app: axum::Router) -> String {
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/auth/login")
                .header(header::HOST, "newsman.test")
                .header(header::CONTENT_TYPE, "application/json")
                .header("x-requested-with", "XMLHttpRequest")
                .body(Body::from(r#"{"key":"test-management-key"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    response
        .headers()
        .get(header::SET_COOKIE)
        .unwrap()
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_string()
}

#[test]
fn migration_five_preserves_sources_and_articles() {
    let dir = TempDir::new("migration");
    let path = dir.path().join("legacy.db");
    {
        let conn = rusqlite::Connection::open(&path).unwrap();
        conn.execute_batch(include_str!("../migrations/0001_init.sql"))
            .unwrap();
        conn.execute_batch(include_str!("../migrations/0002_sessions.sql"))
            .unwrap();
        conn.execute_batch(
            "CREATE TABLE schema_migrations (version INTEGER PRIMARY KEY, applied_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP);",
        )
        .unwrap();
        for (version, sql) in [
            (3, include_str!("../migrations/0003_source_catalog.sql")),
            (4, include_str!("../migrations/0004_collector.sql")),
        ] {
            conn.execute_batch(sql).unwrap();
            conn.execute(
                "INSERT INTO schema_migrations(version) VALUES (?1)",
                [version],
            )
            .unwrap();
        }
        conn.execute(
            "INSERT INTO sources(id,slug,name,url,tier,enabled,region) VALUES (1,'legacy','Legacy','https://example.invalid/feed','T2',1,'')",
            [],
        )
        .unwrap();
        // P1 accepted free text here, so legacy rows look like this.
        conn.execute(
            "INSERT INTO sources(id,slug,name,url,tier,enabled,region) VALUES (2,'free-text','Free text','https://example.invalid/fr','T2',1,'France')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO articles(canonical_url,title,source_id,guid,fetched_at) VALUES ('https://example.invalid/b','Free-text item',2,'g2',1)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO articles(canonical_url,title,source_id,guid,fetched_at) VALUES ('https://example.invalid/a','Old item',1,'g1',1)",
            [],
        )
        .unwrap();
    }
    let db = db::Database::open(&path).unwrap();
    let (version, region, pack_key): (String, Option<String>, Option<String>) = db
        .conn
        .query_row(
            "SELECT region, region, pack_key FROM sources WHERE slug='legacy'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .unwrap();
    assert_eq!(version, "global", "legacy sources are adopted into global");
    assert_eq!(region.as_deref(), Some("global"));
    assert_eq!(pack_key, None);
    let article_region: String = db
        .conn
        .query_row(
            "SELECT region_code FROM articles WHERE canonical_url='https://example.invalid/a'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(article_region, "global");
    let applied: i64 = db
        .conn
        .query_row(
            "SELECT COUNT(*) FROM schema_migrations WHERE version=5",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(applied, 1);
    let free_text: String = db
        .conn
        .query_row(
            "SELECT region FROM sources WHERE slug='free-text'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(
        free_text, "global",
        "free text from P1 must not become an unknown region code"
    );
    let free_article: String = db
        .conn
        .query_row(
            "SELECT region_code FROM articles WHERE canonical_url='https://example.invalid/b'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(free_article, "global");
    let visible = crate::collector::list_articles(&db, None, None, None, 50, 0).unwrap();
    assert_eq!(
        visible.len(),
        2,
        "legacy items must still be readable after migration"
    );
}

#[test]
fn seeding_adds_new_builtin_files_but_never_overwrites_owner_edits() {
    let dir = TempDir::new("seed");
    seed_builtin(dir.path()).unwrap();
    // An owner edits one file and deletes nothing.
    let tax = dir.path().join("tax.toml");
    assert!(tax.exists(), "P3 files are seeded on a fresh directory");
    dir.write("tax.toml", "# owner edit\n");
    // A later release adds a file; simulate it by removing one and re-seeding.
    std::fs::remove_file(dir.path().join("official.toml")).unwrap();
    seed_builtin(dir.path()).unwrap();
    assert!(
        dir.path().join("official.toml").exists(),
        "a missing built-in file is recreated on an existing install"
    );
    assert_eq!(
        std::fs::read_to_string(&tax).unwrap(),
        "# owner edit\n",
        "an existing file is never overwritten"
    );
}
