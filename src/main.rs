mod admin_lifecycle;
#[allow(
    clippy::collapsible_if,
    clippy::collapsible_match,
    clippy::if_same_then_else,
    clippy::let_underscore_future,
    clippy::manual_is_multiple_of,
    clippy::result_large_err,
    clippy::single_match,
    clippy::too_many_arguments,
    clippy::unnecessary_lazy_evaluations
)]
mod agent_bridge;
#[cfg(test)]
#[allow(
    clippy::await_holding_lock,
    clippy::field_reassign_with_default,
    clippy::result_large_err
)]
mod agent_bridge_tests;
#[allow(
    clippy::collapsible_if,
    clippy::large_enum_variant,
    clippy::trim_split_whitespace,
    clippy::type_complexity
)]
mod agent_v2;
mod alerts;
#[allow(dead_code)]
mod auth;
#[cfg(test)]
mod blueprint_tests;
mod catalog;
#[cfg(test)]
mod catalog_tests;
mod classify;
mod collector;
#[cfg(test)]
mod collector_tests;
mod config;
mod connectors;
#[allow(dead_code, clippy::too_many_arguments)]
mod db;
#[cfg(test)]
mod docs_tests;
mod documents;
mod feed;
mod fetch;
mod filings;
mod hub;
mod llm;
mod legal;
#[allow(dead_code)]
mod connector;
mod mcp;
mod oidc;
mod providers;
mod readability;
mod reader;
mod regions;
mod secrets;
mod security;
mod series;
mod tax;
mod telegram;
mod themes;
mod write;

use axum::{
    Json, Router,
    extract::{ConnectInfo, Query, Request, State},
    http::{HeaderMap, HeaderValue, StatusCode, header, uri::Authority},
    middleware::{self, Next},
    response::{Html, IntoResponse, Redirect, Response},
    routing::{get, post},
};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashSet,
    net::SocketAddr,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tower_http::{services::ServeDir, trace::TraceLayer};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

struct AppState {
    pub(crate) cfg: config::Config,
    pub(crate) db: Mutex<db::Database>,
    login_limiter: Mutex<auth::LoginLimiter>,
    started_at: u64,
    fetcher: fetch::Fetcher,
    in_flight: Mutex<HashSet<i64>>,
    connector_in_flight: Mutex<HashSet<String>>,
    /// Last request per host, shared by every path that fetches (scheduler,
    /// "Fetch now", feed preview) so the per-host gap cannot be bypassed.
    host_gates: Mutex<std::collections::HashMap<String, std::time::Instant>>,
    /// Production uses `collector::HOST_GAP`; tests shrink it to keep the suite fast.
    host_gap: std::time::Duration,
    /// `None` when `LNC_MASTER_KEY` is absent; the secret store then stays closed.
    pub(crate) secrets: Option<secrets::SecretBox>,
    /// Operator-supplied provider settings (SEC contact User-Agent and similar).
    connector_config: connectors::ConnectorConfig,
    /// LLM provider configuration (owner-set base URL/model; off by default).
    llm: llm::LlmConfig,
    /// Hub/OpenAI-compatible provider layer (P5/P6): base URL, allowlist, model aliases.
    hub: hub::ProviderConfig,
    /// Telegram alert delivery: relay or direct, plus the hosts allowed to be private.
    telegram: telegram::TelegramConfig,
    /// Business MCP Hub connector (P8): app-specific bearer + delegation secret,
    /// resolved `env_or_cfg`, plus the single-use nonce table for replay defence.
    pub(crate) connector: connector::ConnectorConfig,
    /// Nonces already accepted, mapped to the moment they expire.
    pub(crate) connector_nonces: Mutex<std::collections::HashMap<String, u64>>,
    /// Admin switch for the LLM layer. Off means "rules only", which always works.
    llm_enabled: bool,
    pack: Mutex<regions::PackStatus>,
    /// True once the alert sweeper has been started; set exactly once, by login.
    alerts_running: AtomicBool,
    /// Astraea App-Scoped Conversational Agents Bridge & Hub Standby runtime.
    pub(crate) agent_bridge: Arc<agent_bridge::AgentBridgeRuntime>,
    /// Blueprint v2 security runtime (Argon2id auth state, HKDF keys, OIDC PKCE, limiters).
    pub(crate) security: Arc<security::SecurityRuntime>,
}
impl AppState {
    fn new(cfg: config::Config, db: db::Database, fetcher: fetch::Fetcher) -> Self {
        let agent_bridge = agent_bridge::AgentBridgeRuntime::new(&db);
        let security = Arc::new(security::SecurityRuntime::for_test(&cfg));
        Self {
            cfg,
            db: Mutex::new(db),
            login_limiter: Mutex::new(auth::LoginLimiter::default()),
            started_at: now(),
            fetcher,
            in_flight: Mutex::new(HashSet::new()),
            connector_in_flight: Mutex::new(HashSet::new()),
            host_gates: Mutex::new(std::collections::HashMap::new()),
            host_gap: collector::HOST_GAP,
            secrets: None,
            connector: connector::ConnectorConfig::default(),
            connector_nonces: Mutex::new(std::collections::HashMap::new()),
            connector_config: connectors::ConnectorConfig::default(),
            llm: llm::LlmConfig {
                base_url: None,
                model: "mock".into(),
            },
            hub: hub::ProviderConfig::default(),
            telegram: telegram::TelegramConfig::default(),
            llm_enabled: false,
            pack: Mutex::new(regions::PackStatus::default()),
            alerts_running: AtomicBool::new(false),
            agent_bridge,
            security,
        }
    }
}
type SharedState = Arc<AppState>;

#[derive(Deserialize)]
struct LoginRequest {
    key: Option<String>,
    token: Option<String>,
    password: Option<String>,
}
#[derive(Serialize)]
struct SettingsResponse {
    version: &'static str,
    language: String,
    default_reader_mode: String,
    collector_enabled: bool,
    fulltext_auto_enabled: bool,
    connectors_auto_enabled: bool,
    focus_profile: String,
    brief_morning_time: String,
    brief_evening_time: String,
    brief_timezone: String,
    /// Effective mode (env override applied) and the mode the admin stored.
    telegram_mode: &'static str,
    telegram_mode_stored: &'static str,
    telegram_env_override: bool,
    telegram_ready: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    telegram_reason: Option<&'static str>,
    telegram_chat_id: Option<i64>,
    telegram_alerts_topic_id: Option<i64>,
    telegram_has_relay_token: bool,
    telegram_has_bot_token: bool,
}
#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct SettingsPatch {
    language: Option<String>,
    default_reader_mode: Option<String>,
    collector_enabled: Option<bool>,
    fulltext_auto_enabled: Option<bool>,
    connectors_auto_enabled: Option<bool>,
    focus_profile: Option<String>,
    brief_morning_time: Option<String>,
    brief_evening_time: Option<String>,
    brief_timezone: Option<String>,
    // Telegram fields are listed here rather than flattened: `deny_unknown_fields`
    // stops working once a struct flattens another, and a typo must be an error.
    telegram_mode: Option<String>,
    telegram_chat_id: Option<String>,
    telegram_alerts_topic_id: Option<String>,
}

impl SettingsPatch {
    fn telegram(&self) -> telegram::TelegramSettingsPatch {
        telegram::TelegramSettingsPatch {
            telegram_mode: self.telegram_mode.clone(),
            telegram_chat_id: self.telegram_chat_id.clone(),
            telegram_alerts_topic_id: self.telegram_alerts_topic_id.clone(),
        }
    }
}
#[derive(Deserialize)]
struct DashboardQuery {
    region: Option<String>,
}
#[derive(Serialize)]
struct Dashboard {
    topics: Vec<db::Topic>,
    /// Enabled sources with a reader-visible item count, for the desktop browse list.
    sources: Vec<collector::SourceFacet>,
    stats: collector::Stats,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "lnc_newsman=info,tower_http=info".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();
    let cfg = config::Config::from_env()?;
    let db = db::Database::open(&cfg.db_path())?;
    let addr = SocketAddr::new(cfg.bind_address, cfg.port);
    let mut app_state = AppState::new(cfg, db, fetch::Fetcher::public_only());
    app_state.secrets = secrets::SecretBox::from_env();
    app_state.connector_config = connectors::ConnectorConfig::from_env();
    app_state.llm = llm::LlmConfig::from_env();
    app_state.hub = hub::ProviderConfig::from_env();
    app_state.telegram = telegram::TelegramConfig::from_env();
    app_state.connector = connector::ConnectorConfig::from_env();
    {
        let db_guard = app_state.db.lock().expect("database lock poisoned");
        app_state.security = Arc::new(security::SecurityRuntime::from_env(
            &db_guard,
            &app_state.cfg,
            app_state.secrets.as_ref(),
        ));
        app_state.agent_bridge = agent_bridge::AgentBridgeRuntime::new(&db_guard);
    }
    if app_state.secrets.is_none() {
        tracing::info!("LNC_MASTER_KEY is not set; the provider secret store stays closed");
    }
    let state = Arc::new(app_state);
    // Discovery only: registers regions and sources disabled; nothing is fetched.
    regions::reload(&state);
    // Registering a connector only adds a disabled row; nothing is fetched here.
    match connectors::register_missing(&state.db.lock().expect("database lock poisoned")) {
        Ok(0) => {}
        Ok(added) => tracing::info!(added, "provider connectors registered (all disabled)"),
        Err(err) => tracing::warn!(%err, "could not register provider connectors"),
    }
    // Legal sources are seeded from their audited terms verdicts, always disabled. A
    // source whose terms were not read can never be switched on by the gate.
    match legal::seed(&state.db.lock().expect("database lock poisoned")) {
        Ok(0) => {}
        Ok(added) => tracing::info!(added, "legal sources registered (all disabled)"),
        Err(err) => tracing::warn!(%err, "could not register legal sources"),
    }
    collector::spawn_scheduler(state.clone());
    collector::spawn_content_worker(state.clone());
    connectors::spawn_scheduler(state.clone());
    agent_bridge::spawn_background_workers(state.clone());
    tracing::info!(%addr, data_dir = %state.cfg.data_dir.display(), "starting lnc-newsman");
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(
        listener,
        router(state).into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown(async {
        let _ = tokio::signal::ctrl_c().await;
    })
    .await?;
    Ok(())
}

fn router(state: SharedState) -> Router {
    let session_only = Router::new()
        .route("/api/settings", get(get_settings).patch(patch_settings))
        .route("/api/dashboard", get(dashboard))
        .route(
            "/api/admin/topics",
            get(catalog::list_topics).post(catalog::create_topic),
        )
        .route(
            "/api/admin/topics/{slug}",
            axum::routing::put(catalog::update_topic),
        )
        .route(
            "/api/admin/sources",
            get(catalog::list_sources).post(catalog::create_source),
        )
        .route(
            "/api/admin/sources/{id}",
            axum::routing::put(catalog::update_source),
        )
        .route("/api/admin/sources/preview", post(collector::preview))
        .route("/api/admin/sources/presets", get(catalog::list_presets))
        .route(
            "/api/admin/sources/presets/{slug}/add",
            post(catalog::add_preset),
        )
        .route("/api/admin/sources/{id}/fetch", post(collector::fetch_now))
        .route("/api/admin/sources/{id}/resume", post(collector::resume))
        .route("/api/admin/sources/{id}/runs", get(collector::runs))
        .route("/api/admin/secrets", get(secrets::list))
        .route("/api/admin/secrets/rewrap", post(secrets::rewrap_handler))
        .route(
            "/api/admin/secrets/{name}",
            axum::routing::put(secrets::put).delete(secrets::remove),
        )
        .route("/api/admin/tax-rules", get(tax::declared_rules))
        .route(
            "/api/admin/alerts/rules",
            get(alerts::list_rules).post(alerts::put_rule),
        )
        .route(
            "/api/admin/alerts/rules/{slug}",
            axum::routing::put(alerts::toggle_rule),
        )
        .route("/api/admin/alerts/sweep", post(alerts::run_sweep))
        .route("/api/admin/telegram", get(telegram::status))
        .route("/api/admin/telegram/flush", post(telegram::flush_handler))
        .route("/api/admin/llm", get(llm::status).patch(llm::set_enabled))
        .route("/api/admin/provider", get(hub::status))
        .route("/api/admin/provider/check", post(hub::check))
        .route("/api/admin/classify", post(classify::classify_one))
        .route("/api/admin/classify/reclassify", post(classify::reclassify))
        .route(
            "/api/admin/tax-rules/{code}",
            axum::routing::put(tax::set_rule_enabled),
        )
        .route("/api/admin/legal-sources", get(legal::list_handler))
        .route(
            "/api/admin/legal-sources/{code}",
            axum::routing::put(legal::toggle_handler),
        )
        .route(
            "/api/admin/connector",
            get(connector::admin_status).patch(connector::admin_update),
        )
        // What agents wrote through the connector: the audit trail, so the owner can
        // tell agent work from their own at a glance.
        .route("/api/admin/connector/writes", get(agent_writes))
        .route("/api/admin/connector/analyses", get(agent_analyses))
        .route("/api/admin/connectors", get(connectors::status_handler))
        .route(
            "/api/admin/connectors/{id}",
            axum::routing::put(connectors::toggle_handler),
        )
        .route(
            "/api/admin/connectors/{id}/run",
            post(connectors::run_handler),
        )
        .route("/api/admin/research", post(reader::run_research))
        .route("/api/admin/regions", get(regions::list))
        .route(
            "/api/admin/regions/reload",
            axum::routing::post(regions::reload_handler),
        )
        .route(
            "/api/admin/regions/{code}",
            axum::routing::put(regions::set_enabled),
        )
        .route("/api/alerts", get(alerts::list_inbox))
        .route("/api/alerts/read-all", post(alerts::read_all))
        .route("/api/alerts/{id}/read", post(alerts::read_one))
        .route("/api/notes", post(reader::create_owner_note))
        .route(
            "/api/notes/{id}",
            axum::routing::delete(reader::delete_owner_note),
        )
        .route("/api/articles", get(collector::articles))
        .route(
            "/api/articles/{id}/extract",
            post(collector::extract_article_handler),
        )
        .route(
            "/api/admin/articles/backfill",
            post(collector::backfill_handler),
        )
        .route(
            "/api/app-agents/bridge-config",
            get(agent_bridge::get_bridge_config).put(agent_bridge::put_bridge_config),
        )
        .route(
            "/api/app-agents/bridge-config/probe",
            post(agent_bridge::post_bridge_probe),
        )
        .route("/api/app-agents/models", get(agent_bridge::get_models))
        .route(
            "/api/app-agents/workspaces",
            get(agent_bridge::get_workspaces).post(agent_bridge::post_workspace),
        )
        .route(
            "/api/app-agents/workspaces/{id}",
            axum::routing::put(agent_bridge::put_workspace)
                .delete(agent_bridge::delete_workspace),
        )
        .route("/api/app-agents/skills", get(agent_bridge::get_skills))
        .route(
            "/api/app-agents/skills/sync",
            post(agent_bridge::post_skills_sync),
        )
        .route(
            "/api/app-agents/skills/access-requests",
            get(agent_bridge::get_skill_access_requests)
                .post(agent_bridge::post_skill_access_request),
        )
        .route(
            "/api/app-agents/skills/{slug}",
            get(agent_bridge::get_skill).patch(agent_bridge::patch_skill),
        )
        .route("/api/app-agents/agents", get(agent_bridge::get_agents))
        .route(
            "/api/app-agents/agents/{slug}",
            axum::routing::put(agent_bridge::put_agent).delete(agent_bridge::delete_agent),
        )
        .route(
            "/api/app-agents/threads",
            get(agent_bridge::get_threads).post(agent_bridge::post_thread),
        )
        .route(
            "/api/app-agents/threads/{id}",
            axum::routing::patch(agent_bridge::patch_thread).delete(agent_bridge::delete_thread),
        )
        .route(
            "/api/app-agents/threads/{id}/messages",
            get(agent_bridge::get_thread_messages),
        )
        .route(
            "/api/app-agents/threads/{id}/turns",
            post(agent_bridge::post_thread_turn),
        )
        .route(
            "/api/app-agents/threads/{id}/stream",
            get(agent_bridge::get_thread_stream),
        )
        .route(
            "/api/app-agents/threads/{id}/whiteboard",
            get(agent_bridge::get_whiteboard)
                .post(agent_bridge::post_whiteboard)
                .delete(agent_bridge::delete_whiteboard_query),
        )
        .route(
            "/api/app-agents/threads/{id}/whiteboard/{code}",
            axum::routing::delete(agent_bridge::delete_whiteboard_path),
        )
        .route(
            "/api/app-agents/threads/{id}/round-summary",
            post(agent_bridge::post_round_summary),
        )
        .route(
            "/api/app-agents/schedules",
            get(agent_bridge::get_schedules).post(agent_bridge::post_schedule),
        )
        .route(
            "/api/app-agents/schedules/{id}",
            axum::routing::patch(agent_bridge::patch_schedule)
                .delete(agent_bridge::delete_schedule),
        )
        .route(
            "/api/app-agents/schedules/{id}/run-now",
            post(agent_bridge::post_schedule_run_now),
        )
        .route(
            "/api/app-agents/telegram/config",
            get(agent_bridge::get_telegram_config).put(agent_bridge::put_telegram_config),
        )
        .route(
            "/api/app-agents/telegram/routes",
            get(agent_bridge::get_telegram_routes).post(agent_bridge::post_telegram_route),
        )
        .route(
            "/api/app-agents/telegram/routes/{id}",
            axum::routing::delete(agent_bridge::delete_telegram_route),
        )
        .route("/api/app-agents/manifest", get(agent_v2::get_manifest))
        .route(
            "/api/app-agents/models/catalog",
            get(agent_v2::get_models_catalog).patch(agent_v2::patch_models_catalog),
        )
        .route("/api/app-agents/models/fetch", post(agent_v2::post_models_fetch))
        .route("/api/app-agents/models/upsert", post(agent_v2::post_models_upsert))
        .route(
            "/api/app-agents/models/bulk-upsert",
            post(agent_v2::post_models_bulk_upsert),
        )
        .route(
            "/api/app-agents/models/roles",
            axum::routing::put(agent_v2::put_models_roles),
        )
        .route(
            "/api/app-agents/custom-skills",
            get(agent_v2::get_custom_skills).post(agent_v2::post_custom_skill),
        )
        .route(
            "/api/app-agents/custom-skills/{slug}",
            axum::routing::delete(agent_v2::delete_custom_skill),
        )
        .route(
            "/api/app-agents/mcp-namespaces",
            get(agent_v2::get_mcp_namespaces).put(agent_v2::put_mcp_namespaces),
        )
        .route(
            "/api/app-agents/custom-providers",
            get(agent_v2::get_custom_providers).post(agent_v2::post_custom_provider),
        )
        .route(
            "/api/app-agents/custom-providers/{id}",
            axum::routing::delete(agent_v2::delete_custom_provider),
        )
        .route(
            "/api/app-agents/agents/{slug}/pull-hub",
            post(agent_v2::post_agent_pull_hub),
        )
        .route(
            "/api/app-agents/agents/{slug}/push-draft",
            post(agent_v2::post_agent_push_draft),
        )
        .route(
            "/api/app-agents/threads/{id}/branch",
            post(agent_v2::post_thread_branch),
        )
        .route(
            "/api/app-agents/threads/{id}/seats/{slug}/retry",
            post(agent_v2::post_seat_retry),
        )
        .route(
            "/api/app-agents/threads/{id}/ask-user",
            get(agent_v2::get_thread_ask_user),
        )
        .route(
            "/api/app-agents/threads/{id}/ask-user/{prompt_id}/answer",
            post(agent_v2::post_answer_ask_user),
        )
        .route(
            "/api/app-agents/threads/{id}/activity",
            get(agent_v2::get_thread_activity),
        )
        .route(
            "/api/app-agents/composer/ghost-hint",
            post(agent_v2::post_composer_ghost_hint),
        )
        .route(
            "/api/app-agents/cowriter/suggest-diff",
            post(agent_v2::post_cowriter_suggest_diff),
        )
        .route(
            "/api/app-agents/cowriter/drafts/{id}/decide",
            post(agent_v2::post_cowriter_decide),
        )
        .route(
            "/api/app-agents/memory/graph",
            get(agent_v2::get_memory_graph),
        )
        .route(
            "/api/app-agents/memory/nodes",
            post(agent_v2::post_memory_node),
        )
        .route(
            "/api/app-agents/memory/nodes/{id}",
            axum::routing::delete(agent_v2::delete_memory_node),
        )
        .route(
            "/api/app-agents/memory/update",
            post(agent_v2::post_memory_update),
        )
        .route(
            "/api/app-agents/memory/audit",
            post(agent_v2::post_memory_audit),
        )
        .route(
            "/api/app-agents/memory/dedup",
            post(agent_v2::post_memory_dedup),
        )
        .route(
            "/api/app-agents/telegram/link-code",
            post(agent_v2::post_telegram_link_code),
        )
        .route("/api/auth/revoke-all", post(admin_lifecycle::revoke_all_sessions))
        .route(
            "/api/admin/api-keys",
            get(admin_lifecycle::get_api_keys).post(admin_lifecycle::post_api_key),
        )
        .route(
            "/api/admin/api-keys/{id}",
            axum::routing::patch(admin_lifecycle::patch_api_key)
                .delete(admin_lifecycle::delete_api_key),
        )
        .route(
            "/api/admin/api-keys/{id}/copy",
            post(admin_lifecycle::copy_api_key),
        )
        .route(
            "/api/admin/api-keys/{id}/rotate",
            post(admin_lifecycle::rotate_api_key),
        )
        .route(
            "/api/admin/api-keys/{id}/revoke",
            post(admin_lifecycle::revoke_api_key),
        )
        .route(
            "/api/keys",
            get(admin_lifecycle::get_api_keys).post(admin_lifecycle::post_api_key),
        )
        .route(
            "/api/keys/{id}",
            axum::routing::patch(admin_lifecycle::patch_api_key)
                .delete(admin_lifecycle::delete_api_key),
        )
        .route("/api/keys/{id}/copy", post(admin_lifecycle::copy_api_key))
        .route("/api/keys/{id}/rotate", post(admin_lifecycle::rotate_api_key))
        .route("/api/keys/{id}/revoke", post(admin_lifecycle::revoke_api_key))
        .route("/api/admin/mgmt/rotate", post(admin_lifecycle::rotate_management_key))
        .route(
            "/api/admin/sessions/revoke-all",
            post(admin_lifecycle::revoke_all_sessions),
        )
        .route(
            "/api/admin/audit-events",
            get(admin_lifecycle::get_audit_events),
        )
        .route("/api/admin/audit", get(admin_lifecycle::get_audit_events))
        .route(
            "/api/admin/oidc",
            get(oidc::admin_get_oidc).put(oidc::admin_put_oidc),
        )
        .route("/api/admin/oidc/test", post(oidc::admin_test_oidc))
        .route("/api/admin/oidc/reset", post(oidc::admin_reset_oidc))
        .route(
            "/api/oidc",
            get(oidc::admin_get_oidc).put(oidc::admin_put_oidc),
        )
        .route("/api/oidc/test", post(oidc::admin_test_oidc))
        .route("/api/oidc/reset", post(oidc::admin_reset_oidc))
        .route(
            "/api/admin/providers",
            get(admin_lifecycle::get_providers).post(admin_lifecycle::post_provider),
        )
        .route(
            "/api/admin/providers/{id}",
            axum::routing::put(admin_lifecycle::put_provider)
                .delete(admin_lifecycle::delete_provider),
        )
        .route(
            "/api/admin/providers/{id}/copy",
            post(admin_lifecycle::copy_provider_secret),
        )
        .route(
            "/api/admin/providers/{id}/rotate",
            post(admin_lifecycle::rotate_provider_secret),
        )
        .route(
            "/api/admin/providers/{id}/revoke",
            post(admin_lifecycle::revoke_provider),
        )
        .route(
            "/api/admin/providers/{id}/fetch-models",
            post(admin_lifecycle::fetch_provider_models),
        )
        .route(
            "/api/admin/providers/{id}/test",
            post(admin_lifecycle::fetch_provider_models),
        )
        .route(
            "/api/admin/models/catalog",
            get(agent_v2::get_models_catalog).patch(admin_lifecycle::patch_models_catalog),
        )
        .route(
            "/api/admin/models/bindings",
            axum::routing::put(admin_lifecycle::put_models_bindings),
        )
        .route(
            "/api/admin/mcp-connectors",
            get(admin_lifecycle::get_mcp_connectors).post(admin_lifecycle::post_mcp_connector),
        )
        .route(
            "/api/admin/mcp-connectors/{id}",
            axum::routing::put(admin_lifecycle::put_mcp_connector)
                .delete(admin_lifecycle::delete_mcp_connector),
        )
        .route(
            "/api/admin/mcp-connectors/{id}/copy",
            post(admin_lifecycle::copy_mcp_connector),
        )
        .route(
            "/api/admin/mcp-connectors/{id}/rotate",
            post(admin_lifecycle::rotate_mcp_connector),
        )
        .route(
            "/api/admin/mcp-connectors/{id}/revoke",
            post(admin_lifecycle::revoke_mcp_connector),
        )
        .route(
            "/api/admin/mcp-connectors/{id}/test",
            post(admin_lifecycle::test_mcp_connector),
        )
        .layer(middleware::from_fn_with_state(
            state.clone(),
            require_session,
        ));
    let read_api = Router::new()
        .route("/api/status", get(api_status))
        .route("/api/v1/topics", get(topics))
        .route("/api/v1/regions", get(regions::list_enabled))
        .route("/api/v1/series", get(series::list_series))
        .route(
            "/api/v1/series/{code}/observations",
            get(series::list_observations),
        )
        .route("/api/v1/events", get(series::list_events))
        .route("/api/v1/documents", get(documents::list_documents))
        .route("/api/v1/companies", get(filings::list_companies))
        .route("/api/v1/companies/{id}/filings", get(filings::list_filings))
        .route("/api/v1/companies/{id}/facts", get(filings::list_facts))
        .route("/api/v1/tax-calendar", get(tax::list_calendar))
        .route("/api/v1/articles", get(collector::articles))
        .route(
            "/api/v1/articles/{id}",
            get(collector::article_detail_handler),
        )
        .route("/api/v1/analyses", get(reader::list_analyses))
        .route("/api/v1/analyses/{id}", get(reader::get_analysis))
        .route("/api/v1/notes", get(reader::list_notes))
        .layer(middleware::from_fn_with_state(
            state.clone(),
            require_session_or_api,
        ));
    // P8: read-only MCP over the same data. Its own credential gate, because it also
    // accepts a signed delegation from the Business MCP Hub; reader session/key still
    // work exactly as they do on `/api/v1/*`.
    let mcp_api = Router::new()
        .route("/mcp", post(mcp::handle).get(mcp::get_info))
        .layer(middleware::from_fn_with_state(state.clone(), connector::require_mcp));
    Router::new()
        .route("/health", get(health))
        .route(
            "/v1/app-connectors/lnc-newsman/health",
            get(connector::app_connector_health),
        )
        .route(
            "/v1/app-connectors/lnc-newsman/healthz",
            get(connector::app_connector_health),
        )
        .route(
            "/v1/app-connectors/lnc-newsman/mcp/health",
            get(connector::app_connector_health),
        )
        .route(
            "/v1/app-connectors/lnc-newsman/mcp",
            post(connector::app_connector_mcp_post).get(connector::app_connector_mcp_get),
        )
        .route("/api/auth/status", get(auth_status))
        .route("/api/auth/login", post(login))
        .route(
            "/api/auth/logout",
            post(logout).get(logout_method_not_allowed),
        )
        .route("/api/auth/oidc/login", get(oidc::oidc_login))
        .route("/api/auth/oidc/callback", get(oidc::oidc_callback))
        .route("/api/app-info", get(app_info))
        .route(
            "/api/app-agents/telegram/webhook/direct",
            post(agent_bridge::webhook_direct),
        )
        .route(
            "/api/app-agents/telegram/webhook/cygnus",
            post(agent_bridge::webhook_cygnus),
        )
        .route("/login.html", get(shell))
        .route("/", get(root))
        .route("/admin", get(admin))
        .merge(session_only)
        .merge(read_api)
        .merge(mcp_api)
        .nest_service("/assets", ServeDir::new("ui/dist/assets"))
        .fallback_service(ServeDir::new("ui/dist"))
        .layer(middleware::from_fn_with_state(state.clone(), csrf_guard))
        .layer(middleware::from_fn_with_state(state.clone(), host_guard))
        .layer(middleware::from_fn(security_headers))
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

pub(crate) fn failure(status: StatusCode, code: &'static str) -> Response {
    (
        status,
        Json(serde_json::json!({"ok": false, "error": code})),
    )
        .into_response()
}
pub(crate) fn internal_error(err: impl std::fmt::Display) -> Response {
    tracing::error!(%err, "request failed");
    failure(StatusCode::INTERNAL_SERVER_ERROR, "internal_error")
}

async fn security_headers(req: Request, next: Next) -> Response {
    let path = req.uri().path().to_string();
    let api = path.starts_with("/api/") || path.starts_with("/v1/") || path == "/mcp";
    let html_shell = !api && !path.starts_with("/assets/");
    let mut response = next.run(req).await;
    let headers = response.headers_mut();
    headers.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    headers.insert(
        header::X_FRAME_OPTIONS,
        HeaderValue::from_static("DENY"),
    );
    headers.insert(
        header::HeaderName::from_static("referrer-policy"),
        HeaderValue::from_static("same-origin"),
    );
    headers.insert(
        header::CONTENT_SECURITY_POLICY,
        HeaderValue::from_static("default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data:; object-src 'none'; frame-ancestors 'none'; base-uri 'none'"),
    );
    if api || html_shell {
        headers.insert(
            header::CACHE_CONTROL,
            HeaderValue::from_static("no-store, private"),
        );
    }
    response
}

fn has_query_token_param(req: &Request) -> bool {
    let Some(query) = req.uri().query() else {
        return false;
    };
    query.split('&').any(|pair| {
        let key = pair.split('=').next().unwrap_or("").trim().to_ascii_lowercase();
        matches!(
            key.as_str(),
            "token"
                | "key"
                | "api_key"
                | "apikey"
                | "access_token"
                | "secret"
                | "password"
                | "passwd"
                | "client_secret"
                | "management_key"
                | "bearer"
                | "auth_token"
        )
    })
}

async fn csrf_guard(State(state): State<SharedState>, req: Request, next: Next) -> Response {
    if has_query_token_param(&req) {
        return failure(StatusCode::BAD_REQUEST, "query_token_disabled");
    }
    let path = req.uri().path();
    if connector::presents_connector_bearer(req.headers())
        || ((path == "/mcp" || path.starts_with("/v1/app-connectors/"))
            && auth::cookie_value(req.headers()).is_none())
        || matches!(
            path,
            "/api/app-agents/telegram/webhook/direct" | "/api/app-agents/telegram/webhook/cygnus"
        )
    {
        return next.run(req).await;
    }
    if let Err(res) = security::enforce_csrf(&state, &req) {
        return res;
    }
    next.run(req).await
}

async fn host_guard(State(state): State<SharedState>, req: Request, next: Next) -> Response {
    if state.cfg.allowed_hosts.is_empty() || state.cfg.allowed_hosts.iter().any(|v| v == "*") {
        return next.run(req).await;
    }
    if req.uri().path().starts_with("/v1/app-connectors/lnc-newsman/") {
        return next.run(req).await;
    }
    let host = req
        .headers()
        .get(header::HOST)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse::<Authority>().ok())
        .map(|a| a.host().trim_matches(['[', ']']).to_ascii_lowercase());
    let Some(host) = host else {
        return failure(StatusCode::BAD_REQUEST, "host_not_allowed");
    };
    let hostname = std::env::var("HOSTNAME")
        .unwrap_or_default()
        .to_ascii_lowercase();
    if ["localhost", "127.0.0.1", "::1", "lnc-newsman", "tasks.lnc-newsman"]
        .contains(&host.as_str())
        || (!hostname.is_empty() && host == hostname)
        || state
            .cfg
            .allowed_hosts
            .iter()
            .any(|allowed| allowed.eq_ignore_ascii_case(&host))
    {
        next.run(req).await
    } else {
        failure(StatusCode::BAD_REQUEST, "host_not_allowed")
    }
}

pub(crate) fn session_valid(state: &SharedState, headers: &HeaderMap) -> bool {
    security::validate_session_identity(state, headers).is_some()
}

async fn require_session(State(state): State<SharedState>, req: Request, next: Next) -> Response {
    if session_valid(&state, req.headers()) {
        next.run(req).await
    } else {
        failure(StatusCode::UNAUTHORIZED, "unauthorized")
    }
}
async fn require_session_or_api(
    State(state): State<SharedState>,
    req: Request,
    next: Next,
) -> Response {
    if session_valid(&state, req.headers())
        || security::validate_api_key_request(&state, req.headers())
    {
        next.run(req).await
    } else {
        failure(StatusCode::UNAUTHORIZED, "unauthorized")
    }
}
async fn root(State(state): State<SharedState>, headers: HeaderMap) -> Response {
    if session_valid(&state, &headers) {
        Redirect::to("/admin").into_response()
    } else {
        Redirect::to("/login.html").into_response()
    }
}
async fn admin(State(state): State<SharedState>, headers: HeaderMap) -> Response {
    if session_valid(&state, &headers) {
        shell().await.into_response()
    } else {
        Redirect::to("/login.html").into_response()
    }
}
async fn shell() -> Response {
    match tokio::fs::read_to_string("ui/dist/index.html").await {
        Ok(html) => Html(html).into_response(),
        Err(_) => failure(StatusCode::SERVICE_UNAVAILABLE, "ui_build_missing"),
    }
}
async fn health(State(state): State<SharedState>) -> Json<serde_json::Value> {
    Json(
        serde_json::json!({"ok":true,"service":"lnc-newsman","version":state.cfg.version,"uptime_seconds":now().saturating_sub(state.started_at)}),
    )
}
async fn auth_status(
    State(state): State<SharedState>,
    headers: HeaderMap,
) -> Response {
    let identity = security::validate_session_identity(&state, &headers);
    let authenticated = identity.is_some();
    let management_configured = {
        let db = state.db.lock().expect("database lock poisoned");
        auth::management_key_configured(&db, state.cfg.management_key.as_deref())
    };
    let mut payload = serde_json::json!({
        "ok": true,
        "service": "lnc-newsman",
        "version": state.cfg.version,
        "authenticated": authenticated,
        "management_configured": management_configured,
        "oidc_enabled": state.security.oidc_cfg().is_active(),
        "auth_methods": {
            "oidc": state.security.oidc_cfg().is_active(),
            "local_key": management_configured,
        },
    });
    if let Some((auth_method, username)) = identity
        && let Some(obj) = payload.as_object_mut()
    {
        let rt = state
            .db
            .lock()
            .expect("database lock poisoned")
            .runtime_auth_state()
            .unwrap_or_default();
        obj.insert("auth_method".into(), serde_json::json!(auth_method));
        obj.insert("username".into(), serde_json::json!(username));
        obj.insert(
            "management_source".into(),
            serde_json::json!(if !rt.mgmt_key_hash.is_empty() {
                rt.mgmt_source
            } else if state.cfg.management_key.is_some() {
                "env".into()
            } else {
                "unset".into()
            }),
        );
    }
    let mut res = Json(payload).into_response();
    res.headers_mut().insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("no-store, private"),
    );
    res
}
async fn app_info(State(state): State<SharedState>) -> Json<serde_json::Value> {
    Json(
        serde_json::json!({"name":"lnc-newsman","display_name":"LNC Newsman","version":state.cfg.version,"mode":"reader"}),
    )
}

async fn login(
    State(state): State<SharedState>,
    peer: Option<axum::Extension<ConnectInfo<SocketAddr>>>,
    headers: HeaderMap,
    Json(input): Json<LoginRequest>,
) -> Response {
    let client = security::request_client_ip(&state, peer.as_ref(), &headers);
    if !state
        .login_limiter
        .lock()
        .expect("limiter lock poisoned")
        .allow_and_record(&client, now())
    {
        return failure(StatusCode::TOO_MANY_REQUESTS, "login_rate_limited");
    }
    let configured = {
        let db = state.db.lock().expect("database lock poisoned");
        auth::management_key_configured(&db, state.cfg.management_key.as_deref())
    };
    if !configured {
        return failure(
            StatusCode::SERVICE_UNAVAILABLE,
            "management_key_not_configured",
        );
    }
    let supplied = input
        .key
        .or(input.token)
        .or(input.password)
        .unwrap_or_default();
    let valid = {
        let db = state.db.lock().expect("database lock poisoned");
        auth::verify_management_key(&db, state.cfg.management_key.as_deref(), &supplied)
    };
    if !valid {
        return failure(StatusCode::UNAUTHORIZED, "invalid_management_key");
    }
    let cookie = match security::issue_session_cookie(
        &state,
        &headers,
        "management_key",
        "admin",
        &client,
    ) {
        Ok(c) => c,
        Err(err) => return internal_error(err),
    };
    if !state.alerts_running.swap(true, Ordering::SeqCst) {
        alerts::spawn_sweeper(state.clone());
    }
    state
        .login_limiter
        .lock()
        .expect("limiter lock poisoned")
        .clear(&client);
    let mut response = Json(serde_json::json!({
        "ok": true,
        "authenticated": true,
        "auth_method": "management_key",
        "username": "admin"
    }))
    .into_response();
    response.headers_mut().insert(header::SET_COOKIE, cookie);
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("no-store, private"),
    );
    response
}
async fn logout_method_not_allowed() -> Response {
    let mut res = failure(StatusCode::METHOD_NOT_ALLOWED, "method_not_allowed");
    res.headers_mut()
        .insert(header::ALLOW, HeaderValue::from_static("POST"));
    res
}

async fn logout(State(state): State<SharedState>, headers: HeaderMap) -> Response {
    if let Some(token) = auth::cookie_value(&headers)
        && let Err(err) = state
            .db
            .lock()
            .expect("database lock poisoned")
            .revoke_session(&auth::token_hash(token))
    {
        return internal_error(err);
    }
    let mut response = Json(serde_json::json!({"ok":true,"local_only":true})).into_response();
    response.headers_mut().insert(
        header::SET_COOKIE,
        security::clear_cookie_header(
            auth::COOKIE_NAME,
            "/",
            state.security.is_cookie_secure(&headers),
        ),
    );
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("no-store, private"),
    );
    response
}

async fn api_status(State(state): State<SharedState>) -> Json<serde_json::Value> {
    Json(
        serde_json::json!({"ok":true,"service":"lnc-newsman","version":state.cfg.version,"api":"v1","mcp_available":true,"phase":"P1-collector"}),
    )
}
fn settings_payload(state: &SharedState) -> anyhow::Result<SettingsResponse> {
    let db = state.db.lock().expect("database lock poisoned");
    let resolved = telegram::endpoint(&db, state.secrets.as_ref(), &state.telegram);
    let prefs = reader::brief_preferences(&db)?;
    let has = |name: &str| match state.secrets.as_ref() {
        Some(vault) => secrets::get(&db, vault, name).is_ok(),
        None => false,
    };
    Ok(SettingsResponse {
        version: state.cfg.version,
        language: db.get_setting("language")?.unwrap_or_else(|| "vi".into()),
        default_reader_mode: db
            .get_setting("default_reader_mode")?
            .unwrap_or_else(|| "paper".into()),
        collector_enabled: collector::collector_enabled(&db),
        fulltext_auto_enabled: collector::fulltext_auto_enabled(&db),
        connectors_auto_enabled: connectors::connectors_auto_enabled(&db),
        focus_profile: prefs.focus_profile,
        brief_morning_time: prefs.brief_morning_time,
        brief_evening_time: prefs.brief_evening_time,
        brief_timezone: prefs.brief_timezone,
        telegram_mode: telegram::effective_mode(&db, &state.telegram).as_str(),
        telegram_mode_stored: telegram::stored_mode(&db).as_str(),
        telegram_env_override: state.telegram.mode.is_some(),
        telegram_ready: resolved.is_ok(),
        telegram_reason: resolved.err(),
        telegram_chat_id: db
            .get_setting(telegram::CHAT_ID_KEY)?
            .and_then(|value| value.trim().parse::<i64>().ok()),
        telegram_alerts_topic_id: db
            .get_setting(telegram::TOPIC_ID_KEY)?
            .and_then(|value| value.trim().parse::<i64>().ok()),
        telegram_has_relay_token: has(telegram::SECRET_RELAY),
        telegram_has_bot_token: has(telegram::SECRET_BOT),
    })
}
/// Recent agent writes. Metadata only: client id, tool and record, never the note text,
/// because the owner reads the record itself on the article it belongs to.
async fn agent_writes(State(state): State<SharedState>) -> Response {
    let limit = 100;
    let db = state.db.lock().expect("database lock poisoned");
    match write::write_log(&db, limit) {
        Ok(rows) => Json(serde_json::json!({"data": rows})).into_response(),
        Err(err) => internal_error(err),
    }
}
/// Stored briefs and summaries with their author label, for the admin card.
async fn agent_analyses(State(state): State<SharedState>) -> Response {
    let db = state.db.lock().expect("database lock poisoned");
    match write::analysis_list(&db, None, None, None, 50) {
        Ok(rows) => Json(serde_json::json!({"data": rows})).into_response(),
        Err(err) => internal_error(err),
    }
}
async fn get_settings(State(state): State<SharedState>) -> Response {
    match settings_payload(&state) {
        Ok(value) => Json(value).into_response(),
        Err(err) => internal_error(err),
    }
}
async fn patch_settings(
    State(state): State<SharedState>,
    Json(patch): Json<SettingsPatch>,
) -> Response {
    if patch
        .language
        .as_ref()
        .is_some_and(|v| !["vi", "en"].contains(&v.as_str()))
        || patch
            .default_reader_mode
            .as_ref()
            .is_some_and(|v| !crate::themes::READER_MODES.contains(&v.as_str()))
    {
        return failure(StatusCode::BAD_REQUEST, "invalid_settings");
    }
    let mut writes = Vec::new();
    if let Some(ref value) = patch.language {
        writes.push(("language", value.as_str()));
    }
    if let Some(ref value) = patch.default_reader_mode {
        writes.push(("default_reader_mode", value.as_str()));
    }
    if let Some(enabled) = patch.collector_enabled {
        writes.push(("collector_enabled", if enabled { "true" } else { "false" }));
    }
    if let Some(enabled) = patch.fulltext_auto_enabled {
        writes.push((
            "fulltext_auto_enabled",
            if enabled { "true" } else { "false" },
        ));
    }
    if let Some(enabled) = patch.connectors_auto_enabled {
        writes.push((
            "connectors_auto_enabled",
            if enabled { "true" } else { "false" },
        ));
    }
    let pref_writes = match reader::validate_preferences_patch(
        patch.focus_profile.as_deref(),
        patch.brief_morning_time.as_deref(),
        patch.brief_evening_time.as_deref(),
        patch.brief_timezone.as_deref(),
    ) {
        Ok(writes) => writes,
        Err(code) => return failure(StatusCode::BAD_REQUEST, code),
    };
    // Telegram fields are validated together, so a bad chat id cannot leave the mode
    // changed with no destination. Nothing is written unless the whole patch is valid.
    let telegram_writes = match telegram::settings_writes(&patch.telegram()) {
        Ok(writes) => writes,
        Err(code) => return failure(StatusCode::BAD_REQUEST, code),
    };
    let mut owned: Vec<(&str, String)> = writes
        .iter()
        .map(|(key, value)| (*key, (*value).to_string()))
        .collect();
    owned.extend(pref_writes);
    owned.extend(telegram_writes);
    let owned_refs: Vec<(&str, &str)> = owned
        .iter()
        .map(|(key, value)| (*key, value.as_str()))
        .collect();
    let result = state
        .db
        .lock()
        .expect("database lock poisoned")
        .set_settings(&owned_refs);
    if let Err(err) = result {
        return internal_error(err);
    }
    match settings_payload(&state) {
        Ok(value) => Json(value).into_response(),
        Err(err) => internal_error(err),
    }
}
async fn dashboard(
    State(state): State<SharedState>,
    Query(query): Query<DashboardQuery>,
) -> Response {
    let region = query
        .region
        .as_deref()
        .map(|code| code.trim().to_ascii_lowercase())
        .filter(|code| !code.is_empty());
    let db = state.db.lock().expect("database lock poisoned");
    // Counters follow the header region picker, exactly like the article list.
    if let Some(code) = region.as_deref()
        && !matches!(regions::region_enabled(&db, code), Ok(true))
    {
        return failure(StatusCode::BAD_REQUEST, "invalid_region");
    }
    let result = db.topics().and_then(|topics| {
        let sources = collector::source_facets(&db, region.as_deref())?;
        Ok(Dashboard {
            topics,
            sources,
            stats: collector::stats(&db, region.as_deref())?,
        })
    });
    match result {
        Ok(value) => Json(value).into_response(),
        Err(err) => internal_error(err),
    }
}
async fn topics(State(state): State<SharedState>) -> Response {
    match state.db.lock().expect("database lock poisoned").topics() {
        Ok(topics) => Json(serde_json::json!({"data":topics})).into_response(),
        Err(err) => internal_error(err),
    }
}
fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or(Duration::ZERO)
        .as_secs()
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        body::Body,
        http::{Request, header},
    };
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    fn state() -> SharedState {
        let db = db::Database::in_memory().unwrap();
        let agent_bridge = agent_bridge::AgentBridgeRuntime::new(&db);
        let cfg = config::Config {
            data_dir: ".".into(),
            regions_dir: ".".into(),
            bind_address: "127.0.0.1".parse().unwrap(),
            port: 8000,
            management_key: Some("test-management-key".into()),
            api_keys: vec!["test-api-key".into()],
            allowed_hosts: vec!["newsman.test".into()],
            trusted_proxies: Vec::new(),
            secure_cookie: true,
            version: "test",
        };
        let security = Arc::new(security::SecurityRuntime::for_test(&cfg));
        Arc::new(AppState {
            cfg,
            db: Mutex::new(db),
            login_limiter: Mutex::new(auth::LoginLimiter::default()),
            started_at: now(),
            fetcher: fetch::Fetcher::public_only(),
            in_flight: Mutex::new(HashSet::new()),
            connector_in_flight: Mutex::new(HashSet::new()),
            host_gates: Mutex::new(std::collections::HashMap::new()),
            host_gap: collector::HOST_GAP,
            secrets: None,
            connector_config: connectors::ConnectorConfig::default(),
            llm: llm::LlmConfig {
                base_url: None,
                model: "mock".into(),
            },
            hub: hub::ProviderConfig::default(),
            telegram: telegram::TelegramConfig::default(),
            connector: connector::ConnectorConfig::default(),
            connector_nonces: Mutex::new(std::collections::HashMap::new()),
            llm_enabled: false,
            pack: Mutex::new(regions::PackStatus::default()),
            alerts_running: AtomicBool::new(false),
            agent_bridge,
            security,
        })
    }
    fn req(
        method: &str,
        path: &str,
        body: &str,
        cookie: Option<&str>,
        api_key: Option<&str>,
        csrf: bool,
    ) -> Request<Body> {
        let mut builder = Request::builder()
            .method(method)
            .uri(path)
            .header(header::HOST, "newsman.test")
            .header(header::CONTENT_TYPE, "application/json");
        if let Some(v) = cookie {
            builder = builder.header(header::COOKIE, v);
        }
        if let Some(v) = api_key {
            builder = builder.header(header::AUTHORIZATION, format!("Bearer {v}"));
        }
        if csrf {
            builder = builder.header("x-requested-with", "XMLHttpRequest");
        }
        builder.body(Body::from(body.to_owned())).unwrap()
    }
    /// A request carrying a socket peer (what a real server always has) and an
    /// optional X-Forwarded-For, for the login-throttle tests.
    fn req_from(peer: &str, forwarded_for: Option<&str>, body: &str) -> Request<Body> {
        let mut builder = Request::builder()
            .method("POST")
            .uri("/api/auth/login")
            .header(header::HOST, "newsman.test")
            .header(header::CONTENT_TYPE, "application/json")
            .header("x-requested-with", "XMLHttpRequest")
            .extension(ConnectInfo(peer.parse::<SocketAddr>().unwrap()));
        if let Some(v) = forwarded_for {
            builder = builder.header("x-forwarded-for", v);
        }
        builder.body(Body::from(body.to_owned())).unwrap()
    }
    async fn login_cookie(app: &Router) -> String {
        let response = app
            .clone()
            .oneshot(req(
                "POST",
                "/api/auth/login",
                r#"{"key":"test-management-key"}"#,
                None,
                None,
                true,
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let cookie = response
            .headers()
            .get(header::SET_COOKIE)
            .unwrap()
            .to_str()
            .unwrap();
        assert!(cookie.contains("Secure"));
        cookie.split(';').next().unwrap().to_string()
    }
    #[tokio::test]
    async fn api_key_cannot_change_settings_and_cors_is_disabled() {
        let app = router(state());
        let response = app
            .clone()
            .oneshot(req(
                "GET",
                "/api/v1/topics",
                "",
                None,
                Some("test-api-key"),
                false,
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert!(
            response
                .headers()
                .get(header::ACCESS_CONTROL_ALLOW_ORIGIN)
                .is_none()
        );
        let response = app
            .oneshot(req(
                "PATCH",
                "/api/settings",
                r#"{"language":"en"}"#,
                None,
                Some("test-api-key"),
                true,
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }
    #[tokio::test]
    async fn a_shared_proxy_peer_does_not_lock_everyone_out_behind_it() {
        // Two people behind the Caddy edge arrive from 10.20.0.3. Without a
        // trusted-proxy list the ninth bad attempt from one blocks the other.
        let shared = |trusted: bool| {
            let mut s = Arc::try_unwrap(state()).unwrap_or_else(|_| unreachable!());
            if trusted {
                s.cfg.trusted_proxies = vec!["10.20.0.3".into()];
            }
            router(Arc::new(s))
        };
        let bad = r#"{"key":"wrong"}"#;
        let good = r#"{"key":"test-management-key"}"#;
        for trusted in [false, true] {
            let app = shared(trusted);
            for _ in 0..8 {
                let r = app
                    .clone()
                    .oneshot(req_from("10.20.0.3:52000", Some("10.20.1.44"), bad))
                    .await
                    .unwrap();
                assert_eq!(r.status(), StatusCode::UNAUTHORIZED);
            }
            let other = app
                .clone()
                .oneshot(req_from("10.20.0.3:52000", Some("10.20.1.45"), good))
                .await
                .unwrap();
            // Untrusted: the header is ignored, both share the proxy's bucket.
            // Trusted: the second client has its own bucket and gets in.
            assert_eq!(
                other.status(),
                if trusted {
                    StatusCode::OK
                } else {
                    StatusCode::TOO_MANY_REQUESTS
                },
                "trusted={trusted}"
            );
        }
    }
    #[tokio::test]
    async fn logout_revokes_copied_cookie() {
        let app = router(state());
        let cookie = login_cookie(&app).await;
        assert_eq!(
            app.clone()
                .oneshot(req("GET", "/api/settings", "", Some(&cookie), None, false))
                .await
                .unwrap()
                .status(),
            StatusCode::OK
        );
        assert_eq!(
            app.clone()
                .oneshot(req(
                    "POST",
                    "/api/auth/logout",
                    "",
                    Some(&cookie),
                    None,
                    true
                ))
                .await
                .unwrap()
                .status(),
            StatusCode::OK
        );
        assert_eq!(
            app.oneshot(req("GET", "/api/settings", "", Some(&cookie), None, false))
                .await
                .unwrap()
                .status(),
            StatusCode::UNAUTHORIZED
        );
    }
    #[tokio::test]
    async fn csrf_and_invalid_settings_are_rejected_without_partial_update() {
        let app = router(state());
        assert_eq!(
            app.clone()
                .oneshot(req(
                    "POST",
                    "/api/auth/login",
                    r#"{"key":"test-management-key"}"#,
                    None,
                    None,
                    false
                ))
                .await
                .unwrap()
                .status(),
            StatusCode::FORBIDDEN
        );
        let cookie = login_cookie(&app).await;
        assert_eq!(
            app.clone()
                .oneshot(req(
                    "PATCH",
                    "/api/settings",
                    r#"{"language":"en","default_reader_mode":"invalid"}"#,
                    Some(&cookie),
                    None,
                    true
                ))
                .await
                .unwrap()
                .status(),
            StatusCode::BAD_REQUEST
        );
        let response = app
            .oneshot(req("GET", "/api/settings", "", Some(&cookie), None, false))
            .await
            .unwrap();
        let body = response.into_body().collect().await.unwrap().to_bytes();
        let value: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(value["language"], "vi");
    }
    #[tokio::test]
    async fn session_expiry_and_key_rotation_fail_closed() {
        let state = state();
        let token = auth::new_token(state.cfg.management_key.as_deref().unwrap()).unwrap();
        state
            .db
            .lock()
            .unwrap()
            .create_session(&auth::token_hash(&token), now(), 0)
            .unwrap();
        let mut headers = HeaderMap::new();
        headers.insert(
            header::COOKIE,
            HeaderValue::from_str(&format!("{}={token}", auth::COOKIE_NAME)).unwrap(),
        );
        assert!(!session_valid(&state, &headers));
        assert!(!auth::valid_signature(&token, "rotated-key"));
    }
    #[tokio::test]
    async fn host_allowlist_accepts_ipv6_loopback_and_rejects_other_hosts() {
        let app = router(state());
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/health")
                    .header(header::HOST, "[::1]:8000")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/health")
                    .header(header::HOST, "evil.test")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }
}
