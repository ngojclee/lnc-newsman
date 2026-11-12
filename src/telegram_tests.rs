//! Telegram channel tests: configuration resolution, the outbox's claim-once rule, and
//! the message the relay would receive.
//!
//! No network and no wall clock: the send path is covered by shaping the request body
//! and parsing the answers Telegram and the relay are known to give.

use super::*;
use crate::db::Database;

fn db() -> Database {
    Database::in_memory().unwrap()
}

fn vault() -> secrets::SecretBox {
    secrets::SecretBox::from_bytes([7u8; 32], None)
}

fn cfg() -> TelegramConfig {
    TelegramConfig::default()
}

fn set(db: &Database, key: &str, value: &str) {
    db.conn
        .execute(
            "INSERT INTO settings(key,value) VALUES (?1,?2)
             ON CONFLICT(key) DO UPDATE SET value=excluded.value",
            params![key, value],
        )
        .unwrap();
}

/// One rule + article + delivery row, so outbox rows have a real subject.
fn delivery(db: &Database, title: &str, url: &str, now: i64) -> i64 {
    db.conn
        .execute(
            "INSERT INTO topics(slug,label_vi,label_en,enabled) VALUES ('t','t','t',1)
             ON CONFLICT(slug) DO NOTHING",
            [],
        )
        .unwrap();
    db.conn
        .execute(
            "INSERT INTO sources(slug,name,url,tier,enabled) VALUES ('s','s','https://example.test/f','B',1)
             ON CONFLICT(slug) DO NOTHING",
            [],
        )
        .unwrap();
    let source: i64 = db
        .conn
        .query_row("SELECT id FROM sources WHERE slug='s'", [], |r| r.get(0))
        .unwrap();
    db.conn
        .execute(
            "INSERT INTO articles(canonical_url,title,source_id,topic_slug,fetched_at,region_code)
             VALUES (?1,?2,?3,'t',0,'global')",
            params![url, title, source],
        )
        .unwrap();
    let article = db.conn.last_insert_rowid();
    db.conn
        .execute(
            "INSERT INTO alert_rules(slug,label_vi,label_en,enabled,match_kind,match_value)
             VALUES ('r','r','r',1,'keyword','x') ON CONFLICT(slug) DO NOTHING",
            [],
        )
        .unwrap();
    let rule: i64 = db
        .conn
        .query_row("SELECT id FROM alert_rules WHERE slug='r'", [], |r| r.get(0))
        .unwrap();
    db.conn
        .execute(
            "INSERT INTO alert_deliveries(rule_id,article_id,level,channel,title,body,created_at)
             VALUES (?1,?2,'info','in-app',?3,?4,?5)",
            params![rule, article, title, url, now],
        )
        .unwrap();
    db.conn.last_insert_rowid()
}

#[test]
fn off_by_default_and_env_wins_over_the_stored_setting() {
    let db = db();
    let config = cfg();
    assert_eq!(stored_mode(&db), Mode::Off);
    assert_eq!(effective_mode(&db, &config), Mode::Off);

    set(&db, MODE_KEY, "relay");
    assert_eq!(effective_mode(&db, &config), Mode::Relay);

    // An operator setting `TELEGRAM_MODE=off` in the stack wins: env_or_cfg, env first.
    let env_off = TelegramConfig {
        mode: Some(Mode::Off),
        ..config
    };
    assert_eq!(effective_mode(&db, &env_off), Mode::Off);
}

#[test]
fn an_endpoint_names_exactly_what_is_missing() {
    let db = db();
    let mut config = cfg();
    let vault = vault();
    set(&db, MODE_KEY, "relay");
    assert_eq!(
        endpoint(&db, Some(&vault), &config).unwrap_err(),
        "telegram_chat_id_missing"
    );
    set(&db, CHAT_ID_KEY, "-1001234567890");
    assert_eq!(
        endpoint(&db, Some(&vault), &config).unwrap_err(),
        "telegram_secret_missing"
    );
    let token = "relay-token-not-a-real-one";
    secrets::set(&db, &vault, SECRET_RELAY, token).unwrap();
    let resolved = endpoint(&db, Some(&vault), &config).unwrap();
    assert_eq!(resolved.mode, Mode::Relay);
    assert_eq!(resolved.url, "https://telegram.lengoc.me/send");
    assert_eq!(resolved.chat_id, -1_001_234_567_890);
    assert_eq!(resolved.topic_id, None);

    // A relay that is not https is refused before any request is made.
    config.relay_base_url = "http://telegram.lengoc.me".into();
    assert_eq!(
        endpoint(&db, Some(&vault), &config).unwrap_err(),
        "telegram_bad_relay_url"
    );
    // https on a host nobody allowed is refused too — the allowlist is the point.
    config.relay_base_url = "https://elsewhere.example".into();
    assert_eq!(
        endpoint(&db, Some(&vault), &config).unwrap_err(),
        "telegram_relay_host_not_allowed"
    );
}

#[test]
fn direct_mode_uses_the_bot_api_url_and_never_echoes_the_token() {
    let db = db();
    let vault = vault();
    let config = cfg();
    set(&db, MODE_KEY, "direct");
    set(&db, CHAT_ID_KEY, "-100");
    secrets::set(&db, &vault, SECRET_BOT, "123456:AA-bot-token").unwrap();
    let resolved = endpoint(&db, Some(&vault), &config).unwrap();
    assert_eq!(resolved.mode, Mode::Direct);
    assert!(resolved.url.ends_with("/sendMessage"), "{}", resolved.url);
    assert_eq!(resolved.bearer, None, "no bearer header for the Bot API");
    // The Debug view is used for logs; it must not carry the token-only URL around.
    let shown = format!("{resolved:?}");
    assert!(!shown.contains("AA-bot-token"), "token leaked: {shown}");
}

#[test]
fn the_relay_body_is_a_bot_api_call_with_the_topic() {
    let body = relay_body(-100, Some(42), "hello");
    assert_eq!(body["method"], "sendMessage");
    assert_eq!(body["payload"]["chat_id"], -100);
    assert_eq!(body["payload"]["message_thread_id"], 42);
    assert_eq!(body["payload"]["text"], "hello");
    assert_eq!(body["payload"]["disable_web_page_preview"], true);
    // No topic configured: the field is absent, not zero — Telegram rejects 0.
    let plain = payload(-100, None, "hi");
    assert!(plain.get("message_thread_id").is_none());
}

/// Contract check against the fleet relay as it is deployed (cygnus-gateway 0.3.0.0,
/// `app/server.js` `handleSend`). The gateway accepts either a flat Bot-API body or
/// `{method, payload}`; we send the latter, never a `bot` field — an api key may be
/// pinned to one bot, and naming another bot is a 403 there.
#[test]
fn the_relay_request_matches_the_gateway_send_contract() {
    let body = relay_body(-1001, Some(9), "title
https://example.test/a");
    let keys: Vec<&str> = body.as_object().unwrap().keys().map(String::as_str).collect();
    assert_eq!(
        keys,
        vec!["method", "payload"],
        "the gateway reads `payload` when present and rejects an unknown method"
    );
    assert_eq!(body["method"], "sendMessage");
    assert!(
        body.get("bot").is_none(),
        "a pinned api key must not be overridden by a bot field"
    );
    // The gateway forwards `payload` verbatim to the Bot API, so the fields it requires
    // are exactly the ones we set.
    let payload = body["payload"].as_object().unwrap();
    assert!(payload.contains_key("chat_id"));
    assert!(payload.contains_key("text"));
    assert!(payload.contains_key("message_thread_id"));
    // `Bearer <cyg_* api key>` is what we send on the request; the token never enters
    // the body, so it cannot be forwarded to Telegram in a log or an error message.
    assert!(!body.to_string().contains("token"));
}

#[test]
fn the_message_is_the_title_and_link_and_fits_telegram() {
    let text = alert_text("  Bitcoin hits a record  ", "https://example.test/a");
    assert!(text.starts_with("Bitcoin hits a record\nhttps://example.test/a"));
    // 4096 is Telegram's limit; a longer title is cut, not rejected.
    let long = alert_text(&"á".repeat(9_000), "https://example.test/a");
    assert!(long.chars().count() <= TEXT_LIMIT);
    // Cutting on a character boundary keeps the string valid UTF-8.
    assert!(long.is_char_boundary(long.len()));
}

#[test]
fn a_row_is_claimed_once_and_a_stranded_claim_is_reclaimed() {
    let mut db = db();
    let id = delivery(&db, "claimed", "https://example.test/c", 1_000);
    enqueue(&db, id, 1_000).unwrap();
    // A second enqueue of the same delivery is a no-op, not a second message.
    enqueue(&db, id, 1_000).unwrap();
    assert_eq!(queue_stats(&db).unwrap().pending, 1);

    let first = claim(&mut db, 1_000).unwrap();
    assert_eq!(first.len(), 1);
    assert_eq!(first[0].title, "claimed");
    assert_eq!(queue_stats(&db).unwrap().sending, 1);

    // While it is being sent, a second claim takes nothing: no double send.
    assert!(claim(&mut db, 1_010).unwrap().is_empty());

    // A crash leaves the row in `sending`; after the grace period it is taken over.
    let later = 1_000 + STALE_CLAIM_SECONDS + 1;
    assert_eq!(claim(&mut db, later).unwrap().len(), 1);
}

#[test]
fn a_sent_row_is_never_claimed_again_and_a_failure_retries_then_stops() {
    let mut db = db();
    let ok = delivery(&db, "ok", "https://example.test/1", 1_000);
    let bad = delivery(&db, "bad", "https://example.test/2", 1_000);
    enqueue(&db, ok, 1_000).unwrap();
    enqueue(&db, bad, 1_000).unwrap();
    assert_eq!(claim(&mut db, 1_000).unwrap().len(), 2);
    settle(&db, ok, true, "", 1_010).unwrap();
    settle(&db, bad, false, "http_502", 1_010).unwrap();

    let stats = queue_stats(&db).unwrap();
    assert_eq!((stats.sent, stats.pending), (1, 1));
    let again = claim(&mut db, 1_020).unwrap();
    assert_eq!(again.len(), 1, "only the failed one comes back");
    assert_eq!(again[0].delivery_id, bad);
    settle(&db, bad, false, "http_502", 1_020).unwrap();

    // Every attempt is settled by the flusher; three more reach the limit of 5, after
    // which the row stops retrying and is visible as failed instead of looping forever.
    for now in [1_030, 1_040, 1_050] {
        assert_eq!(claim(&mut db, now).unwrap().len(), 1, "attempt at {now}");
        settle(&db, bad, false, "timeout", now).unwrap();
    }
    assert!(claim(&mut db, 1_070).unwrap().is_empty());
    assert_eq!(queue_stats(&db).unwrap().failed, 1);
}

#[test]
fn settings_writes_validate_every_field_before_any_is_stored() {
    let default = TelegramSettingsPatch::default();
    assert!(settings_writes(&default).unwrap().is_empty());

    let patch = TelegramSettingsPatch {
        telegram_mode: Some("relay".into()),
        telegram_chat_id: Some(" -100123 ".into()),
        telegram_alerts_topic_id: Some("42".into()),
    };
    let writes = settings_writes(&patch).unwrap();
    assert_eq!(
        writes,
        vec![
            (MODE_KEY, "relay".to_string()),
            (CHAT_ID_KEY, "-100123".to_string()),
            (TOPIC_ID_KEY, "42".to_string()),
        ]
    );

    // Clearing a field is allowed (empty string), a wrong value is not.
    let cleared = TelegramSettingsPatch {
        telegram_mode: None,
        telegram_chat_id: Some(String::new()),
        telegram_alerts_topic_id: Some(String::new()),
    };
    assert_eq!(settings_writes(&cleared).unwrap().len(), 2);
    for (patch, code) in [
        (
            TelegramSettingsPatch {
                telegram_mode: Some("sometimes".into()),
                ..Default::default()
            },
            "invalid_telegram_mode",
        ),
        (
            TelegramSettingsPatch {
                telegram_chat_id: Some("not-a-number".into()),
                ..Default::default()
            },
            "invalid_telegram_chat_id",
        ),
        (
            TelegramSettingsPatch {
                telegram_chat_id: Some("0".into()),
                ..Default::default()
            },
            "invalid_telegram_chat_id",
        ),
        (
            TelegramSettingsPatch {
                telegram_alerts_topic_id: Some("-1".into()),
                ..Default::default()
            },
            "invalid_telegram_topic_id",
        ),
    ] {
        assert_eq!(settings_writes(&patch).unwrap_err(), code);
    }
}

#[test]
fn a_rule_may_name_telegram_once_the_channel_is_on_even_without_its_secret() {
    // Declaring intent must not depend on a token already being stored: the secret is
    // written later, and the queue plus `reason` explain any delay.
    let db = db();
    let config = cfg();
    let mut input = crate::alerts::RuleInput {
        slug: "tg".into(),
        label_vi: "Nhãn".into(),
        label_en: "Label".into(),
        enabled: true,
        match_kind: "keyword".into(),
        match_value: "bitcoin".into(),
        level: "info".into(),
        cooldown_seconds: 60,
        quiet_from: String::new(),
        quiet_to: String::new(),
        timezone: "UTC".into(),
        channels: "in-app,telegram".into(),
    };
    // Off: refused, and the channel list offers only the inbox.
    assert_eq!(
        crate::alerts::validate(&input, is_on(&db, &config)),
        Err("channel_not_implemented")
    );
    assert_eq!(
        crate::alerts::available_channels(is_on(&db, &config)),
        vec!["in-app"]
    );
    set(&db, MODE_KEY, "relay");
    assert!(is_on(&db, &config));
    assert_eq!(crate::alerts::validate(&input, is_on(&db, &config)), Ok(()));
    assert_eq!(
        crate::alerts::available_channels(is_on(&db, &config)),
        vec!["in-app", "telegram"]
    );
    // But a send still needs a destination and a secret, and says which is missing.
    assert!(!can_send(&db, None, &config));
    input.channels = "in-app,email".into();
    assert_eq!(
        crate::alerts::validate(&input, is_on(&db, &config)),
        Err("channel_not_implemented"),
        "a channel that is not implemented stays refused"
    );
}

#[test]
fn the_default_relay_host_is_always_allowlisted_for_private_dns() {
    let cfg = cfg();
    assert!(
        cfg.allowed_hosts
            .iter()
            .any(|host| host == "telegram.lengoc.me"),
        "the compiled-in relay must be usable without extra env: {:?}",
        cfg.allowed_hosts
    );
}

#[test]
fn a_reply_counts_as_sent_only_when_it_says_ok() {
    assert!(answer_ok(br#"{"ok":true,"result":{"message_id":7}}"#));
    assert!(!answer_ok(br#"{"ok":false,"error":"chat not found"}"#));
    assert!(!answer_ok(b"not json"));
    assert!(!answer_ok(b"{}"));
}
