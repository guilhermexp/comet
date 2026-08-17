use std::ffi::OsString;
use std::fs;
use std::process::Command;
use std::sync::Mutex;

use tempfile::TempDir;
use zeron_workers_unpeel::{
    LocalWorkersClient, WorkersCreateGroupRequest, WorkersCreateWorktreeRequest,
    WorkersProjectOrganizationPatch, WorkersSessionSort,
};

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
        serde_json::to_vec_pretty(&serde_json::json!({
            "projects": [{
                "id": "root",
                "name": "Root",
                "path": "/tmp/root",
                "workspace_id": "personal",
                "sort_order": 0
            }],
            "presets": [],
            "active_tabs": {},
            "pinned_sessions": {},
            "unknown_future_field": { "keep": true }
        }))?,
    )?;
    let guard = UnpeelHomeGuard::set(home.path());
    Ok((home, guard))
}

#[test]
fn project_contract_covers_unpeels_local_organization_dtos() {
    let worktree = WorkersCreateWorktreeRequest {
        project_id: "root".into(),
        branch: "feature/sidebar".into(),
        name: Some("Sidebar".into()),
        base_ref: Some("main".into()),
    };
    assert_eq!(worktree.project_id, "root");
    assert_eq!(worktree.branch, "feature/sidebar");

    let group = WorkersCreateGroupRequest {
        parent_project_id: "root".into(),
        name: "Research".into(),
    };
    assert_eq!(group.name, "Research");

    assert_ne!(
        WorkersSessionSort::Custom,
        WorkersSessionSort::RecentlyUpdated
    );
    let patch = WorkersProjectOrganizationPatch {
        display_name: Some("Renamed".into()),
        folder_color_id: Some(Some("sky".into())),
        session_sort: Some(WorkersSessionSort::RecentlyUpdated),
        sort_order: Some(1),
    };
    assert_eq!(patch.sort_order, Some(1));
}

#[test]
fn group_and_sort_mutations_preserve_unknown_app_state() -> Result<(), Box<dyn std::error::Error>> {
    let _lock = ENV_LOCK.lock().expect("UNPEEL_HOME test lock");
    let (home, _guard) = isolated_home()?;
    let client = LocalWorkersClient::new();

    let group_id = client.create_group(WorkersCreateGroupRequest {
        parent_project_id: "root".into(),
        name: "Research".into(),
    })?;
    client.set_project_organization(
        &group_id,
        WorkersProjectOrganizationPatch {
            display_name: Some("Investigations".into()),
            session_sort: Some(WorkersSessionSort::RecentlyUpdated),
            ..Default::default()
        },
    )?;

    let state: serde_json::Value =
        serde_json::from_slice(&fs::read(home.path().join("app-state.json"))?)?;
    assert_eq!(state["unknown_future_field"]["keep"], true);
    assert_eq!(state["session_sort_modes"][&group_id], "date");
    let group = state["projects"]
        .as_array()
        .and_then(|projects| projects.iter().find(|project| project["id"] == group_id))
        .expect("new group record");
    assert_eq!(group["name"], "Investigations");
    assert_eq!(group["parent_project_id"], "root");
    assert_eq!(group["is_folder"], true);
    Ok(())
}

#[test]
fn invalid_worktree_request_fails_before_registering_a_child_project()
-> Result<(), Box<dyn std::error::Error>> {
    let _lock = ENV_LOCK.lock().expect("UNPEEL_HOME test lock");
    let (home, _guard) = isolated_home()?;
    let error = LocalWorkersClient::new()
        .create_worktree(WorkersCreateWorktreeRequest {
            project_id: "root".into(),
            branch: "feature/sidebar".into(),
            name: None,
            base_ref: None,
        })
        .expect_err("the fixture path is not a git repository");
    assert!(!error.to_string().is_empty());

    let state: serde_json::Value =
        serde_json::from_slice(&fs::read(home.path().join("app-state.json"))?)?;
    assert_eq!(state["projects"].as_array().map(Vec::len), Some(1));
    Ok(())
}

#[test]
fn worktree_lifecycle_registers_and_removes_the_child_project()
-> Result<(), Box<dyn std::error::Error>> {
    let _lock = ENV_LOCK.lock().expect("UNPEEL_HOME test lock");
    let (home, _guard) = isolated_home()?;
    let repo = home.path().join("repo");
    fs::create_dir_all(&repo)?;
    for args in [
        vec!["init", "-b", "main"],
        vec!["config", "user.email", "workers@example.test"],
        vec!["config", "user.name", "Workers Tests"],
    ] {
        assert!(
            Command::new("git")
                .args(args)
                .current_dir(&repo)
                .status()?
                .success()
        );
    }
    fs::write(repo.join("README.md"), "fixture\n")?;
    assert!(
        Command::new("git")
            .args(["add", "README.md"])
            .current_dir(&repo)
            .status()?
            .success()
    );
    assert!(
        Command::new("git")
            .args(["commit", "-m", "fixture"])
            .current_dir(&repo)
            .status()?
            .success()
    );

    let mut state: serde_json::Value =
        serde_json::from_slice(&fs::read(home.path().join("app-state.json"))?)?;
    state["projects"][0]["path"] = serde_json::json!(repo);
    fs::write(
        home.path().join("app-state.json"),
        serde_json::to_vec_pretty(&state)?,
    )?;

    let client = LocalWorkersClient::new();
    let worktree = client.create_worktree(WorkersCreateWorktreeRequest {
        project_id: "root".into(),
        branch: "feature/sidebar".into(),
        name: Some("Sidebar".into()),
        base_ref: Some("main".into()),
    })?;
    assert!(std::path::Path::new(&worktree.path).exists());
    let snapshot = client.bootstrap()?;
    let child = snapshot
        .projects
        .iter()
        .find(|project| project.id == worktree.project_id)
        .expect("worktree child project");
    assert_eq!(child.parent_project_id.as_deref(), Some("root"));
    assert_eq!(child.worktree_branch.as_deref(), Some("feature/sidebar"));

    client.remove_worktree(&worktree.project_id, true)?;
    assert!(!std::path::Path::new(&worktree.path).exists());
    assert!(
        client
            .bootstrap()?
            .projects
            .iter()
            .all(|project| project.id != worktree.project_id)
    );
    Ok(())
}

#[test]
fn removing_an_empty_group_preserves_the_parent_and_unknown_state()
-> Result<(), Box<dyn std::error::Error>> {
    let _lock = ENV_LOCK.lock().expect("UNPEEL_HOME test lock");
    let (home, _guard) = isolated_home()?;
    let client = LocalWorkersClient::new();
    let group_id = client.create_group(WorkersCreateGroupRequest {
        parent_project_id: "root".into(),
        name: "Temporary".into(),
    })?;

    client.remove_group(&group_id)?;

    let state: serde_json::Value =
        serde_json::from_slice(&fs::read(home.path().join("app-state.json"))?)?;
    assert_eq!(state["unknown_future_field"]["keep"], true);
    assert!(
        state["projects"]
            .as_array()
            .is_some_and(|projects| projects.iter().any(|project| project["id"] == "root"))
    );
    assert!(
        state["projects"]
            .as_array()
            .is_some_and(|projects| projects.iter().all(|project| project["id"] != group_id))
    );
    Ok(())
}
