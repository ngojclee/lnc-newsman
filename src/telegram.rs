//! Telegram alert delivery (P7): relay through `telegram.lengoc.me`, or direct Bot API.
//!
//! Two configuration paths, one decision for the whole fleet:
//!
//! * **relay** — `POST {relay}/send` on the fleet Cygnus gateway. The bot lives on the
//!   relay; this app holds only `relay_token` (write-only) and the group `chat_id`.
//! * **direct** — the canonical Bot API with our own `bot_token` (write-only).
//!
//! Sending is network I/O and must never run while the database lock is held, so the
//! sweep only *queues*: every delivery that asks for Telegram leaves one row in
//! `alert_telegram_outbox`, and a flusher claims it (`pending` -> `sending`) before it
//! calls out. A row can therefore be sent at most once even with two flushers running,
//! and a row stranded in `sending` by a crash is reclaimed after [`STALE_CLAIM_SECONDS`].
//!
//! Nothing here invents data: the message is the article title and its link, exactly
//! what the in-app notification shows.

use crate::{SharedState, db, secrets};
use axum::{
    Json,
    extract::State,
    response::{IntoResponse, Response},
};
use rusqlite::params;
use serde::{Deserialize, Serialize};
use url::Url;

/// Channel name as stored in `alert_rules.channels`.
pub const CHANNEL: &str = "telegram";
/// Fleet relay; also the only relay host trusted without an explicit allowlist entry.
pub const DEFAULT_RELAY_BASE: &str = "https://telegram.lengoc.me";
pub const SECRET_RELAY: &str = "telegram-relay-token";
pub const SECRET_BOT: &str = "telegram-bot-token";
pub const CHAT_ID_KEY: &str = "telegram_chat_id";
pub const TOPIC_ID_KEY: &str = "telegram_alerts_topic_id";
pub const MODE_KEY: &str = "telegram_mode";

/// A claim left behind by a crashed flusher is taken over after this long.
pub const STALE_CLAIM_SECONDS: i64 = 300;
const MAX_ATTEMPTS: i64 = 5;
const BATCH: i64 = 20;
const TEXT_LIMIT: usize = 4096;
const SEND_TIMEOUT_SECONDS: u64 = 20;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Relay,
    Direct,
    Off,
}

impl Mode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Relay => "relay",
            Self::Direct => "direct",
            Self::Off => "off",
        }
    }
    fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "relay" => Some(Self::Relay),
            "direct" => Some(Self::Direct),
            "off" | "" => Some(Self::Off),
            _ => None,
        }
    }
}

/// Environment half of `env_or_cfg`. A missing `TELEGRAM_MODE` means "the admin's
/// choice in settings decides"; any value here wins over the stored setting.
#[derive(Debug, Clone)]
pub struct TelegramConfig {
    pub mode: Option<Mode>,
    pub relay_base_url: String,
    /// Hosts allowed to resolve to a private address for outbound Telegram calls.
    /// The default relay host is always allowed — it is compiled in, not admin-set.
    pub allowed_hosts: Vec<String>,
}

impl Default for TelegramConfig {
    fn default() -> Self {
        // The compiled-in relay host is always allowed: it is our own fleet service, not
        // an admin-supplied address, so it cannot be used to reach an arbitrary private
        // host. Anything else must be named in `TELEGRAM_ALLOWED_HOSTS`.
        let allowed_hosts = Url::parse(DEFAULT_RELAY_BASE)
            .ok()
            .and_then(|url| url.host_str().map(str::to_ascii_lowercase))
            .into_iter()
            .collect();
        Self {
            mode: None,
            relay_base_url: DEFAULT_RELAY_BASE.into(),
            allowed_hosts,
        }
    }
}

impl TelegramConfig {
    pub fn from_env() -> Self {
        let env = |name: &str| {
            std::env::var(name)
                .ok()
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty())
        };
        let mut allowed_hosts: Vec<String> = std::env::var("TELEGRAM_ALLOWED_HOSTS")
            .unwrap_or_default()
            .split(',')
            .map(|value| value.trim().to_ascii_lowercase())
            .filter(|value| !value.is_empty())
            .collect();
        if let Some(host) = Url::parse(DEFAULT_RELAY_BASE)
            .ok()
            .and_then(|url| url.host_str().map(str::to_ascii_lowercase))
            && !allowed_hosts.contains(&host)
        {
            allowed_hosts.push(host);
        }
        Self {
            mode: env("TELEGRAM_MODE").and_then(|value| Mode::parse(&value)),
            relay_base_url: env("TELEGRAM_RELAY_BASE_URL")
                .map(|value| value.trim_end_matches('/').to_string())
                .unwrap_or_else(|| DEFAULT_RELAY_BASE.into()),
            allowed_hosts,
        }
    }
}

/// Everything one send needs, resolved from settings + vault. `Err` carries the code
/// the admin surface reports, so "why is nothing arriving" always has an answer.
#[derive(Clone)]
pub struct Endpoint {
    pub mode: Mode,
    pub url: String,
    pub chat_id: i64,
    pub topic_id: Option<i64>,
    pub bearer: Option<String>,
}

impl std::fmt::Debug for Endpoint {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Direct mode carries the bot token in the URL path; never print it.
        f.debug_struct("Endpoint")
            .field("mode", &self.mode)
            .field("chat_id", &self.chat_id)
            .field("topic_id", &self.topic_id)
            .field("has_bearer", &self.bearer.is_some())
            .finish_non_exhaustive()
    }
}

pub fn stored_mode(db: &db::Database) -> Mode {
    db.get_setting(MODE_KEY)
        .ok()
        .flatten()
        .and_then(|value| Mode::parse(&value))
        .unwrap_or(Mode::Off)
}

/// Env wins when set; otherwise the stored setting. This is the whole `env_or_cfg` rule.
pub fn effective_mode(db: &db::Database, cfg: &TelegramConfig) -> Mode {
    cfg.mode.unwrap_or_else(|| stored_mode(db))
}

fn stored_chat_id(db: &db::Database) -> Option<i64> {
    db.get_setting(CHAT_ID_KEY)
        .ok()
        .flatten()
        .and_then(|value| value.trim().parse::<i64>().ok())
        .filter(|value| *value != 0)
}

fn stored_topic_id(db: &db::Database) -> Option<i64> {
    db.get_setting(TOPIC_ID_KEY)
        .ok()
        .flatten()
        .and_then(|value| value.trim().parse::<i64>().ok())
        .filter(|value| *value > 0)
}

fn host_allowed(cfg: &TelegramConfig, url: &Url) -> bool {
    url.host_str().is_some_and(|host| {
        cfg.allowed_hosts
            .iter()
            .any(|allowed| allowed.eq_ignore_ascii_case(host))
    })
}

/// Resolve the send endpoint. A relay base that is not https, or whose host is not
/// allowlisted, is refused here rather than at the network layer.
pub fn endpoint(
    db: &db::Database,
    vault: Option<&secrets::SecretBox>,
    cfg: &TelegramConfig,
) -> Result<Endpoint, &'static str> {
    match effective_mode(db, cfg) {
        Mode::Off => Err("telegram_off"),
        Mode::Relay => {
            let base = Url::parse(&cfg.relay_base_url).map_err(|_| "telegram_bad_relay_url")?;
            if base.scheme() != "https" {
                return Err("telegram_bad_relay_url");
            }
            if !host_allowed(cfg, &base) {
                return Err("telegram_relay_host_not_allowed");
            }
            let chat_id = stored_chat_id(db).ok_or("telegram_chat_id_missing")?;
            let token = vault
                .and_then(|vault| secrets::get(db, vault, SECRET_RELAY).ok())
                .ok_or("telegram_secret_missing")?;
            let url = format!("{}/send", cfg.relay_base_url.trim_end_matches('/'));
            Ok(Endpoint {
                mode: Mode::Relay,
                url,
                chat_id,
                topic_id: stored_topic_id(db),
                bearer: Some(token),
            })
        }
        Mode::Direct => {
            let chat_id = stored_chat_id(db).ok_or("telegram_chat_id_missing")?;
            let token = vault
                .and_then(|vault| secrets::get(db, vault, SECRET_BOT).ok())
                .ok_or("telegram_secret_missing")?;
            Ok(Endpoint {
                mode: Mode::Direct,
                url: format!("https://api.telegram.org/bot{token}/sendMessage"),
                chat_id,
                topic_id: stored_topic_id(db),
                // The token already lives in the URL path for the Bot API.
                bearer: None,
            })
        }
    }
}

/// Is the channel switched on at all? This decides whether a rule may *name* Telegram:
/// the declaration is about intent, so a missing token later shows up in the queue and
/// in `reason`, not as a rule the admin cannot save.
pub fn is_on(db: &db::Database, cfg: &TelegramConfig) -> bool {
    effective_mode(db, cfg) != Mode::Off
}

/// Can a send leave right now (mode on, destination and secret present)? Used to decide
/// whether the sweep queues an outbox row; a false answer never blocks the in-app
/// notification, and the reason surfaces on `/api/admin/telegram`.
pub fn can_send(db: &db::Database, vault: Option<&secrets::SecretBox>, cfg: &TelegramConfig) -> bool {
    endpoint(db, vault, cfg).is_ok()
}

/// The exact body either path sends. Relay wraps it as a Bot-API method call; direct
/// posts the payload itself.
pub fn payload(chat_id: i64, topic_id: Option<i64>, text: &str) -> serde_json::Value {
    let mut body = serde_json::json!({
        "chat_id": chat_id,
        "text": text,
        "disable_web_page_preview": true,
    });
    if let Some(topic) = topic_id {
        body["message_thread_id"] = serde_json::json!(topic);
    }
    body
}

pub fn relay_body(chat_id: i64, topic_id: Option<i64>, text: &str) -> serde_json::Value {
    serde_json::json!({
        "method": "sendMessage",
        "payload": payload(chat_id, topic_id, text),
    })
}

/// A short message: the same title and link the in-app inbox shows. Telegram rejects
/// bodies over 4096 characters, so the text is cut on a character boundary.
pub fn alert_text(title: &str, url: &str) -> String {
    let text = format!("{}\n{}", title.trim(), url.trim());
    match text.char_indices().nth(TEXT_LIMIT) {
        Some((cut, _)) => text[..cut].to_string(),
        None => text,
    }
}

/// Parse a Telegram (or relay) answer. Both report `{"ok":true,...}`; anything else is
/// a failure, and only the status code is kept — never the body or the token.
pub(crate) fn answer_ok(bytes: &[u8]) -> bool {
    serde_json::from_slice::<serde_json::Value>(bytes)
        .ok()
        .and_then(|value| value.get("ok").and_then(serde_json::Value::as_bool))
        .unwrap_or(false)
}

// ---------------------------------------------------------------- outbox

#[derive(Debug, Clone)]
pub struct Queued {
    pub delivery_id: i64,
    pub title: String,
    pub url: String,
}

pub fn enqueue(db: &db::Database, delivery_id: i64, now: i64) -> rusqlite::Result<()> {
    db.conn.execute(
        "INSERT OR IGNORE INTO alert_telegram_outbox(delivery_id,state,created_at,updated_at)
         VALUES (?1,'pending',?2,?2)",
        params![delivery_id, now],
    )?;
    Ok(())
}

/// Take up to `BATCH` due rows, marking them `sending` in the same transaction so a
/// second flusher cannot pick the same row. Rows already at the attempt limit are left
/// as `failed` for the admin to see, not retried forever.
pub fn claim(db: &mut db::Database, now: i64) -> rusqlite::Result<Vec<Queued>> {
    let tx = db.conn.transaction()?;
    tx.execute(
        "UPDATE alert_telegram_outbox SET state='pending', updated_at=?1
         WHERE state='sending' AND updated_at < ?2",
        params![now, now - STALE_CLAIM_SECONDS],
    )?;
    let due: Vec<Queued> = {
        let mut stmt = tx.prepare(
            "SELECT o.delivery_id, d.title, d.body
             FROM alert_telegram_outbox o
             JOIN alert_deliveries d ON d.id=o.delivery_id
             WHERE o.state='pending' AND o.attempts < ?1
             ORDER BY o.created_at, o.delivery_id LIMIT ?2",
        )?;
        stmt.query_map(params![MAX_ATTEMPTS, BATCH], |r| {
            Ok(Queued {
                delivery_id: r.get(0)?,
                title: r.get(1)?,
                url: r.get(2)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?
    };
    for row in &due {
        tx.execute(
            "UPDATE alert_telegram_outbox SET state='sending', attempts=attempts+1, updated_at=?2
             WHERE delivery_id=?1",
            params![row.delivery_id, now],
        )?;
    }
    tx.commit()?;
    Ok(due)
}

pub fn settle(
    db: &db::Database,
    delivery_id: i64,
    ok: bool,
    error: &str,
    now: i64,
) -> rusqlite::Result<()> {
    // A successful send is final; a failure stays `pending` until the attempt limit,
    // then `failed` — so the admin surface can show a queue that is not moving.
    db.conn.execute(
        "UPDATE alert_telegram_outbox
         SET state = CASE WHEN ?2=1 THEN 'sent'
                          WHEN attempts >= ?3 THEN 'failed'
                          ELSE 'pending' END,
             last_error = ?4, updated_at = ?5
         WHERE delivery_id=?1",
        params![delivery_id, ok as i64, MAX_ATTEMPTS, error, now],
    )?;
    Ok(())
}

#[derive(Debug, Default, Serialize)]
pub struct QueueStats {
    pub pending: i64,
    pub sending: i64,
    pub sent: i64,
    pub failed: i64,
}

pub fn queue_stats(db: &db::Database) -> rusqlite::Result<QueueStats> {
    let mut stats = QueueStats::default();
    let mut stmt = db
        .conn
        .prepare("SELECT state, COUNT(*) FROM alert_telegram_outbox GROUP BY state")?;
    let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))?;
    for row in rows {
        let (state, count) = row?;
        match state.as_str() {
            "pending" => stats.pending = count,
            "sending" => stats.sending = count,
            "sent" => stats.sent = count,
            "failed" => stats.failed = count,
            _ => {}
        }
    }
    Ok(stats)
}

// ---------------------------------------------------------------- flush

#[derive(Debug, Default, Serialize)]
pub struct FlushOutcome {
    pub sent: usize,
    pub failed: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub skipped: Option<&'static str>,
}

pub fn now_seconds() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

/// Send everything queued, then record each outcome. Returns what happened so both the
/// interval and the admin's "flush now" button show the same thing.
pub async fn flush(state: SharedState) -> FlushOutcome {
    let (endpoint, queued) = {
        let mut db = match state.db.lock() {
            Ok(db) => db,
            Err(_) => {
                return FlushOutcome {
                    skipped: Some("telegram_off"),
                    ..Default::default()
                };
            }
        };
        let endpoint = match endpoint(&db, state.secrets.as_ref(), &state.telegram) {
            Ok(endpoint) => endpoint,
            Err(code) => {
                return FlushOutcome {
                    skipped: Some(code),
                    ..Default::default()
                };
            }
        };
        let queued = claim(&mut db, now_seconds()).unwrap_or_default();
        (endpoint, queued)
    };
    if queued.is_empty() {
        return FlushOutcome::default();
    }
    let host = Url::parse(&endpoint.url)
        .ok()
        .and_then(|url| url.host_str().map(str::to_string));
    let client = match host {
        Some(host) => state.fetcher.clone().with_private_hosts(vec![host]),
        None => state.fetcher.clone(),
    };
    let mut outcome = FlushOutcome::default();
    for row in queued {
        let text = alert_text(&row.title, &row.url);
        let body = match endpoint.mode {
            Mode::Relay => relay_body(endpoint.chat_id, endpoint.topic_id, &text),
            _ => payload(endpoint.chat_id, endpoint.topic_id, &text),
        };
        let sent = serde_json::to_string(&body).ok();
        let result = match sent {
            Some(body) => {
                tokio::time::timeout(
                    std::time::Duration::from_secs(SEND_TIMEOUT_SECONDS),
                    client.post_json(&endpoint.url, &body, endpoint.bearer.as_deref()),
                )
                .await
            }
            None => {
                settle(
                    &state.db.lock().expect("database lock poisoned"),
                    row.delivery_id,
                    false,
                    "encode_failed",
                    now_seconds(),
                )
                .ok();
                outcome.failed += 1;
                continue;
            }
        };
        let (ok, error) = match result {
            Ok(Ok(crate::fetch::FetchOutcome::Body { bytes, .. })) => {
                let ok = answer_ok(&bytes);
                (ok, if ok { String::new() } else { "relay_rejected".into() })
            }
            Ok(Ok(crate::fetch::FetchOutcome::NotModified)) => (false, "unexpected_304".into()),
            Ok(Err(err)) => (false, err.code()),
            Err(_) => (false, "timeout".to_string()),
        };
        settle(
            &state.db.lock().expect("database lock poisoned"),
            row.delivery_id,
            ok,
            &error,
            now_seconds(),
        )
        .ok();
        if ok {
            outcome.sent += 1;
        } else {
            outcome.failed += 1;
            tracing::warn!(delivery = row.delivery_id, %error, "telegram send failed");
        }
    }
    outcome
}

// ---------------------------------------------------------------- handlers

#[derive(Debug, Serialize)]
struct StatusPayload {
    /// Mode that will actually be used (env override applied).
    mode: &'static str,
    /// Mode the admin stored in settings.
    stored_mode: &'static str,
    env_override: bool,
    relay_base_url: String,
    relay_host_allowed: bool,
    chat_id: Option<i64>,
    alerts_topic_id: Option<i64>,
    has_relay_token: bool,
    has_bot_token: bool,
    /// True when a send would leave the app right now.
    ready: bool,
    /// Why not, in the same vocabulary the API uses for errors.
    reason: Option<&'static str>,
    queue: QueueStats,
}

/// Admin view: what is configured, what is missing, and what is queued. No secret is
/// ever echoed — only whether one is stored.
pub async fn status(State(state): State<SharedState>) -> Response {
    let db = state.db.lock().expect("database lock poisoned");
    let vault = state.secrets.as_ref();
    let resolved = endpoint(&db, vault, &state.telegram);
    let has = |name: &str| match vault {
        Some(vault) => secrets::get(&db, vault, name).is_ok(),
        None => false,
    };
    let relay_host_allowed = Url::parse(&state.telegram.relay_base_url)
        .ok()
        .map(|url| host_allowed(&state.telegram, &url))
        .unwrap_or(false);
    let queue = queue_stats(&db).unwrap_or_default();
    let payload = StatusPayload {
        mode: effective_mode(&db, &state.telegram).as_str(),
        stored_mode: stored_mode(&db).as_str(),
        env_override: state.telegram.mode.is_some(),
        relay_base_url: state.telegram.relay_base_url.clone(),
        relay_host_allowed,
        chat_id: stored_chat_id(&db),
        alerts_topic_id: stored_topic_id(&db),
        has_relay_token: has(SECRET_RELAY),
        has_bot_token: has(SECRET_BOT),
        ready: resolved.is_ok(),
        reason: resolved.err(),
        queue,
    };
    Json(payload).into_response()
}

pub async fn flush_handler(State(state): State<SharedState>) -> Response {
    let outcome = flush(state).await;
    Json(serde_json::json!({
        "ok": true,
        "sent": outcome.sent,
        "failed": outcome.failed,
        "skipped": outcome.skipped,
    }))
    .into_response()
}

/// Settings writes accepted by `PATCH /api/settings`. Returned separately so the caller
/// can validate the whole patch before writing anything.
pub fn settings_writes(patch: &TelegramSettingsPatch) -> Result<Vec<(&'static str, String)>, &'static str> {
    let mut writes = Vec::new();
    if let Some(mode) = patch.telegram_mode.as_ref() {
        let mode = Mode::parse(mode).ok_or("invalid_telegram_mode")?;
        writes.push((MODE_KEY, mode.as_str().to_string()));
    }
    if let Some(chat) = patch.telegram_chat_id.as_ref() {
        let chat = chat.trim();
        if chat.is_empty() {
            writes.push((CHAT_ID_KEY, String::new()));
        } else {
            let value = chat.parse::<i64>().map_err(|_| "invalid_telegram_chat_id")?;
            if value == 0 {
                return Err("invalid_telegram_chat_id");
            }
            writes.push((CHAT_ID_KEY, value.to_string()));
        }
    }
    if let Some(topic) = patch.telegram_alerts_topic_id.as_ref() {
        let topic = topic.trim();
        if topic.is_empty() {
            writes.push((TOPIC_ID_KEY, String::new()));
        } else {
            let value = topic.parse::<i64>().map_err(|_| "invalid_telegram_topic_id")?;
            if value <= 0 {
                return Err("invalid_telegram_topic_id");
            }
            writes.push((TOPIC_ID_KEY, value.to_string()));
        }
    }
    Ok(writes)
}

#[derive(Debug, Default, Deserialize)]
pub struct TelegramSettingsPatch {
    pub telegram_mode: Option<String>,
    pub telegram_chat_id: Option<String>,
    pub telegram_alerts_topic_id: Option<String>,
}

#[cfg(test)]
#[path = "telegram_tests.rs"]
mod tests;
