//! Unpeel Sessions MCP: `unpeel-host __mcp__` speaks MCP (JSON-RPC 2.0 over stdio)
//! and lets an agent session inspect and control sibling Unpeel sessions.
//!
//! The process is spawned by provider CLIs (Claude via `--mcp-config`, Codex via
//! the wrapper's `-c mcp_servers...` overrides) and inherits the session env, so
//! `UNPEEL_SESSION_ID` identifies the calling session without any handshake.
//! Session control goes directly through the per-session control socket
//! (`~/.unpeel/app-sessions/<id>/session.sock`); no running desktop app is
//! required beyond the hosts themselves.

use crate::session_host::{self, HostedSessionManifest, HostedSessionState, SessionHostCommand};
use crate::session_input::sanitize_paste_text;
#[cfg(test)]
use crate::session_input::{encode_bracketed_paste, looks_like_it_contains_a_path};
use crate::state::{current_timestamp_ms, McpGrant, McpRole, McpScope};
use crate::transcripts::{
    format_transcript_markdown, load_transcript_settings, provider_label_for_command,
    read_transcript_snapshot, resolve_provider_transcript, transcript_provider_for_command,
    transcript_status_hint, TranscriptEntry,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::io::{BufRead, Write};
use std::thread;
use std::time::{Duration, Instant};

pub const MCP_HOST_ARG: &str = "__mcp__";

/// Registration-scoped upper bound on the domains this MCP process may
/// advertise or call. The Session manifest remains the normal grant source;
/// persistent environment-gated registrations use this additional mask so a
/// runtime cannot inherit a domain its config never registered.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct McpDomainMask {
    pub sessions: bool,
    pub browser: bool,
    pub computer: bool,
}

impl McpDomainMask {
    pub const ALL: Self = Self {
        sessions: true,
        browser: true,
        computer: true,
    };

    fn allows_tool(self, name: &str) -> bool {
        match name {
            SESSIONS_TOOL => self.sessions,
            BROWSER_TOOL => self.browser,
            COMPUTER_TOOL => self.computer,
            name if name.strip_prefix("browser_").is_some_and(is_browser_action) => self.browser,
            "start_session" | "delegate_task" | "delegate_batch" => self.sessions,
            name if legacy_sessions_action(name).is_some() => self.sessions,
            _ => true,
        }
    }
}

const PROTOCOL_VERSION_FALLBACK: &str = "2025-06-18";
// The unified Unpeel MCP server: one tool per capability domain (`sessions`,
// `browser`, later `computer`/`device`), each taking an `action` parameter,
// instead of one server per domain with a dozen tools each. Keeps the
// per-request context cost flat as domains are added; full per-action docs
// load lazily through `action: "help"`.
// Renamed from `unpeel-mcp` 2026-07-25; the old name survives only as pruned
// legacy config entries and the pre-rename config-file names (kept stable so
// restart commands recorded by older sessions keep resolving).
const SERVER_NAME: &str = "unpeel";
const KEY_DELAY_DEFAULT_MS: u64 = 60;
const KEY_DELAY_MAX_MS: u64 = 1_000;
const MAX_KEYS_PER_CALL: usize = 40;
const START_MESSAGE_TIMEOUT_MS: u64 = 20_000;
const START_MESSAGE_POLL_MS: u64 = 100;
const READ_OUTPUT_DEFAULT_TAIL_BYTES: usize = 16 * 1024;
const READ_OUTPUT_MAX_TAIL_BYTES: usize = 256 * 1024;
const READ_SCREEN_MAX_ROWS: u16 = 500;
const READ_TRANSCRIPT_DEFAULT_ENTRIES: usize = 5;
const READ_TRANSCRIPT_MAX_ENTRIES: usize = 100;
const INSPECT_SCREEN_ROWS: u16 = 12;
const INSPECT_TRANSCRIPT_ENTRIES: usize = 4;
const INSPECT_LINE_MAX_CHARS: usize = 240;
const WAIT_DEFAULT_TIMEOUT_MS: u64 = 30_000;
const WAIT_MIN_TIMEOUT_MS: u64 = 1_000;
const WAIT_MAX_TIMEOUT_MS: u64 = 120_000;
const WAIT_POLL_INTERVAL_MS: u64 = 250;
/// How much of the final screen a wait_for_text timeout reports back, so the
/// caller can see what the session was actually showing.
const WAIT_TIMEOUT_REPORT_LINES: usize = 12;
const DELEGATE_SUMMARY_DEFAULT_ENTRIES: usize = 5;
const DELEGATE_SUMMARY_MAX_ENTRIES: usize = 20;
const DELEGATE_SCREEN_FALLBACK_ROWS: u16 = 10;
#[derive(Debug, Clone, Default, Deserialize)]
struct ActivityStateFile {
    #[serde(default)]
    sessions: HashMap<String, ActivityStateEntry>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub(crate) struct ActivityStateEntry {
    #[serde(default)]
    activity_status: Option<String>,
    #[serde(default)]
    raw_status: Option<String>,
    #[serde(default)]
    unread: bool,
    #[serde(default)]
    completed: bool,
}

pub fn run_stdio() -> Result<(), String> {
    run_stdio_with_domains(McpDomainMask::ALL)
}

pub fn run_stdio_with_domains(domains: McpDomainMask) -> Result<(), String> {
    trace(&format!(
        "start self={} pid={}",
        self_session_id().unwrap_or_else(|| "-".into()),
        std::process::id()
    ));

    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    for line in stdin.lock().lines() {
        let line = line.map_err(|e| format!("Failed to read MCP stdin: {e}"))?;
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let Ok(message) = serde_json::from_str::<Value>(trimmed) else {
            // Without a parseable id there is no valid JSON-RPC error to send.
            trace("dropped unparseable message");
            continue;
        };
        if let Some(response) = handle_message_with_domains(&message, domains) {
            let body = serde_json::to_string(&response)
                .map_err(|e| format!("Failed to encode MCP response: {e}"))?;
            let mut out = stdout.lock();
            out.write_all(body.as_bytes())
                .and_then(|_| out.write_all(b"\n"))
                .and_then(|_| out.flush())
                .map_err(|e| format!("Failed to write MCP response: {e}"))?;
        }
    }
    trace("stdin closed, exiting");
    Ok(())
}

#[cfg(test)]
fn handle_message(message: &Value) -> Option<Value> {
    handle_message_with_domains(message, McpDomainMask::ALL)
}

fn handle_message_with_domains(message: &Value, domains: McpDomainMask) -> Option<Value> {
    let method = message.get("method").and_then(Value::as_str)?;
    let id = match message.get("id") {
        Some(id) if !id.is_null() => id.clone(),
        // Notifications (initialized, cancelled, ...) need no reply.
        _ => return None,
    };
    let params = message.get("params").cloned().unwrap_or(Value::Null);

    let outcome = match method {
        "initialize" => Ok(initialize_result(&params)),
        "ping" => Ok(json!({})),
        "tools/list" => Ok(json!({ "tools": tool_definitions_with_domains(domains) })),
        "tools/call" => tools_call_with_domains(&params, domains),
        _ => Err(json!({
            "code": -32601,
            "message": format!("Method not found: {method}"),
        })),
    };

    Some(match outcome {
        Ok(result) => json!({ "jsonrpc": "2.0", "id": id, "result": result }),
        Err(error) => json!({ "jsonrpc": "2.0", "id": id, "error": error }),
    })
}

fn initialize_result(params: &Value) -> Value {
    let protocol_version = params
        .get("protocolVersion")
        .and_then(Value::as_str)
        .unwrap_or(PROTOCOL_VERSION_FALLBACK);
    json!({
        "protocolVersion": protocol_version,
        "capabilities": { "tools": {} },
        "serverInfo": {
            "name": SERVER_NAME,
            "version": env!("CARGO_PKG_VERSION"),
        },
        "instructions": "Unpeel capabilities for this session, one tool per domain; every tool \
    takes an 'action' plus parameters, and {\"action\":\"help\"} returns full per-action docs \
    (add help_for for one action). \
    'sessions' inspects and controls the user's other Unpeel terminal sessions. Reading is \
    open: any session can read any other. Writing (send_text/send_keys) is free between \
    sessions in the same sidebar group; writing to another group asks the user for approval \
    first — the call blocks (up to ~2 minutes) on a \
    desktop dialog, and an approved pair is remembered. Do not retry in a loop if declined. \
    You can never write into or close your own session, and can close only another session in \
    your group. Agents never create sessions; the user groups sessions from the sidebar. \
    Preferred flow: action 'list' to pick \
    a target, 'inspect' for a compact look, then small reads; after send_text/send_keys, to \
    wait for the agent to finish its turn use wait_for_status with status 'idle' (this also \
    matches 'done' — same settled state, for any provider), or wait_for_text for a specific \
    expected output. Group peers are flagged with relation_to_caller=group; coordinate them \
    with list_group/wait_for_group/summarize_group, and send a structured update to a chosen \
    peer with report_to_group. \
    'browser' (when present) operates a real browser isolated to this session (own profile and \
    window, closed with the session). Core loop: open a URL, snapshot for element refs like \
    @e1, act by ref (click/fill), re-snapshot after navigation or DOM changes — refs go stale. \
    Prefer refs over CSS selectors. Use wait after actions that trigger loads; screenshot saves \
    into this session's artifact folder and returns the file path; check console when a page \
    misbehaves; call {\"action\":\"context\"} if browser tools seem unavailable. Do not paste \
    cookies, tokens, passwords, or downloaded private files into the conversation unless the \
    user explicitly asks. \
    'computer' (when present) controls this Mac's real apps — the user's desktop, not a \
    sandbox. The first action may block on a one-time user approval; if declined, do not \
    retry. Loop: 'launch' an app for its pid + windows, 'see' a window for its element tree \
    [N] + screenshot, act by element_index ('click'/'type'/'set_value'), then re-'see' to \
    verify (indices go stale on every see; an unchanged tree means the action likely \
    no-oped). Control is background: it never moves the user's cursor or steals focus; \
    desktop-wide capture/input needs an explicit 'escalate'. Screenshots save as session \
    artifacts and return file paths. The screen can show sensitive user content — never \
    quote secrets you see into the conversation.",
    })
}

fn tools_call_with_domains(params: &Value, domains: McpDomainMask) -> Result<Value, Value> {
    let name = params.get("name").and_then(Value::as_str).ok_or(json!({
        "code": -32602,
        "message": "tools/call requires a string 'name'",
    }))?;
    let arguments = params.get("arguments").cloned().unwrap_or(json!({}));

    // Gating happens per domain inside run_tool: sessions and browser have
    // different access models, and help/context actions must stay reachable
    // so an agent can discover *why* a domain is refusing.
    let outcome = if domains.allows_tool(name) {
        run_tool(name, &arguments)
    } else {
        Err(format!(
            "The '{name}' tool is not enabled for this MCP registration."
        ))
    };

    match outcome {
        Ok(text) => {
            trace(&format!("tool={name} ok"));
            Ok(json!({
                "content": [{ "type": "text", "text": text }],
                "isError": false,
            }))
        }
        Err(error) => {
            trace(&format!("tool={name} error={error}"));
            Ok(json!({
                "content": [{ "type": "text", "text": error }],
                "isError": true,
            }))
        }
    }
}

fn run_tool(name: &str, arguments: &Value) -> Result<String, String> {
    match name {
        SESSIONS_TOOL => {
            let action = required_action(arguments, sessions_action_names())?;
            run_sessions_action(&action, arguments)
        }
        BROWSER_TOOL => {
            let action = required_action(arguments, browser_action_names())?;
            run_browser_action(&action, arguments)
        }
        COMPUTER_TOOL => {
            let action = required_action(arguments, computer_action_names())?;
            run_computer_action(&action, arguments)
        }
        // Legacy per-capability tool names: stale clients from before the
        // unified surface (a live session whose CLI reconnects its MCP server
        // onto an updated binary) keep working, unadvertised.
        name if name.strip_prefix("browser_").is_some_and(is_browser_action) => {
            run_browser_action(name.strip_prefix("browser_").unwrap(), arguments)
        }
        // Session creation is a user-only action in Unpeel — agents never spawn
        // sessions. These tools are no longer advertised; refuse them explicitly
        // in case a stale client still calls one.
        "start_session" | "delegate_task" | "delegate_batch" => Err(creation_disabled_message()),
        name if legacy_sessions_action(name).is_some() => {
            sessions_gate()?;
            run_sessions_tool(name, arguments)
        }
        _ => Err(format!(
            "Unknown tool: {name}. This server exposes one tool per domain ('sessions', \
'browser') taking an 'action' parameter; call {{\"action\":\"help\"}} on a tool for docs."
        )),
    }
}

const SESSIONS_TOOL: &str = "sessions";
const BROWSER_TOOL: &str = "browser";
const COMPUTER_TOOL: &str = "computer";

/// Unified action name → legacy tool name. The legacy names double as the
/// dispatch keys of `run_sessions_tool` and the doc source for `help`.
const SESSIONS_ACTIONS: &[(&str, &str)] = &[
    ("current", "get_current_session"),
    ("list", "list_sessions"),
    ("inspect", "inspect_session"),
    ("read_screen", "read_screen"),
    ("read_output", "read_output"),
    ("read_transcript", "read_transcript"),
    ("wait_for_text", "wait_for_text"),
    ("wait_for_status", "wait_for_status"),
    ("send_text", "send_text"),
    ("send_keys", "send_keys"),
    ("list_group", "list_group"),
    ("wait_for_group", "wait_for_group"),
    ("summarize_group", "summarize_group"),
    ("report_to_group", "report_to_group"),
    ("add_to_gallery", "add_to_gallery"),
    ("list_presets", "list_presets"),
    ("create_worktree", "create_worktree"),
    ("list_worktrees", "list_worktrees"),
    ("close", "close_session"),
];

const BROWSER_ACTIONS: &[&str] = &[
    "open",
    "snapshot",
    "click",
    "fill",
    "type",
    "press",
    "get",
    "screenshot",
    "wait",
    "scroll",
    "console",
    "close",
    "context",
];

fn sessions_action_names() -> Vec<&'static str> {
    let mut names: Vec<&'static str> = SESSIONS_ACTIONS.iter().map(|(action, _)| *action).collect();
    names.push("help");
    names
}

fn browser_action_names() -> Vec<&'static str> {
    let mut names: Vec<&'static str> = BROWSER_ACTIONS.to_vec();
    names.push("help");
    names
}

fn computer_action_names() -> Vec<&'static str> {
    let mut names: Vec<&'static str> = crate::computer_mcp::COMPUTER_ACTIONS.to_vec();
    names.push("help");
    names
}

fn is_browser_action(action: &str) -> bool {
    BROWSER_ACTIONS.contains(&action)
}

fn legacy_sessions_action(legacy_name: &str) -> Option<&'static str> {
    SESSIONS_ACTIONS
        .iter()
        .find(|(_, legacy)| *legacy == legacy_name)
        .map(|(action, _)| *action)
        .or(match legacy_name {
            // Decode-only compatibility for live sessions whose provider
            // cached the pre-group tool names. They now operate on peers in
            // the caller's effective group; no lineage is consulted.
            "list_children" => Some("list_group"),
            "wait_for_children" => Some("wait_for_group"),
            "summarize_children" => Some("summarize_group"),
            "report_to_parent" => Some("report_to_group"),
            _ => None,
        })
}

fn required_action(arguments: &Value, valid: Vec<&'static str>) -> Result<String, String> {
    match arguments.get("action").and_then(Value::as_str) {
        Some(action) if !action.trim().is_empty() => Ok(action.trim().to_string()),
        _ => Err(format!(
            "Missing required parameter: action (one of: {}).",
            valid.join(", ")
        )),
    }
}

fn run_sessions_action(action: &str, arguments: &Value) -> Result<String, String> {
    if action == "help" {
        return Ok(sessions_help(optional_trimmed_str(arguments, "help_for")));
    }
    let normalized = match action {
        "list_children" => "list_group",
        "wait_for_children" => "wait_for_group",
        "summarize_children" => "summarize_group",
        "report_to_parent" => "report_to_group",
        _ => action,
    };
    let Some((_, legacy)) = SESSIONS_ACTIONS
        .iter()
        .find(|(name, _)| *name == normalized)
    else {
        return Err(format!(
            "Unknown sessions action: {action}. Valid actions: {}. Call {{\"action\":\"help\"}} \
for per-action docs.",
            sessions_action_names().join(", ")
        ));
    };
    sessions_gate()?;
    run_sessions_tool(legacy, arguments)
}

fn run_browser_action(action: &str, arguments: &Value) -> Result<String, String> {
    match action {
        "help" => Ok(browser_help(optional_trimmed_str(arguments, "help_for"))),
        // Context stays reachable regardless of access state so an agent can
        // discover *why* the browser tools are refusing.
        "context" => crate::browser_mcp::tool_browser_context(),
        action if is_browser_action(action) => {
            if let Some(reason) = crate::browser_mcp::caller_refusal_reason() {
                return Err(reason);
            }
            if let Some(manifest) = caller_manifest() {
                if !manifest.browser_mcp_enabled() {
                    return Err(
                        "Browser tools were not enabled when this terminal was configured. They \
apply after Browser access is turned on and the terminal is reloaded or resumed."
                            .into(),
                    );
                }
            }
            crate::browser_mcp::run_tool(&format!("browser_{action}"), arguments)
        }
        _ => Err(format!(
            "Unknown browser action: {action}. Valid actions: {}. Call {{\"action\":\"help\"}} \
for per-action docs.",
            browser_action_names().join(", ")
        )),
    }
}

fn run_computer_action(action: &str, arguments: &Value) -> Result<String, String> {
    match action {
        "help" => Ok(computer_help(optional_trimmed_str(arguments, "help_for"))),
        // Context stays reachable regardless of access state so an agent can
        // discover *why* the computer tools are refusing (and never triggers
        // the approval prompt itself).
        "context" => crate::computer_mcp::tool_computer_context(),
        action if crate::computer_mcp::is_computer_action(action) => {
            if let Some(manifest) = caller_manifest() {
                if !manifest.computer_mcp_enabled() {
                    return Err(
                        "Computer tools were not enabled when this terminal was configured. They \
apply after Computer access is turned on and the terminal is reloaded or resumed."
                            .into(),
                    );
                }
            }
            if let Some(reason) = crate::computer_mcp::caller_refusal_reason() {
                return Err(reason);
            }
            crate::computer_mcp::run_action(action, arguments)
        }
        _ => Err(format!(
            "Unknown computer action: {action}. Valid actions: {}. Call {{\"action\":\"help\"}} \
for per-action docs.",
            computer_action_names().join(", ")
        )),
    }
}

fn computer_help(help_for: Option<&str>) -> String {
    let docs: Vec<(String, Value)> = crate::computer_mcp::action_docs()
        .into_iter()
        .filter_map(|definition| {
            let action = definition.get("name").and_then(Value::as_str)?;
            Some((action.to_string(), definition))
        })
        .collect();
    render_action_help(COMPUTER_TOOL, &docs, help_for)
}

/// The per-call sessions-domain gate: the shared caller checks plus the
/// launch-time domain grant. A session launched with the Sessions MCP disabled
/// can still reach this server through the unified config (injected when any
/// domain is enabled) or manual registration, so the manifest grant is
/// enforced here.
fn sessions_gate() -> Result<(), String> {
    if let Some(reason) = caller_refusal_reason() {
        return Err(reason);
    }
    if let Some(manifest) = caller_manifest() {
        if !manifest.sessions_mcp_enabled() {
            return Err(
                "Sessions tools were not enabled when this terminal was configured. They apply \
after Sessions MCP is enabled and the terminal is reloaded or resumed."
                    .into(),
            );
        }
    }
    Ok(())
}

fn run_sessions_tool(name: &str, arguments: &Value) -> Result<String, String> {
    match name {
        "get_current_session" => tool_get_current_session(arguments),
        "list_sessions" => tool_list_sessions(arguments),
        "inspect_session" => tool_inspect_session(arguments),
        "read_screen" => tool_read_screen(arguments),
        "read_output" => tool_read_output(arguments),
        "read_transcript" => tool_read_transcript(arguments),
        "wait_for_text" => tool_wait_for_text(arguments),
        "wait_for_status" => tool_wait_for_status(arguments),
        "send_text" => tool_send_text(arguments),
        "send_keys" => tool_send_keys(arguments),
        "list_group" | "list_children" => tool_list_group(arguments),
        "wait_for_group" | "wait_for_children" => tool_wait_for_group(arguments),
        "summarize_group" | "summarize_children" => tool_summarize_group(arguments),
        "report_to_group" | "report_to_parent" => tool_report_to_group(arguments),
        "add_to_gallery" => tool_add_to_gallery(arguments),
        "list_presets" => tool_list_presets(arguments),
        "create_worktree" => tool_create_worktree(arguments),
        "list_worktrees" => tool_list_worktrees(arguments),
        "close_session" => tool_close_session(arguments),
        _ => Err(format!("Unknown tool: {name}")),
    }
}

/// Whether Settings ▸ Sessions use allows sessions to create worktrees.
/// Parsed leniently from the app-state JSON (absent/malformed → false).
fn worktree_access_enabled(state: &Value) -> bool {
    state
        .get("mcp_worktree_access")
        .and_then(Value::as_bool)
        .unwrap_or(false)
}

fn require_worktree_access() -> Result<(), String> {
    let path = crate::app_paths::unpeel_home().join("app-state.json");
    let state = std::fs::read(&path)
        .ok()
        .and_then(|raw| serde_json::from_slice::<Value>(&raw).ok())
        .unwrap_or(Value::Null);
    if worktree_access_enabled(&state) {
        Ok(())
    } else {
        Err(
            "Creating worktrees from sessions is disabled. The user can enable it in \
Settings ▸ Sessions use (\"Let sessions create worktrees\")."
                .into(),
        )
    }
}

/// Create (or adopt) an Unpeel-managed worktree of a project and register it
/// as a child project — the same resolution the UI's "In a new worktree"
/// flow uses, minus the session launch (creation stays user-only).
fn tool_create_worktree(args: &Value) -> Result<String, String> {
    require_worktree_access()?;
    let branch = required_str(args, "branch")?;
    let project_id = resolve_project_id(args)?;
    let mut payload = json!({
        "project_id": project_id,
        "branch": branch,
    });
    if let Some(name) = optional_trimmed_str(args, "name") {
        payload["name"] = json!(name);
    }
    if let Some(base_ref) = optional_trimmed_str(args, "base_ref") {
        payload["base_ref"] = json!(base_ref);
    }
    let response = app_request("/mcp/create-worktree", &payload)?;
    let path = response
        .get("path")
        .and_then(Value::as_str)
        .unwrap_or("(unknown path)");
    let child_project = response
        .get("project_id")
        .and_then(Value::as_str)
        .unwrap_or("(unknown)");
    let adopted = response.get("adopted").and_then(Value::as_bool) == Some(true);
    let mut message = format!(
        "{} worktree for branch '{branch}' at {path} (child project {child_project}).",
        if adopted {
            "Adopted existing"
        } else {
            "Created"
        }
    );
    message.push_str(
        " The user can launch sessions into it from the sidebar; you can point shell \
commands at the path.",
    );
    Ok(message)
}

/// List a project's Unpeel-managed worktree child projects.
fn tool_list_worktrees(args: &Value) -> Result<String, String> {
    require_worktree_access()?;
    let project_id = resolve_project_id(args)?;
    let response = app_request("/mcp/list-worktrees", &json!({ "project_id": project_id }))?;
    serde_json::to_string_pretty(&response)
        .map_err(|e| format!("Failed to render worktree list: {e}"))
}

/// The MCP security state read from `app-state.json`: the project records and
/// the per-session access overrides. The file is the source of truth shared
/// by all instances and reflects role/reach changes immediately.
struct McpSecurity {
    /// Per-session access overrides. Sessions absent from this map use
    /// `default_grant`.
    grants: HashMap<String, McpGrant>,
    /// The app-wide default grant for sessions without an explicit override.
    default_grant: McpGrant,
    /// App-wide policy for writes outside the caller's effective group:
    /// ask (default), deny, or allow. Same-group writes never consult this.
    nonchild_write_access: crate::state::McpNonChildWriteAccess,
    /// User-approved cross-group write pairs, caller id → approved target ids.
    /// Written by the native app when the user answers the approval prompt.
    write_approvals: HashMap<String, Vec<String>>,
}

/// Read the security state leniently from the persisted app state. Each field
/// is extracted independently from the parsed JSON so a malformed override map
/// can never wipe the project list. An unparseable grant entry is dropped,
/// which falls back to the default grant rather than erroring.
fn load_security() -> McpSecurity {
    let path = crate::app_paths::unpeel_home().join("app-state.json");
    let value = std::fs::read(&path)
        .ok()
        .and_then(|raw| serde_json::from_slice::<Value>(&raw).ok())
        .unwrap_or(Value::Null);
    let grants = value
        .get("mcp_orchestrators")
        .and_then(Value::as_object)
        .map(|map| {
            map.iter()
                .filter_map(|(id, raw)| {
                    let grant = serde_json::from_value::<McpGrant>(raw.clone()).ok()?;
                    Some((id.clone(), grant))
                })
                .collect()
        })
        .unwrap_or_default();
    let default_grant = value
        .get("mcp_default_access")
        .and_then(|raw| serde_json::from_value::<McpGrant>(raw.clone()).ok())
        .unwrap_or_default();
    let nonchild_write_access = value
        .get("mcp_nonchild_write_access")
        .and_then(Value::as_str)
        .map(crate::state::McpNonChildWriteAccess::from_state_str)
        .unwrap_or_default();
    let write_approvals = value
        .get("mcp_write_approvals")
        .cloned()
        .and_then(|raw| serde_json::from_value::<HashMap<String, Vec<String>>>(raw).ok())
        .unwrap_or_default();
    McpSecurity {
        grants,
        default_grant,
        nonchild_write_access,
        write_approvals,
    }
}

pub(crate) fn load_activity_state() -> HashMap<String, ActivityStateEntry> {
    std::fs::read_to_string(crate::app_paths::activity_state_path())
        .ok()
        .and_then(|raw| serde_json::from_str::<ActivityStateFile>(&raw).ok())
        .map(|file| file.sessions)
        .unwrap_or_default()
}

fn activity_entry_for<'a>(
    activity: &'a HashMap<String, ActivityStateEntry>,
    session_id: &str,
) -> Option<&'a ActivityStateEntry> {
    activity.get(session_id)
}

pub(crate) fn activity_status_for_manifest(
    activity: &HashMap<String, ActivityStateEntry>,
    manifest: &HostedSessionManifest,
) -> String {
    activity_entry_for(activity, &manifest.session.id)
        .and_then(|entry| entry.activity_status.as_deref())
        .filter(|status| valid_activity_status(status))
        .map(str::to_string)
        .unwrap_or_else(|| match manifest.state {
            HostedSessionState::Running => "idle".to_string(),
            HostedSessionState::Exited => "exited".to_string(),
        })
}

/// Whether the session's `current` activity status satisfies a `wait_for_status`
/// target. Exact match, plus one equivalence: **`done` and `idle` are the same
/// underlying settled state** — a session that finishes a turn is internally
/// idle, and the app reports it as `done` only while it's *unread* (settled
/// while the user isn't looking at it). A waiting agent can't control the
/// user's UI focus, so waiting for the "wrong" label would hang until timeout.
/// Treating them as one "the turn finished" target is what an agent driving a
/// session actually means.
fn status_matches(current: &str, desired: &str) -> bool {
    if current == desired {
        return true;
    }
    matches!((current, desired), ("done", "idle") | ("idle", "done"))
}

fn valid_activity_status(status: &str) -> bool {
    matches!(
        status,
        "starting" | "working" | "blocked" | "done" | "idle" | "exited" | "unknown"
    )
}

impl McpSecurity {
    /// The full grant (role + reach) the given caller is evaluated against. An
    /// unknown caller has no access (`Off`); a known caller absent from the
    /// override map gets the app-wide default grant.
    fn effective_grant(&self, caller: Option<&HostedSessionManifest>) -> McpGrant {
        match caller {
            None => McpGrant {
                role: McpRole::Off,
                reach: McpScope::Project,
            },
            Some(manifest) => self
                .grants
                .get(&manifest.session.id)
                .copied()
                .unwrap_or(self.default_grant),
        }
    }

    /// The capability role the given caller is evaluated against.
    fn effective_role(&self, caller: Option<&HostedSessionManifest>) -> McpRole {
        self.effective_grant(caller).role
    }

    /// Whether the caller may SEE and READ `target`. Reading is open across
    /// ALL sessions (2026-07-16 model change — visibility used to stop at the
    /// caller's project tree): any enabled caller reads everything; only
    /// writes are gated (same group or a user-approved pair). An unknown caller
    /// or one whose access is internally Off still sees nothing.
    fn permits_manifest(
        &self,
        caller: Option<&HostedSessionManifest>,
        target: &HostedSessionManifest,
    ) -> bool {
        let _ = target;
        caller.is_some() && self.effective_role(caller) != McpRole::Off
    }

    /// Whether the user already approved `caller` writing into `target`
    /// (the remembered answer to a previous approval prompt).
    fn write_pair_approved(&self, caller_id: &str, target_id: &str) -> bool {
        self.write_approvals
            .get(caller_id)
            .map(|targets| targets.iter().any(|id| id == target_id))
            .unwrap_or(false)
    }
}

fn known_project_ids() -> HashSet<String> {
    let path = crate::app_paths::unpeel_home().join("app-state.json");
    std::fs::read(&path)
        .ok()
        .and_then(|raw| serde_json::from_slice::<Value>(&raw).ok())
        .and_then(|value| value.get("projects").cloned())
        .and_then(|projects| projects.as_array().cloned())
        .map(|projects| {
            projects
                .into_iter()
                .filter_map(|project| project.get("id")?.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

/// The group a session currently renders in. A valid shared project override
/// moves a live session between plain groups without rewriting its immutable
/// launch manifest; stale overrides fall back to the manifest project, exactly
/// like the desktop and TUI sidebars.
fn effective_group_id(
    manifest: &HostedSessionManifest,
    known_projects: &HashSet<String>,
) -> String {
    crate::session_ops::project_override_marker(&manifest.session.id)
        .filter(|project_id| known_projects.contains(project_id))
        .unwrap_or_else(|| manifest.session.project_id.clone())
}

/// Same-group sessions are the free-write/control unit. Project roots, plain
/// organizational groups, and worktrees each have distinct project ids, so
/// moving a row in the sidebar immediately changes this boundary.
fn caller_shares_group(
    caller: Option<&HostedSessionManifest>,
    target: &HostedSessionManifest,
) -> bool {
    let Some(caller) = caller else {
        return false;
    };
    let known_projects = known_project_ids();
    effective_group_id(caller, &known_projects) == effective_group_id(target, &known_projects)
}

/// The only message channel today: terminal-to-terminal. Future channels
/// (Slack↔terminal — see docs/feature/sessions-mcp-channels.md) add their ids
/// alongside this one.
pub(crate) const MESSAGE_CHANNEL_TERMINAL: &str = "terminal";

/// Rendered provenance header for a message crossing into a session's
/// terminal: the receiving agent learns who sent it (the id is the reply
/// address for send_text) and over which channel.
fn message_envelope_header(from_session_id: &str, channel: &str) -> String {
    format!("[message from id:{from_session_id}, channel: {channel}]")
}

/// The envelope to prepend to a send_text body, or None when the send should
/// stay untouched. Same-group traffic is orchestration (including exact shell
/// commands, while report_to_group self-identifies); cross-group traffic is
/// attributed so it cannot be mistaken for the user typing.
fn send_text_envelope(
    caller: Option<&HostedSessionManifest>,
    target: Option<&HostedSessionManifest>,
) -> Option<String> {
    let caller = caller?;
    let target = target?;
    if caller_shares_group(Some(caller), target) {
        return None;
    }
    Some(message_envelope_header(
        &caller.session.id,
        MESSAGE_CHANNEL_TERMINAL,
    ))
}

/// The calling session's full manifest, used for scope checks that need the
/// caller's working directory (native worktree projects may be UserDefaults-only
/// and therefore absent from app-state.json).
fn caller_manifest() -> Option<HostedSessionManifest> {
    let self_id = self_session_id()?;
    load_manifest(&self_id)
}

/// Why the whole tool call should be refused regardless of target: the calling
/// session is unknown, or its Session Access is internally disabled.
/// Re-checked per call so a role/reach change applies immediately, even to
/// already-connected sessions.
fn caller_refusal_reason() -> Option<String> {
    let security = load_security();
    let manifest = caller_manifest();
    if manifest.is_none() {
        return Some(
            "The calling session is unknown, so Unpeel MCP can't authorize access. \
Run this from a hosted Unpeel session."
                .into(),
        );
    }
    if security.effective_role(manifest.as_ref()) == McpRole::Off {
        return Some(
            "This session's Sessions use access is disabled by a saved setting. \
Restart the session to use the session-control tools."
                .into(),
        );
    }
    None
}

/// Error returned when the caller cannot be identified. Reads are open to all
/// sessions for any known caller, so an unknown caller is the only read
/// refusal left.
fn read_denied_message() -> String {
    "The calling session is unknown, so Unpeel MCP can't authorize \
cross-session access. Run this from a hosted Unpeel session."
        .into()
}

/// Error returned when a caller tries to write to a cross-group session while
/// the user has set the write policy to Never allow.
fn write_denied_message() -> String {
    "You can only write to (send_text/send_keys) sessions in your current sidebar group: the \
user set Settings ▸ Sessions MCP ▸ Writing to other groups to Never allow. Every session can \
still be read. Ask the user to move the sessions into the same group or change that setting."
        .into()
}

/// Error returned when a caller tries to create a session. Creation is a
/// user-only action in Unpeel; agents drive sessions the user created, they
/// never spawn their own.
fn creation_disabled_message() -> String {
    "Agents cannot create sessions in Unpeel — session creation is a user-only action. \
Ask the user to create the session in the desired sidebar group; sessions in the same group \
can then coordinate without a write approval."
        .into()
}

/// Error returned when a caller tries to close a session outside its group.
fn close_denied_message() -> String {
    "You can only close another session in your current sidebar group. Move the session into \
this group first, or close it from the Unpeel UI."
        .into()
}

/// The advertised surface: one action-enum tool per domain, computed per
/// caller. A domain the session launched without is absent entirely (zero
/// context cost); an unknown caller (dev testing via a raw pipe) sees both,
/// since visibility is not a grant — the per-call gates enforce access.
fn tool_definitions_with_domains(domains: McpDomainMask) -> Vec<Value> {
    let manifest = caller_manifest();
    tool_definitions_for_manifest(manifest.as_ref(), domains)
}

fn tool_definitions_for_manifest(
    manifest: Option<&HostedSessionManifest>,
    domains: McpDomainMask,
) -> Vec<Value> {
    let advertise_sessions =
        domains.sessions && manifest.is_none_or(HostedSessionManifest::sessions_mcp_enabled);
    let advertise_browser =
        domains.browser && manifest.is_none_or(HostedSessionManifest::browser_mcp_enabled);
    let advertise_computer =
        domains.computer && manifest.is_none_or(HostedSessionManifest::computer_mcp_enabled);
    let mut tools = Vec::new();
    if advertise_sessions {
        tools.push(sessions_tool_definition());
    }
    if advertise_browser {
        tools.push(browser_tool_definition());
    }
    if advertise_computer {
        tools.push(computer_tool_definition());
    }
    tools
}

fn computer_tool_definition() -> Value {
    json!({
        "name": COMPUTER_TOOL,
        "description": "Control this Mac's real apps in the background — no focus steal, \
    the user's cursor never moves (the user sees an overlay cursor; actions may need their \
    one-time approval). Core loop: 'launch' an app → pid + windows; 'see' a window → \
    element tree with [N] indices PLUS a screenshot artifact; act by element_index \
    ('click'/'type'/'set_value'); re-'see' to verify — indices go stale on every see, and \
    an unchanged tree means the action likely no-oped. When the tree lies or is empty \
    (Electron/canvas), act by x/y read off the same screenshot. Desktop-wide scope needs \
    'escalate'. {\"action\":\"help\"} returns full per-action docs; \
    {\"action\":\"context\"} explains access and permission state.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": computer_action_names(),
                    "description": "What to do; see the tool description and help"
                },
                "pid": { "type": "integer", "description": "Target process id (from launch/apps)" },
                "window_id": { "type": "integer", "description": "Target window (from launch/windows; required with element_index)" },
                "element_index": { "type": "integer", "description": "click/type/set_value/press/scroll: [N] from the latest see" },
                "x": { "type": "number", "description": "Pixel X off the latest see screenshot (window-local, top-left)" },
                "y": { "type": "number", "description": "Pixel Y (see x)" },
                "text": { "type": "string", "description": "type: text to insert" },
                "value": { "type": "string", "description": "set_value: non-text control value" },
                "key": { "type": "string", "description": "press: key name (return, tab, escape, arrows…)" },
                "keys": { "type": "string", "description": "hotkey: [\"cmd\",\"shift\",\"t\"] or \"cmd,shift,t\"" },
                "modifiers": { "type": "array", "items": {"type": "string"}, "description": "click/press/drag: held modifiers (cmd, shift, option, ctrl)" },
                "app": { "type": "string", "description": "launch: application name" },
                "bundle_id": { "type": "string", "description": "launch: bundle id (wins over app)" },
                "urls": { "type": "array", "items": {"type": "string"}, "description": "launch: documents/URLs to open" },
                "new_instance": { "type": "boolean", "description": "launch: force a separate app instance" },
                "query": { "type": "string", "description": "see: filter the element tree" },
                "screenshot": { "type": "boolean", "description": "see: capture pixels too (default true)" },
                "double": { "type": "boolean", "description": "click: double-click / open" },
                "right": { "type": "boolean", "description": "click: right-click / context menu" },
                "button": { "type": "string", "description": "click/drag: left | right | middle" },
                "count": { "type": "integer", "description": "click: click count (pixel path)" },
                "direction": { "type": "string", "enum": ["up", "down", "left", "right"], "description": "scroll: direction" },
                "amount": { "type": "integer", "description": "scroll: how far" },
                "from_x": { "type": "number", "description": "drag: start X" },
                "from_y": { "type": "number", "description": "drag: start Y" },
                "to_x": { "type": "number", "description": "drag: end X" },
                "to_y": { "type": "number", "description": "drag: end Y" },
                "scope": { "type": "string", "description": "click/type/press/hotkey/scroll: \"desktop\" for screen-absolute input (needs escalate)" },
                "delivery_mode": { "type": "string", "description": "Input rung: background (default) | foreground — escalate only when the driver recommends it" },
                "reason": { "type": "string", "description": "escalate: advertised reason (e.g. \"foreground_ineffective\")" },
                "help_for": { "type": "string", "description": "help: docs for one action only" },
            },
            "required": ["action"],
            "additionalProperties": false,
        },
    })
}

fn sessions_tool_definition() -> Value {
    json!({
        "name": SESSIONS_TOOL,
        "description": "Inspect and control the user's other Unpeel terminal sessions. Flow: \
    action 'list' to pick a target, 'inspect' for a compact look, then small reads; after \
    send_text/send_keys use wait_for_status or wait_for_text. Reads are open; writes inside \
    your sidebar group are free and writes to other groups ask for approval. \
    {\"action\":\"help\"} returns full \
    per-action docs and required params.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": sessions_action_names(),
                    "description": "What to do; see the tool description and help"
                },
                "session_id": { "type": "string", "description": "Target session id (most actions; from action 'list')" },
                "text": { "type": "string", "description": "send_text: text to type; wait_for_text: substring to wait for" },
                "submit": { "type": "boolean", "description": "send_text/report_to_group: press Enter after (default true)" },
                "keys": { "type": "array", "items": { "type": "string" }, "description": "send_keys: keys in order, e.g. [\"down\",\"enter\"] (max 40)" },
                "delay_ms": { "type": "integer", "description": "send_keys: delay between keys in ms (default 60, max 1000)" },
                "timeout_ms": { "type": "integer", "description": "wait_* actions: wait budget in ms (default 30000, max 120000)" },
                "status": { "type": "string", "description": "wait_for_status: idle (turn finished; also matches done), working, blocked, starting, exited; report_to_group: update|done|blocked" },
                "rows": { "type": "integer", "description": "read_screen: rows to return (default terminal height, max 500)" },
                "scroll_offset_rows": { "type": "integer", "description": "read_screen: scroll up into scrollback (default 0)" },
                "tail_bytes": { "type": "integer", "description": "read_output: trailing bytes (default 16384, max 262144)" },
                "strip_ansi": { "type": "boolean", "description": "read_output: strip ANSI sequences (default true)" },
                "entries": { "type": "integer", "description": "read_transcript/summarize_group: recent entries" },
                "include_tools": { "type": "boolean", "description": "read_transcript: include tool call/result entries" },
                "case_sensitive": { "type": "boolean", "description": "wait_for_text: match case-sensitively (default false)" },
                "session_ids": { "type": "array", "items": { "type": "string" }, "description": "wait_for_group/summarize_group: subset of group peer ids" },
                "include_exited": { "type": "boolean", "description": "list_group: include exited peers (default true)" },
                "project_id": { "type": "string", "description": "list_presets/create_worktree/list_worktrees: project (default: the calling session's)" },
                "branch": { "type": "string", "description": "create_worktree: branch to create or adopt" },
                "name": { "type": "string", "description": "create_worktree: worktree folder (default: branch slug)" },
                "base_ref": { "type": "string", "description": "create_worktree: base for a new branch (default: mainline)" },
                "summary": { "type": "string", "description": "report_to_group: concise result (required there)" },
                "details": { "type": "string", "description": "report_to_group: optional details" },
                "proof": { "type": "array", "items": { "type": "string" }, "description": "report_to_group: evidence" },
                "changed_paths": { "type": "array", "items": { "type": "string" }, "description": "report_to_group: changed files" },
                "artifacts": { "type": "array", "items": { "type": "string" }, "description": "report_to_group: artifact paths/URLs" },
                "path": { "type": "string", "description": "add_to_gallery: local PNG/JPEG/GIF/WebP path" },
                "blockers": { "type": "array", "items": { "type": "string" }, "description": "report_to_group: blocking issues" },
                "questions": { "type": "array", "items": { "type": "string" }, "description": "report_to_group: questions for the peer/user" },
                "next_steps": { "type": "array", "items": { "type": "string" }, "description": "report_to_group: suggested follow-ups" },
                "help_for": { "type": "string", "description": "help: docs for one action only" },
            },
            "required": ["action"],
            "additionalProperties": false,
        },
    })
}

fn browser_tool_definition() -> Value {
    json!({
        "name": BROWSER_TOOL,
        "description": "Operate a real browser isolated to this session. Core loop: action \
    'open' a URL, 'snapshot' for element refs like @e1, act by ref ('click'/'fill'), then \
    re-snapshot after navigation — refs go stale. 'screenshot' saves into this session's \
    artifacts and returns the file path. {\"action\":\"help\"} returns full per-action docs; \
    {\"action\":\"context\"} explains configuration if tools seem unavailable.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": browser_action_names(),
                    "description": "What to do; see the tool description and help"
                },
                "url": { "type": "string", "description": "open: URL to open" },
                "target": { "type": "string", "description": "click/fill/type/get: snapshot ref (@e1) or CSS selector" },
                "text": { "type": "string", "description": "fill/type: text to enter" },
                "key": { "type": "string", "description": "press: key or combination (Enter, Tab, Control+a)" },
                "what": { "type": "string", "enum": ["text", "html", "value", "url", "title", "count"], "description": "get: what to read (element reads need target)" },
                "interactive": { "type": "boolean", "description": "snapshot: interactive elements only (default true)" },
                "compact": { "type": "boolean", "description": "snapshot: drop empty structural nodes (default true)" },
                "full": { "type": "boolean", "description": "screenshot: full page instead of viewport (default false)" },
                "annotate": { "type": "boolean", "description": "screenshot: numbered labels matching snapshot refs (default false)" },
                "gallery": { "type": "boolean", "description": "screenshot: add to gallery; omit for Settings default" },
                "selector": { "type": "string", "description": "wait: until this CSS selector exists" },
                "load": { "type": "string", "enum": ["load", "domcontentloaded", "networkidle"], "description": "wait: until this load state" },
                "ms": { "type": "integer", "description": "wait: fixed delay in ms (max 30000)" },
                "direction": { "type": "string", "enum": ["up", "down", "left", "right"], "description": "scroll: direction" },
                "pixels": { "type": "integer", "description": "scroll: distance in pixels" },
                "into_view": { "type": "string", "description": "scroll: instead, scroll this ref/selector into view" },
                "clear": { "type": "boolean", "description": "console: clear the log after reading (default false)" },
                "help_for": { "type": "string", "description": "help: docs for one action only" },
            },
            "required": ["action"],
            "additionalProperties": false,
        },
    })
}

fn sessions_help(help_for: Option<&str>) -> String {
    let docs: Vec<(String, Value)> = legacy_sessions_tool_definitions()
        .into_iter()
        .filter_map(|definition| {
            let legacy = definition.get("name").and_then(Value::as_str)?;
            let action = legacy_sessions_action(legacy)?;
            Some((action.to_string(), definition))
        })
        .collect();
    let mut text = render_action_help(SESSIONS_TOOL, &docs, help_for);
    // The legacy definitions cross-reference each other by old tool name;
    // rewrite those mentions to the action vocabulary agents actually use.
    for (action, legacy) in SESSIONS_ACTIONS {
        if action != legacy {
            text = text.replace(legacy, action);
        }
    }
    text
}

fn browser_help(help_for: Option<&str>) -> String {
    let docs: Vec<(String, Value)> = crate::browser_mcp::tool_definitions()
        .into_iter()
        .filter_map(|definition| {
            let legacy = definition.get("name").and_then(Value::as_str)?;
            let action = legacy.strip_prefix("browser_")?;
            Some((action.to_string(), definition))
        })
        .collect();
    let mut text = render_action_help(BROWSER_TOOL, &docs, help_for);
    for action in browser_action_names() {
        text = text.replace(&format!("browser_{action}"), action);
    }
    text
}

/// Render per-action docs from the legacy tool definitions, which stay the
/// single source of truth for full descriptions and parameter contracts.
fn render_action_help(tool: &str, docs: &[(String, Value)], help_for: Option<&str>) -> String {
    let mut sections = Vec::new();
    for (action, definition) in docs {
        if help_for.is_some_and(|wanted| wanted != action) {
            continue;
        }
        let mut lines = vec![format!("### {action}")];
        if let Some(description) = definition.get("description").and_then(Value::as_str) {
            lines.push(collapse_whitespace(description));
        }
        let schema = definition
            .get("inputSchema")
            .cloned()
            .unwrap_or(Value::Null);
        let required: HashSet<&str> = schema
            .get("required")
            .and_then(Value::as_array)
            .map(|values| values.iter().filter_map(Value::as_str).collect())
            .unwrap_or_default();
        if let Some(properties) = schema.get("properties").and_then(Value::as_object) {
            if !properties.is_empty() {
                lines.push("Params:".into());
                for (key, prop) in properties {
                    let kind = prop.get("type").and_then(Value::as_str).unwrap_or("value");
                    let requirement = if required.contains(key.as_str()) {
                        ", required"
                    } else {
                        ""
                    };
                    let description = prop
                        .get("description")
                        .and_then(Value::as_str)
                        .map(collapse_whitespace)
                        .unwrap_or_default();
                    lines.push(format!("- {key} ({kind}{requirement}): {description}"));
                }
            }
        }
        sections.push(lines.join("\n"));
    }
    if sections.is_empty() {
        let known = docs
            .iter()
            .map(|(action, _)| action.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        return format!(
            "No such {tool} action: {}. Known actions: {known}.",
            help_for.unwrap_or("")
        );
    }
    format!(
        "'{tool}' actions — pass these as {{\"action\": ...}} with the listed params:\n\n{}",
        sections.join("\n\n")
    )
}

fn collapse_whitespace(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// The pre-unification per-tool definitions. No longer advertised; kept as
/// the doc source for `action: "help"` and the contract reference for the
/// legacy tool names that stale clients may still call.
pub(crate) fn legacy_sessions_tool_definitions() -> Vec<Value> {
    vec![
        json!({
            "name": "get_current_session",
            "description": "Return the calling session's own identity, activity, \
        Sessions MCP access, effective sidebar group, and group member count. Use this to \
        answer questions like \"who am I\" or \"which sessions can I coordinate without \
        approval\" without reading manifests from disk.",
            "inputSchema": {
                "type": "object",
                "properties": {},
                "additionalProperties": false,
            },
        }),
        json!({
            "name": "list_sessions",
            "description": "List running Unpeel terminal sessions (agents and shells). \
        Use this only to choose a target, then call inspect_session before deeper reads. \
        The calling session is marked with \"self\": true and cannot be written to.",
            "inputSchema": {
                "type": "object",
                "properties": {},
                "additionalProperties": false,
            },
        }),
        json!({
            "name": "inspect_session",
            "description": "Compact first look at one session: metadata, current screen tail, \
        tiny Claude/Codex transcript tail when available, and the next recommended tool. \
        Prefer this over read_screen/read_transcript when you are orienting and want low context.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "session_id": { "type": "string", "description": "Target session id from list_sessions" },
                },
                "required": ["session_id"],
                "additionalProperties": false,
            },
        }),
        json!({
            "name": "read_screen",
            "description": "Read the current rendered terminal screen of a session \
        (what a human looking at it sees, including TUI dialogs and permission prompts). \
        Use after inspect_session when you need more current UI detail; pass a small rows \
        value for minimal context. Use scroll_offset_rows to look back into scrollback.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "session_id": { "type": "string", "description": "Target session id from list_sessions" },
                    "rows": { "type": "integer", "description": "Number of rows to return (default: terminal height, max 500)" },
                    "scroll_offset_rows": { "type": "integer", "description": "Scroll this many rows up into scrollback (default 0)" },
                },
                "required": ["session_id"],
                "additionalProperties": false,
            },
        }),
        json!({
            "name": "read_output",
            "description": "Read the tail of a session's raw output log. Use as a fallback \
        when inspect_session, read_transcript, or read_screen cannot answer the question; \
        also works for exited sessions. ANSI escape sequences are stripped by default.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "session_id": { "type": "string", "description": "Target session id from list_sessions" },
                    "tail_bytes": { "type": "integer", "description": "How many bytes of trailing output to read (default 16384, max 262144)" },
                    "strip_ansi": { "type": "boolean", "description": "Strip ANSI escape sequences (default true)" },
                },
                "required": ["session_id"],
                "additionalProperties": false,
            },
        }),
        json!({
            "name": "read_transcript",
            "description": "Read a Claude/Codex conversation transcript as Markdown for a \
        session when available. This is better than read_screen for conversation \
        history because it includes user/assistant messages and concise tool events \
        even when the terminal TUI has redrawn over them. Content defaults come from \
        the user's Settings ▸ Transcripts options; the args below override \
        them. Use after inspect_session when you need more history.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "session_id": { "type": "string", "description": "Target session id from list_sessions" },
                    "entries": { "type": "integer", "description": "Number of most-recent transcript entries to return (max 100). Omit to use the Settings default." },
                    "include_tools": { "type": "boolean", "description": "Include concise tool call/result entries. Omit to use the Settings default." },
                },
                "required": ["session_id"],
                "additionalProperties": false,
            },
        }),
        json!({
            "name": "wait_for_text",
            "description": "Block until the given text appears on a session's rendered \
        screen, then return the matching line — much more reliable than polling read_screen \
        after send_text/send_keys. Matches a plain substring (case-insensitive by default). \
        Fails after timeout_ms with the session's final screen content.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "session_id": { "type": "string", "description": "Target session id from list_sessions" },
                    "text": { "type": "string", "description": "Substring to wait for on the rendered screen" },
                    "timeout_ms": { "type": "integer", "description": "How long to wait before failing (default 30000, max 120000)" },
                    "case_sensitive": { "type": "boolean", "description": "Match case-sensitively (default false)" },
                },
                "required": ["session_id", "text"],
                "additionalProperties": false,
            },
        }),
        json!({
            "name": "wait_for_status",
            "description": "Block until a session reaches a product activity status: \
        working, blocked, done, idle, starting, or exited. Works for every provider \
        (Claude, Codex, Gemini, …). To wait for an agent to **finish its turn**, wait for \
        'idle' — this also matches 'done' (they are the same settled state; a session reads \
        as 'done' only while you aren't looking at it). Wait for 'blocked' for a permission \
        prompt. Tip: this returns immediately if the session is already in the target state, \
        so if a turn might already be running, wait_for_text on an expected output is more \
        precise.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "session_id": { "type": "string", "description": "Target session id from list_sessions" },
                    "status": {
                        "type": "string",
                        "enum": ["starting", "working", "blocked", "done", "idle", "exited", "unknown"],
                        "description": "Activity status to wait for"
                    },
                    "timeout_ms": { "type": "integer", "description": "How long to wait before failing (default 30000, max 120000)" },
                },
                "required": ["session_id", "status"],
                "additionalProperties": false,
            },
        }),
        json!({
            "name": "send_text",
            "description": "Type text into another session's terminal as a bracketed \
        paste and (by default) press Enter to submit it. Use this to prompt an agent \
        running in that session or to run a shell command there. Check the session's \
        screen with read_screen first so you know what will receive the input. \
        To wait for the agent's reply, follow with wait_for_status status='idle' (the \
        finished-turn state; it also matches 'done'). Do NOT wait for 'working' to confirm \
        it started — a fast turn can finish before you poll, and 'working' would then never \
        match and hang until timeout; wait_for_text on an expected output is the precise \
        alternative. Writing to a session outside your sidebar group asks the \
        user for approval — the call may block up to ~2 minutes on the dialog — and the \
        delivered text is prefixed with a provenance header, \
        '[message from id:<your session id>, channel: terminal]', so the receiving agent \
        knows who is talking and can reply to that id. Same-group sends are delivered \
        verbatim.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "session_id": { "type": "string", "description": "Target session id from list_sessions" },
                    "text": { "type": "string", "description": "Text to type into the session" },
                    "submit": { "type": "boolean", "description": "Press Enter after the text (default true)" },
                },
                "required": ["session_id", "text"],
                "additionalProperties": false,
            },
        }),
        json!({
            "name": "send_keys",
            "description": "Send individual keystrokes to another session, e.g. to \
        answer an interactive prompt: [\"down\", \"enter\"] selects the second option of \
        a menu. Supported keys: enter, tab, shift+tab, space, esc, up, down, left, right, \
        home, end, pageup, pagedown, backspace, delete, ctrl+<letter>, or any single character. \
        Same approval rule as send_text: cross-group targets ask the user first.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "session_id": { "type": "string", "description": "Target session id from list_sessions" },
                    "keys": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Keys to send in order (max 40)",
                    },
                    "delay_ms": { "type": "integer", "description": "Delay between keys in milliseconds (default 60, max 1000)" },
                },
                "required": ["session_id", "keys"],
                "additionalProperties": false,
            },
        }),
        json!({
            "name": "list_group",
            "description": "List the other sessions in your effective sidebar group. Use this \
        to see peer ids, activity, and transcript availability for sessions you can coordinate \
        without a write approval.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "include_exited": { "type": "boolean", "description": "Include exited group peers (default true)." },
                },
                "additionalProperties": false,
            },
        }),
        json!({
            "name": "wait_for_group",
            "description": "Wait until selected peers in your group reach a terminal \
        coordination state: done, blocked, or exited.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "session_ids": { "type": "array", "items": { "type": "string" }, "description": "Optional subset of group peer ids to wait for. Defaults to all peers." },
                    "timeout_ms": { "type": "integer", "description": "How long to wait before failing (default 30000, max 120000)." },
                },
                "additionalProperties": false,
            },
        }),
        json!({
            "name": "summarize_group",
            "description": "Return compact transcript/screen tails for the other sessions in \
        your group so you can synthesize results without reading full logs. \
        This tool does not invent conclusions; it collects each peer's latest visible \
        evidence.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "session_ids": { "type": "array", "items": { "type": "string" }, "description": "Optional subset of group peer ids to summarize. Defaults to all peers." },
                    "entries": { "type": "integer", "description": "Transcript entries per peer when available (default 5, max 20)." },
                },
                "additionalProperties": false,
            },
        }),
        json!({
            "name": "report_to_group",
            "description": "Send a structured update or final result to a chosen peer in your \
        sidebar group. Same-group reports never require a write approval.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "session_id": { "type": "string", "description": "Target peer id from list_group." },
                    "status": {
                        "type": "string",
                        "enum": ["update", "done", "blocked"],
                        "description": "Report status (default update)."
                    },
                    "summary": { "type": "string", "description": "Concise result or status summary." },
                    "details": { "type": "string", "description": "Optional details the peer should know." },
                    "proof": { "type": "array", "items": { "type": "string" }, "description": "Commands, checks, screenshots, artifacts, or evidence." },
                    "changed_paths": { "type": "array", "items": { "type": "string" }, "description": "Files or artifacts changed by the reporting session." },
                    "artifacts": { "type": "array", "items": { "type": "string" }, "description": "Generated artifact paths, URLs, or identifiers." },
                    "blockers": { "type": "array", "items": { "type": "string" }, "description": "Blocking issues or missing approvals/context." },
                    "questions": { "type": "array", "items": { "type": "string" }, "description": "Questions that need peer/user input." },
                    "next_steps": { "type": "array", "items": { "type": "string" }, "description": "Suggested follow-up steps." },
                    "submit": { "type": "boolean", "description": "Press Enter after sending the report (default true)." },
                },
                "required": ["session_id", "summary"],
                "additionalProperties": false,
            },
        }),
        json!({
            "name": "add_to_gallery",
            "description": "Copy a local PNG, JPEG, GIF, or WebP image into this session's \
        gallery and return its durable gallery path. Relative paths resolve from the session's \
        working directory. Publishes only to the calling session (maximum 32 MiB).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Local image path (absolute or relative to the session working directory)" },
                },
                "required": ["path"],
                "additionalProperties": false,
            },
        }),
        json!({
            "name": "list_presets",
            "description": "List the launch presets configured in Unpeel (global and \
        project-scoped) so you can tell the user which presets exist when they ask. Defaults \
        to the calling session's project.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "project_id": { "type": "string", "description": "Project to list presets for (default: the calling session's project)" },
                },
                "additionalProperties": false,
            },
        }),
        json!({
            "name": "create_worktree",
            "description": "Create (or adopt) an Unpeel-managed git worktree of a project and \
        register it as a child project in the sidebar. Session creation remains user-only. \
        Requires the user's Settings ▸ Sessions use permission.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "branch": { "type": "string", "description": "Branch to create or adopt" },
                    "project_id": { "type": "string", "description": "Project to branch from (default: the calling session's)" },
                    "name": { "type": "string", "description": "Worktree folder name (default: branch slug)" },
                    "base_ref": { "type": "string", "description": "Base ref for a new branch (default: the repo mainline)" },
                },
                "required": ["branch"],
                "additionalProperties": false,
            },
        }),
        json!({
            "name": "list_worktrees",
            "description": "List a project's Unpeel-managed worktree child projects (branch \
        and checkout path each).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "project_id": { "type": "string", "description": "Root project (default: the calling session's)" },
                },
                "additionalProperties": false,
            },
        }),
        json!({
            "name": "close_session",
            "description": "Close/delete a Unpeel session: kills its process and removes it \
        from the sidebar. You can close another session in your current group, never your own \
        session or one in another group.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "session_id": { "type": "string", "description": "Target session id from list_sessions" },
                },
                "required": ["session_id"],
                "additionalProperties": false,
            },
        }),
    ]
}

fn session_context_json(
    manifest: &HostedSessionManifest,
    activity: &HashMap<String, ActivityStateEntry>,
) -> Value {
    let session = &manifest.session;
    let activity_entry = activity_entry_for(activity, &session.id);
    let group_id = effective_group_id(manifest, &known_project_ids());
    json!({
        "id": session.id,
        "available": true,
        "label": session.label,
        "provider": provider_label_for_command(&session.command),
        "transcript": transcript_status_hint(manifest),
        "activity_status": activity_status_for_manifest(activity, manifest),
        "raw_status": activity_entry.and_then(|entry| entry.raw_status.as_deref()),
        "unread": activity_entry.map(|entry| entry.unread).unwrap_or(false),
        "completed": activity_entry.map(|entry| entry.completed).unwrap_or(false),
        "state": hosted_session_state_label(manifest.state),
        "command": session.command,
        "project_id": session.project_id,
        "group_id": group_id,
        "cwd": manifest.cwd,
        "worktree_branch": session.worktree_branch,
        "created_at": session.created_at,
        "spawned_by": session.spawned_by,
        "role": session.role,
        "task": session.task,
        "self": self_session_id().as_deref() == Some(session.id.as_str()),
    })
}

fn tool_get_current_session(_args: &Value) -> Result<String, String> {
    let caller = caller_manifest().ok_or_else(|| {
        "The calling session is unknown, so current-session context is unavailable.".to_string()
    })?;
    let security = load_security();
    let grant = security.effective_grant(Some(&caller));
    let activity = load_activity_state();
    let known_projects = known_project_ids();
    let group_id = effective_group_id(&caller, &known_projects);
    let group_peer_count = session_host::list_manifests()
        .into_iter()
        .filter(|manifest| {
            manifest.session.id != caller.session.id
                && effective_group_id(manifest, &known_projects) == group_id
                && security.permits_manifest(Some(&caller), manifest)
        })
        .count();

    let result = json!({
        "current_session": session_context_json(&caller, &activity),
        "group": {
            "id": group_id,
            "peer_count": group_peer_count,
        },
        "access": {
            "can_read_project_sessions": grant.effective_scope() != McpScope::Off,
            "can_create_sessions": false,
            "can_control_group": group_peer_count > 0,
        },
    });

    serde_json::to_string_pretty(&result)
        .map_err(|e| format!("Failed to encode current-session context: {e}"))
}

fn tool_add_to_gallery(args: &Value) -> Result<String, String> {
    let caller = caller_manifest().ok_or_else(|| {
        "The calling session is unknown, so its gallery cannot be resolved.".to_string()
    })?;
    let raw_path = required_str(args, "path")?.trim();
    let source = std::path::PathBuf::from(raw_path);
    let source = if source.is_absolute() {
        source
    } else {
        std::path::Path::new(&caller.cwd).join(source)
    };
    let published = crate::session_artifacts::publish_local_image(&caller.session.id, &source)?;
    Ok(format!(
        "Added {} to this session's gallery ({} bytes, {}): {}",
        published.name,
        published.size,
        published.content_type,
        published.path.display()
    ))
}

fn tool_list_sessions(_args: &Value) -> Result<String, String> {
    let self_id = self_session_id();
    let security = load_security();
    let caller = caller_manifest();
    let activity = load_activity_state();
    let known_projects = known_project_ids();
    let caller_group = caller
        .as_ref()
        .map(|manifest| effective_group_id(manifest, &known_projects));
    let mut manifests = session_host::list_manifests();
    // Show only running sessions the caller is actually allowed to reach
    // (same project tree when its reach is 'project'). This is the same gate
    // require_session enforces on writes.
    manifests.retain(|manifest| {
        manifest.state == HostedSessionState::Running
            && security.permits_manifest(caller.as_ref(), manifest)
    });
    manifests.sort_by(|a, b| b.session.created_at.cmp(&a.session.created_at));

    let sessions: Vec<Value> = manifests
        .iter()
        .map(|manifest| {
            let session = &manifest.session;
            let activity_entry = activity_entry_for(&activity, &session.id);
            let group_id = effective_group_id(manifest, &known_projects);
            let relation = if self_id.as_deref() == Some(session.id.as_str()) {
                "self"
            } else if caller_group.as_deref() == Some(group_id.as_str()) {
                "group"
            } else {
                "other"
            };
            json!({
                "id": session.id,
                "label": session.label,
                "provider": provider_label_for_command(&session.command),
                "transcript": transcript_status_hint(manifest),
                "activity_status": activity_status_for_manifest(&activity, manifest),
                "raw_status": activity_entry.and_then(|entry| entry.raw_status.as_deref()),
                "unread": activity_entry.map(|entry| entry.unread).unwrap_or(false),
                "completed": activity_entry.map(|entry| entry.completed).unwrap_or(false),
                "command": session.command,
                "project_id": session.project_id,
                "group_id": group_id,
                "cwd": manifest.cwd,
                "worktree_branch": session.worktree_branch,
                "relation_to_caller": relation,
                "can_control": relation == "group",
                "spawned_by": session.spawned_by,
                "role": session.role,
                "task": session.task,
                "created_at": session.created_at,
                "self": self_id.as_deref() == Some(session.id.as_str()),
            })
        })
        .collect();

    serde_json::to_string_pretty(&json!({ "sessions": sessions }))
        .map_err(|e| format!("Failed to encode session list: {e}"))
}

fn tool_inspect_session(args: &Value) -> Result<String, String> {
    let session_id = required_str(args, "session_id")?;
    require_session(session_id, WriteAccess::Read)?;
    let manifest = load_manifest(session_id).ok_or_else(|| {
        format!("Unknown session id '{session_id}'. Use list_sessions to find valid targets.")
    })?;
    let session = &manifest.session;
    let provider = provider_label_for_command(&session.command);
    let is_self = self_session_id().as_deref() == Some(session.id.as_str());
    let activity = load_activity_state();
    let activity_entry = activity_entry_for(&activity, &session.id);
    let activity_status = activity_status_for_manifest(&activity, &manifest);

    let mut out = Vec::new();
    out.push(format!(
        "session id={} provider={} state={} self={} transcript={}",
        session.id,
        provider,
        hosted_session_state_label(manifest.state),
        is_self,
        transcript_status_hint(&manifest)
    ));
    out.push(format!(
        "activity={} raw_status={} unread={} completed={}",
        activity_status,
        activity_entry
            .and_then(|entry| entry.raw_status.as_deref())
            .unwrap_or("unknown"),
        activity_entry.map(|entry| entry.unread).unwrap_or(false),
        activity_entry.map(|entry| entry.completed).unwrap_or(false)
    ));
    out.push(format!(
        "label={} cwd={}",
        compact_one_line(&session.label, INSPECT_LINE_MAX_CHARS),
        manifest.cwd
    ));
    if let Some(branch) = session
        .worktree_branch
        .as_deref()
        .filter(|branch| !branch.trim().is_empty())
    {
        out.push(format!("worktree_branch={branch}"));
    }
    out.push(format!(
        "group_id={}",
        effective_group_id(&manifest, &known_project_ids())
    ));
    if session.spawned_by.is_some() || session.role.is_some() || session.task.is_some() {
        out.push(format!(
            "metadata spawned_by={} role={} task={}",
            session.spawned_by.as_deref().unwrap_or("none"),
            session.role.as_deref().unwrap_or("none"),
            compact_one_line(
                session.task.as_deref().unwrap_or("none"),
                INSPECT_LINE_MAX_CHARS
            )
        ));
    }

    let (screen_text, screen_tail) = match session_host::request_current_viewport_snapshot(
        session_id,
        0,
        Some(INSPECT_SCREEN_ROWS),
    ) {
        Ok(snapshot) if snapshot.cols <= 2 && snapshot.rows <= 2 => (
            String::new(),
            vec!["unavailable: older host cannot serve screen snapshots".to_string()],
        ),
        Ok(snapshot) => {
            let text = snapshot_screen_text(&snapshot);
            let tail = compact_tail_lines(&text, INSPECT_SCREEN_ROWS as usize);
            (text, tail)
        }
        Err(error) => (
            String::new(),
            vec![format!("unavailable: {}", compact_one_line(&error, 180))],
        ),
    };

    out.push("screen_tail:".to_string());
    if screen_tail.is_empty() {
        out.push("(empty)".to_string());
    } else {
        out.extend(screen_tail);
    }

    let mut transcript_has_entries = false;
    match resolve_provider_transcript(&manifest) {
        Ok(transcript) => {
            out.push(format!(
                "transcript_source={} provider_session={}",
                transcript.source,
                transcript
                    .provider_session_id
                    .as_deref()
                    .unwrap_or("unknown")
            ));
            match read_transcript_snapshot(&manifest, INSPECT_TRANSCRIPT_ENTRIES, false, None) {
                Ok(snapshot) => {
                    let compact =
                        compact_transcript_entries(&snapshot.entries, INSPECT_TRANSCRIPT_ENTRIES);
                    transcript_has_entries = !compact.is_empty();
                    if transcript_has_entries {
                        out.push("transcript_tail:".to_string());
                        out.extend(compact);
                    }
                }
                Err(error) => out.push(format!(
                    "transcript_unreadable={}",
                    compact_one_line(&error, 180)
                )),
            }
        }
        Err(error) if transcript_provider_for_command(&manifest.session.command).is_some() => {
            out.push(format!(
                "transcript_unavailable={}",
                compact_one_line(&error, 180)
            ));
        }
        Err(_) => {}
    }

    out.push(format!(
        "next={}",
        inspect_next_step(&manifest, &screen_text, transcript_has_entries)
    ));
    Ok(out.join("\n"))
}

fn tool_read_screen(args: &Value) -> Result<String, String> {
    let session_id = required_str(args, "session_id")?;
    require_session(session_id, WriteAccess::Read)?;
    let scroll_offset_rows = args
        .get("scroll_offset_rows")
        .and_then(Value::as_u64)
        .unwrap_or(0) as u32;
    let rows = args
        .get("rows")
        .and_then(Value::as_u64)
        .map(|value| (value.min(READ_SCREEN_MAX_ROWS as u64)).max(1) as u16);

    let snapshot =
        session_host::request_current_viewport_snapshot(session_id, scroll_offset_rows, rows)?;
    if snapshot.cols <= 2 && snapshot.rows <= 2 {
        // Hosts spawned by older Unpeel builds treat cols=0/rows=0 as a real
        // resize instead of "keep current size" and end up with a 1x1 grid.
        return Err(format!(
            "Session '{session_id}' is hosted by an older Unpeel build that cannot \
serve screen snapshots. Use read_output for this session, or restart it from Unpeel."
        ));
    }

    let body = snapshot_screen_text(&snapshot);
    Ok(format!(
        "screen {}x{} cursor=({},{}) scrollback_rows={} scroll_offset={}\n{}\n{}",
        snapshot.cols,
        snapshot.rows,
        snapshot.cursor_row,
        snapshot.cursor_col,
        snapshot.scrollback_rows,
        snapshot.scroll_offset_rows,
        "-".repeat(40),
        body.trim_end(),
    ))
}

fn tool_read_output(args: &Value) -> Result<String, String> {
    let session_id = required_str(args, "session_id")?;
    require_session(session_id, WriteAccess::Read)?;
    let tail_bytes = args
        .get("tail_bytes")
        .and_then(Value::as_u64)
        .map(|value| value as usize)
        .unwrap_or(READ_OUTPUT_DEFAULT_TAIL_BYTES)
        .clamp(1, READ_OUTPUT_MAX_TAIL_BYTES);
    let strip = args
        .get("strip_ansi")
        .and_then(Value::as_bool)
        .unwrap_or(true);

    let chunk =
        session_host::read_output_chunk(session_id, None, Some(tail_bytes), Some(tail_bytes))?;
    let text = String::from_utf8_lossy(&chunk.data);
    let text = if strip {
        strip_ansi(&text)
    } else {
        text.to_string()
    };
    Ok(format!(
        "output tail ({} bytes read, session {}):\n{}",
        chunk.data.len(),
        if chunk.exited { "exited" } else { "running" },
        text
    ))
}

fn tool_read_transcript(args: &Value) -> Result<String, String> {
    let session_id = required_str(args, "session_id")?;
    require_session(session_id, WriteAccess::Read)?;
    // The app-wide transcript settings are the defaults; explicit tool args win.
    let mut opts = load_transcript_settings();
    if let Some(n) = args.get("entries").and_then(Value::as_u64) {
        opts.max_entries = (n as usize).clamp(1, READ_TRANSCRIPT_MAX_ENTRIES);
    } else if opts.max_entries == 0 || opts.max_entries > READ_TRANSCRIPT_MAX_ENTRIES {
        // Whole-conversation (or oversized) defaults are capped for MCP reads,
        // which are meant to be a compact tail rather than a full export.
        opts.max_entries = READ_TRANSCRIPT_DEFAULT_ENTRIES;
    }
    if let Some(include_tools) = args.get("include_tools").and_then(Value::as_bool) {
        opts.include_tools = include_tools;
    }
    let entries = opts.max_entries;
    let collect_tools = opts.include_tools
        || opts.include_reasoning
        || opts.include_file_changes
        || opts.include_plan_updates;
    let manifest = load_manifest(session_id).ok_or_else(|| {
        format!("Unknown session id '{session_id}'. Use list_sessions to find valid targets.")
    })?;
    let snapshot = read_transcript_snapshot(&manifest, entries, collect_tools, None)?;
    if snapshot.entries.is_empty() {
        return Err(format!(
            "Found {} transcript at {}, but no readable user/assistant entries were found.",
            snapshot.provider, snapshot.path
        ));
    }

    let body = format_transcript_markdown(&snapshot, &opts);
    Ok(format!(
        "transcript provider={} source={} session={} path={}\n{}\n{}",
        snapshot.provider,
        snapshot.source,
        snapshot.provider_session_id.as_deref().unwrap_or("unknown"),
        snapshot.path,
        "-".repeat(40),
        body.trim_end()
    ))
}

fn hosted_session_state_label(state: HostedSessionState) -> &'static str {
    match state {
        HostedSessionState::Running => "running",
        HostedSessionState::Exited => "exited",
    }
}
fn truncate_text(text: &str, max_chars: usize) -> String {
    let collapsed = text.trim();
    if collapsed.chars().count() <= max_chars {
        return collapsed.to_string();
    }
    let mut out = collapsed
        .chars()
        .take(max_chars.saturating_sub(3))
        .collect::<String>();
    out.push_str("...");
    out
}

fn compact_one_line(text: &str, max_chars: usize) -> String {
    truncate_text(
        &text.split_whitespace().collect::<Vec<_>>().join(" "),
        max_chars,
    )
}

fn compact_tail_lines(text: &str, max_lines: usize) -> Vec<String> {
    let lines: Vec<String> = text
        .lines()
        .map(str::trim_end)
        .filter(|line| !line.trim().is_empty())
        .map(|line| truncate_text(line, INSPECT_LINE_MAX_CHARS))
        .collect();
    let start = lines.len().saturating_sub(max_lines);
    lines[start..].to_vec()
}

fn compact_transcript_entries(entries: &[TranscriptEntry], max_entries: usize) -> Vec<String> {
    let start = entries.len().saturating_sub(max_entries);
    entries[start..]
        .iter()
        .map(|entry| {
            format!(
                "[{}] {}",
                entry.role,
                compact_one_line(&entry.text, INSPECT_LINE_MAX_CHARS)
            )
        })
        .collect()
}

fn inspect_next_step(
    manifest: &HostedSessionManifest,
    screen_text: &str,
    transcript_has_entries: bool,
) -> &'static str {
    if screen_suggests_interaction(screen_text) {
        return "read_screen rows=40, then send_keys/send_text if input is needed";
    }
    if transcript_has_entries {
        return "read_transcript entries=5 include_tools=false for more history";
    }
    if transcript_provider_for_command(&manifest.session.command).is_some() {
        return "read_transcript entries=5 include_tools=false if you need history, otherwise read_screen rows=20";
    }
    if manifest.state == HostedSessionState::Exited {
        return "read_output tail_bytes=12000";
    }
    "read_screen rows=20"
}

fn screen_suggests_interaction(screen_text: &str) -> bool {
    let lower = screen_text.to_lowercase();
    [
        "permission",
        "approval",
        "allow",
        "deny",
        "do you want",
        "would you like",
        "continue?",
        "proceed?",
        "press enter",
        "arrow keys",
        "[y/n]",
        "(y/n)",
        "yes/no",
    ]
    .iter()
    .any(|needle| lower.contains(needle))
}

/// Render a viewport snapshot to the plain-text screen form shared by
/// read_screen and wait_for_text (right-trimmed rows, newline-joined).
fn snapshot_screen_text(snapshot: &crate::terminal_viewport::TerminalViewportSnapshot) -> String {
    let mut body = String::new();
    for row in &snapshot.viewport_rows {
        body.push_str(row.text.trim_end());
        body.push('\n');
    }
    body
}

/// First line of `screen` containing `needle` under the given case rule.
/// `needle` must already be lowercased when `case_sensitive` is false.
fn find_matching_line<'a>(screen: &'a str, needle: &str, case_sensitive: bool) -> Option<&'a str> {
    screen.lines().find(|line| {
        if case_sensitive {
            line.contains(needle)
        } else {
            line.to_lowercase().contains(needle)
        }
    })
}

fn tool_wait_for_text(args: &Value) -> Result<String, String> {
    let session_id = required_str(args, "session_id")?;
    let needle = required_str(args, "text")?;
    require_session(session_id, WriteAccess::Read)?;
    let case_sensitive = args
        .get("case_sensitive")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let timeout_ms = args
        .get("timeout_ms")
        .and_then(Value::as_u64)
        .unwrap_or(WAIT_DEFAULT_TIMEOUT_MS)
        .clamp(WAIT_MIN_TIMEOUT_MS, WAIT_MAX_TIMEOUT_MS);

    let needle_cmp = if case_sensitive {
        needle.to_string()
    } else {
        needle.to_lowercase()
    };

    let started = std::time::Instant::now();
    let mut last_screen = String::new();
    loop {
        match session_host::request_current_viewport_snapshot(session_id, 0, None) {
            Ok(snapshot) => {
                let screen = snapshot_screen_text(&snapshot);
                if let Some(line) = find_matching_line(&screen, &needle_cmp, case_sensitive) {
                    return Ok(format!(
                        "Found {needle:?} after {}ms on session {session_id}:\n{}",
                        started.elapsed().as_millis(),
                        line.trim_end(),
                    ));
                }
                last_screen = screen;
            }
            Err(error) => {
                // A snapshot failure usually means the host died mid-wait;
                // report the exit instead of spinning out the full timeout.
                match load_manifest(session_id) {
                    Some(manifest) if manifest.state == HostedSessionState::Running => {
                        // Transient (e.g. socket busy) — keep waiting.
                    }
                    Some(_) => {
                        return Err(format!(
                            "Session '{session_id}' exited before {needle:?} appeared \
(waited {}ms).",
                            started.elapsed().as_millis()
                        ));
                    }
                    None => {
                        return Err(format!(
                            "Session '{session_id}' disappeared while waiting: {error}"
                        ));
                    }
                }
            }
        }

        if started.elapsed().as_millis() as u64 >= timeout_ms {
            let tail: Vec<&str> = {
                let lines: Vec<&str> = last_screen
                    .lines()
                    .filter(|line| !line.trim().is_empty())
                    .collect();
                let start = lines.len().saturating_sub(WAIT_TIMEOUT_REPORT_LINES);
                lines[start..].to_vec()
            };
            return Err(format!(
                "Timed out after {timeout_ms}ms waiting for {needle:?} on session \
{session_id}. Final screen tail:\n{}",
                tail.join("\n")
            ));
        }
        thread::sleep(Duration::from_millis(WAIT_POLL_INTERVAL_MS));
    }
}

fn tool_wait_for_status(args: &Value) -> Result<String, String> {
    let session_id = required_str(args, "session_id")?;
    let desired = required_str(args, "status")?;
    if !valid_activity_status(desired) {
        return Err(format!(
            "Unsupported status {desired:?}; expected one of starting, working, blocked, done, idle, exited, unknown"
        ));
    }
    require_session(session_id, WriteAccess::Read)?;
    let timeout_ms = args
        .get("timeout_ms")
        .and_then(Value::as_u64)
        .unwrap_or(WAIT_DEFAULT_TIMEOUT_MS)
        .clamp(WAIT_MIN_TIMEOUT_MS, WAIT_MAX_TIMEOUT_MS);

    let started = std::time::Instant::now();
    loop {
        let manifest = load_manifest(session_id).ok_or_else(|| {
            format!("Session '{session_id}' disappeared while waiting for status {desired:?}")
        })?;
        let activity = load_activity_state();
        let current = activity_status_for_manifest(&activity, &manifest);
        if status_matches(&current, desired) {
            let entry = activity_entry_for(&activity, session_id);
            return Ok(format!(
                "Session {session_id} reached status {current:?} after {}ms (unread={}, completed={}).",
                started.elapsed().as_millis(),
                entry.map(|entry| entry.unread).unwrap_or(false),
                entry.map(|entry| entry.completed).unwrap_or(false)
            ));
        }

        if started.elapsed().as_millis() as u64 >= timeout_ms {
            return Err(format!(
                "Timed out after {timeout_ms}ms waiting for session {session_id} to reach status {desired:?}; current status is {current:?}."
            ));
        }
        thread::sleep(Duration::from_millis(WAIT_POLL_INTERVAL_MS));
    }
}

fn tool_send_text(args: &Value) -> Result<String, String> {
    let session_id = required_str(args, "session_id")?;
    let text = required_str(args, "text")?;
    let submit = args.get("submit").and_then(Value::as_bool).unwrap_or(true);
    require_session(session_id, WriteAccess::Write)?;
    let target = load_manifest(session_id);

    let sanitized = sanitize_paste_text(text);
    if sanitized.is_empty() && !submit {
        return Err("Nothing to send: text is empty after removing control characters".into());
    }

    let envelope = if sanitized.is_empty() {
        None
    } else {
        send_text_envelope(caller_manifest().as_ref(), target.as_ref())
    };
    let delivered = match envelope.as_deref() {
        Some(header) => format!("{header}\n{sanitized}"),
        None => sanitized.clone(),
    };
    deliver_text_to_terminal(session_id, &delivered, submit)?;
    Ok(format!(
        "Sent {} characters to session {}{}{}",
        sanitized.chars().count(),
        session_id,
        if submit { " and pressed Enter" } else { "" },
        if envelope.is_some() {
            " (prefixed with your sender envelope so the receiving agent knows who is talking)"
        } else {
            ""
        }
    ))
}

fn tool_send_keys(args: &Value) -> Result<String, String> {
    let session_id = required_str(args, "session_id")?;
    let keys = args
        .get("keys")
        .and_then(Value::as_array)
        .ok_or("send_keys requires a 'keys' array")?;
    if keys.is_empty() {
        return Err("send_keys requires at least one key".into());
    }
    if keys.len() > MAX_KEYS_PER_CALL {
        return Err(format!(
            "send_keys accepts at most {MAX_KEYS_PER_CALL} keys per call"
        ));
    }
    let delay_ms = args
        .get("delay_ms")
        .and_then(Value::as_u64)
        .unwrap_or(KEY_DELAY_DEFAULT_MS)
        .min(KEY_DELAY_MAX_MS);

    let mut sequences = Vec::with_capacity(keys.len());
    for key in keys {
        let key = key
            .as_str()
            .ok_or("send_keys 'keys' entries must be strings")?;
        let sequence = key_sequence(key).ok_or_else(|| format!("Unsupported key: {key:?}"))?;
        sequences.push(sequence);
    }

    require_session(session_id, WriteAccess::Write)?;
    for (index, sequence) in sequences.iter().enumerate() {
        if index > 0 && delay_ms > 0 {
            thread::sleep(Duration::from_millis(delay_ms));
        }
        write_to_session(session_id, sequence)?;
    }
    Ok(format!(
        "Sent {} keys to session {}",
        sequences.len(),
        session_id
    ))
}

fn tool_list_group(args: &Value) -> Result<String, String> {
    let include_exited = args
        .get("include_exited")
        .and_then(Value::as_bool)
        .unwrap_or(true);
    let peers = group_peer_manifests_for_caller(include_exited, None)?;
    let activity = load_activity_state();
    let sessions: Vec<Value> = peers
        .iter()
        .map(|manifest| group_peer_status_json(manifest, &activity))
        .collect();
    let caller = caller_manifest().ok_or_else(|| {
        "The calling session is unknown, so its group cannot be resolved.".to_string()
    })?;
    let group_id = effective_group_id(&caller, &known_project_ids());
    serde_json::to_string_pretty(&json!({ "group_id": group_id, "sessions": sessions }))
        .map_err(|e| format!("Failed to encode group list: {e}"))
}

fn tool_wait_for_group(args: &Value) -> Result<String, String> {
    let only_ids = optional_string_set(args, "session_ids")?;
    let timeout_ms = args
        .get("timeout_ms")
        .and_then(Value::as_u64)
        .unwrap_or(WAIT_DEFAULT_TIMEOUT_MS)
        .clamp(WAIT_MIN_TIMEOUT_MS, WAIT_MAX_TIMEOUT_MS);
    let started = Instant::now();

    loop {
        let peers = group_peer_manifests_for_caller(true, only_ids.as_ref())?;
        if peers.is_empty() {
            return Err("No other sessions found in the calling session's group.".into());
        }
        let activity = load_activity_state();
        let statuses: Vec<Value> = peers
            .iter()
            .map(|manifest| group_peer_status_json(manifest, &activity))
            .collect();
        let complete = statuses.iter().all(|status| {
            status
                .get("activity_status")
                .and_then(Value::as_str)
                .map(group_status_is_terminal)
                .unwrap_or(false)
        });
        if complete {
            return serde_json::to_string_pretty(&json!({
                "complete": true,
                "elapsed_ms": started.elapsed().as_millis(),
                "sessions": statuses,
            }))
            .map_err(|e| format!("Failed to encode delegate wait result: {e}"));
        }
        if started.elapsed().as_millis() as u64 >= timeout_ms {
            let body = serde_json::to_string_pretty(&json!({
                "complete": false,
                "elapsed_ms": started.elapsed().as_millis(),
                "sessions": statuses,
            }))
            .map_err(|e| format!("Failed to encode delegate wait timeout: {e}"))?;
            return Err(format!(
                "Timed out after {timeout_ms}ms waiting for group sessions to finish (idle/done), block, or exit.\n{body}"
            ));
        }
        thread::sleep(Duration::from_millis(WAIT_POLL_INTERVAL_MS));
    }
}

fn tool_summarize_group(args: &Value) -> Result<String, String> {
    let only_ids = optional_string_set(args, "session_ids")?;
    let entries = args
        .get("entries")
        .and_then(Value::as_u64)
        .map(|value| value as usize)
        .unwrap_or(DELEGATE_SUMMARY_DEFAULT_ENTRIES)
        .clamp(1, DELEGATE_SUMMARY_MAX_ENTRIES);
    let peers = group_peer_manifests_for_caller(true, only_ids.as_ref())?;
    if peers.is_empty() {
        return Err("No other sessions found in the calling session's group.".into());
    }
    let activity = load_activity_state();
    let mut out = Vec::new();
    out.push("group_sessions:".to_string());
    for manifest in peers {
        let session = &manifest.session;
        out.push(format!(
            "- id={} role={} activity={} state={} transcript={}",
            session.id,
            session.role.as_deref().unwrap_or("delegate"),
            activity_status_for_manifest(&activity, &manifest),
            hosted_session_state_label(manifest.state),
            transcript_status_hint(&manifest)
        ));
        out.push(format!(
            "  task={}",
            compact_one_line(session.task.as_deref().unwrap_or(&session.label), 320)
        ));
        match read_transcript_snapshot(&manifest, entries, false, None) {
            Ok(snapshot) if !snapshot.entries.is_empty() => {
                out.push(format!(
                    "  transcript_tail provider={} source={} session={}:",
                    snapshot.provider,
                    snapshot.source,
                    snapshot.provider_session_id.as_deref().unwrap_or("unknown")
                ));
                for entry in compact_transcript_entries(&snapshot.entries, entries) {
                    out.push(format!("  {entry}"));
                }
            }
            _ => match session_host::request_current_viewport_snapshot(
                &session.id,
                0,
                Some(DELEGATE_SCREEN_FALLBACK_ROWS),
            ) {
                Ok(snapshot) if snapshot.cols > 2 || snapshot.rows > 2 => {
                    let text = snapshot_screen_text(&snapshot);
                    let tail = compact_tail_lines(&text, DELEGATE_SCREEN_FALLBACK_ROWS as usize);
                    out.push("  screen_tail:".to_string());
                    if tail.is_empty() {
                        out.push("  (empty)".to_string());
                    } else {
                        for line in tail {
                            out.push(format!("  {line}"));
                        }
                    }
                }
                _ => {
                    match session_host::read_output_chunk(
                        &session.id,
                        None,
                        Some(READ_OUTPUT_DEFAULT_TAIL_BYTES),
                        Some(READ_OUTPUT_DEFAULT_TAIL_BYTES),
                    ) {
                        Ok(chunk) if !chunk.data.is_empty() => {
                            let text = strip_ansi(&String::from_utf8_lossy(&chunk.data));
                            let tail =
                                compact_tail_lines(&text, DELEGATE_SCREEN_FALLBACK_ROWS as usize);
                            out.push("  output_tail:".to_string());
                            if tail.is_empty() {
                                out.push("  (empty)".to_string());
                            } else {
                                for line in tail {
                                    out.push(format!("  {line}"));
                                }
                            }
                        }
                        _ => {
                            out.push(
                                "  summary_unavailable=transcript, screen tail, and output tail unavailable"
                                    .into(),
                            );
                        }
                    }
                }
            },
        }
    }
    Ok(out.join("\n"))
}

fn tool_report_to_group(args: &Value) -> Result<String, String> {
    let caller = caller_manifest().ok_or_else(|| {
        "report_to_group requires the calling session to have a manifest.".to_string()
    })?;
    let target_session_id = required_str(args, "session_id")?;
    let target = load_manifest(target_session_id)
        .ok_or_else(|| format!("Group session '{target_session_id}' is no longer available."))?;
    if !caller_shares_group(Some(&caller), &target) {
        return Err(format!(
            "Session '{target_session_id}' is not in the caller's current group; use send_text \
for a cross-group message (subject to the user's write policy)."
        ));
    }
    require_session(target_session_id, WriteAccess::Write)?;
    let report = build_group_report(args, &caller, &target)?;
    let submit = args.get("submit").and_then(Value::as_bool).unwrap_or(true);
    send_initial_text_to_session(target_session_id, &report, submit)?;
    Ok(format!(
        "Reported {} characters to group session {}{}",
        sanitize_paste_text(&report).chars().count(),
        target_session_id,
        if submit { " and pressed Enter" } else { "" }
    ))
}

fn tool_list_presets(args: &Value) -> Result<String, String> {
    let project_id = resolve_project_id(args)?;
    let response = app_request("/mcp/list-presets", &json!({ "project_id": project_id }))?;
    serde_json::to_string_pretty(&response)
        .map_err(|e| format!("Failed to encode preset list: {e}"))
}

fn tool_close_session(args: &Value) -> Result<String, String> {
    let session_id = required_str(args, "session_id")?;
    if self_session_id().as_deref() == Some(session_id) {
        return Err("Refusing to close the calling session's own terminal \
(that would kill the agent that issued this tool call)."
            .into());
    }
    let manifest = load_manifest(session_id).ok_or_else(|| {
        format!("Unknown session id '{session_id}'. Use list_sessions to find valid targets.")
    })?;
    let security = load_security();
    let caller = caller_manifest();
    if !security.permits_manifest(caller.as_ref(), &manifest) {
        return Err(read_denied_message());
    }
    // Closing is group-scoped and never falls through to the cross-group
    // approval policy: moving a session is the explicit user-owned grant.
    if !caller_shares_group(caller.as_ref(), &manifest) {
        return Err(close_denied_message());
    }
    app_request("/mcp/close-session", &json!({ "session_id": session_id }))?;
    Ok(format!("Closed session {session_id}"))
}

/// Project to operate on: explicit argument, else the calling session's own
/// project (resolved from its manifest).
fn resolve_project_id(args: &Value) -> Result<String, String> {
    if let Some(project_id) = args
        .get("project_id")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
    {
        return Ok(project_id.to_string());
    }
    caller_manifest()
        .map(|manifest| manifest.session.project_id)
        .ok_or_else(|| {
            "No project_id given and the calling session has no manifest; pass project_id \
explicitly"
                .to_string()
        })
}

fn build_group_report(
    args: &Value,
    caller: &HostedSessionManifest,
    target: &HostedSessionManifest,
) -> Result<String, String> {
    let summary = required_str(args, "summary")?.trim();
    let status = optional_trimmed_str(args, "status").unwrap_or("update");
    if !matches!(status, "update" | "done" | "blocked") {
        return Err("report_to_group status must be one of: update, done, blocked".into());
    }

    let proof = optional_string_list(args, "proof")?;
    let changed_paths = optional_string_list(args, "changed_paths")?;
    let artifacts = optional_string_list(args, "artifacts")?;
    let blockers = optional_string_list(args, "blockers")?;
    let questions = optional_string_list(args, "questions")?;
    let next_steps = optional_string_list(args, "next_steps")?;

    let mut out = Vec::new();
    out.push("Group session report.".to_string());
    out.push(String::new());
    out.push(format!("Status: {status}"));
    out.push(format!("From session: {}", caller.session.id));
    out.push(format!("From label: {}", caller.session.label));
    out.push(format!("To session: {}", target.session.id));
    if let Some(role) = caller.session.role.as_deref() {
        out.push(format!("Role: {role}"));
    }
    if let Some(task) = caller.session.task.as_deref() {
        out.push(format!("Task: {task}"));
    }

    push_text_section(&mut out, "Summary", summary);
    if let Some(details) = optional_trimmed_str(args, "details") {
        push_text_section(&mut out, "Details", details);
    }
    push_list_section(&mut out, "Proof", &proof);
    push_list_section(&mut out, "Changed paths", &changed_paths);
    push_list_section(&mut out, "Artifacts", &artifacts);
    push_list_section(&mut out, "Blockers", &blockers);
    push_list_section(&mut out, "Questions", &questions);
    push_list_section(&mut out, "Next steps", &next_steps);
    Ok(out.join("\n"))
}

fn push_text_section(out: &mut Vec<String>, title: &str, body: &str) {
    out.push(String::new());
    out.push(format!("{title}:"));
    out.push(body.trim().to_string());
}

fn push_list_section(out: &mut Vec<String>, title: &str, items: &[String]) {
    if items.is_empty() {
        return;
    }
    out.push(String::new());
    out.push(format!("{title}:"));
    for item in items {
        out.push(format!("- {item}"));
    }
}

fn optional_string_list(args: &Value, key: &str) -> Result<Vec<String>, String> {
    let Some(value) = args.get(key) else {
        return Ok(Vec::new());
    };
    let items = value
        .as_array()
        .ok_or_else(|| format!("'{key}' must be an array of strings"))?;
    let mut result = Vec::new();
    for item in items {
        let text = item
            .as_str()
            .ok_or_else(|| format!("'{key}' entries must be strings"))?
            .trim();
        if !text.is_empty() {
            result.push(text.to_string());
        }
    }
    Ok(result)
}

fn optional_string_set(args: &Value, key: &str) -> Result<Option<HashSet<String>>, String> {
    if args.get(key).is_none() {
        return Ok(None);
    }
    Ok(Some(optional_string_list(args, key)?.into_iter().collect()))
}

fn group_peer_manifests_for_caller(
    include_exited: bool,
    only_ids: Option<&HashSet<String>>,
) -> Result<Vec<HostedSessionManifest>, String> {
    let self_id = self_session_id().ok_or_else(|| {
        "The calling session is unknown, so its group cannot be resolved.".to_string()
    })?;
    let security = load_security();
    let caller = caller_manifest().ok_or_else(|| {
        "The calling session is unknown, so its group cannot be resolved.".to_string()
    })?;
    let known_projects = known_project_ids();
    let caller_group = effective_group_id(&caller, &known_projects);
    let mut peers: Vec<HostedSessionManifest> = session_host::list_manifests()
        .into_iter()
        .filter(|manifest| {
            manifest.session.id != self_id
                && effective_group_id(manifest, &known_projects) == caller_group
                && (include_exited || manifest.state == HostedSessionState::Running)
                && only_ids
                    .map(|ids| ids.contains(&manifest.session.id))
                    .unwrap_or(true)
                && security.permits_manifest(Some(&caller), manifest)
        })
        .collect();
    peers.sort_by(|a, b| b.session.created_at.cmp(&a.session.created_at));

    if let Some(only_ids) = only_ids {
        let found: HashSet<String> = peers
            .iter()
            .map(|manifest| manifest.session.id.clone())
            .collect();
        let missing: Vec<String> = only_ids
            .iter()
            .filter(|id| !found.contains(*id))
            .cloned()
            .collect();
        if !missing.is_empty() {
            return Err(format!(
                "These session_ids are not peers in the caller's current group: {}",
                missing.join(", ")
            ));
        }
    }

    Ok(peers)
}

fn group_peer_status_json(
    manifest: &HostedSessionManifest,
    activity: &HashMap<String, ActivityStateEntry>,
) -> Value {
    let session = &manifest.session;
    let activity_entry = activity_entry_for(activity, &session.id);
    json!({
        "id": session.id,
        "label": session.label,
        "provider": provider_label_for_command(&session.command),
        "transcript": transcript_status_hint(manifest),
        "activity_status": activity_status_for_manifest(activity, manifest),
        "raw_status": activity_entry.and_then(|entry| entry.raw_status.as_deref()),
        "unread": activity_entry.map(|entry| entry.unread).unwrap_or(false),
        "completed": activity_entry.map(|entry| entry.completed).unwrap_or(false),
        "state": hosted_session_state_label(manifest.state),
        "command": session.command,
        "project_id": session.project_id,
        "group_id": effective_group_id(manifest, &known_project_ids()),
        "cwd": manifest.cwd,
        "worktree_branch": session.worktree_branch,
        "created_at": session.created_at,
        "spawned_by": session.spawned_by,
        "role": session.role,
        "task": session.task,
    })
}

fn group_status_is_terminal(status: &str) -> bool {
    // `idle` is the same settled state as `done` (see `status_matches`): a
    // peer that finished its turn while observed reports `idle`, so a wait
    // that omitted it would hang. Both count as "finished".
    matches!(status, "done" | "idle" | "blocked" | "exited")
}

/// POST a JSON payload to the desktop app's local bridge and return the JSON
/// response body. Hand-rolled HTTP/1.1 to match the hand-rolled server.
fn app_request(path: &str, payload: &Value) -> Result<Value, String> {
    app_request_with_timeout(path, payload, Duration::from_secs(20))
}

/// [`app_request`] with an explicit read timeout, for routes that wait on the
/// user (the write-approval dialog) rather than on the app.
pub(crate) fn app_request_with_timeout(
    path: &str,
    payload: &Value,
    read_timeout: Duration,
) -> Result<Value, String> {
    use std::io::{BufRead, BufReader, Read};
    use std::net::TcpStream;

    let ports = candidate_app_ports();
    if ports.is_empty() {
        return Err(
            "Unpeel desktop app is not reachable (no UNPEEL_APP_PORT and no ~/.unpeel/app-ports)"
                .into(),
        );
    }
    let token = std::fs::read_to_string(crate::mcp_auth::auth_token_path())
        .map_err(|e| format!("Failed to read MCP auth token: {e}"))?;
    let body = payload.to_string();

    // The session env can outlive an app restart, so the launch-time port may
    // be dead; fall back to the current instance's advertised port.
    let mut stream = None;
    let mut last_error = String::new();
    for candidate in &ports {
        match TcpStream::connect(("127.0.0.1", *candidate)) {
            Ok(connected) => {
                stream = Some((connected, *candidate));
                break;
            }
            Err(error) => {
                last_error =
                    format!("Unpeel desktop app is not reachable on port {candidate}: {error}");
            }
        }
    }
    let (mut stream, port) = stream.ok_or(last_error)?;
    stream
        .set_read_timeout(Some(read_timeout))
        .and_then(|_| stream.set_write_timeout(Some(Duration::from_secs(5))))
        .map_err(|e| format!("Failed to configure bridge connection: {e}"))?;

    let request = format!(
        "POST {path} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nContent-Type: application/json\r\n{}: {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        crate::mcp_auth::MCP_AUTH_HEADER,
        token.trim(),
        body.len(),
    );
    std::io::Write::write_all(&mut stream, request.as_bytes())
        .map_err(|e| format!("Failed to send bridge request: {e}"))?;

    let mut reader = BufReader::new(stream);
    let mut status_line = String::new();
    reader
        .read_line(&mut status_line)
        .map_err(|e| format!("Failed to read bridge response: {e}"))?;
    let status: u16 = status_line
        .split_whitespace()
        .nth(1)
        .and_then(|code| code.parse().ok())
        .ok_or_else(|| format!("Invalid bridge response: {status_line:?}"))?;

    let mut content_length: Option<usize> = None;
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line).is_err() || line.trim().is_empty() {
            break;
        }
        if let Some(value) = line.to_lowercase().strip_prefix("content-length:") {
            content_length = value.trim().parse().ok();
        }
    }
    let mut body = Vec::new();
    match content_length {
        Some(length) => {
            body.resize(length, 0);
            reader
                .read_exact(&mut body)
                .map_err(|e| format!("Failed to read bridge response body: {e}"))?;
        }
        None => {
            let _ = reader.read_to_end(&mut body);
        }
    }
    let response: Value =
        serde_json::from_slice(&body).map_err(|e| format!("Invalid bridge response body: {e}"))?;

    if status != 200 {
        let message = response
            .get("error")
            .and_then(Value::as_str)
            .unwrap_or("bridge request failed");
        return Err(format!(
            "Unpeel app rejected the request ({status}): {message}"
        ));
    }
    Ok(response)
}

fn candidate_app_ports() -> Vec<u16> {
    let mut ports = Vec::new();
    if let Ok(value) = std::env::var("UNPEEL_APP_PORT") {
        let trimmed = value.trim();
        if !trimmed.is_empty() && !trimmed.starts_with("${") {
            if let Ok(port) = trimmed.parse() {
                ports.push(port);
            }
        }
    }
    // The native app registers in the app-ports broadcast registry; try the
    // newest registration first.
    if let Ok(raw) = std::fs::read_to_string(crate::app_paths::unpeel_home().join("app-ports")) {
        for line in raw.lines().rev() {
            if let Ok(port) = line.trim().parse() {
                if !ports.contains(&port) {
                    ports.push(port);
                }
            }
        }
    }
    ports
}

#[derive(PartialEq)]
enum WriteAccess {
    Read,
    Write,
}

fn require_session(session_id: &str, access: WriteAccess) -> Result<(), String> {
    let manifest = load_manifest(session_id).ok_or_else(|| {
        format!("Unknown session id '{session_id}'. Use list_sessions to find valid targets.")
    })?;
    let security = load_security();
    let caller = caller_manifest();
    if !security.permits_manifest(caller.as_ref(), &manifest) {
        return Err(read_denied_message());
    }
    if access == WriteAccess::Write {
        if self_session_id().as_deref() == Some(session_id) {
            return Err("Refusing to write into the calling session's own terminal \
(that would type into the agent that issued this tool call). Target a different session."
                .into());
        }
        // Checked before the approval path so a dead target never shows the
        // user an approval dialog for input that could not be delivered.
        if manifest.state != HostedSessionState::Running {
            return Err(format!(
                "Session '{session_id}' has exited and cannot receive input."
            ));
        }
        // Sessions in the same effective sidebar group may always coordinate.
        // Anything else consults the app-wide policy: ask the user (remembering
        // an approval), deny, or allow.
        if !caller_shares_group(caller.as_ref(), &manifest) {
            use crate::state::McpNonChildWriteAccess as WritePolicy;
            match security.nonchild_write_access {
                WritePolicy::Allow => {}
                WritePolicy::Deny => return Err(write_denied_message()),
                WritePolicy::Ask => {
                    // permits_manifest already required a known caller.
                    let caller_id = caller
                        .as_ref()
                        .map(|manifest| manifest.session.id.clone())
                        .ok_or_else(read_denied_message)?;
                    if !security.write_pair_approved(&caller_id, session_id) {
                        request_write_approval(&caller_id, session_id)?;
                    }
                }
            }
        }
    }
    Ok(())
}

/// Ask the user — through the desktop app's approval dialog — to allow the
/// caller writing into a cross-group session. Blocks until the user answers
/// or the bridge times out (~2 minutes). On approval the app persists the
/// caller→target pair into `mcp_write_approvals`, so later writes to the same
/// pair pass without asking again.
fn request_write_approval(caller_id: &str, target_id: &str) -> Result<(), String> {
    let response = app_request_with_timeout(
        "/mcp/approve-write",
        &json!({
            "caller_session_id": caller_id,
            "target_session_id": target_id,
        }),
        Duration::from_secs(130),
    )
    .map_err(|error| {
        format!(
            "Writing to session '{target_id}' requires the user's approval, but the approval \
prompt did not complete: {error}. If the dialog is still open on the desktop, the user can \
answer it and you can retry once; otherwise ask the user to approve the write or to change \
Settings ▸ Sessions MCP ▸ Writing to other sessions."
        )
    })?;
    if response.get("approved").and_then(Value::as_bool) == Some(true) {
        return Ok(());
    }
    Err(
        "The user declined this write. Do not retry on your own — you can still read the \
session; ask the user if they want to approve future writes (or change Settings ▸ Sessions MCP \
▸ Writing to other sessions)."
            .into(),
    )
}

fn load_manifest(session_id: &str) -> Option<HostedSessionManifest> {
    if session_id.is_empty()
        || session_id.contains('/')
        || session_id.contains("..")
        || session_id.contains('\\')
    {
        return None;
    }
    let raw = std::fs::read(session_host::manifest_path(session_id)).ok()?;
    serde_json::from_slice(&raw).ok()
}

fn write_to_session(session_id: &str, data: &str) -> Result<(), String> {
    session_host::send_command(
        session_id,
        &SessionHostCommand::Write {
            data: data.to_string(),
            write_id: None,
            task_episode_receipt: None,
        },
    )
}

/// Single delivery choke point for typing an inter-session message into a
/// target session's terminal (the proven bracketed-paste + settle + double
/// Enter recipe). Terminal-to-terminal is today's only message channel; when
/// other channels exist (Slack↔terminal — see
/// `docs/feature/sessions-mcp-channels.md`), routing on the message's channel
/// happens above this function, and the sender/channel envelope is prepended
/// to `sanitized` before it reaches the paste. Callers pass already-sanitized
/// text (`sanitize_paste_text`) and must have passed the write gate.
fn deliver_text_to_terminal(session_id: &str, sanitized: &str, submit: bool) -> Result<(), String> {
    crate::session_input::deliver_sanitized_text(session_id, sanitized, submit)
}

fn send_initial_text_to_session(session_id: &str, text: &str, submit: bool) -> Result<(), String> {
    let sanitized = sanitize_paste_text(text);
    if sanitized.is_empty() && !submit {
        return Err("initial prompt is empty after removing control characters".into());
    }

    let started = Instant::now();
    loop {
        match require_session(session_id, WriteAccess::Write)
            .and_then(|_| deliver_text_to_terminal(session_id, &sanitized, submit))
        {
            Ok(()) => return Ok(()),
            Err(error) => {
                if started.elapsed().as_millis() as u64 >= START_MESSAGE_TIMEOUT_MS {
                    return Err(error);
                }
                thread::sleep(Duration::from_millis(START_MESSAGE_POLL_MS));
            }
        }
    }
}

pub(crate) fn self_session_id() -> Option<String> {
    let value = std::env::var("UNPEEL_SESSION_ID").ok()?;
    let trimmed = value.trim();
    // An unexpanded "${UNPEEL_SESSION_ID}" literal means the launcher did not
    // substitute the variable; treat it as unknown rather than a real id.
    if trimmed.is_empty() || trimmed.starts_with("${") {
        return None;
    }
    Some(trimmed.to_string())
}

fn required_str<'a>(args: &'a Value, key: &str) -> Result<&'a str, String> {
    args.get(key)
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| format!("Missing required string argument '{key}'"))
}

fn optional_trimmed_str<'a>(args: &'a Value, key: &str) -> Option<&'a str> {
    args.get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

fn key_sequence(key: &str) -> Option<String> {
    let normalized = key.trim().to_ascii_lowercase();
    let sequence = match normalized.as_str() {
        "enter" | "return" => "\r",
        "tab" => "\t",
        "shift+tab" | "backtab" => "\x1b[Z",
        "space" => " ",
        "esc" | "escape" => "\x1b",
        "up" => "\x1b[A",
        "down" => "\x1b[B",
        "right" => "\x1b[C",
        "left" => "\x1b[D",
        "home" => "\x1b[H",
        "end" => "\x1b[F",
        "pageup" | "page_up" => "\x1b[5~",
        "pagedown" | "page_down" => "\x1b[6~",
        "backspace" => "\x7f",
        "delete" | "del" => "\x1b[3~",
        _ => {
            if let Some(rest) = normalized.strip_prefix("ctrl+") {
                let mut chars = rest.chars();
                if let (Some(c), None) = (chars.next(), chars.next()) {
                    if c.is_ascii_lowercase() {
                        return Some(((c as u8 - b'a' + 1) as char).to_string());
                    }
                }
                return None;
            }
            let mut chars = key.chars();
            if let (Some(c), None) = (chars.next(), chars.next()) {
                if !c.is_control() {
                    return Some(c.to_string());
                }
            }
            return None;
        }
    };
    Some(sequence.to_string())
}

pub(crate) fn strip_ansi(text: &str) -> String {
    enum State {
        Ground,
        Escape,
        Csi,
        Osc,
        OscEscape,
    }

    let mut state = State::Ground;
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match state {
            State::Ground => match c {
                '\u{1b}' => state = State::Escape,
                '\r' => out.push('\n'),
                c if !c.is_control() || matches!(c, '\n' | '\t') => out.push(c),
                _ => {}
            },
            State::Escape => match c {
                '[' => state = State::Csi,
                // DCS, APC, SOS, PM are ST-terminated strings like OSC
                // (e.g. kitty-graphics probes: `ESC _ G ... ESC \`).
                ']' | 'P' | '_' | 'X' | '^' => state = State::Osc,
                _ => state = State::Ground,
            },
            State::Csi => {
                if ('\u{40}'..='\u{7e}').contains(&c) {
                    state = State::Ground;
                }
            }
            State::Osc => match c {
                '\u{07}' => state = State::Ground,
                '\u{1b}' => state = State::OscEscape,
                _ => {}
            },
            State::OscEscape => {
                state = if c == '\\' { State::Ground } else { State::Osc };
            }
        }
    }

    // TUI repaints leave long runs of blank lines once escapes are stripped.
    let mut collapsed = String::with_capacity(out.len());
    let mut blank_run = 0usize;
    for line in out.lines() {
        if line.trim().is_empty() {
            blank_run += 1;
            if blank_run > 2 {
                continue;
            }
        } else {
            blank_run = 0;
        }
        collapsed.push_str(line);
        collapsed.push('\n');
    }
    collapsed
}

fn trace(message: &str) {
    let path = crate::app_paths::unpeel_home()
        .join("hooks")
        .join("trace.log");
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
    {
        let _ = writeln!(file, "{} mcp-host {}", current_timestamp_ms(), message);
    }
}

#[cfg(test)]
mod tests {
    use crate::state::SessionInfo;
    use crate::transcripts::{
        collect_transcript_entries, resume_id_from_command, TranscriptProvider,
    };
    use std::path::Path;

    use super::*;

    fn transcript_provider(slug: &str) -> TranscriptProvider {
        TranscriptProvider::for_legacy_slug(slug).expect("test provider is registered")
    }

    fn test_manifest(
        id: &str,
        project_id: &str,
        cwd: &Path,
        command: &str,
    ) -> HostedSessionManifest {
        HostedSessionManifest {
            session: SessionInfo {
                id: id.to_string(),
                project_id: project_id.to_string(),
                label: command.to_string(),
                custom_title: false,
                command: command.to_string(),
                created_at: 1,
                tag_id: None,
                worktree_path: None,
                worktree_branch: None,
                parent_session_id: None,
                spawned_by: None,
                role: None,
                task: None,
            },
            cwd: cwd.to_string_lossy().to_string(),
            state: HostedSessionState::Running,
            pid: Some(42),
            pid_started_at: None,
            exit_code: None,
            host_build_id: None,
            host_protocol_version: None,
            has_been_written_to: true,
            provider_session_id: None,
            provider_transcript_path: None,
            managed_storage_path: None,
            resume_failure_markers: Vec::new(),
            runtime: None,
            runtime_launch_generation: 0,
            runtime_launch_pending: false,
            runtime_launched_at: None,
            runtime_launch_output_offset: 0,
            mcp_enabled: None,
            browser_mcp_enabled: None,
            computer_mcp_enabled: None,
            mcp_client_registered: false,
            browser_client_registered: false,
            computer_client_registered: false,
            menu_prompt_active: false,
            screen_changed_at: None,
            detected_local_urls: Vec::new(),
            heartbeat_at: 1,
            updated_at: 1,
        }
    }

    #[test]
    fn initialize_echoes_protocol_version_and_advertises_tools() {
        let response = handle_message(&json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": { "protocolVersion": "2025-03-26" },
        }))
        .expect("initialize must produce a response");
        assert_eq!(response["result"]["protocolVersion"], "2025-03-26");
        assert_eq!(response["result"]["serverInfo"]["name"], "unpeel");
        assert!(response["result"]["capabilities"]["tools"].is_object());
    }

    #[test]
    fn notifications_get_no_response() {
        assert!(handle_message(&json!({
            "jsonrpc": "2.0",
            "method": "notifications/initialized",
        }))
        .is_none());
    }

    #[test]
    fn tools_list_exposes_one_action_tool_per_domain() {
        // tools/list output depends on the ambient caller's manifest (the
        // test process may itself run inside a hosted session), so assert
        // the advertised set is drawn from the domain tools…
        let response = handle_message(&json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "tools/list",
        }))
        .expect("tools/list must produce a response");
        for tool in response["result"]["tools"].as_array().unwrap() {
            let name = tool["name"].as_str().unwrap();
            assert!(
                name == SESSIONS_TOOL || name == BROWSER_TOOL || name == COMPUTER_TOOL,
                "unexpected advertised tool {name}"
            );
        }

        // …and validate the domain tool shapes directly, environment-free.
        let tools = [
            sessions_tool_definition(),
            browser_tool_definition(),
            computer_tool_definition(),
        ];
        for tool in &tools {
            assert!(tool["inputSchema"]["type"] == "object");
            assert!(tool["description"].as_str().unwrap().len() > 20);
            assert_eq!(tool["inputSchema"]["required"], json!(["action"]));
            let actions = tool["inputSchema"]["properties"]["action"]["enum"]
                .as_array()
                .expect("action enum");
            assert!(actions.iter().any(|action| action == "help"));
        }

        let sessions = &tools[0];
        let actions = sessions["inputSchema"]["properties"]["action"]["enum"]
            .as_array()
            .unwrap();
        for expected in ["list", "inspect", "send_text", "report_to_group", "close"] {
            assert!(
                actions.iter().any(|action| action == expected),
                "missing sessions action {expected}"
            );
        }
        // Agents never create sessions: no creation action is advertised.
        assert!(!actions
            .iter()
            .any(|action| action == "start_session" || action == "delegate_task"));
        assert!(sessions["inputSchema"]["properties"]["summary"].is_object());

        let browser = &tools[1];
        let actions = browser["inputSchema"]["properties"]["action"]["enum"]
            .as_array()
            .unwrap();
        for expected in ["open", "snapshot", "click", "screenshot", "context"] {
            assert!(
                actions.iter().any(|action| action == expected),
                "missing browser action {expected}"
            );
        }

        let computer = &tools[2];
        let actions = computer["inputSchema"]["properties"]["action"]["enum"]
            .as_array()
            .unwrap();
        for expected in [
            "launch",
            "see",
            "click",
            "type",
            "screenshot",
            "escalate",
            "context",
        ] {
            assert!(
                actions.iter().any(|action| action == expected),
                "missing computer action {expected}"
            );
        }
        // Sessions and scope are server-managed: agents never declare or end
        // the engine session themselves.
        assert!(!actions
            .iter()
            .any(|action| action == "start_session" || action == "end_session"));
    }

    #[test]
    fn advertised_schema_stays_within_the_context_budget() {
        // The point of the unified surface: the whole advertised schema must
        // stay small. Measured over every domain definition explicitly
        // (the registered tool list is caller-dependent and could hide domains in
        // the test environment). If this fails, trim descriptions or move
        // detail into `action: "help"` — do not raise the ceiling casually.
        // Reference: the two pre-unification servers alone cost ~15.8 KB.
        let all_domains = vec![
            sessions_tool_definition(),
            browser_tool_definition(),
            computer_tool_definition(),
        ];
        let serialized = serde_json::to_string(&all_domains).unwrap();
        assert!(
            serialized.len() < 11 * 1024,
            "advertised tool schemas grew to {} bytes (~{} tokens); keep the surface terse",
            serialized.len(),
            serialized.len() / 4
        );

        // Each domain also stays individually lean.
        for definition in &all_domains {
            let size = serde_json::to_string(definition).unwrap().len();
            assert!(
                size < 4 * 1024,
                "{} schema grew to {size} bytes; move detail into help",
                definition["name"]
            );
        }
    }

    #[test]
    fn registration_domain_mask_overrides_broader_manifest_grants() {
        let mut manifest = test_manifest("masked", "project", Path::new("/tmp"), "kiro-cli");
        manifest.mcp_enabled = Some(true);
        manifest.browser_mcp_enabled = Some(true);
        manifest.computer_mcp_enabled = Some(true);
        let domains = McpDomainMask {
            sessions: true,
            browser: true,
            computer: false,
        };

        let names = tool_definitions_for_manifest(Some(&manifest), domains)
            .into_iter()
            .filter_map(|tool| tool["name"].as_str().map(str::to_string))
            .collect::<Vec<_>>();
        assert_eq!(names, vec![SESSIONS_TOOL, BROWSER_TOOL]);

        let denied = tools_call_with_domains(
            &json!({ "name": COMPUTER_TOOL, "arguments": { "action": "help" } }),
            domains,
        )
        .expect("domain denial is an MCP tool result");
        assert_eq!(denied["isError"], true);
        assert!(denied["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains("not enabled for this MCP registration"));
    }

    #[test]
    fn sessions_actions_route_to_the_legacy_handlers() {
        // Unknown action: helpful error naming the valid actions.
        let err = run_tool("sessions", &json!({ "action": "explode" }))
            .expect_err("unknown action must fail");
        assert!(err.contains("Unknown sessions action"));
        assert!(err.contains("help"));

        // Missing action: names the parameter and the options.
        let err = run_tool("sessions", &json!({})).expect_err("missing action must fail");
        assert!(err.contains("action"));

        // Every advertised action resolves to a handler or help.
        for (action, legacy) in SESSIONS_ACTIONS {
            assert!(
                legacy_sessions_action(legacy) == Some(*action),
                "round-trip failed for {action}"
            );
        }
    }

    #[test]
    fn worktree_access_parses_leniently() {
        assert!(worktree_access_enabled(
            &json!({ "mcp_worktree_access": true })
        ));
        assert!(!worktree_access_enabled(
            &json!({ "mcp_worktree_access": false })
        ));
        assert!(!worktree_access_enabled(&json!({})));
        assert!(!worktree_access_enabled(
            &json!({ "mcp_worktree_access": "yes" })
        ));
        assert!(!worktree_access_enabled(&Value::Null));
    }

    #[test]
    fn browser_action_validation_is_helpful() {
        let err = run_tool("browser", &json!({ "action": "teleport" }))
            .expect_err("unknown action must fail");
        assert!(err.contains("Unknown browser action"));
        assert!(err.contains("snapshot"));
    }

    #[test]
    fn help_actions_render_per_action_docs_without_a_caller_gate() {
        // help must work even with no session identity (that is the point:
        // discoverability before/despite gating).
        let sessions_help_text =
            run_tool("sessions", &json!({ "action": "help" })).expect("sessions help");
        for expected in ["send_text", "wait_for_status", "report_to_group", "close"] {
            assert!(
                sessions_help_text.contains(expected),
                "sessions help missing {expected}"
            );
        }

        let one = run_tool(
            "sessions",
            &json!({ "action": "help", "help_for": "send_keys" }),
        )
        .expect("scoped sessions help");
        assert!(one.contains("send_keys"));
        assert!(!one.contains("### report_to_group"));

        let browser_help_text =
            run_tool("browser", &json!({ "action": "help" })).expect("browser help");
        for expected in ["open", "snapshot", "click", "screenshot"] {
            assert!(
                browser_help_text.contains(expected),
                "browser help missing {expected}"
            );
        }

        let missing = run_tool(
            "browser",
            &json!({ "action": "help", "help_for": "teleport" }),
        )
        .expect("help for unknown action still answers");
        assert!(missing.contains("No such browser action"));
    }

    #[test]
    fn legacy_tool_names_still_dispatch() {
        // Stale clients (sessions launched before the unified surface) call
        // the old names; every legacy name must still resolve to a handler.
        // No tool is invoked here: the test environment may itself be a
        // hosted Unpeel session, so a live call could really list sessions.
        for definition in legacy_sessions_tool_definitions() {
            let legacy = definition["name"].as_str().unwrap();
            assert!(
                legacy_sessions_action(legacy).is_some(),
                "legacy tool {legacy} lost its dispatch mapping"
            );
        }

        let err = run_tool("nonsense_tool", &json!({})).expect_err("unknown tool must fail");
        assert!(err.contains("Unknown tool"));
        assert!(err.contains("'sessions'"));
    }

    #[test]
    fn creation_tools_are_refused_when_called_blind() {
        // Even if a stale client calls a removed creation tool by name, it is
        // refused rather than silently doing nothing.
        for name in ["start_session", "delegate_task", "delegate_batch"] {
            let err = run_tool(name, &json!({ "command": "claude" }))
                .expect_err("creation tool must be refused");
            assert!(
                err.contains("cannot create sessions"),
                "unexpected error for {name}: {err}"
            );
        }
    }

    #[test]
    fn transcript_defaults_are_low_context() {
        assert_eq!(READ_TRANSCRIPT_DEFAULT_ENTRIES, 5);
        let tools = legacy_sessions_tool_definitions();
        let read_transcript = tools
            .iter()
            .find(|tool| tool["name"] == "read_transcript")
            .expect("read_transcript tool must exist");
        assert!(
            read_transcript["inputSchema"]["properties"]["include_tools"]["description"]
                .as_str()
                .unwrap()
                .contains("Settings default")
        );
        assert!(initialize_result(&json!({}))["instructions"]
            .as_str()
            .unwrap()
            .contains("same sidebar group"));
    }

    #[test]
    fn group_report_is_structured() {
        let mut caller = test_manifest("caller", "p", Path::new("/tmp/p"), "codex");
        caller.session.role = Some("Reviewer".to_string());
        caller.session.task = Some("Check the implementation".to_string());
        let target = test_manifest("target", "p", Path::new("/tmp/p"), "claude");

        let report = build_group_report(
            &json!({
                "status": "done",
                "summary": "Looks correct.",
                "proof": ["cargo test -p unpeel-core mcp_host"],
                "changed_paths": ["crates/unpeel-core/src/mcp_host.rs"],
            }),
            &caller,
            &target,
        )
        .expect("report must build");

        assert!(report.contains("Status: done"));
        assert!(report.contains("Role: Reviewer"));
        assert!(report.contains("Task: Check the implementation"));
        assert!(report.contains("Summary:"));
        assert!(report.contains("cargo test -p unpeel-core mcp_host"));
    }

    #[test]
    fn unknown_method_returns_method_not_found() {
        let response = handle_message(&json!({
            "jsonrpc": "2.0",
            "id": 3,
            "method": "resources/list",
        }))
        .expect("unknown method must produce an error response");
        assert_eq!(response["error"]["code"], -32601);
    }

    #[test]
    fn unknown_tool_returns_tool_error_result() {
        let response = handle_message(&json!({
            "jsonrpc": "2.0",
            "id": 4,
            "method": "tools/call",
            "params": { "name": "definitely_not_a_tool", "arguments": {} },
        }))
        .expect("tools/call must produce a response");
        assert_eq!(response["result"]["isError"], true);
    }

    #[test]
    fn key_sequences_cover_navigation_and_control_keys() {
        assert_eq!(key_sequence("enter").as_deref(), Some("\r"));
        assert_eq!(key_sequence("down").as_deref(), Some("\x1b[B"));
        assert_eq!(key_sequence("up").as_deref(), Some("\x1b[A"));
        assert_eq!(key_sequence("shift+tab").as_deref(), Some("\x1b[Z"));
        assert_eq!(key_sequence("ctrl+c").as_deref(), Some("\x03"));
        assert_eq!(key_sequence("ctrl+r").as_deref(), Some("\x12"));
        assert_eq!(key_sequence("esc").as_deref(), Some("\x1b"));
        assert_eq!(key_sequence("1").as_deref(), Some("1"));
        assert_eq!(key_sequence("Y").as_deref(), Some("Y"));
        assert_eq!(key_sequence("ctrl+shift+x"), None);
        assert_eq!(key_sequence("madeup"), None);
    }

    #[test]
    fn paste_text_is_sanitized_and_wrapped() {
        let sanitized = sanitize_paste_text("hi\r\nthere\x1b[31m end\x07");
        assert_eq!(sanitized, "hi\nthere[31m end");
        assert_eq!(encode_bracketed_paste("hello"), "\x1b[200~hello\x1b[201~");
    }

    #[test]
    fn strip_ansi_removes_escapes_and_collapses_blank_runs() {
        let stripped = strip_ansi("\x1b[2J\x1b[Ha\x1b]0;title\x07b\r\n\n\n\n\nc");
        assert_eq!(stripped, "ab\n\n\nc\n");
    }

    #[test]
    fn transcript_command_parsing_detects_provider_and_resume_ids() {
        assert_eq!(
            transcript_provider_for_command("claude --dangerously-skip-permissions"),
            Some(transcript_provider("claude"))
        );
        assert_eq!(
            transcript_provider_for_command("/tmp/bin/codex resume 019abc"),
            Some(transcript_provider("codex"))
        );
        assert_eq!(
            resume_id_from_command(transcript_provider("claude"), "claude --resume abc-123"),
            Some("abc-123".to_string())
        );
        assert_eq!(
            resume_id_from_command(transcript_provider("claude"), "claude --resume=abc-456"),
            Some("abc-456".to_string())
        );
        assert_eq!(
            resume_id_from_command(
                transcript_provider("codex"),
                "codex --dangerously-bypass-approvals-and-sandbox resume 019abc"
            ),
            Some("019abc".to_string())
        );
        assert_eq!(
            resume_id_from_command(transcript_provider("codex"), "codex resume --last"),
            None
        );
        assert_eq!(
            resume_id_from_command(
                transcript_provider("codex"),
                "codex --dangerously-bypass-approvals-and-sandbox resume --last"
            ),
            None
        );
    }

    #[test]
    fn codex_transcript_entries_are_compact_and_deduped() {
        let raw = r#"
{"type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"fix it"}]}}
{"type":"event_msg","payload":{"type":"user_message","message":"fix it"}}
{"type":"response_item","payload":{"type":"function_call","call_id":"c1","name":"exec_command","arguments":"{\"cmd\":\"cargo test\"}"}}
{"type":"response_item","payload":{"type":"function_call_output","call_id":"c1","output":"Process exited with code 0\nall good"}}
{"type":"event_msg","payload":{"type":"agent_message","message":"Done."}}
"#;
        let entries = collect_transcript_entries(transcript_provider("codex"), raw, true);
        assert_eq!(entries[0].role, "User");
        assert_eq!(entries[0].text, "fix it");
        assert!(entries
            .iter()
            .any(|entry| entry.text.contains("cargo test")));
        assert_eq!(entries.last().unwrap().text, "Done.");
        assert_eq!(
            entries
                .iter()
                .filter(|entry| entry.role == "User" && entry.text == "fix it")
                .count(),
            1
        );
    }

    #[test]
    fn codex_transcript_entries_drop_bootstrap_noise_and_hide_tools_by_default() {
        let raw = r##"
{"type":"event_msg","payload":{"type":"user_message","message":"# AGENTS.md instructions for /tmp/repo\nFollow these rules."}}
{"type":"event_msg","payload":{"type":"user_message","message":"<environment_context>\n  <cwd>/tmp/repo</cwd>\n</environment_context>"}}
{"type":"event_msg","payload":{"type":"user_message","message":"fix the broken prompt\n\n[sent from Unpeel session_id=\"caller-1\"]"}}
{"type":"response_item","payload":{"type":"function_call","call_id":"c1","name":"exec_command","arguments":"{\"cmd\":\"cargo test\"}"}}
{"type":"response_item","payload":{"type":"function_call_output","call_id":"c1","output":"test output"}}
{"type":"event_msg","payload":{"type":"agent_message","message":"Patched."}}
"##;

        let entries = collect_transcript_entries(transcript_provider("codex"), raw, false);
        assert_eq!(entries.len(), 2);
        assert_eq!(
            (entries[0].role, entries[0].text.as_str()),
            ("User", "fix the broken prompt")
        );
        assert_eq!(
            (entries[1].role, entries[1].text.as_str()),
            ("Assistant", "Patched.")
        );

        let entries_with_tools =
            collect_transcript_entries(transcript_provider("codex"), raw, true);
        assert!(entries_with_tools
            .iter()
            .any(|entry| entry.role == "Tool" && entry.text.contains("cargo test")));
        assert!(!entries_with_tools
            .iter()
            .any(|entry| entry.text.contains("AGENTS.md")));
    }

    #[test]
    fn claude_transcript_entries_skip_internal_user_wrappers() {
        let raw = r#"
{"type":"user","userType":"external","message":{"role":"user","content":[{"type":"text","text":"hello claude"}]}}
{"type":"user","message":{"role":"user","content":"<local-command-stdout>noise</local-command-stdout>"}}
{"type":"assistant","message":{"content":[{"type":"text","text":"hello back"},{"type":"tool_use","id":"t1","name":"Read","input":{"file_path":"src/main.rs"}}]}}
{"type":"user","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"t1","content":"file contents"}]}}
"#;
        let entries = collect_transcript_entries(transcript_provider("claude"), raw, true);
        assert!(entries.iter().any(|entry| entry.text == "hello claude"));
        assert!(!entries
            .iter()
            .any(|entry| entry.text.contains("local-command")));
        assert!(entries
            .iter()
            .any(|entry| entry.text.contains("src/main.rs")));
        assert!(entries
            .iter()
            .any(|entry| entry.text.contains("hello back")));
    }

    #[test]
    fn manifest_lookup_rejects_path_traversal() {
        assert!(load_manifest("../other").is_none());
        assert!(load_manifest("a/b").is_none());
        assert!(load_manifest("").is_none());
    }

    fn security(grants: HashMap<String, McpGrant>) -> McpSecurity {
        McpSecurity {
            grants,
            default_grant: McpGrant::default(),
            nonchild_write_access: crate::state::McpNonChildWriteAccess::Ask,
            write_approvals: HashMap::new(),
        }
    }

    fn grant(role: McpRole, reach: McpScope) -> McpGrant {
        McpGrant { role, reach }
    }

    #[test]
    fn reads_are_open_across_all_sessions() {
        // The core of the model: any enabled caller reads ANY session — its
        // own project, another project, no relation at all. Only an unknown
        // caller (no manifest) reads nothing.
        let security = security(HashMap::new());
        let caller = test_manifest("caller", "p", Path::new("/tmp/p"), "claude");
        let same_project = test_manifest("same", "p", Path::new("/tmp/p"), "codex");
        let other_project = test_manifest("other", "q", Path::new("/tmp/q"), "codex");
        assert_eq!(security.effective_role(Some(&caller)), McpRole::Read);
        assert!(security.permits_manifest(Some(&caller), &same_project));
        assert!(security.permits_manifest(Some(&caller), &other_project));
        assert!(!security.permits_manifest(None, &same_project));
        assert_eq!(security.effective_role(None), McpRole::Off);
    }

    #[test]
    fn internal_off_grant_denies_a_caller() {
        // Off is internal now, but the gate still treats it as no access —
        // an explicit per-session Off override wins over the default grant.
        let security = security(HashMap::from([(
            "caller".to_string(),
            grant(McpRole::Off, McpScope::Project),
        )]));
        let caller = test_manifest("caller", "p", Path::new("/tmp/p"), "claude");
        let target = test_manifest("target", "p", Path::new("/tmp/p"), "codex");
        assert_eq!(security.effective_role(Some(&caller)), McpRole::Off);
        assert!(!security.permits_manifest(Some(&caller), &target));
    }

    #[test]
    fn any_session_reads_others_and_group_bounds_free_writes() {
        // Reading is open; same-group writes are free and cross-group writes
        // go through the approval policy in require_session.
        let security = security(HashMap::new());
        let caller = test_manifest("caller", "p", Path::new("/tmp/p"), "claude");
        let peer = test_manifest("peer", "p", Path::new("/tmp/p"), "codex");
        let other = test_manifest("other", "q", Path::new("/tmp/q"), "codex");
        assert!(security.permits_manifest(Some(&caller), &other));
        assert!(caller_shares_group(Some(&caller), &peer));
        assert!(!caller_shares_group(Some(&caller), &other));
    }

    #[test]
    fn legacy_parent_id_does_not_change_group_permissions() {
        let mut legacy = test_manifest("legacy", "q", Path::new("/tmp/q"), "codex");
        legacy.session.parent_session_id = Some("caller".to_string());
        let caller = test_manifest("caller", "p", Path::new("/tmp/p"), "claude");
        assert!(!caller_shares_group(Some(&caller), &legacy));
    }

    #[test]
    fn send_text_envelopes_cross_group_messages_only() {
        let caller = test_manifest("caller", "p", Path::new("/tmp/p"), "claude");
        let peer = test_manifest("peer", "p", Path::new("/tmp/p"), "codex");
        let other = test_manifest("other", "q", Path::new("/tmp/q"), "codex");

        // A cross-group message carries sender provenance the receiver can
        // reply to.
        assert_eq!(
            send_text_envelope(Some(&caller), Some(&other)).as_deref(),
            Some("[message from id:caller, channel: terminal]")
        );
        // Same-group traffic is delivered verbatim in both directions.
        assert!(send_text_envelope(Some(&caller), Some(&peer)).is_none());
        assert!(send_text_envelope(Some(&peer), Some(&caller)).is_none());
        // Unknown sender or target: nothing to attribute.
        assert!(send_text_envelope(None, Some(&peer)).is_none());
        assert!(send_text_envelope(Some(&caller), None).is_none());
    }

    #[test]
    fn write_pair_approvals_are_directional_and_per_target() {
        let mut security = security(HashMap::new());
        security.write_approvals =
            HashMap::from([("caller".to_string(), vec!["target".to_string()])]);
        assert!(security.write_pair_approved("caller", "target"));
        // Approving caller→target does not approve the reverse direction…
        assert!(!security.write_pair_approved("target", "caller"));
        // …nor a different target for the same caller.
        assert!(!security.write_pair_approved("caller", "other"));
    }

    #[test]
    fn nonchild_write_access_parses_leniently_and_defaults_to_ask() {
        use crate::state::McpNonChildWriteAccess as Policy;
        assert_eq!(Policy::from_state_str("deny"), Policy::Deny);
        assert_eq!(Policy::from_state_str(" ALLOW "), Policy::Allow);
        assert_eq!(Policy::from_state_str("ask"), Policy::Ask);
        // Unknown/malformed values must never silently widen or lock access.
        assert_eq!(Policy::from_state_str("bogus"), Policy::Ask);
        assert_eq!(Policy::default(), Policy::Ask);
    }

    #[test]
    fn path_detection_drives_paste_settle_delay() {
        assert!(looks_like_it_contains_a_path("look at src/lib/foo.ts"));
        assert!(!looks_like_it_contains_a_path("fix the login bug"));
    }

    #[test]
    fn wait_matching_is_case_insensitive_by_default() {
        let screen = "❯ npm test\nAll Tests PASSED (42)\n";
        // Caller lowercases the needle for the insensitive path.
        assert_eq!(
            find_matching_line(screen, "tests passed", false),
            Some("All Tests PASSED (42)")
        );
        assert_eq!(find_matching_line(screen, "tests passed", true), None);
        assert_eq!(
            find_matching_line(screen, "Tests PASSED", true),
            Some("All Tests PASSED (42)")
        );
        assert_eq!(find_matching_line(screen, "no such text", false), None);
    }

    #[test]
    fn wait_for_text_validates_arguments_and_session() {
        // Missing text argument fails before touching any session.
        let result = tool_wait_for_text(&json!({ "session_id": "nope" }));
        assert!(result.unwrap_err().contains("'text'"));
        // Unknown session fails fast, not after the timeout.
        let result = tool_wait_for_text(&json!({
            "session_id": "definitely-not-a-session",
            "text": "ready",
        }));
        assert!(result.unwrap_err().contains("Unknown session id"));
    }

    #[test]
    fn done_and_idle_are_equivalent_settled_states() {
        // The core fix: a finished turn reads as `done` (unread) or `idle`
        // (observed) depending on UI focus the agent can't control, so waits
        // must treat them as one target — for every provider.
        assert!(status_matches("idle", "done"));
        assert!(status_matches("done", "idle"));
        assert!(status_matches("idle", "idle"));
        assert!(status_matches("done", "done"));
        assert!(status_matches("working", "working"));
        // Not equivalent to unrelated states.
        assert!(!status_matches("working", "idle"));
        assert!(!status_matches("blocked", "done"));
        // Group terminal set includes idle (a peer that finished while
        // observed) so wait_for_group can't hang on it.
        assert!(group_status_is_terminal("idle"));
        assert!(group_status_is_terminal("done"));
        assert!(!group_status_is_terminal("working"));
    }

    #[test]
    fn wait_for_status_validates_arguments_and_session() {
        let result = tool_wait_for_status(&json!({ "session_id": "nope" }));
        assert!(result.unwrap_err().contains("'status'"));

        let result = tool_wait_for_status(&json!({
            "session_id": "nope",
            "status": "busy",
        }));
        assert!(result.unwrap_err().contains("Unsupported status"));

        let result = tool_wait_for_status(&json!({
            "session_id": "definitely-not-a-session",
            "status": "done",
        }));
        assert!(result.unwrap_err().contains("Unknown session id"));
    }
}
