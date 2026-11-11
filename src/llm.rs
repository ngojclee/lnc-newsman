//! LLM layer (P5 foundation). The provider is pluggable and **no real provider ships**.
//!
//! Design rules that make this safe before any model is connected:
//! * the layer is **off by default** (`llm_enabled` setting false), and the rule
//!   classifier always runs, so "no-LLM mode" is the normal mode;
//! * a call needs a base URL and an API key from the secret store; neither is accepted
//!   from a request body, so an admin cannot be tricked into pointing the app at an
//!   arbitrary host through the API;
//! * the base URL must pass the same SSRF checks as every other outbound call, and the
//!   request goes through `fetch::Fetcher`;
//! * a UTC-daily token budget is enforced **before** the request, and the cache is keyed
//!   by a hash of model + prompt version + content, so repeating work is free;
//! * any failure returns `None` and the rule labels stand — the reader never breaks
//!   because a model is down.

use crate::{
    SharedState,
    classify::Classification,
    db,
    fetch::{FetchOutcome, Fetcher},
    secrets::{self, SecretBox},
};
use axum::{
    Json,
    extract::State,
    response::{IntoResponse, Response},
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// The prompt version is part of the cache key: changing the prompt invalidates answers
/// without touching stored data.
pub const PROMPT_VERSION: &str = "classify-v1";
pub const SECRET_NAME: &str = "llm-api-key";
pub const DEFAULT_DAILY_TOKEN_BUDGET: i64 = 50_000;
/// Refuse a single call that would obviously exceed a day's budget.
const MAX_TOKENS_PER_CALL: i64 = 4_000;

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct LlmLabel {
    pub topic: Option<String>,
    pub entities: Vec<String>,
    pub model: String,
    pub cached: bool,
    pub tokens: i64,
}

/// Provider configuration. `base_url` is owner configuration (environment), never input.
#[derive(Debug, Clone)]
pub struct LlmConfig {
    pub base_url: Option<String>,
    pub model: String,
}

impl LlmConfig {
    pub fn from_env() -> Self {
        Self {
            base_url: std::env::var("LLM_BASE_URL")
                .ok()
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty())
                .map(|value| value.trim_end_matches('/').to_string()),
            model: std::env::var("LLM_MODEL")
                .ok()
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty())
                .unwrap_or_else(|| "mock".into()),
        }
    }
}

/// Is the LLM layer allowed to run? Off unless the admin enabled it *and* the provider
/// is configured. The rule layer is unaffected either way.
pub fn enabled(state: &SharedState) -> bool {
    if state.llm.base_url.is_none() {
        return false;
    }
    // `llm_enabled` is stored in settings so a restart keeps the admin's choice; the
    // in-memory field is only a fast path set at startup.
    let from_settings = state
        .db
        .lock()
        .ok()
        .map(|db| db.get_setting("llm_enabled").ok().flatten().as_deref() == Some("true"))
        .unwrap_or(false);
    state.llm_enabled || from_settings
}

pub fn day_key(now: i64) -> String {
    chrono::DateTime::from_timestamp(now, 0)
        .unwrap_or_default()
        .format("%Y-%m-%d")
        .to_string()
}

fn budget(db: &db::Database) -> i64 {
    db.get_setting("llm_daily_token_budget")
        .ok()
        .flatten()
        .and_then(|value| value.parse::<i64>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(DEFAULT_DAILY_TOKEN_BUDGET)
}

pub fn tokens_used_today(db: &db::Database, now: i64) -> rusqlite::Result<i64> {
    db.conn
        .query_row(
            "SELECT tokens_used FROM llm_usage WHERE day=?1",
            [day_key(now)],
            |r| r.get(0),
        )
        .or_else(|err| match err {
            rusqlite::Error::QueryReturnedNoRows => Ok(0),
            other => Err(other),
        })
}

pub fn cache_key(model: &str, content: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(model.as_bytes());
    hasher.update(b"|");
    hasher.update(PROMPT_VERSION.as_bytes());
    hasher.update(b"|");
    hasher.update(content.as_bytes());
    hasher
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

#[derive(Serialize)]
struct ChatRequest<'a> {
    model: &'a str,
    messages: Vec<ChatMessage<'a>>,
    temperature: f32,
    max_tokens: i64,
}

#[derive(Serialize)]
struct ChatMessage<'a> {
    role: &'a str,
    content: &'a str,
}

#[derive(Deserialize)]
struct ChatResponse {
    #[serde(default)]
    choices: Vec<Choice>,
    #[serde(default)]
    usage: Option<Usage>,
}

#[derive(Deserialize)]
struct Choice {
    message: Msg,
}

#[derive(Deserialize)]
struct Msg {
    content: String,
}

#[derive(Deserialize)]
struct Usage {
    #[serde(default)]
    total_tokens: i64,
}

/// What the model is asked. Deliberately tiny and strict: JSON only, no prose, no
/// publisher text beyond the title.
pub fn prompt(title: &str, rules: &Classification) -> String {
    let hint = rules
        .topic
        .as_ref()
        .map(|topic| topic.value.as_str())
        .unwrap_or("none");
    format!(
        "Classify this news headline. Return JSON only: {{\"topic\":\"<slug>\",\"entities\":[\"TYPE:VALUE\"]}}.\n\
         Allowed topics: finance, technology-ai, politics, ecommerce, world.\n\
         Allowed entity types: TICKER, ORG, CRYPTO, CURRENCY.\n\
         The rule classifier guessed topic={hint}; correct it only if clearly wrong.\n\
         Headline: {title}"
    )
}

/// Asks the model, honouring budget and cache. Returns `None` on any failure so callers
/// can keep the rule labels.
pub async fn classify(
    state: &SharedState,
    title: &str,
    rules: &Classification,
) -> Option<LlmLabel> {
    let base_url = state.llm.base_url.as_deref()?;
    let model = state.llm.model.clone();
    let now = crate::now() as i64;
    let key = cache_key(&model, title);
    {
        let db = state.db.lock().expect("database lock poisoned");
        if let Ok(Some((answer, tokens))) = db
            .conn
            .query_row(
                "SELECT answer,tokens FROM llm_cache WHERE cache_key=?1",
                [&key],
                |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)),
            )
            .map(Some)
            .or_else(|err| match err {
                rusqlite::Error::QueryReturnedNoRows => Ok(None),
                other => Err(other),
            })
        {
            let mut label = parse_answer(&answer, &model)?;
            label.cached = true;
            label.tokens = tokens;
            return Some(label);
        }
        let limit = budget(&db);
        let used = tokens_used_today(&db, now).unwrap_or(limit);
        if used >= limit {
            tracing::warn!(used, limit, "llm token budget exhausted; rules only");
            return None;
        }
    }
    let secret = {
        let db = state.db.lock().expect("database lock poisoned");
        let vault: Option<&SecretBox> = state.secrets.as_ref();
        vault.and_then(|vault| secrets::get(&db, vault, SECRET_NAME).ok())
    };
    let url = format!("{base_url}/chat/completions");
    let body = serde_json::to_string(&ChatRequest {
        model: &model,
        messages: vec![ChatMessage {
            role: "user",
            content: &prompt(title, rules),
        }],
        temperature: 0.0,
        max_tokens: 200,
    })
    .ok()?;
    // The request goes through the shared fetcher, so the provider URL gets the same
    // SSRF and size checks as every other outbound call.
    let client: Fetcher = state.fetcher.clone();
    let response = client
        .post_json(&url, &body, secret.as_deref())
        .await
        .ok()?;
    let bytes = match response {
        FetchOutcome::Body { bytes, .. } => bytes,
        FetchOutcome::NotModified => return None,
    };
    let parsed: ChatResponse = serde_json::from_slice(&bytes).ok()?;
    let usage = parsed.usage.map(|u| u.total_tokens).unwrap_or(0).max(1);
    let answer = parsed.choices.first()?.message.content.clone();
    let label = parse_answer(&answer, &model)?;
    if label.tokens > MAX_TOKENS_PER_CALL {
        return None;
    }
    {
        let db = state.db.lock().expect("database lock poisoned");
        let day = day_key(now);
        let _ = db.conn.execute(
            "INSERT INTO llm_usage(day,tokens_used,calls) VALUES (?1,?2,1)
             ON CONFLICT(day) DO UPDATE SET tokens_used=tokens_used+excluded.tokens_used, calls=calls+1",
            rusqlite::params![day, usage],
        );
        let _ = db.conn.execute(
            "INSERT OR REPLACE INTO llm_cache(cache_key,answer,tokens,model) VALUES (?1,?2,?3,?4)",
            rusqlite::params![key, answer, usage, model],
        );
    }
    Some(LlmLabel {
        topic: label.topic,
        entities: label.entities,
        model,
        cached: false,
        tokens: usage,
    })
    .map(|mut label| {
        label.tokens = usage;
        label
    })
}

/// Parses the model answer defensively: unknown topics are dropped rather than stored.
pub fn parse_answer(answer: &str, model: &str) -> Option<LlmLabel> {
    let cleaned = answer
        .trim()
        .trim_start_matches("```json")
        .trim_end_matches("```")
        .trim();
    let value: serde_json::Value = serde_json::from_str(cleaned).ok()?;
    let topic = value
        .get("topic")
        .and_then(|topic| topic.as_str())
        .map(str::trim)
        .filter(|slug| {
            crate::classify::TOPIC_RULES
                .iter()
                .any(|(allowed, _)| allowed == slug)
        })
        .map(str::to_string);
    let entities = value
        .get("entities")
        .and_then(|entities| entities.as_array())
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.as_str())
                .map(str::trim)
                .filter(|item| {
                    let Some((kind, rest)) = item.split_once(':') else {
                        return false;
                    };
                    matches!(kind, "TICKER" | "ORG" | "CRYPTO" | "CURRENCY")
                        && !rest.is_empty()
                        && rest.len() <= 32
                        && rest
                            .chars()
                            .all(|ch| ch.is_ascii_alphanumeric() || ch == '.' || ch == '-')
                })
                .take(10)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default();
    Some(LlmLabel {
        topic,
        entities,
        model: model.into(),
        cached: false,
        tokens: 0,
    })
}

pub fn usage(db: &db::Database, now: i64) -> rusqlite::Result<(i64, i64, i64)> {
    let used = tokens_used_today(db, now)?;
    let calls: i64 = db
        .conn
        .query_row(
            "SELECT COALESCE(calls,0) FROM llm_usage WHERE day=?1",
            [day_key(now)],
            |r| r.get(0),
        )
        .unwrap_or(0);
    Ok((used, budget(db), calls))
}

#[cfg(test)]
#[path = "llm_tests.rs"]
mod tests;

#[derive(Serialize)]
pub struct LlmStatus {
    configured: bool,
    enabled: bool,
    model: String,
    tokens_used_today: i64,
    daily_budget: i64,
    calls_today: i64,
    has_secret: bool,
}

/// Admin status: never returns the key, the base URL or any prompt text.
pub async fn status(State(state): State<SharedState>) -> impl IntoResponse {
    let now = crate::now() as i64;
    let (used, budget, calls, has_secret, toggle) = {
        let db = state.db.lock().expect("database lock poisoned");
        let (used, budget, calls) = usage(&db, now).unwrap_or((0, DEFAULT_DAILY_TOKEN_BUDGET, 0));
        let has_secret = match state.secrets.as_ref() {
            Some(vault) => secrets::get(&db, vault, SECRET_NAME).is_ok(),
            None => false,
        };
        // Read the switch while the lock is held: `enabled()` takes the lock itself, and
        // calling it here would deadlock a non-reentrant mutex.
        let toggle = state.llm_enabled
            || db.get_setting("llm_enabled").ok().flatten().as_deref() == Some("true");
        (used, budget, calls, has_secret, toggle)
    };
    Json(LlmStatus {
        configured: state.llm.base_url.is_some(),
        enabled: state.llm.base_url.is_some() && toggle,
        model: state.llm.model.clone(),
        tokens_used_today: used,
        daily_budget: budget,
        calls_today: calls,
        has_secret,
    })
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LlmToggle {
    enabled: bool,
}

/// Turns the LLM layer on or off. Turning it on without a configured provider is
/// refused, so the switch can never silently do nothing.
pub async fn set_enabled(
    State(state): State<SharedState>,
    Json(input): Json<LlmToggle>,
) -> Response {
    set_enabled_inner(state, input).await
}

async fn set_enabled_inner(state: SharedState, input: LlmToggle) -> Response {
    if input.enabled && state.llm.base_url.is_none() {
        return crate::failure(axum::http::StatusCode::CONFLICT, "llm_not_configured");
    }
    let writes = [("llm_enabled", if input.enabled { "true" } else { "false" })];
    if let Err(err) = state
        .db
        .lock()
        .expect("database lock poisoned")
        .set_settings(&writes)
    {
        return crate::internal_error(err);
    }
    status(State(state)).await.into_response()
}
