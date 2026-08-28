use serde_json::json;
use std::ffi::OsString;
use std::fs;
use std::sync::Mutex;
use tempfile::TempDir;
use zeron_workers_unpeel::{
    WorkersSession, WorkersSessionCapabilities, controller_mcp_archive_guard,
    controller_mcp_briefing_stability_key, controller_mcp_choose_semantic_output,
    controller_mcp_clean_output, controller_mcp_consume_authority_marker,
    controller_mcp_encode_keys, controller_mcp_handle_request, controller_mcp_is_booting_screen,
    controller_mcp_is_briefing_screen_ready, controller_mcp_parse_launch,
    controller_mcp_parse_launch_briefing, controller_mcp_sanitize_text,
    controller_mcp_startup_prompt_response, controller_mcp_take_parent_chat_id,
    controller_mcp_tracks_task_episode, ensure_controller_mcp_host_launcher, is_session_host_mode,
    register_worker_parent_at, worker_parent_links_at,
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
fn launch_requires_exactly_one_launch_mode() {
    assert!(controller_mcp_parse_launch(json!({ "project_id": "p" })).is_err());
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

struct UnpeelHomeGuard(Option<OsString>);

impl UnpeelHomeGuard {
    fn set(path: &std::path::Path) -> Self {
        let previous = std::env::var_os("UNPEEL_HOME");
        // SAFETY: every mutation in this test binary holds ENV_LOCK.
        unsafe { std::env::set_var("UNPEEL_HOME", path) };
        Self(previous)
    }
}

impl Drop for UnpeelHomeGuard {
    fn drop(&mut self) {
        // SAFETY: the guard is dropped while the test still holds ENV_LOCK.
        unsafe {
            match self.0.take() {
                Some(previous) => std::env::set_var("UNPEEL_HOME", previous),
                None => std::env::remove_var("UNPEEL_HOME"),
            }
        }
    }
}

#[test]
fn tools_call_lists_real_controller_projects() -> Result<(), Box<dyn std::error::Error>> {
    let _lock = ENV_LOCK.lock().expect("UNPEEL_HOME test lock");
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

/// `launch_worker` only accepts a project_id that is already in the list, so a
/// checkout nobody registered is unlaunchable. Without this action the caller's
/// only working move was an ancestor project — which is how two workers ended
/// up running in $HOME instead of the repo they were briefed about.
#[test]
fn add_project_registers_an_unlisted_checkout_and_is_idempotent()
-> Result<(), Box<dyn std::error::Error>> {
    let _lock = ENV_LOCK.lock().expect("UNPEEL_HOME test lock");
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
    let _lock = ENV_LOCK.lock().expect("UNPEEL_HOME test lock");
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
    let _lock = ENV_LOCK.lock().expect("environment test lock");
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
    let _lock = ENV_LOCK.lock().expect("environment test lock");
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
        capabilities: WorkersSessionCapabilities::default(),
    }
}
