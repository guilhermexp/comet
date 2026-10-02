mod support;
use parking_lot::Mutex;
use serde_json::json;
use std::ffi::OsString;
use std::fs;
use std::process::Command;
use tempfile::TempDir;
use zeron_workers_unpeel::{
    WorkerCompletionEvidence, WorkersSession, WorkersSessionCapabilities,
    begin_worker_parent_task_at, controller_mcp_archive_guard,
    controller_mcp_briefing_stability_key, controller_mcp_choose_semantic_output,
    controller_mcp_clean_output, controller_mcp_consume_authority_marker,
    controller_mcp_encode_keys, controller_mcp_handle_request, controller_mcp_is_booting_screen,
    controller_mcp_is_briefing_screen_ready, controller_mcp_live_worker_guard,
    controller_mcp_parse_launch, controller_mcp_parse_launch_briefing,
    controller_mcp_replacement_session_id, controller_mcp_sanitize_text,
    controller_mcp_startup_prompt_response, controller_mcp_take_parent_chat_id,
    controller_mcp_tracks_task_episode, current_episode_completed_with_evidence_at,
    ensure_controller_mcp_host_launcher, is_session_host_mode, register_worker_parent_at,
    worker_parent_links_at,
};

#[test]
fn worker_parent_links_are_read_only_and_deterministic() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("app-state.json");
    register_worker_parent_at(&path, "worker-z", "chat-2", 200).unwrap();
    register_worker_parent_at(&path, "worker-a", "chat-1", 100).unwrap();

    let links = worker_parent_links_at(&path).unwrap();

    assert_eq!(links.len(), 2);
    assert_eq!(links[0].worker_session_id, "worker-a");
    assert_eq!(links[0].parent_chat_id, "chat-1");
    assert_eq!(links[0].registered_at_unix_ms, 100);
    assert_eq!(links[1].worker_session_id, "worker-z");
    assert_eq!(links[1].parent_chat_id, "chat-2");
    assert_eq!(links[1].registered_at_unix_ms, 200);
}

#[test]
fn controller_mode_is_claimed_without_claiming_normal_cli_commands() {
    assert!(is_session_host_mode(&["__workers_mcp__".into()]));
    assert!(!is_session_host_mode(&["workers".into(), "top".into()]));
}

#[test]
fn initialize_and_tools_list_advertise_one_compact_workers_tool() {
    let initialize = controller_mcp_handle_request(json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {}
    }))
    .expect("initialize responds");
    assert_eq!(initialize["result"]["serverInfo"]["name"], "comet-workers");

    let tools = controller_mcp_handle_request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/list",
        "params": {}
    }))
    .expect("tools/list responds");
    let tools = tools["result"]["tools"]
        .as_array()
        .expect("tools is an array");
    assert_eq!(tools.len(), 1);
    assert_eq!(tools[0]["name"], "workers");
}

/// A compound action-dispatch tool is callable on the first try only if the
/// schema says which field belongs to which action. Without that, delegating
/// costs a `help` round-trip while editing locally costs nothing — and an
/// orchestrator under that gradient inspects forever instead of delegating.
#[test]
fn the_workers_schema_documents_every_action_and_names_the_other_substance() {
    let tools = controller_mcp_handle_request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/list",
        "params": {}
    }))
    .expect("tools/list responds");
    let tool = &tools["result"]["tools"][0];

    let description = tool["description"].as_str().expect("tool description");
    assert!(
        description.contains("`task`"),
        "the description must also say when workers is NOT the right substance: {description}"
    );

    let properties = tool["inputSchema"]["properties"]
        .as_object()
        .expect("schema properties");
    let mut documentation = description.to_owned();
    for (name, property) in properties {
        let field = property["description"]
            .as_str()
            .unwrap_or_else(|| panic!("field {name} carries no description"));
        assert!(
            !field.trim().is_empty(),
            "field {name} carries an empty description"
        );
        documentation.push('\n');
        documentation.push_str(field);
    }

    for action in properties["action"]["enum"]
        .as_array()
        .expect("action enum")
    {
        let action = action.as_str().expect("action is a string");
        assert!(
            documentation.contains(action),
            "action {action} is named in no description, so a caller cannot build the call without a help round-trip"
        );
    }

    let new_worktree = &properties["new_worktree"];
    assert_eq!(new_worktree["type"], "object");
    assert_eq!(new_worktree["additionalProperties"], false);
    assert_eq!(new_worktree["required"][0], "branch");
    assert_eq!(new_worktree["properties"]["branch"]["type"], "string");
    assert_eq!(new_worktree["properties"]["base_ref"]["type"], "string");
    assert!(
        new_worktree["description"]
            .as_str()
            .unwrap()
            .contains("Mutually exclusive with worktree_path/worktree_branch")
    );
    assert!(description.contains("new_worktree: {branch, base_ref?}"));
    assert!(description.contains("sharing the project checkout is intentional"));
}

#[test]
fn controller_help_explains_isolated_and_explicit_shared_checkout_launches() {
    let response = controller_mcp_handle_request(json!({
        "jsonrpc": "2.0",
        "id": 9,
        "method": "tools/call",
        "params": {
            "name": "workers",
            "arguments": { "action": "help" }
        }
    }))
    .expect("help action responds");
    let help = &response["result"]["structuredContent"];

    assert!(
        help["workflow"]
            .as_str()
            .unwrap()
            .contains("each independent slice")
    );
    assert!(
        help["workflow"]
            .as_str()
            .unwrap()
            .contains("new_worktree={branch, base_ref?}")
    );
    assert!(
        help["workflow"]
            .as_str()
            .unwrap()
            .contains("sharing the project checkout is intentional")
    );
    assert!(
        help["launch_worker"]["independent_slice"]
            .as_str()
            .unwrap()
            .contains("unique branch")
    );
    assert!(
        help["launch_worker"]["project_checkout"]
            .as_str()
            .unwrap()
            .contains("explicit choice")
    );
}

#[test]
fn notifications_do_not_receive_json_rpc_responses() {
    assert!(
        controller_mcp_handle_request(json!({
            "jsonrpc": "2.0",
            "method": "notifications/initialized",
            "params": {}
        }))
        .is_none()
    );
}

#[test]
fn launch_accepts_only_an_enabled_preset_never_a_raw_command() {
    assert!(controller_mcp_parse_launch(json!({ "project_id": "p" })).is_err());
    assert!(
        controller_mcp_parse_launch(json!({
            "project_id": "p",
            "command": "omp --model anthropic/claude-opus-4-8"
        }))
        .is_err()
    );
    assert!(
        controller_mcp_parse_launch(json!({
            "project_id": "p",
            "preset_id": "preset",
            "command": "codex"
        }))
        .is_err()
    );
    let preset = controller_mcp_parse_launch(json!({
        "project_id": "p",
        "preset_id": "preset"
    }))
    .expect("preset launch parses");
    assert_eq!(
        preset.wire_body(),
        json!({ "projectID": "p", "presetID": "preset" })
    );
}

#[test]
fn controller_defers_a_sanitized_briefing_until_after_session_creation() {
    let (launch, briefing) = controller_mcp_parse_launch_briefing(json!({
        "project_id": "p",
        "preset_id": "claude",
        "initial_text": "review\u{0} this\r\ncarefully"
    }))
    .expect("launch and briefing parse");

    assert_eq!(
        launch.wire_body(),
        json!({ "projectID": "p", "presetID": "claude" })
    );
    assert_eq!(briefing.as_deref(), Some("review this\ncarefully"));
    assert!(
        controller_mcp_parse_launch_briefing(json!({
            "project_id": "p",
            "preset_id": "claude",
            "initial_text": "\u{0}\u{1b}"
        }))
        .is_err()
    );
}

#[test]
fn launch_title_reaches_the_host_as_a_sanitized_session_title() {
    let launch = controller_mcp_parse_launch(json!({
        "project_id": "p",
        "preset_id": "claude",
        "title": "WT-1\u{1b} fix parser"
    }))
    .expect("titled launch parses");
    assert_eq!(
        launch.wire_body(),
        json!({ "projectID": "p", "presetID": "claude", "title": "WT-1 fix parser" })
    );
    assert!(
        controller_mcp_parse_launch(json!({
            "project_id": "p",
            "preset_id": "claude",
            "title": " \u{0} "
        }))
        .is_err()
    );
}

#[test]
fn key_encoder_is_bounded_and_deterministic() {
    assert_eq!(
        controller_mcp_encode_keys(&["escape".into(), "down".into(), "enter".into()])
            .expect("known keys encode"),
        "\u{1b}\u{1b}[B\r"
    );
    assert!(controller_mcp_encode_keys(&vec!["enter".into(); 65]).is_err());
    assert!(controller_mcp_encode_keys(&["unknown-special".into()]).is_err());
}

#[test]
fn ansi_cleanup_caps_model_output() {
    assert_eq!(
        controller_mcp_clean_output("\u{1b}[31mhello\u{1b}[0m", 1_024),
        "hello"
    );
    assert_eq!(controller_mcp_clean_output("abcdef", 4), "…f");
}

#[test]
fn semantic_screen_replaces_raw_tui_repaint_frames() {
    let raw = "\u{1b}[27;1H•Wor\u{1b}[27;1H•Work\u{1b}[27;1H•Working";
    let semantic = controller_mcp_choose_semantic_output(
        raw,
        Some(vec![
            "Final report".into(),
            "- changed parser".into(),
            "".into(),
        ]),
        64 * 1024,
    );

    assert_eq!(semantic, "Final report\n- changed parser");
    assert!(!semantic.contains("•Wor"));
}

#[test]
fn semantic_fallback_interprets_repaints_and_removes_controls() {
    let raw = "\u{1b}[27;1H•Wor\u{1b}[27;1H•Work\u{1b}[27;1HFinal reporx\u{8}t\u{0}";
    let semantic = controller_mcp_choose_semantic_output(raw, None, 64 * 1024);

    assert_eq!(semantic, "Final report");
    assert!(!semantic.chars().any(char::is_control));
}

#[cfg(unix)]
#[test]
fn worker_output_preserves_answer_when_repaints_depend_on_terminal_width() {
    use std::io::{BufRead, BufReader, Write};
    use std::os::unix::net::UnixListener;
    use unpeel_core::session_host::{SessionHostCommand, SessionHostResponse, socket_path};
    use unpeel_core::terminal_viewport::TerminalViewportState;

    let _lock = ENV_LOCK.lock();
    let home = tempfile::Builder::new()
        .prefix("out-")
        .tempdir_in("/tmp")
        .unwrap();
    let _home = UnpeelHomeGuard::set(home.path());
    let socket = socket_path("output-smoke");
    fs::create_dir_all(socket.parent().unwrap()).unwrap();
    let listener = UnixListener::bind(socket).unwrap();
    listener.set_nonblocking(true).unwrap();
    // The footer wraps at the real width. Clearing its two rows must not erase
    // the answer above it, as a replay at an invented wider width would.
    let raw = "RESULTADO: 527\r\nxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx\x1b[2K\r\x1b[1A\x1b[2K\r";
    let host = std::thread::spawn(move || {
        let mut terminal = TerminalViewportState::new(20, 5);
        terminal.feed(raw.as_bytes());
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        let (mut stream, _) = loop {
            match listener.accept() {
                Ok(connection) => break connection,
                Err(error)
                    if error.kind() == std::io::ErrorKind::WouldBlock
                        && std::time::Instant::now() < deadline =>
                {
                    std::thread::sleep(std::time::Duration::from_millis(10));
                }
                Err(error) => panic!("viewport request did not arrive: {error}"),
            }
        };
        stream.set_nonblocking(false).unwrap();
        stream
            .set_read_timeout(Some(std::time::Duration::from_secs(5)))
            .unwrap();
        let mut request = String::new();
        BufReader::new(&stream).read_line(&mut request).unwrap();
        let SessionHostCommand::ViewportSnapshot {
            cols,
            rows,
            scroll_offset_rows,
            viewport_rows,
        } = serde_json::from_str(&request).unwrap()
        else {
            panic!("expected viewport request");
        };
        let snapshot = if cols == 0 && rows == 0 {
            terminal.snapshot(scroll_offset_rows, viewport_rows)
        } else {
            terminal.snapshot_resized(cols, rows, scroll_offset_rows, viewport_rows)
        };
        let response = SessionHostResponse {
            ok: true,
            error: None,
            viewport: Some(snapshot),
            activity_token: None,
        };
        writeln!(stream, "{}", serde_json::to_string(&response).unwrap()).unwrap();
    });
    let text = zeron_workers_unpeel::worker_output_text("output-smoke", raw, 4096);
    host.join().unwrap();
    assert_eq!(text, "RESULTADO: 527");
}

#[test]
fn semantic_fallback_keeps_crlf_lines_and_repaints_over_carriage_return() {
    let raw = "first line\r\nsecond line\r\nold status\rnew status\r\n";
    let semantic = controller_mcp_choose_semantic_output(raw, None, 64 * 1024);

    assert_eq!(semantic, "first line\nsecond line\nnew status");
}

#[test]
fn parent_notification_assembly_consumes_semantic_viewport_when_present() {
    let raw = "stale paint\rAsk the user a question\r";
    let tail = controller_mcp_choose_semantic_output(
        raw,
        Some(vec!["Ask the user a question".into()]),
        4 * 1024,
    );
    assert_eq!(tail, "Ask the user a question");
    assert!(!tail.contains("stale paint"));
}

#[test]
fn parent_notification_assembly_falls_back_to_raw_when_semantic_is_empty() {
    let raw = "Ask the user a question\r";
    let tail = controller_mcp_choose_semantic_output(raw, None, 4 * 1024);
    assert!(
        tail.contains("Ask the user a question"),
        "raw fallback must keep the last paint when the grid is empty: {tail:?}"
    );
}

#[test]
fn known_startup_prompts_are_dismissed_before_submitting_the_brief() {
    assert_eq!(
        controller_mcp_startup_prompt_response(
            "Update available! 0.147 -> 0.148\n1. Update now\n2. Skip\nPress enter"
        )
        .as_deref(),
        Some("2\r")
    );
    assert_eq!(
        controller_mcp_startup_prompt_response(
            "Quick safety check: Is this a project you created or one you trust?\n1. Yes, I trust this folder\n2. No, exit"
        )
        .as_deref(),
        Some("1\r")
    );
    assert_eq!(controller_mcp_startup_prompt_response("❯"), None);
}

#[test]
fn briefing_waits_for_a_stable_agent_prompt_and_rejects_unknown_menus() {
    assert!(!controller_mcp_is_briefing_screen_ready(
        "claude",
        "Loading agent…",
        100
    ));
    assert!(!controller_mcp_is_briefing_screen_ready(
        "claude",
        "Loading agent…",
        1_000
    ));
    assert!(controller_mcp_is_briefing_screen_ready(
        "claude",
        "Claude Code\n❯",
        400
    ));
    assert!(controller_mcp_is_briefing_screen_ready(
        "gemini",
        "Gemini CLI\n> ",
        400
    ));
    assert!(!controller_mcp_is_briefing_screen_ready(
        "claude",
        "Choose setup:\n1. Continue\n2. Exit\nPress enter",
        1_000
    ));
    let agy_ready = "▄▀▀▄        Antigravity CLI 1.1.22\n\
                     ▀▀▀▀▀▀       agenthermes.varela@gmail.com\n\
                     ────────────────────────────────────────────────────\n\
                     >\n\
                     ────────────────────────────────────────────────────\n\
                     ? for shortcuts        Gemini 3.7 Flash · high (Google AI Pro)";
    assert!(controller_mcp_is_briefing_screen_ready(
        "agy", agy_ready, 400
    ));
    let agy_trust_selector = "Do you trust the contents of this project?\n\
                              Antigravity CLI requires permission to read, edit, and execute files here.\n\
                              > Yes, I trust this folder\n\
                                No, exit";
    assert!(!controller_mcp_is_briefing_screen_ready(
        "agy",
        agy_trust_selector,
        1_000
    ));
}

#[test]
fn a_selection_glyph_does_not_hide_a_numbered_menu() {
    // Codex prints its update menu with the cursor glyph on the selected row.
    // Anchoring `1.` at the start of the line missed it, and the same glyph is
    // what the codex prompt check reads as a ready composer.
    let menu = "Update available! 0.147.0 -> 0.150.1\n\
                › 1. Update now\n  2. Skip\n  3. Skip until next version\n\
                Press enter to continue";
    assert!(!controller_mcp_is_briefing_screen_ready(
        "codex", menu, 5_000
    ));
}

#[test]
fn a_booting_runtime_is_not_ready_even_with_its_prompt_painted() {
    let booting =
        "Starting MCP servers (2/6): codex_apps, context7, ...\n› Ask Codex to do anything";
    assert!(controller_mcp_is_booting_screen(booting));
    assert!(!controller_mcp_is_briefing_screen_ready(
        "codex", booting, 5_000
    ));
    let booted = "codex-cli 0.150.1\n› Ask Codex to do anything";
    assert!(!controller_mcp_is_booting_screen(booted));
    assert!(controller_mcp_is_briefing_screen_ready(
        "codex", booted, 400
    ));
}

#[test]
fn self_repainting_status_lines_do_not_restart_the_stability_window() {
    // Each frame of the boot counter and of the `esc to interrupt` status line
    // used to read as a screen change, so stability never reached 300ms and
    // the entire readiness budget burned without a single ready check.
    let first = "codex-cli 0.150.1\nStarting MCP servers (1/6): codex_apps, ...\n› Ask Codex to do anything\nWorking (1s • esc to interrupt)";
    let later = "codex-cli 0.150.1\nStarting MCP servers (5/6): codex_apps, context7, node_repl, ...\n› Ask Codex to do anything\nWorking (7s • esc to interrupt)";
    assert_eq!(
        controller_mcp_briefing_stability_key(first),
        controller_mcp_briefing_stability_key(later)
    );
    // Durable rows still count: filtering the status lines must not blind the
    // window to a menu appearing.
    assert_ne!(
        controller_mcp_briefing_stability_key(later),
        controller_mcp_briefing_stability_key(
            "codex-cli 0.150.1\n› 1. Update now\nWorking (7s • esc to interrupt)"
        )
    );
}

static ENV_LOCK: Mutex<()> = Mutex::new(());

struct UnpeelHomeGuard(Vec<(&'static str, Option<OsString>)>);

impl UnpeelHomeGuard {
    fn set(path: &std::path::Path) -> Self {
        let values = [
            ("UNPEEL_HOME", path.to_path_buf()),
            ("ZERON_WORKTREES_DIR", path.join("worktrees")),
            (
                "ZERON_WORKTREE_OWNERSHIP_FILE",
                path.join("worktree-ownership.json"),
            ),
        ];
        let previous = values
            .iter()
            .map(|(key, _)| (*key, std::env::var_os(key)))
            .collect();
        // SAFETY: every mutation in this test binary holds ENV_LOCK.
        for (key, value) in values {
            unsafe { std::env::set_var(key, value) };
        }
        Self(previous)
    }
}

impl Drop for UnpeelHomeGuard {
    fn drop(&mut self) {
        // SAFETY: the guard is dropped while the test still holds ENV_LOCK.
        unsafe {
            for (key, previous) in self.0.drain(..) {
                match previous {
                    Some(previous) => std::env::set_var(key, previous),
                    None => std::env::remove_var(key),
                }
            }
        }
    }
}

struct EnvironmentVariableGuard {
    key: &'static str,
    previous: Option<OsString>,
}

impl EnvironmentVariableGuard {
    fn set(key: &'static str, value: impl AsRef<std::ffi::OsStr>) -> Self {
        let previous = std::env::var_os(key);
        // SAFETY: every mutation in this test binary holds ENV_LOCK.
        unsafe { std::env::set_var(key, value) };
        Self { key, previous }
    }
}

impl Drop for EnvironmentVariableGuard {
    fn drop(&mut self) {
        // SAFETY: the caller still holds ENV_LOCK when this guard is dropped.
        unsafe {
            match self.previous.take() {
                Some(previous) => std::env::set_var(self.key, previous),
                None => std::env::remove_var(self.key),
            }
        }
    }
}

/// Launch preparation is deliberately made to fail after each checkout is
/// created. The controller must return its stable project id and path so the
/// caller can recover both isolated worktrees; no Worker process is started.
#[test]
fn controller_new_worktree_launch_failure_returns_recoverable_distinct_checkouts()
-> Result<(), Box<dyn std::error::Error>> {
    let _lock = ENV_LOCK.lock();
    let home = TempDir::new()?;
    let _home = UnpeelHomeGuard::set(home.path());

    let repo = home.path().join("repo");
    fs::create_dir_all(&repo)?;
    let git = |args: &[&str]| -> Result<(), Box<dyn std::error::Error>> {
        let status = Command::new("git").args(args).current_dir(&repo).status()?;
        if !status.success() {
            return Err(format!("git {args:?} failed with {status}").into());
        }
        Ok(())
    };
    git(&["init", "--quiet", "--initial-branch=main"])?;
    git(&["config", "user.email", "controller@example.test"])?;
    git(&["config", "user.name", "Controller Test"])?;
    fs::write(repo.join("README.md"), "controller fixture\n")?;
    git(&["add", "README.md"])?;
    git(&["commit", "--quiet", "-m", "fixture"])?;

    let state_path = home.path().join("app-state.json");
    fs::write(
        &state_path,
        serde_json::to_vec_pretty(&json!({
            "projects": [{
                "id": "root",
                "name": "Root",
                "path": repo,
                "workspace_id": "personal",
                "sort_order": 0,
                "is_folder": false
            }],
            "presets": [{
                "id": "claude",
                "label": "Claude",
                "command": "claude",
                "enabled": true,
                "quick_launch": false
            }],
            "active_tabs": {},
            "pinned_sessions": {}
        }))?,
    )?;

    // Trust is checked after checkout creation and before contacting the
    // controller API. A regular file cannot serve as the config directory.
    let blocked_trust_parent = home.path().join("claude-config-is-a-file");
    fs::write(&blocked_trust_parent, "not a directory")?;
    let _trust =
        EnvironmentVariableGuard::set("CLAUDE_CONFIG_DIR", blocked_trust_parent.as_os_str());

    let call = |branch: &str, request_id: u64| {
        controller_mcp_handle_request(json!({
            "jsonrpc": "2.0",
            "id": request_id,
            "method": "tools/call",
            "params": {
                "name": "workers",
                "arguments": {
                    "action": "launch_worker",
                    "project_id": "root",
                    "preset_id": "claude",
                    "new_worktree": { "branch": branch }
                }
            }
        }))
        .expect("tools/call responds")
    };

    let mut recovered = Vec::new();
    for (request_id, branch) in [(21, "feature/controller-a"), (22, "feature/controller-b")] {
        let response = call(branch, request_id);
        assert_eq!(response["result"]["isError"], true, "{response}");
        let message = response["result"]["content"][0]["text"]
            .as_str()
            .expect("tool error text");
        assert!(
            message.contains("recoverable checkout project_id="),
            "{message}"
        );
        assert!(
            message.contains("CLAUDE_CONFIG_DIR") || message.contains("claude-config-is-a-file"),
            "{message}"
        );
        assert!(
            !message.contains("session_id"),
            "a Worker must not have started: {message}"
        );

        let recovery = message
            .split_once("recoverable checkout project_id=")
            .expect("controller preserves the recoverable checkout identity")
            .1;
        let (project_id, path) = recovery
            .split_once(" path=")
            .expect("controller preserves the recoverable checkout path");
        recovered.push((branch, project_id.to_owned(), path.trim().to_owned()));
    }

    assert_ne!(
        recovered[0].1, recovered[1].1,
        "each checkout gets its own project id"
    );
    assert_ne!(
        recovered[0].2, recovered[1].2,
        "each branch gets its own directory"
    );
    for (branch, project_id, path) in &recovered {
        let path = std::path::Path::new(path);
        assert!(
            path.is_dir(),
            "recoverable worktree survives at {}",
            path.display()
        );
        let current_branch = Command::new("git")
            .args([
                "-C",
                path.to_str().expect("UTF-8 fixture path"),
                "branch",
                "--show-current",
            ])
            .output()?;
        assert!(current_branch.status.success());
        assert_eq!(String::from_utf8(current_branch.stdout)?.trim(), *branch);
        let state: serde_json::Value = serde_json::from_slice(&fs::read(&state_path)?)?;
        assert!(
            state["projects"].as_array().unwrap().iter().any(|project| {
                project["id"] == project_id.as_str()
                    && project["path"] == path.to_string_lossy().as_ref()
            }),
            "project identity and exact path remain registered after failed launch"
        );
    }

    // An agent-named branch the user already owns must not be adopted, even
    // when no worktree checks it out.
    git(&["branch", "feature/user-owned"])?;
    let projects_before = fs::read(&state_path)?;
    let response = call("feature/user-owned", 23);
    assert_eq!(response["result"]["isError"], true, "{response}");
    let message = response["result"]["content"][0]["text"]
        .as_str()
        .expect("tool error text");
    assert!(message.contains("already taken"), "{message}");
    assert!(!message.contains("recoverable checkout"), "{message}");
    assert_eq!(fs::read(&state_path)?, projects_before);
    let worktrees = Command::new("git")
        .args(["worktree", "list", "--porcelain"])
        .current_dir(&repo)
        .output()?;
    assert!(
        !String::from_utf8(worktrees.stdout)?.contains("refs/heads/feature/user-owned"),
        "the existing branch was checked out into a Comet worktree"
    );
    Ok(())
}

/// Exercise the real controller-to-Workers launch route without invoking a
/// provider CLI. The fake Host runs the enabled shell preset in the supplied
/// cwd and publishes the same manifest fields the StartingWorker monitor
/// checks. Each independent branch must reach a separate directory.
#[cfg(unix)]
#[test]
fn controller_launches_independent_new_worktrees_in_distinct_cwds_and_returns_hook_warning()
-> Result<(), Box<dyn std::error::Error>> {
    use std::os::unix::fs::PermissionsExt;

    let _lock = ENV_LOCK.lock();
    let home = TempDir::new()?;
    let _home = UnpeelHomeGuard::set(home.path());

    let repo = home.path().join("repo");
    fs::create_dir_all(&repo)?;
    let git = |args: &[&str]| -> Result<(), Box<dyn std::error::Error>> {
        let status = Command::new("git").args(args).current_dir(&repo).status()?;
        if !status.success() {
            return Err(format!("git {args:?} failed with {status}").into());
        }
        Ok(())
    };
    git(&["init", "--quiet", "--initial-branch=main"])?;
    git(&["config", "user.email", "controller@example.test"])?;
    git(&["config", "user.name", "Controller Test"])?;
    fs::write(repo.join("README.md"), "controller fixture\n")?;
    fs::create_dir_all(repo.join(".config"))?;
    fs::write(repo.join(".config/wt.toml"), "post-start = \"true\"\n")?;
    git(&["add", "README.md", ".config/wt.toml"])?;
    git(&["commit", "--quiet", "-m", "fixture"])?;

    let state_path = home.path().join("app-state.json");
    fs::write(
        &state_path,
        serde_json::to_vec_pretty(&json!({
            "projects": [{
                "id": "root",
                "name": "Root",
                "path": repo,
                "workspace_id": "personal",
                "sort_order": 0,
                "is_folder": false
            }],
            "presets": [{
                "id": "test-shell",
                "label": "Test shell",
                "command": "sh -c 'pwd > worker-cwd.txt'",
                "enabled": true,
                "quick_launch": false
            }],
            "active_tabs": {},
            "pinned_sessions": {}
        }))?,
    )?;

    let fake_host = home.path().join("fake-host.py");
    fs::write(
        &fake_host,
        r#"#!/usr/bin/env python3
import json
import os
from pathlib import Path
import subprocess
import sys

launch_path = Path(sys.argv[1])
launch = json.loads(launch_path.read_text())
session = launch["session"]
cwd = Path(launch["cwd"])
completed = subprocess.run(session["command"], cwd=cwd, shell=True, check=False)
session_dir = Path(os.environ["UNPEEL_HOME"]) / "app-sessions" / session["id"]
session_dir.mkdir(parents=True, exist_ok=True)
manifest = {
    "session": session,
    "cwd": str(cwd),
    "state": "exited",
    "pid": None,
    "exit_code": completed.returncode,
}
(session_dir / "manifest.json").write_text(json.dumps(manifest))
sys.exit(completed.returncode)
"#,
    )?;
    let mut permissions = fs::metadata(&fake_host)?.permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&fake_host, permissions)?;
    let _host = EnvironmentVariableGuard::set("UNPEEL_HOST_CMD", &fake_host);

    let call = |branch: &str, request_id: u64| {
        controller_mcp_handle_request(json!({
            "jsonrpc": "2.0",
            "id": request_id,
            "method": "tools/call",
            "params": {
                "name": "workers",
                "arguments": {
                    "action": "launch_worker",
                    "project_id": "root",
                    "preset_id": "test-shell",
                    "new_worktree": { "branch": branch }
                }
            }
        }))
        .expect("tools/call responds")
    };

    let first = call("feature/controller-launch-a", 41);
    let second = call("feature/controller-launch-b", 42);
    for response in [&first, &second] {
        assert_eq!(response["result"]["isError"], false, "{response}");
        assert_eq!(response["result"]["structuredContent"]["launched"], true);
        assert!(
            response["result"]["structuredContent"]["session_id"]
                .as_str()
                .is_some_and(|id| !id.is_empty()),
            "{response}"
        );
        let warning = response["result"]["structuredContent"]["hook_warning"]
            .as_str()
            .unwrap();
        assert!(warning.contains("post-start hook"), "{warning}");
        assert!(warning.contains("pending approval"), "{warning}");
    }

    let first = &first["result"]["structuredContent"];
    let second = &second["result"]["structuredContent"];
    assert_ne!(first["project_id"], second["project_id"]);
    assert_ne!(first["path"], second["path"]);
    assert_ne!(first["session_id"], second["session_id"]);
    for launch in [first, second] {
        let path = std::path::Path::new(launch["path"].as_str().unwrap());
        assert!(
            path.is_dir(),
            "created checkout survives at {}",
            path.display()
        );
        assert_eq!(
            fs::read_to_string(path.join("worker-cwd.txt"))?.trim(),
            path.canonicalize()?.to_string_lossy()
        );
        let manifest =
            unpeel_core::session_host::load_manifest(launch["session_id"].as_str().unwrap())
                .expect("fake Host publishes a manifest for the launched session");
        assert_eq!(manifest.session.project_id, launch["project_id"]);
        assert_eq!(
            std::fs::canonicalize(manifest.cwd)?,
            path.canonicalize()?,
            "Host received the exact isolated checkout cwd"
        );
    }

    // The detached-start guard is intentionally retained until the Host's
    // manifest confirms the checkout and exact project. Wait for its release
    // before restoring UNPEEL_HOME so this test doesn't leak a reservation.
    let client = zeron_workers_unpeel::LocalWorkersClient::new();
    for launch in [first, second] {
        let path = std::path::Path::new(launch["path"].as_str().unwrap());
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        while client.checkout_is_busy(path)? && std::time::Instant::now() < deadline {
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert!(
            !client.checkout_is_busy(path)?,
            "confirmed exited Host manifest releases the StartingWorker reservation"
        );
    }
    Ok(())
}

/// The emitted order is the Presets screen order (fallback order), not
/// "starred first": a starred preset that is NOT first must stay in place,
/// carry `preferred: true`, and a disabled preset must not appear at all.
#[test]
fn list_presets_emits_screen_order_with_fallback_order_and_preferred()
-> Result<(), Box<dyn std::error::Error>> {
    let _lock = ENV_LOCK.lock();
    let home = TempDir::new()?;
    fs::write(
        home.path().join("app-state.json"),
        serde_json::to_vec(&json!({
            "projects": [],
            "presets": [
                { "id": "omp", "label": "OMP CLI", "command": "omp", "enabled": true, "quick_launch": false },
                { "id": "disabled", "label": "pi", "command": "pi", "enabled": false, "quick_launch": true },
                { "id": "claude", "label": "claude", "command": "claude", "enabled": true, "quick_launch": true },
                { "id": "codex", "label": "codex", "command": "codex", "enabled": true, "quick_launch": false }
            ],
            "active_tabs": {},
            "pinned_sessions": {}
        }))?,
    )?;
    let _guard = UnpeelHomeGuard::set(home.path());

    let response = controller_mcp_handle_request(json!({
        "jsonrpc": "2.0",
        "id": 10,
        "method": "tools/call",
        "params": {
            "name": "workers",
            "arguments": { "action": "list_presets" }
        }
    }))
    .expect("tools/call responds");

    assert_eq!(response["result"]["isError"], false);
    let presets = &response["result"]["structuredContent"]["presets"];
    let rows: Vec<(&str, u64, bool)> = presets
        .as_array()
        .expect("presets array")
        .iter()
        .map(|row| {
            (
                row["id"].as_str().expect("id"),
                row["fallback_order"].as_u64().expect("fallback_order"),
                row["preferred"].as_bool().expect("preferred"),
            )
        })
        .collect();
    assert_eq!(
        rows,
        vec![("omp", 1, false), ("claude", 2, true), ("codex", 3, false)]
    );
    assert!(presets[0].get("enabled").is_none());
    Ok(())
}

// ── Project registry: a project is a Space ────────────────────────────────

use support::engine::{FakeEngine, LOCAL_DEVICE};
use zeron_workers_unpeel::space_registry::ENGINE_ENDPOINT_ENV;

/// A committed Git repository at `path`, returned canonical.
fn git_repo(path: &std::path::Path) -> std::path::PathBuf {
    fs::create_dir_all(path).unwrap();
    for args in [
        &["init", "--quiet", "--initial-branch=main"][..],
        &["config", "user.email", "registry@example.test"],
        &["config", "user.name", "Registry Test"],
        &["config", "commit.gpgsign", "false"],
    ] {
        assert!(
            Command::new("git")
                .args(args)
                .current_dir(path)
                .status()
                .unwrap()
                .success()
        );
    }
    fs::write(path.join("README.md"), "registry fixture\n").unwrap();
    for args in [
        &["add", "README.md"][..],
        &["commit", "--quiet", "-m", "fixture"],
    ] {
        assert!(
            Command::new("git")
                .args(args)
                .current_dir(path)
                .status()
                .unwrap()
                .success()
        );
    }
    fs::canonicalize(path).unwrap()
}

fn git_worktree(
    repo: &std::path::Path,
    path: &std::path::Path,
    branch: &str,
) -> std::path::PathBuf {
    assert!(
        Command::new("git")
            .args(["worktree", "add", "--quiet", "-b", branch])
            .arg(path)
            .current_dir(repo)
            .status()
            .unwrap()
            .success()
    );
    fs::canonicalize(path).unwrap()
}

fn write_workers_state(home: &std::path::Path, projects: serde_json::Value) {
    fs::write(
        home.join("app-state.json"),
        serde_json::to_vec(&json!({
            "projects": projects,
            "presets": [{
                "id": "test-shell",
                "label": "Test shell",
                "command": "sh -c 'pwd > worker-cwd.txt'",
                "enabled": true,
                "quick_launch": false
            }],
            "active_tabs": {},
            "pinned_sessions": {}
        }))
        .unwrap(),
    )
    .unwrap();
}

fn workers(arguments: serde_json::Value) -> serde_json::Value {
    controller_mcp_handle_request(json!({
        "jsonrpc": "2.0",
        "id": 70,
        "method": "tools/call",
        "params": { "name": "workers", "arguments": arguments }
    }))
    .expect("tools/call responds")["result"]
        .clone()
}

fn ok(result: serde_json::Value) -> serde_json::Value {
    assert_eq!(result["isError"], false, "{result}");
    result["structuredContent"].clone()
}

fn space_paths(engine: &FakeEngine) -> Vec<String> {
    engine
        .spaces()
        .iter()
        .map(|space| space["path"].as_str().unwrap().to_owned())
        .collect()
}

/// Scenario "Adding from Workers creates the Space".
#[test]
fn adding_from_workers_creates_the_space() {
    let _lock = ENV_LOCK.lock();
    let home = TempDir::new().unwrap();
    let _home = UnpeelHomeGuard::set(home.path());
    write_workers_state(home.path(), json!([]));
    let engine = FakeEngine::start(&[]);
    let _endpoint = EnvironmentVariableGuard::set(ENGINE_ENDPOINT_ENV, &engine.endpoint);
    let folder = git_repo(&home.path().join("fresh"));

    let added = ok(workers(json!({ "action": "add_project", "path": folder })));

    let spaces = engine.spaces();
    assert_eq!(spaces.len(), 1, "one project for the folder");
    assert_eq!(spaces[0]["deviceId"], LOCAL_DEVICE);
    assert_eq!(spaces[0]["path"], folder.to_string_lossy().as_ref());
    assert_eq!(
        added["project_id"], spaces[0]["id"],
        "the response names the Space id"
    );
    assert_eq!(added["path"], folder.to_string_lossy().as_ref());
    assert!(added["checkout_id"].as_str().unwrap().starts_with("comet-"));
}

/// Scenario "Adding a folder twice from different entry points": the
/// Orchestrator created the Space first; Workers reuses it, twice.
#[test]
fn adding_a_folder_twice_from_different_entry_points_reuses_the_project() {
    let _lock = ENV_LOCK.lock();
    let home = TempDir::new().unwrap();
    let _home = UnpeelHomeGuard::set(home.path());
    write_workers_state(home.path(), json!([]));
    let engine = FakeEngine::start(&[]);
    let _endpoint = EnvironmentVariableGuard::set(ENGINE_ENDPOINT_ENV, &engine.endpoint);
    let folder = git_repo(&home.path().join("shared"));
    engine.add_space(
        "space-from-orchestrator",
        LOCAL_DEVICE,
        &folder.to_string_lossy(),
    );

    let first = ok(workers(json!({ "action": "add_project", "path": folder })));
    // Same folder through a non-canonical spelling: still one project.
    let second = ok(workers(
        json!({ "action": "add_project", "path": folder.join(".") }),
    ));

    assert_eq!(first["project_id"], "space-from-orchestrator");
    assert_eq!(second["project_id"], "space-from-orchestrator");
    assert_eq!(first["checkout_id"], second["checkout_id"]);
    assert_eq!(engine.spaces().len(), 1);
}

/// Scenario "Adding a linked worktree": the root has a project, so no new one
/// is created and the worktree becomes its checkout with its branch.
#[test]
fn adding_a_linked_worktree_joins_the_root_project() {
    let _lock = ENV_LOCK.lock();
    let home = TempDir::new().unwrap();
    let _home = UnpeelHomeGuard::set(home.path());
    write_workers_state(home.path(), json!([]));
    let engine = FakeEngine::start(&[]);
    let _endpoint = EnvironmentVariableGuard::set(ENGINE_ENDPOINT_ENV, &engine.endpoint);
    let root = git_repo(&home.path().join("root"));
    let worktree = git_worktree(&root, &home.path().join("root-feature"), "feature/tree");
    engine.add_space("space-root", LOCAL_DEVICE, &root.to_string_lossy());

    let added = ok(workers(
        json!({ "action": "add_project", "path": worktree }),
    ));
    assert_eq!(added["project_id"], "space-root");
    assert_eq!(
        engine.spaces().len(),
        1,
        "no project for the worktree folder"
    );

    let listed = ok(workers(json!({ "action": "list_projects" })));
    let project = &listed["projects"][0];
    assert_eq!(project["id"], "space-root");
    let checkouts = project["checkouts"].as_array().unwrap();
    assert_eq!(checkouts.len(), 1);
    assert_eq!(checkouts[0]["checkout_id"], added["checkout_id"]);
    assert_eq!(checkouts[0]["branch"], "feature/tree");
    assert_eq!(checkouts[0]["principal"], false);
}

/// Scenario "Adding a linked worktree whose root has no Space".
#[test]
fn adding_a_linked_worktree_creates_the_project_of_its_root() {
    let _lock = ENV_LOCK.lock();
    let home = TempDir::new().unwrap();
    let _home = UnpeelHomeGuard::set(home.path());
    write_workers_state(home.path(), json!([]));
    let engine = FakeEngine::start(&[]);
    let _endpoint = EnvironmentVariableGuard::set(ENGINE_ENDPOINT_ENV, &engine.endpoint);
    let root = git_repo(&home.path().join("unregistered-root"));
    let worktree = git_worktree(&root, &home.path().join("orphan-feature"), "feature/orphan");

    let added = ok(workers(
        json!({ "action": "add_project", "path": worktree }),
    ));

    assert_eq!(
        space_paths(&engine),
        vec![root.to_string_lossy().into_owned()]
    );
    assert_eq!(added["project_id"], engine.spaces()[0]["id"]);
    let listed = ok(workers(json!({ "action": "list_projects" })));
    assert_eq!(
        listed["projects"][0]["checkouts"][0]["checkout_id"],
        added["checkout_id"]
    );
    assert!(listed["association_pending"].as_array().unwrap().is_empty());
}

/// Scenario "The engine is unreachable": no endpoint, or an endpoint nobody
/// answers — the add fails naming the registry and writes nothing.
#[test]
fn adding_without_a_reachable_registry_fails_and_writes_nothing() {
    let _lock = ENV_LOCK.lock();
    let home = TempDir::new().unwrap();
    let _home = UnpeelHomeGuard::set(home.path());
    write_workers_state(home.path(), json!([]));
    let folder = git_repo(&home.path().join("offline"));
    let before = fs::read(home.path().join("app-state.json")).unwrap();

    let _unset = EnvironmentVariableGuard::set(ENGINE_ENDPOINT_ENV, "");
    let result = workers(json!({ "action": "add_project", "path": folder }));
    assert_eq!(result["isError"], true, "{result}");
    assert!(
        result["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains("project registry unreachable"),
        "{result}"
    );

    let closed = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let dead = format!("ws://{}", closed.local_addr().unwrap());
    drop(closed);
    let _dead = EnvironmentVariableGuard::set(ENGINE_ENDPOINT_ENV, &dead);
    let result = workers(json!({ "action": "add_project", "path": folder }));
    assert_eq!(result["isError"], true, "{result}");
    assert!(
        result["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains("project registry unreachable"),
        "{result}"
    );
    assert_eq!(
        fs::read(home.path().join("app-state.json")).unwrap(),
        before,
        "no Workers-only registration is written"
    );
}

/// Scenarios "Listing mirrors the chat MCP" and "A Workers-only project cannot
/// exist": registry projects from every device with their device fields; only
/// the local one carries checkouts; an unlinked registration is pending.
#[test]
fn listing_mirrors_the_registry_with_local_checkouts_only() {
    let _lock = ENV_LOCK.lock();
    let home = TempDir::new().unwrap();
    let _home = UnpeelHomeGuard::set(home.path());
    let stray = home.path().join("stray");
    fs::create_dir_all(&stray).unwrap();
    write_workers_state(
        home.path(),
        json!([{ "id": "comet-stray", "name": "stray", "path": stray, "sort_order": 0 }]),
    );
    let engine = FakeEngine::start(&[("device-mini", "Mac mini")]);
    let _endpoint = EnvironmentVariableGuard::set(ENGINE_ENDPOINT_ENV, &engine.endpoint);
    let root = git_repo(&home.path().join("local-repo"));
    let worktree = git_worktree(&root, &home.path().join("local-feature"), "feature/listed");
    engine.add_space("space-local", LOCAL_DEVICE, &root.to_string_lossy());
    engine.add_space(
        "space-remote",
        "device-mini",
        "/Users/other/craft-agents-oss",
    );
    ok(workers(json!({ "action": "add_project", "path": root })));
    ok(workers(
        json!({ "action": "add_project", "path": worktree }),
    ));

    let listed = ok(workers(json!({ "action": "list_projects" })));
    let projects = listed["projects"].as_array().unwrap();
    assert_eq!(projects.len(), 2);
    let local = projects.iter().find(|p| p["id"] == "space-local").unwrap();
    let remote = projects.iter().find(|p| p["id"] == "space-remote").unwrap();
    assert_eq!(local["device_id"], LOCAL_DEVICE);
    assert_eq!(local["device_name"], "This Mac");
    assert_eq!(local["name"], "local-repo");
    assert_eq!(remote["device_id"], "device-mini");
    assert_eq!(remote["device_name"], "Mac mini");
    assert_eq!(remote["path"], "/Users/other/craft-agents-oss");
    assert_eq!(remote["checkouts"], json!([]));
    let branches: Vec<&str> = local["checkouts"]
        .as_array()
        .unwrap()
        .iter()
        .map(|checkout| checkout["branch"].as_str().unwrap())
        .collect();
    assert_eq!(branches.len(), 2);
    assert!(branches.contains(&"main") && branches.contains(&"feature/listed"));
    assert!(projects.iter().all(|p| p["id"] != "comet-stray"));
    assert_eq!(
        listed["association_pending"][0]["checkout_id"],
        "comet-stray"
    );
}

/// Scenario "A Workers-only project cannot exist": after migration over a
/// state whose live registration has no evidence (folder gone, no Git), the
/// registration is not a project — only an association-pending checkout.
#[test]
fn a_workers_only_registration_is_pending_after_migration() {
    let _lock = ENV_LOCK.lock();
    let home = TempDir::new().unwrap();
    let _home = UnpeelHomeGuard::set(home.path());
    let gone = home.path().join("renamed-away");
    write_workers_state(
        home.path(),
        json!([{ "id": "comet-gone", "name": "gone", "path": gone, "sort_order": 0 }]),
    );
    let engine = FakeEngine::start(&[]);
    let _endpoint = EnvironmentVariableGuard::set(ENGINE_ENDPOINT_ENV, &engine.endpoint);
    let registry = zeron_workers_unpeel::space_registry::RpcSpaceRegistry::new(&engine.endpoint);

    zeron_workers_unpeel::space_links::migrate_at(
        &home.path().join("app-state.json"),
        &registry,
        1,
    )
    .unwrap();

    assert!(engine.spaces().is_empty(), "no project was minted for it");
    let listed = ok(workers(json!({ "action": "list_projects" })));
    assert_eq!(listed["projects"], json!([]));
    assert_eq!(
        listed["association_pending"][0]["checkout_id"],
        "comet-gone"
    );
}

// ── Project activity: what Settings → Projects shows, per project ─────────

/// A harness ticket as `work-ticket.sh` writes it, under
/// `<workspace>/brain-source/projects/<brain_project>/tickets/`.
fn write_ticket(
    workspace: &std::path::Path,
    brain_project: &str,
    id: &str,
    status: &str,
    created: &str,
    cwd: &str,
) {
    let tickets = workspace
        .join("brain-source")
        .join("projects")
        .join(brain_project)
        .join("tickets");
    fs::create_dir_all(&tickets).unwrap();
    fs::write(
        tickets.join(format!("{id}.md")),
        format!(
            "---\nid: {id}\ntitle: \"Title of {id}\"\nstatus: {status}\ncreated: \"{created}\"\ncwd: {cwd}\nworkers:\n  - \"w-1\"\nnext: \"next step of {id}\"\n---\n\n# {id}\n"
        ),
    )
    .unwrap();
}

/// A Worker session the Host recorded for `checkout_id`: running (no
/// process to probe, so it stays running) or exited, optionally archived.
fn write_worker_session(
    home: &std::path::Path,
    session_id: &str,
    checkout_id: &str,
    command: &str,
    running: bool,
    archived: bool,
) {
    let dir = home.join("app-sessions").join(session_id);
    fs::create_dir_all(&dir).unwrap();
    fs::write(
        dir.join("manifest.json"),
        serde_json::to_vec(&json!({
            "session": {
                "id": session_id, "project_id": checkout_id, "label": format!("Worker {session_id}"),
                "command": command, "created_at": 1_000
            },
            "cwd": home, "state": if running { "running" } else { "exited" },
            "pid": null, "exit_code": if running { json!(null) } else { json!(0) }
        }))
        .unwrap(),
    )
    .unwrap();
    if archived {
        fs::write(dir.join("archived.json"), r#"{"archived_at": 2000}"#).unwrap();
    }
}

fn ids(list: &serde_json::Value, key: &str) -> Vec<String> {
    list.as_array()
        .unwrap_or_else(|| panic!("not a list: {list}"))
        .iter()
        .map(|item| item[key].as_str().unwrap().to_owned())
        .collect()
}

/// Scenario "A local project with tickets and sessions": tickets matched by
/// checkout `cwd` and by harness folder name, Worker sessions on the
/// principal and a worktree, and the project's chats.
#[test]
fn a_local_project_lists_its_tickets_worker_sessions_and_chats() {
    let _lock = ENV_LOCK.lock();
    let home = TempDir::new().unwrap();
    let _home = UnpeelHomeGuard::set(home.path());
    write_workers_state(home.path(), json!([]));
    let engine = FakeEngine::start(&[]);
    let _endpoint = EnvironmentVariableGuard::set(ENGINE_ENDPOINT_ENV, &engine.endpoint);
    let root = git_repo(&home.path().join("JK Distribuição"));
    assert!(
        Command::new("git")
            .args(["remote", "add", "origin", "https://github.com/acme/jk.git"])
            .current_dir(&root)
            .status()
            .unwrap()
            .success()
    );
    let worktree = git_worktree(&root, &home.path().join("jk-sec-cron"), "sec/cron");
    engine.add_space("space-jk", LOCAL_DEVICE, &root.to_string_lossy());
    let principal = ok(workers(json!({ "action": "add_project", "path": root })));
    let feature = ok(workers(
        json!({ "action": "add_project", "path": worktree }),
    ));
    let principal_checkout = principal["checkout_id"].as_str().unwrap().to_owned();
    let worktree_checkout = feature["checkout_id"].as_str().unwrap().to_owned();

    let workspace = home.path().join("orchestrator");
    let _orch = EnvironmentVariableGuard::set("ORCH_WORKSPACE", &workspace);
    let root_cwd = root.to_string_lossy().into_owned();
    let worktree_cwd = worktree.to_string_lossy().into_owned();
    // By checkout `cwd` (worktree and principal) and by harness folder name.
    write_ticket(
        &workspace,
        "jk-distribuicao",
        "WT-20261001-sec-cron",
        "open",
        "2026-10-01T10:00:00",
        &worktree_cwd,
    );
    write_ticket(
        &workspace,
        "harness-elsewhere",
        "WT-20260930-root-fix",
        "open",
        "2026-09-30T10:00:00",
        &root_cwd,
    );
    write_ticket(
        &workspace,
        "jk-distribuicao",
        "WT-20260920-old",
        "closed-out",
        "2026-09-20T10:00:00",
        "/elsewhere/old-jk",
    );
    write_ticket(
        &workspace,
        "other-project",
        "WT-20260925-other",
        "open",
        "2026-09-25T10:00:00",
        "/p/other",
    );

    write_worker_session(
        home.path(),
        "w-live",
        &principal_checkout,
        "claude",
        true,
        false,
    );
    write_worker_session(
        home.path(),
        "w-archived",
        &worktree_checkout,
        "codex",
        false,
        true,
    );
    engine.add_chat(
        "c-new",
        "space-jk",
        "Plan JK",
        false,
        "2026-10-01T12:00:00Z",
    );
    engine.add_chat("c-old", "space-jk", "Old JK", true, "2026-09-20T12:00:00Z");
    engine.add_chat(
        "c-other",
        "space-other",
        "Other",
        false,
        "2026-10-02T12:00:00Z",
    );
    register_worker_parent_at(
        &home.path().join("app-state.json"),
        "w-live",
        "c-new",
        1_500,
    )
    .unwrap();

    let listed = ok(workers(json!({ "action": "list_projects" })));
    let project = listed["projects"]
        .as_array()
        .unwrap()
        .iter()
        .find(|project| project["id"] == "space-jk")
        .cloned()
        .expect("the project is listed");

    let general = &project["general"];
    assert_eq!(
        general["added_at_unix_ms"], 1_790_640_000_000_u64,
        "{general}"
    );
    assert!(general["last_opened_at_unix_ms"].as_u64().unwrap() >= 1_790_856_000_000);
    assert_eq!(general["remote_url"], "https://github.com/acme/jk.git");
    assert_eq!(general["default_branch"], "main");

    let tickets = &project["tickets"];
    assert_eq!(
        tickets["counts"],
        json!({ "open": 2, "closed-out": 1 }),
        "{tickets}"
    );
    assert_eq!(
        ids(&tickets["open"], "id"),
        vec!["WT-20261001-sec-cron", "WT-20260930-root-fix"]
    );
    assert_eq!(ids(&tickets["recent"], "id"), vec!["WT-20260920-old"]);
    let open = &tickets["open"][0];
    assert_eq!(open["title"], "Title of WT-20261001-sec-cron");
    assert_eq!(open["status"], "open");
    assert_eq!(open["created"], "2026-10-01T10:00:00");
    assert_eq!(open["cwd"], worktree_cwd.as_str());
    assert_eq!(open["next"], "next step of WT-20261001-sec-cron");
    assert!(tickets.get("error").is_none(), "{tickets}");

    let sessions = &project["worker_sessions"];
    assert_eq!(
        sessions["counts"],
        json!({ "live": 1, "stopped": 0, "archived": 1 }),
        "{sessions}"
    );
    assert_eq!(ids(&sessions["live"], "session_id"), vec!["w-live"]);
    assert_eq!(ids(&sessions["archived"], "session_id"), vec!["w-archived"]);
    let live = &sessions["live"][0];
    assert_eq!(live["checkout_id"], principal_checkout.as_str());
    assert_eq!(live["title"], "Worker w-live");
    assert_eq!(live["provider"], "claude");
    assert_eq!(live["state"], "running");
    assert!(live["activity"].is_string() && live["updated_at_unix_ms"].is_u64());
    assert_eq!(
        sessions["archived"][0]["checkout_id"],
        worktree_checkout.as_str()
    );

    let chats = &project["orchestrator_sessions"];
    assert_eq!(
        chats["counts"],
        json!({ "live": 1, "archived": 1 }),
        "{chats}"
    );
    assert_eq!(ids(&chats["recent"], "chat_id"), vec!["c-new", "c-old"]);
    assert_eq!(chats["recent"][0]["title"], "Plan JK");
    assert_eq!(chats["recent"][0]["archived"], false);
    assert_eq!(chats["recent"][0]["workers_launched"], 1);
    assert_eq!(
        chats["recent"][0]["last_activity_unix_ms"],
        1_790_856_000_000_u64
    );

    // The ids are the ones the Settings → Projects tabs list: the same
    // readers over the same registry, history and workspace.
    let settings = settings_tab_ids(&engine, &workspace, "space-jk");
    let mut listed_tickets = ids(&tickets["open"], "id");
    listed_tickets.extend(ids(&tickets["recent"], "id"));
    listed_tickets.sort();
    assert_eq!(listed_tickets, settings.tickets);
    let mut listed_sessions = ids(&sessions["live"], "session_id");
    listed_sessions.extend(ids(&sessions["archived"], "session_id"));
    listed_sessions.sort();
    assert_eq!(listed_sessions, settings.sessions);
    assert_eq!(ids(&chats["recent"], "chat_id"), settings.chats);
}

/// What the Settings → Projects Tickets, Worker sessions and Orchestrator
/// sessions tabs list for `space_id`, through the readers that page renders.
struct SettingsTabIds {
    tickets: Vec<String>,
    sessions: Vec<String>,
    chats: Vec<String>,
}

fn settings_tab_ids(
    engine: &FakeEngine,
    workspace: &std::path::Path,
    space_id: &str,
) -> SettingsTabIds {
    use zeron_workers_unpeel::{project_activity, project_tickets};
    let registry = zeron_workers_unpeel::space_registry::RpcSpaceRegistry::new(&engine.endpoint)
        .read_with_chats()
        .unwrap();
    let chats = registry.chats.unwrap();
    let client = zeron_workers_unpeel::LocalWorkersClient::new();
    let (rows, _) = project_activity::checkout_rows(&client).unwrap();
    let (entries, _) = project_activity::project_entries(
        &registry.spaces,
        &registry.devices,
        Some(&registry.local_device_id),
        &chats,
        &rows,
    );
    let entry = entries
        .iter()
        .find(|entry| entry.space.id == space_id)
        .unwrap();
    let workers = project_activity::with_archived_sessions(
        &client,
        client.bootstrap().unwrap().sessions,
        &rows,
    );
    let tickets = project_tickets::load_tickets(workspace).unwrap();
    let mut ticket_ids: Vec<String> = project_tickets::tickets_for_project(entry, &tickets)
        .iter()
        .map(|ticket| ticket.id.clone())
        .collect();
    ticket_ids.sort();
    let mut session_ids: Vec<String> = project_activity::project_sessions(entry, &workers)
        .iter()
        .map(|(session, _)| session.id.clone())
        .collect();
    session_ids.sort();
    SettingsTabIds {
        tickets: ticket_ids,
        sessions: session_ids,
        chats: project_activity::project_chats(entry, &chats)
            .iter()
            .map(|chat| chat.id.clone())
            .collect(),
    }
}

fn listed_project(listed: &serde_json::Value, id: &str) -> serde_json::Value {
    listed["projects"]
        .as_array()
        .unwrap()
        .iter()
        .find(|project| project["id"] == id)
        .cloned()
        .unwrap_or_else(|| panic!("{id} is listed: {listed}"))
}

/// Scenario "A ticket matches only its own project": each project's
/// `tickets` holds only the tickets whose `cwd` is one of its checkouts.
#[test]
fn a_ticket_matches_only_its_own_project() {
    let _lock = ENV_LOCK.lock();
    let home = TempDir::new().unwrap();
    let _home = UnpeelHomeGuard::set(home.path());
    write_workers_state(home.path(), json!([]));
    let engine = FakeEngine::start(&[]);
    let _endpoint = EnvironmentVariableGuard::set(ENGINE_ENDPOINT_ENV, &engine.endpoint);
    let alpha = git_repo(&home.path().join("alpha"));
    let beta = git_repo(&home.path().join("beta"));
    engine.add_space("space-alpha", LOCAL_DEVICE, &alpha.to_string_lossy());
    engine.add_space("space-beta", LOCAL_DEVICE, &beta.to_string_lossy());
    ok(workers(json!({ "action": "add_project", "path": alpha })));
    ok(workers(json!({ "action": "add_project", "path": beta })));
    let workspace = home.path().join("orchestrator");
    let _orch = EnvironmentVariableGuard::set("ORCH_WORKSPACE", &workspace);
    // Filed under a harness folder named after neither project.
    let alpha_cwd = alpha.join("src").to_string_lossy().into_owned();
    let beta_cwd = beta.to_string_lossy().into_owned();
    write_ticket(
        &workspace,
        "harness",
        "WT-20261001-alpha",
        "open",
        "2026-10-01T10:00:00",
        &alpha_cwd,
    );
    write_ticket(
        &workspace,
        "harness",
        "WT-20261001-beta",
        "accepted",
        "2026-10-01T11:00:00",
        &beta_cwd,
    );
    // A sibling folder sharing the name prefix is not a checkout of alpha.
    let prefix_cwd = format!("{}-old", alpha.to_string_lossy());
    write_ticket(
        &workspace,
        "harness",
        "WT-20261001-prefix",
        "open",
        "2026-10-01T12:00:00",
        &prefix_cwd,
    );

    let listed = ok(workers(json!({ "action": "list_projects" })));

    let alpha = listed_project(&listed, "space-alpha");
    assert_eq!(alpha["tickets"]["counts"], json!({ "open": 1 }), "{alpha}");
    assert_eq!(
        ids(&alpha["tickets"]["open"], "id"),
        vec!["WT-20261001-alpha"]
    );
    let beta = listed_project(&listed, "space-beta");
    assert_eq!(
        beta["tickets"]["counts"],
        json!({ "accepted": 1 }),
        "{beta}"
    );
    assert_eq!(
        ids(&beta["tickets"]["recent"], "id"),
        vec!["WT-20261001-beta"]
    );
    assert_eq!(beta["tickets"]["open"], json!([]));
}

/// Scenario "The Orchestrator workspace is missing": every project is still
/// listed, each `tickets` empty and naming the unreadable source.
#[test]
fn a_missing_orchestrator_workspace_leaves_tickets_empty_and_named() {
    let _lock = ENV_LOCK.lock();
    let home = TempDir::new().unwrap();
    let _home = UnpeelHomeGuard::set(home.path());
    write_workers_state(home.path(), json!([]));
    let engine = FakeEngine::start(&[("device-mini", "Mac mini")]);
    let _endpoint = EnvironmentVariableGuard::set(ENGINE_ENDPOINT_ENV, &engine.endpoint);
    let local = git_repo(&home.path().join("local-repo"));
    engine.add_space("space-local", LOCAL_DEVICE, &local.to_string_lossy());
    engine.add_space("space-remote", "device-mini", "/Users/other/remote-repo");
    ok(workers(json!({ "action": "add_project", "path": local })));
    let missing = home.path().join("no-such-workspace");
    let _orch = EnvironmentVariableGuard::set("ORCH_WORKSPACE", &missing);

    let listed = ok(workers(json!({ "action": "list_projects" })));

    let projects = listed["projects"].as_array().unwrap();
    assert_eq!(projects.len(), 2, "{listed}");
    for project in projects {
        let tickets = &project["tickets"];
        assert_eq!(tickets["counts"], json!({}), "{tickets}");
        assert_eq!(tickets["open"], json!([]));
        assert_eq!(tickets["recent"], json!([]));
        let error = tickets["error"]
            .as_str()
            .expect("the section names its error");
        assert!(error.contains(&*missing.to_string_lossy()), "{error}");
        // The other sections still answer.
        assert!(project["orchestrator_sessions"].get("error").is_none());
    }
}

/// Scenario "A remote project's activity": Worker sessions are device-local,
/// so a project of another device has none; its chats are listed.
#[test]
fn a_remote_project_lists_its_chats_but_no_worker_sessions() {
    let _lock = ENV_LOCK.lock();
    let home = TempDir::new().unwrap();
    let _home = UnpeelHomeGuard::set(home.path());
    write_workers_state(home.path(), json!([]));
    let engine = FakeEngine::start(&[("device-mini", "Mac mini")]);
    let _endpoint = EnvironmentVariableGuard::set(ENGINE_ENDPOINT_ENV, &engine.endpoint);
    let workspace = home.path().join("orchestrator");
    fs::create_dir_all(workspace.join("brain-source").join("projects")).unwrap();
    let _orch = EnvironmentVariableGuard::set("ORCH_WORKSPACE", &workspace);
    // Same folder name on this device: a local Worker session there must not
    // leak into the remote project.
    let local = git_repo(&home.path().join("craft"));
    engine.add_space("space-local", LOCAL_DEVICE, &local.to_string_lossy());
    engine.add_space("space-remote", "device-mini", "/Users/other/craft");
    let added = ok(workers(json!({ "action": "add_project", "path": local })));
    write_worker_session(
        home.path(),
        "w-local",
        added["checkout_id"].as_str().unwrap(),
        "claude",
        true,
        false,
    );
    engine.add_chat(
        "c-remote-new",
        "space-remote",
        "Remote plan",
        false,
        "2026-10-01T12:00:00Z",
    );
    engine.add_chat(
        "c-remote-old",
        "space-remote",
        "Remote old",
        true,
        "2026-09-01T12:00:00Z",
    );

    let listed = ok(workers(json!({ "action": "list_projects" })));

    let remote = listed_project(&listed, "space-remote");
    let sessions = &remote["worker_sessions"];
    assert_eq!(
        sessions["counts"],
        json!({ "live": 0, "stopped": 0, "archived": 0 }),
        "{sessions}"
    );
    assert_eq!(sessions["live"], json!([]));
    assert!(sessions.get("error").is_none());
    let chats = &remote["orchestrator_sessions"];
    assert_eq!(
        chats["counts"],
        json!({ "live": 1, "archived": 1 }),
        "{chats}"
    );
    assert_eq!(
        ids(&chats["recent"], "chat_id"),
        vec!["c-remote-new", "c-remote-old"]
    );
    assert_eq!(remote["general"]["remote_url"], json!(null));
    let local = listed_project(&listed, "space-local");
    assert_eq!(
        ids(&local["worker_sessions"]["live"], "session_id"),
        vec!["w-local"]
    );
}

fn install_fake_host(home: &std::path::Path) -> EnvironmentVariableGuard {
    use std::os::unix::fs::PermissionsExt;
    let fake_host = home.join("fake-host.py");
    fs::write(
        &fake_host,
        r#"#!/usr/bin/env python3
import json, os, subprocess, sys
from pathlib import Path
launch = json.loads(Path(sys.argv[1]).read_text())
session = launch["session"]
cwd = Path(launch["cwd"])
completed = subprocess.run(session["command"], cwd=cwd, shell=True, check=False)
session_dir = Path(os.environ["UNPEEL_HOME"]) / "app-sessions" / session["id"]
session_dir.mkdir(parents=True, exist_ok=True)
(session_dir / "manifest.json").write_text(json.dumps({
    "session": session, "cwd": str(cwd), "state": "exited", "pid": None,
    "exit_code": completed.returncode,
}))
sys.exit(completed.returncode)
"#,
    )
    .unwrap();
    let mut permissions = fs::metadata(&fake_host).unwrap().permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&fake_host, permissions).unwrap();
    EnvironmentVariableGuard::set("UNPEEL_HOST_CMD", &fake_host)
}

fn wait_released(path: &std::path::Path) {
    let client = zeron_workers_unpeel::LocalWorkersClient::new();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while client.checkout_is_busy(path).unwrap() && std::time::Instant::now() < deadline {
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
}

/// Scenarios "Launching into a project" (principal registered on demand) and
/// "Launching into an exact checkout".
#[cfg(unix)]
#[test]
fn launching_by_project_runs_in_the_principal_and_by_checkout_runs_there() {
    let _lock = ENV_LOCK.lock();
    let home = TempDir::new().unwrap();
    let _home = UnpeelHomeGuard::set(home.path());
    write_workers_state(home.path(), json!([]));
    let _host = install_fake_host(home.path());
    let engine = FakeEngine::start(&[]);
    let _endpoint = EnvironmentVariableGuard::set(ENGINE_ENDPOINT_ENV, &engine.endpoint);
    let root = git_repo(&home.path().join("jk"));
    let worktree = git_worktree(&root, &home.path().join("jk-sec"), "sec/cron");
    // Only the worktree is registered: its principal has no execution record.
    let added = ok(workers(
        json!({ "action": "add_project", "path": worktree }),
    ));
    let project_id = added["project_id"].as_str().unwrap().to_owned();

    let launched = ok(workers(json!({
        "action": "launch_worker", "project_id": project_id, "preset_id": "test-shell"
    })));
    wait_released(&root);
    assert_eq!(
        fs::read_to_string(root.join("worker-cwd.txt"))
            .unwrap()
            .trim(),
        root.to_string_lossy()
    );
    let manifest =
        unpeel_core::session_host::load_manifest(launched["session_id"].as_str().unwrap())
            .expect("launched session manifest");
    assert_eq!(manifest.session.project_id, launched["checkout_id"]);
    let listed = ok(workers(json!({ "action": "list_projects" })));
    let principal = listed["projects"][0]["checkouts"]
        .as_array()
        .unwrap()
        .iter()
        .find(|checkout| checkout["principal"] == true)
        .cloned()
        .expect("the principal was registered on demand");
    assert_eq!(principal["checkout_id"], launched["checkout_id"]);
    assert_eq!(
        engine.spaces().len(),
        1,
        "no second project for the principal"
    );

    let exact = ok(workers(json!({
        "action": "launch_worker", "project_id": added["checkout_id"], "preset_id": "test-shell"
    })));
    wait_released(&worktree);
    assert_eq!(exact["checkout_id"], added["checkout_id"]);
    assert_eq!(
        fs::read_to_string(worktree.join("worker-cwd.txt"))
            .unwrap()
            .trim(),
        worktree.to_string_lossy()
    );
}

/// Scenario "Launching into a remote project": refused before spawn, naming
/// the owning device; no session exists afterwards.
#[cfg(unix)]
#[test]
fn launching_into_a_remote_project_fails_before_spawn() {
    let _lock = ENV_LOCK.lock();
    let home = TempDir::new().unwrap();
    let _home = UnpeelHomeGuard::set(home.path());
    write_workers_state(home.path(), json!([]));
    let _host = install_fake_host(home.path());
    let engine = FakeEngine::start(&[("device-mini", "Mac mini")]);
    let _endpoint = EnvironmentVariableGuard::set(ENGINE_ENDPOINT_ENV, &engine.endpoint);
    engine.add_space(
        "space-remote",
        "device-mini",
        "/Users/other/craft-agents-oss",
    );
    let before = fs::read(home.path().join("app-state.json")).unwrap();

    let result = workers(json!({
        "action": "launch_worker", "project_id": "space-remote", "preset_id": "test-shell"
    }));
    assert_eq!(result["isError"], true, "{result}");
    assert!(
        result["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains("Mac mini"),
        "{result}"
    );
    let workers_listed = ok(workers(json!({ "action": "list_workers" })));
    assert_eq!(workers_listed["workers"], json!([]));
    assert_eq!(
        fs::read(home.path().join("app-state.json")).unwrap(),
        before
    );
}

#[test]
fn controller_mcp_prepares_the_current_binary_as_session_host() {
    let _lock = ENV_LOCK.lock();
    let previous = std::env::var_os("UNPEEL_HOST_CMD");
    // SAFETY: this test binary serializes its environment mutations with ENV_LOCK.
    unsafe { std::env::remove_var("UNPEEL_HOST_CMD") };

    ensure_controller_mcp_host_launcher().expect("controller configures its host launcher");

    assert_eq!(
        std::env::var_os("UNPEEL_HOST_CMD").map(std::path::PathBuf::from),
        std::env::current_exe().ok()
    );
    // SAFETY: restore the process environment before releasing ENV_LOCK.
    unsafe {
        match previous {
            Some(value) => std::env::set_var("UNPEEL_HOST_CMD", value),
            None => std::env::remove_var("UNPEEL_HOST_CMD"),
        }
    }
}

#[test]
fn controller_authority_marker_is_consumed_before_workers_launch() {
    let _lock = ENV_LOCK.lock();
    let previous = std::env::var_os("COMET_WORKERS_CONTROLLER");
    // SAFETY: this test binary serializes its environment mutations with ENV_LOCK.
    unsafe { std::env::set_var("COMET_WORKERS_CONTROLLER", "1") };

    controller_mcp_consume_authority_marker().expect("valid marker is consumed");

    assert!(std::env::var_os("COMET_WORKERS_CONTROLLER").is_none());
    // SAFETY: restore before releasing ENV_LOCK.
    unsafe {
        match previous {
            Some(value) => std::env::set_var("COMET_WORKERS_CONTROLLER", value),
            None => std::env::remove_var("COMET_WORKERS_CONTROLLER"),
        }
    }
}

#[test]
fn controller_parent_chat_identity_is_consumed_before_worker_descendants_spawn() {
    let _lock = ENV_LOCK.lock();
    let previous = std::env::var_os("COMET_WORKERS_PARENT_CHAT_ID");
    // SAFETY: this test binary serializes its environment mutations with ENV_LOCK.
    unsafe { std::env::set_var("COMET_WORKERS_PARENT_CHAT_ID", " parent-chat-1 ") };

    assert_eq!(
        controller_mcp_take_parent_chat_id().as_deref(),
        Some("parent-chat-1")
    );
    assert!(std::env::var_os("COMET_WORKERS_PARENT_CHAT_ID").is_none());

    // SAFETY: restore before releasing ENV_LOCK.
    unsafe {
        match previous {
            Some(value) => std::env::set_var("COMET_WORKERS_PARENT_CHAT_ID", value),
            None => std::env::remove_var("COMET_WORKERS_PARENT_CHAT_ID"),
        }
    }
}

#[test]
fn task_episode_tracking_requires_a_parent_and_a_submitted_prompt() {
    assert!(controller_mcp_tracks_task_episode(
        Some("parent-chat"),
        true
    ));
    assert!(!controller_mcp_tracks_task_episode(None, true));
    assert!(!controller_mcp_tracks_task_episode(
        Some("parent-chat"),
        false
    ));
}

#[test]
fn archive_requires_an_explicit_stop_for_live_workers() {
    let mut session = worker_with_state("running");
    assert!(controller_mcp_archive_guard(&session).is_err());

    session.state = "exited".into();
    assert!(controller_mcp_archive_guard(&session).is_ok());
}

/// Hibernation stops idle Workers behind the orchestrator's back, so writing
/// to one must name the way back instead of typing into a dead PTY.
#[test]
fn writing_to_a_hibernated_worker_names_restart_worker() {
    let mut session = worker_with_state("exited");
    session.archived = true;
    let error =
        controller_mcp_live_worker_guard(&session).expect_err("a stopped Worker refuses input");
    assert!(
        error.contains("restart_worker"),
        "the guard must name the action that brings the Worker back: {error}"
    );

    let live = worker_with_state("running");
    assert!(controller_mcp_live_worker_guard(&live).is_ok());
}

/// Restarting replaces the Session, so the orchestrator needs the id it got
/// back, not the one it asked with.
#[test]
fn restart_reports_the_replacement_session_id() {
    let before = ["worker-1".to_owned(), "worker-2".to_owned()];
    let after = ["worker-2".to_owned(), "worker-3".to_owned()];
    assert_eq!(
        controller_mcp_replacement_session_id(&before, &after),
        Some("worker-3".to_owned())
    );
    // An ambiguous or unchanged listing must not invent an id.
    assert_eq!(
        controller_mcp_replacement_session_id(&before, &before),
        None
    );
    assert_eq!(
        controller_mcp_replacement_session_id(
            &before,
            &["worker-3".to_owned(), "worker-4".to_owned()]
        ),
        None
    );
}

#[test]
fn controller_text_uses_the_runtime_sanitizer() {
    assert_eq!(
        controller_mcp_sanitize_text("hello\u{0} world\r\nnext\u{1b}"),
        "hello world\nnext"
    );
}

fn worker_with_state(state: &str) -> WorkersSession {
    WorkersSession {
        id: "worker-1".into(),
        project_id: "project-1".into(),
        title: "Worker".into(),
        command: "codex".into(),
        state: state.into(),
        activity: "idle".into(),
        unread: false,
        pinned: false,
        archived: false,
        provider_id: None,
        active_runtime_id: None,
        runtime_launch_pending: false,
        runtime_generation: 1,
        notify_when_done: false,
        terminal_background_hex: None,
        worktree_branch: None,
        created_at_unix_ms: 1,
        updated_at_unix_ms: 1,
        idle_since_unix_ms: None,
        idle_confirmed_by_hook: false,
        resumable_conversation: false,
        total_tokens: None,
        model_usage: Vec::new(),
        capabilities: WorkersSessionCapabilities::default(),
    }
}

#[test]
fn workers_serve_answers_ping_while_a_request_is_pending_and_drops_cancelled_requests() {
    use std::io::Cursor;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};

    struct SharedWriter(Arc<Mutex<Vec<u8>>>);
    impl std::io::Write for SharedWriter {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.0.lock().extend_from_slice(buf);
            Ok(buf.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    let input = concat!(
        r#"{"jsonrpc":"2.0","id":1,"method":"block","params":{}}"#,
        "\n",
        r#"{"jsonrpc":"2.0","id":2,"method":"ping","params":{}}"#,
        "\n",
        r#"{"jsonrpc":"2.0","method":"notifications/cancelled","params":{"requestId":1}}"#,
        "\n",
    );
    let output = Arc::new(Mutex::new(Vec::new()));
    let handler = |request: serde_json::Value, cancel: &AtomicBool| {
        let id = request["id"].clone();
        if request["method"] == "block" {
            // A serial loop would never read the cancellation line: this only
            // returns once `serve` processed it concurrently.
            while !cancel.load(Ordering::SeqCst) {
                std::thread::sleep(std::time::Duration::from_millis(20));
            }
        }
        Some(json!({ "jsonrpc": "2.0", "id": id, "result": {} }))
    };
    zeron_workers_unpeel::controller_mcp_serve(
        Cursor::new(input),
        SharedWriter(Arc::clone(&output)),
        handler,
    )
    .expect("serve drains its input");

    let written = String::from_utf8(output.lock().clone()).unwrap();
    let ids: Vec<serde_json::Value> = written
        .lines()
        .map(|line| serde_json::from_str::<serde_json::Value>(line).unwrap()["id"].clone())
        .collect();
    assert_eq!(
        ids,
        vec![json!(2)],
        "ping is answered and the cancelled request is not: {written}"
    );
}

#[test]
fn workers_wait_until_times_out_with_next_guidance_and_stops_on_cancel() {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::time::{Duration, Instant};

    let cancel = AtomicBool::new(false);
    let started = Instant::now();
    let result =
        zeron_workers_unpeel::controller_mcp_wait_until(1, "gone-for-test", &cancel, || {
            Ok(worker_with_state("running"))
        })
        .expect("timeout is a normal read");
    assert!(started.elapsed() >= Duration::from_secs(1));
    assert_eq!(result["timed_out"], true);
    assert_eq!(result["worker"]["state"], "running");
    let next = result["next"].as_str().expect("next guidance");
    assert!(
        next.contains("[worker-task-notification]") && next.contains("end your turn"),
        "{next}"
    );
    assert_eq!(next, zeron_workers_unpeel::WAIT_TIMED_OUT_NEXT);

    let cancel = Arc::new(AtomicBool::new(false));
    let flip = Arc::clone(&cancel);
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(300));
        flip.store(true, Ordering::SeqCst);
    });
    let started = Instant::now();
    let error =
        zeron_workers_unpeel::controller_mcp_wait_until(60, "gone-for-test", &cancel, || {
            Ok(worker_with_state("running"))
        })
        .expect_err("cancellation interrupts the wait");
    assert!(error.contains("cancelled"), "{error}");
    assert!(
        started.elapsed() < Duration::from_secs(5),
        "cancel must not wait for the deadline"
    );

    let matched =
        zeron_workers_unpeel::controller_mcp_wait_until(60, "gone-for-test", &cancel, || {
            Ok(worker_with_state("gone-for-test"))
        })
        .expect("match wins even when cancelled");
    assert_eq!(matched["matched"], true);
}

#[test]
fn workers_wait_for_status_ceiling_is_orchestrator_owned_and_documented() {
    assert_eq!(
        zeron_workers_unpeel::WAIT_FOR_STATUS_MAX_TIMEOUT_SECONDS,
        4 * 60 * 60,
        "the ceiling is a 4h transport sanity bound; the orchestrator picks the actual wait"
    );

    let tools = controller_mcp_handle_request(json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/list",
        "params": {}
    }))
    .expect("tools/list responds");

    let tool = &tools["result"]["tools"][0];
    let timeout_prop = &tool["inputSchema"]["properties"]["timeout_seconds"];

    let max_timeout = timeout_prop["maximum"].as_u64().expect("maximum is u64");
    assert_eq!(
        max_timeout,
        zeron_workers_unpeel::WAIT_FOR_STATUS_MAX_TIMEOUT_SECONDS,
        "the tools/list schema must derive its maximum blocking wait ceiling from WAIT_FOR_STATUS_MAX_TIMEOUT_SECONDS"
    );

    let help_response = controller_mcp_handle_request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": {
            "name": "workers",
            "arguments": { "action": "help" }
        }
    }))
    .expect("help action responds");
    let help_wait_limit = help_response["result"]["structuredContent"]["limits"]["wait_seconds"]
        .as_u64()
        .expect("wait_seconds in help limits");
    assert_eq!(
        help_wait_limit,
        zeron_workers_unpeel::WAIT_FOR_STATUS_MAX_TIMEOUT_SECONDS,
        "help limits must derive wait_seconds from WAIT_FOR_STATUS_MAX_TIMEOUT_SECONDS"
    );

    assert_eq!(
        zeron_workers_unpeel::clamp_wait_for_status_timeout(Some(1_000_000)),
        zeron_workers_unpeel::WAIT_FOR_STATUS_MAX_TIMEOUT_SECONDS,
        "clamp must bound upper timeouts to WAIT_FOR_STATUS_MAX_TIMEOUT_SECONDS"
    );
    assert_eq!(
        zeron_workers_unpeel::clamp_wait_for_status_timeout(Some(0)),
        1,
        "clamp must bound lower timeouts to 1"
    );
    assert_eq!(
        zeron_workers_unpeel::clamp_wait_for_status_timeout(None),
        30,
        "default timeout when unspecified must be 30s"
    );
    assert_eq!(
        zeron_workers_unpeel::clamp_wait_for_status_timeout(Some(60)),
        60,
        "valid timeouts within bounds must be preserved"
    );

    let desc = timeout_prop["description"].as_str().expect("description");
    assert!(
        desc.contains("timed_out: true"),
        "timeout_seconds description must explicitly name timed_out: true: {desc}"
    );
    assert!(
        desc.contains("snapshot"),
        "timeout_seconds description must explain worker snapshot: {desc}"
    );
    assert!(
        !desc.contains("replaces manual polling"),
        "timeout_seconds description must not claim it replaces manual polling: {desc}"
    );
    let sentence_count = desc.split('.').filter(|s| !s.trim().is_empty()).count();
    assert!(
        sentence_count <= 2,
        "timeout_seconds description must be concise and at most 2 sentences, got {sentence_count}: {desc}"
    );
}

fn write_stop_hook(root: &std::path::Path, session_id: &str, generation: u64) {
    let dir = root.join(session_id);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("comet-hook-events.jsonl"),
        format!(
            "{}\n",
            serde_json::to_string(&json!({
                "sequence": 1,
                "hook_event_name": "Stop",
                "runtime_generation": generation,
                "occurred_at_unix_ms": 1_000
            }))
            .unwrap()
        ),
    )
    .unwrap();
}

#[test]
fn wait_for_completed_matches_live_idle_worker_with_current_episode_evidence() {
    use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
    use std::time::{Duration, Instant};

    let dir = TempDir::new().unwrap();
    let path = dir.path().join("app-state.json");
    std::fs::write(
        &path,
        serde_json::to_vec(&json!({
            "projects": [],
            "presets": [],
            "active_tabs": {},
            "pinned_sessions": {}
        }))
        .unwrap(),
    )
    .unwrap();
    register_worker_parent_at(&path, "worker-1", "parent-chat-1", 900).unwrap();
    begin_worker_parent_task_at(&path, "worker-1", 950).unwrap();
    let sessions_root = dir.path().join("sessions");
    write_stop_hook(&sessions_root, "worker-1", 1);

    let cancel = AtomicBool::new(false);
    let polls = AtomicU32::new(0);
    let started = Instant::now();
    let result = zeron_workers_unpeel::controller_mcp_wait_until_matching(
        1800,
        "completed",
        &cancel,
        || {
            let n = polls.fetch_add(1, Ordering::SeqCst);
            assert!(
                n < 3,
                "completed wait polled {n} times instead of matching current-episode evidence on a live idle Worker"
            );
            Ok(worker_with_state("running"))
        },
        |session| {
            current_episode_completed_with_evidence_at(
                &path,
                session,
                &sessions_root,
                WorkerCompletionEvidence::quiescent(),
            )
            .unwrap_or(false)
        },
    )
    .expect("completed should match");
    assert_eq!(result["matched"], true);
    assert!(
        started.elapsed() < Duration::from_secs(2),
        "wait must not consume the 1800s timeout"
    );
}

#[test]
fn wait_for_status_rejects_unknown_status_immediately() {
    let _lock = ENV_LOCK.lock();
    let home = TempDir::new().unwrap();
    let path = home.path().join("app-state.json");
    std::fs::write(
        &path,
        serde_json::to_vec(&json!({
            "projects": [],
            "presets": [],
            "active_tabs": {},
            "pinned_sessions": {}
        }))
        .unwrap(),
    )
    .unwrap();
    let session_dir = home.path().join("app-sessions").join("worker-live");
    std::fs::create_dir_all(&session_dir).unwrap();
    std::fs::write(
        session_dir.join("manifest.json"),
        serde_json::to_vec(&json!({
            "session": {
                "id": "worker-live",
                "project_id": "project-1",
                "label": "Worker Live",
                "command": "claude",
                "created_at": 1000
            },
            "cwd": "/tmp",
            "state": "running",
            "pid": 12345,
            "exit_code": null,
            "has_been_written_to": true,
            "updated_at": 1000
        }))
        .unwrap(),
    )
    .unwrap();
    let _guard = UnpeelHomeGuard::set(home.path());

    let started = std::time::Instant::now();
    let response = controller_mcp_handle_request(json!({
        "jsonrpc": "2.0",
        "id": 99,
        "method": "tools/call",
        "params": {
            "name": "workers",
            "arguments": {
                "action": "wait_for_status",
                "session_id": "worker-live",
                "status": "nonexistent_status",
                "timeout_seconds": 60
            }
        }
    }))
    .expect("tools/call responds");

    let elapsed = started.elapsed();
    assert!(
        elapsed < std::time::Duration::from_millis(500),
        "unknown status must return immediately without waiting, took {elapsed:?}"
    );
    assert_eq!(response["result"]["isError"], true);
    let error_text = response["result"]["content"][0]["text"]
        .as_str()
        .expect("error text in content");
    assert!(
        error_text.contains("Unknown status 'nonexistent_status'"),
        "error must mention the unknown status: {error_text}"
    );
    for accepted in &[
        "completed",
        "running",
        "exited",
        "starting",
        "working",
        "blocked",
        "done",
        "idle",
    ] {
        assert!(
            error_text.contains(accepted),
            "error must list accepted status '{accepted}': {error_text}"
        );
    }
}

#[test]
fn read_transcript_returns_the_conversation_of_a_managed_omp_worker() {
    let _lock = ENV_LOCK.lock();
    let home = TempDir::new().unwrap();
    std::fs::write(
        home.path().join("app-state.json"),
        serde_json::to_vec(&json!({
            "projects": [],
            "presets": [],
            "active_tabs": {},
            "pinned_sessions": {}
        }))
        .unwrap(),
    )
    .unwrap();
    let storage = home.path().join("pi-sessions").join("worker-omp");
    std::fs::create_dir_all(&storage).unwrap();
    let transcript = storage.join("20260930_omp-provider-1.jsonl");
    std::fs::write(
        &transcript,
        concat!(
            r#"{"type":"message","message":{"role":"user","content":[{"type":"text","text":"briefing from the orchestrator"}]}}"#,
            "\n",
            r#"{"type":"message","message":{"role":"assistant","content":[{"type":"text","text":"report from the worker"}]}}"#,
            "\n",
        ),
    )
    .unwrap();
    let session_dir = home.path().join("app-sessions").join("worker-omp");
    std::fs::create_dir_all(&session_dir).unwrap();
    std::fs::write(
        session_dir.join("manifest.json"),
        serde_json::to_vec(&json!({
            "session": {
                "id": "worker-omp",
                "project_id": "project-1",
                "label": "Worker OMP",
                "command": "omp",
                "created_at": 1000
            },
            "cwd": "/tmp",
            "state": "running",
            "pid": 12345,
            "exit_code": null,
            "has_been_written_to": true,
            "updated_at": 1000,
            "managed_storage_path": storage.to_string_lossy(),
            "provider_transcript_path": transcript.to_string_lossy()
        }))
        .unwrap(),
    )
    .unwrap();
    let _guard = UnpeelHomeGuard::set(home.path());

    let response = controller_mcp_handle_request(json!({
        "jsonrpc": "2.0",
        "id": 7,
        "method": "tools/call",
        "params": {
            "name": "workers",
            "arguments": { "action": "read_transcript", "session_id": "worker-omp" }
        }
    }))
    .expect("tools/call responds");

    assert_ne!(response["result"]["isError"], true, "{response}");
    let markdown = response["result"]["structuredContent"]["markdown"]
        .as_str()
        .unwrap_or_else(|| panic!("markdown in structured content: {response}"));
    assert!(
        markdown.contains("briefing from the orchestrator"),
        "{markdown}"
    );
    assert!(markdown.contains("report from the worker"), "{markdown}");
}

#[test]
fn wait_for_status_completed_on_untracked_worker_is_rejected_immediately() {
    let _lock = ENV_LOCK.lock();
    let home = TempDir::new().unwrap();
    let path = home.path().join("app-state.json");
    std::fs::write(
        &path,
        serde_json::to_vec(&json!({
            "projects": [],
            "presets": [],
            "active_tabs": {},
            "pinned_sessions": {}
        }))
        .unwrap(),
    )
    .unwrap();
    let session_dir = home.path().join("app-sessions").join("worker-untracked");
    std::fs::create_dir_all(&session_dir).unwrap();
    std::fs::write(
        session_dir.join("manifest.json"),
        serde_json::to_vec(&json!({
            "session": {
                "id": "worker-untracked",
                "project_id": "project-1",
                "label": "Worker Untracked",
                "command": "claude",
                "created_at": 1000
            },
            "cwd": "/tmp",
            "state": "running",
            "pid": 12345,
            "exit_code": null,
            "has_been_written_to": true,
            "updated_at": 1000
        }))
        .unwrap(),
    )
    .unwrap();
    let _guard = UnpeelHomeGuard::set(home.path());

    let started = std::time::Instant::now();
    let response = controller_mcp_handle_request(json!({
        "jsonrpc": "2.0",
        "id": 101,
        "method": "tools/call",
        "params": {
            "name": "workers",
            "arguments": {
                "action": "wait_for_status",
                "session_id": "worker-untracked",
                "status": "completed",
                "timeout_seconds": 60
            }
        }
    }))
    .expect("tools/call responds");

    let elapsed = started.elapsed();
    assert!(
        elapsed < std::time::Duration::from_millis(500),
        "untracked worker completed wait must return immediately without waiting, took {elapsed:?}"
    );
    assert_eq!(response["result"]["isError"], true);
    let error_text = response["result"]["content"][0]["text"]
        .as_str()
        .expect("error text in content");
    assert!(
        error_text.contains("Completion is not tracked for worker 'worker-untracked'")
            || error_text.contains("not tracked"),
        "error must state completion is not tracked: {error_text}"
    );
}

#[test]
fn wait_for_status_schema_and_help_document_completed_and_lifecycle_targets() {
    let tools = controller_mcp_handle_request(json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/list",
        "params": {}
    }))
    .expect("tools/list responds");

    let tool = &tools["result"]["tools"][0];
    let tool_desc = tool["description"].as_str().expect("tool description");
    let status_desc = tool["inputSchema"]["properties"]["status"]["description"]
        .as_str()
        .expect("status description");

    let help_response = controller_mcp_handle_request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": {
            "name": "workers",
            "arguments": { "action": "help" }
        }
    }))
    .expect("help action responds");
    let help_struct = &help_response["result"]["structuredContent"];
    let help_text = serde_json::to_string(help_struct).expect("help json text");

    for (target_name, text) in [
        ("tool description", tool_desc),
        ("status description", status_desc),
        ("action=help", &help_text),
    ] {
        assert!(
            text.contains("completed"),
            "{target_name} must name 'completed': {text}"
        );
        assert!(
            text.contains("idle") && text.contains("exited"),
            "{target_name} must mention idle and exited: {text}"
        );
        assert!(
            text.contains("finished") || text.contains("finish"),
            "{target_name} must document finished/finish target: {text}"
        );
        assert!(
            text.contains("subagent") || text.contains("dead process") || text.contains("pause"),
            "{target_name} must explain idle/exited distinction: {text}"
        );
    }
}

#[test]
fn wait_for_status_exited_returns_when_live_worker_episode_completes() {
    use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
    use std::time::{Duration, Instant};

    let dir = TempDir::new().unwrap();
    let path = dir.path().join("app-state.json");
    std::fs::write(
        &path,
        serde_json::to_vec(&json!({
            "projects": [],
            "presets": [],
            "active_tabs": {},
            "pinned_sessions": {}
        }))
        .unwrap(),
    )
    .unwrap();
    register_worker_parent_at(&path, "worker-1", "parent-chat-1", 900).unwrap();
    begin_worker_parent_task_at(&path, "worker-1", 950).unwrap();
    let sessions_root = dir.path().join("sessions");

    let cancel = AtomicBool::new(false);
    let polls = AtomicU32::new(0);
    let started = Instant::now();
    let result = zeron_workers_unpeel::controller_mcp_wait_until_matching(
        30,
        "exited",
        &cancel,
        || {
            let n = polls.fetch_add(1, Ordering::SeqCst);
            if n > 0 {
                write_stop_hook(&sessions_root, "worker-1", 1);
            }
            Ok(worker_with_state("running"))
        },
        |session| {
            current_episode_completed_with_evidence_at(
                &path,
                session,
                &sessions_root,
                WorkerCompletionEvidence::quiescent(),
            )
            .unwrap_or(false)
        },
    )
    .expect("wait on exited must return when episode completes");

    assert_eq!(result["matched"], false, "must not claim exited matched");
    assert_eq!(
        result["completed"], true,
        "must mark that the episode completed"
    );
    assert!(
        started.elapsed() < Duration::from_secs(2),
        "must return within one poll tick of completion becoming observable"
    );
}

#[test]
fn wait_for_status_prior_completion_does_not_end_new_lifecycle_wait() {
    use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
    use std::time::{Duration, Instant};

    let dir = TempDir::new().unwrap();
    let path = dir.path().join("app-state.json");
    std::fs::write(
        &path,
        serde_json::to_vec(&json!({
            "projects": [],
            "presets": [],
            "active_tabs": {},
            "pinned_sessions": {}
        }))
        .unwrap(),
    )
    .unwrap();
    register_worker_parent_at(&path, "worker-1", "parent-chat-1", 900).unwrap();
    begin_worker_parent_task_at(&path, "worker-1", 950).unwrap();
    let sessions_root = dir.path().join("sessions");
    write_stop_hook(&sessions_root, "worker-1", 1);

    let cancel = AtomicBool::new(false);
    let polls = AtomicU32::new(0);
    let started = Instant::now();
    let result = zeron_workers_unpeel::controller_mcp_wait_until_matching(
        1,
        "working",
        &cancel,
        || {
            polls.fetch_add(1, Ordering::SeqCst);
            Ok(worker_with_state("running"))
        },
        |session| {
            current_episode_completed_with_evidence_at(
                &path,
                session,
                &sessions_root,
                WorkerCompletionEvidence::quiescent(),
            )
            .unwrap_or(false)
        },
    )
    .expect("wait should time out");

    assert_eq!(result["matched"], false);
    assert_eq!(
        result["timed_out"], true,
        "prior completion must not end a new wait on working; it must time out"
    );
    assert!(
        started.elapsed() >= Duration::from_millis(900),
        "must wait for the timeout instead of returning early"
    );
}
