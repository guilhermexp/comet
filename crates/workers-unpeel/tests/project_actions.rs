use std::ffi::OsString;
use std::fs;
use std::process::Command;
use std::sync::{Arc, Barrier, Mutex};

use tempfile::TempDir;
use zeron_workers_unpeel::{
    LocalWorkersClient, WorkersCreateGroupRequest, WorkersCreateWorktreeRequest,
    WorkersLaunchRequest, WorkersProjectOrganizationPatch, WorkersSessionSort, reserve_chat_run,
};

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
        // SAFETY: every environment mutation in this test binary holds ENV_LOCK.
        for (key, value) in values {
            unsafe { std::env::set_var(key, value) };
        }
        Self(previous)
    }
}

impl Drop for UnpeelHomeGuard {
    fn drop(&mut self) {
        // SAFETY: the caller still holds ENV_LOCK when this guard is dropped.
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

fn git_ref_exists(repo: &std::path::Path, reference: &str) -> Result<bool, std::io::Error> {
    let status = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(["show-ref", "--verify", "--quiet", reference])
        .status()?;
    Ok(status.success())
}

fn commit_all(repo: &std::path::Path, message: &str) -> Result<(), Box<dyn std::error::Error>> {
    for args in [vec!["add", "-A"], vec!["commit", "-m", message]] {
        assert!(
            Command::new("git")
                .arg("-C")
                .arg(repo)
                .args(args)
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

    // The branch came from disk, not the registry, and the launch payload
    // echoes what the sidebar read — the catalog has to agree with it.
    let client = LocalWorkersClient::new();
    let message = launch_error(
        &client,
        WorkersLaunchRequest::preset("checkout", "no-such-preset")
            .with_worktree(&project.path, "feature/sidebar"),
    );
    assert!(
        !message.contains("worktree does not belong to project"),
        "{message}"
    );
    assert!(!message.contains("project is a folder"), "{message}");
    assert!(message.contains("unknown preset id"), "{message}");

    // The app did not create this checkout, so it is archived, never deleted.
    client
        .remove_worktree("checkout", false)
        .expect_err("an adopted checkout is not app-owned");
    assert!(checkout.exists(), "the adopted checkout stays on disk");
    Ok(())
}

#[test]
fn worktree_lifecycle_removes_checkout_but_retains_child_history()
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
    let snapshot = client.bootstrap()?;
    let historical = snapshot
        .projects
        .iter()
        .find(|project| project.id == worktree.project_id)
        .expect("physical checkout removal preserves its registration and session references");
    assert!(historical.checkout_archived);
    assert_eq!(historical.parent_project_id.as_deref(), Some("root"));
    assert!(
        !Command::new("git")
            .arg("-C")
            .arg(&repo)
            .args(["show-ref", "--verify", "refs/heads/feature/sidebar"])
            .status()?
            .success(),
        "the branch adds nothing to main and should leave with the checkout"
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
fn launch_cannot_bypass_failed_checkout_preparation_and_recreation_retries_same_path()
-> Result<(), Box<dyn std::error::Error>> {
    let _lock = ENV_LOCK.lock().expect("UNPEEL_HOME test lock");
    let (home, _guard) = isolated_home()?;
    let repo = root_at_fixture_repo(home.path())?;
    let config = repo.join(".comet/worktree.json");
    fs::create_dir_all(config.parent().unwrap())?;
    fs::write(&config, r#"{"setup-worktree":"exit 17"}"#)?;
    commit_all(&repo, "add failing worktree setup")?;

    let client = LocalWorkersClient::new();
    let request = WorkersCreateWorktreeRequest {
        project_id: "root".into(),
        branch: "feature/retry-setup".into(),
        name: None,
        base_ref: Some("main".into()),
    };
    let failed = client.create_worktree(request.clone())?;
    assert_eq!(failed.setup_failed_command.as_deref(), Some("exit 17"));

    // The raw project flag and ownership journal are independent recovery
    // evidence. Even if the JSON marker was lost, launch must still notice the
    // journal's pending preparation record.
    let mut state = read_state(home.path())?;
    let failed_record = state["projects"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|project| project["id"] == failed.project_id)
        .expect("failed worktree project is registered");
    failed_record["comet_setup_pending"] = serde_json::json!(false);
    write_state(home.path(), &state)?;
    let error = launch_error(
        &client,
        WorkersLaunchRequest::preset(&failed.project_id, "no-such-preset"),
    );
    assert!(
        error.contains("preparation is pending"),
        "launch must be blocked before preset validation while preparation is pending: {error}"
    );
    assert!(error.contains(&failed.project_id), "{error}");
    assert!(error.contains(&failed.path), "{error}");

    // The next creation request for the same branch retries setup in place.
    fs::write(&config, r#"{"setup-worktree":"true"}"#)?;
    let retried = client.create_worktree(request)?;
    assert_eq!(retried.project_id, failed.project_id);
    assert_eq!(retried.path, failed.path);
    assert_eq!(retried.setup_failed_command, None);
    let error = launch_error(
        &client,
        WorkersLaunchRequest::preset(&retried.project_id, "no-such-preset"),
    );
    assert!(error.contains("unknown preset id"), "{error}");

    // Conversely, preserve a stale app-state pending bit as a second guard if
    // the ownership journal was repaired but the app-state write did not land.
    let mut state = read_state(home.path())?;
    let retried_record = state["projects"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|project| project["id"] == retried.project_id)
        .expect("retried worktree project remains registered");
    retried_record["comet_setup_pending"] = serde_json::json!(true);
    write_state(home.path(), &state)?;
    let error = launch_error(
        &client,
        WorkersLaunchRequest::preset(&retried.project_id, "no-such-preset"),
    );
    assert!(error.contains("preparation is pending"), "{error}");
    Ok(())
}

#[test]
fn concurrent_creation_of_the_same_branch_reuses_one_project_identity_and_path()
-> Result<(), Box<dyn std::error::Error>> {
    let _lock = ENV_LOCK.lock().expect("UNPEEL_HOME test lock");
    let (home, _guard) = isolated_home()?;
    let repo = root_at_fixture_repo(home.path())?;
    let config = repo.join(".comet/worktree.json");
    fs::create_dir_all(config.parent().unwrap())?;
    fs::write(
        &config,
        r#"{"setup-worktree":"touch \"$ROOT_WORKTREE_PATH/setup-started\"; while [ ! -f \"$ROOT_WORKTREE_PATH/release-setup\" ]; do sleep 0.02; done; printf x >> \"$ROOT_WORKTREE_PATH/setup-runs\""}"#,
    )?;
    commit_all(&repo, "add blocking setup command")?;
    let state_path = home.path().join("app-state.json");
    let barrier = Arc::new(Barrier::new(2));
    let create = || WorkersCreateWorktreeRequest {
        project_id: "root".into(),
        branch: "feature/concurrent-same-branch".into(),
        name: None,
        base_ref: Some("main".into()),
    };
    let first_state = state_path.clone();
    let first_barrier = barrier.clone();
    let first_request = std::thread::spawn(move || {
        first_barrier.wait();
        LocalWorkersClient::create_worktree_at(&first_state, create())
    });
    barrier.wait();

    let started = repo.join("setup-started");
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while !started.exists() && std::time::Instant::now() < deadline {
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert!(started.exists(), "first setup reached its blocking command");

    let second_state = state_path.clone();
    let (second_tx, second_rx) = std::sync::mpsc::channel();
    let second_request = std::thread::spawn(move || {
        let result = LocalWorkersClient::create_worktree_at(&second_state, create());
        let _ = second_tx.send(result);
    });
    let second_while_preparing = second_rx.recv_timeout(std::time::Duration::from_millis(500));
    fs::write(repo.join("release-setup"), "go")?;
    let first = first_request
        .join()
        .map_err(|_| "first worktree creation thread panicked")??;
    let second = match second_while_preparing {
        Ok(result) => result,
        Err(std::sync::mpsc::RecvTimeoutError::Timeout) => second_rx
            .recv_timeout(std::time::Duration::from_secs(15))
            .map_err(|_| "second worktree creation did not finish after setup release")?,
        Err(error) => return Err(error.into()),
    };
    second_request
        .join()
        .map_err(|_| "second worktree creation thread panicked")?;

    if let Ok(second) = &second {
        assert_eq!(first.project_id, second.project_id);
        assert_eq!(first.path, second.path);
        assert_eq!(first.branch, second.branch);
    } else {
        let error = second.as_ref().unwrap_err().to_string();
        assert!(error.contains(&first.project_id), "{error}");
        assert!(error.contains(&first.path), "{error}");
    }
    assert!(std::path::Path::new(&first.path).is_dir());
    assert_eq!(
        fs::read_to_string(repo.join("setup-runs"))?,
        "x",
        "only one setup command may run for this checkout"
    );
    let state = read_state(home.path())?;
    let matching: Vec<_> = state["projects"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|project| project["path"] == first.path)
        .collect();
    assert_eq!(matching.len(), 1, "one path must have one project record");
    assert_eq!(matching[0]["id"], first.project_id);
    let listed = Command::new("git")
        .args(["worktree", "list", "--porcelain"])
        .current_dir(repo)
        .output()?;
    assert!(listed.status.success());
    let listed_text = String::from_utf8(listed.stdout)?;
    let registered: Vec<_> = listed_text
        .lines()
        .filter(|line| line.starts_with("worktree "))
        .collect();
    assert_eq!(registered.len(), 2, "main plus one branch worktree");
    Ok(())
}

#[test]
fn a_chat_run_reservation_blocks_removal_of_its_checkout() -> Result<(), Box<dyn std::error::Error>>
{
    let _lock = ENV_LOCK.lock().expect("UNPEEL_HOME test lock");
    let (home, _guard) = isolated_home()?;
    root_at_fixture_repo(home.path())?;
    let client = LocalWorkersClient::new();
    let worktree = client.create_worktree(WorkersCreateWorktreeRequest {
        project_id: "root".into(),
        branch: "feature/chat-active".into(),
        name: None,
        base_ref: Some("main".into()),
    })?;

    let reservation = reserve_chat_run("chat-working", std::path::Path::new(&worktree.path))?
        .expect("a linked checkout receives a Chat run reservation");
    let error = client
        .remove_worktree(&worktree.project_id, false)
        .expect_err("an active Chat must block physical removal");
    assert!(
        error.to_string().contains("Stop active Chats and Workers"),
        "{error}"
    );
    assert!(std::path::Path::new(&worktree.path).is_dir());
    drop(reservation);
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

#[test]
fn archiving_and_restoring_a_checkout_preserves_session_artifacts_and_ids()
-> Result<(), Box<dyn std::error::Error>> {
    let _lock = ENV_LOCK.lock().expect("UNPEEL_HOME test lock");
    let (home, _guard) = isolated_home()?;
    let repo = home.path().join("archive-repo");
    fixture_repo(&repo)?;
    let client = LocalWorkersClient::new();
    let id = client.add_project(&repo)?;
    let session_dir = home.path().join("app-sessions").join("retained-worker");
    fs::create_dir_all(&session_dir)?;
    let manifest = serde_json::to_vec_pretty(&serde_json::json!({
        "session": {"id":"retained-worker","project_id":id,"label":"Keep my work","command":"codex","created_at":1},
        "cwd":repo,"state":"exited","pid":null,"exit_code":0,"updated_at":1
    }))?;
    fs::write(session_dir.join("manifest.json"), &manifest)?;
    fs::write(session_dir.join("output.bin"), "retained terminal history")?;
    client.archive_checkout(&id)?;
    let snapshot = client.bootstrap()?;
    assert!(
        snapshot
            .projects
            .iter()
            .find(|p| p.id == id)
            .unwrap()
            .checkout_archived
    );
    assert_eq!(fs::read(session_dir.join("manifest.json"))?, manifest);
    assert_eq!(
        fs::read_to_string(session_dir.join("output.bin"))?,
        "retained terminal history"
    );
    assert!(repo.exists());
    let restart = client
        .session_action(
            "retained-worker",
            zeron_workers_unpeel::SessionAction::Restart,
        )
        .expect_err("an archived checkout cannot restart a Worker");
    assert!(restart.to_string().contains("archived"), "{restart}");
    client.restore_checkout(&id)?;
    assert!(
        !client
            .bootstrap()?
            .projects
            .iter()
            .find(|p| p.id == id)
            .unwrap()
            .checkout_archived
    );
    let state: serde_json::Value =
        serde_json::from_slice(&fs::read(home.path().join("app-state.json"))?)?;
    assert_eq!(state["unknown_future_field"]["keep"], true);
    assert_eq!(client.add_project(&repo)?, id);
    Ok(())
}

#[test]
fn externally_registered_worktree_inside_managed_root_cannot_be_physically_removed()
-> Result<(), Box<dyn std::error::Error>> {
    let _lock = ENV_LOCK.lock().expect("UNPEEL_HOME test lock");
    let (home, _guard) = isolated_home()?;
    let repo = home.path().join("external-repo");
    fixture_repo(&repo)?;
    let checkout = home.path().join("worktrees").join("external");
    fs::create_dir_all(checkout.parent().unwrap())?;
    assert!(
        Command::new("git")
            .arg("-C")
            .arg(&repo)
            .args(["worktree", "add", "-qb", "external"])
            .arg(&checkout)
            .status()?
            .success()
    );
    let client = LocalWorkersClient::new();
    let id = client.add_project(&checkout)?;
    let project = client
        .bootstrap()?
        .projects
        .into_iter()
        .find(|project| project.id == id)
        .expect("external worktree is registered");
    assert_eq!(
        project.checkout_ownership,
        Some(zeron_workers_unpeel::CheckoutOwnership::External)
    );
    assert!(client.remove_worktree(&id, true).is_err());
    assert!(client.remove_worktree_without_hooks(&id).is_err());
    assert!(checkout.exists());
    assert!(git_ref_exists(&repo, "refs/heads/external")?);
    assert!(
        client
            .bootstrap()?
            .projects
            .iter()
            .any(|project| project.id == id)
    );
    Ok(())
}

#[test]
fn externally_registered_worktree_symlink_inside_managed_root_stays_external()
-> Result<(), Box<dyn std::error::Error>> {
    let _lock = ENV_LOCK.lock().expect("UNPEEL_HOME test lock");
    let (home, _guard) = isolated_home()?;
    let repo = home.path().join("symlink-repo");
    fixture_repo(&repo)?;
    let checkout = home.path().join("external-checkout");
    assert!(
        Command::new("git")
            .arg("-C")
            .arg(&repo)
            .args(["worktree", "add", "-qb", "external-symlink"])
            .arg(&checkout)
            .status()?
            .success()
    );
    let managed_alias = home.path().join("worktrees").join("alias");
    fs::create_dir_all(managed_alias.parent().unwrap())?;
    std::os::unix::fs::symlink(&checkout, &managed_alias)?;

    let client = LocalWorkersClient::new();
    let id = client.add_project(&managed_alias)?;
    let project = client
        .bootstrap()?
        .projects
        .into_iter()
        .find(|project| project.id == id)
        .expect("symlink target is registered");
    assert_eq!(
        project.path,
        fs::canonicalize(&checkout)?.to_string_lossy().into_owned()
    );
    assert_eq!(
        project.checkout_ownership,
        Some(zeron_workers_unpeel::CheckoutOwnership::External)
    );

    assert!(client.remove_worktree(&id, true).is_err());
    assert!(client.remove_worktree_without_hooks(&id).is_err());
    assert!(managed_alias.exists());
    assert!(checkout.exists());
    assert!(git_ref_exists(&repo, "refs/heads/external-symlink")?);
    Ok(())
}

#[test]
fn legacy_worker_app_managed_record_migrates_with_matching_git_evidence()
-> Result<(), Box<dyn std::error::Error>> {
    let _lock = ENV_LOCK.lock().expect("UNPEEL_HOME test lock");
    let (home, _guard) = isolated_home()?;
    let repo = home.path().join("legacy-worker-repo");
    fixture_repo(&repo)?;
    let client = LocalWorkersClient::new();
    let parent_id = client.add_project(&repo)?;

    // This location predates the canonical Comet worktree root. Ownership must
    // come from the old Workers marker plus matching current Git evidence.
    let legacy_root = home.path().join("legacy-unpeel/worktrees");
    let checkout = legacy_root.join("legacy-worker");
    fs::create_dir_all(&legacy_root)?;
    assert!(
        Command::new("git")
            .arg("-C")
            .arg(&repo)
            .args(["worktree", "add", "-qb", "feature/legacy-worker"])
            .arg(&checkout)
            .status()?
            .success()
    );
    let worker_id = client.add_project(&checkout)?;

    let mut state = read_state(home.path())?;
    let project = state["projects"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|project| project["id"] == worker_id)
        .expect("legacy worker project record");
    project["parent_project_id"] = parent_id.into();
    project["worktree_branch"] = "feature/legacy-worker".into();
    project["is_folder"] = true.into();
    let identity = state["comet_project_identity"]["checkouts"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|checkout| checkout["projectID"] == worker_id)
        .expect("legacy worker identity record");
    assert_eq!(identity["ownership"], "external");
    let common_dir = fs::canonicalize(repo.join(".git"))?
        .to_string_lossy()
        .into_owned();
    assert_eq!(
        identity["observedCommonDir"].as_str(),
        Some(common_dir.as_str())
    );
    identity["ownership"] = "app_managed".into();
    write_state(home.path(), &state)?;

    client.remove_worktree(&worker_id, false)?;
    assert!(!checkout.exists());
    // Adopted legacy records cannot prove Comet created the branch, so removal
    // keeps it even when it is integrated.
    assert!(git_ref_exists(&repo, "refs/heads/feature/legacy-worker")?);
    let archived = client
        .bootstrap()?
        .projects
        .into_iter()
        .find(|project| project.id == worker_id)
        .expect("removed checkout history remains registered");
    assert!(archived.checkout_archived);
    Ok(())
}

#[test]
fn forced_managed_removal_does_not_discard_untracked_work() -> Result<(), Box<dyn std::error::Error>>
{
    let _lock = ENV_LOCK.lock().expect("UNPEEL_HOME test lock");
    let (home, _guard) = isolated_home()?;
    let repo = home.path().join("dirty-repo");
    fixture_repo(&repo)?;
    let client = LocalWorkersClient::new();
    let id = client.add_project(&repo)?;
    let child = client.create_worktree(WorkersCreateWorktreeRequest {
        project_id: id,
        branch: "feature/preserve".into(),
        name: None,
        base_ref: Some("main".into()),
    })?;
    let precious = std::path::Path::new(&child.path).join("precious.txt");
    fs::write(&precious, "uncommitted user work")?;
    assert!(client.remove_worktree(&child.project_id, true).is_err());
    assert_eq!(fs::read_to_string(&precious)?, "uncommitted user work");
    assert!(
        client
            .bootstrap()?
            .projects
            .iter()
            .any(|project| project.id == child.project_id)
    );
    Ok(())
}

#[test]
fn approved_pre_remove_hook_cleans_ignored_cache_before_final_cleanliness_check()
-> Result<(), Box<dyn std::error::Error>> {
    let _lock = ENV_LOCK.lock().expect("UNPEEL_HOME test lock");
    let (home, _guard) = isolated_home()?;
    let repo = home.path().join("pre-remove-repo");
    fixture_repo(&repo)?;
    let hook = "rm -rf {{ worktree_path }}/ignored-cache";
    fs::create_dir_all(repo.join(".config"))?;
    fs::write(
        repo.join(".config/wt.toml"),
        format!("pre-remove = {hook:?}\n"),
    )?;
    fs::write(repo.join(".gitignore"), "ignored-cache/\n")?;
    commit_all(&repo, "configure pre-remove hook")?;

    let client = LocalWorkersClient::new();
    let parent_id = client.add_project(&repo)?;
    let child = client.create_worktree(WorkersCreateWorktreeRequest {
        project_id: parent_id,
        branch: "feature/pre-remove-cleanup".into(),
        name: None,
        base_ref: Some("main".into()),
    })?;
    let checkout = std::path::PathBuf::from(&child.path);
    let cache = checkout.join("ignored-cache/cache.bin");
    fs::create_dir_all(cache.parent().unwrap())?;
    fs::write(&cache, "disposable cache")?;
    assert!(
        Command::new("git")
            .arg("-C")
            .arg(&checkout)
            .args(["check-ignore", "--quiet", "ignored-cache/cache.bin"])
            .status()?
            .success(),
        "fixture cache must be ignored"
    );

    client.approve_worktrunk_hook(&child.project_id, "pre-remove", None, hook)?;
    client.remove_worktree(&child.project_id, false)?;
    assert!(!checkout.exists());
    assert!(!git_ref_exists(
        &repo,
        "refs/heads/feature/pre-remove-cleanup"
    )?);
    Ok(())
}

#[test]
fn removal_hooks_come_from_the_principal_checkout_not_the_removed_branch()
-> Result<(), Box<dyn std::error::Error>> {
    let _lock = ENV_LOCK.lock().expect("UNPEEL_HOME test lock");
    let (home, _guard) = isolated_home()?;
    let repo = home.path().join("branch-hook-repo");
    fixture_repo(&repo)?;
    let client = LocalWorkersClient::new();
    let parent_id = client.add_project(&repo)?;
    let child = client.create_worktree(WorkersCreateWorktreeRequest {
        project_id: parent_id,
        branch: "feature/branch-only-hook".into(),
        name: None,
        base_ref: Some("main".into()),
    })?;
    let checkout = std::path::PathBuf::from(&child.path);
    fs::create_dir_all(checkout.join(".config"))?;
    fs::write(
        checkout.join(".config/wt.toml"),
        "pre-remove = \"exit 1\"\n",
    )?;
    commit_all(&checkout, "branch-only pre-remove hook")?;

    client.remove_worktree(&child.project_id, false)?;
    assert!(!checkout.exists());
    assert!(git_ref_exists(
        &repo,
        "refs/heads/feature/branch-only-hook"
    )?);
    Ok(())
}

#[test]
fn open_terminal_blocks_removal_until_it_closes() -> Result<(), Box<dyn std::error::Error>> {
    let _lock = ENV_LOCK.lock().expect("UNPEEL_HOME test lock");
    let (home, _guard) = isolated_home()?;
    let repo = home.path().join("terminal-repo");
    fixture_repo(&repo)?;
    let client = LocalWorkersClient::new();
    let parent_id = client.add_project(&repo)?;
    let child = client.create_worktree(WorkersCreateWorktreeRequest {
        project_id: parent_id,
        branch: "feature/terminal-open".into(),
        name: None,
        base_ref: Some("main".into()),
    })?;
    let checkout = std::path::PathBuf::from(&child.path);
    let nested = checkout.join("nested");
    fs::create_dir_all(&nested)?;
    let terminal = zeron_workers_unpeel::reserve_terminal("terminal-test", &nested)?
        .expect("a linked checkout is reserved");

    assert!(!client.checkout_is_busy(&checkout)?);
    let error = client
        .remove_worktree(&child.project_id, false)
        .expect_err("an open terminal keeps the checkout in use");
    assert!(error.to_string().contains("Stop active"), "{error}");
    assert!(checkout.exists());

    fs::remove_dir(&nested)?;
    drop(terminal);
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        match client.remove_worktree(&child.project_id, false) {
            Ok(()) => break,
            Err(error) if std::time::Instant::now() < deadline => {
                assert!(error.to_string().contains("Stop active"), "{error}");
                std::thread::sleep(std::time::Duration::from_millis(25));
            }
            Err(error) => return Err(error.into()),
        }
    }
    assert!(!checkout.exists());
    Ok(())
}

#[test]
fn removing_without_hooks_still_rejects_ignored_files() -> Result<(), Box<dyn std::error::Error>> {
    let _lock = ENV_LOCK.lock().expect("UNPEEL_HOME test lock");
    let (home, _guard) = isolated_home()?;
    let repo = home.path().join("skip-hooks-repo");
    fixture_repo(&repo)?;
    fs::create_dir_all(repo.join(".config"))?;
    fs::write(repo.join(".config/wt.toml"), "pre-remove = \"exit 0\"\n")?;
    fs::write(repo.join(".gitignore"), "ignored-cache/\n")?;
    commit_all(&repo, "configure hooks and ignored cache")?;

    let client = LocalWorkersClient::new();
    let parent_id = client.add_project(&repo)?;
    let child = client.create_worktree(WorkersCreateWorktreeRequest {
        project_id: parent_id,
        branch: "feature/skip-hooks-dirty".into(),
        name: None,
        base_ref: Some("main".into()),
    })?;
    let checkout = std::path::PathBuf::from(&child.path);
    let ignored = checkout.join("ignored-cache/keep.bin");
    fs::create_dir_all(ignored.parent().unwrap())?;
    fs::write(&ignored, "keep this ignored user data")?;
    assert!(
        Command::new("git")
            .arg("-C")
            .arg(&checkout)
            .args(["check-ignore", "--quiet", "ignored-cache/keep.bin"])
            .status()?
            .success(),
        "fixture file must be ignored"
    );

    let error = client
        .remove_worktree_without_hooks(&child.project_id)
        .expect_err("skipping hooks must not skip the dirty-tree guard");
    assert!(
        error.to_string().contains("Preserve local changes"),
        "expected the cleanliness guard after bypassing hook approval, got: {error}"
    );
    assert!(checkout.exists());
    assert_eq!(fs::read_to_string(&ignored)?, "keep this ignored user data");
    assert!(git_ref_exists(
        &repo,
        "refs/heads/feature/skip-hooks-dirty"
    )?);
    Ok(())
}

#[test]
fn interrupted_removal_blocks_restart_until_explicit_clean_restore()
-> Result<(), Box<dyn std::error::Error>> {
    let _lock = ENV_LOCK.lock().expect("UNPEEL_HOME test lock");
    let (home, _guard) = isolated_home()?;
    let repo = home.path().join("interrupted-repo");
    fixture_repo(&repo)?;
    let client = LocalWorkersClient::new();
    let id = client.add_project(&repo)?;
    let session_dir = home.path().join("app-sessions/interrupted-worker");
    fs::create_dir_all(&session_dir)?;
    let manifest = serde_json::to_vec(&serde_json::json!({
        "session":{"id":"interrupted-worker","project_id":id,"label":"Retained","command":"codex","created_at":1},
        "cwd":repo,"state":"exited","pid":null,"exit_code":0,"updated_at":1
    }))?;
    fs::write(session_dir.join("manifest.json"), &manifest)?;
    let mut state = read_state(home.path())?;
    let checkout = state["comet_project_identity"]["checkouts"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|c| c["projectID"] == id)
        .unwrap();
    checkout["removalInterrupted"] = true.into();
    checkout["lastRemovalError"] = "interrupted test mutation".into();
    write_state(home.path(), &state)?;
    client.reconcile_project_identity()?;
    let error = client
        .session_action(
            "interrupted-worker",
            zeron_workers_unpeel::SessionAction::Restart,
        )
        .expect_err("reconciliation must not authorize an interrupted checkout");
    assert!(error.to_string().contains("interrupted"), "{error}");
    let unsaved = repo.join("unsaved.txt");
    fs::write(&unsaved, "preserve me")?;
    assert!(client.restore_checkout(&id).is_err());
    assert_eq!(fs::read_to_string(&unsaved)?, "preserve me");
    // Resolve the fixture's untracked work, then explicitly acknowledge recovery.
    fs::remove_file(unsaved)?;
    client.restore_checkout(&id)?;
    let registry = client.project_identity_registry()?;
    assert!(
        !registry
            .checkout(&id)
            .unwrap()
            .extra
            .contains_key("removalInterrupted")
    );
    assert_eq!(fs::read(session_dir.join("manifest.json"))?, manifest);
    Ok(())
}
