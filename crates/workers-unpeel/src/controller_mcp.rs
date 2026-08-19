//! Comet-owned MCP surface for the primary Orchestrator.
//!
//! This is intentionally separate from Unpeel's worker-to-worker MCP host: only
//! ACP controller sessions receive this process in their `mcpServers` list.

use std::io::{BufRead as _, Write as _};
use std::time::{Duration, Instant};

use serde_json::{Value, json};

use crate::{
    InitialTextSubmitMode, LocalWorkersClient, SessionAction, WorkersLaunchRequest, WorkersSession,
    WorkersSessionCommand,
};

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
                let arguments = request
                    .pointer("/params/arguments")
                    .cloned()
                    .unwrap_or_else(|| json!({}));
                match dispatch_action(&LocalWorkersClient::new(), &arguments) {
                    Ok(value) => tool_success(id, value),
                    Err(error) => tool_error(id, &error),
                }
            }
        }
        _ => error_response(id, -32601, "Method not found"),
    })
}

pub fn parse_launch(arguments: Value) -> Result<WorkersLaunchRequest, String> {
    let project_id = required_string(&arguments, "project_id")?;
    let preset_id = optional_string(&arguments, "preset_id");
    let command = optional_string(&arguments, "command");
    let mut request = match (preset_id, command) {
        (Some(preset_id), None) => WorkersLaunchRequest::preset(project_id, preset_id),
        (None, Some(command)) if !command.trim().is_empty() => {
            WorkersLaunchRequest::command(project_id, command)
        }
        _ => {
            return Err("launch_worker requires exactly one non-empty preset_id or command".into());
        }
    };
    match (
        optional_string(&arguments, "worktree_path"),
        optional_string(&arguments, "worktree_branch"),
    ) {
        (Some(path), Some(branch)) => request = request.with_worktree(path, branch),
        (None, None) => {}
        _ => return Err("worktree_path and worktree_branch must be provided together".into()),
    }
    if let Some(text) = optional_string(&arguments, "initial_text") {
        if text.len() > 64 * 1024 {
            return Err("initial_text exceeds 64 KiB".into());
        }
        request = request.with_initial_text(text, InitialTextSubmitMode::PasteAndSubmit);
    }
    Ok(request)
}

pub fn encode_keys(keys: &[String]) -> Result<String, String> {
    if keys.len() > 64 {
        return Err("send_keys accepts at most 64 entries".into());
    }
    let mut encoded = String::new();
    for key in keys {
        match key.as_str() {
            "enter" | "return" => encoded.push('\r'),
            "escape" | "esc" => encoded.push('\u{1b}'),
            "tab" => encoded.push('\t'),
            "backspace" => encoded.push('\u{7f}'),
            "up" => encoded.push_str("\u{1b}[A"),
            "down" => encoded.push_str("\u{1b}[B"),
            "right" => encoded.push_str("\u{1b}[C"),
            "left" => encoded.push_str("\u{1b}[D"),
            "ctrl-c" => encoded.push('\u{3}'),
            value if value.starts_with("text:") => {
                let value = value.trim_start_matches("text:");
                if value.len() > 16 * 1024 {
                    return Err("one text: key entry exceeds 16 KiB".into());
                }
                encoded.push_str(value);
            }
            value if value.chars().count() == 1 => encoded.push_str(value),
            _ => {
                return Err(format!(
                    "Unknown key '{key}'. Use action=help for valid keys."
                ));
            }
        }
    }
    Ok(encoded)
}

pub fn clean_output(text: &str, max_bytes: usize) -> String {
    #[derive(Clone, Copy)]
    enum State {
        Text,
        Escape,
        Csi,
        Osc,
        OscEscape,
    }
    let mut state = State::Text;
    let mut clean = String::with_capacity(text.len().min(max_bytes));
    for character in text.chars() {
        state = match state {
            State::Text if character == '\u{1b}' => State::Escape,
            State::Text => {
                clean.push(character);
                State::Text
            }
            State::Escape if character == '[' => State::Csi,
            State::Escape if character == ']' => State::Osc,
            State::Escape => State::Text,
            State::Csi if ('@'..='~').contains(&character) => State::Text,
            State::Csi => State::Csi,
            State::Osc if character == '\u{7}' => State::Text,
            State::Osc if character == '\u{1b}' => State::OscEscape,
            State::Osc => State::Osc,
            State::OscEscape if character == '\\' => State::Text,
            State::OscEscape => State::Osc,
        };
    }
    truncate_tail(&clean, max_bytes)
}

fn dispatch_action(client: &LocalWorkersClient, arguments: &Value) -> Result<Value, String> {
    let action = required_string(arguments, "action")?;
    match action.as_str() {
        "help" => Ok(json!({
            "actions": ACTIONS,
            "workflow": "list_projects -> list_presets -> launch_worker -> wait_for_status/read_output -> stop_worker/archive_worker",
            "keys": ["enter", "escape", "tab", "backspace", "up", "down", "left", "right", "ctrl-c", "text:<literal>"],
            "limits": { "wait_seconds": 120, "keys": 64, "output_bytes": 65536, "transcript_bytes": 98304 }
        })),
        "list_projects" => {
            let bootstrap = client.bootstrap().map_err(|error| error.to_string())?;
            Ok(json!({
                "projects": bootstrap.projects.into_iter().map(|project| json!({
                    "id": project.id,
                    "name": project.name,
                    "path": project.path,
                    "is_group": project.is_group,
                    "worktree_branch": project.worktree_branch,
                    "git_branch": project.git_branch
                })).collect::<Vec<_>>()
            }))
        }
        "list_presets" => {
            let bootstrap = client.bootstrap().map_err(|error| error.to_string())?;
            Ok(json!({
                "presets": bootstrap.presets.into_iter().filter(|preset| preset.enabled).map(|preset| json!({
                    "id": preset.id,
                    "label": preset.label,
                    "command": preset.command,
                    "cli_id": preset.cli_id,
                    "is_default": preset.is_default
                })).collect::<Vec<_>>()
            }))
        }
        "launch_worker" => {
            let request = parse_launch(arguments.clone())?;
            validate_launch_target(client, &request)?;
            let session_id = client
                .launch_session(&request)
                .map_err(|error| error.to_string())?;
            Ok(json!({ "session_id": session_id, "launched": true }))
        }
        "list_workers" => {
            let bootstrap = client.bootstrap().map_err(|error| error.to_string())?;
            Ok(json!({
                "workers": bootstrap.sessions.iter().map(session_json).collect::<Vec<_>>()
            }))
        }
        "inspect_worker" => {
            let session = find_session(client, arguments)?;
            let output = client
                .read_output(&session.id, None, 0)
                .map(|output| clean_output(&String::from_utf8_lossy(&output.data), 16 * 1024))
                .unwrap_or_default();
            Ok(json!({ "worker": session_json(&session), "output_tail": output }))
        }
        "read_output" => {
            let session = find_session(client, arguments)?;
            let output = client
                .read_output(&session.id, None, 0)
                .map_err(|error| error.to_string())?;
            Ok(json!({
                "session_id": session.id,
                "offset": output.offset,
                "next_offset": output.next_offset,
                "truncated_upstream": output.truncated,
                "text": clean_output(&String::from_utf8_lossy(&output.data), 64 * 1024)
            }))
        }
        "read_transcript" => {
            let session = find_session(client, arguments)?;
            let entries = arguments
                .get("entries")
                .and_then(Value::as_u64)
                .unwrap_or(50)
                .clamp(1, 500) as usize;
            let transcript = client
                .transcript_markdown(&session.id, Some(entries))
                .map_err(|error| error.to_string())?;
            Ok(json!({
                "session_id": session.id,
                "entries": entries,
                "markdown": truncate_tail(&transcript, 96 * 1024)
            }))
        }
        "send_text" => {
            let session = find_session(client, arguments)?;
            let text = required_string(arguments, "text")?;
            if text.len() > 64 * 1024 {
                return Err("text exceeds 64 KiB".into());
            }
            let submit = arguments
                .get("submit")
                .and_then(Value::as_bool)
                .unwrap_or(true);
            let mut data = format!("\u{1b}[200~{text}\u{1b}[201~");
            if submit {
                data.push('\r');
            }
            client
                .write(&session.id, &data)
                .map_err(|error| error.to_string())?;
            Ok(json!({ "session_id": session.id, "sent": true, "submitted": submit }))
        }
        "send_keys" => {
            let session = find_session(client, arguments)?;
            let keys = arguments
                .get("keys")
                .and_then(Value::as_array)
                .ok_or_else(|| "send_keys requires keys[]".to_owned())?
                .iter()
                .map(|value| {
                    value
                        .as_str()
                        .map(str::to_owned)
                        .ok_or_else(|| "every key must be a string".to_owned())
                })
                .collect::<Result<Vec<_>, _>>()?;
            let data = encode_keys(&keys)?;
            client
                .write(&session.id, &data)
                .map_err(|error| error.to_string())?;
            Ok(json!({ "session_id": session.id, "sent": true, "key_count": keys.len() }))
        }
        "wait_for_status" => wait_for_status(client, arguments),
        "stop_worker" => {
            let session = find_session(client, arguments)?;
            client
                .session_action(&session.id, SessionAction::Stop)
                .map_err(|error| error.to_string())?;
            Ok(json!({ "session_id": session.id, "stopped": true }))
        }
        "archive_worker" => {
            let session = find_session(client, arguments)?;
            client
                .session_command(&session, WorkersSessionCommand::Archive)
                .map_err(|error| error.to_string())?;
            Ok(json!({ "session_id": session.id, "archived": true }))
        }
        _ => Err(format!(
            "Unknown workers action '{action}'. Use action=help."
        )),
    }
}

fn validate_launch_target(
    client: &LocalWorkersClient,
    request: &WorkersLaunchRequest,
) -> Result<(), String> {
    let bootstrap = client.bootstrap().map_err(|error| error.to_string())?;
    if !bootstrap
        .projects
        .iter()
        .any(|project| project.id == request.project_id && !project.is_group)
    {
        return Err(format!(
            "Unknown runnable project '{}'.",
            request.project_id
        ));
    }
    if let Some(preset_id) = request.preset_id.as_deref()
        && !bootstrap
            .presets
            .iter()
            .any(|preset| preset.id == preset_id && preset.enabled)
    {
        return Err(format!("Unknown or disabled preset '{preset_id}'."));
    }
    Ok(())
}

fn wait_for_status(client: &LocalWorkersClient, arguments: &Value) -> Result<Value, String> {
    let session_id = required_string(arguments, "session_id")?;
    let wanted = required_string(arguments, "status")?.to_ascii_lowercase();
    let timeout = arguments
        .get("timeout_seconds")
        .and_then(Value::as_u64)
        .unwrap_or(30)
        .clamp(1, 120);
    let deadline = Instant::now() + Duration::from_secs(timeout);
    loop {
        let bootstrap = client.bootstrap().map_err(|error| error.to_string())?;
        let Some(session) = bootstrap
            .sessions
            .into_iter()
            .find(|session| session.id == session_id)
        else {
            return Err(format!("Worker '{session_id}' no longer exists."));
        };
        let matched = session.activity.eq_ignore_ascii_case(&wanted)
            || session.state.eq_ignore_ascii_case(&wanted);
        if matched {
            return Ok(json!({ "matched": true, "worker": session_json(&session) }));
        }
        if Instant::now() >= deadline {
            return Ok(json!({
                "matched": false,
                "timed_out": true,
                "wanted": wanted,
                "worker": session_json(&session)
            }));
        }
        std::thread::sleep(Duration::from_millis(250));
    }
}

fn find_session(client: &LocalWorkersClient, arguments: &Value) -> Result<WorkersSession, String> {
    let session_id = required_string(arguments, "session_id")?;
    client
        .bootstrap()
        .map_err(|error| error.to_string())?
        .sessions
        .into_iter()
        .find(|session| session.id == session_id)
        .ok_or_else(|| format!("Unknown worker '{session_id}'. Use action=list_workers."))
}

fn session_json(session: &WorkersSession) -> Value {
    json!({
        "id": session.id,
        "project_id": session.project_id,
        "title": session.title,
        "command": session.command,
        "state": session.state,
        "activity": session.activity,
        "unread": session.unread,
        "archived": session.archived,
        "provider_id": session.provider_id,
        "active_runtime_id": session.active_runtime_id,
        "worktree_branch": session.worktree_branch,
        "updated_at_unix_ms": session.updated_at_unix_ms
    })
}

fn required_string(value: &Value, key: &str) -> Result<String, String> {
    optional_string(value, key)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| format!("'{key}' is required"))
}

fn optional_string(value: &Value, key: &str) -> Option<String> {
    value.get(key).and_then(Value::as_str).map(str::to_owned)
}

fn truncate_tail(text: &str, max_bytes: usize) -> String {
    if text.len() <= max_bytes {
        return text.to_owned();
    }
    let mut start = text
        .len()
        .saturating_sub(max_bytes.saturating_sub('…'.len_utf8()));
    while start < text.len() && !text.is_char_boundary(start) {
        start += 1;
    }
    format!("…{}", &text[start..])
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
                "entries": { "type": "integer", "minimum": 1, "maximum": 500 },
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

fn tool_success(id: Value, value: Value) -> Value {
    let text = serde_json::to_string_pretty(&value).unwrap_or_else(|_| value.to_string());
    result_response(
        id,
        json!({
            "content": [{ "type": "text", "text": text }],
            "structuredContent": value,
            "isError": false
        }),
    )
}
