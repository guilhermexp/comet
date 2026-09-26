//! Bind the selected Worktrunk hooks to Comet's checkout lifecycle.

use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::git_command::run_git;
use crate::worktrunk_hooks::{self, HookKind, HookRenderContext, RenderedHook, WorktreeHooks};

const PRE_HOOK_TIMEOUT: Duration = Duration::from_secs(300);

pub(crate) struct StartHookOutcome {
    pub failure: Option<String>,
    pub warning: Option<String>,
    pub post_start: Option<RenderedHook>,
}

fn checkout_context(
    repository: &Path,
    checkout: &Path,
    branch: &str,
    base: Option<&str>,
    kind: HookKind,
    cwd: &Path,
) -> HookRenderContext {
    let mut context = HookRenderContext::default();
    let repository_name = repository
        .file_name()
        .map(|value| value.to_string_lossy().into_owned())
        .unwrap_or_default();
    let checkout_name = checkout
        .file_name()
        .map(|value| value.to_string_lossy().into_owned())
        .unwrap_or_default();
    let commit = run_git(checkout, &["rev-parse", "HEAD"])
        .unwrap_or_default()
        .trim()
        .to_owned();
    let default_branch = crate::branch_cleanup::default_branch_name(repository)
        .ok()
        .flatten();
    for (key, value) in [
        ("branch", branch.to_owned()),
        ("worktree_path", checkout.display().to_string()),
        ("worktree_name", checkout_name),
        ("repo", repository_name),
        ("repo_path", repository.display().to_string()),
        ("primary_worktree_path", repository.display().to_string()),
        ("commit", commit.clone()),
        ("short_commit", commit.chars().take(7).collect()),
        ("base", base.unwrap_or_default().to_owned()),
        ("hook_type", kind.as_str().to_owned()),
        ("cwd", cwd.display().to_string()),
    ] {
        context.insert(key, value);
    }
    // An absent default branch has no truthful template value. Let the
    // renderer report a missing variable if a hook asks for it.
    if let Some(default_branch) = default_branch {
        context.insert("default_branch", default_branch);
    }
    context
}

fn hook_log_path(checkout: &Path, kind: HookKind) -> PathBuf {
    let name = checkout
        .file_name()
        .map(|value| value.to_string_lossy().into_owned())
        .unwrap_or_else(|| "checkout".into());
    checkout
        .parent()
        .unwrap_or(checkout)
        .join(".logs")
        .join(name)
        .join(format!("{}.log", kind.as_str()))
}

fn prepared_hook(
    state_path: &Path,
    source: &Path,
    repository: &Path,
    checkout: &Path,
    branch: &str,
    base: Option<&str>,
    kind: HookKind,
    cwd: &Path,
) -> Result<Option<RenderedHook>, String> {
    let hooks = WorktreeHooks::read_from(source).map_err(|error| error.to_string())?;
    let Some(plan) = hooks.hook(kind) else {
        return Ok(None);
    };
    // Render the entire hook before executing a stage. An unsupported token in
    // a later pipeline stage must not leave a partially run hook behind.
    let rendered = plan
        .render(&checkout_context(
            repository, checkout, branch, base, kind, cwd,
        ))
        .map_err(|error| error.to_string())?;
    let approvals =
        crate::worktrunk_approvals::approved_for_plan_at(state_path, source, kind, plan)?;
    if let Some(pending) = approvals.iter().find(|command| !command.approved) {
        return Err(format!(
            "{} hook `{}` is pending approval in Settings > Projects",
            kind.as_str(),
            pending.name.as_deref().unwrap_or(&pending.command)
        ));
    }
    Ok(Some(rendered))
}

pub(crate) fn run_start_hooks(
    state_path: &Path,
    repository: &Path,
    checkout: &Path,
    branch: &str,
    base: Option<&str>,
) -> StartHookOutcome {
    let mut outcome = StartHookOutcome {
        failure: None,
        warning: None,
        post_start: None,
    };
    match prepared_hook(
        state_path,
        repository,
        repository,
        checkout,
        branch,
        base,
        HookKind::PreStart,
        checkout,
    ) {
        Ok(Some(hook)) => {
            if let Err(error) = worktrunk_hooks::run_pre_hook(&hook, checkout, PRE_HOOK_TIMEOUT) {
                outcome.failure = Some(error.to_string());
                return outcome;
            }
        }
        Ok(None) => {}
        Err(error) => {
            outcome.failure = Some(error);
            return outcome;
        }
    }
    match prepared_hook(
        state_path,
        repository,
        repository,
        checkout,
        branch,
        base,
        HookKind::PostStart,
        checkout,
    ) {
        Ok(Some(hook)) => outcome.post_start = Some(hook),
        Ok(None) => {}
        Err(error) => outcome.warning = Some(error),
    }
    outcome
}

/// Start the advisory hook only after durable preparation has been recorded.
pub(crate) fn spawn_post_start(hook: RenderedHook, checkout: &Path) -> Result<(), String> {
    let checkout = std::fs::canonicalize(checkout).map_err(|error| error.to_string())?;
    worktrunk_hooks::spawn_post_hook(
        hook,
        &checkout,
        &checkout,
        &hook_log_path(&checkout, HookKind::PostStart),
    )
    .map_err(|error| error.to_string())
}

pub(crate) fn run_pre_remove(
    state_path: &Path,
    repository: &Path,
    checkout: &Path,
    branch: &str,
    skip_hooks: bool,
) -> Result<(), String> {
    if skip_hooks {
        return Ok(());
    }
    if let Some(hook) = prepared_hook(
        state_path,
        repository,
        repository,
        checkout,
        branch,
        None,
        HookKind::PreRemove,
        checkout,
    )? {
        worktrunk_hooks::run_pre_hook(&hook, checkout, PRE_HOOK_TIMEOUT)
            .map_err(|error| error.to_string())?;
    }
    Ok(())
}

pub(crate) fn prepare_post_remove(
    state_path: &Path,
    repository: &Path,
    checkout: &Path,
    branch: &str,
    skip_hooks: bool,
) -> Result<Option<RenderedHook>, String> {
    if skip_hooks {
        return Ok(None);
    }
    prepared_hook(
        state_path,
        repository,
        repository,
        checkout,
        branch,
        None,
        HookKind::PostRemove,
        repository,
    )
}

pub(crate) fn spawn_post_remove(
    hook: RenderedHook,
    repository: &Path,
    checkout: &Path,
) -> Result<(), String> {
    worktrunk_hooks::spawn_post_hook(
        hook,
        repository,
        checkout,
        &hook_log_path(checkout, HookKind::PostRemove),
    )
    .map_err(|error| error.to_string())
}
