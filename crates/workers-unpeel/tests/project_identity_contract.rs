mod support;
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Mutex;
use std::thread;
use std::time::Duration;

use tempfile::TempDir;
use zeron_workers_unpeel::project_identity::{
    self, CheckoutAvailability, CheckoutKind, CheckoutOwnership, IdentityRegistry,
};
use zeron_workers_unpeel::project_ledger;
use zeron_workers_unpeel::{LocalWorkersClient, WorkersError};

// Each integration test binary is a process, so this only serializes the
// temporary UNPEEL_HOME changes made by tests in this file.
static ENV_LOCK: Mutex<()> = Mutex::new(());

struct UnpeelHomeGuard {
    previous: Option<OsString>,
}

impl UnpeelHomeGuard {
    fn set(path: &Path) -> Self {
        let previous = std::env::var_os("UNPEEL_HOME");
        // SAFETY: callers hold ENV_LOCK for the guard's full lifetime.
        unsafe { std::env::set_var("UNPEEL_HOME", path) };
        Self { previous }
    }
}

impl Drop for UnpeelHomeGuard {
    fn drop(&mut self) {
        // SAFETY: the guard is dropped while its test still holds ENV_LOCK.
        unsafe {
            match self.previous.take() {
                Some(previous) => std::env::set_var("UNPEEL_HOME", previous),
                None => std::env::remove_var("UNPEEL_HOME"),
            }
        }
    }
}

fn write_state(home: &Path, state: &serde_json::Value) -> Result<(), Box<dyn std::error::Error>> {
    fs::write(
        home.join("app-state.json"),
        serde_json::to_vec_pretty(state)?,
    )?;
    Ok(())
}

fn git(path: &Path, args: &[&str]) -> Result<(), Box<dyn std::error::Error>> {
    let status = Command::new("git")
        .arg("-C")
        .arg(path)
        .args(args)
        .status()?;
    assert!(
        status.success(),
        "git -C {} {args:?} failed",
        path.display()
    );
    Ok(())
}

fn canonical_string(path: &Path) -> Result<String, Box<dyn std::error::Error>> {
    Ok(fs::canonicalize(path)?.to_string_lossy().into_owned())
}

fn fixture_repo(home: &Path) -> Result<(PathBuf, PathBuf), Box<dyn std::error::Error>> {
    let repo = home.join("comet");
    let checkout = home.join("comet-feature");
    fs::create_dir_all(&repo)?;
    git(&repo, &["init", "-q", "-b", "main"])?;
    git(&repo, &["config", "user.email", "workers@example.test"])?;
    git(&repo, &["config", "user.name", "Workers Tests"])?;
    fs::write(repo.join("README.md"), "fixture\n")?;
    git(&repo, &["add", "README.md"])?;
    git(&repo, &["commit", "-qm", "fixture"])?;
    let checkout_string = checkout.to_string_lossy().into_owned();
    git(
        &repo,
        &[
            "worktree",
            "add",
            "-q",
            "-b",
            "feature/identity",
            &checkout_string,
        ],
    )?;
    Ok((repo, checkout))
}

fn state_with_projects(projects: serde_json::Value) -> serde_json::Value {
    serde_json::json!({
        "projects": projects,
        "presets": [],
        "active_tabs": {},
        "pinned_sessions": {},
        "sessions": [{"id": "session-kept", "project_id": "feature"}],
        "future_owner_key": {"must": "survive"}
    })
}

fn project(path: &Path, id: &str, name: &str) -> serde_json::Value {
    serde_json::json!({
        "id": id,
        "name": name,
        "path": path,
        "workspace_id": "personal",
        "sort_order": 0
    })
}

fn register_project(
    state: &mut serde_json::Value,
    project_id: &str,
    path: &Path,
    ownership: CheckoutOwnership,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut observation = project_identity::probe_checkout(path);
    observation.project_id = Some(project_id.to_owned());
    observation.ownership = Some(ownership);
    project_identity::register_observation_in_state(
        state.as_object_mut().expect("app state object"),
        project_id,
        observation,
    )?;
    Ok(())
}

#[test]
fn persisted_typed_identity_is_projected_through_real_bootstrap()
-> Result<(), Box<dyn std::error::Error>> {
    let _lock = ENV_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let home = TempDir::new()?;
    let (repo, checkout) = fixture_repo(home.path())?;
    let mut state = state_with_projects(serde_json::json!([
        project(&repo, "main", "comet"),
        project(&checkout, "feature", "feature/identity"),
    ]));
    register_project(&mut state, "main", &repo, CheckoutOwnership::External)?;
    register_project(
        &mut state,
        "feature",
        &checkout,
        CheckoutOwnership::External,
    )?;
    write_state(home.path(), &state)?;
    let _home = UnpeelHomeGuard::set(home.path());

    let client = LocalWorkersClient::new();
    let registry: IdentityRegistry = client.project_identity_registry()?;
    let repository_id = registry
        .checkout("main")
        .and_then(|checkout| checkout.repository_id.as_deref())
        .expect("typed registry links the primary checkout")
        .to_owned();
    assert_eq!(
        registry
            .checkout("feature")
            .and_then(|checkout| checkout.repository_id.as_deref()),
        Some(repository_id.as_str())
    );

    let bootstrap = client.bootstrap()?;
    let main = bootstrap
        .projects
        .iter()
        .find(|project| project.id == "main")
        .expect("primary checkout in bootstrap");
    let feature = bootstrap
        .projects
        .iter()
        .find(|project| project.id == "feature")
        .expect("linked checkout in bootstrap");
    assert_eq!(main.repository_id.as_deref(), Some(repository_id.as_str()));
    assert_eq!(
        feature.repository_id.as_deref(),
        Some(repository_id.as_str())
    );
    assert_eq!(feature.checkout_kind, Some(CheckoutKind::Linked));
    assert_eq!(
        feature.checkout_ownership,
        Some(CheckoutOwnership::External)
    );
    assert_eq!(
        feature.checkout_availability,
        Some(CheckoutAvailability::Available)
    );
    assert_eq!(feature.worktree_branch.as_deref(), Some("feature/identity"));
    assert_eq!(feature.parent_project_id.as_deref(), Some("main"));
    assert_eq!(
        main.repository_path.as_deref(),
        Some(canonical_string(&repo)?.as_str())
    );
    assert_eq!(state["future_owner_key"]["must"], "survive");
    Ok(())
}

#[test]
fn external_checkout_keeps_identity_after_directory_disappears()
-> Result<(), Box<dyn std::error::Error>> {
    let _lock = ENV_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let home = TempDir::new()?;
    let (repo, checkout) = fixture_repo(home.path())?;
    write_state(
        home.path(),
        &state_with_projects(serde_json::json!([project(&repo, "main", "comet")])),
    )?;
    let _home = UnpeelHomeGuard::set(home.path());
    let client = LocalWorkersClient::new();
    let feature_id = client
        .add_project(&checkout, &support::registry())?
        .checkout_id;
    let before = client.project_identity_registry()?;
    let before_checkout = before
        .checkout(&feature_id)
        .expect("external registration is persisted");
    let repository_id = before_checkout
        .repository_id
        .clone()
        .expect("external checkout has repository identity");
    assert_eq!(before_checkout.ownership, CheckoutOwnership::External);
    assert_eq!(
        before_checkout.last_known_branch.as_deref(),
        Some("feature/identity")
    );

    fs::remove_dir_all(&checkout)?;
    let report = client.reconcile_project_identity()?;
    assert!(
        report.changed,
        "removing the directory changes availability"
    );

    let after = client.project_identity_registry()?;
    let historical = after
        .checkout(&feature_id)
        .expect("missing external checkout remains in history");
    assert_eq!(
        historical.repository_id.as_deref(),
        Some(repository_id.as_str())
    );
    assert_eq!(historical.availability, CheckoutAvailability::Missing);
    assert_eq!(historical.ownership, CheckoutOwnership::External);
    assert_eq!(historical.branch, None);
    assert_eq!(
        historical.last_known_branch.as_deref(),
        Some("feature/identity")
    );
    let project_ids = &after.repository(&repository_id).unwrap().project_ids;
    assert!(project_ids.iter().any(|id| id == &feature_id));
    assert!(project_ids.iter().any(|id| id == "main"));
    assert_eq!(project_ids.len(), 2);

    let bootstrap = client.bootstrap()?;
    let historical_project = bootstrap
        .projects
        .iter()
        .find(|project| project.id == feature_id)
        .expect("historical project remains in real bootstrap");
    assert_eq!(
        historical_project.repository_id.as_deref(),
        Some(repository_id.as_str())
    );
    assert_eq!(
        historical_project.checkout_availability,
        Some(CheckoutAvailability::Missing)
    );
    Ok(())
}

#[test]
fn child_first_then_principal_uses_one_repository_identity()
-> Result<(), Box<dyn std::error::Error>> {
    let _lock = ENV_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let home = TempDir::new()?;
    let (repo, checkout) = fixture_repo(home.path())?;
    let state_path = home.path().join("app-state.json");
    write_state(
        home.path(),
        &state_with_projects(serde_json::json!([project(
            &checkout,
            "feature",
            "feature/identity"
        )])),
    )?;
    let _home = UnpeelHomeGuard::set(home.path());

    let first = project_identity::reconcile_at(&state_path)?;
    assert_eq!(first.associated, 1);
    let first_registry = project_identity::read_registry_at(&state_path)?;
    let repository_id = first_registry
        .checkout("feature")
        .and_then(|checkout| checkout.repository_id.clone())
        .expect("child-first probe still identifies the repository");
    assert_eq!(
        first_registry
            .repository(&repository_id)
            .and_then(|repository| repository.primary_project_id.as_deref()),
        None
    );

    unpeel_core::app_state::edit_at(&state_path, |state| {
        state
            .get_mut("projects")
            .and_then(serde_json::Value::as_array_mut)
            .expect("projects array")
            .push(project(&repo, "main", "comet"));
        Ok(())
    })?;
    let second = project_identity::reconcile_at(&state_path)?;
    assert_eq!(second.associated, 2);
    let second_registry = project_identity::read_registry_at(&state_path)?;
    assert_eq!(
        second_registry
            .checkout("main")
            .and_then(|checkout| checkout.repository_id.as_deref()),
        Some(repository_id.as_str())
    );
    let repository = second_registry
        .repository(&repository_id)
        .expect("repository container survives principal registration");
    assert_eq!(repository.primary_project_id.as_deref(), Some("main"));
    assert_eq!(
        repository.primary_path.as_deref(),
        Some(canonical_string(&repo)?.as_str())
    );

    let client = LocalWorkersClient::new();
    let bootstrap = client.bootstrap()?;
    let feature = bootstrap
        .projects
        .iter()
        .find(|project| project.id == "feature")
        .expect("child checkout in bootstrap");
    assert_eq!(
        feature.repository_id.as_deref(),
        Some(repository_id.as_str())
    );
    assert_eq!(feature.parent_project_id.as_deref(), Some("main"));
    Ok(())
}

#[test]
fn repeated_reconciliation_of_fresh_observations_does_not_write()
-> Result<(), Box<dyn std::error::Error>> {
    let _lock = ENV_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let home = TempDir::new()?;
    let (repo, checkout) = fixture_repo(home.path())?;
    let state_path = home.path().join("app-state.json");
    let mut state = state_with_projects(serde_json::json!([
        project(&repo, "main", "comet"),
        project(&checkout, "feature", "feature/identity"),
    ]));
    state["sessions"] = serde_json::json!([
        {"id": "main-session", "project_id": "main"},
        {"id": "feature-session", "project_id": "feature"}
    ]);
    write_state(home.path(), &state)?;

    let first = project_identity::reconcile_at(&state_path)?;
    assert!(first.changed);
    let state_bytes = fs::read(&state_path)?;
    let backup_path = state_path.with_extension("identity-backup.json");
    let journal_path = state_path.with_extension("identity-journal.jsonl");
    let backup_bytes = fs::read(&backup_path)?;
    let journal_bytes = fs::read(&journal_path)?;
    let state_modified = fs::metadata(&state_path)?.modified()?;
    let journal_modified = fs::metadata(&journal_path)?.modified()?;
    thread::sleep(Duration::from_millis(20));

    let second = project_identity::reconcile_at(&state_path)?;
    assert!(!second.changed);
    assert_eq!(second.examined, first.examined);
    assert_eq!(fs::read(&state_path)?, state_bytes);
    assert_eq!(fs::read(&backup_path)?, backup_bytes);
    assert_eq!(fs::read(&journal_path)?, journal_bytes);
    assert_eq!(fs::metadata(&state_path)?.modified()?, state_modified);
    assert_eq!(fs::metadata(&journal_path)?.modified()?, journal_modified);
    Ok(())
}

#[test]
fn diagnose_is_read_only_and_identity_rollback_keeps_later_sessions()
-> Result<(), Box<dyn std::error::Error>> {
    let _lock = ENV_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let home = TempDir::new()?;
    let (repo, checkout) = fixture_repo(home.path())?;
    let state_path = home.path().join("app-state.json");
    let mut state = state_with_projects(serde_json::json!([
        project(&repo, "main", "comet"),
        project(&checkout, "feature", "feature/identity")
    ]));
    state["sessions"] = serde_json::json!([{"id": "before"}]);
    write_state(home.path(), &state)?;
    let original = fs::read(&state_path)?;
    let _home = UnpeelHomeGuard::set(home.path());

    let report = project_identity::diagnose_at(&state_path)?;
    assert!(report.changed);
    assert_eq!(fs::read(&state_path)?, original);
    assert!(!state_path.with_extension("identity-backup.json").exists());
    assert!(!state_path.with_extension("identity-journal.jsonl").exists());

    project_identity::reconcile_at(&state_path)?;
    unpeel_core::app_state::edit_at(&state_path, |state| {
        state["sessions"]
            .as_array_mut()
            .expect("sessions array")
            .push(serde_json::json!({"id": "created-after-migration"}));
        Ok(())
    })?;
    assert!(project_identity::rollback_identity_at(&state_path)?);
    let rolled_back = unpeel_core::app_state::load_for_edit_at(&state_path)?;
    assert!(rolled_back.get(project_identity::IDENTITY_KEY).is_none());
    assert!(
        rolled_back["sessions"]
            .as_array()
            .unwrap()
            .iter()
            .any(|session| session["id"] == "created-after-migration")
    );
    Ok(())
}

#[test]
fn malformed_identity_is_rejected_without_altering_state() -> Result<(), Box<dyn std::error::Error>>
{
    let _lock = ENV_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let home = TempDir::new()?;
    let state = serde_json::json!({
        "projects": [],
        "presets": [],
        "comet_project_identity": {
            "version": 1,
            "repositories": [],
            "checkouts": {"this": "must be an array"}
        },
        "future_owner_key": {"keep": true}
    });
    write_state(home.path(), &state)?;
    let original = fs::read(home.path().join("app-state.json"))?;
    let _home = UnpeelHomeGuard::set(home.path());

    let error = LocalWorkersClient::new()
        .reconcile_project_identity()
        .expect_err("malformed identity must fail closed");
    assert!(matches!(error, WorkersError::State(_)));
    assert!(error.to_string().contains("invalid identity registry"));
    assert_eq!(fs::read(home.path().join("app-state.json"))?, original);
    assert!(!home.path().join("app-state.identity-backup.json").exists());
    assert!(
        !home
            .path()
            .join("app-state.identity-journal.jsonl")
            .exists()
    );
    Ok(())
}

#[test]
fn forgetting_a_checkout_stays_hidden_until_an_explicit_readd()
-> Result<(), Box<dyn std::error::Error>> {
    let _lock = ENV_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let home = TempDir::new()?;
    let (repo, checkout) = fixture_repo(home.path())?;
    write_state(
        home.path(),
        &state_with_projects(serde_json::json!([project(&repo, "main", "comet")])),
    )?;
    let _home = UnpeelHomeGuard::set(home.path());
    let client = LocalWorkersClient::new();
    let checkout_id = client
        .add_project(&checkout, &support::registry())?
        .checkout_id;
    let checkout_path = canonical_string(&checkout)?;

    let visible_before = client.projects_with_ledger()?;
    assert!(visible_before.iter().any(|row| row.path == checkout_path));
    let sessions_before = unpeel_core::app_state::load()?["sessions"].clone();

    assert!(project_ledger::forget(&checkout_path)?);
    let visible_after_poll = client.projects_with_ledger()?;
    assert!(
        !visible_after_poll
            .iter()
            .any(|row| row.path == checkout_path)
    );

    // A fresh client models reopening the Settings/Workers surface after the
    // polling pass. Suppression is durable, so bootstrap reconciliation must
    // not re-create the forgotten ledger row.
    let reopened = LocalWorkersClient::new();
    let visible_after_reopen = reopened.projects_with_ledger()?;
    assert!(
        !visible_after_reopen
            .iter()
            .any(|row| row.path == checkout_path)
    );
    let after_forget = unpeel_core::app_state::load()?;
    assert_eq!(after_forget["sessions"], sessions_before);

    // Explicit registration is the user's opt-in restoration and clears the
    // suppression marker in the same atomic write as the project registration.
    assert_eq!(
        reopened
            .add_project(&checkout, &support::registry())?
            .checkout_id,
        checkout_id
    );
    let visible_after_readd = reopened.projects_with_ledger()?;
    assert!(
        visible_after_readd
            .iter()
            .any(|row| row.path == checkout_path)
    );
    let registry = reopened.project_identity_registry()?;
    assert!(
        !registry
            .suppressed_project_ids
            .iter()
            .any(|project_id| project_id == &checkout_id)
    );
    Ok(())
}

#[test]
fn ledger_only_paths_get_stable_non_executable_identity_and_readd_reuses_it()
-> Result<(), Box<dyn std::error::Error>> {
    let home = TempDir::new()?;
    let (repo, checkout) = fixture_repo(home.path())?;
    let missing = home.path().join("removed-feature");
    let checkout_path = canonical_string(&checkout)?;
    let missing_path = missing.to_string_lossy().into_owned();
    let state_path = home.path().join("app-state.json");
    let mut state = state_with_projects(serde_json::json!([]));
    state[project_ledger::LEDGER_KEY] = serde_json::json!([
        {
            "path": checkout_path,
            "name": "feature/identity",
            "added_at_unix_ms": 11,
            "last_seen_at_unix_ms": 22
        },
        {
            "path": missing_path,
            "name": "removed-feature",
            "added_at_unix_ms": 33,
            "last_seen_at_unix_ms": 44
        }
    ]);
    write_state(home.path(), &state)?;

    let first = project_identity::reconcile_at(&state_path)?;
    assert_eq!(first.examined, 2);
    let first_state = unpeel_core::app_state::load_for_edit_at(&state_path)?;
    assert!(first_state["projects"].as_array().unwrap().is_empty());
    let first_registry: IdentityRegistry =
        serde_json::from_value(first_state[project_identity::IDENTITY_KEY].clone())?;
    let historical = first_registry
        .checkouts
        .iter()
        .find(|checkout| checkout.path == checkout_path)
        .expect("ledger worktree identity");
    let historical_id = historical.project_id.clone();
    let historical_checkout_id = historical.checkout_id.clone();
    let historical_repository_id = historical.repository_id.clone();
    assert!(historical_id.starts_with("comet-history-"));
    assert_eq!(historical.kind, CheckoutKind::Linked);
    assert_eq!(historical.availability, CheckoutAvailability::Available);
    assert_eq!(historical.extra["ledgerOnly"], true);
    assert_eq!(historical.extra["nonExecutable"], true);
    assert_eq!(
        historical_checkout_id.as_deref(),
        Some(historical_id.as_str())
    );
    assert!(historical_repository_id.is_some());

    let removed = first_registry
        .checkouts
        .iter()
        .find(|checkout| checkout.path == missing_path)
        .expect("ledger missing identity");
    assert!(removed.project_id.starts_with("comet-history-"));
    assert_eq!(removed.availability, CheckoutAvailability::Missing);
    assert_eq!(removed.extra["nonExecutable"], true);

    // A historical synthetic ID is a valid association target even though it
    // never appeared in projects[].
    let mut associated_state = first_state.clone();
    let repository_id = first_registry
        .repositories
        .first()
        .map(|repository| repository.id.clone())
        .expect("available historical checkout repository");
    project_identity::associate_checkout_in_state(
        &mut associated_state,
        &removed.project_id,
        &repository_id,
    )?;

    // Re-add through the same atomic registration seam used by LocalWorkers.
    let live_id = "readded-feature";
    unpeel_core::app_state::edit_at(&state_path, |state| {
        state["projects"] = serde_json::json!([project(&checkout, live_id, "feature/identity")]);
        let mut observation = project_identity::probe_checkout(&checkout);
        observation.project_id = Some(live_id.to_owned());
        observation.ownership = Some(CheckoutOwnership::External);
        project_identity::register_observation_in_state(state, live_id, observation).map(|_| ())
    })?;
    let second = project_identity::reconcile_at(&state_path)?;
    assert!(
        !second.changed,
        "reconcile must not duplicate the re-added path"
    );
    let second_state = unpeel_core::app_state::load_for_edit_at(&state_path)?;
    let second_registry: IdentityRegistry =
        serde_json::from_value(second_state[project_identity::IDENTITY_KEY].clone())?;
    let readded = second_registry
        .checkout(live_id)
        .expect("re-added checkout is executable");
    assert_eq!(second_registry.checkouts.len(), 2);
    assert_eq!(readded.checkout_id, historical_checkout_id);
    assert_eq!(readded.repository_id, historical_repository_id);
    assert!(!readded.extra.contains_key("ledgerOnly"));
    assert!(!readded.extra.contains_key("nonExecutable"));
    assert!(!readded.archived);
    assert!(second_registry.checkout(&historical_id).is_none());
    assert_eq!(
        second_state[project_ledger::LEDGER_KEY][0]["added_at_unix_ms"],
        11
    );
    assert_eq!(second_state["projects"].as_array().unwrap().len(), 1);
    let _ = repo;
    Ok(())
}
