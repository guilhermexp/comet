//! Local approval for the selected Worktrunk project hooks.
//!
//! Approval binds the Git repository identity and exact source command text.
//! A changed command therefore cannot inherit approval from an older one.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

use crate::worktrunk_hooks::{HookKind, HookPlan, WorktreeHooks};

const STATE_KEY: &str = "comet_worktrunk_hook_approvals";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkersWorktrunkHookCommand {
    pub hook_type: String,
    pub name: Option<String>,
    pub command: String,
    pub approved: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkersWorktrunkHooksSnapshot {
    pub source_path: String,
    pub commands: Vec<WorkersWorktrunkHookCommand>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Approval {
    repository_identity: String,
    hook_type: String,
    name: Option<String>,
    command_sha256: String,
}

fn digest(value: &str) -> String {
    let bytes = Sha256::digest(value.as_bytes());
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        use std::fmt::Write;
        let _ = write!(out, "{byte:02x}");
    }
    out
}

fn repository_identity(path: &Path) -> Result<String, String> {
    let observation = crate::project_identity::probe_checkout(path);
    let common = observation
        .common_dir
        .ok_or_else(|| "Cannot identify the project's Git common directory".to_owned())?;
    let common = std::fs::canonicalize(common).map_err(|error| error.to_string())?;
    let fingerprint = observation
        .common_dir_stable_fingerprint
        .or(observation.common_dir_fingerprint)
        .ok_or_else(|| {
            "Cannot identify the project's Git common directory on this device".to_owned()
        })?;
    Ok(digest(&format!("{}\0{fingerprint}", common.display())))
}

fn project_path(state: &serde_json::Value, project_id: &str) -> Result<PathBuf, String> {
    state
        .get("projects")
        .and_then(serde_json::Value::as_array)
        .and_then(|projects| {
            projects.iter().find(|project| {
                project.get("id").and_then(serde_json::Value::as_str) == Some(project_id)
            })
        })
        .and_then(|project| project.get("path"))
        .and_then(serde_json::Value::as_str)
        .map(PathBuf::from)
        .ok_or_else(|| format!("Unknown project id: {project_id}"))
}

fn approvals(state: &serde_json::Map<String, serde_json::Value>) -> Result<Vec<Approval>, String> {
    match state.get(STATE_KEY) {
        Some(value) => serde_json::from_value(value.clone())
            .map_err(|error| format!("Invalid Worktrunk hook approvals: {error}")),
        None => Ok(Vec::new()),
    }
}

fn commands_for(path: &Path, grants: &[Approval]) -> Result<WorkersWorktrunkHooksSnapshot, String> {
    let source_path = path.join(".config/wt.toml");
    let hooks = WorktreeHooks::read_from(path).map_err(|error| error.to_string())?;
    if [
        HookKind::PreStart,
        HookKind::PostStart,
        HookKind::PreRemove,
        HookKind::PostRemove,
    ]
    .iter()
    .all(|kind| hooks.hook(*kind).is_none())
    {
        return Ok(WorkersWorktrunkHooksSnapshot {
            source_path: source_path.to_string_lossy().into_owned(),
            commands: Vec::new(),
        });
    }
    let identity = repository_identity(path)?;
    let mut commands = Vec::new();
    for kind in [
        HookKind::PreStart,
        HookKind::PostStart,
        HookKind::PreRemove,
        HookKind::PostRemove,
    ] {
        let Some(hook) = hooks.hook(kind) else {
            continue;
        };
        for stage in hook.stages() {
            for command in stage {
                let approved = grants.iter().any(|grant| {
                    grant.repository_identity == identity
                        && grant.hook_type == kind.as_str()
                        && grant.name == command.name
                        && grant.command_sha256 == digest(&command.source)
                });
                commands.push(WorkersWorktrunkHookCommand {
                    hook_type: kind.as_str().into(),
                    name: command.name.clone(),
                    command: command.source.clone(),
                    approved,
                });
            }
        }
    }
    Ok(WorkersWorktrunkHooksSnapshot {
        source_path: source_path.to_string_lossy().into_owned(),
        commands,
    })
}

pub(crate) fn status_at(
    state_path: &Path,
    project_id: &str,
) -> Result<WorkersWorktrunkHooksSnapshot, String> {
    let state = unpeel_core::app_state::load_for_edit_at(state_path)?;
    let path = project_path(&state, project_id)?;
    commands_for(
        &path,
        &approvals(state.as_object().ok_or("Invalid app state")?)?,
    )
}

pub(crate) fn approve_at(
    state_path: &Path,
    project_id: &str,
    hook_type: &str,
    command_name: Option<&str>,
    expected_command: &str,
) -> Result<(), String> {
    let state = unpeel_core::app_state::load_for_edit_at(state_path)?;
    let path = project_path(&state, project_id)?;
    let snapshot = commands_for(
        &path,
        &approvals(state.as_object().ok_or("Invalid app state")?)?,
    )?;
    let matching = snapshot.commands.iter().any(|command| {
        command.hook_type == hook_type
            && command.name.as_deref() == command_name
            && command.command == expected_command
    });
    if !matching {
        return Err("Hook command changed since it was inspected; reload before approving".into());
    }
    let grant = Approval {
        repository_identity: repository_identity(&path)?,
        hook_type: hook_type.to_owned(),
        name: command_name.map(str::to_owned),
        command_sha256: digest(expected_command),
    };
    unpeel_core::app_state::edit_at(state_path, |state| {
        let mut grants = approvals(state)?;
        if !grants.contains(&grant) {
            grants.push(grant.clone());
        }
        state.insert(
            STATE_KEY.into(),
            serde_json::to_value(grants).map_err(|e| e.to_string())?,
        );
        Ok(())
    })
}

/// Check approvals for the exact parsed plan that will be rendered and run.
/// This deliberately does not reopen `.config/wt.toml`: approval must not be
/// borrowed from a different command that appeared after the caller parsed it.
pub(crate) fn approved_for_plan_at(
    state_path: &Path,
    path: &Path,
    kind: HookKind,
    plan: &HookPlan,
) -> Result<Vec<WorkersWorktrunkHookCommand>, String> {
    let state = unpeel_core::app_state::load_for_edit_at(state_path)?;
    let grants = approvals(state.as_object().ok_or("Invalid app state")?)?;
    let identity = repository_identity(path)?;
    let mut commands = Vec::new();
    for stage in plan.stages() {
        for command in stage {
            let approved = grants.iter().any(|grant| {
                grant.repository_identity == identity
                    && grant.hook_type == kind.as_str()
                    && grant.name == command.name
                    && grant.command_sha256 == digest(&command.source)
            });
            commands.push(WorkersWorktrunkHookCommand {
                hook_type: kind.as_str().into(),
                name: command.name.clone(),
                command: command.source.clone(),
                approved,
            });
        }
    }
    Ok(commands)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;

    fn git(path: &Path, args: &[&str]) {
        assert!(
            Command::new("git")
                .arg("-C")
                .arg(path)
                .args(args)
                .status()
                .unwrap()
                .success()
        );
    }

    #[test]
    fn changed_command_revokes_local_approval() {
        let temp = tempfile::tempdir().unwrap();
        let repo = temp.path().join("repo");
        std::fs::create_dir_all(repo.join(".config")).unwrap();
        git(&repo, &["init", "-q"]);
        std::fs::write(repo.join(".config/wt.toml"), "pre-start = \"echo first\"\n").unwrap();
        let state = temp.path().join("app-state.json");
        unpeel_core::app_state::edit_at(&state, |value| {
            value.insert(
                "projects".into(),
                serde_json::json!([{"id":"repo", "path":repo}]),
            );
            Ok(())
        })
        .unwrap();
        let first = status_at(&state, "repo").unwrap();
        assert!(!first.commands[0].approved);
        approve_at(&state, "repo", "pre-start", None, "echo first").unwrap();
        assert!(status_at(&state, "repo").unwrap().commands[0].approved);
        std::fs::write(
            repo.join(".config/wt.toml"),
            "pre-start = \"echo second\"\n",
        )
        .unwrap();
        assert!(!status_at(&state, "repo").unwrap().commands[0].approved);
        assert!(approve_at(&state, "repo", "pre-start", None, "echo first").is_err());
    }

    #[test]
    fn approval_check_uses_the_same_plan_snapshot_that_will_run() {
        let temp = tempfile::tempdir().unwrap();
        let repo = temp.path().join("repo");
        std::fs::create_dir_all(repo.join(".config")).unwrap();
        git(&repo, &["init", "-q"]);
        std::fs::write(
            repo.join(".config/wt.toml"),
            "pre-start = \"echo approved\"\n",
        )
        .unwrap();
        let state = temp.path().join("app-state.json");
        unpeel_core::app_state::edit_at(&state, |value| {
            value.insert(
                "projects".into(),
                serde_json::json!([{"id":"repo", "path":repo}]),
            );
            Ok(())
        })
        .unwrap();
        approve_at(&state, "repo", "pre-start", None, "echo approved").unwrap();

        // The caller parses and renders this command. Simulate the config
        // changing to an already approved command before the approval lookup.
        std::fs::write(
            repo.join(".config/wt.toml"),
            "pre-start = \"echo pending\"\n",
        )
        .unwrap();
        let plan = WorktreeHooks::read_from(&repo)
            .unwrap()
            .hook(HookKind::PreStart)
            .unwrap()
            .clone();
        std::fs::write(
            repo.join(".config/wt.toml"),
            "pre-start = \"echo approved\"\n",
        )
        .unwrap();

        let commands = approved_for_plan_at(&state, &repo, HookKind::PreStart, &plan).unwrap();
        assert_eq!(commands.len(), 1);
        assert_eq!(commands[0].command, "echo pending");
        assert!(!commands[0].approved);
    }
}
