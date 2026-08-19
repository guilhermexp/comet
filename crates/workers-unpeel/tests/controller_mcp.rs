use serde_json::json;
use std::ffi::OsString;
use std::fs;
use std::sync::Mutex;
use tempfile::TempDir;
use zeron_workers_unpeel::{
    controller_mcp_clean_output, controller_mcp_encode_keys, controller_mcp_handle_request,
    controller_mcp_parse_launch, ensure_controller_mcp_host_launcher, is_session_host_mode,
};

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
