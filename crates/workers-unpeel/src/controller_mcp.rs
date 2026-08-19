//! Comet-owned MCP surface for the primary Orchestrator.
//!
//! This is intentionally separate from Unpeel's worker-to-worker MCP host: only
//! ACP controller sessions receive this process in their `mcpServers` list.

use std::io::{BufRead as _, Write as _};

use serde_json::{Value, json};

pub const CONTROLLER_MCP_ARG: &str = "__workers_mcp__";
const CONTROLLER_ENV: &str = "COMET_WORKERS_CONTROLLER";

const ACTIONS: &[&str] = &[
    "help",
    "list_projects",
    "list_presets",
    "launch_worker",
    "list_workers",
    "inspect_worker",
    "read_output",
    "read_transcript",
    "send_text",
    "send_keys",
    "wait_for_status",
    "stop_worker",
    "archive_worker",
];

pub fn run_stdio() -> Result<(), String> {
    if std::env::var(CONTROLLER_ENV).ok().as_deref() != Some("1") {
        return Err(format!(
            "{CONTROLLER_MCP_ARG} is reserved for Comet controller sessions"
        ));
    }
    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout().lock();
    for line in stdin.lock().lines() {
        let line = line.map_err(|error| error.to_string())?;
        if line.trim().is_empty() {
            continue;
        }
        let response = match serde_json::from_str::<Value>(&line) {
            Ok(request) => handle_request(request),
            Err(_) => Some(error_response(Value::Null, -32700, "Parse error")),
        };
        if let Some(response) = response {
            serde_json::to_writer(&mut stdout, &response).map_err(|error| error.to_string())?;
            stdout.write_all(b"\n").map_err(|error| error.to_string())?;
            stdout.flush().map_err(|error| error.to_string())?;
        }
    }
    Ok(())
}

pub fn handle_request(request: Value) -> Option<Value> {
    let id = request.get("id").cloned();
    let method = request.get("method").and_then(Value::as_str);
    if request.get("jsonrpc").and_then(Value::as_str) != Some("2.0") || method.is_none() {
        return id.map(|id| error_response(id, -32600, "Invalid Request"));
    }
    let method = method.expect("checked above");
    if id.is_none() {
        return None;
    }
    let id = id.expect("checked above");
    Some(match method {
        "initialize" => {
            let protocol_version = request
                .pointer("/params/protocolVersion")
                .cloned()
                .unwrap_or_else(|| json!("2024-11-05"));
            result_response(
                id,
                json!({
                    "protocolVersion": protocol_version,
                    "capabilities": { "tools": { "listChanged": false } },
                    "serverInfo": { "name": "comet-workers", "version": env!("CARGO_PKG_VERSION") }
                }),
            )
        }
        "ping" => result_response(id, json!({})),
        "tools/list" => result_response(id, json!({ "tools": [tool_definition()] })),
        "tools/call" => {
            let name = request.pointer("/params/name").and_then(Value::as_str);
            if name != Some("workers") {
                tool_error(id, "Unknown tool. Use 'workers'.")
            } else {
                tool_error(id, "Workers action is unavailable in this build.")
            }
        }
        _ => error_response(id, -32601, "Method not found"),
    })
}

fn tool_definition() -> Value {
    json!({
        "name": "workers",
        "description": "Launch and coordinate Comet CLI Workers. Start with action=help or list_projects.",
        "inputSchema": {
            "type": "object",
            "required": ["action"],
            "properties": {
                "action": { "type": "string", "enum": ACTIONS },
                "project_id": { "type": "string" },
                "preset_id": { "type": "string" },
                "command": { "type": "string" },
                "session_id": { "type": "string" },
                "text": { "type": "string" },
                "keys": { "type": "array", "items": { "type": "string" }, "maxItems": 64 },
                "submit": { "type": "boolean" },
                "status": { "type": "string" },
                "timeout_seconds": { "type": "integer", "minimum": 1, "maximum": 120 },
                "initial_text": { "type": "string" },
                "worktree_path": { "type": "string" },
                "worktree_branch": { "type": "string" }
            },
            "additionalProperties": false
        }
    })
}

fn result_response(id: Value, result: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "result": result })
}

fn error_response(id: Value, code: i64, message: &str) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } })
}

fn tool_error(id: Value, message: &str) -> Value {
    result_response(
        id,
        json!({
            "content": [{ "type": "text", "text": message }],
            "isError": true
        }),
    )
}
