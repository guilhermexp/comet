//! Checkout removal validates ownership independently from branch presentation.
use crate::git_command::run_git;
use std::path::{Path, PathBuf};

pub(crate) struct CreatedCheckout {
    pub path: PathBuf,
    pub branch: String,
}

fn default_base_ref(repository: &Path) -> Option<String> {
    if let Ok(base) = run_git(
        repository,
        &[
            "symbolic-ref",
            "--quiet",
            "--short",
            "refs/remotes/origin/HEAD",
        ],
    ) {
        let base = base.trim();
        if !base.is_empty() {
            return Some(base.into());
        }
    }
    for candidate in ["origin/main", "origin/master", "main", "master"] {
        if run_git(repository, &["rev-parse", "--verify", "--quiet", candidate]).is_ok() {
            return Some(candidate.into());
        }
    }
    None
}

/// Caller holds the shared action lock across creation and association. The
/// journal, rather than the directory prefix, establishes ownership.
pub(crate) fn create_checkout_under_lock(
    _action: &CheckoutActionLock,
    repository: &Path,
    name: &str,
    branch: &str,
    base_ref: Option<&str>,
    root: &Path,
    journal: &crate::worktree_ownership::OwnershipJournal,
) -> Result<CreatedCheckout, crate::WorkersError> {
    use crate::WorkersError;
    let branch = branch.trim();
    if branch.is_empty() || run_git(repository, &["check-ref-format", "--branch", branch]).is_err()
    {
        return Err(WorkersError::State("Invalid worktree branch".into()));
    }
    if base_ref.is_some_and(|base| base.is_empty() || base.starts_with('-')) {
        return Err(WorkersError::State("Invalid worktree base ref".into()));
    }
    let repo_root =
        run_git(repository, &["rev-parse", "--show-toplevel"]).map_err(WorkersError::State)?;
    let repo_root = std::fs::canonicalize(repo_root.trim())
        .map_err(|error| WorkersError::State(error.to_string()))?;
    let target = crate::worktree_ownership::worktree_path(root, &repo_root, name)
        .map_err(|error| WorkersError::State(error.to_string()))?;
    if std::fs::symlink_metadata(&target).is_ok() {
        let target_canonical = std::fs::canonicalize(&target)
            .map_err(|error| WorkersError::State(error.to_string()))?;
        let listed = run_git(&repo_root, &["worktree", "list", "--porcelain", "-z"])
            .map_err(WorkersError::State)?;
        let found = listed
            .split('\0')
            .filter_map(|line| line.strip_prefix("worktree "))
            .any(|path| std::fs::canonicalize(path).ok().as_ref() == Some(&target_canonical));
        if !found {
            return Err(WorkersError::State(format!(
                "Worktree destination exists but is not a linked checkout: {}",
                target.display()
            )));
        }
        let current_branch = run_git(
            &target_canonical,
            &["symbolic-ref", "--quiet", "--short", "HEAD"],
        )
        .map_err(WorkersError::State)?;
        if current_branch.trim() != branch {
            return Err(WorkersError::State(format!(
                "Worktree destination already checks out `{}`, not `{branch}`",
                current_branch.trim()
            )));
        }
        return Ok(CreatedCheckout {
            path: target_canonical,
            branch: branch.into(),
        });
    }

    let operation_id = uuid::Uuid::new_v4().to_string();
    journal
        .pending_create(&repo_root, &target, &operation_id)
        .map_err(|error| WorkersError::State(error.to_string()))?;
    // An unavailable remote must not prevent local work, as in the previous
    // Workers path. The mutation itself remains bounded by git_command.
    let _ = crate::git_command::run_git_mutation(&repo_root, &["fetch", "--quiet", "origin"]);
    let base = base_ref
        .map(str::to_owned)
        .or_else(|| default_base_ref(&repo_root));
    let target_text = target
        .to_str()
        .ok_or_else(|| WorkersError::State("Worktree path is not UTF-8".into()))?;
    let branch_exists = run_git(
        &repo_root,
        &[
            "show-ref",
            "--verify",
            "--quiet",
            &format!("refs/heads/{branch}"),
        ],
    )
    .is_ok();
    let mut args = vec!["worktree", "add"];
    if branch_exists {
        args.extend([target_text, branch]);
    } else {
        args.extend(["-b", branch, target_text]);
        if let Some(base) = base.as_deref() {
            args.push(base);
        }
    }
    if let Err(error) = crate::git_command::run_git_mutation(&repo_root, &args) {
        // A failed Git command can leave a partial worktree. Release the
        // reservation only when both filesystem and Git agree that nothing
        // was created; otherwise keep PendingCreate for explicit recovery.
        let _ = journal.abort_absent_create(&repo_root, &target, &operation_id);
        return Err(WorkersError::State(error));
    }
    journal
        .created_observed(&repo_root, &target, &operation_id)
        .map_err(|error| WorkersError::State(format!(
            "Worktree {} was created but its Git identity could not be recorded: {error}; it has been preserved for explicit recovery",
            target.display()
        )))?;
    journal
        .finalize_owned(&repo_root, &target, &operation_id)
        .map_err(|error| WorkersError::State(format!(
            "Worktree {} was created but ownership could not be finalized: {error}; it has been preserved for recovery",
            target.display()
        )))?;
    Ok(CreatedCheckout {
        path: target,
        branch: branch.into(),
    })
}

pub(crate) fn validate_removal(
    path: &Path,
    owned: bool,
    active_worker: bool,
) -> Result<PathBuf, String> {
    if !owned {
        return Err("This checkout was not created by Comet; archive it instead".into());
    }
    if active_worker {
        return Err("Stop the checkout's Workers before removing it".into());
    }
    let target = std::fs::canonicalize(path).map_err(|e| format!("Checkout unavailable: {e}"))?;
    let top = run_git(&target, &["rev-parse", "--show-toplevel"])?;
    if std::fs::canonicalize(top.trim()).map_err(|e| e.to_string())? != target {
        return Err("The removal target must be the checkout root".into());
    }
    let git_dir = run_git(&target, &["rev-parse", "--absolute-git-dir"])?;
    let common = run_git(
        &target,
        &["rev-parse", "--path-format=absolute", "--git-common-dir"],
    )?;
    if std::fs::canonicalize(git_dir.trim()).map_err(|e| e.to_string())?
        == std::fs::canonicalize(common.trim()).map_err(|e| e.to_string())?
    {
        return Err("The principal checkout cannot be removed as a worktree".into());
    }
    let listing = run_git(&target, &["worktree", "list", "--porcelain", "-z"])?;
    let registered = listing
        .split('\0')
        .filter_map(|line| line.strip_prefix("worktree "))
        .any(|p| std::fs::canonicalize(p).is_ok_and(|p| p == target));
    if !registered {
        return Err("The checkout is not a registered Git worktree".into());
    }
    let status = run_git(
        &target,
        &[
            "status",
            "--porcelain",
            "--untracked-files=all",
            "--ignored",
        ],
    )?;
    if !status.trim().is_empty() {
        return Err(
            "Preserve local changes and untracked files before removing this checkout".into(),
        );
    }
    let retained = run_git(
        &target,
        &[
            "for-each-ref",
            "--contains=HEAD",
            "--format=%(refname)",
            "refs/heads",
            "refs/tags",
        ],
    )?;
    if retained.trim().is_empty() {
        return Err(
            "Create a branch or tag preserving this checkout's HEAD before removing it".into(),
        );
    }
    Ok(target)
}

/// Serializes Comet launches/removals across UI and controller MCP processes.
/// Kept separate from app-state.lock: Git and host creation never hold the
/// shared JSON write lock.
pub(crate) struct CheckoutActionLock(std::fs::File);

pub(crate) fn lock_checkout_actions() -> Result<CheckoutActionLock, crate::WorkersError> {
    let root = unpeel_core::app_paths::unpeel_home();
    std::fs::create_dir_all(&root).map_err(|e| crate::WorkersError::State(e.to_string()))?;
    lock_checkout_actions_at(&root.join("checkout-actions.lock"))
        .map_err(crate::WorkersError::State)
}

fn lock_checkout_actions_at(path: &Path) -> Result<CheckoutActionLock, String> {
    use std::os::fd::AsRawFd;
    use std::os::unix::fs::OpenOptionsExt;
    let file = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .mode(0o600)
        .open(path)
        .map_err(|e| e.to_string())?;
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(360);
    loop {
        // SAFETY: the descriptor is owned and stays open for the guard's lifetime.
        if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } == 0 {
            break;
        }
        let error = std::io::Error::last_os_error();
        if !matches!(error.raw_os_error(), Some(code) if code == libc::EWOULDBLOCK || code == libc::EAGAIN)
        {
            return Err(error.to_string());
        }
        if std::time::Instant::now() >= deadline {
            return Err(format!(
                "Timed out waiting for checkout action lock {}",
                path.display()
            ));
        }
        std::thread::sleep(std::time::Duration::from_millis(25));
    }
    Ok(CheckoutActionLock(file))
}

pub(crate) fn checkout_is_busy(
    client: &crate::LocalWorkersClient,
    path: &Path,
) -> Result<bool, crate::WorkersError> {
    let _action = lock_checkout_actions()?;
    checkout_is_busy_under_lock(client, path)
}

fn checkout_is_busy_under_lock(
    client: &crate::LocalWorkersClient,
    path: &Path,
) -> Result<bool, crate::WorkersError> {
    use crate::WorkersError;
    let canonical =
        std::fs::canonicalize(path).map_err(|error| WorkersError::State(error.to_string()))?;
    if crate::checkout_activity::busy_at(&crate::checkout_activity::activity_file(), &canonical)
        .map_err(WorkersError::State)?
        .is_some()
    {
        return Ok(true);
    }
    let snapshot = client.bootstrap()?;
    Ok(snapshot.sessions.iter().any(|session| {
        session.is_live()
            && snapshot.projects.iter().any(|project| {
                project.id == session.project_id
                    && std::fs::canonicalize(&project.path).ok().as_deref()
                        == Some(canonical.as_path())
            })
    }))
}

/// The physical removal used by both Chat and Workers. The action lock must
/// cover the first ownership proof through the final Git mutation. It is not
/// enough for a checkout to sit below the configured worktree root.
pub(crate) fn remove_checkout_under_lock(
    _action: &CheckoutActionLock,
    client: &crate::LocalWorkersClient,
    repository: &Path,
    checkout: &Path,
    journal: &crate::worktree_ownership::OwnershipJournal,
    state_path: &Path,
    skip_hooks: bool,
) -> Result<(), crate::WorkersError> {
    use crate::WorkersError;
    let checkout = std::fs::canonicalize(checkout)
        .map_err(|error| WorkersError::State(format!("Checkout unavailable: {error}")))?;
    if !journal
        .verify_owned(repository, &checkout)
        .map_err(|error| WorkersError::State(error.to_string()))?
    {
        return Err(WorkersError::State(
            "This checkout has no matching Comet ownership proof; archive it instead".into(),
        ));
    }
    if checkout_is_busy_under_lock(client, &checkout)? {
        return Err(WorkersError::State(
            "Stop active Chats and Workers before removing this checkout".into(),
        ));
    }
    let branch = run_git(&checkout, &["symbolic-ref", "--quiet", "--short", "HEAD"])
        .unwrap_or_default()
        .trim()
        .to_owned();
    if skip_hooks {
        unpeel_core::hook_assets::append_trace_log_line(&format!(
            "Worktree removal without hooks requested for {}",
            checkout.display()
        ));
    }
    crate::worktrunk_lifecycle::run_pre_remove(
        state_path, repository, &checkout, &branch, skip_hooks,
    )
    .map_err(WorkersError::State)?;
    // Prepare the post hook while the source checkout and its config still
    // exist. A pending post hook is advisory; removal is still safe to finish.
    let post = match crate::worktrunk_lifecycle::prepare_post_remove(
        state_path, repository, &checkout, &branch, skip_hooks,
    ) {
        Ok(post) => post,
        Err(error) => {
            unpeel_core::hook_assets::append_trace_log_line(&format!(
                "Post-remove hook skipped for {}: {error}",
                checkout.display()
            ));
            None
        }
    };
    if !journal
        .verify_owned(repository, &checkout)
        .map_err(|error| WorkersError::State(error.to_string()))?
    {
        return Err(WorkersError::State(
            "Checkout identity changed while pre-remove ran; refusing removal".into(),
        ));
    }
    if checkout_is_busy_under_lock(client, &checkout)? {
        return Err(WorkersError::State(
            "A Chat or Worker became active during pre-remove; refusing removal".into(),
        ));
    }
    crate::worktrunk_hooks::stop_post_start(&checkout)
        .map_err(|error| WorkersError::State(error.to_string()))?;
    validate_removal(&checkout, true, false).map_err(WorkersError::State)?;
    let path = checkout
        .to_str()
        .ok_or_else(|| WorkersError::State("Checkout path is not UTF-8".into()))?;
    crate::git_command::run_git_mutation(repository, &["worktree", "remove", path])
        .map_err(WorkersError::State)?;
    journal
        .retire_removed(repository, &checkout)
        .map_err(|error| {
            WorkersError::State(format!(
                "Checkout removed, but ownership journal could not be retired: {error}"
            ))
        })?;
    if let Some(post) = post {
        if let Err(error) =
            crate::worktrunk_lifecycle::spawn_post_remove(post, repository, &checkout)
        {
            unpeel_core::hook_assets::append_trace_log_line(&format!(
                "Post-remove hook failed to start for {}: {error}",
                checkout.display()
            ));
        }
    }
    Ok(())
}

/// Prune only Git's stale administrative entry for a previously Comet-owned
/// checkout whose leaf has vanished. This never removes a directory or branch.
pub(crate) fn prune_missing_checkout_under_lock(
    _action: &CheckoutActionLock,
    repository: &Path,
    checkout: &Path,
    journal: &crate::worktree_ownership::OwnershipJournal,
) -> Result<(), crate::WorkersError> {
    use crate::WorkersError;
    let missing = match std::fs::symlink_metadata(checkout) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => true,
        Ok(_) => false,
        Err(error) => return Err(WorkersError::State(error.to_string())),
    };
    if !missing {
        return Err(WorkersError::State(
            "Checkout still exists; stale-registration prune refused".into(),
        ));
    }
    let parent = checkout
        .parent()
        .ok_or_else(|| WorkersError::State("Checkout has no parent".into()))?;
    let parent = std::fs::canonicalize(parent)
        .map_err(|error| WorkersError::State(format!("Checkout parent is unavailable: {error}")))?;
    let leaf = checkout
        .file_name()
        .ok_or_else(|| WorkersError::State("Checkout has no leaf".into()))?;
    let target = parent.join(leaf);
    if !journal
        .was_owned_missing(repository, &target)
        .map_err(|error| WorkersError::State(error.to_string()))?
    {
        return Err(WorkersError::State(
            "Missing checkout has no Comet ownership proof".into(),
        ));
    }
    let registered = |listing: &str| {
        listing
            .split('\0')
            .filter_map(|part| part.strip_prefix("worktree "))
            .any(|path| Path::new(path) == target)
    };
    let before = run_git(repository, &["worktree", "list", "--porcelain", "-z"])
        .map_err(WorkersError::State)?;
    if !registered(&before) {
        return Err(WorkersError::State(
            "Missing checkout is not registered with Git".into(),
        ));
    }
    // `--expire now` can also drop other stale admin entries, but it never
    // touches filesystem directories or branches. A targeted forced remove
    // could erase a leaf recreated between the absence probe and Git spawn.
    crate::git_command::run_git_mutation(repository, &["worktree", "prune", "--expire", "now"])
        .map_err(WorkersError::State)?;
    let after = run_git(repository, &["worktree", "list", "--porcelain", "-z"])
        .map_err(WorkersError::State)?;
    if registered(&after) {
        return Err(WorkersError::State(
            "Git did not prune the missing checkout registration".into(),
        ));
    }
    journal
        .retire_removed(repository, &target)
        .map_err(|error| {
            WorkersError::State(format!(
                "Git registration pruned, but ownership journal could not be retired: {error}"
            ))
        })?;
    Ok(())
}

impl Drop for CheckoutActionLock {
    fn drop(&mut self) {
        use std::os::fd::AsRawFd;
        // SAFETY: this guard still owns the valid descriptor.
        unsafe {
            libc::flock(self.0.as_raw_fd(), libc::LOCK_UN);
        }
    }
}

pub(crate) fn remove_owned_checkout(
    client: &crate::LocalWorkersClient,
    project_id: &str,
    skip_hooks: bool,
) -> Result<(), crate::WorkersError> {
    use crate::{CheckoutKind, CheckoutOwnership, WorkersError};
    let action = lock_checkout_actions()?;
    client.reconcile_project_identity()?;
    let registry = client.project_identity_registry()?;
    let checkout = registry
        .checkout(project_id)
        .ok_or_else(|| WorkersError::State("Unknown checkout".into()))?
        .clone();
    if checkout.kind != CheckoutKind::Linked
        || checkout.ownership != CheckoutOwnership::AppManaged
        || checkout.conflict.is_some()
    {
        return Err(WorkersError::State("Only an unchanged Comet-owned linked worktree can be removed; archive this checkout instead".into()));
    }
    let repository = checkout
        .repository_id
        .as_deref()
        .and_then(|id| registry.repository(id))
        .and_then(|record| record.primary_path.as_deref())
        .or(checkout.main_repo.as_deref())
        .ok_or_else(|| {
            WorkersError::State("Checkout has no principal repository identity".into())
        })?;
    let repository = Path::new(repository);
    let path = Path::new(&checkout.path);
    let journal = crate::worktree_ownership::OwnershipJournal::for_current_user()
        .map_err(|error| WorkersError::State(error.to_string()))?;
    // Only the authoritative legacy Workers registry may import its old
    // AppManaged evidence. Chat legacy records and roots alone never migrate.
    journal
        .migrate_legacy_worker(
            repository,
            path,
            checkout.ownership,
            checkout.observed_common_dir.as_deref().map(Path::new),
        )
        .map_err(|error| WorkersError::State(error.to_string()))?;
    if !journal
        .verify_owned(repository, path)
        .map_err(|error| WorkersError::State(error.to_string()))?
    {
        return Err(WorkersError::State(
            "Checkout has no matching Comet ownership proof; archive it instead".into(),
        ));
    }
    set_removal_pending(project_id, &checkout, true)?;
    let result = remove_checkout_under_lock(
        &action,
        client,
        repository,
        path,
        &journal,
        &unpeel_core::app_paths::app_state_path(),
        skip_hooks,
    )
    .and_then(|()| edit_archived(project_id, true));
    if let Err(error) = &result {
        if !path.exists() {
            let _ = mark_removal_interrupted(project_id, &error.to_string());
        }
    }
    let released = set_removal_pending(project_id, &checkout, false);
    result.and(released)
}

fn mark_removal_interrupted(project_id: &str, message: &str) -> Result<(), crate::WorkersError> {
    use crate::project_identity::{IDENTITY_KEY, IdentityRegistry};
    unpeel_core::app_state::edit(|state| {
        let mut registry: IdentityRegistry = serde_json::from_value(
            state
                .get(IDENTITY_KEY)
                .cloned()
                .ok_or("Missing checkout identity")?,
        )
        .map_err(|e| e.to_string())?;
        let checkout = registry
            .checkouts
            .iter_mut()
            .find(|c| c.project_id == project_id)
            .ok_or("Unknown checkout")?;
        checkout
            .extra
            .insert("removalInterrupted".into(), true.into());
        checkout
            .extra
            .insert("lastRemovalError".into(), message.into());
        checkout.availability = if Path::new(&checkout.path).is_dir() {
            crate::CheckoutAvailability::ProbeFailed
        } else {
            crate::CheckoutAvailability::Missing
        };
        state.insert(
            IDENTITY_KEY.into(),
            serde_json::to_value(registry).map_err(|e| e.to_string())?,
        );
        Ok(())
    })
    .map_err(crate::WorkersError::State)
}

fn set_removal_pending(
    project_id: &str,
    expected: &crate::CheckoutIdentity,
    pending: bool,
) -> Result<(), crate::WorkersError> {
    use crate::project_identity::{IDENTITY_KEY, IDENTITY_SCHEMA_VERSION, IdentityRegistry};
    unpeel_core::app_state::edit(|state| {
        let mut registry: IdentityRegistry = serde_json::from_value(
            state
                .get(IDENTITY_KEY)
                .cloned()
                .ok_or("Missing checkout identity")?,
        )
        .map_err(|e| e.to_string())?;
        if registry.version != IDENTITY_SCHEMA_VERSION {
            return Err("Unsupported identity version".into());
        }
        let checkout = registry
            .checkouts
            .iter_mut()
            .find(|c| c.project_id == project_id)
            .ok_or("Unknown checkout")?;
        if checkout.path != expected.path
            || checkout.repository_id != expected.repository_id
            || checkout.ownership != expected.ownership
        {
            return Err("Checkout identity changed during removal".into());
        }
        if pending {
            // The cross-process action lock is held by the caller. A remaining
            // marker therefore belongs to a previous interrupted operation.
            checkout
                .extra
                .insert("removalPending".into(), serde_json::Value::Bool(true));
        } else {
            checkout.extra.remove("removalPending");
        }
        state.insert(
            IDENTITY_KEY.into(),
            serde_json::to_value(registry).map_err(|e| e.to_string())?,
        );
        Ok(())
    })
    .map_err(crate::WorkersError::State)
}

pub(crate) fn ensure_session_checkout_available(
    client: &crate::LocalWorkersClient,
    session_id: &str,
) -> Result<(), crate::WorkersError> {
    use crate::{CheckoutAvailability, WorkersError};
    if Path::new(session_id).components().count() != 1
        || !matches!(
            Path::new(session_id).components().next(),
            Some(std::path::Component::Normal(_))
        )
    {
        return Err(WorkersError::State("Invalid Worker id".into()));
    }
    let path = unpeel_core::app_paths::app_sessions_root()
        .join(session_id)
        .join("manifest.json");
    let manifest: serde_json::Value = serde_json::from_slice(
        &std::fs::read(path).map_err(|e| WorkersError::State(e.to_string()))?,
    )?;
    let cwd = manifest
        .get("cwd")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| WorkersError::State("Worker checkout is unavailable".into()))?;
    if !Path::new(cwd).is_absolute() || !Path::new(cwd).is_dir() {
        return Err(WorkersError::State(
            "Worker checkout is unavailable; restore its folder before restarting".into(),
        ));
    }
    let registry = client.project_identity_registry()?;
    let project_id = manifest
        .pointer("/session/project_id")
        .and_then(serde_json::Value::as_str);
    let identity = project_id
        .and_then(|id| registry.checkout(id))
        .or_else(|| registry.checkouts.iter().find(|c| c.path == cwd));
    if let Some(checkout) = identity {
        if checkout.archived {
            return Err(WorkersError::State(
                "Restore the archived checkout before restarting its Worker".into(),
            ));
        }
        if checkout.conflict.is_some()
            || checkout
                .extra
                .get("removalPending")
                .and_then(serde_json::Value::as_bool)
                == Some(true)
            || checkout
                .extra
                .get("removalInterrupted")
                .and_then(serde_json::Value::as_bool)
                == Some(true)
        {
            return Err(WorkersError::State(
                "Resolve the checkout's interrupted removal or identity conflict before restarting"
                    .into(),
            ));
        }
        if checkout.availability != CheckoutAvailability::Available {
            return Err(WorkersError::State("Worker checkout is unavailable".into()));
        }
    }
    Ok(())
}

impl crate::LocalWorkersClient {
    pub fn archive_checkout(&self, project_id: &str) -> Result<(), crate::WorkersError> {
        let _checkout_action = lock_checkout_actions()?;
        self.reconcile_project_identity()?;
        edit_archived(project_id, true)
    }

    pub fn restore_checkout(&self, project_id: &str) -> Result<(), crate::WorkersError> {
        let _checkout_action = lock_checkout_actions()?;
        self.reconcile_project_identity()?;
        let registry = self.project_identity_registry()?;
        let checkout = registry
            .checkout(project_id)
            .ok_or_else(|| crate::WorkersError::State("Unknown checkout".into()))?;
        if checkout.availability != crate::CheckoutAvailability::Available
            || checkout.conflict.is_some()
        {
            return Err(crate::WorkersError::State(
                "Restore requires an available checkout with a resolved identity".into(),
            ));
        }
        if checkout
            .extra
            .get("removalInterrupted")
            .and_then(serde_json::Value::as_bool)
            == Some(true)
        {
            let status = run_git(
                Path::new(&checkout.path),
                &["status", "--porcelain", "--untracked-files=all"],
            )
            .map_err(crate::WorkersError::State)?;
            if !status.trim().is_empty() {
                return Err(crate::WorkersError::State("Repair or preserve the checkout's local changes before restoring an interrupted removal".into()));
            }
        }
        edit_archived(project_id, false)
    }
}

fn edit_archived(project_id: &str, archived: bool) -> Result<(), crate::WorkersError> {
    unpeel_core::app_state::edit(|state| {
        let mut value = serde_json::Value::Object(state.clone());
        crate::project_identity::set_checkout_archived(&mut value, project_id, archived)
            .map_err(|e| e.to_string())?;
        let key = crate::project_identity::IDENTITY_KEY;
        let mut identity: crate::IdentityRegistry =
            serde_json::from_value(value[key].clone()).map_err(|e| e.to_string())?;
        if let Some(checkout) = identity
            .checkouts
            .iter_mut()
            .find(|c| c.project_id == project_id)
        {
            checkout.extra.remove("removalPending");
            if !archived {
                checkout.extra.remove("removalInterrupted");
                checkout.extra.remove("lastRemovalError");
            }
        }
        value[key] = serde_json::to_value(identity).map_err(|e| e.to_string())?;
        *state = value.as_object().cloned().ok_or("Invalid project state")?;
        Ok(())
    })
    .map_err(crate::WorkersError::State)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;
    struct Fixture {
        root: PathBuf,
        repo: PathBuf,
        managed: PathBuf,
        checkout: PathBuf,
    }
    impl Fixture {
        fn new() -> Self {
            let root =
                std::env::temp_dir().join(format!("comet-lifecycle-{}", uuid::Uuid::new_v4()));
            let repo = root.join("repo");
            let managed = root.join("managed");
            let checkout = managed.join("feature");
            std::fs::create_dir_all(&repo).unwrap();
            std::fs::create_dir_all(&managed).unwrap();
            git(&repo, &["init", "-q"]);
            git(
                &repo,
                &[
                    "-c",
                    "user.name=Test",
                    "-c",
                    "user.email=test@example.invalid",
                    "commit",
                    "--allow-empty",
                    "-qm",
                    "initial",
                ],
            );
            git(
                &repo,
                &[
                    "worktree",
                    "add",
                    "-qb",
                    "feature",
                    checkout.to_str().unwrap(),
                ],
            );
            Self {
                root,
                repo,
                managed,
                checkout,
            }
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }
    fn git(path: &Path, args: &[&str]) {
        let out = Command::new("git")
            .arg("-C")
            .arg(path)
            .args(args)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
    #[test]
    fn clean_owned_linked_checkout_is_removable_without_touching_the_branch() {
        let f = Fixture::new();
        let target = validate_removal(&f.checkout, true, false).unwrap();
        assert_eq!(target, std::fs::canonicalize(&f.checkout).unwrap());
        assert!(f.checkout.is_dir());
        git(&f.repo, &["show-ref", "--verify", "refs/heads/feature"]);
    }
    #[test]
    fn failed_git_add_without_checkout_does_not_block_corrected_retry() {
        let f = Fixture::new();
        let root = f.root.join("common-worktrees");
        let journal = crate::worktree_ownership::OwnershipJournal::at(
            f.root.join("worktree-ownership.json"),
            root.clone(),
        );
        let action = lock_checkout_actions_at(&f.root.join("checkout-actions.lock")).unwrap();
        let failed = create_checkout_under_lock(
            &action,
            &f.repo,
            "retry",
            "retrybranch",
            Some("no-such-ref"),
            &root,
            &journal,
        );
        assert!(failed.is_err());
        let created = create_checkout_under_lock(
            &action,
            &f.repo,
            "retry",
            "retrybranch",
            Some("HEAD"),
            &root,
            &journal,
        )
        .unwrap();
        assert!(journal.verify_owned(&f.repo, &created.path).unwrap());
    }
    #[test]
    fn rejects_external_main_arbitrary_and_active_checkout() {
        let f = Fixture::new();
        for (path, owned, active) in [
            (&f.checkout, false, false),
            (&f.repo, true, false),
            (&f.managed, true, false),
            (&f.checkout, true, true),
        ] {
            assert!(validate_removal(path, owned, active).is_err());
            assert!(path.exists());
        }
    }
    #[test]
    fn untracked_and_modified_files_block_checkout_removal() {
        let f = Fixture::new();
        std::fs::write(f.checkout.join("precious.txt"), "user work").unwrap();
        assert!(validate_removal(&f.checkout, true, false).is_err());
        git(&f.checkout, &["add", "precious.txt"]);
        git(
            &f.checkout,
            &[
                "-c",
                "user.name=Test",
                "-c",
                "user.email=test@example.invalid",
                "commit",
                "-qm",
                "keep",
            ],
        );
        std::fs::write(f.checkout.join("precious.txt"), "modified work").unwrap();
        assert!(validate_removal(&f.checkout, true, false).is_err());
        assert_eq!(
            std::fs::read_to_string(f.checkout.join("precious.txt")).unwrap(),
            "modified work"
        );
    }
    #[test]
    fn principal_inside_managed_directory_is_still_protected() {
        let f = Fixture::new();
        assert!(validate_removal(&f.repo, true, false).is_err());
        assert!(f.repo.join(".git").is_dir());
    }
    #[cfg(unix)]
    #[test]
    fn symlink_cannot_make_an_external_checkout_managed() {
        let f = Fixture::new();
        let alias = f.managed.join("alias");
        std::os::unix::fs::symlink(&f.repo, &alias).unwrap();
        assert!(validate_removal(&alias, true, false).is_err());
        assert!(f.repo.is_dir());
    }
    #[test]
    fn ignored_user_files_are_not_silently_deleted() {
        let f = Fixture::new();
        std::fs::write(f.checkout.join(".gitignore"), "precious.local\n").unwrap();
        git(&f.checkout, &["add", ".gitignore"]);
        git(
            &f.checkout,
            &[
                "-c",
                "user.name=Test",
                "-c",
                "user.email=test@example.invalid",
                "commit",
                "-qm",
                "ignore",
            ],
        );
        std::fs::write(f.checkout.join("precious.local"), "local only").unwrap();
        assert!(validate_removal(&f.checkout, true, false).is_err());
        assert!(f.checkout.join("precious.local").exists());
    }

    #[test]
    fn detached_unique_commits_must_be_preserved_before_removal() {
        let f = Fixture::new();
        git(&f.checkout, &["checkout", "--detach"]);
        git(
            &f.checkout,
            &[
                "-c",
                "user.name=Test",
                "-c",
                "user.email=test@example.invalid",
                "commit",
                "--allow-empty",
                "-qm",
                "only reference",
            ],
        );
        assert!(validate_removal(&f.checkout, true, false).is_err());
    }
    #[test]
    fn checkout_actions_wait_until_the_previous_operation_releases_its_lock() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("checkout-actions.lock");
        let first = lock_checkout_actions_at(&path).unwrap();
        let (entered_tx, entered_rx) = std::sync::mpsc::channel();
        let worker = std::thread::spawn(move || {
            let _second = lock_checkout_actions_at(&path).unwrap();
            entered_tx.send(()).unwrap();
        });
        assert!(
            entered_rx
                .recv_timeout(std::time::Duration::from_millis(50))
                .is_err()
        );
        drop(first);
        entered_rx
            .recv_timeout(std::time::Duration::from_secs(2))
            .unwrap();
        worker.join().unwrap();
    }
}
