//! Alert engine tests against a real SQLite database (P7 foundation).
//!
//! These cover the three promises the module makes: a rule fires **exactly once** per
//! event, the **cooldown** stops a burst and then expires, and a **quiet window** leaves
//! the event undelivered so it fires afterwards rather than being lost. No network, no
//! wall clock — every sweep is given an explicit `now`.

use super::*;
use rusqlite::params;

fn db() -> crate::db::Database {
    crate::db::Database::in_memory().unwrap()
}

/// One enabled source in `global` with one enabled topic, so articles are visible to the
/// same predicates the reader uses.
fn seed_article(db: &crate::db::Database, title: &str, topic: &str, entity: Option<&str>) -> i64 {
    db.conn
        .execute(
            "INSERT INTO topics(slug,label_vi,label_en,parent_slug,enabled) VALUES (?1,?1,?1,NULL,1)
             ON CONFLICT(slug) DO NOTHING",
            [topic],
        )
        .unwrap();
    db.conn
        .execute(
            "INSERT INTO sources(slug,name,url,tier,enabled) VALUES ('s','s','https://example.test/feed','B',1)
             ON CONFLICT(slug) DO NOTHING",
            [],
        )
        .unwrap();
    let source = db
        .conn
        .query_row("SELECT id FROM sources WHERE slug='s'", [], |r| r.get::<_, i64>(0))
        .unwrap();
    db.conn
        .execute(
            "INSERT INTO articles(canonical_url,title,source_id,topic_slug,fetched_at,region_code)
             VALUES (?1,?2,?3,?4,0,'global')",
            params![format!("https://example.test/{title}"), title, source, topic],
        )
        .unwrap();
    let id = db.conn.last_insert_rowid();
    if let Some(value) = entity {
        db.conn
            .execute(
                "INSERT INTO article_matches(article_id,kind,value,origin) VALUES (?1,'entity',?2,'rule')",
                params![id, value],
            )
            .unwrap();
    }
    id
}

fn save(db: &crate::db::Database, slug: &str, kind: &str, value: &str, quiet: (&str, &str)) {
    let input = RuleInput {
        slug: slug.into(),
        label_vi: "Nhãn".into(),
        label_en: "Label".into(),
        enabled: true,
        match_kind: kind.into(),
        match_value: value.into(),
        level: "important".into(),
        cooldown_seconds: 600,
        quiet_from: quiet.0.into(),
        quiet_to: quiet.1.into(),
        timezone: "UTC".into(),
        channels: "in-app".into(),
    };
    validate(&input).expect("valid rule");
    upsert(db, &input).unwrap();
}

#[test]
fn one_event_delivers_once_no_matter_how_often_the_sweep_runs() {
    let db = db();
    seed_article(&db, "btc jumps", "crypto", Some("BTC"));
    save(&db, "btc", "entity", "BTC", ("", ""));

    let (created, _) = sweep(&db, 1_000).unwrap();
    assert_eq!(created, 1, "the matching article delivers once");
    for now in [2_000, 5_000, 90_000] {
        let (again, _) = sweep(&db, now).unwrap();
        assert_eq!(again, 0, "sweeping again at {now} must not deliver twice");
    }
    assert_eq!(inbox(&db, false, 10).unwrap().len(), 1);
}

#[test]
fn a_second_matching_event_waits_for_the_cooldown() {
    let db = db();
    seed_article(&db, "first", "crypto", Some("BTC"));
    save(&db, "btc", "entity", "BTC", ("", ""));
    assert_eq!(sweep(&db, 1_000).unwrap().0, 1);

    seed_article(&db, "second", "crypto", Some("BTC"));
    // 100 s later: inside the 600 s cooldown, so the new article is skipped, not lost.
    let (created, skips) = sweep(&db, 1_100).unwrap();
    assert_eq!(created, 0);
    assert!(skips.contains(&("cooldown".to_string(), 1)), "skips: {skips:?}");
    // After the window the same article fires.
    let (created, _) = sweep(&db, 1_700).unwrap();
    assert_eq!(created, 1);
    assert_eq!(inbox(&db, false, 10).unwrap().len(), 2);
}

#[test]
fn quiet_hours_leave_the_event_for_after_the_window() {
    let db = db();
    seed_article(&db, "night news", "crypto", Some("BTC"));
    // Quiet 22:00-07:00 UTC. 23:00 UTC is inside it.
    save(&db, "btc", "entity", "BTC", ("22:00", "07:00"));
    let late = 23 * 3600;
    let (created, skips) = sweep(&db, late).unwrap();
    assert_eq!(created, 0);
    assert!(skips.contains(&("quiet_hours".to_string(), 1)), "skips: {skips:?}");
    // 08:00 UTC the next day is outside the window: the same article now delivers.
    let (created, _) = sweep(&db, 8 * 3600).unwrap();
    assert_eq!(created, 1, "a quiet skip must not consume the event");
    assert_eq!(unread_count(&db).unwrap(), 1);
}

#[test]
fn an_article_hidden_by_region_or_topic_never_notifies() {
    let db = db();
    // Topic disabled after the article was stored: the reader hides it, so must the rule.
    let id = seed_article(&db, "hidden", "crypto", Some("BTC"));
    db.conn
        .execute("UPDATE topics SET enabled=0 WHERE slug='crypto'", [])
        .unwrap();
    save(&db, "btc", "entity", "BTC", ("", ""));
    assert_eq!(sweep(&db, 1_000).unwrap().0, 0);
    // Re-enabling it makes the same article visible again.
    db.conn
        .execute("UPDATE topics SET enabled=1 WHERE slug='crypto'", [])
        .unwrap();
    assert_eq!(sweep(&db, 2_000).unwrap().0, 1);
    assert_eq!(inbox(&db, false, 10).unwrap()[0].article_id, id);
}

#[test]
fn a_disabled_rule_delivers_nothing_until_it_is_enabled() {
    let db = db();
    seed_article(&db, "quiet rule", "crypto", Some("BTC"));
    save(&db, "btc", "entity", "BTC", ("", ""));
    assert!(set_enabled(&db, "btc", false).unwrap());
    assert_eq!(sweep(&db, 1_000).unwrap().0, 0);
    assert!(set_enabled(&db, "btc", true).unwrap());
    assert_eq!(sweep(&db, 2_000).unwrap().0, 1);
}

#[test]
fn keyword_and_topic_rules_only_match_their_own_subject() {
    let db = db();
    seed_article(&db, "Bitcoin breaks a record", "crypto", None);
    seed_article(&db, "Wheat prices fall", "macro", None);
    save(&db, "kw", "keyword", "bitcoin", ("", ""));
    let (created, _) = sweep(&db, 1_000).unwrap();
    assert_eq!(created, 1, "only the Bitcoin title matches the keyword rule");
    let items = inbox(&db, false, 10).unwrap();
    assert!(items[0].article_title.contains("Bitcoin"));
    // Marking read moves it out of the unread inbox and back with unread=0.
    assert!(mark_read(&db, items[0].id, 1_100).unwrap());
    assert!(inbox(&db, true, 10).unwrap().is_empty());
    assert_eq!(inbox(&db, false, 10).unwrap().len(), 1);
    // Marking the same row read twice changes nothing the second time.
    assert!(!mark_read(&db, items[0].id, 1_200).unwrap());
}

#[test]
fn the_skip_log_explains_why_nothing_was_sent() {
    let db = db();
    seed_article(&db, "held back", "crypto", Some("BTC"));
    save(&db, "btc", "entity", "BTC", ("22:00", "07:00"));
    let (_, _) = sweep(&db, 23 * 3600).unwrap();
    let (rule, article, reason): (String, String, String) = db
        .conn
        .query_row(
            "SELECT r.slug, a.title, s.reason FROM alert_skip_log s
               JOIN alert_rules r ON r.id=s.rule_id JOIN articles a ON a.id=s.article_id",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .unwrap();
    assert_eq!((rule.as_str(), article.as_str(), reason.as_str()), ("btc", "held back", "quiet_hours"));
    // A successful delivery clears the stale skip row for that pair.
    let (created, _) = sweep(&db, 8 * 3600).unwrap();
    assert_eq!(created, 1);
    let left: i64 = db
        .conn
        .query_row("SELECT COUNT(*) FROM alert_skip_log", [], |r| r.get(0))
        .unwrap();
    assert_eq!(left, 0);
}

#[test]
fn an_unknown_match_kind_matches_nothing_rather_than_everything() {
    let db = db();
    seed_article(&db, "anything", "crypto", None);
    // Bypasses validate() on purpose: storage must stay safe even with a hand-edited row.
    db.conn
        .execute(
            "INSERT INTO alert_rules(slug,label_vi,label_en,enabled,match_kind,match_value,level,
                                     cooldown_seconds,timezone,channels)
             VALUES ('bad','x','x',1,'sql','1=1','info',0,'UTC','in-app')",
            [],
        )
        .unwrap();
    assert_eq!(sweep(&db, 1_000).unwrap().0, 0);
    let listed = rules(&db).unwrap();
    assert!(listed.iter().any(|r| r.slug == "bad"));
    assert!(matching_articles(&db, &listed[0], 10).unwrap().is_empty());
}

#[test]
fn inbox_limits_are_applied_newest_first() {
    let db = db();
    for i in 0..3 {
        seed_article(&db, &format!("btc {i}"), "crypto", Some("BTC"));
        save(&db, "btc", "entity", "BTC", ("", ""));
        sweep(&db, 1_000 + i * 10_000).unwrap();
    }
    let items = inbox(&db, false, 2).unwrap();
    assert_eq!(items.len(), 2);
    assert!(items[0].created_at >= items[1].created_at);
    assert_eq!(unread_count(&db).unwrap(), 3);
    assert_eq!(mark_all_read(&db, 99_000).unwrap(), 3);
    assert_eq!(unread_count(&db).unwrap(), 0);
}

// ---------------------------------------------------------------- router level

mod router {
    use super::*;
    use axum::body::Body;
    use axum::http::{Request, StatusCode, header};
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    fn state() -> crate::SharedState {
        let dir = std::env::temp_dir().join(format!("lnc-newsman-alerts-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::sync::Arc::new(crate::AppState::new(
            crate::config::Config {
                data_dir: dir.clone(),
                regions_dir: dir.join("regions"),
                bind_address: "127.0.0.1".parse().unwrap(),
                port: 8000,
                management_key: Some("test-management-key".into()),
                api_keys: vec!["test-api-key".into()],
                allowed_hosts: vec!["newsman.test".into()],
                trusted_proxies: Vec::new(),
                secure_cookie: true,
                version: "test",
            },
            crate::db::Database::in_memory().unwrap(),
            crate::fetch::Fetcher::public_only(),
        ))
    }

    async fn caller(
        app: axum::Router,
        method: &str,
        path: &str,
        body: &str,
        cookie: Option<&str>,
    ) -> (StatusCode, serde_json::Value) {
        let mut builder = Request::builder()
            .method(method)
            .uri(path)
            .header(header::HOST, "newsman.test")
            .header(header::CONTENT_TYPE, "application/json")
            .header("x-requested-with", "XMLHttpRequest");
        if let Some(cookie) = cookie {
            builder = builder.header(header::COOKIE, cookie);
        }
        let response = app
            .oneshot(builder.body(Body::from(body.to_string())).unwrap())
            .await
            .unwrap();
        let status = response.status();
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        let value = serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null);
        (status, value)
    }

    async fn session(app: axum::Router) -> String {
        let (_, _) = caller(
            app.clone(),
            "POST",
            "/api/auth/login",
            r#"{"key":"test-management-key"}"#,
            None,
        )
        .await;
        // Re-login through the same router is not idempotent for cookies, so read it here.
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

    fn rule_body(slug: &str, channels: &str) -> String {
        serde_json::json!({
            "slug": slug, "label_vi": "Nhãn", "label_en": "Label", "enabled": true,
            "match_kind": "keyword", "match_value": "bitcoin", "level": "info",
            "cooldown_seconds": 60, "timezone": "UTC", "channels": channels
        })
        .to_string()
    }

    #[tokio::test]
    async fn the_alert_api_needs_a_session_and_refuses_channels_it_cannot_send() {
        let app = crate::router(state());
        // No cookie, no API key: the inbox and the admin surface are both closed.
        let (status, _) = caller(app.clone(), "GET", "/api/alerts", "", None).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
        let cookie = session(app.clone()).await;
        let (status, _) = caller(app.clone(), "GET", "/api/alerts", "", Some(&cookie)).await;
        assert_eq!(status, StatusCode::OK);
        // A read-only API key may not administer rules.
        let (status, _) = caller(
            app.clone(),
            "POST",
            "/api/admin/alerts/rules",
            &rule_body("btc", "in-app"),
            None,
        )
        .await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
        // A channel nothing delivers to is refused, not stored and silently ignored.
        let (status, body) = caller(
            app.clone(),
            "POST",
            "/api/admin/alerts/rules",
            &rule_body("btc", "telegram"),
            Some(&cookie),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(body["error"], "channel_not_implemented");
        // The in-app channel is accepted and listed.
        let (status, _) = caller(
            app.clone(),
            "POST",
            "/api/admin/alerts/rules",
            &rule_body("btc", "in-app"),
            Some(&cookie),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        let (status, body) = caller(
            app.clone(),
            "GET",
            "/api/admin/alerts/rules",
            "",
            Some(&cookie),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["data"].as_array().unwrap().len(), 1);
        assert_eq!(body["data"][0]["slug"], "btc");
        assert_eq!(body["channels"][0], "in-app");
        // A manual sweep with nothing to match answers 200 with zero created.
        let (status, body) = caller(
            app.clone(),
            "POST",
            "/api/admin/alerts/sweep",
            "",
            Some(&cookie),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["created"], 0);
    }

    #[tokio::test]
    async fn article_payload_carries_rule_labels_for_the_reader() {
        let state = state();
        let cookie = {
            let app = crate::router(state.clone());
            session(app).await
        };
        // Store one visible article and label it the way the rule classifier does.
        {
            let db = state.db.lock().unwrap();
            let id = seed_article(&db, "Bitcoin record", "crypto", Some("BTC"));
            db.conn
                .execute(
                    "INSERT INTO article_matches(article_id,kind,value,origin) VALUES (?1,'topic','crypto','rule')",
                    params![id],
                )
                .unwrap();
        }
        let app = crate::router(state);
        let (status, body) = caller(app, "GET", "/api/articles", "", Some(&cookie)).await;
        assert_eq!(status, StatusCode::OK);
        let item = &body["data"][0];
        assert_eq!(item["title"], "Bitcoin record");
        // The article's own topic plus its entities travel with the row.
        assert_eq!(item["labels"]["topics"][0], "crypto");
        assert_eq!(item["labels"]["entities"][0], "BTC");
    }
}
