//! Checkout removal validates ownership independently from branch presentation.
use crate::git_command::run_git;
use std::path::{Path, PathBuf};

pub(crate) fn validate_removal(
    path: &Path,
    managed_root: &Path,
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
    let root = std::fs::canonicalize(managed_root)
        .map_err(|e| format!("Managed worktree root unavailable: {e}"))?;
    if target == root || !target.starts_with(&root) {
        return Err("Refusing to remove a checkout outside the managed worktree directory".into());
    }
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
    // SAFETY: the descriptor is owned and stays open for the guard's lifetime.
    if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX) } != 0 {
        return Err(std::io::Error::last_os_error().to_string());
    }
    Ok(CheckoutActionLock(file))
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
) -> Result<(), crate::WorkersError> {
    use crate::{CheckoutKind, CheckoutOwnership, WorkersError};
    let _checkout_action = lock_checkout_actions()?;
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
    let path = Path::new(&checkout.path);
    let common = run_git(
        path,
        &["rev-parse", "--path-format=absolute", "--git-common-dir"],
    )
    .map_err(WorkersError::State)?;
    let observed = checkout.observed_common_dir.as_deref().ok_or_else(|| {
        WorkersError::State("Checkout ownership has no Git identity evidence".into())
    })?;
    let current_common =
        std::fs::canonicalize(common.trim()).map_err(|e| WorkersError::State(e.to_string()))?;
    let expected_common =
        std::fs::canonicalize(observed).map_err(|e| WorkersError::State(e.to_string()))?;
    if current_common != expected_common {
        return Err(WorkersError::State(
            "Checkout Git identity changed; refusing removal".into(),
        ));
    }
    let active = |snapshot: &crate::WorkersBootstrap| {
        snapshot.sessions.iter().any(|session| {
            session.is_live()
                && snapshot.projects.iter().any(|project| {
                    project.id == session.project_id
                        && (project.id == project_id
                            || std::fs::canonicalize(&project.path).ok()
                                == std::fs::canonicalize(path).ok())
                })
        })
    };
    let target = validate_removal(
        path,
        &unpeel_core::app_paths::worktrees_root(),
        true,
        active(&client.bootstrap()?),
    )
    .map_err(WorkersError::State)?;
    set_removal_pending(project_id, &checkout, true)?;
    let result = (|| {
        if active(&client.bootstrap()?) {
            return Err(WorkersError::State(
                "A Worker became active; stop it before removing the checkout".into(),
            ));
        }
        validate_removal(
            &target,
            &unpeel_core::app_paths::worktrees_root(),
            true,
            false,
        )
        .map_err(WorkersError::State)?;
        // No --force, no branch deletion, no recursive filesystem fallback.
        crate::git_command::run_git_mutation(
            &target,
            &[
                "worktree",
                "remove",
                target.to_str().ok_or_else(|| {
                    WorkersError::State("Checkout path is not valid UTF-8".into())
                })?,
            ],
        )
        .map_err(|error| {
            let note = format!("Checkout removal did not complete: {error}. Session history is retained; inspect the checkout before restoring it.");
            let _ = mark_removal_interrupted(project_id, &note);
            WorkersError::State(note)
        })?;
        edit_archived(project_id, true)
    })();
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
        let target = validate_removal(&f.checkout, &f.managed, true, false).unwrap();
        assert_eq!(target, std::fs::canonicalize(&f.checkout).unwrap());
        assert!(f.checkout.is_dir());
        git(&f.repo, &["show-ref", "--verify", "refs/heads/feature"]);
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
            assert!(validate_removal(path, &f.managed, owned, active).is_err());
            assert!(path.exists());
        }
    }
    #[test]
    fn untracked_and_modified_files_block_checkout_removal() {
        let f = Fixture::new();
        std::fs::write(f.checkout.join("precious.txt"), "user work").unwrap();
        assert!(validate_removal(&f.checkout, &f.managed, true, false).is_err());
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
        assert!(validate_removal(&f.checkout, &f.managed, true, false).is_err());
        assert_eq!(
            std::fs::read_to_string(f.checkout.join("precious.txt")).unwrap(),
            "modified work"
        );
    }
    #[test]
    fn principal_inside_managed_directory_is_still_protected() {
        let f = Fixture::new();
        assert!(validate_removal(&f.repo, &f.root, true, false).is_err());
        assert!(f.repo.join(".git").is_dir());
    }
    #[cfg(unix)]
    #[test]
    fn symlink_cannot_make_an_external_checkout_managed() {
        let f = Fixture::new();
        let alias = f.managed.join("alias");
        std::os::unix::fs::symlink(&f.repo, &alias).unwrap();
        assert!(validate_removal(&alias, &f.managed, true, false).is_err());
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
        assert!(validate_removal(&f.checkout, &f.managed, true, false).is_err());
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
        assert!(validate_removal(&f.checkout, &f.managed, true, false).is_err());
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
