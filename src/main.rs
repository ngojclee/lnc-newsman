mod alerts;
mod auth;
mod catalog;
#[cfg(test)]
mod catalog_tests;
mod classify;
mod collector;
#[cfg(test)]
mod collector_tests;
mod config;
mod connectors;
mod db;
#[cfg(test)]
mod docs_tests;
mod documents;
mod feed;
mod fetch;
mod filings;
mod hub;
mod llm;
mod mcp;
mod providers;
mod regions;
mod secrets;
mod series;
mod tax;
mod themes;

use axum::{
    Json, Router,
    extract::{ConnectInfo, Query, Request, State},
    http::{HeaderMap, HeaderValue, Method, StatusCode, header, uri::Authority},
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
    cfg: config::Config,
    db: Mutex<db::Database>,
    login_limiter: Mutex<auth::LoginLimiter>,
    started_at: u64,
    fetcher: fetch::Fetcher,
    in_flight: Mutex<HashSet<i64>>,
    /// Last request per host, shared by every path that fetches (scheduler,
    /// "Fetch now", feed preview) so the per-host gap cannot be bypassed.
    host_gates: Mutex<std::collections::HashMap<String, std::time::Instant>>,
    /// Production uses `collector::HOST_GAP`; tests shrink it to keep the suite fast.
    host_gap: std::time::Duration,
    /// `None` when `LNC_MASTER_KEY` is absent; the secret store then stays closed.
    secrets: Option<secrets::SecretBox>,
    /// Operator-supplied provider settings (SEC contact User-Agent and similar).
    connector_config: connectors::ConnectorConfig,
    /// LLM provider configuration (owner-set base URL/model; off by default).
    llm: llm::LlmConfig,
    /// Hub/OpenAI-compatible provider layer (P5/P6): base URL, allowlist, model aliases.
    hub: hub::ProviderConfig,
    /// Admin switch for the LLM layer. Off means "rules only", which always works.
    llm_enabled: bool,
    pack: Mutex<regions::PackStatus>,
    /// True once the alert sweeper has been started; set exactly once, by login.
    alerts_running: AtomicBool,
}
impl AppState {
    fn new(cfg: config::Config, db: db::Database, fetcher: fetch::Fetcher) -> Self {
        Self {
            cfg,
            db: Mutex::new(db),
            login_limiter: Mutex::new(auth::LoginLimiter::default()),
            started_at: now(),
            fetcher,
            in_flight: Mutex::new(HashSet::new()),
            host_gates: Mutex::new(std::collections::HashMap::new()),
            host_gap: collector::HOST_GAP,
            secrets: None,
            connector_config: connectors::ConnectorConfig::default(),
            llm: llm::LlmConfig {
                base_url: None,
                model: "mock".into(),
            },
            hub: hub::ProviderConfig::default(),
            llm_enabled: false,
            pack: Mutex::new(regions::PackStatus::default()),
            alerts_running: AtomicBool::new(false),
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
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SettingsPatch {
    language: Option<String>,
    default_reader_mode: Option<String>,
    collector_enabled: Option<bool>,
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
    collector::spawn_scheduler(state.clone());
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
        .route("/api/admin/llm", get(llm::status).patch(llm::set_enabled))
        .route("/api/admin/provider", get(hub::status))
        .route("/api/admin/provider/check", post(hub::check))
        .route("/api/admin/classify", post(classify::classify_one))
        .route("/api/admin/classify/reclassify", post(classify::reclassify))
        .route(
            "/api/admin/tax-rules/{code}",
            axum::routing::put(tax::set_rule_enabled),
        )
        .route("/api/admin/connectors", get(connectors::status_handler))
        .route(
            "/api/admin/connectors/{id}/run",
            post(connectors::run_handler),
        )
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
        .route("/api/articles", get(collector::articles))
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
        // P8: read-only MCP over the same data, same credentials as REST v1.
        .route("/mcp", post(mcp::handle).get(mcp::get_info))
        .layer(middleware::from_fn_with_state(
            state.clone(),
            require_session_or_api,
        ));
    Router::new()
        .route("/health", get(health))
        .route("/api/auth/status", get(auth_status))
        .route("/api/auth/login", post(login))
        .route("/api/auth/logout", post(logout))
        .route("/api/app-info", get(app_info))
        .route("/login.html", get(shell))
        .route("/", get(root))
        .route("/admin", get(admin))
        .merge(session_only)
        .merge(read_api)
        .nest_service("/assets", ServeDir::new("ui/dist/assets"))
        .fallback_service(ServeDir::new("ui/dist"))
        .layer(middleware::from_fn(csrf_guard))
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
    let path = req.uri().path();
    let api = path.starts_with("/api/");
    // Hashed bundles may be cached; HTML shells must revalidate so new builds load.
    let revalidate = !api && !path.starts_with("/assets/");
    let mut response = next.run(req).await;
    let headers = response.headers_mut();
    headers.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    headers.insert(header::CONTENT_SECURITY_POLICY, HeaderValue::from_static("default-src 'self'; img-src 'self' data:; object-src 'none'; frame-ancestors 'none'; base-uri 'none'"));
    if api {
        headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    } else if revalidate {
        headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-cache"));
    }
    response
}

async fn csrf_guard(req: Request, next: Next) -> Response {
    if !matches!(*req.method(), Method::GET | Method::HEAD | Method::OPTIONS)
        && req
            .headers()
            .get("x-requested-with")
            .and_then(|v| v.to_str().ok())
            != Some("XMLHttpRequest")
    {
        return failure(StatusCode::FORBIDDEN, "csrf_header_required");
    }
    next.run(req).await
}

async fn host_guard(State(state): State<SharedState>, req: Request, next: Next) -> Response {
    if state.cfg.allowed_hosts.is_empty() || state.cfg.allowed_hosts.iter().any(|v| v == "*") {
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
    if ["localhost", "127.0.0.1", "::1"].contains(&host.as_str())
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

fn session_valid(state: &SharedState, headers: &HeaderMap) -> bool {
    let (Some(key), Some(token)) = (
        state.cfg.management_key.as_deref(),
        auth::cookie_value(headers),
    ) else {
        return false;
    };
    if !auth::valid_signature(token, key) {
        return false;
    }
    match state
        .db
        .lock()
        .expect("database lock poisoned")
        .session_exists(&auth::token_hash(token), now())
    {
        Ok(valid) => valid,
        Err(err) => {
            tracing::error!(%err, "session lookup failed");
            false
        }
    }
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
    if session_valid(&state, req.headers()) || auth::has_api_key(req.headers(), &state.cfg.api_keys)
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
) -> Json<serde_json::Value> {
    Json(
        serde_json::json!({"ok":true,"service":"lnc-newsman","version":state.cfg.version,"authenticated":session_valid(&state, &headers),"management_configured":state.cfg.management_key.is_some()}),
    )
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
    let Some(key) = state.cfg.management_key.as_deref() else {
        return failure(
            StatusCode::SERVICE_UNAVAILABLE,
            "management_key_not_configured",
        );
    };
    let peer = peer
        .map(|axum::Extension(ConnectInfo(addr))| addr.ip().to_string())
        .unwrap_or_else(|| "unknown".into());
    let forwarded_for = headers.get("x-forwarded-for").and_then(|v| v.to_str().ok());
    let client = auth::client_key(&peer, forwarded_for, &state.cfg.trusted_proxies);
    if !state
        .login_limiter
        .lock()
        .expect("limiter lock poisoned")
        .allow_and_record(&client, now())
    {
        return failure(StatusCode::TOO_MANY_REQUESTS, "login_rate_limited");
    }
    let supplied = input
        .key
        .or(input.token)
        .or(input.password)
        .unwrap_or_default();
    if !auth::valid_key(&supplied, key) {
        return failure(StatusCode::UNAUTHORIZED, "invalid_management_key");
    }
    let token = match auth::new_token(key) {
        Ok(token) => token,
        Err(err) => return internal_error(err),
    };
    let timestamp = now();
    {
        let db = state.db.lock().expect("database lock poisoned");
        if let Err(err) = db.create_session(
            &auth::token_hash(&token),
            timestamp + auth::SESSION_SECONDS,
            timestamp,
        ) {
            return internal_error(err);
        }
        // Turn the alert engine's background sweep on at the same moment the first
        // admin logs in — until then nothing is enabled, so it has nothing to send.
        if !state.alerts_running.swap(true, Ordering::SeqCst) {
            alerts::spawn_sweeper(state.clone());
        }
    }
    state
        .login_limiter
        .lock()
        .expect("limiter lock poisoned")
        .clear(&client);
    let cookie = format!(
        "{}={}; Path=/; HttpOnly; SameSite=Lax; Max-Age={}{}",
        auth::COOKIE_NAME,
        token,
        auth::SESSION_SECONDS,
        if state.cfg.secure_cookie {
            "; Secure"
        } else {
            ""
        }
    );
    let mut response = Json(serde_json::json!({"ok":true,"authenticated":true})).into_response();
    response.headers_mut().insert(
        header::SET_COOKIE,
        HeaderValue::from_str(&cookie).expect("safe cookie"),
    );
    response
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
    let cookie = format!(
        "{}=; Path=/; Max-Age=0; HttpOnly; SameSite=Lax{}",
        auth::COOKIE_NAME,
        if state.cfg.secure_cookie {
            "; Secure"
        } else {
            ""
        }
    );
    let mut response = Json(serde_json::json!({"ok":true})).into_response();
    response.headers_mut().insert(
        header::SET_COOKIE,
        HeaderValue::from_str(&cookie).expect("safe cookie"),
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
    Ok(SettingsResponse {
        version: state.cfg.version,
        language: db.get_setting("language")?.unwrap_or_else(|| "vi".into()),
        default_reader_mode: db
            .get_setting("default_reader_mode")?
            .unwrap_or_else(|| "paper".into()),
        collector_enabled: collector::collector_enabled(&db),
    })
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
    let result = state
        .db
        .lock()
        .expect("database lock poisoned")
        .set_settings(&writes);
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
        Arc::new(AppState {
            cfg: config::Config {
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
            },
            db: Mutex::new(db::Database::in_memory().unwrap()),
            login_limiter: Mutex::new(auth::LoginLimiter::default()),
            started_at: now(),
            fetcher: fetch::Fetcher::public_only(),
            in_flight: Mutex::new(HashSet::new()),
            host_gates: Mutex::new(std::collections::HashMap::new()),
            host_gap: collector::HOST_GAP,
            secrets: None,
            connector_config: connectors::ConnectorConfig::default(),
            llm: llm::LlmConfig {
                base_url: None,
                model: "mock".into(),
            },
            hub: hub::ProviderConfig::default(),
            llm_enabled: false,
            pack: Mutex::new(regions::PackStatus::default()),
            alerts_running: AtomicBool::new(false),
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
