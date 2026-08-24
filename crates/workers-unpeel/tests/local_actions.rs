use std::ffi::OsString;
use std::fs;
use std::sync::Mutex;

use tempfile::TempDir;
use zeron_workers_unpeel::{
    InitialTextSubmitMode, LocalWorkersClient, WorkersError, WorkersLaunchRequest,
    is_session_host_mode, session_host_launch_args, session_host_launcher_path,
};

#[test]
fn process_mode_only_claims_unpeel_session_hosts() {
    assert!(is_session_host_mode(&[
        "__session_host__".to_owned(),
        "/tmp/launch.json".to_owned(),
    ]));
    assert!(!is_session_host_mode(&["headless".to_owned()]));
    assert!(!is_session_host_mode(&[]));
    assert_eq!(
        session_host_launch_args(&["__session_host__".to_owned(), "/tmp/launch.json".to_owned(),]),
        Some(&["/tmp/launch.json".to_owned()][..])
    );
    assert_eq!(
        session_host_launcher_path(&["/tmp/launch.json".to_owned()]),
        Some(std::path::Path::new("/tmp/launch.json"))
    );
    assert_eq!(session_host_launcher_path(&["headless".to_owned()]), None);
    assert_eq!(
        session_host_launcher_path(&["/tmp/launch.json".to_owned(), "unexpected".to_owned(),]),
        None
    );
}

static ENV_LOCK: Mutex<()> = Mutex::new(());

struct UnpeelHomeGuard(Option<OsString>);

impl UnpeelHomeGuard {
    fn set(path: &std::path::Path) -> Self {
        let previous = std::env::var_os("UNPEEL_HOME");
        // SAFETY: every environment mutation in this test binary holds ENV_LOCK.
        unsafe { std::env::set_var("UNPEEL_HOME", path) };
        Self(previous)
    }
}

impl Drop for UnpeelHomeGuard {
    fn drop(&mut self) {
        // SAFETY: the caller still holds ENV_LOCK when this guard is dropped.
        unsafe {
            match self.0.take() {
                Some(previous) => std::env::set_var("UNPEEL_HOME", previous),
                None => std::env::remove_var("UNPEEL_HOME"),
            }
        }
    }
}

fn isolated_home() -> Result<(TempDir, UnpeelHomeGuard), Box<dyn std::error::Error>> {
    let home = TempDir::new()?;
    fs::write(
        home.path().join("app-state.json"),
        serde_json::to_vec(&serde_json::json!({
            "projects": [],
            "active_project_id": null,
            "presets": [],
            "active_tabs": {},
            "pinned_sessions": {}
        }))?,
    )?;
    let guard = UnpeelHomeGuard::set(home.path());
    Ok((home, guard))
}

#[test]
fn output_rejects_unsafe_session_ids_before_reading_disk() -> Result<(), Box<dyn std::error::Error>>
{
    let _lock = ENV_LOCK.lock().expect("UNPEEL_HOME test lock");
    let (_home, _guard) = isolated_home()?;

    let error = LocalWorkersClient::new()
        .read_output("../escape", None, 0)
        .expect_err("unsafe session id must be rejected");

    assert!(matches!(error, WorkersError::Upstream { status: 400, .. }));
    Ok(())
}

#[test]
fn create_session_rejects_unknown_projects() -> Result<(), Box<dyn std::error::Error>> {
    let _lock = ENV_LOCK.lock().expect("UNPEEL_HOME test lock");
    let (_home, _guard) = isolated_home()?;

    let error = LocalWorkersClient::new()
        .create_session("missing-project", "zsh")
        .expect_err("unknown project must be rejected");

    assert!(matches!(error, WorkersError::Upstream { status: 400, .. }));
    Ok(())
}

#[test]
fn launch_request_serializes_every_host_mode_exactly() {
    let terminal = WorkersLaunchRequest::terminal("project-1");
    assert_eq!(
        terminal.wire_body(),
        serde_json::json!({ "projectID": "project-1", "command": "" })
    );

    let preset = WorkersLaunchRequest::preset("project-1", "preset-1");
    assert_eq!(
        preset.wire_body(),
        serde_json::json!({ "projectID": "project-1", "presetID": "preset-1" })
    );

    for (mode, wire) in [
        (InitialTextSubmitMode::PasteOnly, "pasteOnly"),
        (InitialTextSubmitMode::PasteAndSubmit, "pasteAndSubmit"),
        (InitialTextSubmitMode::Raw, "raw"),
    ] {
        let request = WorkersLaunchRequest::command("worktree-1", " codex --full-auto ")
            .with_worktree("/tmp/worktree", "feature/parity")
            .with_initial_text("implement this", mode);
        assert_eq!(
            request.wire_body(),
            serde_json::json!({
                "projectID": "worktree-1",
                "command": " codex --full-auto ",
                "worktreePath": "/tmp/worktree",
                "worktreeBranch": "feature/parity",
                "initialText": "implement this",
                "initialTextSubmitMode": wire,
            })
        );
    }
}
