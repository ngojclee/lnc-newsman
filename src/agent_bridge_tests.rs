//! End-to-end loopback integration tests for `lnc-newsman` App-Scoped Agents:
//! 1. Outbound Reverse Relay WSS (`lnc-app-bridge.v1`) + `v4s` capability verification + SSRF guard.
//! 2. Descendant-aware `ResourceCtx { regions, topics }` row-level isolation + legal/tax disclaimer preservation + unknown arg rejection.
//! 3. All 4 Group Discussion modes (`parallel_panel`, `sequential_build`, `cross_debate`, `mention_or_lead` <= 3 hops) + 7-event SSE ring buffer + Whiteboard + 16 KiB `ActiveViewContext`.
//! 4. DeepTutor Studio CRUD + `SecretBox` (`LNC_MASTER_KEY`) encryption/redaction/persistence + graceful `secrets == None` degradation + gated `brief_propose` + `telegram.rs` alert coexistence.
//! 5. Dual-mode Conversational Telegram (`direct_webhook`, `direct_poll`, `cygnus_webhook`, `cygnus_poll` + `open|mention|topics_only` + `message_thread_id`) + Cron/Interval Scheduler.

use crate::{
    AppState, SharedState, agent_bridge, auth, collector, config::Config, connector, connectors,
    db::Database, documents, fetch::Fetcher, hub, llm, mcp, now, regions, secrets, series, tax,
    telegram,
};
use axum::{
    Json, Router,
    body::Body,
    extract::State,
    http::{HeaderMap, Request, StatusCode, header},
    routing::{get, post},
};
use chrono::Utc;
use futures_util::{SinkExt, StreamExt};
use http_body_util::BodyExt;
use rusqlite::params;
use serde_json::{Value, json};
use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    time::Duration,
};
use tokio::net::TcpListener;
use tokio_tungstenite::{accept_async, tungstenite::Message as WsMessage};
use tower::ServiceExt;

const TEST_DELEGATION_SECRET: &str = "newsman-test-v4s-delegation-secret-32b";
const TEST_MGMT_KEY: &str = "newsman-test-mgmt-key";
const TEST_API_KEY: &str = "newsman-test-api-key";

static TEST_COUNTER: AtomicU64 = AtomicU64::new(1);

fn unique_temp_dir(tag: &str) -> PathBuf {
    let n = TEST_COUNTER.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!(
        "lnc-newsman-agent-bridge-{tag}-{}-{n}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("regions")).unwrap();
    dir
}

struct SeededIds {
    vn_fin_art: i64,
    vn_hcmc_fin_art: i64,
    vn_crypto_art: i64,
    us_fin_art: i64,
    vn_doc_id: i64,
    us_doc_id: i64,
}

fn seed_newsroom_fixture(db: &Database) -> SeededIds {
    // Region tree: global -> vn -> vn-hcmc, plus sibling us.
    db.conn
        .execute(
            "INSERT OR REPLACE INTO regions(code,parent_code,label_vi,label_en,enabled,timezone)
             VALUES ('vn','global','Việt Nam','Vietnam',1,'Asia/Ho_Chi_Minh')",
            [],
        )
        .unwrap();
    db.conn
        .execute(
            "INSERT OR REPLACE INTO regions(code,parent_code,label_vi,label_en,enabled,timezone)
             VALUES ('vn-hcmc','vn','TP.HCM','Ho Chi Minh City',1,'Asia/Ho_Chi_Minh')",
            [],
        )
        .unwrap();
    db.conn
        .execute(
            "INSERT OR REPLACE INTO regions(code,parent_code,label_vi,label_en,enabled,timezone)
             VALUES ('us','global','Hoa Kỳ','United States',1,'America/New_York')",
            [],
        )
        .unwrap();

    // Topics: finance, tax, crypto.
    for (slug, vi, en) in [
        ("finance", "Tài chính", "Finance"),
        ("tax", "Thuế", "Tax"),
        ("crypto", "Tiền mã hóa", "Crypto"),
    ] {
        db.conn
            .execute(
                "INSERT INTO topics(slug,label_vi,label_en,enabled) VALUES (?1,?2,?3,1)
                 ON CONFLICT(slug) DO UPDATE SET enabled=1",
                params![slug, vi, en],
            )
            .unwrap();
    }

    // Sources per region.
    for (slug, region) in [
        ("src-vn", "vn"),
        ("src-hcmc", "vn-hcmc"),
        ("src-us", "us"),
    ] {
        db.conn
            .execute(
                "INSERT INTO sources(slug,name,url,tier,enabled,region)
                 VALUES (?1,?1,'https://example.test/rss','T1',1,?2)",
                params![slug, region],
            )
            .unwrap();
    }
    let src_vn: i64 = db
        .conn
        .query_row("SELECT id FROM sources WHERE slug='src-vn'", [], |r| {
            r.get(0)
        })
        .unwrap();
    let src_hcmc: i64 = db
        .conn
        .query_row("SELECT id FROM sources WHERE slug='src-hcmc'", [], |r| {
            r.get(0)
        })
        .unwrap();
    let src_us: i64 = db
        .conn
        .query_row("SELECT id FROM sources WHERE slug='src-us'", [], |r| {
            r.get(0)
        })
        .unwrap();

    let insert_art = |url: &str, title: &str, src: i64, topic: &str, region: &str| -> i64 {
        db.conn
            .execute(
                "INSERT INTO articles(canonical_url,title,source_id,topic_slug,fetched_at,region_code)
                 VALUES (?1,?2,?3,?4,1762900000,?5)",
                params![url, title, src, topic, region],
            )
            .unwrap();
        db.conn.last_insert_rowid()
    };

    let vn_fin_art = insert_art(
        "https://example.test/vn-fin-1",
        "NHNN điều hành lãi suất liên ngân hàng",
        src_vn,
        "finance",
        "vn",
    );
    let vn_hcmc_fin_art = insert_art(
        "https://example.test/hcmc-fin-1",
        "Tín dụng doanh nghiệp tại TP.HCM tăng trưởng quý IV",
        src_hcmc,
        "finance",
        "vn-hcmc",
    );
    let vn_crypto_art = insert_art(
        "https://example.test/vn-crypto-1",
        "Thử nghiệm sandbox tài sản số tại Việt Nam",
        src_vn,
        "crypto",
        "vn",
    );
    let us_fin_art = insert_art(
        "https://example.test/us-fin-1",
        "FOMC holds benchmark rate steady",
        src_us,
        "finance",
        "us",
    );

    // Official documents (with written summary -> server attaches SUMMARY_DISCLAIMER).
    documents::upsert(
        db,
        &documents::DocumentRow {
            provider: "vn-gazette".into(),
            region: "vn".into(),
            doc_type: "decree".into(),
            agency: "Bộ Tài chính".into(),
            number: "ND-123-2026".into(),
            title: "Nghị định hướng dẫn kê khai thuế GTGT điện tử".into(),
            effective_on: Some("2026-12-01".into()),
            published_on: Some("2026-11-01".into()),
            url: "https://example.test/docs/nd-123".into(),
            summary: Some("Hướng dẫn chi tiết kỳ kê khai theo tháng và quý.".into()),
            topic: "tax".into(),
            disclaimer: String::new(),
        },
    )
    .unwrap();
    documents::upsert(
        db,
        &documents::DocumentRow {
            provider: "federal-register".into(),
            region: "us".into(),
            doc_type: "rule".into(),
            agency: "Treasury".into(),
            number: "FR-999-2026".into(),
            title: "Treasury reporting rule update".into(),
            effective_on: Some("2026-12-15".into()),
            published_on: Some("2026-11-02".into()),
            url: "https://example.test/docs/fr-999".into(),
            summary: Some("Updates corporate filing thresholds.".into()),
            topic: "tax".into(),
            disclaimer: String::new(),
        },
    )
    .unwrap();

    let vn_doc_id: i64 = db
        .conn
        .query_row(
            "SELECT id FROM documents WHERE number='ND-123-2026'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    let us_doc_id: i64 = db
        .conn
        .query_row(
            "SELECT id FROM documents WHERE number='FR-999-2026'",
            [],
            |r| r.get(0),
        )
        .unwrap();

    // Tax deadlines: unverified (`verified = false`) to verify flag & disclaimer preservation.
    db.conn
        .execute(
            "INSERT INTO tax_deadlines(pack_key,region,source_code,code,name_vi,name_en,cadence,day,weekend_rule,lead_days,verified,enabled,note)
             VALUES ('vn/vat-monthly','vn','gdt','vn-vat-m','Tờ khai GTGT tháng','Monthly VAT return','monthly',20,'next_business_day',10,0,1,'Theo Thông tư 80')",
            [],
        )
        .unwrap();
    db.conn
        .execute(
            "INSERT INTO tax_deadlines(pack_key,region,source_code,code,name_vi,name_en,cadence,day,weekend_rule,lead_days,verified,enabled,note)
             VALUES ('us/payroll-m','us','irs','us-941-m','Payroll deposit','Monthly payroll deposit','monthly',15,'next_business_day',7,0,1,'IRS Form 941')",
            [],
        )
        .unwrap();

    // Series & observations in `vn` vs `us`.
    series::upsert_series(
        db,
        &series::SeriesDef {
            code: "vn_cpi_yoy",
            provider: "gso-test",
            region: "vn",
            label_vi: "CPI Việt Nam YoY",
            label_en: "Vietnam CPI YoY",
            unit: "%",
            frequency: "monthly",
            attribution: "Source: GSO test fixture",
        },
    )
    .unwrap();
    series::record(
        db,
        &series::NewObservation::final_value("vn_cpi_yoy", 1761955200, 1762041600, 3.42),
    )
    .unwrap();

    series::upsert_series(
        db,
        &series::SeriesDef {
            code: "us_cpi_yoy",
            provider: "bls-test",
            region: "us",
            label_vi: "CPI Mỹ YoY",
            label_en: "US CPI YoY",
            unit: "%",
            frequency: "monthly",
            attribution: "Source: BLS test fixture",
        },
    )
    .unwrap();
    series::record(
        db,
        &series::NewObservation::final_value("us_cpi_yoy", 1761955200, 1762041600, 2.65),
    )
    .unwrap();
    db.conn
        .execute(
            "UPDATE series SET enabled=1 WHERE code IN ('vn_cpi_yoy','us_cpi_yoy')",
            [],
        )
        .unwrap();

    SeededIds {
        vn_fin_art,
        vn_hcmc_fin_art,
        vn_crypto_art,
        us_fin_art,
        vn_doc_id,
        us_doc_id,
    }
}

fn build_test_state(
    dir: PathBuf,
    db: Database,
    fetcher: Fetcher,
    secrets_box: Option<secrets::SecretBox>,
) -> SharedState {
    let bridge_rt = agent_bridge::AgentBridgeRuntime::new(&db);
    let mut conn_cfg = connector::ConnectorConfig::default();
    conn_cfg.bearer = Some("newsman-test-connector-bearer".to_string());
    conn_cfg.delegation_secret = Some(TEST_DELEGATION_SECRET.to_string());
    conn_cfg.audience = Some("lnc-newsman".to_string());

    Arc::new(AppState {
        cfg: Config {
            data_dir: dir.clone(),
            regions_dir: dir.join("regions"),
            bind_address: "127.0.0.1".parse().unwrap(),
            port: 0,
            management_key: Some(TEST_MGMT_KEY.into()),
            api_keys: vec![TEST_API_KEY.into()],
            allowed_hosts: vec!["newsman.test".into(), "127.0.0.1".into()],
            trusted_proxies: Vec::new(),
            secure_cookie: false,
            version: "test",
        },
        db: Mutex::new(db),
        login_limiter: Mutex::new(auth::LoginLimiter::default()),
        started_at: now(),
        fetcher,
        in_flight: Mutex::new(HashSet::new()),
        connector_in_flight: Mutex::new(HashSet::new()),
        host_gates: Mutex::new(HashMap::new()),
        host_gap: collector::HOST_GAP,
        secrets: secrets_box,
        connector_config: connectors::ConnectorConfig::default(),
        llm: llm::LlmConfig {
            base_url: None,
            model: "mock".into(),
        },
        hub: hub::ProviderConfig::default(),
        telegram: telegram::TelegramConfig::default(),
        connector: conn_cfg,
        connector_nonces: Mutex::new(HashMap::new()),
        llm_enabled: false,
        pack: Mutex::new(regions::PackStatus::default()),
        alerts_running: AtomicBool::new(false),
        agent_bridge: bridge_rt,
    })
}

async fn login_cookie(app: &Router) -> String {
    let req = Request::builder()
        .method("POST")
        .uri("/api/auth/login")
        .header(header::HOST, "newsman.test")
        .header(header::CONTENT_TYPE, "application/json")
        .header("x-requested-with", "XMLHttpRequest")
        .body(Body::from(json!({ "key": TEST_MGMT_KEY }).to_string()))
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let set_cookie = resp
        .headers()
        .get(header::SET_COOKIE)
        .unwrap()
        .to_str()
        .unwrap();
    set_cookie.split(';').next().unwrap().to_string()
}

async fn authed_json(
    app: &Router,
    cookie: &str,
    method: &str,
    uri: &str,
    body: Option<Value>,
) -> (StatusCode, Value) {
    let mut builder = Request::builder()
        .method(method)
        .uri(uri)
        .header(header::HOST, "newsman.test")
        .header(header::COOKIE, cookie)
        .header("x-requested-with", "XMLHttpRequest");
    let req_body = if let Some(b) = body {
        builder = builder.header(header::CONTENT_TYPE, "application/json");
        Body::from(b.to_string())
    } else {
        Body::empty()
    };
    let resp = app.clone().oneshot(builder.body(req_body).unwrap()).await.unwrap();
    let status = resp.status();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let val = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    (status, val)
}

fn make_v4s_grant(
    audience: &str,
    app_id: &str,
    caller_agent: &str,
    scopes: &str,
    nonce: &str,
    timestamp: i64,
    rctx: connector::ResourceCtx,
) -> connector::V4sGrant {
    let scope_vec: Vec<String> = scopes
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect();
    let sig = connector::sign_v4s(
        TEST_DELEGATION_SECRET,
        audience,
        caller_agent,
        "owner",
        app_id,
        timestamp,
        nonce,
        &scope_vec,
        &rctx,
    );
    connector::V4sGrant {
        version: "v4s".into(),
        audience: audience.into(),
        caller_agent: caller_agent.into(),
        delegated_user: "owner".into(),
        app_id: app_id.into(),
        timestamp,
        nonce: nonce.into(),
        scopes: scope_vec,
        resource_ctx: serde_json::to_value(rctx).unwrap(),
        signature: sig,
    }
}

fn call_tool_parsed(
    state: &SharedState,
    caller: &connector::Caller,
    name: &str,
    arguments: &Value,
) -> (Value, bool) {
    let raw = mcp::call_tool_direct(state, Some(caller), name, arguments);
    let is_err = raw.get("isError").and_then(Value::as_bool).unwrap_or(true);
    let text = raw
        .pointer("/content/0/text")
        .and_then(Value::as_str)
        .unwrap_or("");
    let parsed = serde_json::from_str::<Value>(text).unwrap_or_else(|_| json!({ "error": text }));
    (parsed, is_err)
}

// ---------------------------------------------------------------------------
// Test 1: Outbound Reverse Relay WSS + `v4s` Capability Verification + SSRF Guard
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_1_reverse_relay_wss_handshake_v4s_and_ssrf_guard() {
    let dir = unique_temp_dir("t1");
    let db = Database::in_memory().unwrap();
    let seeded = seed_newsroom_fixture(&db);
    let vault = secrets::SecretBox::from_bytes([0x11; 32], None);

    // 1A. SSRF check: `Fetcher::public_only()` without allowlisting `127.0.0.1` MUST block loopback WS!
    let strict_fetcher = Fetcher::public_only();
    let ssrf_err = strict_fetcher
        .resolve_ws_target("ws://127.0.0.1/api/v1/app-bridge/ws")
        .await
        .expect_err("public_only Fetcher must refuse loopback WebSocket IP");
    assert_eq!(ssrf_err.code(), "blocked_address");
    let port_err = strict_fetcher
        .resolve_ws_target("ws://127.0.0.1:19999/api/v1/app-bridge/ws")
        .await
        .expect_err("public_only Fetcher must refuse off-standard port");
    assert_eq!(port_err.code(), "invalid_url");

    // 1B. Spin up loopback mock Astraea Reverse Relay WS server.
    let ws_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let ws_addr = ws_listener.local_addr().unwrap();

    let state = build_test_state(dir, db, Fetcher::public_only(), Some(vault));
    let app = crate::router(state.clone());
    let cookie = login_cookie(&app).await;

    // Configure bridge with `allowed_hosts: ["127.0.0.1"]` and sealed `astraea_token`.
    let (st, cfg_body) = authed_json(
        &app,
        &cookie,
        "PUT",
        "/api/app-agents/bridge-config",
        Some(json!({
            "astraea_url": format!("http://{ws_addr}"),
            "allowed_hosts": ["127.0.0.1"],
            "astraea_token": "relay-secret-token-123",
        })),
    )
    .await;
    assert_eq!(st, StatusCode::OK, "{cfg_body}");
    assert_eq!(cfg_body["has_astraea_token"], true);

    let relay_worker = agent_bridge::spawn_relay_worker(state.clone());

    let (tcp_stream, _) = tokio::time::timeout(Duration::from_secs(5), ws_listener.accept())
        .await
        .expect("relay worker should connect")
        .unwrap();
    let mut server_ws = accept_async(tcp_stream).await.unwrap();

    // Receive `hello` frame and verify contract.
    let hello_raw = tokio::time::timeout(Duration::from_secs(5), server_ws.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    let hello: Value = serde_json::from_str(hello_raw.to_text().unwrap()).unwrap();
    assert_eq!(hello["type"], "hello");
    assert_eq!(hello["protocol"], "lnc-app-bridge.v1");
    assert_eq!(hello["app_id"], "lnc-newsman");
    assert_eq!(hello["manifest"]["scope_kinds"], json!(["regions", "topics"]));
    let manifest_tools = hello["manifest"]["tools"].as_array().unwrap();
    assert!(manifest_tools.iter().any(|t| t["name"] == "newsman__latest_articles" && t["tier"] == "read"));
    assert!(manifest_tools.iter().any(|t| t["name"] == "newsman__brief_propose" && t["tier"] == "write_gated"));

    // Verify `bridge-config` reports `astraea_relay` while WS is live.
    tokio::time::sleep(Duration::from_millis(40)).await;
    let (_, live_status) = authed_json(&app, &cookie, "GET", "/api/app-agents/bridge-config", None).await;
    assert_eq!(live_status["mode"], "astraea_relay");
    assert_eq!(live_status["relay_connected"], true);

    let now_ts = Utc::now().timestamp();
    let vn_ctx = connector::ResourceCtx {
        regions: Some(vec!["vn".into()]),
        topics: None,
    };
    assert!(!vn_ctx.is_unrestricted());
    assert!(connector::resource_ctx_canonical_json(&vn_ctx).contains("\"regions\":[\"vn\"]"));

    // 1C. Valid `v4s` `tool_call` for `newsman__latest_articles` -> returns `vn` + descendant `vn-hcmc`, hides `us`!
    let valid_grant = make_v4s_grant(
        "lnc-newsman",
        "lnc-newsman",
        "macro-argus",
        "newsman:read",
        "nonce-relay-valid-1",
        now_ts,
        vn_ctx.clone(),
    );
    server_ws
        .send(WsMessage::Text(
            json!({
                "type": "tool_call",
                "call_id": "c_valid_1",
                "tool_name": "newsman__latest_articles",
                "arguments": { "limit": 20 },
                "grant": valid_grant,
            })
            .to_string()
            .into(),
        ))
        .await
        .unwrap();

    let res1_raw = server_ws.next().await.unwrap().unwrap();
    let res1: Value = serde_json::from_str(res1_raw.to_text().unwrap()).unwrap();
    assert_eq!(res1["type"], "tool_result");
    assert_eq!(res1["call_id"], "c_valid_1");
    assert_eq!(res1["is_error"], false, "{res1}");
    let art_ids: Vec<i64> = res1["result"]["data"]
        .as_array()
        .unwrap()
        .iter()
        .map(|a| a["id"].as_i64().unwrap())
        .collect();
    assert!(art_ids.contains(&seeded.vn_fin_art));
    assert!(art_ids.contains(&seeded.vn_hcmc_fin_art), "descendant vn-hcmc must be included");
    assert!(!art_ids.contains(&seeded.us_fin_art), "out-of-scope us article must be filtered out");

    // 1D. Valid `v4s` `tool_call` for `newsman__tax_calendar` -> preserves disclaimers & `verified == false`!
    let tax_grant = make_v4s_grant(
        "lnc-newsman",
        "lnc-newsman",
        "tax-linh",
        "newsman:read",
        "nonce-relay-tax-2",
        now_ts,
        vn_ctx.clone(),
    );
    server_ws
        .send(WsMessage::Text(
            json!({
                "type": "tool_call",
                "call_id": "c_tax_2",
                "tool": "newsman__tax_calendar",
                "arguments": { "horizon_days": 365 },
                "v4s": tax_grant,
            })
            .to_string()
            .into(),
        ))
        .await
        .unwrap();
    let res_tax: Value = serde_json::from_str(server_ws.next().await.unwrap().unwrap().to_text().unwrap()).unwrap();
    assert_eq!(res_tax["is_error"], false, "{res_tax}");
    assert_eq!(res_tax["result"]["disclaimer_vi"], tax::DISCLAIMER_VI);
    assert_eq!(res_tax["result"]["disclaimer_en"], tax::DISCLAIMER_EN);
    assert_eq!(res_tax["result"]["holidays_modelled"], false);
    let deadlines = res_tax["result"]["data"].as_array().unwrap();
    assert_eq!(deadlines.len(), 1);
    assert_eq!(deadlines[0]["code"], "vn-vat-m");
    assert_eq!(deadlines[0]["verified"], false);

    // 1E. Rejections: replay nonce, cross-app audience (`lnc-library`), and tampered `resource_ctx`!
    // Replay same `valid_grant` (`nonce-relay-valid-1`):
    server_ws
        .send(WsMessage::Text(
            json!({
                "type": "tool_call",
                "call_id": "c_replay",
                "tool": "newsman__latest_articles",
                "arguments": {},
                "v4s": valid_grant,
            })
            .to_string()
            .into(),
        ))
        .await
        .unwrap();
    let res_replay: Value = serde_json::from_str(server_ws.next().await.unwrap().unwrap().to_text().unwrap()).unwrap();
    assert_eq!(res_replay["is_error"], true);
    assert_eq!(res_replay["error_code"], connector::DENIED_DELEGATION);

    // Cross-app audience `lnc-library`:
    let cross_app_grant = make_v4s_grant(
        "lnc-library",
        "lnc-library",
        "librarian-mia",
        "newsman:read",
        "nonce-cross-app-3",
        now_ts,
        vn_ctx.clone(),
    );
    server_ws
        .send(WsMessage::Text(
            json!({
                "type": "tool_call",
                "call_id": "c_cross_app",
                "tool": "newsman__latest_articles",
                "arguments": {},
                "v4s": cross_app_grant,
            })
            .to_string()
            .into(),
        ))
        .await
        .unwrap();
    let res_cross: Value = serde_json::from_str(server_ws.next().await.unwrap().unwrap().to_text().unwrap()).unwrap();
    assert_eq!(res_cross["is_error"], true);
    assert_eq!(res_cross["error_code"], connector::DENIED_AUDIENCE);

    // Tampered `resource_ctx` (`us` substituted after signing `vn`):
    let mut tampered = make_v4s_grant(
        "lnc-newsman",
        "lnc-newsman",
        "macro-argus",
        "newsman:read",
        "nonce-tampered-4",
        now_ts,
        vn_ctx,
    );
    tampered.resource_ctx = json!({ "regions": ["us"] });
    server_ws
        .send(WsMessage::Text(
            json!({
                "type": "tool_call",
                "call_id": "c_tampered",
                "tool": "newsman__latest_articles",
                "arguments": {},
                "v4s": tampered,
            })
            .to_string()
            .into(),
        ))
        .await
        .unwrap();
    let res_tampered: Value =
        serde_json::from_str(server_ws.next().await.unwrap().unwrap().to_text().unwrap()).unwrap();
    assert_eq!(res_tampered["is_error"], true);
    assert_eq!(res_tampered["error_code"], connector::DENIED_DELEGATION);

    // 1F. Also verify HTTP `POST /mcp` `x-lnc-v4s-grant` rejects cross-app with HTTP 403!
    let http_cross = make_v4s_grant(
        "lnc-library",
        "lnc-library",
        "librarian-mia",
        "newsman:read",
        "nonce-http-cross-5",
        now_ts,
        connector::ResourceCtx::default(),
    );
    let mcp_req = Request::builder()
        .method("POST")
        .uri("/mcp")
        .header(header::HOST, "newsman.test")
        .header(header::CONTENT_TYPE, "application/json")
        .header(header::AUTHORIZATION, "Bearer newsman-test-connector-bearer")
        .header(
            "x-lnc-v4s-grant",
            serde_json::to_string(&http_cross).unwrap(),
        )
        .body(Body::from(
            json!({"jsonrpc":"2.0","id":1,"method":"tools/list"}).to_string(),
        ))
        .unwrap();
    let mcp_resp = app.clone().oneshot(mcp_req).await.unwrap();
    assert_eq!(mcp_resp.status(), StatusCode::FORBIDDEN);

    relay_worker.abort();
}

// ---------------------------------------------------------------------------
// Test 2: Descendant-Aware `ResourceCtx` Row-Level Isolation & Unknown Args
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_2_descendant_aware_resource_ctx_isolation_and_unknown_args() {
    let dir = unique_temp_dir("t2");
    let db = Database::in_memory().unwrap();
    let seeded = seed_newsroom_fixture(&db);
    let state = build_test_state(dir, db, Fetcher::public_only(), None);

    // Caller scoped strictly to `regions: ["vn"]` (expands to `vn` + `vn-hcmc`) and `topics: ["finance", "tax"]`.
    let vn_caller = connector::Caller {
        scopes: vec![connector::READ_SCOPE.into(), connector::WRITE_SCOPE.into()],
        client_id: "macro-argus".into(),
        app_id: Some("lnc-newsman".into()),
        delegated_user: Some("owner".into()),
        resource_ctx: Some(connector::ResourceCtx {
            regions: Some(vec!["vn".into()]),
            topics: Some(vec!["finance".into(), "tax".into()]),
        }),
    };
    assert_eq!(vn_caller.delegated_user.as_deref(), Some("owner"));

    let us_caller = connector::Caller {
        scopes: vec![connector::READ_SCOPE.into(), connector::WRITE_SCOPE.into()],
        client_id: "us-desk".into(),
        app_id: Some("lnc-newsman".into()),
        delegated_user: Some("owner".into()),
        resource_ctx: Some(connector::ResourceCtx {
            regions: Some(vec!["us".into()]),
            topics: Some(vec!["finance".into(), "tax".into()]),
        }),
    };

    // 2A. `latest_articles` & `article_get`:
    let (arts, is_err) = call_tool_parsed(&state, &vn_caller, "latest_articles", &json!({}));
    assert!(!is_err, "{arts}");
    let vn_ids: Vec<i64> = arts["data"]
        .as_array()
        .unwrap()
        .iter()
        .map(|a| a["id"].as_i64().unwrap())
        .collect();
    assert_eq!(vn_ids.len(), 2, "must see vn + vn-hcmc finance, NOT vn crypto or us finance");
    assert!(vn_ids.contains(&seeded.vn_fin_art));
    assert!(vn_ids.contains(&seeded.vn_hcmc_fin_art));
    assert!(!vn_ids.contains(&seeded.vn_crypto_art));
    assert!(!vn_ids.contains(&seeded.us_fin_art));

    let (_, err_us_art) = call_tool_parsed(
        &state,
        &vn_caller,
        "article_get",
        &json!({ "id": seeded.us_fin_art }),
    );
    assert!(err_us_art, "vn_caller must not read us article by id");

    let (_, err_crypto_art) = call_tool_parsed(
        &state,
        &vn_caller,
        "article_get",
        &json!({ "id": seeded.vn_crypto_art }),
    );
    assert!(err_crypto_art, "vn_caller without crypto topic must not read crypto article");

    // 2B. `documents` (preserving `SUMMARY_DISCLAIMER`):
    let (docs, is_err) = call_tool_parsed(&state, &vn_caller, "documents", &json!({}));
    assert!(!is_err, "{docs}");
    let doc_arr = docs["data"].as_array().unwrap();
    assert_eq!(doc_arr.len(), 1);
    assert_eq!(doc_arr[0]["number"], "ND-123-2026");
    assert_eq!(doc_arr[0]["disclaimer"], documents::SUMMARY_DISCLAIMER);

    // 2C. `series` & `observations`:
    let (ser, is_err) = call_tool_parsed(&state, &vn_caller, "series", &json!({}));
    assert!(!is_err);
    let ser_codes: Vec<&str> = ser["data"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["code"].as_str().unwrap())
        .collect();
    assert_eq!(ser_codes, vec!["vn_cpi_yoy"]);

    let (_, err_us_obs) = call_tool_parsed(
        &state,
        &vn_caller,
        "observations",
        &json!({ "code": "us_cpi_yoy" }),
    );
    assert!(err_us_obs, "vn_caller must be blocked from us_cpi_yoy observations");

    // 2D. Write isolation (`note_create`, `analysis_create`) & cross-caller visibility:
    let (_, err_note_us) = call_tool_parsed(
        &state,
        &vn_caller,
        "note_create",
        &json!({ "article_id": seeded.us_fin_art, "body": "Cross-region note attempt" }),
    );
    assert!(err_note_us, "vn_caller cannot attach note to us article");

    let (_, err_note_us_doc) = call_tool_parsed(
        &state,
        &vn_caller,
        "note_create",
        &json!({ "document_id": seeded.us_doc_id, "body": "Cross-region doc note" }),
    );
    assert!(err_note_us_doc, "vn_caller cannot attach note to us document");

    let (ok_note_vn, err_note_vn) = call_tool_parsed(
        &state,
        &vn_caller,
        "note_create",
        &json!({ "document_id": seeded.vn_doc_id, "body": "Ghi chú hợp lệ cho ND-123" }),
    );
    assert!(!err_note_vn, "{ok_note_vn}");

    // `us_caller` cannot read notes on `vn_doc_id`:
    let (_, err_us_reads_vn_notes) = call_tool_parsed(
        &state,
        &us_caller,
        "notes",
        &json!({ "document_id": seeded.vn_doc_id }),
    );
    assert!(err_us_reads_vn_notes);

    // `vn_caller` cannot create an analysis in `us` or `crypto`:
    let (_, err_ana_us) = call_tool_parsed(
        &state,
        &vn_caller,
        "analysis_create",
        &json!({
            "title": "US Macro",
            "body": "Out of scope",
            "region_code": "us",
            "topic_slug": "finance"
        }),
    );
    assert!(err_ana_us);

    let (ok_ana_vn, err_ana_vn) = call_tool_parsed(
        &state,
        &vn_caller,
        "analysis_create",
        &json!({
            "title": "Bản tin lãi suất TP.HCM",
            "body": "Tổng hợp thanh khoản liên ngân hàng.",
            "region_code": "vn-hcmc",
            "topic_slug": "finance",
            "source_article_ids": [seeded.vn_hcmc_fin_art]
        }),
    );
    assert!(!err_ana_vn, "{ok_ana_vn}");

    // `us_caller` listing `analyses` must NOT see the `vn-hcmc` analysis!
    let (us_analyses, is_err) = call_tool_parsed(&state, &us_caller, "analyses", &json!({}));
    assert!(!is_err);
    assert_eq!(us_analyses["data"].as_array().unwrap().len(), 0);

    // 2E. Unknown / forged tool arguments MUST be rejected on both read and write tools:
    let (forged_read, is_err) = call_tool_parsed(
        &state,
        &vn_caller,
        "latest_articles",
        &json!({ "forged_bypass": true }),
    );
    assert!(is_err);
    assert!(forged_read["error"].as_str().unwrap().contains("unknown argument"));

    let (forged_write, is_err) = call_tool_parsed(
        &state,
        &vn_caller,
        "note_create",
        &json!({ "article_id": seeded.vn_fin_art, "body": "hi", "author": "spoofed" }),
    );
    assert!(is_err);
    assert!(forged_write["error"].as_str().unwrap().contains("unknown argument"));
}

// ---------------------------------------------------------------------------
// Test 3: All 4 Group Chat Modes + 7 SSE Events + Whiteboard + 16 KiB ActiveViewContext
// ---------------------------------------------------------------------------

#[derive(Default)]
struct MockLlmState {
    calls: Mutex<Vec<Value>>,
    fail_next: AtomicBool,
}

async fn mock_openai_completions(
    State(st): State<Arc<MockLlmState>>,
    Json(body): Json<Value>,
) -> (StatusCode, Json<Value>) {
    if st.fail_next.swap(false, Ordering::SeqCst) {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": { "message": "simulated upstream failure" } })),
        );
    }

    st.calls.lock().unwrap().push(body.clone());
    let messages = body["messages"].as_array().cloned().unwrap_or_default();
    let sys_text = messages
        .first()
        .and_then(|m| m["content"].as_str())
        .unwrap_or("");
    let last_role = messages
        .last()
        .and_then(|m| m["role"].as_str())
        .unwrap_or("");
    let user_text = messages
        .iter()
        .rev()
        .find(|m| m["role"] == "user")
        .and_then(|m| m["content"].as_str())
        .unwrap_or("");

    // Step 0: if `TRIGGER_TOOL` is in user message and we haven't returned a tool result yet,
    // request `latest_articles` (or `brief_propose` if `TRIGGER_BRIEF_PROPOSE` is present).
    if last_role != "tool" {
        if user_text.contains("TRIGGER_BRIEF_PROPOSE") {
            return (
                StatusCode::OK,
                Json(json!({
                    "id": "chatcmpl-prop",
                    "choices": [{
                        "index": 0,
                        "message": {
                            "role": "assistant",
                            "reasoning_content": "Preparing gated brief proposal for owner review.",
                            "tool_calls": [{
                                "id": "call_brief_1",
                                "type": "function",
                                "function": {
                                    "name": "brief_propose",
                                    "arguments": "{\"title\":\"Bản tin sáng VN\",\"body\":\"Tổng hợp lãi suất & thuế GTGT.\",\"kind\":\"brief\",\"region_code\":\"vn\",\"topic_slug\":\"finance\"}"
                                }
                            }]
                        }
                    }]
                })),
            );
        }
        if user_text.contains("TRIGGER_TOOL") {
            return (
                StatusCode::OK,
                Json(json!({
                    "id": "chatcmpl-tool",
                    "choices": [{
                        "index": 0,
                        "message": {
                            "role": "assistant",
                            "reasoning_content": "Checking latest Vietnam newsroom items first.",
                            "tool_calls": [{
                                "id": "call_art_1",
                                "type": "function",
                                "function": {
                                    "name": "latest_articles",
                                    "arguments": "{\"limit\":5}"
                                }
                            }]
                        }
                    }]
                })),
            );
        }
    }

    // If testing peer-hop bounding (`PING_PONG_HOPS`), keep mentioning `@tax-linh` and `@macro-argus`
    // so the orchestrator proves it stops cleanly at `MAX_PEER_CONSULT_HOPS` (3)!
    if user_text.contains("PING_PONG_HOPS")
        || messages
            .iter()
            .any(|m| m["content"].as_str().unwrap_or("").contains("PING_PONG_HOPS"))
    {
        let reply = if sys_text.contains("slug: @macro-argus") {
            "Góc nhìn vĩ mô đã rõ, mời @tax-linh bổ sung quy định thuế!"
        } else {
            "Đã đối chiếu kỳ kê khai thuế, mời @macro-argus kiểm tra lại lãi suất!"
        };
        return (
            StatusCode::OK,
            Json(json!({
                "id": "chatcmpl-hop",
                "choices": [{
                    "index": 0,
                    "message": {
                        "role": "assistant",
                        "reasoning_content": "Consulting peer specialist.",
                        "content": reply,
                    }
                }]
            })),
        );
    }

    let content = format!("Phân tích tổng hợp hoàn tất (sys_len={}).", sys_text.len());
    (
        StatusCode::OK,
        Json(json!({
            "id": "chatcmpl-ok",
            "choices": [{
                "index": 0,
                "message": {
                    "role": "assistant",
                    "reasoning_content": "Cross-checked sources and attributions.",
                    "content": content,
                }
            }]
        })),
    )
}

async fn spawn_mock_llm() -> (String, Arc<MockLlmState>, tokio::task::JoinHandle<()>) {
    let st = Arc::new(MockLlmState::default());
    let st_clone = st.clone();
    let app = Router::new()
        .route("/v1/chat/completions", post(mock_openai_completions))
        .route(
            "/v1/models",
            get(|| async {
                Json(json!({
                    "data": [
                        { "id": "gpt-5.4-mini" },
                        { "id": "gpt-5.4" }
                    ]
                }))
            }),
        )
        .with_state(st_clone);
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let handle = tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    (format!("http://{addr}"), st, handle)
}

#[tokio::test]
async fn test_3_group_chat_four_modes_sse_whiteboard_and_active_view_context() {
    let dir = unique_temp_dir("t3");
    let db = Database::in_memory().unwrap();
    let seeded = seed_newsroom_fixture(&db);
    let vault = secrets::SecretBox::from_bytes([0x33; 32], None);
    let (llm_url, llm_state, llm_task) = spawn_mock_llm().await;

    let state = build_test_state(dir, db, Fetcher::allow_loopback_for_tests(), Some(vault));
    let app = crate::router(state.clone());
    let cookie = login_cookie(&app).await;

    let (st, _) = authed_json(
        &app,
        &cookie,
        "PUT",
        "/api/app-agents/bridge-config",
        Some(json!({
            "hub_url": llm_url,
            "hub_token": "hub-test-bearer",
            "allowed_hosts": ["127.0.0.1"],
        })),
    )
    .await;
    assert_eq!(st, StatusCode::OK);

    // 3A. Create a 3-specialist group thread (`parallel_panel`) and test Zero-RTT `ActiveViewContext` + tool loop:
    let (st, created_thr) = authed_json(
        &app,
        &cookie,
        "POST",
        "/api/app-agents/threads",
        Some(json!({
            "kind": "group",
            "title": "Bàn tròn Lãi suất & Thuế",
            "article_id": seeded.vn_fin_art,
            "member_slugs": ["macro-argus", "tax-linh", "editor-minh"],
            "discussion_mode": "parallel_panel",
            "lead_slug": "editor-minh",
        })),
    )
    .await;
    assert_eq!(st, StatusCode::OK);
    let tid = created_thr["thread"]["id"].as_str().unwrap().to_string();

    // Oversized (> 16 KiB) `active_view_context` MUST be rejected with 400 Bad Request:
    let huge_text = "x".repeat(17 * 1024);
    let (st_huge, _) = authed_json(
        &app,
        &cookie,
        "POST",
        &format!("/api/app-agents/threads/{tid}/turns"),
        Some(json!({
            "text": "Tóm tắt bài này",
            "active_view_context": {
                "kind": "article",
                "article_id": seeded.vn_fin_art,
                "content_text": huge_text,
            }
        })),
    )
    .await;
    assert_eq!(st_huge, StatusCode::BAD_REQUEST);

    // Valid `parallel_panel` turn with `TRIGGER_TOOL` + `ActiveViewContext`:
    let (st_p, turn_parallel) = authed_json(
        &app,
        &cookie,
        "POST",
        &format!("/api/app-agents/threads/{tid}/turns"),
        Some(json!({
            "text": "TRIGGER_TOOL Phân tích tác động lãi suất liên ngân hàng",
            "idempotency_key": "idem-parallel-1",
            "active_view_context": {
                "kind": "article",
                "ref_id": seeded.vn_fin_art.to_string(),
                "article_id": seeded.vn_fin_art,
                "title": "NHNN điều hành lãi suất liên ngân hàng",
                "source_attribution": "Source: Wire VN",
                "selected_text": "Lãi suất qua đêm giữ ổn định ở mức 4,1%/năm."
            }
        })),
    )
    .await;
    assert_eq!(st_p, StatusCode::OK, "{turn_parallel}");
    assert_eq!(turn_parallel["messages"].as_array().unwrap().len(), 3, "all 3 panel seats must respond");

    // Verify Zero-RTT `ActiveViewContext` (`selected_text` & `source_attribution`) reached the system prompt!
    {
        let calls = llm_state.calls.lock().unwrap();
        assert!(calls.iter().any(|c| {
            c["messages"][0]["content"]
                .as_str()
                .unwrap_or("")
                .contains("Lãi suất qua đêm giữ ổn định ở mức 4,1%/năm.")
        }));
    }

    // 3B. Switch to `sequential_build` and verify Speaker 2 receives Speaker 1's contribution:
    let (st_patch, _) = authed_json(
        &app,
        &cookie,
        "PATCH",
        &format!("/api/app-agents/threads/{tid}"),
        Some(json!({ "discussion_mode": "sequential_build" })),
    )
    .await;
    assert_eq!(st_patch, StatusCode::OK);
    llm_state.calls.lock().unwrap().clear();

    let (st_seq, turn_seq) = authed_json(
        &app,
        &cookie,
        "POST",
        &format!("/api/app-agents/threads/{tid}/turns"),
        Some(json!({ "text": "Xây dựng khung bản tin tuần tự" })),
    )
    .await;
    assert_eq!(st_seq, StatusCode::OK);
    let seq_msgs = turn_seq["messages"].as_array().unwrap();
    assert_eq!(seq_msgs.len(), 3);
    assert_eq!(seq_msgs[0]["round_idx"], 1);
    assert_eq!(seq_msgs[1]["round_idx"], 2);
    assert_eq!(seq_msgs[2]["round_idx"], 3);
    {
        let calls = llm_state.calls.lock().unwrap();
        assert!(calls.iter().any(|c| {
            c["messages"]
                .as_array()
                .unwrap()
                .iter()
                .any(|m| m["content"].as_str().unwrap_or("").contains("[Current-turn contribution from @macro-argus]"))
        }));
    }

    // 3C. Switch to `cross_debate` and verify 2-round opening thesis + cross-rebuttal (`3 * 2 = 6` messages):
    authed_json(
        &app,
        &cookie,
        "PATCH",
        &format!("/api/app-agents/threads/{tid}"),
        Some(json!({ "discussion_mode": "cross_debate" })),
    )
    .await;
    let (st_deb, turn_deb) = authed_json(
        &app,
        &cookie,
        "POST",
        &format!("/api/app-agents/threads/{tid}/turns"),
        Some(json!({ "text": "Tranh biện kịch bản tỷ giá quý tới" })),
    )
    .await;
    assert_eq!(st_deb, StatusCode::OK);
    let deb_msgs = turn_deb["messages"].as_array().unwrap();
    assert_eq!(deb_msgs.len(), 6, "3 opening theses (round 1) + 3 cross-rebuttals (round 2)");
    assert!(deb_msgs.iter().any(|m| m["round_idx"] == 1));
    assert!(deb_msgs.iter().any(|m| m["round_idx"] == 2));

    // 3D. Switch to `mention_or_lead` and verify cyclic `@mention` ping-pong stops at `<= 3` peer hops (4 total rounds):
    authed_json(
        &app,
        &cookie,
        "PATCH",
        &format!("/api/app-agents/threads/{tid}"),
        Some(json!({ "discussion_mode": "mention_or_lead", "lead_slug": "macro-argus" })),
    )
    .await;
    let (st_hop, turn_hop) = authed_json(
        &app,
        &cookie,
        "POST",
        &format!("/api/app-agents/threads/{tid}/turns"),
        Some(json!({
            "text": "PING_PONG_HOPS @macro-argus kiểm tra chéo với phòng thuế",
            "mentions": ["macro-argus"]
        })),
    )
    .await;
    assert_eq!(st_hop, StatusCode::OK);
    let hop_msgs = turn_hop["messages"].as_array().unwrap();
    assert_eq!(
        hop_msgs.len(),
        4,
        "initial speaker (hop 0) + max 3 peer consult hops = 4 bounded messages"
    );
    assert_eq!(hop_msgs.last().unwrap()["round_idx"], 4);

    // 3E. Trigger an upstream LLM error (`fail_next = true`) on a single-agent thread so `error` SSE event is emitted too,
    // proving all 7 SSE event kinds (`seat_state`, `text`, `reasoning`, `tool`, `tool_result`, `result`, `error`)!
    llm_state.fail_next.store(true, Ordering::SeqCst);
    // Disable backup_model on `macro-argus` momentarily so single failure surfaces `error` SSE event directly:
    let (_, agents_list) = authed_json(&app, &cookie, "GET", "/api/app-agents/agents", None).await;
    let mut argus = agents_list["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["slug"] == "macro-argus")
        .unwrap()
        .clone();
    argus["backup_model"] = json!("");
    let (st_put_argus, _) =
        authed_json(&app, &cookie, "PUT", "/api/app-agents/agents/macro-argus", Some(argus)).await;
    assert_eq!(st_put_argus, StatusCode::OK);

    let (st_err_turn, _) = authed_json(
        &app,
        &cookie,
        "POST",
        &format!("/api/app-agents/threads/{tid}/turns"),
        Some(json!({ "text": "@macro-argus test error event", "mentions": ["macro-argus"] })),
    )
    .await;
    assert_eq!(st_err_turn, StatusCode::BAD_GATEWAY);

    // Replay ring buffer from `since_seq = 0` and verify ALL 7 SSE event kinds are present!
    let (replayed, _rx) = state.agent_bridge.subscribe_stream(&tid, Some(0));
    let kinds: HashSet<String> = replayed.iter().map(|e| e.kind.clone()).collect();
    for expected_kind in [
        "seat_state",
        "text",
        "reasoning",
        "tool",
        "tool_result",
        "result",
        "error",
    ] {
        assert!(
            kinds.contains(expected_kind),
            "missing SSE event kind `{expected_kind}` in {kinds:?}"
        );
    }

    // Also verify HTTP `GET /api/app-agents/threads/{id}/stream?since_seq=0` streams valid SSE frames!
    let sse_req = Request::builder()
        .method("GET")
        .uri(format!("/api/app-agents/threads/{tid}/stream?since_seq=0"))
        .header(header::HOST, "newsman.test")
        .header(header::COOKIE, &cookie)
        .body(Body::empty())
        .unwrap();
    let sse_resp = app.clone().oneshot(sse_req).await.unwrap();
    assert_eq!(sse_resp.status(), StatusCode::OK);
    let mut body_stream = sse_resp.into_body().into_data_stream();
    let first_chunk = tokio::time::timeout(Duration::from_secs(2), body_stream.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    let second_chunk = tokio::time::timeout(Duration::from_secs(2), body_stream.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    let combined_sse = format!(
        "{}{}",
        std::str::from_utf8(&first_chunk).unwrap(),
        std::str::from_utf8(&second_chunk).unwrap()
    );
    assert!(combined_sse.contains("event: seat_state\n"));

    // 3F. Whiteboard CRUD + `POST /round-summary`:
    let (st_wb_add, wb_added) = authed_json(
        &app,
        &cookie,
        "POST",
        &format!("/api/app-agents/threads/{tid}/whiteboard"),
        Some(json!({ "content": "Chốt số liệu CPI 3,42% YoY", "author_slug": "owner", "pinned": true })),
    )
    .await;
    assert_eq!(st_wb_add, StatusCode::OK);
    let note_id = wb_added["note"]["id"].as_str().unwrap().to_string();

    let (st_sum, wb_sum) = authed_json(
        &app,
        &cookie,
        "POST",
        &format!("/api/app-agents/threads/{tid}/round-summary"),
        Some(json!({})),
    )
    .await;
    assert_eq!(st_sum, StatusCode::OK, "{wb_sum}");

    let (_, wb_list) = authed_json(
        &app,
        &cookie,
        "GET",
        &format!("/api/app-agents/threads/{tid}/whiteboard"),
        None,
    )
    .await;
    assert!(wb_list["items"].as_array().unwrap().len() >= 2);

    let (st_del_wb, _) = authed_json(
        &app,
        &cookie,
        "DELETE",
        &format!("/api/app-agents/threads/{tid}/whiteboard/{note_id}"),
        None,
    )
    .await;
    assert_eq!(st_del_wb, StatusCode::OK);

    llm_task.abort();
}

// ---------------------------------------------------------------------------
// Test 4: Studio CRUD + SecretBox Redaction/Persistence + Missing LNC_MASTER_KEY + Gated `brief_propose`
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_4_studio_crud_secretbox_redaction_persistence_missing_master_key_and_gated_brief_propose() {
    // 4A. Graceful degradation when `state.secrets == None` (`LNC_MASTER_KEY` absent):
    {
        let dir_no_key = unique_temp_dir("t4-nokey");
        let db_no_key = Database::in_memory().unwrap();
        let state_no_key = build_test_state(dir_no_key, db_no_key, Fetcher::public_only(), None);
        let app_no_key = crate::router(state_no_key);
        let cookie_nk = login_cookie(&app_no_key).await;

        let (st_get, cfg_nk) =
            authed_json(&app_no_key, &cookie_nk, "GET", "/api/app-agents/bridge-config", None).await;
        assert_eq!(st_get, StatusCode::OK);
        assert_eq!(cfg_nk["secret_store_open"], false);

        // Saving non-secret URLs works cleanly even without `LNC_MASTER_KEY`:
        let (st_non_sec, _) = authed_json(
            &app_no_key,
            &cookie_nk,
            "PUT",
            "/api/app-agents/bridge-config",
            Some(json!({ "default_model": "gpt-5.4" })),
        )
        .await;
        assert_eq!(st_non_sec, StatusCode::OK);

        // Attempting to write a secret when `secrets == None` returns 503 `secret_store_not_configured` (never panics or stores plaintext):
        let (st_sec_fail, body_sec_fail) = authed_json(
            &app_no_key,
            &cookie_nk,
            "PUT",
            "/api/app-agents/bridge-config",
            Some(json!({ "astraea_token": "must-not-store-plaintext" })),
        )
        .await;
        assert_eq!(st_sec_fail, StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(body_sec_fail["error"], "secret_store_not_configured");
    }

    // 4B. On-disk DB + `SecretBox` persistence across restart + redaction + Studio CRUD + gated `brief_propose`:
    let dir = unique_temp_dir("t4-persist");
    let db_path = dir.join("newsman.db");
    let master_bytes = [0x5A; 32];
    let (llm_url, _llm_st, llm_task) = spawn_mock_llm().await;

    let thread_id_saved: String;
    {
        let db = Database::open(&db_path).unwrap();
        let _ = seed_newsroom_fixture(&db);
        let vault = secrets::SecretBox::from_bytes(master_bytes, None);
        let state = build_test_state(
            dir.clone(),
            db,
            Fetcher::allow_loopback_for_tests(),
            Some(vault),
        );
        let app = crate::router(state.clone());
        let cookie = login_cookie(&app).await;

        // Verify 3 seeded personas exist:
        let (_, agents_res) = authed_json(&app, &cookie, "GET", "/api/app-agents/agents", None).await;
        let slugs: Vec<&str> = agents_res["items"]
            .as_array()
            .unwrap()
            .iter()
            .map(|a| a["slug"].as_str().unwrap())
            .collect();
        assert!(slugs.contains(&"macro-argus"));
        assert!(slugs.contains(&"tax-linh"));
        assert!(slugs.contains(&"editor-minh"));

        // Reject raw secret in `remote_endpoint_cfg.secret_ref` (must be `vault://`):
        let mut custom = agents_res["items"][0].clone();
        custom["slug"] = json!("fx-desk");
        custom["remote_endpoint_cfg"] = json!({ "url": llm_url, "secret_ref": "sk-raw-secret-forbidden" });
        let (st_bad_ref, _) = authed_json(
            &app,
            &cookie,
            "PUT",
            "/api/app-agents/agents/fx-desk",
            Some(custom.clone()),
        )
        .await;
        assert_eq!(st_bad_ref, StatusCode::BAD_REQUEST);

        // Save valid `vault://` custom agent + update `editor-minh`:
        custom["remote_endpoint_cfg"] = json!({ "url": llm_url, "secret_ref": "vault://app-agents-hub-token" });
        custom["display_name"] = json!("Chuyên gia Tỷ giá FX");
        let (st_ok_agent, _) =
            authed_json(&app, &cookie, "PUT", "/api/app-agents/agents/fx-desk", Some(custom)).await;
        assert_eq!(st_ok_agent, StatusCode::OK);

        // Seal secrets via `PUT /api/app-agents/bridge-config` & `telegram/config`:
        let super_secret_astraea = "ast_live_999888777_never_echo";
        let super_secret_tg = "123456:ABC-DEF-never-echo-tg-bot";
        let (st_cfg, cfg_saved) = authed_json(
            &app,
            &cookie,
            "PUT",
            "/api/app-agents/bridge-config",
            Some(json!({
                "astraea_url": "https://astraea.example.test",
                "hub_url": llm_url,
                "default_model": "gpt-5.4",
                "astraea_token": super_secret_astraea,
                "hub_token": "hub_secret_xyz",
            })),
        )
        .await;
        assert_eq!(st_cfg, StatusCode::OK);
        let dumped_cfg = cfg_saved.to_string();
        assert!(!dumped_cfg.contains(super_secret_astraea), "secret must NEVER be echoed");
        assert_eq!(cfg_saved["has_astraea_token"], true);
        assert_eq!(cfg_saved["has_hub_token"], true);

        let (st_tg, tg_saved) = authed_json(
            &app,
            &cookie,
            "PUT",
            "/api/app-agents/telegram/config",
            Some(json!({
                "transport_mode": "direct_webhook",
                "bot_name": "newsman_room_bot",
                "chat_policy": "mention",
                "bot_token": super_secret_tg,
                "webhook_secret": "wh_sec_777",
                "cygnus_key": "cyg_key_888",
            })),
        )
        .await;
        assert_eq!(st_tg, StatusCode::OK);
        assert!(!tg_saved.to_string().contains(super_secret_tg));
        assert_eq!(tg_saved["has_bot_token"], true);
        assert_eq!(tg_saved["has_webhook_secret"], true);
        assert_eq!(tg_saved["has_cygnus_key"], true);

        // Verify raw SQLite `secrets` table holds ciphertext only, NOT plaintext:
        {
            let db_guard = state.db.lock().unwrap();
            let raw_ct: String = db_guard
                .conn
                .query_row(
                    "SELECT ciphertext FROM secrets WHERE name = 'app-agents-astraea-token'",
                    [],
                    |r| r.get(0),
                )
                .unwrap();
            assert!(!raw_ct.contains(super_secret_astraea));
        }

        // Verify `src/telegram.rs` alert outbox settings (`GET /api/settings`) remain untouched!
        let (st_alert_settings, alert_settings) =
            authed_json(&app, &cookie, "GET", "/api/settings", None).await;
        assert_eq!(st_alert_settings, StatusCode::OK);
        assert_eq!(alert_settings["telegram_mode_stored"], "off");
        assert_eq!(alert_settings["telegram_has_bot_token"], false, "alert bot token slug is separate from conversational bot token");

        // Execute gated `brief_propose` via `editor-minh` turn and verify `approval_required` metadata!
        let (_, thr_res) = authed_json(
            &app,
            &cookie,
            "POST",
            "/api/app-agents/threads",
            Some(json!({
                "kind": "direct",
                "title": "Duyệt bản tin sáng",
                "member_slugs": ["editor-minh"],
                "discussion_mode": "mention_or_lead",
            })),
        )
        .await;
        thread_id_saved = thr_res["thread"]["id"].as_str().unwrap().to_string();

        let (st_prop, turn_prop) = authed_json(
            &app,
            &cookie,
            "POST",
            &format!("/api/app-agents/threads/{thread_id_saved}/turns"),
            Some(json!({ "text": "TRIGGER_BRIEF_PROPOSE Soạn đề xuất bản tin sáng VN" })),
        )
        .await;
        assert_eq!(st_prop, StatusCode::OK, "{turn_prop}");
        let msg0 = &turn_prop["messages"][0];
        let pill = &msg0["tool_calls"][0];
        assert_eq!(pill["tool"], "brief_propose");
        assert_eq!(pill["status"], "approval_required");
        assert_eq!(
            pill["approval_required"]["action_type"],
            "newsman.brief.publish"
        );
        assert!(
            pill["approval_required"]["request_ref"]
                .as_str()
                .unwrap()
                .starts_with("apr_brief_")
        );

        // Verify `newsman_analyses` was NOT published directly — only queued in `app_agent_brief_proposals`!
        let live_analyses: i64 = state
            .db
            .lock()
            .unwrap()
            .conn
            .query_row("SELECT COUNT(*) FROM newsman_analyses", [], |r| r.get(0))
            .unwrap();
        assert_eq!(live_analyses, 0, "gated brief_propose must NOT publish directly without approval");
    }

    // Reopen SQLite DB from disk with same `SecretBox` key (`seed_defaults` runs again on `Database::open`!)
    // and prove custom agent, modified config, sealed secrets, and messages all survive intact!
    {
        let db2 = Database::open(&db_path).unwrap();
        let vault2 = secrets::SecretBox::from_bytes(master_bytes, None);
        let state2 = build_test_state(dir, db2, Fetcher::allow_loopback_for_tests(), Some(vault2));
        let app2 = crate::router(state2);
        let cookie2 = login_cookie(&app2).await;

        let (_, cfg2) = authed_json(&app2, &cookie2, "GET", "/api/app-agents/bridge-config", None).await;
        assert_eq!(cfg2["default_model"], "gpt-5.4");
        assert_eq!(cfg2["has_astraea_token"], true);
        assert_eq!(cfg2["has_hub_token"], true);

        let (_, agents2) = authed_json(&app2, &cookie2, "GET", "/api/app-agents/agents", None).await;
        assert!(
            agents2["items"]
                .as_array()
                .unwrap()
                .iter()
                .any(|a| a["slug"] == "fx-desk" && a["display_name"] == "Chuyên gia Tỷ giá FX")
        );

        let (_, msgs2) = authed_json(
            &app2,
            &cookie2,
            "GET",
            &format!("/api/app-agents/threads/{thread_id_saved}/messages"),
            None,
        )
        .await;
        assert_eq!(msgs2["items"].as_array().unwrap().len(), 2);
    }

    llm_task.abort();
}

// ---------------------------------------------------------------------------
// Test 5: Dual-Mode Conversational Telegram (4 Transports + 3 Policies + Topic Threading) & Scheduler
// ---------------------------------------------------------------------------

#[derive(Default)]
struct MockTelegramState {
    direct_updates: Mutex<Vec<Value>>,
    direct_sent: Mutex<Vec<Value>>,
    cygnus_replies: Mutex<Vec<Value>>,
    cygnus_sent: Mutex<Vec<(Option<String>, Value)>>,
}

async fn spawn_mock_telegram_and_cygnus() -> (String, Arc<MockTelegramState>, tokio::task::JoinHandle<()>) {
    let st = Arc::new(MockTelegramState::default());
    let st_c = st.clone();
    let app = Router::new()
        .route(
            "/bottest_bot_tok/getUpdates",
            get(|State(s): State<Arc<MockTelegramState>>| async move {
                let items: Vec<Value> = s.direct_updates.lock().unwrap().drain(..).collect();
                Json(json!({ "ok": true, "result": items }))
            }),
        )
        .route(
            "/bottest_bot_tok/sendMessage",
            post(|State(s): State<Arc<MockTelegramState>>, Json(b): Json<Value>| async move {
                s.direct_sent.lock().unwrap().push(b);
                Json(json!({ "ok": true, "result": { "message_id": 901 } }))
            }),
        )
        .route(
            "/replies",
            get(|State(s): State<Arc<MockTelegramState>>, headers: HeaderMap| async move {
                assert_eq!(
                    headers.get("x-gateway-key").and_then(|v| v.to_str().ok()),
                    Some("cyg-secret-key")
                );
                let items: Vec<Value> = s.cygnus_replies.lock().unwrap().drain(..).collect();
                Json(json!({ "ok": true, "replies": items }))
            }),
        )
        .route(
            "/send",
            post(
                |State(s): State<Arc<MockTelegramState>>, headers: HeaderMap, Json(b): Json<Value>| async move {
                    let gw = headers
                        .get("x-gateway-key")
                        .and_then(|v| v.to_str().ok())
                        .map(str::to_string);
                    s.cygnus_sent.lock().unwrap().push((gw, b));
                    Json(json!({ "ok": true }))
                },
            ),
        )
        .with_state(st_c);
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let handle = tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    (format!("http://{addr}"), st, handle)
}

#[tokio::test]
async fn test_5_dual_mode_conversational_telegram_and_scheduler() {
    let dir = unique_temp_dir("t5");
    let db = Database::in_memory().unwrap();
    let _ = seed_newsroom_fixture(&db);
    let vault = secrets::SecretBox::from_bytes([0x77; 32], None);
    let (llm_url, _llm_st, llm_task) = spawn_mock_llm().await;
    let (tg_url, tg_st, tg_task) = spawn_mock_telegram_and_cygnus().await;

    let state = build_test_state(dir, db, Fetcher::allow_loopback_for_tests(), Some(vault));
    let app = crate::router(state.clone());
    let cookie = login_cookie(&app).await;

    authed_json(
        &app,
        &cookie,
        "PUT",
        "/api/app-agents/bridge-config",
        Some(json!({
            "hub_url": llm_url,
            "hub_token": "hub-tok",
            "allowed_hosts": ["127.0.0.1"],
        })),
    )
    .await;

    authed_json(
        &app,
        &cookie,
        "PUT",
        "/api/app-agents/telegram/config",
        Some(json!({
            "transport_mode": "direct_webhook",
            "bot_name": "newsman_bot",
            "api_base_url": tg_url,
            "cygnus_url": tg_url,
            "chat_policy": "mention",
            "bot_token": "test_bot_tok",
            "webhook_secret": "direct-wh-sec",
            "cygnus_key": "cyg-secret-key",
        })),
    )
    .await;

    // Create routes testing `mention`, `topics_only`, and `open` policies across Direct & Cygnus:
    // Route 1: chat `-10011`, topic `777`, `direct_webhook`, policy `mention` -> `macro-argus`
    authed_json(
        &app,
        &cookie,
        "POST",
        "/api/app-agents/telegram/routes",
        Some(json!({
            "transport_mode": "direct_webhook",
            "policy": "mention",
            "chat_id": "-10011",
            "message_thread_id": 777,
            "agent_slug": "macro-argus",
            "bot_name": "newsman_bot",
            "enabled": true,
        })),
    )
    .await;

    // Route 2: chat `-10022`, topic `888`, `cygnus_webhook`, policy `topics_only` -> `tax-linh`
    authed_json(
        &app,
        &cookie,
        "POST",
        "/api/app-agents/telegram/routes",
        Some(json!({
            "transport_mode": "cygnus_webhook",
            "policy": "topics_only",
            "chat_id": "-10022",
            "message_thread_id": 888,
            "agent_slug": "tax-linh",
            "bot_name": "newsman_bot",
            "enabled": true,
        })),
    )
    .await;

    // Route 3: chat `-10033`, `open` policy for `direct_poll` & Route 4: chat `-10044`, `open` policy for `cygnus_poll`
    authed_json(
        &app,
        &cookie,
        "POST",
        "/api/app-agents/telegram/routes",
        Some(json!({
            "transport_mode": "direct_poll",
            "policy": "open",
            "chat_id": "-10033",
            "message_thread_id": 555,
            "agent_slug": "editor-minh",
            "enabled": true,
        })),
    )
    .await;
    authed_json(
        &app,
        &cookie,
        "POST",
        "/api/app-agents/telegram/routes",
        Some(json!({
            "transport_mode": "cygnus_poll",
            "policy": "open",
            "chat_id": "-10044",
            "message_thread_id": 666,
            "agent_slug": "macro-argus",
            "bot_name": "newsman_bot",
            "enabled": true,
        })),
    )
    .await;

    // 5A. `direct_webhook`: wrong secret -> 401; unmentioned message -> skipped by `mention` policy; `@newsman_bot` mentioned -> handled & replies to `message_thread_id: 777`!
    let bad_wh = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/app-agents/telegram/webhook/direct")
                .header(header::HOST, "newsman.test")
                .header(header::CONTENT_TYPE, "application/json")
                .header("x-telegram-bot-api-secret-token", "wrong-secret")
                .body(Body::from("{}"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(bad_wh.status(), StatusCode::UNAUTHORIZED);

    // Unmentioned in `-10011` topic `777` -> skipped (`policy_blocked`):
    let unmentioned_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/app-agents/telegram/webhook/direct")
                .header(header::HOST, "newsman.test")
                .header(header::CONTENT_TYPE, "application/json")
                .header("x-telegram-bot-api-secret-token", "direct-wh-sec")
                .body(Body::from(
                    json!({
                        "update_id": 101,
                        "message": { "chat": { "id": -10011 }, "message_thread_id": 777, "text": "Bàn luận chung không tag bot" }
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    let unm_json: Value =
        serde_json::from_slice(&unmentioned_resp.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(unm_json["handled"], false);
    assert_eq!(unm_json["reason"], "mention_required");

    // Mentioned `@newsman_bot` in `-10011` topic `777` -> handled & outbound `sendMessage` carries `message_thread_id: 777`!
    let mentioned_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/app-agents/telegram/webhook/direct")
                .header(header::HOST, "newsman.test")
                .header(header::CONTENT_TYPE, "application/json")
                .header("x-telegram-bot-api-secret-token", "direct-wh-sec")
                .body(Body::from(
                    json!({
                        "update_id": 102,
                        "message": { "chat": { "id": -10011 }, "message_thread_id": 777, "text": "@newsman_bot Điểm tin vĩ mô sáng nay" }
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    let men_json: Value =
        serde_json::from_slice(&mentioned_resp.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(men_json["handled"], true, "{men_json}");
    {
        let sent = tg_st.direct_sent.lock().unwrap();
        assert_eq!(sent.len(), 1);
        assert_eq!(sent[0]["chat_id"], "-10011");
        assert_eq!(sent[0]["message_thread_id"], 777);
    }

    // 5B. `cygnus_webhook` + `topics_only` policy: message without `message_thread_id` is skipped; message with `message_thread_id: 888` is handled & sent via Cygnus `/send` with `payload.message_thread_id == 888`!
    let cyg_no_topic = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/app-agents/telegram/webhook/cygnus")
                .header(header::HOST, "newsman.test")
                .header(header::CONTENT_TYPE, "application/json")
                .header("x-gateway-key", "cyg-secret-key")
                .body(Body::from(
                    json!({
                        "id": 201,
                        "bot": "newsman_bot",
                        "chat_id": "-10022",
                        "text": "General room message without topic"
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    let cyg_skip: Value =
        serde_json::from_slice(&cyg_no_topic.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(cyg_skip["handled"], false);

    let cyg_ok_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/app-agents/telegram/webhook/cygnus")
                .header(header::HOST, "newsman.test")
                .header(header::CONTENT_TYPE, "application/json")
                .header("x-gateway-key", "cyg-secret-key")
                .body(Body::from(
                    json!({
                        "id": 202,
                        "bot": "newsman_bot",
                        "correlation_id": "corr-202",
                        "payload": {
                            "chat_id": "-10022",
                            "message_thread_id": 888,
                            "text": "Hạn nộp thuế GTGT tháng này?"
                        }
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    let cyg_ok: Value =
        serde_json::from_slice(&cyg_ok_resp.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(cyg_ok["handled"], true, "{cyg_ok}");
    {
        let csent = tg_st.cygnus_sent.lock().unwrap();
        assert_eq!(csent.len(), 1);
        assert_eq!(csent[0].0.as_deref(), Some("cyg-secret-key"));
        assert_eq!(csent[0].1["bot"], "newsman_bot");
        assert_eq!(csent[0].1["correlation_id"], "corr-202");
        assert_eq!(csent[0].1["payload"]["message_thread_id"], 888);
    }

    // 5C. `direct_poll` & `cygnus_poll` background pollers preserve offsets and `message_thread_id` (`555` and `666`):
    tg_st.direct_updates.lock().unwrap().push(json!({
        "update_id": 301,
        "message": { "chat": { "id": -10033 }, "message_thread_id": 555, "text": "Cập nhật nhanh bản tin chiều" }
    }));
    let direct_polled = agent_bridge::poll_direct_once(state.clone()).await.unwrap();
    assert_eq!(direct_polled, 1);
    assert_eq!(tg_st.direct_sent.lock().unwrap().last().unwrap()["message_thread_id"], 555);

    tg_st.cygnus_replies.lock().unwrap().push(json!({
        "id": 401,
        "bot": "newsman_bot",
        "correlation_id": "corr-401",
        "payload": { "chat_id": "-10044", "message_thread_id": 666, "text": "Kiểm tra số liệu CPI qua Cygnus poll" }
    }));
    let cygnus_polled = agent_bridge::poll_cygnus_once(state.clone()).await.unwrap();
    assert_eq!(cygnus_polled, 1);
    assert_eq!(
        tg_st.cygnus_sent.lock().unwrap().last().unwrap().1["payload"]["message_thread_id"],
        666
    );

    // 5D. Cron / Interval Scheduler (`POST /schedules`, `POST /run-now`, and `sweep_due_schedules_once`):
    let (st_sched, sched_created) = authed_json(
        &app,
        &cookie,
        "POST",
        "/api/app-agents/schedules",
        Some(json!({
            "name": "Bản tin vĩ mô đầu ngày",
            "cron_or_interval_secs": "15m",
            "agent_slug": "macro-argus",
            "prompt": "Tổng hợp các diễn biến lãi suất mới nhất",
            "enabled": true,
        })),
    )
    .await;
    assert_eq!(st_sched, StatusCode::OK, "{sched_created}");
    let sched_id = sched_created["schedule"]["id"].as_str().unwrap().to_string();

    // `POST /run-now`:
    let (st_rn, rn_body) = authed_json(
        &app,
        &cookie,
        "POST",
        &format!("/api/app-agents/schedules/{sched_id}/run-now"),
        Some(json!({})),
    )
    .await;
    assert_eq!(st_rn, StatusCode::OK, "{rn_body}");
    assert_eq!(rn_body["messages"].as_array().unwrap().len(), 1);

    // Force `next_run_at` into the past and verify `sweep_due_schedules_once` fires it and advances `next_run_at`!
    {
        let db_guard = state.db.lock().unwrap();
        db_guard
            .conn
            .execute(
                "UPDATE app_agent_schedules SET next_run_at = '2020-01-01T00:00:00Z' WHERE id = ?1",
                [&sched_id],
            )
            .unwrap();
    }
    let fired = agent_bridge::sweep_due_schedules_once(state.clone()).await.unwrap();
    assert_eq!(fired, 1);

    let (_, scheds_after) = authed_json(&app, &cookie, "GET", "/api/app-agents/schedules", None).await;
    let s0 = &scheds_after["items"][0];
    assert_eq!(s0["last_status"], "ok");
    assert!(s0["next_run_at"].as_str().unwrap() > "2026-01-01");

    tg_task.abort();
    llm_task.abort();
}

#[tokio::test]
async fn step3_workspaces_skills_intersection_and_probe() {
    let dir = unique_temp_dir("step3");
    let db = Database::in_memory().unwrap();
    let _ = seed_newsroom_fixture(&db);
    let vault = secrets::SecretBox::from_bytes([0x55; 32], None);
    let (llm_url, llm_state, llm_task) = spawn_mock_llm().await;
    let state = build_test_state(dir, db, Fetcher::allow_loopback_for_tests(), Some(vault));
    let app = crate::router(state.clone());
    let cookie = login_cookie(&app).await;

    // 1. Seeded workspaces & skills are present from Migration 22
    let (st_ws, ws_body) = authed_json(&app, &cookie, "GET", "/api/app-agents/workspaces", None).await;
    assert_eq!(st_ws, StatusCode::OK, "{ws_body}");
    let ws_items = ws_body["items"].as_array().unwrap();
    assert!(ws_items.iter().any(|w| w["id"] == "newsroom-general" && w["is_default"] == true));
    assert!(ws_items.iter().any(|w| w["id"] == "vn-macro-tax-desk"));

    // Create + update + delete custom workspace
    let (st_ws_post, ws_created) = authed_json(
        &app,
        &cookie,
        "POST",
        "/api/app-agents/workspaces",
        Some(json!({
            "id": "asean-fx-desk",
            "name": "Bàn Tỷ giá ASEAN",
            "emoji": "💱",
            "custom_instructions_md": "Luôn đối chiếu dữ liệu tỷ giá NHNN và Fed.",
            "resource_ctx": { "regions": ["vn", "asean"], "topics": ["macro", "fx"] },
            "default_skill_slugs": ["macro-cross-impact-brief"],
            "default_agent_slugs": ["macro-argus"],
            "default_discussion_mode": "parallel_panel",
            "is_default": false,
        })),
    )
    .await;
    assert_eq!(st_ws_post, StatusCode::OK, "{ws_created}");
    assert_eq!(ws_created["workspace"]["id"], "asean-fx-desk");

    // 2. Fail-closed effective_resource_ctx intersection (regions + topics)
    let ws_parsed: agent_bridge::AgentWorkspace =
        serde_json::from_value(ws_created["workspace"].clone()).unwrap();
    let mut agent_cfg: agent_bridge::AppAgentConfig = serde_json::from_value(
        authed_json(&app, &cookie, "GET", "/api/app-agents/agents", None)
            .await
            .1["items"][0]
            .clone(),
    )
    .unwrap();
    agent_cfg.default_resource_ctx = connector::ResourceCtx {
        regions: Some(vec!["asean".into(), "eu".into()]),
        topics: Some(vec!["energy".into()]), // disjoint from ["macro", "fx"]!
    };
    let eff = agent_bridge::effective_resource_ctx(Some(&ws_parsed), &agent_cfg);
    assert_eq!(eff.regions, Some(vec!["asean".to_string()]));
    assert_eq!(eff.topics, Some(Vec::<String>::new())); // empty vec => mcp::AllowedScope fails closed!

    // Delete workspace
    let (st_ws_del, _) =
        authed_json(&app, &cookie, "DELETE", "/api/app-agents/workspaces/asean-fx-desk", None).await;
    assert_eq!(st_ws_del, StatusCode::OK);

    // 3. Skills: PATCH cannot elevate granted; ungranted skills never injected even if requested!
    let (st_sk, sk_body) = authed_json(&app, &cookie, "GET", "/api/app-agents/skills", None).await;
    assert_eq!(st_sk, StatusCode::OK, "{sk_body}");
    let sk_items = sk_body["items"].as_array().unwrap();
    assert!(sk_items.iter().any(|s| s["slug"] == "statutory-tax-citation-guard" && s["granted"] == true));
    assert!(sk_items.iter().any(|s| s["slug"] == "global-commodities-pack-sync" && s["granted"] == false));

    let (st_patch_sk, patched_sk) = authed_json(
        &app,
        &cookie,
        "PATCH",
        "/api/app-agents/skills/global-commodities-pack-sync",
        Some(json!({ "enabled": false, "granted": true })),
    )
    .await;
    assert_eq!(st_patch_sk, StatusCode::OK, "{patched_sk}");
    assert_eq!(patched_sk["skill"]["enabled"], false);
    assert_eq!(patched_sk["skill"]["granted"], false, "PATCH must never elevate granted");

    // Re-enable locally so only `granted == false` blocks it, then verify prompt injection strictly uses granted+enabled
    let _ = authed_json(
        &app,
        &cookie,
        "PATCH",
        "/api/app-agents/skills/global-commodities-pack-sync",
        Some(json!({ "enabled": true })),
    )
    .await;

    // Save bridge config with allowed_hosts + task_models + secret llm_api_key
    let (st_cfg, cfg_body) = authed_json(
        &app,
        &cookie,
        "PUT",
        "/api/app-agents/bridge-config",
        Some(json!({
            "hub_url": llm_url,
            "llm_base_url": llm_url,
            "allowed_hosts": ["127.0.0.1"],
            "llm_api_key": "super-secret-llm-key-should-never-leak",
            "hub_token": "super-secret-hub-token",
            "task_models": {
                "thread_title": "fast-title-model",
                "round_summary": "summary-model-override",
                "companion_brief": "companion-model",
                "tax_doc_digest": "tax-digest-model"
            }
        })),
    )
    .await;
    assert_eq!(st_cfg, StatusCode::OK, "{cfg_body}");
    assert_eq!(cfg_body["task_models"]["round_summary"], "summary-model-override");

    // 4. Probe calls GET /models ONLY (never /chat/completions) and never leaks secrets!
    let prev_calls = llm_state.calls.lock().unwrap().len();
    let (st_probe, probe_body) =
        authed_json(&app, &cookie, "POST", "/api/app-agents/bridge-config/probe", Some(json!({}))).await;
    assert_eq!(st_probe, StatusCode::OK, "{probe_body}");
    let probe_str = serde_json::to_string(&probe_body).unwrap();
    assert!(!probe_str.contains("super-secret"), "probe leaked secret: {probe_str}");
    assert_eq!(probe_body["llm"]["ok"], true);
    assert_eq!(
        llm_state.calls.lock().unwrap().len(),
        prev_calls,
        "probe must NOT invoke /chat/completions"
    );

    // 5. Granted-only skill injection & model_override in real turn execution
    let (st_thr, thr_created) = authed_json(
        &app,
        &cookie,
        "POST",
        "/api/app-agents/threads",
        Some(json!({
            "kind": "direct",
            "title": "Kiểm tra skill và model override",
            "workspace_id": "vn-macro-tax-desk",
            "member_slugs": ["tax-linh"],
            "pinned_skills": ["statutory-tax-citation-guard", "global-commodities-pack-sync"],
            "model_override": "thread-custom-model"
        })),
    )
    .await;
    assert_eq!(st_thr, StatusCode::OK, "{thr_created}");
    let thr_id = thr_created["thread"]["id"].as_str().unwrap().to_string();
    assert_eq!(thr_created["thread"]["workspace_id"], "vn-macro-tax-desk");
    assert_eq!(thr_created["thread"]["model_override"], "thread-custom-model");

    let (st_turn, turn_body) = authed_json(
        &app,
        &cookie,
        "POST",
        &format!("/api/app-agents/threads/{thr_id}/turns"),
        Some(json!({
            "text": "Rà soát hạn thuế chưa verified",
            "skill_slugs": ["editorial-factcheck-digest", "global-commodities-pack-sync"],
            "model_override": "turn-override-model"
        })),
    )
    .await;
    assert_eq!(st_turn, StatusCode::OK, "{turn_body}");
    {
        let calls = llm_state.calls.lock().unwrap();
        let req = calls.last().unwrap();
        assert_eq!(req["model"], "turn-override-model");
        let sys_msg = req["messages"][0]["content"].as_str().unwrap();
        assert!(sys_msg.contains("Bàn Vĩ mô & Thuế VN"), "workspace playbook missing");
        assert!(sys_msg.contains("Statutory Tax & Regulatory Citation Guard"), "granted pinned skill missing");
        assert!(sys_msg.contains("Editorial Fact-Check Digest"), "granted turn skill missing");
        assert!(
            !sys_msg.contains("Global Commodities Pack Sync"),
            "ungranted skill MUST NEVER enter system prompt!"
        );
        // Strip YAML frontmatter check
        assert!(!sys_msg.contains("name: statutory-tax-citation-guard"));
    }

    // 6. Access-request offline/404 fallback records `recorded_local`
    let (st_req, req_body) = authed_json(
        &app,
        &cookie,
        "POST",
        "/api/app-agents/skills/access-requests",
        Some(json!({
            "target_kind": "skill",
            "target_slug": "global-commodities-pack-sync",
            "reason": "Cần dữ liệu hàng hóa để đối chiếu lạm phát."
        })),
    )
    .await;
    assert_eq!(st_req, StatusCode::OK, "{req_body}");
    assert_eq!(req_body["request"]["status"], "recorded_local");

    let (st_reqs, reqs_list) =
        authed_json(&app, &cookie, "GET", "/api/app-agents/skills/access-requests", None).await;
    assert_eq!(st_reqs, StatusCode::OK, "{reqs_list}");
    assert_eq!(reqs_list["items"].as_array().unwrap().len(), 1);

    llm_task.abort();
}

#[derive(Default)]
struct MockHub0026State {
    phase: AtomicU64,
    post_calls: Mutex<Vec<Value>>,
}

async fn spawn_mock_hub_0026() -> (String, Arc<MockHub0026State>, tokio::task::JoinHandle<()>) {
    let st = Arc::new(MockHub0026State::default());
    let st_c = st.clone();
    let app = Router::new()
        .route(
            "/v1/app-connectors/lnc-newsman/skills",
            get(|State(s): State<Arc<MockHub0026State>>| async move {
                let phase = s.phase.load(Ordering::Relaxed);
                let (macro_granted, macro_body, macro_preview, alpha_granted, alpha_body, group_granted) =
                    if phase == 0 {
                        (
                            true,
                            "# Macro Signal\nFull playbook secret steps",
                            "Preview macro signal",
                            false,
                            "",
                            false,
                        )
                    } else {
                        (
                            false,
                            "",
                            "Preview macro signal (now locked)",
                            true,
                            "# Unlocked Alpha\nNow granted",
                            true,
                        )
                    };
                let granted_count = usize::from(macro_granted) + usize::from(alpha_granted);
                Json(json!({
                    "ok": true,
                    "app_id": "lnc-newsman",
                    "groups": [{
                        "slug": "market-intel",
                        "name": "Market Intel",
                        "display_name": "Gói Thị trường & Vĩ mô",
                        "description": "Phân tích chỉ báo vĩ mô & hàng hóa.",
                        "category": "markets",
                        "total_count": 2,
                        "granted_count": granted_count,
                        "granted": group_granted,
                        "skill_slugs": ["macro-signal", "locked-alpha"]
                    }],
                    "skills": [
                        {
                            "slug": "macro-signal",
                            "name": "macro-signal",
                            "display_name": "Tín hiệu Vĩ mô",
                            "description": "Macro signal skill",
                            "category": "markets",
                            "group_slugs": ["market-intel"],
                            "granted": macro_granted,
                            "version": "1.1.0",
                            "body_md": macro_body,
                            "body_preview": macro_preview
                        },
                        {
                            "slug": "locked-alpha",
                            "name": "locked-alpha",
                            "display_name": "Alpha Khóa",
                            "description": "Gated alpha skill",
                            "category": "markets",
                            "group_slugs": ["market-intel"],
                            "granted": alpha_granted,
                            "version": "1.0.0",
                            "body_md": alpha_body,
                            "body_preview": "Safe intro summary for locked-alpha"
                        }
                    ]
                }))
            }),
        )
        .route(
            "/v1/app-connectors/lnc-newsman/skill-access-requests",
            get(|State(s): State<Arc<MockHub0026State>>| async move {
                let phase = s.phase.load(Ordering::Relaxed);
                let items = if phase == 0 {
                    vec![json!({
                        "id": "hub-req-1",
                        "request_id": "hub-req-1",
                        "app_id": "lnc-newsman",
                        "target_kind": "skill_group",
                        "target_slug": "market-intel",
                        "reason": "Xin quyền phân tích hàng hóa.",
                        "status": "pending",
                        "approval_request_id": "appr-777",
                        "created_at": "2026-11-13T10:00:00Z",
                        "updated_at": "2026-11-13T10:00:00Z"
                    })]
                } else {
                    vec![
                        json!({
                            "id": "hub-req-1",
                            "request_id": "hub-req-1",
                            "app_id": "lnc-newsman",
                            "target_kind": "skill_group",
                            "target_slug": "market-intel",
                            "reason": "Xin quyền phân tích hàng hóa.",
                            "status": "approved",
                            "reviewed_by": "owner@lnc",
                            "review_note": "Approved for editorial testing",
                            "approval_request_id": "appr-777",
                            "created_at": "2026-11-13T10:00:00Z",
                            "updated_at": "2026-11-13T11:00:00Z"
                        }),
                        json!({
                            "id": "hub-req-local-2",
                            "request_id": "hub-req-local-2",
                            "app_id": "lnc-newsman",
                            "target_kind": "skill",
                            "target_slug": "locked-alpha",
                            "reason": "Offline request created earlier.",
                            "status": "approved",
                            "reviewed_by": "owner@lnc",
                            "review_note": "Matched open recorded_local row",
                            "approval_request_id": "appr-888",
                            "created_at": "2026-11-13T09:30:00Z",
                            "updated_at": "2026-11-13T11:05:00Z"
                        }),
                    ]
                };
                Json(json!({ "ok": true, "data": items, "items": items }))
            })
            .post(|State(s): State<Arc<MockHub0026State>>, Json(body): Json<Value>| async move {
                s.post_calls.lock().unwrap().push(body);
                (
                    StatusCode::CREATED,
                    Json(json!({
                        "ok": true,
                        "request_id": "hub-req-1",
                        "id": "hub-req-1",
                        "hub_request_id": "hub-req-1",
                        "status": "pending",
                        "approval_request_id": "appr-777"
                    })),
                )
            }),
        )
        .with_state(st_c);
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let handle = tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    (format!("http://{addr}"), st, handle)
}

#[tokio::test]
async fn test_6_hub_0026_skill_groups_body_preview_revocation_and_approval_replay() {
    let dir = unique_temp_dir("hub0026");
    let db = Database::in_memory().unwrap();
    let _ = seed_newsroom_fixture(&db);
    let vault = secrets::SecretBox::from_bytes([0x66; 32], None);
    let (hub_url, hub_state, hub_task) = spawn_mock_hub_0026().await;
    let state = build_test_state(dir, db, Fetcher::allow_loopback_for_tests(), Some(vault));
    let app = crate::router(state.clone());
    let cookie = login_cookie(&app).await;

    // Seed an offline recorded_local access request first (before Hub URL is set).
    let (st_loc, loc_body) = authed_json(
        &app,
        &cookie,
        "POST",
        "/api/app-agents/skills/access-requests",
        Some(json!({
            "target_kind": "skill",
            "target_slug": "locked-alpha",
            "reason": "Offline request created earlier."
        })),
    )
    .await;
    assert_eq!(st_loc, StatusCode::OK, "{loc_body}");
    assert_eq!(loc_body["request"]["status"], "recorded_local");
    let local_req_id = loc_body["request"]["id"].as_str().unwrap().to_string();

    // Point bridge-config at mock Hub 0026.
    let (st_cfg, cfg_body) = authed_json(
        &app,
        &cookie,
        "PUT",
        "/api/app-agents/bridge-config",
        Some(json!({
            "hub_url": hub_url,
            "hub_token": "hub-0026-token",
            "allowed_hosts": ["127.0.0.1"]
        })),
    )
    .await;
    assert_eq!(st_cfg, StatusCode::OK, "{cfg_body}");

    // Phase 0 sync: groups[], display_name, body_preview, granted body_md vs locked empty body_md.
    let (st_sync0, sync0) =
        authed_json(&app, &cookie, "POST", "/api/app-agents/skills/sync", Some(json!({}))).await;
    assert_eq!(st_sync0, StatusCode::OK, "{sync0}");
    assert_eq!(sync0["synced"], true);
    assert_eq!(sync0["sync_mode"], "hub_app_connector");
    let skills0 = sync0["items"].as_array().unwrap();
    let macro0 = skills0.iter().find(|s| s["slug"] == "macro-signal").unwrap();
    assert_eq!(macro0["display_name"], "Tín hiệu Vĩ mô");
    assert_eq!(macro0["granted"], true);
    assert_eq!(macro0["body_md"], "# Macro Signal\nFull playbook secret steps");
    assert_eq!(macro0["body_preview"], "Preview macro signal");

    let alpha0 = skills0.iter().find(|s| s["slug"] == "locked-alpha").unwrap();
    assert_eq!(alpha0["display_name"], "Alpha Khóa");
    assert_eq!(alpha0["granted"], false);
    assert_eq!(alpha0["body_md"], "");
    assert_eq!(alpha0["body_preview"], "Safe intro summary for locked-alpha");

    let groups0 = sync0["groups"].as_array().unwrap();
    let mi_grp0 = groups0.iter().find(|g| g["slug"] == "market-intel").unwrap();
    assert_eq!(mi_grp0["display_name"], "Gói Thị trường & Vĩ mô");
    assert_eq!(mi_grp0["category"], "markets");
    assert_eq!(mi_grp0["icon"], "📈");
    assert_eq!(mi_grp0["granted_count"], 1);
    assert_eq!(mi_grp0["total_count"], 2);
    assert_eq!(mi_grp0["granted"], false);

    // Submit access request twice for skill_group/market-intel -> HTTP 201 on Hub + idempotent upsert.
    for _ in 0..2 {
        let (st_r, rb) = authed_json(
            &app,
            &cookie,
            "POST",
            "/api/app-agents/skills/access-requests",
            Some(json!({
                "target_kind": "skill_group",
                "target_slug": "market-intel",
                "reason": "Xin quyền phân tích hàng hóa."
            })),
        )
        .await;
        assert_eq!(st_r, StatusCode::OK, "{rb}");
        assert_eq!(rb["request"]["status"], "pending");
        assert_eq!(rb["request"]["hub_request_id"], "hub-req-1");
        assert_eq!(rb["request"]["approval_request_id"], "appr-777");
    }
    let post_calls = hub_state.post_calls.lock().unwrap();
    assert_eq!(post_calls.len(), 2);
    assert_eq!(post_calls[0]["app_id"], "lnc-newsman");
    assert_eq!(post_calls[0]["target_kind"], "skill_group");
    assert_eq!(post_calls[0]["target_slug"], "market-intel");
    drop(post_calls);

    // Phase 1: Hub revokes macro-signal, grants locked-alpha, approves both requests.
    hub_state.phase.store(1, Ordering::Relaxed);
    let (st_sync1, sync1) =
        authed_json(&app, &cookie, "POST", "/api/app-agents/skills/sync", Some(json!({}))).await;
    assert_eq!(st_sync1, StatusCode::OK, "{sync1}");
    let skills1 = sync1["items"].as_array().unwrap();
    let macro1 = skills1.iter().find(|s| s["slug"] == "macro-signal").unwrap();
    assert_eq!(macro1["granted"], false);
    assert_eq!(
        macro1["body_md"], "",
        "revoked skill MUST clear cached body_md when excluded.granted = 0"
    );
    assert_eq!(macro1["body_preview"], "Preview macro signal (now locked)");

    let alpha1 = skills1.iter().find(|s| s["slug"] == "locked-alpha").unwrap();
    assert_eq!(alpha1["granted"], true);
    assert_eq!(alpha1["body_md"], "# Unlocked Alpha\nNow granted");

    let groups1 = sync1["groups"].as_array().unwrap();
    let mi_grp1 = groups1.iter().find(|g| g["slug"] == "market-intel").unwrap();
    assert_eq!(mi_grp1["granted"], true);
    assert_eq!(mi_grp1["granted_count"], 1);

    // Reconciled access requests: pending -> approved and recorded_local -> approved in-place.
    let reqs1 = sync1["access_requests"].as_array().unwrap();
    let grp_reqs: Vec<&Value> = reqs1
        .iter()
        .filter(|r| r["target_kind"] == "skill_group" && r["target_slug"] == "market-intel")
        .collect();
    assert_eq!(grp_reqs.len(), 1, "idempotent upsert must not duplicate: {reqs1:?}");
    assert_eq!(grp_reqs[0]["status"], "approved");
    assert_eq!(grp_reqs[0]["reviewed_by"], "owner@lnc");
    assert_eq!(grp_reqs[0]["review_note"], "Approved for editorial testing");
    assert_eq!(grp_reqs[0]["approval_request_id"], "appr-777");

    let local_reconciled = reqs1.iter().find(|r| r["id"] == local_req_id).unwrap();
    assert_eq!(local_reconciled["status"], "approved");
    assert_eq!(local_reconciled["hub_request_id"], "hub-req-local-2");
    assert_eq!(local_reconciled["reviewed_by"], "owner@lnc");
    assert_eq!(local_reconciled["approval_request_id"], "appr-888");

    hub_task.abort();
}
