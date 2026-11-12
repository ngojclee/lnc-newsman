//! A read-only MCP endpoint over the REST v1 data (P8, offline part).
//!
//! JSON-RPC 2.0 over a single `POST /mcp`, which is what an MCP client speaking the
//! "streamable HTTP" transport sends when it does not need a server-initiated stream.
//! Every tool answers by calling the same functions `/api/v1/*` uses, so a tool can
//! never expose a row the REST API hides (enabled regions, enabled topics, disabled
//! series). Nothing here writes.
//!
//! Deliberately omitted until their phases land: `market_snapshot` (needs live
//! provider data), `region_brief` and `alert_inbox` (P6/P7 session-scoped content),
//! and every write tool. A missing tool is better than one that answers "no data"
//! for months.

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde_json::{Value, json};

use crate::SharedState;
use crate::connector::{Caller, scope_allows};

/// Protocol revision this endpoint implements.
const PROTOCOL_VERSION: &str = "2025-06-18";
/// Identical for an unknown tool and for a tool the caller may not use, so a caller
/// cannot enumerate the namespace by comparing two errors.
pub const UNKNOWN_TOOL: &str = "Unknown or unauthorized MCP tool";
pub const SERVER_NAME: &str = "lnc-newsman";

/// One tool: name, description and its input JSON schema. The list is the single
/// source of truth for `tools/list` and for the API documentation check.
///
/// Names are **bare** (`regions`, not `newsman__regions`): the Business MCP Hub adds
/// its namespace when it proxies, so a prefixed name here would reach an agent as
/// `newsman__newsman__regions`. `newsman__*` is still accepted as an alias so a client
/// written against v0.4.3 keeps working.
pub struct Tool {
    pub name: &'static str,
    pub description: &'static str,
    pub input: Value,
}

/// The scope a tool requires. There is exactly one, because every tool is a read; a
/// write tool would have to add its own scope here before it could be granted.
pub const TOOL_SCOPE: &str = crate::connector::READ_SCOPE;

/// The scope needed to call `tool`, or `None` when the name is not a tool we publish.
/// An unknown name denying here is what makes "unlisted tool" and "missing scope" the
/// same answer: a caller learns nothing from probing.
pub fn tool_scope(tool: &str) -> Option<&'static str> {
    let bare = tool.strip_prefix("newsman__").unwrap_or(tool);
    tools()
        .iter()
        .any(|known| known.name == bare)
        .then_some(TOOL_SCOPE)
}

fn empty_object() -> Value {
    json!({"type": "object", "properties": {}, "additionalProperties": false})
}

pub fn tools() -> Vec<Tool> {
    vec![
        Tool {
            name: "regions",
            description: "Enabled regions and the effective set (a region includes its parents).",
            input: empty_object(),
        },
        Tool {
            name: "topics",
            description: "Enabled topics in the reader's tree order.",
            input: empty_object(),
        },
        Tool {
            name: "latest_articles",
            description: "Newest visible articles (titles, links and labels only; no article text is stored).",
            input: json!({
                "type": "object",
                "properties": {
                    "region": {"type": "string"},
                    "topic": {"type": "string", "description": "Topic slug; includes child topics."},
                    "limit": {"type": "integer", "minimum": 1, "maximum": 100, "default": 20}
                },
                "additionalProperties": false
            }),
        },
        Tool {
            name: "documents",
            description: "Official documents (metadata and our written summary, with the legal disclaimer).",
            input: json!({
                "type": "object",
                "properties": {
                    "region": {"type": "string"},
                    "topic": {"type": "string"},
                    "q": {"type": "string", "description": "Accents are optional (thue finds thuế)."},
                    "limit": {"type": "integer", "minimum": 1, "maximum": 100, "default": 20}
                },
                "additionalProperties": false
            }),
        },
        Tool {
            name: "tax_calendar",
            description: "Declared tax deadlines for the enabled regions. Deadlines are pack data and may be unverified; the response says so.",
            input: json!({
                "type": "object",
                "properties": {
                    "region": {"type": "string"},
                    "horizon_days": {"type": "integer", "minimum": 1, "maximum": 366, "default": 120}
                },
                "additionalProperties": false
            }),
        },
        Tool {
            name: "series",
            description: "Enabled numeric time series and their credit line, by region.",
            input: json!({"type": "object", "properties": {
                "region": {"type": "string"},
                "limit": {"type": "integer", "minimum": 1, "maximum": 500, "default": 100}
            }, "additionalProperties": false}),
        },
        Tool {
            name: "observations",
            description: "Points of one series, newest first, with the as-of time and revision of each point.",
            input: json!({
                "type": "object",
                "properties": {
                    "code": {"type": "string", "description": "Series code, from the series tool."},
                    "limit": {"type": "integer", "minimum": 1, "maximum": 200, "default": 30}
                },
                "required": ["code"],
                "additionalProperties": false
            }),
        },
    ]
}

fn tool_descriptors(caller: Option<&Caller>) -> Value {
    Value::Array(
        tools()
            .into_iter()
            .filter(|tool| scope_allows(caller, tool.name))
            .map(|tool| {
                json!({
                    "name": tool.name,
                    "description": tool.description,
                    "inputSchema": tool.input
                })
            })
            .collect(),
    )
}

fn rpc_error(request_id: Value, code: i64, message: &str) -> Response {
    // JSON-RPC-level failures still answer HTTP 200; that is what clients expect.
    Json(json!({
        "jsonrpc": "2.0",
        "id": request_id,
        "error": {"code": code, "message": message}
    }))
    .into_response()
}

/// A tool failure. It travels inside the result (`isError: true`) so a client can show
/// the message without treating the whole call as a transport failure.
fn tool_error(message: &str) -> Value {
    json!({"content": [{"type": "text", "text": message}], "isError": true})
}

fn tool_text(payload: Value) -> Value {
    json!({
        "content": [{"type": "text", "text": payload.to_string()}],
        "isError": false
    })
}

fn string_arg(arguments: &Value, key: &str) -> Option<String> {
    arguments
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn int_arg(arguments: &Value, key: &str, default: i64, min: i64, max: i64) -> Result<i64, String> {
    match arguments.get(key) {
        None | Some(Value::Null) => Ok(default),
        Some(value) => value
            .as_i64()
            .map(|n| n.clamp(min, max))
            .ok_or_else(|| format!("`{key}` must be an integer")),
    }
}

/// Every tool call goes through here. `Err` is a caller mistake (shown as `isError`);
/// a database error becomes the same shape rather than a 500, because a client cannot
/// do anything with a transport failure.
fn call_tool(state: &SharedState, name: &str, arguments: &Value) -> Value {
    match dispatch(state, name, arguments) {
        Ok(value) => tool_text(value),
        Err(message) => tool_error(&message),
    }
}

fn dispatch(state: &SharedState, name: &str, arguments: &Value) -> Result<Value, String> {
    let db = state
        .db
        .lock()
        .map_err(|_| "database lock poisoned".to_string())?;
    match name {
        "regions" => crate::regions::payload(&db, true).map_err(|err| err.to_string()),
        "topics" => db
            .topics()
            .map(|topics| json!({"data": topics}))
            .map_err(|err| err.to_string()),
        "latest_articles" => {
            let region = string_arg(arguments, "region");
            let topic = string_arg(arguments, "topic");
            let limit = int_arg(arguments, "limit", 20, 1, 100)?;
            if let Some(code) = region.as_deref()
                && !matches!(crate::regions::region_enabled(&db, code), Ok(true))
            {
                return Err(format!("region `{code}` is not enabled"));
            }
            crate::collector::list_articles(
                &db,
                topic.as_deref(),
                region.as_deref(),
                None,
                limit,
                0,
            )
            .map(|data| json!({"data": data}))
            .map_err(|err| err.to_string())
        }
        "documents" => {
            let region = string_arg(arguments, "region");
            let topic = string_arg(arguments, "topic");
            let q = string_arg(arguments, "q");
            let limit = int_arg(arguments, "limit", 20, 1, 100)?;
            if let Some(code) = region.as_deref()
                && !matches!(crate::regions::region_enabled(&db, code), Ok(true))
            {
                return Err(format!("region `{code}` is not enabled"));
            }
            crate::documents::list(
                &db,
                region.as_deref(),
                topic.as_deref(),
                q.as_deref(),
                limit,
            )
            .map(|data| json!({"data": data}))
            .map_err(|err| err.to_string())
        }
        "tax_calendar" => {
            let region = string_arg(arguments, "region");
            let horizon = int_arg(arguments, "horizon_days", 120, 1, 366)?;
            if let Some(code) = region.as_deref()
                && !matches!(crate::regions::region_enabled(&db, code), Ok(true))
            {
                return Err(format!("region `{code}` is not enabled"));
            }
            let data = crate::tax::calendar(&db, region.as_deref(), crate::now() as i64, horizon)
                .map_err(|err| err.to_string())?;
            Ok(json!({
                "data": data,
                "disclaimer_vi": crate::tax::DISCLAIMER_VI,
                "disclaimer_en": crate::tax::DISCLAIMER_EN,
                "holidays_modelled": false
            }))
        }
        "series" => {
            let region = string_arg(arguments, "region");
            let limit = int_arg(arguments, "limit", 100, 1, 500)?;
            if let Some(code) = region.as_deref()
                && !matches!(crate::regions::region_enabled(&db, code), Ok(true))
            {
                return Err(format!("region `{code}` is not enabled"));
            }
            let effective =
                crate::regions::effective(&db, region.as_deref()).map_err(|err| err.to_string())?;
            crate::series::list_in(&db, &effective, limit)
                .map(|data| json!({"data": data}))
                .map_err(|err| err.to_string())
        }
        "observations" => {
            let code = string_arg(arguments, "code").ok_or("`code` is required")?;
            let limit = int_arg(arguments, "limit", 30, 1, 200)?;
            let stored = crate::series::series(&db, &code)
                .map_err(|err| err.to_string())?
                .ok_or_else(|| format!("no series `{code}`"))?;
            if !matches!(
                crate::regions::region_enabled(&db, &stored.region),
                Ok(true)
            ) {
                return Err(format!(
                    "series `{code}` is in a region that is not enabled"
                ));
            }
            crate::series::observations(&db, &code, None, limit)
                .map(|data| json!({"data": data, "series": stored}))
                .map_err(|err| err.to_string())
        }
        other => Err(format!("unknown tool `{other}`")),
    }
}

pub async fn handle(
    State(state): State<SharedState>,
    caller: Option<axum::Extension<Caller>>,
    Json(body): Json<Value>,
) -> Response {
    // A reader session or key carries no delegation; it sees everything, exactly as
    // it did before the connector existed.
    let caller = caller.map(|axum::Extension(caller)| caller);
    // A batch is accepted and answered in order; MCP clients may send one.
    if let Value::Array(calls) = body {
        if calls.is_empty() {
            return rpc_error(Value::Null, -32600, "empty batch");
        }
        let answers: Vec<Value> = calls
            .into_iter()
            .map(|call| single(&state, caller.as_ref(), call))
            .collect();
        return Json(Value::Array(answers)).into_response();
    }
    let answer = single(&state, caller.as_ref(), body);
    Json(answer).into_response()
}

fn single(state: &SharedState, caller: Option<&Caller>, body: Value) -> Value {
    let id = body.get("id").cloned().unwrap_or(Value::Null);
    let method = body.get("method").and_then(Value::as_str).unwrap_or("");
    let params = body.get("params").cloned().unwrap_or(Value::Null);
    match method {
        "initialize" => json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": {
                "protocolVersion": PROTOCOL_VERSION,
                "capabilities": {"tools": {"listChanged": false}},
                "serverInfo": {"name": SERVER_NAME, "version": env!("CARGO_PKG_VERSION")},
                "instructions": "Read-only news, official documents, tax deadlines and numeric series. Article text is never stored; every item links to its original."
            }
        }),
        "notifications/initialized" | "initialized" => {
            json!({"jsonrpc": "2.0", "id": id, "result": {}})
        }
        "ping" => json!({"jsonrpc": "2.0", "id": id, "result": {}}),
        "tools/list" => json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": {"tools": tool_descriptors(caller)}
        }),
        "tools/call" => {
            let name = params.get("name").and_then(Value::as_str).unwrap_or("");
            let bare = name.strip_prefix("newsman__").unwrap_or(name);
            let known = tools().iter().any(|tool| tool.name == bare);
            // Discovery and direct calls use the same rule: an unlisted tool and a
            // tool the caller has no scope for answer identically, so probing the
            // namespace yields nothing.
            if !known || !scope_allows(caller, bare) {
                return json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "error": {"code": -32601, "message": UNKNOWN_TOOL}
                });
            }
            let arguments = params.get("arguments").cloned().unwrap_or(json!({}));
            let result = call_tool(state, bare, &arguments);
            json!({"jsonrpc": "2.0", "id": id, "result": result})
        }
        other => json!({
            "jsonrpc": "2.0",
            "id": id,
            "error": {"code": -32601, "message": format!("method `{other}` is not supported")}
        }),
    }
}

/// Answers `GET /mcp` honestly: this endpoint does not open a server-push stream.
pub async fn get_info() -> Response {
    (
        StatusCode::METHOD_NOT_ALLOWED,
        Json(json!({
            "ok": false,
            "error": "stream_not_supported",
            "hint": "POST JSON-RPC 2.0 to /mcp; this server does not open a server-sent event stream."
        })),
    )
        .into_response()
}

#[cfg(test)]
#[path = "mcp_tests.rs"]
mod tests;
