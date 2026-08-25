//! Comet-owned MCP surface for the primary Orchestrator.
//!
//! This is intentionally separate from Unpeel's worker-to-worker MCP host: only
//! ACP controller sessions receive this process in their `mcpServers` list.

use std::io::{BufRead as _, Write as _};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use serde_json::{Value, json};

use crate::{
    LocalWorkersClient, SessionAction, WorkersLaunchRequest, WorkersSession, WorkersSessionCommand,
};

pub const CONTROLLER_MCP_ARG: &str = "__workers_mcp__";
const CONTROLLER_ENV: &str = "COMET_WORKERS_CONTROLLER";
const PARENT_CHAT_ENV: &str = "COMET_WORKERS_PARENT_CHAT_ID";

fn controller_client() -> &'static LocalWorkersClient {
    static CLIENT: OnceLock<LocalWorkersClient> = OnceLock::new();
    CLIENT.get_or_init(LocalWorkersClient::new)
}

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
    consume_authority_marker()?;
    let parent_chat_id = take_parent_chat_id();
    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout().lock();
    for line in stdin.lock().lines() {
        let line = line.map_err(|error| error.to_string())?;
        if line.trim().is_empty() {
            continue;
        }
        let response = match serde_json::from_str::<Value>(&line) {
            Ok(request) => handle_request_with_parent(request, parent_chat_id.as_deref()),
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

pub fn take_parent_chat_id() -> Option<String> {
    let parent = std::env::var(PARENT_CHAT_ENV)
        .ok()
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty());
    // SAFETY: controller startup consumes this before it launches worker hosts.
    unsafe { std::env::remove_var(PARENT_CHAT_ENV) };
    parent
}

pub fn consume_authority_marker() -> Result<(), String> {
    if std::env::var(CONTROLLER_ENV).ok().as_deref() != Some("1") {
        return Err(format!(
            "{CONTROLLER_MCP_ARG} is reserved for Comet controller sessions"
        ));
    }
    // SAFETY: called before the server launches any threads or child workers.
    // Descendants must never inherit the controller-only startup marker.
    unsafe { std::env::remove_var(CONTROLLER_ENV) };
    Ok(())
}

pub fn handle_request(request: Value) -> Option<Value> {
    handle_request_with_parent(request, None)
}

fn handle_request_with_parent(request: Value, parent_chat_id: Option<&str>) -> Option<Value> {
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
                match dispatch_action(controller_client(), &arguments, parent_chat_id) {
                    Ok(value) => tool_success(id, value),
                    Err(error) => tool_error(id, &error),
                }
            }
        }
        _ => error_response(id, -32601, "Method not found"),
    })
}

pub fn parse_launch(arguments: Value) -> Result<WorkersLaunchRequest, String> {
    parse_launch_briefing(arguments).map(|(launch, _)| launch)
}

pub fn parse_launch_briefing(
    arguments: Value,
) -> Result<(WorkersLaunchRequest, Option<String>), String> {
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
    let briefing = if let Some(text) = optional_string(&arguments, "initial_text") {
        if text.len() > 64 * 1024 {
            return Err("initial_text exceeds 64 KiB".into());
        }
        let sanitized = sanitize_text(&text);
        if sanitized.trim().is_empty() {
            return Err("initial_text is empty after removing control characters".into());
        }
        Some(sanitized)
    } else {
        None
    };
    Ok((request, briefing))
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

pub(crate) fn choose_semantic_output(
    raw: &str,
    screen_rows: Option<Vec<String>>,
    max_bytes: usize,
) -> String {
    if let Some(rows) = screen_rows {
        let screen = rows
            .into_iter()
            .map(|row| row.trim_end().to_owned())
            .collect::<Vec<_>>()
            .join("\n");
        let screen = screen.trim();
        if !screen.is_empty() {
            return truncate_tail(screen, max_bytes);
        }
    }
    project_terminal_fallback(raw, max_bytes)
}

fn project_terminal_fallback(raw: &str, max_bytes: usize) -> String {
    #[derive(Clone, Copy)]
    enum State {
        Text,
        Escape,
        Csi,
        Osc,
        OscEscape,
    }

    let mut state = State::Text;
    let mut lines = Vec::new();
    let mut line = String::new();
    for character in raw.chars() {
        state = match state {
            State::Text => match character {
                '\u{1b}' => State::Escape,
                '\r' => {
                    line.clear();
                    State::Text
                }
                '\n' => {
                    lines.push(std::mem::take(&mut line));
                    State::Text
                }
                '\u{8}' | '\u{7f}' => {
                    line.pop();
                    State::Text
                }
                '\t' => {
                    line.push(' ');
                    State::Text
                }
                value if value.is_control() => State::Text,
                value => {
                    line.push(value);
                    State::Text
                }
            },
            State::Escape => match character {
                '[' => State::Csi,
                ']' => State::Osc,
                _ => State::Text,
            },
            State::Csi => {
                if ('@'..='~').contains(&character) {
                    if matches!(character, 'H' | 'f' | 'G' | 'K') {
                        line.clear();
                    } else if character == 'J' {
                        lines.clear();
                        line.clear();
                    }
                    State::Text
                } else {
                    State::Csi
                }
            }
            State::Osc => match character {
                '\u{7}' => State::Text,
                '\u{1b}' => State::OscEscape,
                _ => State::Osc,
            },
            State::OscEscape => {
                if character == '\\' {
                    State::Text
                } else {
                    State::Osc
                }
            }
        };
    }
    if !line.is_empty() {
        lines.push(line);
    }
    let projected = lines
        .into_iter()
        .map(|line| line.trim_end().to_owned())
        .collect::<Vec<_>>()
        .join("\n");
    truncate_tail(projected.trim(), max_bytes)
}

fn semantic_output(session_id: &str, raw: &str, max_bytes: usize) -> String {
    let rows = unpeel_core::terminal_viewport::read_terminal_viewport_snapshot(
        session_id.to_owned(),
        220,
        120,
        Some(256 * 1024),
        Some(0),
        Some(120),
    )
    .ok()
    .map(|snapshot| {
        snapshot
            .viewport_rows
            .into_iter()
            .map(|row| row.text)
            .collect()
    });
    choose_semantic_output(raw, rows, max_bytes)
}

fn dispatch_action(
    client: &LocalWorkersClient,
    arguments: &Value,
    parent_chat_id: Option<&str>,
) -> Result<Value, String> {
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
            let (request, briefing) = parse_launch_briefing(arguments.clone())?;
            validate_launch_target(client, &request)?;
            // Capture before spawning: a fast-failing CLI can exit before
            // `launch_session` returns its id. The id is unique, so this earlier
            // cutoff cannot adopt history from another worker.
            let registered_at_unix_ms = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64;
            let session_id = client
                .launch_session(&request)
                .map_err(|error| error.to_string())?;
            if let Some(parent_chat_id) = parent_chat_id {
                crate::register_worker_parent(
                    &session_id,
                    parent_chat_id,
                    registered_at_unix_ms,
                )
                .map_err(|error| {
                    format!(
                        "Worker {session_id} was created, but its parent chat binding could not be persisted: {error}"
                    )
                })?;
            }
            // The worker exists from here on: returning Err would orphan a
            // live session the caller never learns the id of. Report the real
            // delivery outcome instead so the caller can retry submission on
            // the worker it now owns.
            let mut briefing_error = None;
            if let Some(briefing) = &briefing {
                let track_episode = tracks_task_episode(parent_chat_id, true);
                if let Err(error) =
                    submit_initial_briefing(client, &session_id, briefing, track_episode)
                {
                    briefing_error = Some(error);
                }
            }
            let mut response = json!({
                "session_id": session_id,
                "launched": true,
                "briefing_submitted": briefing.is_some() && briefing_error.is_none()
            });
            if let Some(error) = briefing_error {
                response["briefing_error"] = error.into();
            }
            Ok(response)
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
                .map(|output| {
                    semantic_output(
                        &session.id,
                        &String::from_utf8_lossy(&output.data),
                        16 * 1024,
                    )
                })
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
                "text": semantic_output(
                    &session.id,
                    &String::from_utf8_lossy(&output.data),
                    64 * 1024
                )
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
            let sanitized = sanitize_text(&text);
            if sanitized.trim().is_empty() {
                return Err("text is empty after removing control characters".into());
            }
            let track_episode = tracks_task_episode(parent_chat_id, submit);
            let baseline = track_episode
                .then(|| capture_process_baseline(&session.id))
                .transpose()?;
            let submitted_at_unix_ms = unix_time_ms();
            let episode = baseline
                .map(|baseline| {
                    crate::prepare_worker_parent_task(&session.id, submitted_at_unix_ms, baseline)
                })
                .transpose()?;
            let delivery = match episode {
                Some(episode) => {
                    unpeel_core::session_input::deliver_sanitized_text_with_task_receipt(
                        &session.id,
                        &sanitized,
                        episode,
                    )
                }
                None => unpeel_core::session_input::deliver_sanitized_text(
                    &session.id,
                    &sanitized,
                    submit,
                ),
            };
            if let Err(error) = delivery {
                if let Some(episode) = episode {
                    return Err(cancelled_submission_error(&session.id, episode, &error));
                }
                return Err(error);
            }
            if let Some(episode) = episode {
                crate::confirm_worker_parent_task_submission(&session.id, episode)?;
            }
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
            archive_guard(&session)?;
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

fn unix_time_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

fn capture_process_baseline(session_id: &str) -> Result<Vec<(u32, u64)>, String> {
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        match crate::resources::current_session_process_identities(session_id) {
            Ok(identities) if !identities.is_empty() => return Ok(identities),
            Ok(_) | Err(_) if Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(50));
            }
            Ok(_) => return Err("session process tree is empty".into()),
            Err(error) => return Err(error),
        }
    }
}

/// How long a just-created worker gets to publish its manifest.
const MANIFEST_WAIT: Duration = Duration::from_secs(5);
/// How long the agent prompt gets to become ready once the manifest exists.
const BRIEFING_READY_WAIT: Duration = Duration::from_secs(8);

/// Poll until the session host has published this worker's manifest, and
/// return the runtime that owns its prompt.
fn wait_for_session_runtime(session_id: &str, wait: Duration) -> Result<String, String> {
    let deadline = Instant::now() + wait;
    loop {
        if let Some(manifest) = unpeel_core::session_host::load_manifest(session_id) {
            return Ok(
                unpeel_core::integrations::command_head(&manifest.session.command).to_owned(),
            );
        }
        if Instant::now() >= deadline {
            return Err(format!("worker {session_id} manifest is unavailable"));
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

fn submit_initial_briefing(
    client: &LocalWorkersClient,
    session_id: &str,
    briefing: &str,
    capture_baseline: bool,
) -> Result<(), String> {
    // The session host publishes the manifest asynchronously, so a launch that
    // just returned an id may still have nothing on disk. Wait for it, then
    // start the readiness deadline: otherwise the whole budget can burn before
    // the runtime even exists and the brief is silently never delivered.
    let runtime = wait_for_session_runtime(session_id, MANIFEST_WAIT)?;
    let deadline = Instant::now() + BRIEFING_READY_WAIT;
    let mut last_screen = String::new();
    let mut stable_since = Instant::now();
    loop {
        if let Some(screen) = current_screen_text(session_id) {
            if screen != last_screen {
                last_screen = screen.clone();
                stable_since = Instant::now();
            }
            if let Some(response) = startup_prompt_response(&screen) {
                client
                    .write(session_id, &response)
                    .map_err(|error| error.to_string())?;
                last_screen.clear();
                stable_since = Instant::now();
                std::thread::sleep(Duration::from_millis(200));
                continue;
            }
            if !is_briefing_screen_ready(
                &runtime,
                &screen,
                stable_since.elapsed().as_millis() as u64,
            ) {
                if Instant::now() >= deadline {
                    return Err("agent prompt did not become ready before timeout".into());
                }
                std::thread::sleep(Duration::from_millis(100));
                continue;
            }
        } else if Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(100));
            continue;
        } else {
            return Err("agent screen was unavailable before timeout".into());
        }
        let baseline = match capture_baseline.then(|| capture_process_baseline(session_id)) {
            None => Vec::new(),
            Some(Ok(baseline)) => baseline,
            Some(Err(_)) if Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(100));
                continue;
            }
            Some(Err(error)) => return Err(error),
        };
        let submitted_at_unix_ms = unix_time_ms();
        let episode = capture_baseline
            .then(|| crate::prepare_worker_parent_task(session_id, submitted_at_unix_ms, baseline))
            .transpose()?;
        loop {
            let delivery = match episode {
                Some(episode) => {
                    unpeel_core::session_input::deliver_sanitized_text_with_task_receipt(
                        session_id, briefing, episode,
                    )
                }
                None => {
                    unpeel_core::session_input::deliver_sanitized_text(session_id, briefing, true)
                }
            };
            match delivery {
                Ok(()) => {
                    if let Some(episode) = episode {
                        crate::confirm_worker_parent_task_submission(session_id, episode)?;
                    }
                    return Ok(());
                }
                Err(_) if Instant::now() < deadline => {
                    std::thread::sleep(Duration::from_millis(100));
                }
                Err(error) => {
                    if let Some(episode) = episode {
                        return Err(cancelled_submission_error(session_id, episode, &error));
                    }
                    return Err(error);
                }
            }
        }
    }
}

fn cancelled_submission_error(session_id: &str, episode: u64, delivery_error: &str) -> String {
    match crate::cancel_worker_parent_task(session_id, episode) {
        Ok(()) => delivery_error.to_owned(),
        Err(cancel_error) => format!(
            "{delivery_error}; additionally failed to cancel task episode {episode}: {cancel_error}"
        ),
    }
}

pub fn tracks_task_episode(parent_chat_id: Option<&str>, submit: bool) -> bool {
    submit && parent_chat_id.is_some_and(|parent| !parent.trim().is_empty())
}

pub fn is_briefing_screen_ready(runtime: &str, screen: &str, stable_for_ms: u64) -> bool {
    let lower = screen.to_ascii_lowercase();
    let numbered_menu = lower.contains("press enter")
        && lower
            .lines()
            .any(|line| line.trim_start().starts_with("1."))
        && lower
            .lines()
            .any(|line| line.trim_start().starts_with("2."));
    let prompt_glyph = screen.contains('❯')
        || screen.contains('›')
        || screen.lines().any(|line| matches!(line.trim(), ">" | "> "));
    let prompt_visible = match runtime.trim().to_ascii_lowercase().as_str() {
        "claude" => lower.contains("claude code") && screen.contains('❯'),
        "codex" => screen.contains('›'),
        "kimi" | "kimi-code" => lower.contains("kimi code cli") && lower.contains("input"),
        "pi" => lower.contains("pi v") && lower.contains("escape interrupt"),
        "omp" | "omp-cli" => lower.contains("omp v") && lower.contains("tips"),
        "prime-agent" => {
            lower.contains("version") && lower.contains("model") && lower.contains("cwd")
        }
        "opencode" => lower.contains("opencode") && (prompt_glyph || lower.contains("input")),
        "gemini" => lower.contains("gemini") && prompt_glyph,
        "grok" => lower.contains("grok") && prompt_glyph,
        "cursor-agent" => lower.contains("cursor") && prompt_glyph,
        "kiro-cli" => lower.contains("kiro") && prompt_glyph,
        "copilot" => lower.contains("copilot") && prompt_glyph,
        "cline" => lower.contains("cline") && prompt_glyph,
        "amp" => lower.contains("amp") && prompt_glyph,
        "muse" => lower.contains("muse") && prompt_glyph,
        _ => false,
    };
    stable_for_ms >= 300
        && !screen.trim().is_empty()
        && !numbered_menu
        && !unpeel_core::menu_prompt::viewport_has_menu_prompt(screen)
        && prompt_visible
}

fn current_screen_text(session_id: &str) -> Option<String> {
    unpeel_core::terminal_viewport::read_terminal_viewport_snapshot(
        session_id.to_owned(),
        160,
        80,
        Some(128 * 1024),
        Some(0),
        Some(80),
    )
    .ok()
    .map(|snapshot| {
        snapshot
            .viewport_rows
            .into_iter()
            .map(|row| row.text)
            .collect::<Vec<_>>()
            .join("\n")
    })
}

pub fn startup_prompt_response(screen: &str) -> Option<String> {
    let normalized = screen.to_ascii_lowercase();
    if normalized.contains("update available")
        && normalized.contains("skip")
        && normalized.contains("press enter")
    {
        return Some("2\r".into());
    }
    if (normalized.contains("quick safety check") || normalized.contains("do you trust"))
        && normalized.contains("trust this folder")
    {
        return Some("1\r".into());
    }
    None
}

pub fn sanitize_text(text: &str) -> String {
    unpeel_core::session_input::sanitize_paste_text(text)
}

pub fn archive_guard(session: &WorkersSession) -> Result<(), String> {
    if session.is_live() {
        return Err(format!(
            "Worker '{}' is live. Call stop_worker explicitly, wait for state=exited, then archive_worker.",
            session.id
        ));
    }
    Ok(())
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
