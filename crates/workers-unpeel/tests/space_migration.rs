//! One-time migration of the Workers state into the project registry:
//! backup, identity reconcile, link every checkout to the Space of its
//! repository root (or its own folder), marker. Every repository and state
//! file is a temporary fixture; the registry is in memory.
mod support;

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::{Value, json};
use tempfile::TempDir;
use zeron_workers_unpeel::project_identity;
use zeron_workers_unpeel::space_links::{
    MIGRATION_BACKUP_FILE, MIGRATION_KEY, MigrationOutcome, migrate_at,
};
use zeron_workers_unpeel::space_registry::SpaceRegistry;

fn git(cwd: &Path, args: &[&str]) {
    let status = Command::new("git")
        .args(args)
        .current_dir(cwd)
        .env("GIT_AUTHOR_NAME", "t")
        .env("GIT_AUTHOR_EMAIL", "t@t")
        .env("GIT_COMMITTER_NAME", "t")
        .env("GIT_COMMITTER_EMAIL", "t@t")
        .status()
        .unwrap();
    assert!(status.success(), "git {args:?}");
}

fn repo(path: &Path) -> PathBuf {
    fs::create_dir_all(path).unwrap();
    git(path, &["init", "-q", "-b", "main"]);
    git(path, &["config", "commit.gpgsign", "false"]);
    fs::write(path.join("README.md"), "fixture\n").unwrap();
    git(path, &["add", "README.md"]);
    git(path, &["commit", "-q", "-m", "fixture"]);
    fs::canonicalize(path).unwrap()
}

fn worktree(root: &Path, path: &Path, branch: &str) -> PathBuf {
    git(
        root,
        &[
            "worktree",
            "add",
            "-q",
            "-b",
            branch,
            path.to_str().unwrap(),
        ],
    );
    fs::canonicalize(path).unwrap()
}

fn registration(id: &str, path: &Path) -> Value {
    json!({ "id": id, "name": path.file_name().unwrap().to_string_lossy(), "path": path, "sort_order": 0 })
}

fn write_state(home: &Path, state: Value) -> PathBuf {
    let path = home.join("app-state.json");
    fs::write(&path, serde_json::to_vec_pretty(&state).unwrap()).unwrap();
    path
}

fn space_of(state_path: &Path, checkout_id: &str) -> Option<String> {
    project_identity::read_registry_at(state_path)
        .unwrap()
        .checkout(checkout_id)
        .and_then(|checkout| checkout.space_id.clone())
}

/// Scenario "Worktrees whose principal was not registered".
#[test]
fn unregistered_principal_with_three_worktrees_becomes_one_project() {
    let home = TempDir::new().unwrap();
    let root = repo(&home.path().join("JK Distribuição"));
    let trees: Vec<PathBuf> = ["sec-cron", "sec-xss", "sec-export"]
        .iter()
        .map(|name| worktree(&root, &home.path().join(".worktrees-jk").join(name), name))
        .collect();
    let state = write_state(
        home.path(),
        json!({
            "projects": [
                registration("comet-cron", &trees[0]),
                registration("comet-xss", &trees[1]),
                registration("comet-export", &trees[2]),
            ],
            "presets": [], "active_tabs": {}, "pinned_sessions": {}
        }),
    );
    let registry = support::registry();

    let outcome = migrate_at(&state, &registry, 42).unwrap();

    let spaces = registry.list().unwrap();
    assert_eq!(spaces.len(), 1, "one project for the repository root");
    assert_eq!(spaces[0].path, root.to_string_lossy());
    assert_eq!(spaces[0].name, "JK Distribuição");
    for id in ["comet-cron", "comet-xss", "comet-export"] {
        assert_eq!(space_of(&state, id).as_deref(), Some(spaces[0].id.as_str()));
    }
    let MigrationOutcome::Migrated(outcome) = outcome else {
        panic!("first run migrates");
    };
    assert_eq!(outcome.linked.len(), 3);
    assert!(outcome.pending.is_empty());
    assert!(home.path().join(MIGRATION_BACKUP_FILE).is_file());
    let raw: Value = serde_json::from_slice(&fs::read(&state).unwrap()).unwrap();
    assert_eq!(raw[MIGRATION_KEY]["version"], 1);
}

/// Scenario "A registration that already has a Space".
#[test]
fn a_registration_whose_folder_has_a_space_links_to_it() {
    let home = TempDir::new().unwrap();
    let orchestrator = repo(&home.path().join("orchestrator"));
    let state = write_state(
        home.path(),
        json!({ "projects": [registration("comet-0d5a", &orchestrator)], "presets": [] }),
    );
    let registry = support::registry();
    let existing = registry.ensure(&orchestrator, None).unwrap();

    migrate_at(&state, &registry, 1).unwrap();

    assert_eq!(registry.list().unwrap().len(), 1, "no Space is created");
    assert_eq!(space_of(&state, "comet-0d5a"), Some(existing.id));
}

/// Scenario "Running migration again": nothing created, no link changed, no
/// new backup.
#[test]
fn a_second_run_is_a_no_op() {
    let home = TempDir::new().unwrap();
    let folder = repo(&home.path().join("denchclaw-crm"));
    let state = write_state(
        home.path(),
        json!({ "projects": [registration("comet-101a", &folder)], "presets": [] }),
    );
    let registry = support::registry();
    migrate_at(&state, &registry, 1).unwrap();
    let backup = home.path().join(MIGRATION_BACKUP_FILE);
    let backup_bytes = fs::read(&backup).unwrap();
    let backup_modified = fs::metadata(&backup).unwrap().modified().unwrap();
    let state_bytes = fs::read(&state).unwrap();

    let second = migrate_at(&state, &registry, 2).unwrap();

    assert_eq!(second, MigrationOutcome::AlreadyMigrated);
    assert_eq!(registry.list().unwrap().len(), 1);
    assert_eq!(fs::read(&state).unwrap(), state_bytes, "no link changes");
    assert_eq!(fs::read(&backup).unwrap(), backup_bytes);
    assert_eq!(
        fs::metadata(&backup).unwrap().modified().unwrap(),
        backup_modified
    );
    let backups = fs::read_dir(home.path())
        .unwrap()
        .filter(|entry| {
            entry
                .as_ref()
                .unwrap()
                .file_name()
                .to_string_lossy()
                .contains("space-migration-backup")
        })
        .count();
    assert_eq!(backups, 1);
}

/// Scenario "A record without evidence": a ledger-only record whose folder
/// is gone and whose identity has no repository stays association-pending —
/// it is never linked by name or path prefix — and stays reachable.
#[test]
fn a_record_without_evidence_stays_pending() {
    let home = TempDir::new().unwrap();
    let kept = repo(&home.path().join("kept"));
    let state = write_state(
        home.path(),
        json!({
            "projects": [registration("comet-kept", &kept)],
            "comet_projects": [{
                "path": home.path().join("kept-old-clone").to_string_lossy(),
                "name": "kept",
                "added_at_unix_ms": 1,
                "last_seen_at_unix_ms": 2
            }],
            "presets": []
        }),
    );
    let registry = support::registry();

    let MigrationOutcome::Migrated(outcome) = migrate_at(&state, &registry, 1).unwrap() else {
        panic!("first run migrates");
    };

    assert_eq!(
        registry.list().unwrap().len(),
        1,
        "only the evidenced folder"
    );
    let identity = project_identity::read_registry_at(&state).unwrap();
    let gone = identity
        .checkouts
        .iter()
        .find(|checkout| checkout.path.ends_with("kept-old-clone"))
        .expect("ledger-only record keeps its identity row");
    assert_eq!(gone.space_id, None);
    assert!(outcome.pending.contains(&gone.project_id));
    let rows = zeron_workers_unpeel::project_ledger::decorate_with_identity(
        zeron_workers_unpeel::project_ledger::reconcile(
            &zeron_workers_unpeel::project_ledger::read_at(&state).unwrap(),
            &[],
            3,
        )
        .rows,
        &identity,
    );
    let row = rows
        .iter()
        .find(|row| row.path.ends_with("kept-old-clone"))
        .expect("reachable in Settings → Projects");
    assert!(row.is_pending());
}

/// Scenario "Existing sessions survive the link": ids, sessions, order and
/// sort modes keyed by `comet-*` ids are untouched; only the identity
/// namespace gains links and the marker is added.
#[test]
fn existing_sessions_survive_the_link() {
    let home = TempDir::new().unwrap();
    let root = repo(&home.path().join("comet"));
    let tree = worktree(
        &root,
        &home.path().join("worker-wait-and-notice"),
        "worker/wait",
    );
    let sessions = home.path().join("app-sessions").join("5d1d506a");
    fs::create_dir_all(&sessions).unwrap();
    let manifest =
        json!({ "session": { "id": "5d1d506a", "project_id": "comet-4848" }, "cwd": tree });
    fs::write(sessions.join("manifest.json"), manifest.to_string()).unwrap();
    fs::write(
        home.path().join("session-order.json"),
        json!({ "comet-4848": ["5d1d506a"], "comet-4298": [] }).to_string(),
    )
    .unwrap();
    let state = write_state(
        home.path(),
        json!({
            "projects": [registration("comet-4298", &root), registration("comet-4848", &tree)],
            "session_sort_modes": { "comet-4848": "custom" },
            "pinned_sessions": { "comet-4848": ["5d1d506a"] },
            "active_tabs": { "comet-4848": "5d1d506a" },
            "presets": []
        }),
    );
    let before: Value = serde_json::from_slice(&fs::read(&state).unwrap()).unwrap();
    let order_before = fs::read(home.path().join("session-order.json")).unwrap();
    let manifest_before = fs::read(sessions.join("manifest.json")).unwrap();

    migrate_at(&state, &support::registry(), 1).unwrap();

    let after: Value = serde_json::from_slice(&fs::read(&state).unwrap()).unwrap();
    for key in [
        "projects",
        "session_sort_modes",
        "pinned_sessions",
        "active_tabs",
        "presets",
    ] {
        assert_eq!(after[key], before[key], "{key} must not change");
    }
    assert_eq!(
        fs::read(home.path().join("session-order.json")).unwrap(),
        order_before
    );
    assert_eq!(
        fs::read(sessions.join("manifest.json")).unwrap(),
        manifest_before
    );
    let root_space = space_of(&state, "comet-4298").expect("principal linked");
    assert_eq!(
        space_of(&state, "comet-4848").as_deref(),
        Some(root_space.as_str())
    );
}
