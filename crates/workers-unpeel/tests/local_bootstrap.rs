use std::ffi::OsString;
use std::fs;
use std::sync::Mutex;

use tempfile::TempDir;
use zeron_workers_unpeel::{LocalWorkersClient, WorkersError};

static ENV_LOCK: Mutex<()> = Mutex::new(());

struct UnpeelHomeGuard {
    previous: Option<OsString>,
}

impl UnpeelHomeGuard {
    fn set(path: &std::path::Path) -> Self {
        let previous = std::env::var_os("UNPEEL_HOME");
        // SAFETY: all tests in this crate serialize UNPEEL_HOME changes with ENV_LOCK.
        unsafe { std::env::set_var("UNPEEL_HOME", path) };
        Self { previous }
    }
}

impl Drop for UnpeelHomeGuard {
    fn drop(&mut self) {
        // SAFETY: the guard is dropped while the caller still holds ENV_LOCK.
        unsafe {
            match self.previous.take() {
                Some(value) => std::env::set_var("UNPEEL_HOME", value),
                None => std::env::remove_var("UNPEEL_HOME"),
            }
        }
    }
}

#[test]
fn bootstrap_reads_projects_from_the_canonical_unpeel_home()
-> Result<(), Box<dyn std::error::Error>> {
    let _lock = ENV_LOCK.lock().expect("UNPEEL_HOME test lock");
    let home = TempDir::new()?;
    fs::write(
        home.path().join("app-state.json"),
        serde_json::to_vec_pretty(&serde_json::json!({
            "projects": [{
                "id": "project-1",
                "name": "Workers Project",
                "path": "/tmp/workers-project",
                "sort_order": 0,
                "is_folder": false
            }],
            "active_project_id": "project-1",
            "presets": [{
                "id": "preset-codex",
                "label": "Codex",
                "command": "codex",
                "enabled": true,
                "quick_launch": true
            }],
            "active_tabs": {},
            "pinned_sessions": {}
        }))?,
    )?;
    let _home = UnpeelHomeGuard::set(home.path());

    let snapshot = LocalWorkersClient::new().bootstrap()?;

    assert_eq!(snapshot.protocol.major_version, 1);
    assert!(snapshot.protocol.supports("host.bootstrap"));
    assert_eq!(snapshot.projects.len(), 1);
    assert_eq!(snapshot.projects[0].id, "project-1");
    assert_eq!(snapshot.projects[0].name, "Workers Project");
    assert_eq!(snapshot.projects[0].path, "/tmp/workers-project");
    assert_eq!(snapshot.projects[0].archived_session_count, 0);
    assert_eq!(snapshot.presets.len(), 1);
    assert_eq!(snapshot.presets[0].id, "preset-codex");
    assert_eq!(snapshot.presets[0].command, "codex");
    assert!(snapshot.presets[0].quick_launch);
    assert!(snapshot.sessions.is_empty());
    Ok(())
}

#[test]
fn bootstrap_reports_malformed_state_without_overwriting_it()
-> Result<(), Box<dyn std::error::Error>> {
    let _lock = ENV_LOCK.lock().expect("UNPEEL_HOME test lock");
    let home = TempDir::new()?;
    let state_path = home.path().join("app-state.json");
    let malformed = b"{ this is not valid json";
    fs::write(&state_path, malformed)?;
    let _home = UnpeelHomeGuard::set(home.path());

    let error = LocalWorkersClient::new()
        .bootstrap()
        .expect_err("malformed state must fail closed");

    assert!(matches!(error, WorkersError::Upstream { status: 500, .. }));
    assert_eq!(fs::read(&state_path)?, malformed);
    Ok(())
}
