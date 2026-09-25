//! Device-local proof that Comet created a specific linked Git worktree.
//!
//! The worktree root is a location policy, not proof of ownership. Physical
//! removal must also match a journal entry and fresh Git/filesystem identity.
//! Callers must hold `checkout_lifecycle::lock_checkout_actions()` around all
//! journal operations; this module deliberately does not acquire that lock.

use std::ffi::OsStr;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::git_command::run_git;

const JOURNAL_VERSION: u32 = 1;
const JOURNAL_FILE: &str = "worktree-ownership.json";
const JOURNAL_PATH_OVERRIDE: &str = "ZERON_WORKTREE_OWNERSHIP_FILE";

/// Error from resolving a worktree path or reading/writing its ownership proof.
#[derive(Debug, thiserror::Error)]
pub enum OwnershipError {
    #[error("{0}")]
    Invalid(String),
    #[error("Failed to read worktree ownership journal {path}: {source}")]
    Read {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("Failed to write worktree ownership journal {path}: {source}")]
    Write {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("Worktree ownership journal is invalid: {0}")]
    Journal(String),
}

type Result<T> = std::result::Result<T, OwnershipError>;

/// Stage reached by one app-created checkout operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OwnershipStage {
    /// Persisted before `git worktree add`; it cannot prove ownership.
    PendingCreate,
    /// Git creation was observed and its identity persisted; safe to reconcile.
    CreatedObserved,
    /// A later fresh identity check matched the observation.
    Owned,
    /// A successful Git removal or stale-registration prune retired this
    /// identity; the same destination may be used for a new checkout.
    Retired,
    /// Git creation failed without leaving a checkout or registration.
    Aborted,
}

/// A path reserved for one creation operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreateReservation {
    pub operation_id: String,
    pub path: PathBuf,
}

/// Result of retrying interrupted `CreatedObserved` transitions.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ReconciliationReport {
    pub promoted_operation_ids: Vec<String>,
    pub unresolved_operation_ids: Vec<String>,
}

/// Comet's configured common worktree root. An empty override is treated as
/// unset. The returned path is absolute but need not exist yet.
pub fn common_worktrees_root() -> PathBuf {
    common_worktrees_root_from(
        &dirs::home_dir().unwrap_or_else(|| PathBuf::from(".")),
        std::env::var_os("ZERON_WORKTREES_DIR").as_deref(),
    )
}

/// Ensure and return the configured canonical worktree root. Mutating callers
/// should resolve this once and pass it through the creation operation.
pub fn canonical_root() -> Result<PathBuf> {
    let root = common_worktrees_root();
    fs::create_dir_all(&root).map_err(|source| OwnershipError::Write {
        path: root.clone(),
        source,
    })?;
    fs::canonicalize(&root).map_err(|source| OwnershipError::Write { path: root, source })
}

/// Pure form used by tests and callers with an explicit home/override.
pub fn common_worktrees_root_from(home: &Path, override_dir: Option<&OsStr>) -> PathBuf {
    let configured = override_dir
        .filter(|value| !value.to_string_lossy().trim().is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| home.join(".zeron").join("worktrees"));
    canonical_future_path(&configured).unwrap_or(configured)
}

/// Canonical repository-scoped directory beneath the common root.
///
/// Repository identity is derived from Git's common directory, so passing a
/// linked worktree for the same repository resolves to the same directory.
pub fn repository_worktrees_dir(root: &Path, repository: &Path) -> Result<PathBuf> {
    let identity = RepositoryIdentity::observe(repository)?;
    let root = canonical_future_path(root)?;
    let name = repository_directory_name(&identity.canonical_path);
    Ok(root.join(name))
}

/// Repository directory under the configured production root.
pub fn repository_dir(repository: &Path) -> Result<PathBuf> {
    repository_dir_under(&canonical_root()?, repository)
}

/// Repository directory under an explicit root, for isolated engine/tests.
pub fn repository_dir_under(root: &Path, repository: &Path) -> Result<PathBuf> {
    repository_worktrees_dir(root, repository)
}

/// Stable path used for a newly-created worktree. `name` is slugged as a path
/// component; branch names remain separately stored by Git and may contain `/`.
pub fn worktree_path(root: &Path, repository: &Path, name: &str) -> Result<PathBuf> {
    let slug = unpeel_core::worktrees::slug(name);
    if slug.is_empty() {
        return Err(OwnershipError::Invalid(
            "worktree name has no usable path characters".into(),
        ));
    }
    Ok(repository_worktrees_dir(root, repository)?.join(slug))
}

/// Durable journal for app-created worktrees.
///
/// The normal path is `~/.zeron/worktree-ownership.json`; non-empty
/// `ZERON_WORKTREE_OWNERSHIP_FILE` overrides the journal path for isolated
/// development/tests. `worktrees_root` is independently overridden by
/// `ZERON_WORKTREES_DIR`. Both files remain fail-closed on malformed state.
#[derive(Debug, Clone)]
pub struct OwnershipJournal {
    path: PathBuf,
    worktrees_root: PathBuf,
}

/// The device-local ownership journal shared by Chat and Workers.
pub fn current_journal_path() -> Result<PathBuf> {
    let home = dirs::home_dir()
        .ok_or_else(|| OwnershipError::Invalid("home directory is unavailable".into()))?;
    let configured_journal = std::env::var_os(JOURNAL_PATH_OVERRIDE)
        .filter(|value| !value.to_string_lossy().trim().is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| home.join(".zeron").join(JOURNAL_FILE));
    canonical_future_path(&configured_journal)
}

impl OwnershipJournal {
    /// Construct the production journal for this user and the configured root.
    pub fn for_current_user() -> Result<Self> {
        Ok(Self {
            path: current_journal_path()?,
            worktrees_root: common_worktrees_root(),
        })
    }

    /// Construct a journal with explicit paths. Callers must hold the shared
    /// checkout action lock while using it.
    pub fn at(path: PathBuf, worktrees_root: PathBuf) -> Self {
        Self {
            path,
            worktrees_root,
        }
    }

    /// Persist a `PendingCreate` record before invoking `git worktree add`.
    /// The parent directory is created and canonicalized first; the target must
    /// be a direct child of this repository's generated directory under the
    /// configured root and must not already exist.
    pub fn pending_create(
        &self,
        repository: &Path,
        path: &Path,
        operation_id: &str,
    ) -> Result<CreateReservation> {
        if operation_id.trim().is_empty() {
            return Err(OwnershipError::Invalid(
                "creation operation id must not be empty".into(),
            ));
        }
        let repository_identity = RepositoryIdentity::observe(repository)?;
        let root = fs::create_dir_all(&self.worktrees_root)
            .and_then(|()| fs::canonicalize(&self.worktrees_root))
            .map_err(|source| OwnershipError::Write {
                path: self.worktrees_root.clone(),
                source,
            })?;
        let repository_dir = root.join(repository_directory_name(
            &repository_identity.canonical_path,
        ));
        fs::create_dir_all(&repository_dir).map_err(|source| OwnershipError::Write {
            path: repository_dir.clone(),
            source,
        })?;
        // Reject a substituted/symlinked repo directory. This only constrains
        // where Comet creates; it is never used as ownership evidence.
        if fs::canonicalize(&repository_dir).ok().as_deref() != Some(repository_dir.as_path()) {
            return Err(OwnershipError::Invalid(
                "repository worktree directory resolves outside its canonical location".into(),
            ));
        }
        let target = canonical_future_child(path)?;
        if target.parent() != Some(repository_dir.as_path()) {
            return Err(OwnershipError::Invalid(format!(
                "worktree destination parent {} does not match its canonical repository directory {}",
                target
                    .parent()
                    .unwrap_or_else(|| Path::new("<none>"))
                    .display(),
                repository_dir.display()
            )));
        }
        match fs::symlink_metadata(&target) {
            Ok(_) => {
                return Err(OwnershipError::Invalid(format!(
                    "worktree destination already exists: {}",
                    target.display()
                )));
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err(OwnershipError::Invalid(format!(
                    "cannot inspect worktree destination {}: {error}",
                    target.display()
                )));
            }
        }
        let canonical_target = lexical_canonical_child(&repository_dir, &target)?;

        let mut document = self.read_document()?;
        // A previous removal may have succeeded while retiring its journal
        // record failed. The absent target and absent Git registration prove
        // that old ownership is no longer active; retain the old record as
        // history, then allow a fresh creation with a new identity.
        for record in document.records.iter_mut().filter(|record| {
            record.stage == OwnershipStage::Owned
                && record.target_path == path_string(&canonical_target)
        }) {
            if git_lists_checkout(
                Path::new(&record.repository.canonical_path),
                &record.target_path,
            )? {
                return Err(OwnershipError::Journal(
                    "previous checkout is still registered with Git".into(),
                ));
            }
            record.stage = OwnershipStage::Retired;
        }
        if document.records.iter().any(|record| {
            record.operation_id == operation_id
                || (record.target_path == path_string(&canonical_target)
                    && !matches!(
                        record.stage,
                        OwnershipStage::Retired | OwnershipStage::Aborted
                    ))
        }) {
            return Err(OwnershipError::Journal(
                "creation operation or destination already has an ownership record".into(),
            ));
        }
        document.records.push(OwnershipRecord {
            operation_id: operation_id.to_owned(),
            stage: OwnershipStage::PendingCreate,
            repository: repository_identity,
            target_path: path_string(&canonical_target),
            checkout: None,
            preparation_pending: true,
        });
        self.write_document(&document)?;
        Ok(CreateReservation {
            operation_id: operation_id.to_owned(),
            path: canonical_target,
        })
    }

    /// Record the exact Git identity observed after `git worktree add`.
    /// Failure leaves the durable `PendingCreate` entry in place; that stage
    /// is deliberately never promoted by automatic reconciliation.
    pub fn created_observed(
        &self,
        repository: &Path,
        path: &Path,
        operation_id: &str,
    ) -> Result<()> {
        let mut document = self.read_document()?;
        let index = record_index(&document, operation_id)?;
        let record = &document.records[index];
        let repository = RepositoryIdentity::observe(repository)?;
        let requested_path = fs::canonicalize(path).map_err(|error| {
            OwnershipError::Invalid(format!(
                "cannot canonicalize observed worktree path: {error}"
            ))
        })?;
        if record.repository != repository || record.target_path != path_string(&requested_path) {
            return Err(OwnershipError::Invalid(
                "create observation does not match the pending repository and path".into(),
            ));
        }
        if record.stage != OwnershipStage::PendingCreate || record.checkout.is_some() {
            return Err(OwnershipError::Journal(
                "only a pending create can record its first Git observation".into(),
            ));
        }
        let observed =
            CheckoutIdentity::observe(Path::new(&record.target_path), &record.repository)?;
        document.records[index].checkout = Some(observed);
        document.records[index].stage = OwnershipStage::CreatedObserved;
        self.write_document(&document)
    }

    /// Promote `CreatedObserved` only after a new probe still matches the
    /// persisted repository, checkout path, common Git directory and per-
    /// worktree administrative directory identity.
    pub fn finalize_owned(&self, repository: &Path, path: &Path, operation_id: &str) -> Result<()> {
        let mut document = self.read_document()?;
        let index = record_index(&document, operation_id)?;
        let record = &document.records[index];
        let repository = RepositoryIdentity::observe(repository)?;
        let requested_path = fs::canonicalize(path).map_err(|error| {
            OwnershipError::Invalid(format!("cannot canonicalize owned worktree path: {error}"))
        })?;
        if record.repository != repository || record.target_path != path_string(&requested_path) {
            return Err(OwnershipError::Invalid(
                "ownership finalization does not match the observed repository and path".into(),
            ));
        }
        if record.stage != OwnershipStage::CreatedObserved {
            return Err(OwnershipError::Journal(
                "only a created-observed checkout can become owned".into(),
            ));
        }
        let expected = record.checkout.as_ref().ok_or_else(|| {
            OwnershipError::Journal("created-observed record has no Git identity".into())
        })?;
        let observed =
            CheckoutIdentity::observe(Path::new(&record.target_path), &record.repository)?;
        if &observed != expected {
            return Err(OwnershipError::Invalid(
                "worktree Git identity changed before ownership was finalized".into(),
            ));
        }
        document.records[index].stage = OwnershipStage::Owned;
        self.write_document(&document)
    }

    /// Import a legacy Workers ownership assertion only when the old record
    /// explicitly says `AppManaged` and its recorded common directory still
    /// matches fresh Git evidence. This intentionally accepts old managed
    /// locations outside the new root; the old ownership marker and matching
    /// Git identity are the evidence, never the path prefix.
    pub fn migrate_legacy_worker(
        &self,
        repository: &Path,
        checkout: &Path,
        ownership: crate::project_identity::CheckoutOwnership,
        observed_common_dir: Option<&Path>,
    ) -> Result<bool> {
        if ownership != crate::project_identity::CheckoutOwnership::AppManaged {
            return Ok(false);
        }
        let Some(recorded_common_dir) = observed_common_dir else {
            return Ok(false);
        };
        let recorded_common_dir = match fs::canonicalize(recorded_common_dir) {
            Ok(path) => path,
            Err(_) => return Ok(false),
        };
        let repository = RepositoryIdentity::observe(repository)?;
        if path_string(&recorded_common_dir) != repository.common_dir {
            return Ok(false);
        }
        let checkout = match CheckoutIdentity::observe(checkout, &repository) {
            Ok(identity) => identity,
            Err(_) => return Ok(false),
        };
        let mut document = self.read_document()?;
        if let Some(existing) = document
            .records
            .iter()
            .rev()
            .find(|record| record.target_path == checkout.checkout_path)
        {
            return Ok(existing.stage == OwnershipStage::Owned
                && existing.repository == repository
                && existing.checkout.as_ref() == Some(&checkout));
        }
        let operation_id = format!("legacy-{}", uuid::Uuid::new_v4().simple());
        document.records.push(OwnershipRecord {
            operation_id,
            stage: OwnershipStage::Owned,
            repository,
            target_path: checkout.checkout_path.clone(),
            checkout: Some(checkout),
            // Legacy registrations predate the durable preparation marker, so
            // they cannot prove setup completed. Keep them pending until the
            // current setup flow confirms success.
            preparation_pending: true,
        });
        self.write_document(&document)?;
        Ok(true)
    }

    /// Reconcile only records that durably reached `CreatedObserved`.
    /// Mismatches remain unresolved and never gain ownership.
    pub fn reconcile_created_observed(&self) -> Result<ReconciliationReport> {
        let mut document = self.read_document()?;
        let mut report = ReconciliationReport::default();
        let mut changed = false;
        for index in 0..document.records.len() {
            if document.records[index].stage != OwnershipStage::CreatedObserved {
                continue;
            }
            let operation_id = document.records[index].operation_id.clone();
            let Some(expected) = document.records[index].checkout.clone() else {
                report.unresolved_operation_ids.push(operation_id);
                continue;
            };
            let target_path = document.records[index].target_path.clone();
            let repository = document.records[index].repository.clone();
            let matches = CheckoutIdentity::observe(Path::new(&target_path), &repository)
                .is_ok_and(|observed| observed == expected);
            if matches {
                document.records[index].stage = OwnershipStage::Owned;
                report.promoted_operation_ids.push(operation_id);
                changed = true;
            } else {
                report.unresolved_operation_ids.push(operation_id);
            }
        }
        if changed {
            self.write_document(&document)?;
        }
        Ok(report)
    }

    /// Verify physical-removal ownership against current Git evidence. A path
    /// under the managed root without an `Owned` journal match returns false.
    pub fn verify_owned(&self, repository: &Path, checkout: &Path) -> Result<bool> {
        // A crash after the observed Git identity was persisted may leave the
        // final Owned write unfinished. Callers hold CheckoutActionLock, so
        // completing only that proven transition is safe before this read.
        let repository = RepositoryIdentity::observe(repository)?;
        let observed = CheckoutIdentity::observe(checkout, &repository)?;
        self.reconcile_created_observed()?;
        let document = self.read_document()?;
        let target = path_string(Path::new(&observed.checkout_path));
        Ok(document.records.iter().any(|record| {
            record.stage == OwnershipStage::Owned
                && record.target_path == target
                && record.repository == repository
                && record.checkout.as_ref() == Some(&observed)
        }))
    }

    /// A missing checkout cannot be probed fresh. This only authorizes
    /// removing its stale Git administrative registration, never filesystem
    /// deletion. The caller must also recheck Git registration and absence.
    pub fn was_owned_missing(&self, repository: &Path, checkout: &Path) -> Result<bool> {
        let document = self.read_document()?;
        let repository = RepositoryIdentity::observe(repository)?;
        let target = path_string(&canonical_future_path(checkout)?);
        Ok(document.records.iter().any(|record| {
            record.stage == OwnershipStage::Owned
                && record.target_path == target
                && record.repository == repository
                && record.checkout.is_some()
        }))
    }

    /// Return whether a Comet-owned checkout still needs its blocking setup.
    /// An unrecorded external worktree has no creation setup pending; a
    /// recorded but unverified or changed checkout is an error and callers
    /// must not continue as though preparation had succeeded.
    pub fn preparation_pending(&self, repository: &Path, checkout: &Path) -> Result<bool> {
        let repository = RepositoryIdentity::observe(repository)?;
        let observed = CheckoutIdentity::observe(checkout, &repository)?;
        self.reconcile_created_observed()?;
        let document = self.read_document()?;
        let Some(record) = document.records.iter().find(|record| {
            record.target_path == observed.checkout_path
                && !matches!(
                    record.stage,
                    OwnershipStage::Retired | OwnershipStage::Aborted
                )
        }) else {
            return Ok(false);
        };
        if record.stage != OwnershipStage::Owned
            || record.repository != repository
            || record.checkout.as_ref() != Some(&observed)
        {
            return Err(OwnershipError::Invalid(
                "checkout preparation state does not match its owned Git identity".into(),
            ));
        }
        Ok(record.preparation_pending)
    }

    /// Clear the durable preparation marker only after all blocking setup for
    /// this exact owned checkout has succeeded. Repeated success reports are
    /// idempotent; changed Git identity is rejected.
    pub fn mark_prepared(&self, repository: &Path, checkout: &Path) -> Result<()> {
        let mut document = self.read_document()?;
        let repository = RepositoryIdentity::observe(repository)?;
        let observed = CheckoutIdentity::observe(checkout, &repository)?;
        let index = document
            .records
            .iter()
            .position(|record| {
                record.stage == OwnershipStage::Owned
                    && record.target_path == observed.checkout_path
                    && record.repository == repository
                    && record.checkout.as_ref() == Some(&observed)
            })
            .ok_or_else(|| OwnershipError::Journal("checkout has no ownership record".into()))?;
        let record = &document.records[index];
        if record.stage != OwnershipStage::Owned
            || record.repository != repository
            || record.checkout.as_ref() != Some(&observed)
        {
            return Err(OwnershipError::Invalid(
                "cannot mark preparation complete for a changed or unowned checkout".into(),
            ));
        }
        if record.preparation_pending {
            document.records[index].preparation_pending = false;
            self.write_document(&document)?;
        }
        Ok(())
    }

    /// Retire ownership only after Git has dropped the linked checkout. This
    /// keeps an audit record while allowing a later creation at the same path.
    pub fn retire_removed(&self, repository: &Path, checkout: &Path) -> Result<()> {
        let mut document = self.read_document()?;
        let repository = RepositoryIdentity::observe(repository)?;
        let target = path_string(&canonical_future_path(checkout)?);
        if !path_is_definitely_absent(checkout)?
            || git_lists_checkout(Path::new(&repository.canonical_path), &target)?
        {
            return Err(OwnershipError::Invalid(
                "checkout still exists or is registered with Git".into(),
            ));
        }
        let index = document
            .records
            .iter()
            .rposition(|record| {
                record.stage == OwnershipStage::Owned
                    && record.target_path == target
                    && record.repository == repository
            })
            .ok_or_else(|| {
                OwnershipError::Journal("checkout has no active ownership record".into())
            })?;
        document.records[index].stage = OwnershipStage::Retired;
        self.write_document(&document)
    }

    /// Release a failed creation reservation only when Git left no checkout
    /// and no administrative registration. Ambiguous partial creation stays
    /// PendingCreate and requires explicit recovery.
    pub fn abort_absent_create(
        &self,
        repository: &Path,
        checkout: &Path,
        operation_id: &str,
    ) -> Result<()> {
        let mut document = self.read_document()?;
        let index = record_index(&document, operation_id)?;
        let repository = RepositoryIdentity::observe(repository)?;
        let target = path_string(&canonical_future_path(checkout)?);
        let record = &document.records[index];
        if record.stage != OwnershipStage::PendingCreate
            || record.repository != repository
            || record.target_path != target
            || !path_is_definitely_absent(checkout)?
            || git_lists_checkout(Path::new(&repository.canonical_path), &target)?
        {
            return Err(OwnershipError::Invalid(
                "failed creation left ambiguous checkout state".into(),
            ));
        }
        document.records[index].stage = OwnershipStage::Aborted;
        self.write_document(&document)
    }

    /// Read a stage for diagnostics/tests. Missing operations return `None`.
    pub fn stage(&self, operation_id: &str) -> Result<Option<OwnershipStage>> {
        let document = self.read_document()?;
        Ok(document
            .records
            .iter()
            .find(|record| record.operation_id == operation_id)
            .map(|record| record.stage))
    }

    fn read_document(&self) -> Result<OwnershipDocument> {
        let metadata = match fs::symlink_metadata(&self.path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(OwnershipDocument::default());
            }
            Err(source) => {
                return Err(OwnershipError::Read {
                    path: self.path.clone(),
                    source,
                });
            }
        };
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            return Err(OwnershipError::Journal(
                "journal path must be a regular file, not a symlink".into(),
            ));
        }
        let mut bytes = Vec::new();
        File::open(&self.path)
            .and_then(|mut file| file.read_to_end(&mut bytes))
            .map_err(|source| OwnershipError::Read {
                path: self.path.clone(),
                source,
            })?;
        let document: OwnershipDocument = serde_json::from_slice(&bytes)
            .map_err(|error| OwnershipError::Journal(error.to_string()))?;
        if document.version != JOURNAL_VERSION {
            return Err(OwnershipError::Journal(format!(
                "unsupported journal version {}",
                document.version
            )));
        }
        validate_document(&document)?;
        Ok(document)
    }

    fn write_document(&self, document: &OwnershipDocument) -> Result<()> {
        let parent = self.path.parent().ok_or_else(|| {
            OwnershipError::Invalid("ownership journal path has no parent".into())
        })?;
        fs::create_dir_all(parent).map_err(|source| OwnershipError::Write {
            path: parent.to_path_buf(),
            source,
        })?;
        if fs::symlink_metadata(&self.path).is_ok_and(|metadata| metadata.file_type().is_symlink())
        {
            return Err(OwnershipError::Journal(
                "journal path must not be a symlink".into(),
            ));
        }
        let mut bytes = serde_json::to_vec_pretty(document)
            .map_err(|error| OwnershipError::Journal(error.to_string()))?;
        bytes.push(b'\n');
        let temporary = temporary_path(&self.path);
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let result = (|| {
            let mut file = options.open(&temporary)?;
            file.write_all(&bytes)?;
            file.sync_all()?;
            fs::rename(&temporary, &self.path)?;
            File::open(parent)?.sync_all()?;
            Ok::<(), std::io::Error>(())
        })();
        if result.is_err() {
            let _ = fs::remove_file(&temporary);
        }
        result.map_err(|source| OwnershipError::Write {
            path: self.path.clone(),
            source,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct OwnershipDocument {
    version: u32,
    records: Vec<OwnershipRecord>,
}

fn validate_document(document: &OwnershipDocument) -> Result<()> {
    let mut operation_ids = std::collections::HashSet::new();
    let mut paths = std::collections::HashSet::new();
    for record in &document.records {
        if record.operation_id.trim().is_empty()
            || record.repository.canonical_path.is_empty()
            || record.repository.common_dir.is_empty()
            || record.repository.common_dir_identity.is_empty()
            || record.target_path.is_empty()
        {
            return Err(OwnershipError::Journal(
                "record is missing required ownership identity".into(),
            ));
        }
        let active_path = !matches!(
            record.stage,
            OwnershipStage::Retired | OwnershipStage::Aborted
        );
        if !operation_ids.insert(&record.operation_id)
            || (active_path && !paths.insert(&record.target_path))
        {
            return Err(OwnershipError::Journal(
                "journal contains duplicate operation ids or checkout paths".into(),
            ));
        }
        match (record.stage, record.checkout.as_ref()) {
            (OwnershipStage::PendingCreate | OwnershipStage::Aborted, None) => {}
            (
                OwnershipStage::CreatedObserved | OwnershipStage::Owned | OwnershipStage::Retired,
                Some(checkout),
            ) if checkout.checkout_path == record.target_path
                && checkout.repository_path == record.repository.canonical_path
                && checkout.common_dir == record.repository.common_dir
                && checkout.common_dir_identity == record.repository.common_dir_identity
                && !checkout.worktree_id.is_empty()
                && !checkout.admin_dir.is_empty() => {}
            _ => {
                return Err(OwnershipError::Journal(
                    "record stage and observed Git identity are inconsistent".into(),
                ));
            }
        }
    }
    Ok(())
}

impl Default for OwnershipDocument {
    fn default() -> Self {
        Self {
            version: JOURNAL_VERSION,
            records: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct OwnershipRecord {
    operation_id: String,
    stage: OwnershipStage,
    repository: RepositoryIdentity,
    target_path: String,
    checkout: Option<CheckoutIdentity>,
    #[serde(default = "default_preparation_pending")]
    preparation_pending: bool,
}

fn default_preparation_pending() -> bool {
    // Older journal entries cannot prove that setup completed. Keeping them
    // pending is safe: a retry may repeat setup but cannot skip a required one.
    true
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RepositoryIdentity {
    canonical_path: String,
    common_dir: String,
    common_dir_identity: String,
}

impl RepositoryIdentity {
    fn observe(path: &Path) -> Result<Self> {
        let top =
            run_git(path, &["rev-parse", "--show-toplevel"]).map_err(OwnershipError::Invalid)?;
        let top = fs::canonicalize(top.trim()).map_err(|error| {
            OwnershipError::Invalid(format!("cannot canonicalize repository root: {error}"))
        })?;
        let observation = crate::project_identity::probe_checkout(&top);
        let common_dir = observation.common_dir.ok_or_else(|| {
            OwnershipError::Invalid("Git common directory could not be identified".into())
        })?;
        let common_dir = fs::canonicalize(common_dir).map_err(|error| {
            OwnershipError::Invalid(format!("cannot canonicalize Git common directory: {error}"))
        })?;
        let common_dir_identity = observation
            .common_dir_stable_fingerprint
            .or(observation.common_dir_fingerprint)
            .ok_or_else(|| {
                OwnershipError::Invalid("Git common directory identity is unavailable".into())
            })?;
        let canonical_path = observation
            .main_repo
            .and_then(|path| fs::canonicalize(path).ok())
            .unwrap_or_else(|| {
                if common_dir.file_name() == Some(OsStr::new(".git")) {
                    common_dir
                        .parent()
                        .map(Path::to_path_buf)
                        .unwrap_or_else(|| top.clone())
                } else {
                    top
                }
            });
        Ok(Self {
            canonical_path: path_string(&canonical_path),
            common_dir: path_string(&common_dir),
            common_dir_identity,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CheckoutIdentity {
    checkout_path: String,
    repository_path: String,
    common_dir: String,
    common_dir_identity: String,
    worktree_id: String,
    admin_dir: String,
    admin_dir_inode: u64,
}

impl CheckoutIdentity {
    fn observe(path: &Path, expected_repository: &RepositoryIdentity) -> Result<Self> {
        let checkout_path = fs::canonicalize(path).map_err(|error| {
            OwnershipError::Invalid(format!("worktree is unavailable: {error}"))
        })?;
        let top = run_git(&checkout_path, &["rev-parse", "--show-toplevel"])
            .map_err(OwnershipError::Invalid)?;
        let top = fs::canonicalize(top.trim()).map_err(|error| {
            OwnershipError::Invalid(format!("cannot canonicalize worktree root: {error}"))
        })?;
        if top != checkout_path {
            return Err(OwnershipError::Invalid(
                "ownership target is not the worktree root".into(),
            ));
        }
        let observation = crate::project_identity::probe_checkout(&checkout_path);
        if observation.kind != crate::project_identity::CheckoutKind::Linked {
            return Err(OwnershipError::Invalid(
                "ownership target is not a linked Git worktree".into(),
            ));
        }
        let common_dir = observation.common_dir.ok_or_else(|| {
            OwnershipError::Invalid("worktree Git common directory is unavailable".into())
        })?;
        let common_dir = fs::canonicalize(common_dir).map_err(|error| {
            OwnershipError::Invalid(format!("cannot canonicalize Git common directory: {error}"))
        })?;
        let common_dir_identity = observation
            .common_dir_stable_fingerprint
            .or(observation.common_dir_fingerprint)
            .ok_or_else(|| {
                OwnershipError::Invalid("Git common directory identity is unavailable".into())
            })?;
        if path_string(&common_dir) != expected_repository.common_dir
            || common_dir_identity != expected_repository.common_dir_identity
        {
            return Err(OwnershipError::Invalid(
                "worktree belongs to a different or replaced Git common directory".into(),
            ));
        }
        let repository_path = observation
            .main_repo
            .and_then(|path| fs::canonicalize(path).ok())
            .unwrap_or_else(|| expected_repository.canonical_path.clone().into());
        if path_string(&repository_path) != expected_repository.canonical_path {
            return Err(OwnershipError::Invalid(
                "worktree belongs to a different canonical repository".into(),
            ));
        }

        let admin = run_git(&checkout_path, &["rev-parse", "--absolute-git-dir"])
            .map_err(OwnershipError::Invalid)?;
        let admin_dir = fs::canonicalize(admin.trim()).map_err(|error| {
            OwnershipError::Invalid(format!(
                "cannot canonicalize worktree Git directory: {error}"
            ))
        })?;
        let relative = admin_dir.strip_prefix(&common_dir).map_err(|_| {
            OwnershipError::Invalid("worktree Git directory is outside its common directory".into())
        })?;
        let components = relative.components().collect::<Vec<_>>();
        if components.len() != 2
            || components[0] != Component::Normal(OsStr::new("worktrees"))
            || !matches!(components[1], Component::Normal(_))
        {
            return Err(OwnershipError::Invalid(
                "worktree administrative directory has an unexpected location".into(),
            ));
        }
        let worktree_id = components[1]
            .as_os_str()
            .to_str()
            .ok_or_else(|| OwnershipError::Invalid("worktree id is not UTF-8".into()))?
            .to_owned();
        let metadata = fs::metadata(&admin_dir).map_err(|error| {
            OwnershipError::Invalid(format!("cannot inspect worktree Git directory: {error}"))
        })?;
        let admin_dir_inode = inode(&metadata).ok_or_else(|| {
            OwnershipError::Invalid("worktree Git directory identity is unavailable".into())
        })?;
        Ok(Self {
            checkout_path: path_string(&checkout_path),
            repository_path: path_string(&repository_path),
            common_dir: path_string(&common_dir),
            common_dir_identity,
            worktree_id,
            admin_dir: path_string(&admin_dir),
            admin_dir_inode,
        })
    }
}

fn record_index(document: &OwnershipDocument, operation_id: &str) -> Result<usize> {
    document
        .records
        .iter()
        .position(|record| record.operation_id == operation_id)
        .ok_or_else(|| OwnershipError::Journal("unknown creation operation".into()))
}

fn repository_directory_name(canonical_repository: &str) -> String {
    let name = Path::new(canonical_repository)
        .file_name()
        .map(|name| unpeel_core::worktrees::slug(&name.to_string_lossy()))
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| "repo".into());
    format!("{name}-{:016x}", fnv1a(canonical_repository.as_bytes()))
}

fn fnv1a(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

fn absolute_path(path: &Path) -> Result<PathBuf> {
    if path.is_absolute() {
        Ok(path.to_path_buf())
    } else {
        std::env::current_dir()
            .map(|cwd| cwd.join(path))
            .map_err(|error| OwnershipError::Invalid(format!("cannot resolve path: {error}")))
    }
}

/// Canonicalize a configured future path through its closest existing parent.
/// This also resolves macOS `/var` → `/private/var` aliases when the root has
/// not been created yet.
fn canonical_future_path(path: &Path) -> Result<PathBuf> {
    let absolute = absolute_path(path)?;
    match fs::symlink_metadata(&absolute) {
        Ok(_) => fs::canonicalize(&absolute).map_err(|error| {
            OwnershipError::Invalid(format!(
                "cannot canonicalize configured worktree path {}: {error}",
                absolute.display()
            ))
        }),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            let parent = absolute.parent().ok_or_else(|| {
                OwnershipError::Invalid("configured worktree path has no parent".into())
            })?;
            let name = absolute.file_name().ok_or_else(|| {
                OwnershipError::Invalid("configured worktree path has no final component".into())
            })?;
            Ok(canonical_future_path(parent)?.join(name))
        }
        Err(error) => Err(OwnershipError::Invalid(format!(
            "cannot inspect configured worktree path {}: {error}",
            absolute.display()
        ))),
    }
}

/// Canonicalize the existing parent and append a not-yet-created checkout name.
fn canonical_future_child(path: &Path) -> Result<PathBuf> {
    let absolute = absolute_path(path)?;
    let parent = absolute
        .parent()
        .ok_or_else(|| OwnershipError::Invalid("worktree target has no parent directory".into()))?;
    let name = absolute
        .file_name()
        .ok_or_else(|| OwnershipError::Invalid("worktree target has no final component".into()))?;
    Ok(fs::canonicalize(parent)
        .map_err(|error| {
            OwnershipError::Invalid(format!("cannot canonicalize worktree parent: {error}"))
        })?
        .join(name))
}

fn lexical_canonical_child(parent: &Path, child: &Path) -> Result<PathBuf> {
    let parent = fs::canonicalize(parent).map_err(|error| {
        OwnershipError::Invalid(format!("cannot canonicalize worktree parent: {error}"))
    })?;
    let name = child
        .file_name()
        .ok_or_else(|| OwnershipError::Invalid("worktree target has no name".into()))?;
    Ok(parent.join(name))
}

fn path_string(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

fn git_lists_checkout(repository: &Path, target: &str) -> Result<bool> {
    let listing = run_git(repository, &["worktree", "list", "--porcelain", "-z"])
        .map_err(OwnershipError::Invalid)?;
    Ok(listing
        .split('\0')
        .filter_map(|part| part.strip_prefix("worktree "))
        .any(|path| path == target))
}

fn path_is_definitely_absent(path: &Path) -> Result<bool> {
    match fs::symlink_metadata(path) {
        Ok(_) => Ok(false),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(true),
        Err(error) => Err(OwnershipError::Invalid(format!(
            "cannot inspect checkout path {}: {error}",
            path.display()
        ))),
    }
}

fn temporary_path(path: &Path) -> PathBuf {
    let name = path
        .file_name()
        .and_then(OsStr::to_str)
        .unwrap_or("worktree-ownership");
    path.with_file_name(format!(".{name}.{}.tmp", uuid::Uuid::new_v4().simple()))
}

#[cfg(unix)]
fn inode(metadata: &fs::Metadata) -> Option<u64> {
    use std::os::unix::fs::MetadataExt;
    Some(metadata.ino())
}

#[cfg(not(unix))]
fn inode(_: &fs::Metadata) -> Option<u64> {
    // The current Git identity probe has no stable common-directory identity
    // on non-Unix platforms, so removal remains fail-closed there.
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;

    fn git(cwd: &Path, args: &[&str]) {
        let output = Command::new("git")
            .arg("-C")
            .arg(cwd)
            .args(args)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "git {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr)
        );
    }

    fn repo(path: &Path) {
        fs::create_dir_all(path).unwrap();
        git(path, &["init", "-b", "main"]);
        git(path, &["config", "user.name", "Comet Test"]);
        git(path, &["config", "user.email", "comet@example.invalid"]);
        fs::write(path.join("README.md"), "initial\n").unwrap();
        git(path, &["add", "README.md"]);
        git(path, &["commit", "-m", "initial"]);
    }

    fn worktree(repo: &Path, path: &Path, branch: &str) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        let output = Command::new("git")
            .arg("-C")
            .arg(repo)
            .args(["worktree", "add", "-b", branch])
            .arg(path)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "git worktree add failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    #[test]
    fn same_root_and_homonymous_repositories_get_distinct_repository_directories() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("shared-worktrees");
        let first = temp.path().join("one").join("comet");
        let second = temp.path().join("two").join("comet");
        repo(&first);
        repo(&second);

        let first_dir = repository_worktrees_dir(&root, &first).unwrap();
        let second_dir = repository_worktrees_dir(&root, &second).unwrap();
        let canonical_root = fs::canonicalize(&root)
            .unwrap_or_else(|_| canonical_future_path(&root).expect("canonical future root"));
        assert_eq!(first_dir.parent(), Some(canonical_root.as_path()));
        assert_eq!(second_dir.parent(), Some(canonical_root.as_path()));
        assert_ne!(first_dir, second_dir);

        let configured =
            common_worktrees_root_from(temp.path(), Some(OsStr::new("/tmp/comet-worktrees-test")));
        assert_eq!(
            configured,
            canonical_future_path(Path::new("/tmp/comet-worktrees-test")).unwrap()
        );
        assert_eq!(
            common_worktrees_root_from(temp.path(), Some(OsStr::new("  "))),
            canonical_future_path(&temp.path().join(".zeron/worktrees")).unwrap()
        );

        let first_worktree = temp.path().join("external-checkout");
        worktree(&first, &first_worktree, "feature/same-repo");
        assert_eq!(
            repository_worktrees_dir(&root, &first_worktree).unwrap(),
            first_dir
        );
    }

    #[test]
    fn removed_checkout_can_be_recreated_at_the_same_path_with_fresh_proof() {
        let temp = tempfile::tempdir().unwrap();
        let repository = temp.path().join("repo");
        let root = temp.path().join("worktrees");
        repo(&repository);
        let target = worktree_path(&root, &repository, "feature").unwrap();
        let journal = OwnershipJournal::at(temp.path().join("ownership.json"), root);
        let first = journal
            .pending_create(&repository, &target, "first")
            .unwrap();
        worktree(&repository, &first.path, "feature/first");
        journal
            .created_observed(&repository, &first.path, "first")
            .unwrap();
        journal
            .finalize_owned(&repository, &first.path, "first")
            .unwrap();
        git(
            &repository,
            &["worktree", "remove", first.path.to_str().unwrap()],
        );
        journal.retire_removed(&repository, &first.path).unwrap();
        assert_eq!(
            journal.stage("first").unwrap(),
            Some(OwnershipStage::Retired)
        );

        let second = journal
            .pending_create(&repository, &target, "second")
            .unwrap();
        worktree(&repository, &second.path, "feature/second");
        journal
            .created_observed(&repository, &second.path, "second")
            .unwrap();
        journal
            .finalize_owned(&repository, &second.path, "second")
            .unwrap();
        assert!(journal.verify_owned(&repository, &second.path).unwrap());
        assert!(
            journal
                .preparation_pending(&repository, &second.path)
                .unwrap()
        );
        journal.mark_prepared(&repository, &second.path).unwrap();
        assert!(
            !journal
                .preparation_pending(&repository, &second.path)
                .unwrap()
        );
        let common_dir = RepositoryIdentity::observe(&repository).unwrap().common_dir;
        assert!(
            journal
                .migrate_legacy_worker(
                    &repository,
                    &second.path,
                    crate::project_identity::CheckoutOwnership::AppManaged,
                    Some(Path::new(&common_dir)),
                )
                .unwrap()
        );
        assert_eq!(
            journal.stage("second").unwrap(),
            Some(OwnershipStage::Owned)
        );
    }

    #[test]
    fn proven_interrupted_creation_reconciles_before_preparation() {
        let temp = tempfile::tempdir().unwrap();
        let repository = temp.path().join("repo");
        let root = temp.path().join("worktrees");
        repo(&repository);
        let target = worktree_path(&root, &repository, "interrupted").unwrap();
        let journal = OwnershipJournal::at(temp.path().join("ownership.json"), root);
        journal
            .pending_create(&repository, &target, "interrupted")
            .unwrap();
        worktree(&repository, &target, "feature/interrupted");
        journal
            .created_observed(&repository, &target, "interrupted")
            .unwrap();
        assert_eq!(
            journal.stage("interrupted").unwrap(),
            Some(OwnershipStage::CreatedObserved)
        );
        assert!(journal.preparation_pending(&repository, &target).unwrap());
        assert_eq!(
            journal.stage("interrupted").unwrap(),
            Some(OwnershipStage::Owned)
        );
    }

    #[test]
    fn failed_add_with_no_checkout_releases_its_reservation() {
        let temp = tempfile::tempdir().unwrap();
        let repository = temp.path().join("repo");
        let root = temp.path().join("worktrees");
        repo(&repository);
        let target = worktree_path(&root, &repository, "feature").unwrap();
        let journal = OwnershipJournal::at(temp.path().join("ownership.json"), root);
        journal
            .pending_create(&repository, &target, "failed")
            .unwrap();
        journal
            .abort_absent_create(&repository, &target, "failed")
            .unwrap();
        assert_eq!(
            journal.stage("failed").unwrap(),
            Some(OwnershipStage::Aborted)
        );
        journal
            .pending_create(&repository, &target, "retry")
            .unwrap();
    }

    #[test]
    fn only_created_observed_identity_can_be_reconciled_and_branch_rename_keeps_proof() {
        let temp = tempfile::tempdir().unwrap();
        let repository = temp.path().join("source").join("comet");
        let root = temp.path().join("common-worktrees");
        repo(&repository);
        let target = worktree_path(&root, &repository, "feature/sidebar").unwrap();
        let journal = OwnershipJournal::at(temp.path().join("journal.json"), root.clone());

        // A PendingCreate with a checkout on disk is deliberately not enough:
        // this models failure to persist CreatedObserved after `git worktree add`.
        let pending_path = worktree_path(&journal.worktrees_root, &repository, "pending").unwrap();
        let pending = journal
            .pending_create(&repository, &pending_path, "op-pending")
            .unwrap();
        worktree(&repository, &pending.path, "feature/pending");
        assert!(
            journal
                .reconcile_created_observed()
                .unwrap()
                .promoted_operation_ids
                .is_empty()
        );
        assert_eq!(
            journal.stage("op-pending").unwrap(),
            Some(OwnershipStage::PendingCreate)
        );
        assert!(!journal.verify_owned(&repository, &pending_path).unwrap());
        assert!(
            pending.path.exists(),
            "failure recovery never deletes the checkout"
        );

        let reservation = journal
            .pending_create(&repository, &target, "op-owned")
            .unwrap();
        assert_eq!(reservation.path, target);
        worktree(&repository, &reservation.path, "feature/sidebar");
        journal
            .created_observed(&repository, &reservation.path, "op-owned")
            .unwrap();
        assert_eq!(
            journal.stage("op-owned").unwrap(),
            Some(OwnershipStage::CreatedObserved)
        );

        // Reopening the journal models a process restart between observation
        // and Owned. A fresh matching Git identity permits recovery.
        let restarted = OwnershipJournal::at(journal.path.clone(), journal.worktrees_root.clone());
        let report = restarted.reconcile_created_observed().unwrap();
        assert_eq!(report.promoted_operation_ids, ["op-owned"]);
        assert_eq!(
            restarted.stage("op-owned").unwrap(),
            Some(OwnershipStage::Owned)
        );
        assert!(
            restarted
                .verify_owned(&repository, &reservation.path)
                .unwrap()
        );

        git(
            &reservation.path,
            &["branch", "-m", "feature/sidebar-renamed"],
        );
        assert!(
            restarted
                .verify_owned(&repository, &reservation.path)
                .unwrap()
        );
    }

    #[test]
    fn preparation_marker_survives_restart_until_setup_success_is_recorded() {
        let temp = tempfile::tempdir().unwrap();
        let repository = temp.path().join("source").join("comet");
        let root = temp.path().join("common-worktrees");
        repo(&repository);
        let journal = OwnershipJournal::at(temp.path().join("journal.json"), root.clone());
        let target = worktree_path(&root, &repository, "feature/setup").unwrap();
        let reservation = journal
            .pending_create(&repository, &target, "op-setup")
            .unwrap();
        worktree(&repository, &reservation.path, "feature/setup");
        journal
            .created_observed(&repository, &reservation.path, "op-setup")
            .unwrap();
        journal
            .finalize_owned(&repository, &reservation.path, "op-setup")
            .unwrap();
        assert!(
            journal
                .preparation_pending(&repository, &reservation.path)
                .unwrap()
        );

        let restarted = OwnershipJournal::at(journal.path.clone(), root);
        assert!(
            restarted
                .preparation_pending(&repository, &reservation.path)
                .unwrap()
        );
        restarted
            .mark_prepared(&repository, &reservation.path)
            .unwrap();
        assert!(
            !restarted
                .preparation_pending(&repository, &reservation.path)
                .unwrap()
        );

        // A retried success acknowledgement is harmless and will not cause a
        // later relaunch to repeat setup.
        restarted
            .mark_prepared(&repository, &reservation.path)
            .unwrap();
        assert!(
            !journal
                .preparation_pending(&repository, &reservation.path)
                .unwrap()
        );
    }

    #[test]
    fn foreign_repository_and_external_checkout_do_not_gain_ownership() {
        let temp = tempfile::tempdir().unwrap();
        let repository = temp.path().join("source").join("comet");
        let foreign = temp.path().join("foreign").join("comet");
        let root = temp.path().join("worktrees");
        repo(&repository);
        repo(&foreign);
        let target = worktree_path(&root, &repository, "feature").unwrap();
        let journal = OwnershipJournal::at(temp.path().join("journal.json"), root.clone());
        let reservation = journal.pending_create(&repository, &target, "op").unwrap();
        worktree(&repository, &reservation.path, "feature");
        journal
            .created_observed(&repository, &reservation.path, "op")
            .unwrap();

        assert!(journal.verify_owned(&foreign, &reservation.path).is_err());
        assert_eq!(
            journal
                .reconcile_created_observed()
                .unwrap()
                .promoted_operation_ids,
            ["op"]
        );

        // The root alone is not proof: a different manually-created worktree
        // under the same root is not present in the journal.
        let external = worktree_path(&root, &repository, "external").unwrap();
        worktree(&repository, &external, "other/external");
        assert!(!journal.verify_owned(&repository, &external).unwrap());
        assert!(!journal.preparation_pending(&repository, &external).unwrap());
        assert!(reservation.path.exists());
        assert!(
            journal
                .verify_owned(&repository, &reservation.path)
                .unwrap()
        );
    }

    #[test]
    fn malformed_journal_fails_closed_instead_of_becoming_empty() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("journal.json");
        fs::write(&path, "{ broken").unwrap();
        let journal = OwnershipJournal::at(path, temp.path().join("worktrees"));
        assert!(journal.reconcile_created_observed().is_err());
    }

    #[test]
    fn legacy_worker_migration_requires_app_managed_marker_and_matching_common_dir() {
        use crate::project_identity::CheckoutOwnership;

        let temp = tempfile::tempdir().unwrap();
        let repository = temp.path().join("source").join("comet");
        let foreign = temp.path().join("foreign").join("comet");
        let legacy_target = temp
            .path()
            .join("legacy-unpeel")
            .join("worktrees")
            .join("comet")
            .join("feature");
        repo(&repository);
        repo(&foreign);
        worktree(&repository, &legacy_target, "feature/legacy");

        let journal = OwnershipJournal::at(
            temp.path().join("journal.json"),
            temp.path().join("new-zeron-root"),
        );
        let common_dir = RepositoryIdentity::observe(&repository).unwrap().common_dir;
        let common_dir = Path::new(&common_dir);
        assert!(
            journal
                .migrate_legacy_worker(
                    &repository,
                    &legacy_target,
                    CheckoutOwnership::AppManaged,
                    Some(common_dir),
                )
                .unwrap()
        );
        assert!(journal.verify_owned(&repository, &legacy_target).unwrap());
        assert!(
            journal
                .preparation_pending(&repository, &legacy_target)
                .unwrap()
        );

        // A repeated import is idempotent and an explicit External marker can
        // never use the same Git evidence to gain ownership.
        assert!(
            journal
                .migrate_legacy_worker(
                    &repository,
                    &legacy_target,
                    CheckoutOwnership::AppManaged,
                    Some(common_dir),
                )
                .unwrap()
        );
        assert!(
            !journal
                .migrate_legacy_worker(
                    &repository,
                    &legacy_target,
                    CheckoutOwnership::External,
                    Some(common_dir),
                )
                .unwrap()
        );

        let foreign_common = RepositoryIdentity::observe(&foreign).unwrap().common_dir;
        assert!(
            !journal
                .migrate_legacy_worker(
                    &repository,
                    &legacy_target,
                    CheckoutOwnership::AppManaged,
                    Some(Path::new(&foreign_common)),
                )
                .unwrap()
        );
        assert!(
            !journal
                .migrate_legacy_worker(
                    &repository,
                    &legacy_target,
                    CheckoutOwnership::AppManaged,
                    None,
                )
                .unwrap()
        );
        assert!(legacy_target.exists());
    }

    #[cfg(unix)]
    #[test]
    fn journal_write_failures_preserve_the_checkout_and_never_infer_ownership() {
        use std::os::unix::fs::PermissionsExt;

        let temp = tempfile::tempdir().unwrap();
        let repository = temp.path().join("source").join("comet");
        let root = temp.path().join("worktrees");
        repo(&repository);
        let journal_dir = temp.path().join("journal-dir");
        fs::create_dir_all(&journal_dir).unwrap();
        let journal_path = journal_dir.join("ownership.json");
        let journal = OwnershipJournal::at(journal_path, root.clone());

        let pending_target = worktree_path(&root, &repository, "pending-write-fails").unwrap();
        fs::set_permissions(&journal_dir, fs::Permissions::from_mode(0o500)).unwrap();
        let pending_result = journal.pending_create(&repository, &pending_target, "pending-fail");
        fs::set_permissions(&journal_dir, fs::Permissions::from_mode(0o700)).unwrap();
        assert!(pending_result.is_err());
        assert!(
            !pending_target.exists(),
            "caller must not add without PendingCreate"
        );

        let observed_target = worktree_path(&root, &repository, "observed-write-fails").unwrap();
        let reservation = journal
            .pending_create(&repository, &observed_target, "observed-fail")
            .unwrap();
        worktree(&repository, &reservation.path, "feature/observed-fail");
        fs::set_permissions(&journal_dir, fs::Permissions::from_mode(0o500)).unwrap();
        let observed_result =
            journal.created_observed(&repository, &reservation.path, "observed-fail");
        fs::set_permissions(&journal_dir, fs::Permissions::from_mode(0o700)).unwrap();
        assert!(observed_result.is_err());
        assert!(
            reservation.path.exists(),
            "recording failure preserves the Git worktree"
        );
        assert_eq!(
            journal.stage("observed-fail").unwrap(),
            Some(OwnershipStage::PendingCreate)
        );
        assert!(
            journal
                .reconcile_created_observed()
                .unwrap()
                .promoted_operation_ids
                .is_empty()
        );
        assert!(
            !journal
                .verify_owned(&repository, &reservation.path)
                .unwrap()
        );

        // If the final Owned write fails after CreatedObserved was durable,
        // restart reconciliation may safely finish that one transition.
        let final_target = worktree_path(&root, &repository, "finalize-write-fails").unwrap();
        let final_reservation = journal
            .pending_create(&repository, &final_target, "finalize-fail")
            .unwrap();
        worktree(
            &repository,
            &final_reservation.path,
            "feature/finalize-fail",
        );
        journal
            .created_observed(&repository, &final_reservation.path, "finalize-fail")
            .unwrap();
        fs::set_permissions(&journal_dir, fs::Permissions::from_mode(0o500)).unwrap();
        let finalize_result =
            journal.finalize_owned(&repository, &final_reservation.path, "finalize-fail");
        fs::set_permissions(&journal_dir, fs::Permissions::from_mode(0o700)).unwrap();
        assert!(finalize_result.is_err());
        assert!(final_reservation.path.exists());
        assert_eq!(
            journal.stage("finalize-fail").unwrap(),
            Some(OwnershipStage::CreatedObserved)
        );
        assert_eq!(
            journal
                .reconcile_created_observed()
                .unwrap()
                .promoted_operation_ids,
            ["finalize-fail"]
        );
        assert!(
            journal
                .verify_owned(&repository, &final_reservation.path)
                .unwrap()
        );
    }
}
