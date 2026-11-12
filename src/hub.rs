//! Shared provider layer for the LNC Agent Hub (and any OpenAI-compatible gateway).
//!
//! One provider serves both needs of P5/P6:
//! * **LLM completion** — `POST {base}/chat/completions` with a per-task model alias;
//! * **agent run** — `POST {base}/agents/{id}/run` from the Hub catalog, for research
//!   tasks that a single completion cannot do.
//!
//! Safety rules, all enforced here rather than in callers:
//! * the provider URL must be on the admin's allowlist; a host that resolves to a
//!   private address is only reachable when the admin named **that** host, and the
//!   `Fetcher` is built with exactly that permission;
//! * the Hub key is a secret (`hub-api-key`), never read from a request body, never
//!   returned by an API, never logged;
//! * a token budget per UTC day is checked before a call, and answers are cached by
//!   hash(model + prompt version + content);
//! * every call has a timeout, and any failure returns an error the caller can fall
//!   back from — "no LLM" mode always works.
//!
//! The public functions are used by the admin settings card and by `src/reader.rs`
//! for owner-initiated research and brief generation.
//!
//! Nothing here contacts the real Hub: tests use a loopback mock. See
//! `.docs/hub-integration.md` for what the owner must configure on the Hub side.

use crate::{
    SharedState, db,
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
use std::time::Duration;

pub const SECRET_NAME: &str = "hub-api-key";
pub const DEFAULT_DAILY_TOKEN_BUDGET: i64 = 50_000;
const MAX_AGENT_OUTPUT_CHARS: usize = 20_000;

/// Task classes, each with its own model alias so a cheap model handles classification
/// and a strong one handles analysis. Aliases are owner configuration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
#[allow(dead_code)]
pub enum Task {
    Classify,
    Summarise,
    Analyse,
}

impl Task {
    pub fn alias<'a>(&self, config: &'a ProviderConfig) -> &'a str {
        match self {
            Self::Classify => config.model_classify.as_str(),
            Self::Summarise => config.model_summarise.as_str(),
            Self::Analyse => config.model_analyse.as_str(),
        }
    }
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Classify => "classify",
            Self::Summarise => "summarise",
            Self::Analyse => "analyse",
        }
    }
}

/// Provider configuration, entirely from the environment. No field is settable from an
/// API request.
#[derive(Debug, Clone, Default)]
pub struct ProviderConfig {
    pub base_url: Option<String>,
    pub allowed_hosts: Vec<String>,
    pub model_classify: String,
    pub model_summarise: String,
    pub model_analyse: String,
    pub agent_id: String,
    pub timeout_seconds: u64,
}

impl ProviderConfig {
    pub fn from_env() -> Self {
        let env = |name: &str| {
            std::env::var(name)
                .ok()
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty())
        };
        Self {
            base_url: env("HUB_BASE_URL").map(|value| value.trim_end_matches('/').to_string()),
            allowed_hosts: std::env::var("HUB_ALLOWED_HOSTS")
                .unwrap_or_default()
                .split(',')
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(|value| value.to_ascii_lowercase())
                .collect(),
            model_classify: env("HUB_MODEL_CLASSIFY").unwrap_or_else(|| "cheap".into()),
            model_summarise: env("HUB_MODEL_SUMMARISE").unwrap_or_else(|| "default".into()),
            model_analyse: env("HUB_MODEL_ANALYSE").unwrap_or_else(|| "strong".into()),
            agent_id: env("HUB_AGENT_ID").unwrap_or_else(|| "news-analyst-argus".into()),
            timeout_seconds: env("HUB_TIMEOUT_SECONDS")
                .and_then(|value| value.parse().ok())
                .filter(|value: &u64| (5..=300).contains(value))
                .unwrap_or(60),
        }
    }
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[allow(dead_code)]
pub enum ProviderError {
    NotConfigured,
    HostNotAllowed,
    NoSecret,
    BudgetExhausted,
    Unauthorized,
    Timeout,
    Provider,
    InvalidAnswer,
}

impl ProviderError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::NotConfigured => "provider_not_configured",
            Self::HostNotAllowed => "provider_host_not_allowed",
            Self::NoSecret => "provider_secret_missing",
            Self::BudgetExhausted => "provider_budget_exhausted",
            Self::Unauthorized => "provider_unauthorized",
            Self::Timeout => "provider_timeout",
            Self::Provider => "provider_error",
            Self::InvalidAnswer => "provider_invalid_answer",
        }
    }
}

/// Is the provider usable at all? Off unless a base URL is configured **and** its host
/// is allowlisted. The second condition is what keeps an internal Hub reachable while
/// every other private address stays blocked.
pub fn configured(state: &SharedState) -> bool {
    let provider = &state.hub;
    let Some(base) = provider.base_url.as_deref() else {
        return false;
    };
    let Ok(url) = url::Url::parse(base) else {
        return false;
    };
    let Some(host) = url.host_str() else {
        return false;
    };
    provider
        .allowed_hosts
        .iter()
        .any(|allowed| allowed.eq_ignore_ascii_case(host))
}

/// A fetcher that carries the provider's per-host private permission. Built here so no
/// caller can grant private access to anything else.
pub fn fetcher(state: &SharedState) -> Fetcher {
    let private_hosts = state
        .hub
        .base_url
        .as_deref()
        .and_then(|base| url::Url::parse(base).ok())
        .and_then(|url| url.host_str().map(str::to_string))
        .filter(|host| {
            state
                .hub
                .allowed_hosts
                .iter()
                .any(|allowed| allowed.eq_ignore_ascii_case(host))
        })
        .into_iter()
        .collect();
    state.fetcher.clone().with_private_hosts(private_hosts)
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

pub fn usage(db: &db::Database, now: i64) -> (i64, i64, i64) {
    let used = db
        .conn
        .query_row(
            "SELECT tokens_used FROM llm_usage WHERE day=?1",
            [day_key(now)],
            |r| r.get(0),
        )
        .unwrap_or(0);
    let calls = db
        .conn
        .query_row(
            "SELECT COALESCE(calls,0) FROM llm_usage WHERE day=?1",
            [day_key(now)],
            |r| r.get(0),
        )
        .unwrap_or(0);
    (used, budget(db), calls)
}

pub fn cache_key(model: &str, task: &str, content: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(model.as_bytes());
    hasher.update(b"|");
    hasher.update(task.as_bytes());
    hasher.update(b"|");
    hasher.update(crate::llm::PROMPT_VERSION.as_bytes());
    hasher.update(b"|");
    hasher.update(content.as_bytes());
    hasher
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

fn hub_secret(state: &SharedState) -> Option<String> {
    let db = state.db.lock().ok()?;
    let vault: Option<&SecretBox> = state.secrets.as_ref();
    vault.and_then(|vault| secrets::get(&db, vault, SECRET_NAME).ok())
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

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Completion {
    pub text: String,
    pub model: String,
    pub tokens: i64,
    pub cached: bool,
}

/// One completion for a task class. Budget is checked before the request; the answer is
/// cached; a provider failure is returned as an error and never cached.
pub async fn complete(
    state: &SharedState,
    task: Task,
    prompt: &str,
) -> Result<Completion, ProviderError> {
    if !configured(state) {
        return Err(ProviderError::NotConfigured);
    }
    let base = state
        .hub
        .base_url
        .clone()
        .ok_or(ProviderError::NotConfigured)?;
    let model = task.alias(&state.hub).to_string();
    let key = cache_key(&model, task.as_str(), prompt);
    let now = crate::now() as i64;
    {
        let db = state.db.lock().expect("database lock poisoned");
        if let Ok(answer) = db.conn.query_row(
            "SELECT answer,tokens FROM llm_cache WHERE cache_key=?1",
            [&key],
            |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)),
        ) {
            return Ok(Completion {
                text: answer.0,
                model,
                tokens: answer.1,
                cached: true,
            });
        }
        let (used, limit, _) = usage(&db, now);
        if used >= limit {
            return Err(ProviderError::BudgetExhausted);
        }
    }
    let secret = hub_secret(state).ok_or(ProviderError::NoSecret)?;
    let max_tokens = match task {
        Task::Analyse => 2_000,
        Task::Classify | Task::Summarise => 800,
    };
    let body = serde_json::to_string(&ChatRequest {
        model: &model,
        messages: vec![ChatMessage {
            role: "user",
            content: prompt,
        }],
        temperature: 0.0,
        max_tokens,
    })
    .map_err(|_| ProviderError::Provider)?;
    let url = format!("{base}/chat/completions");
    let client = fetcher(state);
    let response = tokio::time::timeout(
        Duration::from_secs(state.hub.timeout_seconds.clamp(5, 300)),
        client.post_json(&url, &body, Some(&secret)),
    )
    .await
    .map_err(|_| ProviderError::Timeout)?;
    let bytes = match response {
        Ok(FetchOutcome::Body { bytes, .. }) => bytes,
        Ok(FetchOutcome::NotModified) => return Err(ProviderError::Provider),
        Err(crate::fetch::FetchError::HttpStatus(401 | 403)) => {
            return Err(ProviderError::Unauthorized);
        }
        Err(crate::fetch::FetchError::Timeout) => return Err(ProviderError::Timeout),
        Err(_) => return Err(ProviderError::Provider),
    };
    let parsed: ChatResponse =
        serde_json::from_slice(&bytes).map_err(|_| ProviderError::InvalidAnswer)?;
    let text = parsed
        .choices
        .first()
        .map(|choice| choice.message.content.trim().to_string())
        .filter(|text| !text.is_empty())
        .ok_or(ProviderError::InvalidAnswer)?;
    let tokens = parsed
        .usage
        .map(|usage| usage.total_tokens)
        .unwrap_or(1)
        .max(1);
    {
        let db = state.db.lock().expect("database lock poisoned");
        let day = day_key(now);
        let _ = db.conn.execute(
            "INSERT INTO llm_usage(day,tokens_used,calls) VALUES (?1,?2,1)
             ON CONFLICT(day) DO UPDATE SET tokens_used=tokens_used+excluded.tokens_used, calls=calls+1",
            rusqlite::params![day, tokens],
        );
        let _ = db.conn.execute(
            "INSERT OR REPLACE INTO llm_cache(cache_key,answer,tokens,model) VALUES (?1,?2,?3,?4)",
            rusqlite::params![key, text, tokens, model],
        );
    }
    Ok(Completion {
        text,
        model,
        tokens,
        cached: false,
    })
}

#[derive(Serialize)]
struct AgentRequest<'a> {
    input: &'a str,
}

#[derive(Deserialize)]
struct AgentResponse {
    #[serde(default)]
    output: Option<String>,
    #[serde(default)]
    text: Option<String>,
    #[serde(default)]
    usage: Option<Usage>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct AgentRun {
    pub text: String,
    pub agent: String,
    pub tokens: i64,
}

/// Runs a catalog agent for research. Same gates as a completion, plus a hard output cap
/// so a runaway agent cannot fill the database.
pub async fn run_agent(state: &SharedState, input: &str) -> Result<AgentRun, ProviderError> {
    if !configured(state) {
        return Err(ProviderError::NotConfigured);
    }
    let base = state
        .hub
        .base_url
        .clone()
        .ok_or(ProviderError::NotConfigured)?;
    let agent = state.hub.agent_id.clone();
    let now = crate::now() as i64;
    {
        let db = state.db.lock().expect("database lock poisoned");
        let (used, limit, _) = usage(&db, now);
        if used >= limit {
            return Err(ProviderError::BudgetExhausted);
        }
    }
    let secret = hub_secret(state).ok_or(ProviderError::NoSecret)?;
    let body =
        serde_json::to_string(&AgentRequest { input }).map_err(|_| ProviderError::Provider)?;
    let url = format!("{base}/agents/{agent}/run");
    let client = fetcher(state);
    let response = tokio::time::timeout(
        Duration::from_secs(state.hub.timeout_seconds.clamp(5, 300)),
        client.post_json(&url, &body, Some(&secret)),
    )
    .await
    .map_err(|_| ProviderError::Timeout)?;
    let bytes = match response {
        Ok(FetchOutcome::Body { bytes, .. }) => bytes,
        Ok(FetchOutcome::NotModified) => return Err(ProviderError::Provider),
        Err(crate::fetch::FetchError::HttpStatus(401 | 403)) => {
            return Err(ProviderError::Unauthorized);
        }
        Err(crate::fetch::FetchError::Timeout) => return Err(ProviderError::Timeout),
        Err(_) => return Err(ProviderError::Provider),
    };
    let parsed: AgentResponse =
        serde_json::from_slice(&bytes).map_err(|_| ProviderError::InvalidAnswer)?;
    let text = parsed
        .output
        .or(parsed.text)
        .map(|text| text.trim().to_string())
        .filter(|text| !text.is_empty())
        .ok_or(ProviderError::InvalidAnswer)?;
    if text.chars().count() > MAX_AGENT_OUTPUT_CHARS {
        return Err(ProviderError::InvalidAnswer);
    }
    let tokens = parsed
        .usage
        .map(|usage| usage.total_tokens)
        .unwrap_or(1)
        .max(1);
    {
        let db = state.db.lock().expect("database lock poisoned");
        let day = day_key(now);
        let _ = db.conn.execute(
            "INSERT INTO llm_usage(day,tokens_used,calls) VALUES (?1,?2,1)
             ON CONFLICT(day) DO UPDATE SET tokens_used=tokens_used+excluded.tokens_used, calls=calls+1",
            rusqlite::params![day, tokens],
        );
    }
    Ok(AgentRun {
        text,
        agent,
        tokens,
    })
}

#[derive(Deserialize)]
struct ModelsResponse {
    #[serde(default)]
    data: Vec<ModelEntry>,
}

#[derive(Deserialize)]
struct ModelEntry {
    id: String,
}

/// The only call the "Check connection" button makes: list models. It does not generate
/// content, so pressing it costs no tokens.
pub async fn list_models(state: &SharedState) -> Result<Vec<String>, ProviderError> {
    if !configured(state) {
        return Err(ProviderError::NotConfigured);
    }
    let base = state
        .hub
        .base_url
        .clone()
        .ok_or(ProviderError::NotConfigured)?;
    let secret = hub_secret(state).ok_or(ProviderError::NoSecret)?;
    let url = format!("{base}/models");
    let client = fetcher(state);
    let response = tokio::time::timeout(
        Duration::from_secs(state.hub.timeout_seconds.clamp(5, 60)),
        client.get_with_header(&url, "authorization", &format!("Bearer {secret}")),
    )
    .await
    .map_err(|_| ProviderError::Timeout)?;
    let bytes = match response {
        Ok(FetchOutcome::Body { bytes, .. }) => bytes,
        Ok(FetchOutcome::NotModified) => return Err(ProviderError::Provider),
        Err(crate::fetch::FetchError::HttpStatus(401 | 403)) => {
            return Err(ProviderError::Unauthorized);
        }
        Err(_) => return Err(ProviderError::Provider),
    };
    let parsed: ModelsResponse =
        serde_json::from_slice(&bytes).map_err(|_| ProviderError::InvalidAnswer)?;
    Ok(parsed
        .data
        .into_iter()
        .map(|model| model.id)
        .take(200)
        .collect())
}

#[derive(Serialize)]
pub struct ProviderStatus {
    configured: bool,
    base_host: Option<String>,
    allowed_hosts: Vec<String>,
    models: [String; 3],
    agent: String,
    timeout_seconds: u64,
    tokens_used_today: i64,
    daily_budget: i64,
    calls_today: i64,
    has_secret: bool,
}

/// Status for the settings page. Never returns the key, the full URL or any prompt.
pub async fn status(State(state): State<SharedState>) -> Response {
    let now = crate::now() as i64;
    let (used, budget, calls, has_secret) = {
        let db = state.db.lock().expect("database lock poisoned");
        let (used, budget, calls) = usage(&db, now);
        let has_secret = match state.secrets.as_ref() {
            Some(vault) => secrets::get(&db, vault, SECRET_NAME).is_ok(),
            None => false,
        };
        (used, budget, calls, has_secret)
    };
    let base_host = state
        .hub
        .base_url
        .as_deref()
        .and_then(|base| url::Url::parse(base).ok())
        .and_then(|url| url.host_str().map(str::to_string));
    Json(ProviderStatus {
        configured: configured(&state),
        base_host,
        allowed_hosts: state.hub.allowed_hosts.clone(),
        models: [
            state.hub.model_classify.clone(),
            state.hub.model_summarise.clone(),
            state.hub.model_analyse.clone(),
        ],
        agent: state.hub.agent_id.clone(),
        timeout_seconds: state.hub.timeout_seconds,
        tokens_used_today: used,
        daily_budget: budget,
        calls_today: calls,
        has_secret,
    })
    .into_response()
}

/// "Check connection": lists models only. Runs only when the admin presses the button.
pub async fn check(State(state): State<SharedState>) -> Response {
    match list_models(&state).await {
        Ok(models) => Json(serde_json::json!({
            "ok": true,
            "models": models,
            "note": "Only the model list was requested; no content was generated and no tokens were billed.",
        }))
        .into_response(),
        Err(err) => crate::failure(axum::http::StatusCode::BAD_GATEWAY, err.code()),
    }
}

#[cfg(test)]
#[path = "hub_tests.rs"]
mod tests;
