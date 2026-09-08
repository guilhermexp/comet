use std::ffi::OsString;
use std::fs;
use std::process::Command;
use std::sync::Mutex;

use tempfile::TempDir;
use zeron_workers_unpeel::{
    LocalWorkersClient, WorkersCreateGroupRequest, WorkersCreateWorktreeRequest,
    WorkersLaunchRequest, WorkersProjectOrganizationPatch, WorkersSessionSort,
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

/// A one-commit repository on `main` — the least a `git worktree add` needs.
fn fixture_repo(repo: &std::path::Path) -> Result<(), Box<dyn std::error::Error>> {
    fs::create_dir_all(repo)?;
    fs::write(repo.join("README.md"), "fixture\n")?;
    for args in [
        vec!["init", "-b", "main"],
        vec!["config", "user.email", "workers@example.test"],
        vec!["config", "user.name", "Workers Tests"],
        vec!["add", "README.md"],
        vec!["commit", "-m", "fixture"],
    ] {
        assert!(
            Command::new("git")
                .args(args)
                .current_dir(repo)
                .status()?
                .success()
        );
    }
    Ok(())
}

#[test]
fn worktree_registered_as_a_plain_project_is_projected_as_a_worktree()
-> Result<(), Box<dyn std::error::Error>> {
    let _lock = ENV_LOCK.lock().expect("UNPEEL_HOME test lock");
    let (home, _guard) = isolated_home()?;
    let repo = home.path().join("repo");
    fixture_repo(&repo)?;
    let checkout = home.path().join("repo-wt-sidebar");
    assert!(
        Command::new("git")
            .args(["worktree", "add", "-b", "feature/sidebar"])
            .arg(&checkout)
            .current_dir(&repo)
            .status()?
            .success()
    );

    // Registered the way "Add project…" registers any folder: a root project
    // with no parent and no worktree branch. This is what `git worktree add`
    // in a terminal leaves behind, and it is the majority of the real
    // registry — the registry cannot know, only the checkout can.
    let mut state: serde_json::Value =
        serde_json::from_slice(&fs::read(home.path().join("app-state.json"))?)?;
    state["projects"][0]["path"] = serde_json::json!(repo);
    state["projects"]
        .as_array_mut()
        .expect("projects array")
        .push(serde_json::json!({
            "id": "checkout",
            "name": "repo-wt-sidebar",
            "path": checkout,
            "sort_order": 1,
        }));
    fs::write(
        home.path().join("app-state.json"),
        serde_json::to_vec_pretty(&state)?,
    )?;

    let project = LocalWorkersClient::new()
        .bootstrap()?
        .projects
        .iter()
        .find(|project| project.id == "checkout")
        .cloned()
        .expect("the registered checkout");
    assert_eq!(project.worktree_branch.as_deref(), Some("feature/sidebar"));
    assert_eq!(project.parent_project_id.as_deref(), Some("root"));
    assert!(!project.is_group);
    Ok(())
}

#[test]
fn worktree_lifecycle_registers_and_removes_the_child_project()
-> Result<(), Box<dyn std::error::Error>> {
    let _lock = ENV_LOCK.lock().expect("UNPEEL_HOME test lock");
    let (home, _guard) = isolated_home()?;
    let repo = home.path().join("repo");
    fixture_repo(&repo)?;

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
    // A worktree nests like a folder but owns a checkout, so the UI must not
    // read it as organization: `is_group` gates selection, the launcher, the
    // hover controls and ledger membership, and the projection used to set it
    // from `is_folder` + parent alone.
    assert!(!child.is_group, "a worktree must not project as a group");

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

    let group = client
        .bootstrap()?
        .projects
        .iter()
        .find(|project| project.id == group_id)
        .cloned()
        .expect("group project");
    assert!(group.is_group, "a folder without a branch is still a group");

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

/// A repository plus a worktree of it, both registered the way "Add project…"
/// registers any folder: root records with no parent and no worktree branch.
/// This is what `git worktree add` in a terminal leaves behind, so only the
/// checkout — never the registry — can say the child is a worktree.
fn adopted_worktree(
    home: &std::path::Path,
    branch: &str,
) -> Result<(std::path::PathBuf, std::path::PathBuf), Box<dyn std::error::Error>> {
    let repo = home.join("repo");
    fixture_repo(&repo)?;
    let checkout = home.join("repo-wt");
    assert!(
        Command::new("git")
            .args(["worktree", "add", "-b", branch])
            .arg(&checkout)
            .current_dir(&repo)
            .status()?
            .success()
    );
    let mut state = read_state(home)?;
    state["projects"][0]["path"] = serde_json::json!(repo);
    state["projects"]
        .as_array_mut()
        .expect("projects array")
        .push(serde_json::json!({
            "id": "checkout",
            "name": "repo-wt",
            "path": checkout,
            "sort_order": 1,
        }));
    write_state(home, &state)?;
    Ok((repo, checkout))
}

fn read_state(home: &std::path::Path) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    Ok(serde_json::from_slice(&fs::read(
        home.join("app-state.json"),
    )?)?)
}

fn write_state(
    home: &std::path::Path,
    state: &serde_json::Value,
) -> Result<(), Box<dyn std::error::Error>> {
    fs::write(
        home.join("app-state.json"),
        serde_json::to_vec_pretty(state)?,
    )?;
    Ok(())
}

/// Points the seeded `root` project at a real repository, so `create_worktree`
/// has something to branch from.
fn root_at_fixture_repo(
    home: &std::path::Path,
) -> Result<std::path::PathBuf, Box<dyn std::error::Error>> {
    let repo = home.join("repo");
    fixture_repo(&repo)?;
    let mut state = read_state(home)?;
    state["projects"][0]["path"] = serde_json::json!(repo);
    write_state(home, &state)?;
    Ok(repo)
}

/// A launch that gets as far as resolving its preset has passed every gate the
/// creation catalog owns. Nothing is spawned: the preset does not exist.
fn launch_error(client: &LocalWorkersClient, launch: WorkersLaunchRequest) -> String {
    client
        .launch_session(&launch)
        .expect_err("the preset does not exist")
        .to_string()
}

#[test]
fn launching_into_a_created_worktree_is_not_rejected_as_a_folder()
-> Result<(), Box<dyn std::error::Error>> {
    let _lock = ENV_LOCK.lock().expect("UNPEEL_HOME test lock");
    let (home, _guard) = isolated_home()?;
    root_at_fixture_repo(home.path())?;

    let client = LocalWorkersClient::new();
    let worktree = client.create_worktree(WorkersCreateWorktreeRequest {
        project_id: "root".into(),
        branch: "feature/sidebar".into(),
        name: Some("Sidebar".into()),
        base_ref: Some("main".into()),
    })?;

    // A created worktree is registered as `is_folder` — it nests — and the
    // creation catalog used to hand that straight to the launch gate, which
    // refuses a folder before it ever looks at a command. The payload also
    // echoes the snapshot the sidebar read, so the second gate (the worktree
    // must belong to the project) has to pass too.
    let message = launch_error(
        &client,
        WorkersLaunchRequest::preset(&worktree.project_id, "no-such-preset")
            .with_worktree(&worktree.path, &worktree.branch),
    );
    assert!(!message.contains("project is a folder"), "{message}");
    assert!(
        !message.contains("worktree does not belong to project"),
        "{message}"
    );
    assert!(message.contains("unknown preset id"), "{message}");
    Ok(())
}

#[test]
fn a_group_inside_a_worktree_stays_a_group() -> Result<(), Box<dyn std::error::Error>> {
    let _lock = ENV_LOCK.lock().expect("UNPEEL_HOME test lock");
    let (home, _guard) = isolated_home()?;
    root_at_fixture_repo(home.path())?;

    let client = LocalWorkersClient::new();
    let worktree = client.create_worktree(WorkersCreateWorktreeRequest {
        project_id: "root".into(),
        branch: "feature/sidebar".into(),
        name: Some("Sidebar".into()),
        base_ref: Some("main".into()),
    })?;
    // A group created on a worktree row inherits the worktree's path, so disk
    // detection would read it as a worktree of its own and name the parent's
    // branch on it.
    let group_id = client.create_group(WorkersCreateGroupRequest {
        parent_project_id: worktree.project_id.clone(),
        name: "Research".into(),
    })?;

    let group = client
        .bootstrap()?
        .projects
        .iter()
        .find(|project| project.id == group_id)
        .cloned()
        .expect("the group project");
    assert!(group.is_group, "a folder with a parent and no branch");
    assert_eq!(
        group.worktree_branch, None,
        "a group names no branch, whatever its path is a checkout of"
    );

    // The damage a branch on this row would have done: `is_group` picks the
    // removal route, and removing a label must never take the parent's
    // checkout down with it.
    client.remove_group(&group_id)?;
    assert!(
        std::path::Path::new(&worktree.path).exists(),
        "removing a label must not delete the parent's checkout"
    );
    Ok(())
}

#[test]
fn a_detached_worktree_names_no_branch_but_keeps_its_place()
-> Result<(), Box<dyn std::error::Error>> {
    let _lock = ENV_LOCK.lock().expect("UNPEEL_HOME test lock");
    let (home, _guard) = isolated_home()?;
    let (_repo, checkout) = adopted_worktree(home.path(), "feature/sidebar")?;
    assert!(
        Command::new("git")
            .args(["checkout", "--detach"])
            .current_dir(&checkout)
            .status()?
            .success()
    );
    let head = String::from_utf8(
        Command::new("git")
            .args(["rev-parse", "HEAD"])
            .current_dir(&checkout)
            .output()?
            .stdout,
    )?;
    let short_sha: String = head.trim().chars().take(7).collect();

    let project = LocalWorkersClient::new()
        .bootstrap()?
        .projects
        .iter()
        .find(|project| project.id == "checkout")
        .cloned()
        .expect("the registered checkout");
    // A short hash is not a branch: nothing can launch on it or sign a pull
    // request with it, so it must not be promoted to a worktree branch.
    assert_eq!(project.worktree_branch, None);
    assert_eq!(project.git_branch.as_deref(), Some(short_sha.as_str()));
    // It is still a checkout under its repository, not organization.
    assert!(!project.is_group);
    assert_eq!(project.parent_project_id.as_deref(), Some("root"));
    Ok(())
}

#[test]
fn a_failed_launch_keeps_the_worktree_it_just_created() -> Result<(), Box<dyn std::error::Error>> {
    let _lock = ENV_LOCK.lock().expect("UNPEEL_HOME test lock");
    let (home, _guard) = isolated_home()?;
    root_at_fixture_repo(home.path())?;

    let client = LocalWorkersClient::new();
    let error = client
        .create_worktree_and_launch(
            WorkersCreateWorktreeRequest {
                project_id: "root".into(),
                branch: "feature/sidebar".into(),
                name: Some("Sidebar".into()),
                base_ref: Some("main".into()),
            },
            WorkersLaunchRequest::preset("root", "no-such-preset"),
        )
        .expect_err("the preset does not exist");
    assert!(error.to_string().contains("unknown preset id"));

    // The checkout already exists and is already registered; deleting it over
    // a bad preset would lose whatever the setup step put there.
    let child = client
        .bootstrap()?
        .projects
        .iter()
        .find(|project| project.worktree_branch.as_deref() == Some("feature/sidebar"))
        .cloned()
        .expect("the worktree stays registered");
    assert!(std::path::Path::new(&child.path).exists());
    Ok(())
}

#[test]
fn renaming_an_adopted_worktree_writes_the_project_record() -> Result<(), Box<dyn std::error::Error>>
{
    let _lock = ENV_LOCK.lock().expect("UNPEEL_HOME test lock");
    let (home, _guard) = isolated_home()?;
    adopted_worktree(home.path(), "feature/sidebar")?;

    let client = LocalWorkersClient::new();
    // An adopted worktree names no branch in the registry, so a route that
    // asks "is this a worktree?" sends it to the group rename, which refuses
    // it with "only plain groups can be renamed here".
    client.set_project_organization(
        "checkout",
        WorkersProjectOrganizationPatch {
            display_name: Some("Sidebar".into()),
            ..Default::default()
        },
    )?;
    let group_id = client.create_group(WorkersCreateGroupRequest {
        parent_project_id: "root".into(),
        name: "Research".into(),
    })?;
    client.set_project_organization(
        &group_id,
        WorkersProjectOrganizationPatch {
            display_name: Some("Investigations".into()),
            ..Default::default()
        },
    )?;

    let state = read_state(home.path())?;
    let name_of = |id: &str| -> Option<String> {
        state["projects"]
            .as_array()?
            .iter()
            .find(|project| project["id"] == id)?["name"]
            .as_str()
            .map(str::to_owned)
    };
    assert_eq!(name_of("checkout").as_deref(), Some("Sidebar"));
    assert_eq!(name_of(&group_id).as_deref(), Some("Investigations"));
    Ok(())
}

#[test]
fn a_child_that_is_not_a_group_removes_as_a_project() -> Result<(), Box<dyn std::error::Error>> {
    let _lock = ENV_LOCK.lock().expect("UNPEEL_HOME test lock");
    let (home, _guard) = isolated_home()?;
    let child_path = home.path().join("nested");
    fs::create_dir_all(&child_path)?;
    let mut state = read_state(home.path())?;
    // A nested project record that is not organization: it has a parent but no
    // `is_folder`, and its path is no checkout, so it names no branch either.
    state["projects"]
        .as_array_mut()
        .expect("projects array")
        .push(serde_json::json!({
            "id": "nested",
            "name": "Nested",
            "path": child_path,
            "parent_project_id": "root",
            "sort_order": 1,
        }));
    write_state(home.path(), &state)?;

    let client = LocalWorkersClient::new();
    let child = client
        .bootstrap()?
        .projects
        .iter()
        .find(|project| project.id == "nested")
        .cloned()
        .expect("the nested project");
    // `is_group` — not the presence of a parent — picks the removal route.
    assert!(!child.is_group);
    assert!(child.worktree_branch.is_none());
    let refused = client
        .remove_group("nested")
        .expect_err("a project is not a group");
    assert!(refused.to_string().contains("project is not a group"));

    client.remove_project("nested")?;
    assert!(
        client
            .bootstrap()?
            .projects
            .iter()
            .all(|project| project.id != "nested")
    );
    Ok(())
}
