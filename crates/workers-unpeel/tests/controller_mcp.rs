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

#[test]
fn tools_call_lists_real_controller_projects() -> Result<(), Box<dyn std::error::Error>> {
    let _lock = ENV_LOCK.lock();
    let home = TempDir::new()?;
    fs::write(
        home.path().join("app-state.json"),
        serde_json::to_vec(&json!({
            "projects": [{
                "id": "project-1",
                "name": "Project One",
                "path": "/tmp/project-one",
                "sort_order": 0,
                "is_folder": false
            }],
            "presets": [],
            "active_tabs": {},
            "pinned_sessions": {}
        }))?,
    )?;
    let _guard = UnpeelHomeGuard::set(home.path());

    let response = controller_mcp_handle_request(json!({
        "jsonrpc": "2.0",
        "id": 9,
        "method": "tools/call",
        "params": {
            "name": "workers",
            "arguments": { "action": "list_projects" }
        }
    }))
    .expect("tools/call responds");

    assert_eq!(response["result"]["isError"], false);
    assert_eq!(
        response["result"]["structuredContent"]["projects"][0]["id"],
        "project-1"
    );
    Ok(())
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

/// `launch_worker` only accepts a project_id that is already in the list, so a
/// checkout nobody registered is unlaunchable. Without this action the caller's
/// only working move was an ancestor project — which is how two workers ended
/// up running in $HOME instead of the repo they were briefed about.
#[test]
fn add_project_registers_an_unlisted_checkout_and_is_idempotent()
-> Result<(), Box<dyn std::error::Error>> {
    let _lock = ENV_LOCK.lock();
    let home = TempDir::new()?;
    fs::write(
        home.path().join("app-state.json"),
        serde_json::to_vec(&json!({
            "projects": [{
                "id": "ancestor",
                "name": "Home",
                "path": "/tmp",
                "sort_order": 0,
                "is_folder": false
            }],
            "presets": [],
            "active_tabs": {},
            "pinned_sessions": {}
        }))?,
    )?;
    let _guard = UnpeelHomeGuard::set(home.path());

    let checkout = TempDir::new()?;
    let call = |arguments: serde_json::Value| {
        controller_mcp_handle_request(json!({
            "jsonrpc": "2.0",
            "id": 11,
            "method": "tools/call",
            "params": { "name": "workers", "arguments": arguments }
        }))
        .expect("tools/call responds")
    };

    // Advertised, not just dispatchable: dispatch matches the raw string, so an
    // action missing from the enum is invisible to the caller that reads the
    // schema — which is every caller.
    let tools = controller_mcp_handle_request(json!({
        "jsonrpc": "2.0", "id": 10, "method": "tools/list", "params": {}
    }))
    .expect("tools/list responds");
    assert!(
        tools["result"]["tools"][0]["inputSchema"]["properties"]["action"]["enum"]
            .as_array()
            .expect("action enum")
            .iter()
            .any(|action| action == "add_project")
    );

    let listed = call(json!({ "action": "list_projects" }));
    assert_eq!(
        listed["result"]["structuredContent"]["projects"]
            .as_array()
            .map(Vec::len),
        Some(1)
    );

    let added = call(json!({
        "action": "add_project",
        "path": checkout.path().to_string_lossy()
    }));
    assert_eq!(added["result"]["isError"], false);
    let id = added["result"]["structuredContent"]["project_id"]
        .as_str()
        .expect("add_project returns the id launch_worker needs")
        .to_owned();
    // The echoed path is the canonical one the worker will run in — on macOS a
    // temp dir resolves through /private, and that gap is the whole bug class.
    assert_eq!(
        added["result"]["structuredContent"]["path"].as_str(),
        Some(
            std::fs::canonicalize(checkout.path())?
                .to_string_lossy()
                .as_ref()
        )
    );

    let listed = call(json!({ "action": "list_projects" }));
    let projects = listed["result"]["structuredContent"]["projects"]
        .as_array()
        .expect("projects");
    assert_eq!(projects.len(), 2);
    assert!(projects.iter().any(|project| project["id"] == id.as_str()));

    let again = call(json!({
        "action": "add_project",
        "path": checkout.path().to_string_lossy()
    }));
    assert_eq!(
        again["result"]["structuredContent"]["project_id"].as_str(),
        Some(id.as_str()),
        "re-registering the same checkout must reuse its id, not fork a duplicate"
    );

    let rejected = call(json!({ "action": "add_project" }));
    assert_eq!(rejected["result"]["isError"], true);
    Ok(())
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
