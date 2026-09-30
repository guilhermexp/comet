//! Checkout ↔ project links: which Space each Workers checkout belongs to.
//!
//! The link lives on `CheckoutIdentity::space_id` (design D1), so `comet-*`
//! ids referenced by sessions, manifests, order and sort modes never change.
//! A Git checkout belongs to the Space of its repository root; a non-Git
//! folder to its own Space. A record without filesystem or persisted Git
//! evidence stays association-pending: nothing is linked by name, prefix or
//! remote. Registry I/O always happens outside the app-state lock; links are
//! written under it and never overwrite an existing link.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde_json::json;

use crate::project_identity::{
    self, CheckoutAvailability, CheckoutIdentity, CheckoutKind, IdentityRegistry,
};
use crate::space_registry::{SpaceRegistry, canonical_project_path};

/// Top-level app-state key marking the one-time migration as done.
pub const MIGRATION_KEY: &str = "comet_space_migration";
/// Copy of `app-state.json` taken before the migration's first write.
pub const MIGRATION_BACKUP_FILE: &str = "app-state.space-migration-backup.json";
const MIGRATION_VERSION: u64 = 1;

/// The folder whose project owns `checkout`: the repository root for a Git
/// checkout, the folder itself for an available non-Git folder, `None` when
/// there is no evidence.
pub fn project_folder(checkout: &CheckoutIdentity, identity: &IdentityRegistry) -> Option<PathBuf> {
    if let Some(repository) = checkout
        .repository_id
        .as_deref()
        .and_then(|id| identity.repository(id))
    {
        if let Some(primary) = repository.primary_path.as_deref() {
            return Some(PathBuf::from(primary));
        }
        let common = Path::new(repository.common_dir.as_deref()?);
        return (common.file_name() == Some(".git".as_ref()))
            .then(|| common.parent().map(Path::to_path_buf))
            .flatten();
    }
    (checkout.kind == CheckoutKind::NonGit
        && checkout.availability == CheckoutAvailability::Available)
        .then(|| PathBuf::from(&checkout.path))
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LinkOutcome {
    /// `(checkout id, space id)` written by this pass.
    pub linked: Vec<(String, String)>,
    /// Checkout ids left association-pending (no evidence, or folder gone
    /// with no existing project for it).
    pub pending: Vec<String>,
}

/// Link every checkout that has no project yet, creating the local project of
/// its folder when absent.
pub fn link_unlinked_at(
    state_path: &Path,
    registry: &dyn SpaceRegistry,
) -> Result<LinkOutcome, String> {
    let identity = project_identity::read_registry_at(state_path)?;
    let unlinked = identity
        .checkouts
        .iter()
        .filter(|checkout| checkout.space_id.is_none())
        .collect::<Vec<_>>();
    if unlinked.is_empty() {
        return Ok(LinkOutcome::default());
    }
    let spaces = registry.list()?;
    let mut resolved: HashMap<PathBuf, Option<String>> = HashMap::new();
    let mut outcome = LinkOutcome::default();
    for checkout in unlinked {
        let Some(folder) = project_folder(checkout, &identity) else {
            outcome.pending.push(checkout.project_id.clone());
            continue;
        };
        let space_id = match resolved.get(&folder) {
            Some(space_id) => space_id.clone(),
            None => {
                let space_id = match canonical_project_path(&folder) {
                    Ok(canonical) => Some(registry.ensure(&canonical, None)?.id),
                    // A folder that is gone links only to a project that
                    // already exists for it; it never creates one.
                    Err(_) => spaces
                        .iter()
                        .find(|space| space.local && Path::new(&space.path) == folder)
                        .map(|space| space.id.clone()),
                };
                resolved.insert(folder, space_id.clone());
                space_id
            }
        };
        match space_id {
            Some(space_id) => outcome.linked.push((checkout.project_id.clone(), space_id)),
            None => outcome.pending.push(checkout.project_id.clone()),
        }
    }
    if !outcome.linked.is_empty() {
        let links = outcome.linked.clone();
        let written = unpeel_core::app_state::edit_at(state_path, move |state| {
            project_identity::link_spaces_in_state(state, &links)
        })?;
        outcome.linked.retain(|link| written.contains(&link.0));
    }
    Ok(outcome)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MigrationOutcome {
    /// The marker was already present: nothing read, created or written.
    AlreadyMigrated,
    Migrated(LinkOutcome),
}

/// One-time migration of the Workers state into the project registry
/// (design D4): backup, identity reconcile, link/ensure, marker.
pub fn migrate_at(
    state_path: &Path,
    registry: &dyn SpaceRegistry,
    now_unix_ms: u64,
) -> Result<MigrationOutcome, String> {
    let state = unpeel_core::app_state::load_for_edit_at(state_path)?;
    if state.get(MIGRATION_KEY).is_some() {
        return Ok(MigrationOutcome::AlreadyMigrated);
    }
    // An interrupted earlier attempt already holds the pre-migration copy;
    // never replace it with a partially migrated state.
    let backup = state_path.with_file_name(MIGRATION_BACKUP_FILE);
    if state_path.exists() && !backup.exists() {
        std::fs::copy(state_path, &backup)
            .map_err(|error| format!("{}: {error}", backup.display()))?;
    }
    project_identity::reconcile_at(state_path)?;
    let outcome = link_unlinked_at(state_path, registry)?;
    unpeel_core::app_state::edit_at(state_path, |state| {
        state.insert(
            MIGRATION_KEY.to_owned(),
            json!({ "version": MIGRATION_VERSION, "completed_at_unix_ms": now_unix_ms }),
        );
        Ok(())
    })?;
    Ok(MigrationOutcome::Migrated(outcome))
}

/// This device's Workers state file (`$UNPEEL_HOME/app-state.json`).
pub fn workers_state_path() -> PathBuf {
    unpeel_core::app_paths::app_state_path()
}

/// `(checkout path, space id)` for every linked checkout — the read-only view
/// Source Control authorizes Worker checkouts through. Never writes state.
pub fn linked_checkouts_at(state_path: &Path) -> Result<Vec<(PathBuf, String)>, String> {
    if !state_path.exists() {
        return Ok(Vec::new());
    }
    let identity = project_identity::read_registry_at(state_path)?;
    Ok(identity
        .checkouts
        .into_iter()
        .filter_map(|checkout| {
            let space_id = checkout.space_id?;
            let path = checkout.canonical_path.unwrap_or(checkout.path);
            Some((PathBuf::from(path), space_id))
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project_identity::CheckoutIdentity;
    use crate::space_registry::memory::{MemorySpaceRegistry, local_space};

    fn checkout(
        id: &str,
        path: &str,
        repository: Option<&str>,
        kind: CheckoutKind,
    ) -> CheckoutIdentity {
        serde_json::from_value(serde_json::json!({
            "projectID": id,
            "path": path,
            "repositoryID": repository,
            "kind": kind,
            "availability": "available"
        }))
        .unwrap()
    }

    #[test]
    fn a_checkout_belongs_to_its_repository_root_or_its_own_folder() {
        let identity: IdentityRegistry = serde_json::from_value(serde_json::json!({
            "repositories": [
                { "id": "repo-jk", "primaryPath": "/p/JK Distribuição" },
                { "id": "repo-hub", "commonDir": "/p/jk-erp-hub/.git" },
                { "id": "repo-bare", "commonDir": "/p/bare.git" }
            ]
        }))
        .unwrap();
        let folder = |checkout: &CheckoutIdentity| project_folder(checkout, &identity);
        assert_eq!(
            folder(&checkout(
                "a",
                "/p/.worktrees-jk/sec-cron",
                Some("repo-jk"),
                CheckoutKind::Linked
            )),
            Some(PathBuf::from("/p/JK Distribuição"))
        );
        assert_eq!(
            folder(&checkout(
                "b",
                "/p/.worktrees-hub/produto",
                Some("repo-hub"),
                CheckoutKind::Linked
            )),
            Some(PathBuf::from("/p/jk-erp-hub"))
        );
        assert_eq!(
            folder(&checkout(
                "c",
                "/x",
                Some("repo-bare"),
                CheckoutKind::Linked
            )),
            None
        );
        assert_eq!(
            folder(&checkout("d", "/notes", None, CheckoutKind::NonGit)),
            Some(PathBuf::from("/notes"))
        );
        assert_eq!(
            folder(&checkout("e", "/gone", None, CheckoutKind::Unresolved)),
            None
        );
    }

    #[test]
    fn linking_reuses_one_project_per_folder_and_needs_the_registry() {
        let dir = tempfile::tempdir().unwrap();
        let root = std::fs::canonicalize(dir.path()).unwrap();
        let notes = root.join("notes");
        std::fs::create_dir(&notes).unwrap();
        let state = root.join("app-state.json");
        std::fs::write(
            &state,
            serde_json::json!({
                "projects": [],
                crate::project_identity::IDENTITY_KEY: {
                    "version": 1,
                    "checkouts": [
                        { "projectID": "comet-notes", "path": notes, "kind": "non_git", "availability": "available" },
                        { "projectID": "comet-gone", "path": root.join("gone"), "kind": "unresolved", "availability": "missing" }
                    ]
                }
            })
            .to_string(),
        )
        .unwrap();

        let offline = MemorySpaceRegistry::unreachable();
        assert!(
            link_unlinked_at(&state, &offline).is_err(),
            "no registry, no link"
        );

        let registry = MemorySpaceRegistry::new(vec![local_space("space-notes", &notes)]);
        let outcome = link_unlinked_at(&state, &registry).unwrap();
        assert_eq!(
            outcome.linked,
            vec![("comet-notes".to_owned(), "space-notes".to_owned())]
        );
        assert_eq!(outcome.pending, vec!["comet-gone".to_owned()]);
        assert_eq!(registry.snapshot().len(), 1, "no Space created");
        let again = link_unlinked_at(&state, &registry).unwrap();
        assert!(again.linked.is_empty());
        assert_eq!(
            linked_checkouts_at(&state).unwrap(),
            vec![(notes, "space-notes".to_owned())]
        );
    }
}
