mod support;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::{Value, json};
use tempfile::TempDir;
use zeron_workers_unpeel::{LocalWorkersClient, controller_mcp_handle_request};

// Every case uses a child test process with a fixed synthetic profile. A late
// reconciliation thread can never observe another test or the user's profile.
fn isolated_case(name: &str) -> Result<Option<PathBuf>, Box<dyn std::error::Error>> {
    if std::env::var("COMET_IDENTITY_TEST_CASE").ok().as_deref() == Some(name) {
        return Ok(Some(PathBuf::from(
            std::env::var_os("UNPEEL_HOME").expect("child profile"),
        )));
    }
    let home = TempDir::new()?;
    let output = Command::new(std::env::current_exe()?)
        .args(["--exact", name, "--nocapture"])
        .env("COMET_IDENTITY_TEST_CASE", name)
        .env("UNPEEL_HOME", home.path())
        .env("COMET_WORKERS_HOOKS_DIR", home.path().join("hooks"))
        .env("ZERON_DATA_DIR", home.path().join("zeron-data"))
        .output()?;
    assert!(
        output.status.success(),
        "isolated {name} failed:\n{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(None)
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

fn fixture_repo(home: &Path) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let repo = home.join("repository");
    fs::create_dir_all(&repo)?;
    git(&repo, &["init", "-q", "-b", "main"])?;
    git(&repo, &["config", "user.email", "workers@example.test"])?;
    git(&repo, &["config", "user.name", "Workers Tests"])?;
    fs::write(repo.join("README.md"), "fixture\n")?;
    git(&repo, &["add", "README.md"])?;
    git(&repo, &["commit", "-qm", "fixture"])?;
    Ok(repo)
}

fn write_state(home: &Path, state: &Value) -> Result<(), Box<dyn std::error::Error>> {
    fs::write(
        home.join("app-state.json"),
        serde_json::to_vec_pretty(state)?,
    )?;
    Ok(())
}

fn conflict_state(repo: &Path) -> Value {
    let path = repo.to_string_lossy();
    let common_dir = fs::canonicalize(repo.join(".git"))
        .expect("fixture Git common directory")
        .to_string_lossy()
        .into_owned();
    json!({
        "projects": [{
            "id": "blocked-project",
            "name": "Blocked Project",
            "path": path,
            "workspace_id": "identity-tests",
            "sort_order": 0
        }],
        "presets": [],
        "sessions": [{"id": "session-kept", "project_id": "blocked-project"}],
        "comet_project_identity": {
            "version": 1,
            "repositories": [{
                "id": "repository-1",
                "commonDir": common_dir,
                "commonDirFingerprint": "unix:16777229:325942460",
                "projectIDs": ["blocked-project", "unrelated-project"]
            }],
            "checkouts": [{
                "projectID": "blocked-project",
                "checkoutID": "blocked-project",
                "repositoryID": "repository-1",
                "path": path,
                "kind": "primary",
                "ownership": "external",
                "availability": "available",
                "conflict": "common directory fingerprint changed from unix:16777229:325942460 to unix:16777234:325942460"
            }, {
                "projectID": "unrelated-project",
                "checkoutID": "unrelated-project",
                "repositoryID": "repository-1",
                "path": path,
                "kind": "primary",
                "ownership": "external",
                "availability": "available",
                "conflict": "association removed by user"
            }]
        },
        "future_owner_key": {"must": "survive"}
    })
}

fn mcp_recovery_request(expected_current_fingerprint: &str) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": "recover-identity",
        "method": "tools/call",
        "params": {
            "name": "workers",
            "arguments": {
                "action": "recover_project_identity",
                "project_id": "blocked-project",
                "expected_old_fingerprint": "unix:16777229:325942460",
                "expected_current_fingerprint": expected_current_fingerprint
            }
        }
    })
}

#[test]
fn blocked_registered_project_reports_identity_conflict_before_spawn()
-> Result<(), Box<dyn std::error::Error>> {
    let Some(home) =
        isolated_case("blocked_registered_project_reports_identity_conflict_before_spawn")?
    else {
        return Ok(());
    };
    let repo = fixture_repo(home.as_path())?;
    write_state(home.as_path(), &conflict_state(&repo))?;

    let error = LocalWorkersClient::new()
        .create_session("blocked-project", "true")
        .expect_err("identity-conflicted projects must not launch");
    let message = error.to_string();
    assert!(
        message.contains("conflict") || message.contains("recovery"),
        "known blocked project must name its blocker and recovery path, got: {message}"
    );
    assert!(
        !message.contains("unknown project"),
        "registered project must not be reported as unknown, got: {message}"
    );
    assert!(
        !unpeel_core::app_paths::app_sessions_root().exists(),
        "blocked launch must not create a worker session"
    );
    Ok(())
}

#[test]
fn unknown_project_stays_distinct_from_a_known_blocked_project()
-> Result<(), Box<dyn std::error::Error>> {
    let Some(home) = isolated_case("unknown_project_stays_distinct_from_a_known_blocked_project")?
    else {
        return Ok(());
    };
    let repo = fixture_repo(home.as_path())?;
    write_state(home.as_path(), &conflict_state(&repo))?;

    let error = LocalWorkersClient::new()
        .create_session("missing-project", "true")
        .expect_err("unknown projects must not launch");
    assert!(
        error.to_string().contains("unknown project"),
        "unregistered id should retain the unknown-project diagnosis: {error}"
    );
    Ok(())
}

#[test]
fn adding_a_git_project_persists_a_stable_volume_identity() -> Result<(), Box<dyn std::error::Error>>
{
    let Some(home) = isolated_case("adding_a_git_project_persists_a_stable_volume_identity")?
    else {
        return Ok(());
    };
    let repo = fixture_repo(home.as_path())?;

    LocalWorkersClient::new()
        .add_project(&repo, &support::registry())?
        .checkout_id;
    let state: Value = serde_json::from_slice(&fs::read(home.as_path().join("app-state.json"))?)?;
    let repository = &state["comet_project_identity"]["repositories"][0];
    #[cfg(target_os = "macos")]
    assert!(
        repository["commonDirStableFingerprint"].is_string(),
        "fresh registration must persist the stable volume/directory identity: {repository}"
    );
    #[cfg(target_os = "macos")]
    assert!(
        repository["commonDirStableFingerprint"]
            .as_str()
            .is_some_and(|fingerprint| fingerprint.starts_with("macos:")),
        "macOS stable identity must use the macOS volume UUID format: {repository}"
    );
    Ok(())
}

#[test]
fn stale_recovery_expectation_refuses_to_change_identity_state()
-> Result<(), Box<dyn std::error::Error>> {
    let Some(home) = isolated_case("stale_recovery_expectation_refuses_to_change_identity_state")?
    else {
        return Ok(());
    };
    let repo = fixture_repo(home.as_path())?;
    write_state(home.as_path(), &conflict_state(&repo))?;
    let state_path = home.as_path().join("app-state.json");
    let before = fs::read(&state_path)?;
    let current = zeron_workers_unpeel::project_identity::probe_checkout(&repo)
        .common_dir_stable_fingerprint
        .or_else(|| {
            zeron_workers_unpeel::project_identity::probe_checkout(&repo).common_dir_fingerprint
        })
        .expect("fixture common directory fingerprint");
    let error = zeron_workers_unpeel::project_identity::recover_project_identity_at(
        &state_path,
        "blocked-project",
        "unix:stale-device:stale-inode",
        &current,
    )
    .expect_err("stale expected old identity must refuse recovery");
    assert!(
        error.contains("expected old") || error.contains("matching"),
        "stale recovery should explain the guarded rejection: {error}"
    );
    assert_eq!(
        fs::read(&state_path)?,
        before,
        "stale recovery must not write app state"
    );
    Ok(())
}

#[test]
fn controller_exposes_explicit_identity_recovery_action() -> Result<(), Box<dyn std::error::Error>>
{
    let Some(home) = isolated_case("controller_exposes_explicit_identity_recovery_action")? else {
        return Ok(());
    };
    let repo = fixture_repo(home.as_path())?;
    let mut state = conflict_state(&repo);
    // Diagnosis must be sufficient input for recovery even when a background
    // reconciler has not yet persisted its freshly observed conflict.
    state["comet_project_identity"]["checkouts"][0]["conflict"] = Value::Null;
    write_state(home.as_path(), &state)?;
    let observation = zeron_workers_unpeel::project_identity::probe_checkout(&repo);
    let current = observation
        .common_dir_stable_fingerprint
        .as_deref()
        .or(observation.common_dir_fingerprint.as_deref())
        .expect("fixture common directory fingerprint");

    let diagnosis = controller_mcp_handle_request(json!({
        "jsonrpc": "2.0",
        "id": "diagnose-identity",
        "method": "tools/call",
        "params": {
            "name": "workers",
            "arguments": {"action": "diagnose_project_identity"}
        }
    }))
    .expect("MCP diagnosis responds");
    let candidate = diagnosis["result"]["content"][0]["text"]
        .as_str()
        .and_then(|text| serde_json::from_str::<Value>(text).ok())
        .or_else(|| diagnosis.get("result").cloned())
        .expect("diagnosis payload");
    let candidates = candidate
        .get("report")
        .and_then(|report| report.get("recoveryCandidates"))
        .and_then(Value::as_array)
        .expect("diagnosis returns recovery candidates");
    assert!(
        candidates.iter().any(|candidate| {
            candidate["projectId"] == "blocked-project"
                && candidate["expectedOldFingerprint"] == "unix:16777229:325942460"
                && candidate["expectedCurrentFingerprint"] == current
        }),
        "diagnosis must return exact recovery arguments: {diagnosis}"
    );

    let response = controller_mcp_handle_request(mcp_recovery_request(current))
        .expect("MCP tools/call responds");
    assert_eq!(
        response["result"]["isError"], false,
        "explicit recovery action should be callable: {response}"
    );
    let recovered_state: Value =
        serde_json::from_slice(&fs::read(home.as_path().join("app-state.json"))?)?;
    assert_eq!(
        recovered_state["sessions"][0]["id"], "session-kept",
        "identity recovery must retain sessions"
    );
    assert_eq!(
        recovered_state["future_owner_key"]["must"], "survive",
        "identity recovery must retain unknown top-level fields"
    );
    assert_eq!(
        recovered_state["comet_project_identity"]["checkouts"][1]["conflict"],
        "association removed by user",
        "unrelated conflicts must remain intact"
    );
    let repeated = controller_mcp_handle_request(mcp_recovery_request(current))
        .expect("repeated MCP recovery responds");
    assert_eq!(
        repeated["result"]["structuredContent"]["changed"], false,
        "recovery is idempotent after the first guarded write: {repeated}"
    );
    Ok(())
}
