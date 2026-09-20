//! Durable repository/check-out identity for the Workers working set.
//!
//! The upstream project list is an execution registry: its `path` is the
//! directory a Worker must run in and its id is referenced by sessions.  It
//! is therefore deliberately not replaced by a repository model.  This
//! module stores the additional relationship in the Comet-owned
//! `comet_project_identity` namespace so a worktree can disappear from disk
//! without turning its old sessions into a new project.
//!
//! Git is only used as a local probe.  Remote URLs are observations and never
//! repository keys: two clones of the same remote remain separate until the
//! user explicitly associates them.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};
use std::time::{SystemTime, UNIX_EPOCH};

use base64::Engine as _;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

pub const IDENTITY_KEY: &str = "comet_project_identity";
pub const IDENTITY_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckoutKind {
    Primary,
    Linked,
    NonGit,
    Unresolved,
}

impl Default for CheckoutKind {
    fn default() -> Self {
        Self::Unresolved
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckoutOwnership {
    AppManaged,
    External,
    Unknown,
}

impl Default for CheckoutOwnership {
    fn default() -> Self {
        Self::Unknown
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckoutAvailability {
    Available,
    Missing,
    ProbeFailed,
}

impl Default for CheckoutAvailability {
    fn default() -> Self {
        Self::Missing
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RemoteObservation {
    pub name: String,
    pub url: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckoutObservation {
    pub project_id: Option<String>,
    pub path: String,
    pub canonical_path: Option<String>,
    pub common_dir: Option<String>,
    /// Local identity of the common Git directory. This is deliberately
    /// filesystem metadata rather than a commit, branch, remote, or path:
    /// replacing a repository in the same folder must become a conflict.
    pub common_dir_fingerprint: Option<String>,
    /// Stable macOS volume UUID plus common-directory inode. The legacy
    /// `common_dir_fingerprint` remains persisted for old Comet binaries and
    /// is refreshed to the current tuple when the stable identity agrees.
    pub common_dir_stable_fingerprint: Option<String>,
    pub main_repo: Option<String>,
    pub branch: Option<String>,
    pub detached_oid: Option<String>,
    pub remotes: Vec<RemoteObservation>,
    pub kind: CheckoutKind,
    pub availability: CheckoutAvailability,
    pub probe_error: Option<String>,
    pub ownership: Option<CheckoutOwnership>,
    pub observed_at_unix_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RepositoryIdentity {
    pub id: String,
    #[serde(default)]
    pub common_dir: Option<String>,
    #[serde(default)]
    pub common_dir_fingerprint: Option<String>,
    #[serde(default)]
    pub common_dir_stable_fingerprint: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub primary_path: Option<String>,
    #[serde(default, rename = "primaryProjectID", alias = "primaryProjectId")]
    pub primary_project_id: Option<String>,
    #[serde(default, rename = "projectIDs", alias = "projectIds")]
    pub project_ids: Vec<String>,
    #[serde(default)]
    pub last_seen_unix_ms: Option<u64>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CheckoutIdentity {
    #[serde(rename = "projectID", alias = "projectId")]
    pub project_id: String,
    #[serde(default, rename = "checkoutID", alias = "checkoutId")]
    pub checkout_id: Option<String>,
    #[serde(default, rename = "repositoryID", alias = "repositoryId")]
    pub repository_id: Option<String>,
    #[serde(default)]
    pub path: String,
    #[serde(default)]
    pub canonical_path: Option<String>,
    #[serde(default)]
    pub main_repo: Option<String>,
    #[serde(default)]
    pub kind: CheckoutKind,
    #[serde(default)]
    pub ownership: CheckoutOwnership,
    #[serde(default)]
    pub availability: CheckoutAvailability,
    #[serde(default)]
    pub branch: Option<String>,
    #[serde(default, rename = "lastKnownBranch")]
    pub last_known_branch: Option<String>,
    #[serde(default)]
    pub detached_oid: Option<String>,
    #[serde(default)]
    pub detached: bool,
    #[serde(default)]
    pub remotes: Vec<RemoteObservation>,
    #[serde(default)]
    pub observed_common_dir: Option<String>,
    #[serde(default)]
    pub conflict: Option<String>,
    #[serde(default)]
    pub last_observed_unix_ms: Option<u64>,
    #[serde(default)]
    pub archived: bool,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IdentityRegistry {
    #[serde(default = "default_schema_version")]
    pub version: u32,
    #[serde(default)]
    pub repositories: Vec<RepositoryIdentity>,
    #[serde(default)]
    pub checkouts: Vec<CheckoutIdentity>,
    #[serde(default)]
    pub suppressed_project_ids: Vec<String>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

impl Default for IdentityRegistry {
    fn default() -> Self {
        Self {
            version: IDENTITY_SCHEMA_VERSION,
            repositories: Vec::new(),
            checkouts: Vec::new(),
            suppressed_project_ids: Vec::new(),
            extra: BTreeMap::new(),
        }
    }
}

impl IdentityRegistry {
    pub fn checkout(&self, project_id: &str) -> Option<&CheckoutIdentity> {
        self.checkouts
            .iter()
            .find(|checkout| checkout.project_id == project_id)
    }

    pub fn repository(&self, repository_id: &str) -> Option<&RepositoryIdentity> {
        self.repositories
            .iter()
            .find(|repository| repository.id == repository_id)
    }

    pub fn repository_for_project(&self, project_id: &str) -> Option<&RepositoryIdentity> {
        self.checkout(project_id)
            .and_then(|checkout| checkout.repository_id.as_deref())
            .and_then(|repository_id| self.repository(repository_id))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepositoryCatalog {
    pub repository: RepositoryIdentity,
    pub checkouts: Vec<CheckoutIdentity>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ProjectCatalog {
    pub repositories: Vec<RepositoryCatalog>,
    pub pending: Vec<CheckoutIdentity>,
}

pub fn project_catalog(identity: &IdentityRegistry) -> ProjectCatalog {
    let mut grouped: BTreeMap<String, Vec<CheckoutIdentity>> = BTreeMap::new();
    let mut pending = Vec::new();
    for checkout in &identity.checkouts {
        if let Some(repository_id) = checkout.repository_id.as_deref() {
            grouped
                .entry(repository_id.to_owned())
                .or_default()
                .push(checkout.clone());
        } else {
            pending.push(checkout.clone());
        }
    }
    let repositories = identity
        .repositories
        .iter()
        .filter_map(|repository| {
            grouped.remove(&repository.id).map(|mut checkouts| {
                checkouts.sort_by(|left, right| left.project_id.cmp(&right.project_id));
                RepositoryCatalog {
                    repository: repository.clone(),
                    checkouts,
                }
            })
        })
        .collect();
    ProjectCatalog {
        repositories,
        pending,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IdentityPatch {
    pub project_id: String,
    pub observation: CheckoutObservation,
    pub repository_id: Option<String>,
    pub conflict: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ReconciliationPlan {
    pub base_state: Value,
    pub patches: Vec<IdentityPatch>,
    pub next_registry: IdentityRegistry,
    pub unsupported_version: Option<u32>,
    pub invalid_state: Option<String>,
}

impl ReconciliationPlan {
    pub fn changed(&self) -> bool {
        if self.unsupported_version.is_some() || self.invalid_state.is_some() {
            return false;
        }
        self.base_state
            .get(IDENTITY_KEY)
            .map(|value| value != &serde_json::to_value(&self.next_registry).unwrap_or(Value::Null))
            .unwrap_or(!self.next_registry.checkouts.is_empty())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IdentityRecoveryCandidate {
    pub project_id: String,
    pub repository_id: String,
    pub expected_old_fingerprint: String,
    pub expected_current_fingerprint: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MigrationReport {
    pub schema_version: u32,
    pub examined: usize,
    pub associated: usize,
    pub preserved: usize,
    pub missing: usize,
    pub pending: usize,
    pub conflicts: usize,
    pub changed: bool,
    #[serde(rename = "recoveryCandidates")]
    pub recovery_candidates: Vec<IdentityRecoveryCandidate>,
}

/// The result of an explicit, guarded identity repair. A repair only changes
/// the Comet identity namespace; project rows, sessions, presets and unknown
/// app-state keys remain outside this report and outside the mutation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IdentityRecoveryReport {
    pub project_id: String,
    pub repository_id: String,
    pub repaired_project_ids: Vec<String>,
    pub current_fingerprint: String,
    pub changed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IdentityConflict {
    StateChanged,
    UnsupportedVersion(u32),
    InvalidState(String),
    UnknownProject(String),
    UnknownRepository(String),
}

impl std::fmt::Display for IdentityConflict {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::StateChanged => formatter.write_str("identity state changed while reconciling"),
            Self::UnsupportedVersion(version) => {
                write!(formatter, "unsupported project identity version {version}")
            }
            Self::InvalidState(message) => formatter.write_str(message),
            Self::UnknownProject(project_id) => write!(formatter, "unknown project {project_id}"),
            Self::UnknownRepository(repository_id) => {
                write!(formatter, "unknown repository {repository_id}")
            }
        }
    }
}

impl std::error::Error for IdentityConflict {}

fn default_schema_version() -> u32 {
    IDENTITY_SCHEMA_VERSION
}

fn now_unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or_default()
}

fn generated_id(prefix: &str) -> String {
    format!("comet-{prefix}-{}", uuid::Uuid::new_v4().simple())
}

fn canonical_path(path: &Path) -> Option<String> {
    std::fs::canonicalize(path)
        .ok()
        .map(|path| path.to_string_lossy().into_owned())
}

fn common_dir_fingerprint(path: &str) -> Option<String> {
    let metadata = std::fs::metadata(path).ok()?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        return Some(format!("unix:{}:{}", metadata.dev(), metadata.ino()));
    }
    #[cfg(not(unix))]
    {
        let _ = metadata;
        None
    }
}

/// Return a persistent identity for a directory on macOS. `st_dev` is a
/// mount-local device number and can change after a volume is remounted, so it
/// is deliberately kept out of this value. `getattrlist(2)` is the verified
/// macOS volume API for the filesystem UUID; the common-directory inode then
/// distinguishes replacement on that same volume.
#[cfg(target_os = "macos")]
fn macos_volume_uuid(path: &Path) -> Option<uuid::Uuid> {
    use std::ffi::{CStr, CString};
    use std::os::unix::ffi::OsStrExt;

    let path = CString::new(path.as_os_str().as_bytes()).ok()?;
    // Volume attributes are only valid when the path is the volume root. The
    // mount point from statfs is the stable OS-resolved root even when the
    // checkout is below a symlink or a remounted volume.
    let mut filesystem = unsafe { std::mem::zeroed::<libc::statfs>() };
    if unsafe { libc::statfs(path.as_ptr(), &mut filesystem) } != 0 {
        return None;
    }
    let mountpoint = unsafe { CStr::from_ptr(filesystem.f_mntonname.as_ptr()) };
    let mountpoint = CString::new(mountpoint.to_bytes()).ok()?;
    let mut attributes = libc::attrlist {
        bitmapcount: libc::ATTR_BIT_MAP_COUNT as u16,
        reserved: 0,
        commonattr: 0,
        volattr: libc::ATTR_VOL_INFO | libc::ATTR_VOL_UUID,
        dirattr: 0,
        fileattr: 0,
        forkattr: 0,
    };
    // The only requested value is a 16-byte UUID following a four-byte length.
    // A missing/unsupported attribute produces a shorter result and is refused.
    #[repr(C, align(4))]
    struct VolumeUuidAttributes {
        length: u32,
        uuid: libc::uuid_t,
    }
    let mut buffer: VolumeUuidAttributes = unsafe { std::mem::zeroed() };
    let result = unsafe {
        libc::getattrlist(
            mountpoint.as_ptr(),
            (&mut attributes as *mut libc::attrlist).cast(),
            (&mut buffer as *mut VolumeUuidAttributes).cast(),
            std::mem::size_of::<VolumeUuidAttributes>(),
            0,
        )
    };
    if result != 0 {
        return None;
    }
    let length = buffer.length as usize;
    if length != std::mem::size_of::<VolumeUuidAttributes>() {
        return None;
    }
    let bytes = buffer.uuid;
    (!bytes.iter().all(|byte| *byte == 0)).then(|| uuid::Uuid::from_bytes(bytes))
}

fn common_dir_stable_fingerprint(path: &str) -> Option<String> {
    let metadata = std::fs::metadata(path).ok()?;
    #[cfg(target_os = "macos")]
    {
        use std::os::unix::fs::MetadataExt;
        let volume_uuid = macos_volume_uuid(Path::new(path))?;
        return Some(format!(
            "macos:{}:{}",
            volume_uuid.hyphenated(),
            metadata.ino()
        ));
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = metadata;
        None
    }
}

fn command_output(path: &Path, args: &[&str]) -> Result<String, String> {
    crate::git_command::run_git(path, args).map(|output| output.trim().to_owned())
}

fn resolve_git_path(root: &Path, path: &str) -> PathBuf {
    let path = Path::new(path);
    if path.is_absolute() {
        path.to_owned()
    } else {
        root.join(path)
    }
}

fn parse_remotes(output: &str) -> Vec<RemoteObservation> {
    let mut remotes = Vec::new();
    for line in output.lines() {
        let mut fields = line.split_whitespace();
        let Some(name) = fields.next() else { continue };
        let Some(url) = fields.next() else { continue };
        if !remotes
            .iter()
            .any(|remote: &RemoteObservation| remote.name == name)
        {
            remotes.push(RemoteObservation {
                name: name.to_owned(),
                url: url.to_owned(),
            });
        }
    }
    remotes
}

/// Probe a checkout without network access. A missing directory is distinct
/// from a directory where Git cannot be read (permissions, corruption, etc.).
pub fn probe_checkout(path: &Path) -> CheckoutObservation {
    let observed_at_unix_ms = now_unix_ms();
    let path_string = path.to_string_lossy().into_owned();
    let canonical = canonical_path(path);
    if !path.exists() {
        return CheckoutObservation {
            project_id: None,
            path: path_string,
            canonical_path: canonical,
            common_dir: None,
            common_dir_fingerprint: None,
            common_dir_stable_fingerprint: None,
            main_repo: None,
            branch: None,
            detached_oid: None,
            remotes: Vec::new(),
            kind: CheckoutKind::Unresolved,
            availability: CheckoutAvailability::Missing,
            probe_error: None,
            ownership: None,
            observed_at_unix_ms,
        };
    }
    if !path.is_dir() {
        return CheckoutObservation {
            project_id: None,
            path: path_string,
            canonical_path: canonical,
            common_dir: None,
            common_dir_fingerprint: None,
            common_dir_stable_fingerprint: None,
            main_repo: None,
            branch: None,
            detached_oid: None,
            remotes: Vec::new(),
            kind: CheckoutKind::Unresolved,
            availability: CheckoutAvailability::ProbeFailed,
            probe_error: Some("checkout path is not a directory".to_owned()),
            ownership: None,
            observed_at_unix_ms,
        };
    }
    let root = command_output(path, &["rev-parse", "--show-toplevel"]);
    let Ok(root) = root else {
        let has_git_metadata = path.join(".git").exists();
        return CheckoutObservation {
            project_id: None,
            path: path_string,
            canonical_path: canonical,
            common_dir: None,
            common_dir_fingerprint: None,
            common_dir_stable_fingerprint: None,
            main_repo: None,
            branch: None,
            detached_oid: None,
            remotes: Vec::new(),
            kind: if has_git_metadata {
                CheckoutKind::Unresolved
            } else {
                CheckoutKind::NonGit
            },
            availability: if has_git_metadata {
                CheckoutAvailability::ProbeFailed
            } else {
                CheckoutAvailability::Available
            },
            probe_error: has_git_metadata.then(|| "Git metadata could not be read".to_owned()),
            ownership: None,
            observed_at_unix_ms,
        };
    };
    let root_path = PathBuf::from(root);
    let git_dir = command_output(path, &["rev-parse", "--git-dir"])
        .ok()
        .map(|value| resolve_git_path(&root_path, &value));
    let common_dir = command_output(path, &["rev-parse", "--git-common-dir"])
        .ok()
        .map(|value| resolve_git_path(&root_path, &value))
        .and_then(|value| {
            canonical_path(&value).or_else(|| Some(value.to_string_lossy().into_owned()))
        });
    let common_dir_fingerprint = common_dir.as_deref().and_then(common_dir_fingerprint);
    let common_dir_stable_fingerprint = common_dir
        .as_deref()
        .and_then(common_dir_stable_fingerprint);
    let git_dir_string = git_dir
        .as_deref()
        .and_then(canonical_path)
        .or_else(|| git_dir.map(|value| value.to_string_lossy().into_owned()));
    let linked = match (&git_dir_string, &common_dir) {
        (Some(git_dir), Some(common_dir)) => git_dir != common_dir,
        _ => false,
    };
    let branch = command_output(path, &["symbolic-ref", "--quiet", "--short", "HEAD"])
        .ok()
        .filter(|branch| !branch.is_empty());
    let detached_oid = if branch.is_none() {
        command_output(path, &["rev-parse", "--short=12", "HEAD"])
            .ok()
            .filter(|oid| !oid.is_empty())
    } else {
        None
    };
    let main_repo = common_dir.as_deref().and_then(|common| {
        let path = Path::new(common);
        (path.file_name().and_then(|name| name.to_str()) == Some(".git"))
            .then(|| {
                path.parent()
                    .map(|parent| parent.to_string_lossy().into_owned())
            })
            .flatten()
    });
    let remotes = command_output(path, &["remote", "-v"])
        .map(|value| parse_remotes(&value))
        .unwrap_or_default();
    CheckoutObservation {
        project_id: None,
        path: path_string,
        canonical_path: canonical,
        common_dir,
        common_dir_fingerprint,
        common_dir_stable_fingerprint,
        main_repo,
        branch,
        detached_oid,
        remotes,
        kind: if linked {
            CheckoutKind::Linked
        } else {
            CheckoutKind::Primary
        },
        availability: CheckoutAvailability::Available,
        probe_error: None,
        ownership: None,
        observed_at_unix_ms,
    }
}

fn is_group(value: &Value) -> bool {
    value
        .get("is_group")
        .or_else(|| value.get("isGroup"))
        .and_then(Value::as_bool)
        .unwrap_or(false)
        || (value
            .get("is_folder")
            .or_else(|| value.get("isFolder"))
            .and_then(Value::as_bool)
            .unwrap_or(false)
            && value
                .get("parent_project_id")
                .or_else(|| value.get("parentProjectID"))
                .and_then(Value::as_str)
                .is_some()
            && value
                .get("worktree_branch")
                .or_else(|| value.get("worktreeBranch"))
                .and_then(Value::as_str)
                .is_none())
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ProjectEntry {
    project_id: String,
    path: String,
    /// A ledger-only entry is historical context. It is kept in the identity
    /// registry so Settings can associate it and a later explicit add can
    /// reuse its checkout/repository, but it must never become an executable
    /// `projects[]` record.
    ledger_only: bool,
}

fn identity_path_key(path: &str) -> String {
    let path = canonical_path(Path::new(path)).unwrap_or_else(|| path.to_owned());
    let path = path.replace('\\', "/");
    let path = path.trim_end_matches('/');
    if path.is_empty() {
        "/".to_owned()
    } else {
        path.to_owned()
    }
}

fn ledger_project_id(path: &str) -> String {
    let encoded =
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(identity_path_key(path).as_bytes());
    format!("comet-history-{encoded}")
}

fn project_entries(state: &Value, registry: &IdentityRegistry) -> Vec<ProjectEntry> {
    let mut entries = Vec::new();

    for project in state
        .get("projects")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        if is_group(project) {
            continue;
        }
        let Some(project_id) = project.get("id").and_then(Value::as_str) else {
            continue;
        };
        let Some(path) = project.get("path").and_then(Value::as_str) else {
            continue;
        };
        entries.push(ProjectEntry {
            project_id: project_id.to_owned(),
            path: path.to_owned(),
            ledger_only: false,
        });
    }

    let live_paths = entries
        .iter()
        .map(|entry| identity_path_key(&entry.path))
        .collect::<std::collections::HashSet<_>>();
    let suppressed = registry
        .suppressed_project_ids
        .iter()
        .collect::<std::collections::HashSet<_>>();
    let mut ledger_paths = live_paths;
    for ledger in state
        .get("comet_projects")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let Some(path) = ledger.get("path").and_then(Value::as_str) else {
            continue;
        };
        let key = identity_path_key(path);
        if !ledger_paths.insert(key.clone()) {
            continue;
        }
        let existing = registry
            .checkouts
            .iter()
            .find(|checkout| identity_path_key(&checkout.path) == key);
        let project_id = existing
            .map(|checkout| checkout.project_id.clone())
            .unwrap_or_else(|| ledger_project_id(path));
        if suppressed.contains(&project_id) {
            continue;
        }
        entries.push(ProjectEntry {
            project_id,
            path: path.to_owned(),
            ledger_only: true,
        });
    }
    entries
}

/// Inputs that determine which checkouts a reconciliation must observe. The
/// snapshot intentionally omits names, ordering, sessions and other state
/// that may change while Git probes run; a newly registered project or ledger
/// path must instead cause the stale plan to retry.
fn reconciliation_input_snapshot(state: &Value) -> Vec<(String, String)> {
    let mut snapshot = Vec::new();
    for project in state
        .get("projects")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let Some(project_id) = project.get("id").and_then(Value::as_str) else {
            continue;
        };
        let Some(path) = project.get("path").and_then(Value::as_str) else {
            continue;
        };
        snapshot.push((format!("project:{project_id}"), identity_path_key(path)));
    }
    for ledger in state
        .get("comet_projects")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let Some(path) = ledger.get("path").and_then(Value::as_str) else {
            continue;
        };
        snapshot.push(("ledger".to_owned(), identity_path_key(path)));
    }
    snapshot.sort();
    snapshot
}

fn validate_reconciliation_inputs(
    state: &Value,
    expected: &[(String, String)],
) -> Result<(), String> {
    (reconciliation_input_snapshot(state) == expected)
        .then_some(())
        .ok_or_else(|| {
            "projects or ledger changed while identity was being reconciled; retry".to_owned()
        })
}

fn missing_observation(project_id: &str, path: &str) -> CheckoutObservation {
    CheckoutObservation {
        project_id: Some(project_id.to_owned()),
        path: path.to_owned(),
        canonical_path: None,
        common_dir: None,
        common_dir_fingerprint: None,
        common_dir_stable_fingerprint: None,
        main_repo: None,
        branch: None,
        detached_oid: None,
        remotes: Vec::new(),
        kind: CheckoutKind::Unresolved,
        availability: CheckoutAvailability::Missing,
        probe_error: None,
        ownership: None,
        observed_at_unix_ms: now_unix_ms(),
    }
}

fn read_registry(state: &Value) -> Result<IdentityRegistry, IdentityConflict> {
    let Some(value) = state.get(IDENTITY_KEY) else {
        return Ok(IdentityRegistry::default());
    };
    let version = value
        .get("version")
        .and_then(Value::as_u64)
        .unwrap_or(IDENTITY_SCHEMA_VERSION as u64) as u32;
    if version > IDENTITY_SCHEMA_VERSION {
        return Err(IdentityConflict::UnsupportedVersion(version));
    }
    serde_json::from_value(value.clone()).map_err(|error| {
        IdentityConflict::InvalidState(format!("invalid identity registry: {error}"))
    })
}

fn repository_for_common<'a>(
    registry: &'a IdentityRegistry,
    common_dir: Option<&str>,
) -> Option<&'a RepositoryIdentity> {
    common_dir.and_then(|common_dir| {
        registry
            .repositories
            .iter()
            .find(|repository| repository.common_dir.as_deref() == Some(common_dir))
    })
}

fn project_is_in_state(state: &Value, registry: &IdentityRegistry, project_id: &str) -> bool {
    project_entries(state, registry)
        .iter()
        .any(|entry| entry.project_id == project_id)
}

fn remove_checkout_membership(registry: &mut IdentityRegistry, project_id: &str) {
    for repository in &mut registry.repositories {
        repository.project_ids.retain(|id| id != project_id);
        if repository.primary_project_id.as_deref() == Some(project_id) {
            repository.primary_project_id = None;
        }
    }
}

fn mark_ledger_only(checkout: &mut CheckoutIdentity, ledger_only: bool) {
    if ledger_only {
        checkout
            .extra
            .insert("ledgerOnly".to_owned(), Value::Bool(true));
        checkout
            .extra
            .insert("nonExecutable".to_owned(), Value::Bool(true));
    } else {
        checkout.extra.remove("ledgerOnly");
        checkout.extra.remove("nonExecutable");
    }
}

fn build_patch(
    registry: &mut IdentityRegistry,
    existing: Option<&CheckoutIdentity>,
    observation: &CheckoutObservation,
) -> IdentityPatch {
    let project_id = observation
        .project_id
        .clone()
        .expect("project observations always have an id");
    let existing_repository_id = existing.and_then(|checkout| checkout.repository_id.clone());
    let observation_changed = existing.is_none_or(|previous| {
        previous.path != observation.path
            || previous.canonical_path != observation.canonical_path
            || previous.main_repo != observation.main_repo
            || previous.kind != observation.kind
            || previous.availability != observation.availability
            || previous.branch != observation.branch
            || previous.detached_oid != observation.detached_oid
            || previous.remotes != observation.remotes
            || observation.ownership.is_some_and(|ownership| {
                previous.ownership != ownership && previous.ownership == CheckoutOwnership::Unknown
            })
    });
    let manually_unlinked = existing
        .and_then(|checkout| checkout.conflict.as_deref())
        .is_some_and(|conflict| conflict.starts_with("association removed by user"));
    let matching_repository = (!manually_unlinked)
        .then(|| repository_for_common(registry, observation.common_dir.as_deref()))
        .flatten();
    let mut repository_id = existing_repository_id
        .clone()
        .or_else(|| matching_repository.map(|repository| repository.id.clone()));
    let mut conflict = existing.and_then(|checkout| checkout.conflict.clone());
    if let (Some(previous), Some(current)) = (
        existing.and_then(|checkout| checkout.observed_common_dir.as_deref()),
        observation.common_dir.as_deref(),
    ) {
        if previous != current {
            conflict = Some(format!("checkout moved from {previous} to {current}"));
        }
    }
    if let (Some(previous), Some(current)) = (
        existing_repository_id.as_deref().and_then(|repository_id| {
            registry
                .repository(repository_id)
                .and_then(|repository| repository.common_dir.as_deref())
        }),
        observation.common_dir.as_deref(),
    ) {
        if previous != current {
            conflict = Some(format!(
                "repository identity changed from {previous} to {current}"
            ));
        }
    }
    if let Some(repository_id) = repository_id.as_deref() {
        let repository = registry.repository(repository_id);
        let previous_stable =
            repository.and_then(|repository| repository.common_dir_stable_fingerprint.as_deref());
        let current_stable = observation.common_dir_stable_fingerprint.as_deref();
        let previous_legacy =
            repository.and_then(|repository| repository.common_dir_fingerprint.as_deref());
        let current_legacy = observation.common_dir_fingerprint.as_deref();
        if observation.availability == CheckoutAvailability::Available
            && observation.common_dir.is_some()
        {
            match (previous_stable, current_stable) {
                (Some(previous), Some(current)) => {
                    if previous != current {
                        conflict = Some(format!(
                            "stable common directory identity changed from {previous} to {current}"
                        ));
                    } else if conflict.as_deref()
                        == Some("stable common directory identity is unavailable")
                        || legacy_conflict_targets(
                            conflict.as_deref(),
                            current_legacy.unwrap_or_default(),
                        )
                    {
                        // An older binary can reintroduce its device-number
                        // conflict after the stable identity already agrees.
                        // Clear that recognized stale legacy conflict while
                        // preserving every unrelated/manual conflict string.
                        conflict = None;
                    }
                }
                (Some(_), None) => {
                    if conflict.is_none() {
                        conflict =
                            Some("stable common directory identity is unavailable".to_owned());
                    }
                }
                (None, Some(_)) | (None, None) => {
                    // A legacy tuple can only be upgraded when its full value
                    // still matches. A device-number mismatch remains an explicit
                    // recovery case even when the inode happens to be unchanged.
                    if let (Some(previous), Some(current)) = (previous_legacy, current_legacy)
                        && previous != current
                    {
                        conflict = Some(format!(
                            "common directory fingerprint changed from {previous} to {current}"
                        ));
                    }
                }
            }
        }
    }
    if repository_id.is_none() && observation.common_dir.is_some() && !manually_unlinked {
        let id = generated_id("repository");
        registry.repositories.push(RepositoryIdentity {
            id: id.clone(),
            common_dir: observation.common_dir.clone(),
            common_dir_fingerprint: observation.common_dir_fingerprint.clone(),
            common_dir_stable_fingerprint: observation.common_dir_stable_fingerprint.clone(),
            name: observation
                .main_repo
                .as_deref()
                .or(Some(observation.path.as_str()))
                .and_then(|path| Path::new(path).file_name())
                .and_then(|name| name.to_str())
                .map(str::to_owned),
            primary_path: observation.main_repo.clone().or_else(|| {
                (observation.kind == CheckoutKind::Primary).then(|| {
                    observation
                        .canonical_path
                        .clone()
                        .unwrap_or_else(|| observation.path.clone())
                })
            }),
            primary_project_id: (observation.kind == CheckoutKind::Primary)
                .then(|| project_id.clone()),
            project_ids: vec![project_id.clone()],
            last_seen_unix_ms: Some(observation.observed_at_unix_ms),
            extra: BTreeMap::new(),
        });
        repository_id = Some(id);
    }
    if let Some(repository_id) = repository_id.as_deref() {
        if let Some(repository) = registry
            .repositories
            .iter_mut()
            .find(|repository| repository.id == repository_id)
        {
            if !repository.project_ids.iter().any(|id| id == &project_id) {
                repository.project_ids.push(project_id.clone());
            }
            if repository.primary_project_id.is_none() && observation.kind == CheckoutKind::Primary
            {
                repository.primary_project_id = Some(project_id.clone());
            }
            if repository.primary_path.is_none() && observation.kind == CheckoutKind::Primary {
                repository.primary_path = Some(
                    observation
                        .canonical_path
                        .clone()
                        .unwrap_or_else(|| observation.path.clone()),
                );
            }
            if repository.primary_path.is_none() {
                repository.primary_path = observation.main_repo.clone();
            }
            if repository.name.is_none() {
                repository.name = observation
                    .main_repo
                    .as_deref()
                    .or(Some(observation.path.as_str()))
                    .and_then(|path| Path::new(path).file_name())
                    .and_then(|name| name.to_str())
                    .map(str::to_owned);
            }
            if conflict.is_none() {
                if let Some(fingerprint) = observation.common_dir_fingerprint.as_ref() {
                    repository.common_dir_fingerprint = Some(fingerprint.clone());
                }
                if let Some(fingerprint) = observation.common_dir_stable_fingerprint.as_ref() {
                    repository.common_dir_stable_fingerprint = Some(fingerprint.clone());
                }
            }
            if observation_changed {
                repository.last_seen_unix_ms = Some(observation.observed_at_unix_ms);
            }
        }
    }
    IdentityPatch {
        project_id,
        observation: observation.clone(),
        repository_id,
        conflict,
    }
}

fn checkout_from_patch(
    patch: &IdentityPatch,
    previous: Option<&CheckoutIdentity>,
) -> CheckoutIdentity {
    let observation = &patch.observation;
    let last_known_branch = observation
        .branch
        .clone()
        .or_else(|| previous.and_then(|checkout| checkout.last_known_branch.clone()));
    CheckoutIdentity {
        project_id: patch.project_id.clone(),
        checkout_id: previous
            .and_then(|checkout| checkout.checkout_id.clone())
            .or_else(|| Some(patch.project_id.clone())),
        repository_id: patch.repository_id.clone(),
        path: observation.path.clone(),
        canonical_path: observation.canonical_path.clone(),
        main_repo: observation
            .main_repo
            .clone()
            .or_else(|| previous.and_then(|checkout| checkout.main_repo.clone())),
        kind: observation.kind,
        ownership: previous
            .map(|checkout| checkout.ownership)
            .filter(|ownership| *ownership != CheckoutOwnership::Unknown)
            .or(observation.ownership)
            .unwrap_or_default(),
        availability: observation.availability,
        branch: observation.branch.clone(),
        last_known_branch,
        detached_oid: observation.detached_oid.clone(),
        detached: match observation.availability {
            CheckoutAvailability::Available => observation.detached_oid.is_some(),
            CheckoutAvailability::Missing | CheckoutAvailability::ProbeFailed => previous
                .map(|checkout| checkout.detached)
                .unwrap_or_else(|| observation.detached_oid.is_some()),
        },
        remotes: if observation.remotes.is_empty() {
            previous
                .map(|checkout| checkout.remotes.clone())
                .unwrap_or_default()
        } else {
            observation.remotes.clone()
        },
        observed_common_dir: observation
            .common_dir
            .clone()
            .or_else(|| previous.and_then(|checkout| checkout.observed_common_dir.clone())),
        conflict: patch.conflict.clone(),
        last_observed_unix_ms: if previous.is_some_and(|previous| {
            previous.path == observation.path
                && previous.canonical_path == observation.canonical_path
                && previous.main_repo == observation.main_repo
                && previous.kind == observation.kind
                && previous.availability == observation.availability
                && previous.branch == observation.branch
                && previous.detached_oid == observation.detached_oid
                && previous.remotes == observation.remotes
        }) {
            previous.and_then(|checkout| checkout.last_observed_unix_ms)
        } else {
            Some(observation.observed_at_unix_ms)
        },
        archived: previous.map(|checkout| checkout.archived).unwrap_or(false),
        extra: previous
            .map(|checkout| checkout.extra.clone())
            .unwrap_or_default(),
    }
}

/// Build a pure, repeatable plan from a raw app-state snapshot and Git
/// observations. The caller may probe outside the app-state lock, then apply
/// this plan only after re-reading and comparing the snapshot.
pub fn plan_reconciliation(
    state: &Value,
    observations: &[CheckoutObservation],
) -> ReconciliationPlan {
    let unsupported_version = state
        .get(IDENTITY_KEY)
        .and_then(|value| value.get("version"))
        .and_then(Value::as_u64)
        .map(|version| version as u32)
        .filter(|version| *version > IDENTITY_SCHEMA_VERSION);
    let (mut registry, invalid_state) = if unsupported_version.is_some() {
        (IdentityRegistry::default(), None)
    } else {
        match read_registry(state) {
            Ok(registry) => (registry, None),
            Err(error) => (IdentityRegistry::default(), Some(error.to_string())),
        }
    };
    let mut patches = Vec::new();
    let entries = project_entries(state, &registry);
    for entry in entries {
        let project_id = entry.project_id;
        let path = entry.path;
        let observation = observations
            .iter()
            .find(|observation| observation.project_id.as_deref() == Some(project_id.as_str()))
            .cloned()
            .unwrap_or_else(|| missing_observation(&project_id, &path));
        let previous = registry.checkout(&project_id).cloned().or_else(|| {
            if !entry.ledger_only {
                return None;
            }
            registry
                .checkouts
                .iter()
                .find(|checkout| identity_path_key(&checkout.path) == identity_path_key(&path))
                .cloned()
        });
        if let Some(previous_id) = previous
            .as_ref()
            .map(|checkout| checkout.project_id.as_str())
            .filter(|previous_id| *previous_id != project_id)
        {
            remove_checkout_membership(&mut registry, previous_id);
        }
        let patch = build_patch(&mut registry, previous.as_ref(), &observation);
        let mut checkout = checkout_from_patch(&patch, previous.as_ref());
        mark_ledger_only(&mut checkout, entry.ledger_only);
        let previous_id = previous.as_ref().map(|old| old.project_id.clone());
        registry.checkouts.retain(|item| {
            item.project_id != project_id && previous_id.as_deref() != Some(&item.project_id)
        });
        registry.checkouts.push(checkout);
        patches.push(patch);
    }
    registry
        .checkouts
        .sort_by(|left, right| left.project_id.cmp(&right.project_id));
    registry
        .repositories
        .sort_by(|left, right| left.id.cmp(&right.id));
    ReconciliationPlan {
        base_state: state.clone(),
        patches,
        next_registry: registry,
        unsupported_version,
        invalid_state,
    }
}

fn report(plan: &ReconciliationPlan, changed: bool) -> MigrationReport {
    let missing = plan
        .patches
        .iter()
        .filter(|patch| patch.observation.availability == CheckoutAvailability::Missing)
        .count();
    let pending = plan
        .patches
        .iter()
        .filter(|patch| patch.repository_id.is_none())
        .count();
    let conflicts = plan
        .patches
        .iter()
        .filter(|patch| patch.conflict.is_some())
        .count();
    let recovery_candidates = plan
        .patches
        .iter()
        .filter_map(|patch| {
            let conflict = patch.conflict.as_deref()?;
            let repository_id = patch.repository_id.as_deref()?;
            let repository = plan.next_registry.repository(repository_id)?;
            let (expected_old, expected_current) = if conflict
                .strip_prefix("stable common directory identity changed from ")
                .is_some()
            {
                (
                    repository.common_dir_stable_fingerprint.as_deref()?,
                    patch.observation.common_dir_stable_fingerprint.as_deref()?,
                )
            } else if conflict
                .strip_prefix("common directory fingerprint changed from ")
                .is_some()
            {
                (
                    repository.common_dir_fingerprint.as_deref()?,
                    patch
                        .observation
                        .common_dir_stable_fingerprint
                        .as_deref()
                        .or(patch.observation.common_dir_fingerprint.as_deref())?,
                )
            } else {
                return None;
            };
            Some(IdentityRecoveryCandidate {
                project_id: patch.project_id.clone(),
                repository_id: repository_id.to_owned(),
                expected_old_fingerprint: expected_old.to_owned(),
                expected_current_fingerprint: expected_current.to_owned(),
            })
        })
        .collect();
    MigrationReport {
        schema_version: IDENTITY_SCHEMA_VERSION,
        examined: plan.patches.len(),
        associated: plan
            .patches
            .iter()
            .filter(|patch| patch.repository_id.is_some() && patch.conflict.is_none())
            .count(),
        preserved: plan
            .patches
            .iter()
            .filter(|patch| patch.observation.availability == CheckoutAvailability::Missing)
            .count(),
        missing,
        pending,
        conflicts,
        changed,
        recovery_candidates,
    }
}

/// Apply only the identity namespace. Existing projects, sessions, unknown
/// top-level keys and future identity fields are left intact.
pub fn apply_reconciliation(
    state: &mut Value,
    plan: &ReconciliationPlan,
) -> Result<MigrationReport, IdentityConflict> {
    if let Some(version) = plan.unsupported_version {
        return Err(IdentityConflict::UnsupportedVersion(version));
    }
    if let Some(error) = plan.invalid_state.as_deref() {
        return Err(IdentityConflict::InvalidState(error.to_owned()));
    }
    if state != &plan.base_state {
        return Err(IdentityConflict::StateChanged);
    }
    let object = state
        .as_object_mut()
        .ok_or_else(|| IdentityConflict::InvalidState("app state is not an object".to_owned()))?;
    let value = serde_json::to_value(&plan.next_registry)
        .map_err(|error| IdentityConflict::InvalidState(error.to_string()))?;
    let changed = object.get(IDENTITY_KEY) != Some(&value);
    if changed {
        object.insert(IDENTITY_KEY.to_owned(), value);
    }
    Ok(report(plan, changed))
}

fn observation_for_registered_project(
    project_id: &str,
    path: &Path,
    ownership: CheckoutOwnership,
) -> CheckoutObservation {
    let mut observation = probe_checkout(path);
    observation.project_id = Some(project_id.to_owned());
    observation.ownership = Some(ownership);
    observation
}

/// Update one registration inside an existing `app_state::edit` closure.
/// This is used by `add_project` and `create_worktree` so the project record
/// and its identity become visible in one atomic state edit.
pub fn register_observation_in_state(
    state: &mut Map<String, Value>,
    project_id: &str,
    mut observation: CheckoutObservation,
) -> Result<CheckoutIdentity, String> {
    observation.project_id = Some(project_id.to_owned());
    let value = Value::Object(state.clone());
    let mut registry = read_registry(&value).map_err(|error| error.to_string())?;
    // Registering a checkout explicitly is the user's re-add decision. It
    // clears a prior Forget suppression so a later background pass does not
    // hide the checkout that was deliberately restored.
    registry
        .suppressed_project_ids
        .retain(|suppressed| suppressed != project_id);
    let previous = registry.checkout(project_id).cloned().or_else(|| {
        registry
            .checkouts
            .iter()
            .find(|checkout| {
                identity_path_key(&checkout.path) == identity_path_key(&observation.path)
            })
            .cloned()
    });
    if let Some(previous_id) = previous
        .as_ref()
        .map(|checkout| checkout.project_id.as_str())
        .filter(|previous_id| *previous_id != project_id)
    {
        registry
            .suppressed_project_ids
            .retain(|suppressed| suppressed != previous_id);
        remove_checkout_membership(&mut registry, previous_id);
    }
    let patch = build_patch(&mut registry, previous.as_ref(), &observation);
    let mut checkout = checkout_from_patch(&patch, previous.as_ref());
    mark_ledger_only(&mut checkout, false);
    // Explicit registration is also an explicit restore after an archived or
    // forgotten ledger row was added again.
    checkout.archived = false;
    let previous_id = previous.as_ref().map(|old| old.project_id.clone());
    registry.checkouts.retain(|item| {
        item.project_id != project_id && previous_id.as_deref() != Some(&item.project_id)
    });
    registry.checkouts.push(checkout);
    registry
        .checkouts
        .sort_by(|left, right| left.project_id.cmp(&right.project_id));
    registry
        .repositories
        .sort_by(|left, right| left.id.cmp(&right.id));
    let identity_value = serde_json::to_value(&registry).map_err(|error| error.to_string())?;
    state.insert(IDENTITY_KEY.to_owned(), identity_value);
    let checkout = registry
        .checkout(project_id)
        .cloned()
        .ok_or_else(|| format!("identity registration did not create project {project_id}"))?;
    Ok(checkout)
}

/// Convenience for callers that are not already holding an app-state edit.
/// Production registration paths should call [`probe_checkout`] before
/// entering `app_state::edit` and pass the observation to
/// [`register_observation_in_state`], keeping Git I/O outside the lock.
pub fn register_project_in_state(
    state: &mut Map<String, Value>,
    project_id: &str,
    path: &Path,
    ownership: CheckoutOwnership,
) -> Result<CheckoutIdentity, String> {
    register_observation_in_state(
        state,
        project_id,
        observation_for_registered_project(project_id, path, ownership),
    )
}

pub fn set_checkout_archived(
    state: &mut Value,
    project_id: &str,
    archived: bool,
) -> Result<(), IdentityConflict> {
    let mut registry = read_registry(state)?;
    let checkout = registry
        .checkouts
        .iter_mut()
        .find(|checkout| checkout.project_id == project_id)
        .ok_or_else(|| IdentityConflict::UnknownProject(project_id.to_owned()))?;
    checkout.archived = archived;
    let object = state
        .as_object_mut()
        .ok_or_else(|| IdentityConflict::InvalidState("app state is not an object".to_owned()))?;
    object.insert(
        IDENTITY_KEY.to_owned(),
        serde_json::to_value(registry)
            .map_err(|error| IdentityConflict::InvalidState(error.to_string()))?,
    );
    Ok(())
}

pub fn associate_checkout_in_state(
    state: &mut Value,
    project_id: &str,
    repository_id: &str,
) -> Result<(), IdentityConflict> {
    let mut registry = read_registry(state)?;
    if !project_is_in_state(state, &registry, project_id) && registry.checkout(project_id).is_none()
    {
        return Err(IdentityConflict::UnknownProject(project_id.to_owned()));
    }
    if registry.repository(repository_id).is_none() {
        return Err(IdentityConflict::UnknownRepository(
            repository_id.to_owned(),
        ));
    }
    let old_repository_id = registry
        .checkout(project_id)
        .and_then(|checkout| checkout.repository_id.clone());
    if old_repository_id
        .as_deref()
        .is_some_and(|old_repository_id| old_repository_id != repository_id)
    {
        remove_checkout_membership(&mut registry, project_id);
    }
    let checkout = registry
        .checkouts
        .iter_mut()
        .find(|checkout| checkout.project_id == project_id)
        .ok_or_else(|| IdentityConflict::UnknownProject(project_id.to_owned()))?;
    checkout.repository_id = Some(repository_id.to_owned());
    checkout.conflict = None;
    if let Some(repository) = registry
        .repositories
        .iter_mut()
        .find(|repository| repository.id == repository_id)
    {
        if !repository.project_ids.iter().any(|id| id == project_id) {
            repository.project_ids.push(project_id.to_owned());
        }
    }
    let object = state
        .as_object_mut()
        .ok_or_else(|| IdentityConflict::InvalidState("app state is not an object".to_owned()))?;
    object.insert(
        IDENTITY_KEY.to_owned(),
        serde_json::to_value(registry)
            .map_err(|error| IdentityConflict::InvalidState(error.to_string()))?,
    );
    Ok(())
}

pub fn undo_checkout_association_in_state(
    state: &mut Value,
    project_id: &str,
) -> Result<(), IdentityConflict> {
    let mut registry = read_registry(state)?;
    let repository_id = registry
        .checkout(project_id)
        .and_then(|checkout| checkout.repository_id.clone())
        .ok_or_else(|| IdentityConflict::UnknownProject(project_id.to_owned()))?;
    if let Some(checkout) = registry
        .checkouts
        .iter_mut()
        .find(|checkout| checkout.project_id == project_id)
    {
        checkout.repository_id = None;
        checkout.conflict = Some("association removed by user".to_owned());
    }
    if let Some(repository) = registry
        .repositories
        .iter_mut()
        .find(|repository| repository.id == repository_id)
    {
        repository.project_ids.retain(|id| id != project_id);
        if repository.primary_project_id.as_deref() == Some(project_id) {
            repository.primary_project_id = None;
        }
    }
    let object = state
        .as_object_mut()
        .ok_or_else(|| IdentityConflict::InvalidState("app state is not an object".to_owned()))?;
    object.insert(
        IDENTITY_KEY.to_owned(),
        serde_json::to_value(registry)
            .map_err(|error| IdentityConflict::InvalidState(error.to_string()))?,
    );
    Ok(())
}

fn observations_for_state(state: &Value) -> Vec<CheckoutObservation> {
    let registry = read_registry(state).unwrap_or_default();
    project_entries(state, &registry)
        .into_iter()
        .map(|entry| {
            let project_id = entry.project_id;
            let path = entry.path;
            let mut observation = probe_checkout(Path::new(&path));
            observation.project_id = Some(project_id);
            observation
        })
        .collect()
}

pub fn read_registry_at(path: &Path) -> Result<IdentityRegistry, String> {
    let state = unpeel_core::app_state::load_for_edit_at(path)?;
    read_registry(&state).map_err(|error| error.to_string())
}

/// Produce the migration report without writing the state, backup, or journal.
/// Git probes happen before any app-state lock is taken, just as they do for
/// [`reconcile_at`].
pub fn diagnose_at(path: &Path) -> Result<MigrationReport, String> {
    let state = unpeel_core::app_state::load_for_edit_at(path)?;
    let observations = observations_for_state(&state);
    let plan = plan_reconciliation(&state, &observations);
    if plan.unsupported_version.is_some() || plan.invalid_state.is_some() {
        let mut unchanged = state;
        return apply_reconciliation(&mut unchanged, &plan).map_err(|error| error.to_string());
    }
    Ok(report(&plan, plan.changed()))
}

fn conflict_matches_fingerprint(conflict: Option<&str>, expected_old: &str) -> bool {
    conflict.is_some_and(|conflict| {
        [
            "common directory fingerprint changed from ",
            "stable common directory identity changed from ",
        ]
        .iter()
        .filter_map(|prefix| conflict.strip_prefix(prefix))
        .any(|rest| {
            rest.starts_with(expected_old)
                && rest
                    .get(expected_old.len()..)
                    .is_some_and(|suffix| suffix.starts_with(" to "))
        })
    })
}

fn legacy_conflict_targets(conflict: Option<&str>, expected_current: &str) -> bool {
    conflict
        .and_then(|conflict| {
            conflict
                .strip_prefix("common directory fingerprint changed from ")
                .and_then(|transition| transition.rsplit_once(" to "))
        })
        .is_some_and(|(_, current)| current == expected_current)
}

fn observation_matches_fingerprint(observation: &CheckoutObservation, expected: &str) -> bool {
    observation
        .common_dir_stable_fingerprint
        .as_deref()
        .or(observation.common_dir_fingerprint.as_deref())
        == Some(expected)
}

fn repository_matches_fingerprint(repository: &RepositoryIdentity, expected: &str) -> bool {
    repository
        .common_dir_stable_fingerprint
        .as_deref()
        .or(repository.common_dir_fingerprint.as_deref())
        == Some(expected)
}

fn checkout_is_recoverable(checkout: &CheckoutIdentity) -> bool {
    !checkout.archived
        && !["removalPending", "removalInterrupted"]
            .iter()
            .any(|field| checkout.extra.get(*field).and_then(Value::as_bool) == Some(true))
}

fn recovery_observations(
    registry: &IdentityRegistry,
    repository_id: &str,
    repository_common_dir: &str,
    project_id: &str,
    expected_old: &str,
    expected_current: &str,
) -> Result<BTreeMap<String, CheckoutObservation>, String> {
    let mut observations = BTreeMap::new();
    for checkout in registry.checkouts.iter().filter(|checkout| {
        checkout.repository_id.as_deref() == Some(repository_id)
            && checkout_is_recoverable(checkout)
            && (checkout.project_id == project_id
                || conflict_matches_fingerprint(checkout.conflict.as_deref(), expected_old))
    }) {
        let mut observation = probe_checkout(Path::new(&checkout.path));
        observation.project_id = Some(checkout.project_id.clone());
        if observation.availability != CheckoutAvailability::Available {
            return Err(format!(
                "project {} could not be freshly probed; retry after its checkout is available",
                checkout.project_id
            ));
        }
        if observation.common_dir.as_deref() != Some(repository_common_dir) {
            return Err(format!(
                "project {} no longer resolves to repository {}; refusing recovery",
                checkout.project_id, repository_id
            ));
        }
        if !observation_matches_fingerprint(&observation, expected_current) {
            return Err(format!(
                "project {} does not match expected current identity {expected_current}; retry diagnosis",
                checkout.project_id
            ));
        }
        observations.insert(checkout.project_id.clone(), observation);
    }
    observations.get(project_id).ok_or_else(|| {
        format!("project {project_id} is not an active checkout of repository {repository_id}")
    })?;
    Ok(observations)
}

fn recheck_recovery_fingerprints(
    observations: &BTreeMap<String, CheckoutObservation>,
) -> Result<(), String> {
    for observation in observations.values() {
        let common_dir = observation.common_dir.as_deref().ok_or_else(|| {
            format!(
                "project {} lost its common directory before recovery",
                observation.project_id.as_deref().unwrap_or("<unknown>")
            )
        })?;
        let legacy = common_dir_fingerprint(common_dir);
        let stable = common_dir_stable_fingerprint(common_dir);
        let fingerprint_unchanged = if observation.common_dir_stable_fingerprint.is_some() {
            stable == observation.common_dir_stable_fingerprint
        } else {
            legacy == observation.common_dir_fingerprint
        };
        if !fingerprint_unchanged {
            return Err(format!(
                "project {} changed filesystem identity after diagnosis; retry diagnosis",
                observation.project_id.as_deref().unwrap_or("<unknown>")
            ));
        }
    }
    Ok(())
}

fn project_row_path(state: &Value, project_id: &str) -> Option<String> {
    state
        .get("projects")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .find(|project| project.get("id").and_then(Value::as_str) == Some(project_id))
        .and_then(|project| project.get("path").and_then(Value::as_str))
        .map(str::to_owned)
}

fn removal_flags(checkout: &CheckoutIdentity) -> (bool, bool) {
    (
        checkout
            .extra
            .get("removalPending")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        checkout
            .extra
            .get("removalInterrupted")
            .and_then(Value::as_bool)
            .unwrap_or(false),
    )
}

/// Recover a known identity conflict after probing every active matching
/// checkout outside the app-state lock. The guarded edit revalidates the
/// repository, project paths and expected old fingerprint before it clears any
/// conflict, so a stale diagnosis cannot overwrite a newer registration.
pub fn recover_project_identity_at(
    path: &Path,
    project_id: &str,
    expected_old: &str,
    expected_current: &str,
) -> Result<IdentityRecoveryReport, String> {
    if expected_old.is_empty() || expected_current.is_empty() {
        return Err("expected old and current fingerprints are required".to_owned());
    }
    let initial_state = unpeel_core::app_state::load_for_edit_at(path)?;
    let initial_registry = read_registry(&initial_state).map_err(|error| error.to_string())?;
    let target = initial_registry
        .checkout(project_id)
        .ok_or_else(|| format!("unknown project {project_id}"))?;
    let repository_id = target
        .repository_id
        .clone()
        .ok_or_else(|| format!("project {project_id} has no repository identity to recover"))?;
    let repository = initial_registry
        .repository(&repository_id)
        .ok_or_else(|| format!("unknown repository {repository_id}"))?;
    let repository_common_dir = repository
        .common_dir
        .as_deref()
        .ok_or_else(|| format!("repository {repository_id} has no common directory"))?;
    let target_has_matching_conflict =
        conflict_matches_fingerprint(target.conflict.as_deref(), expected_old);
    let repository_is_current = repository_matches_fingerprint(repository, expected_current);
    let repository_is_old = repository_matches_fingerprint(repository, expected_old);
    if !target_has_matching_conflict && !repository_is_current && !repository_is_old {
        return Err(format!(
            "project {project_id} has no conflict or repository fingerprint matching expected old value {expected_old}"
        ));
    }
    if !repository_is_current && !repository_is_old {
        return Err(
            "identity state changed before recovery; repository fingerprint no longer matches the expected old value; retry diagnosis"
                .to_owned(),
        );
    }
    let observations = recovery_observations(
        &initial_registry,
        &repository_id,
        repository_common_dir,
        project_id,
        expected_old,
        expected_current,
    )?;
    let observed_checkouts: BTreeMap<String, CheckoutIdentity> = observations
        .keys()
        .filter_map(|checkout_id| {
            initial_registry
                .checkout(checkout_id)
                .cloned()
                .map(|checkout| (checkout_id.clone(), checkout))
        })
        .collect();
    let observed_project_paths: BTreeMap<String, Option<String>> = observations
        .keys()
        .map(|checkout_id| {
            (
                checkout_id.clone(),
                project_row_path(&initial_state, checkout_id),
            )
        })
        .collect();
    let initial_repository = initial_registry
        .repository(&repository_id)
        .cloned()
        .ok_or_else(|| format!("unknown repository {repository_id}"))?;
    let current_fingerprint = expected_current.to_owned();
    let report = unpeel_core::app_state::edit_at(path, |state| {
        let current = Value::Object(state.clone());
        let mut registry = read_registry(&current).map_err(|error| error.to_string())?;
        let current_target = registry
            .checkout(project_id)
            .ok_or_else(|| format!("project {project_id} disappeared during recovery"))?;
        if current_target.path != target.path {
            return Err(format!(
                "project {project_id} changed path during recovery; retry diagnosis"
            ));
        }
        let current_repository_id = current_target.repository_id.as_deref().ok_or_else(|| {
            format!("project {project_id} lost its repository identity during recovery")
        })?;
        if current_repository_id != repository_id {
            return Err(format!(
                "project {project_id} changed repository during recovery; retry diagnosis"
            ));
        }
        let current_repository = registry
            .repository(&repository_id)
            .ok_or_else(|| format!("repository {repository_id} disappeared during recovery"))?;
        if current_repository.common_dir.as_deref() != Some(repository_common_dir) {
            return Err(format!(
                "repository {repository_id} changed common directory during recovery; retry diagnosis"
            ));
        }
        if current_repository.common_dir_stable_fingerprint
            != initial_repository.common_dir_stable_fingerprint
            || current_repository.common_dir_fingerprint
                != initial_repository.common_dir_fingerprint
            || current_repository.project_ids != initial_repository.project_ids
            || current_repository.primary_project_id != initial_repository.primary_project_id
        {
            return Err(
                "identity state changed during recovery; repository membership or fingerprint changed; retry diagnosis"
                    .to_owned(),
            );
        }
        let repository_is_current =
            repository_matches_fingerprint(current_repository, expected_current);
        if !repository_is_current
            && !repository_matches_fingerprint(current_repository, expected_old)
        {
            return Err(
                "identity state changed during recovery; repository fingerprint no longer matches the expected old value; retry diagnosis"
                    .to_owned(),
            );
        }
        for (checkout_id, observation) in &observations {
            let current_checkout = registry
                .checkout(checkout_id)
                .ok_or_else(|| format!("project {checkout_id} disappeared during recovery"))?;
            let expected_checkout = observed_checkouts
                .get(checkout_id)
                .ok_or_else(|| format!("project {checkout_id} was not in the recovery snapshot"))?;
            if current_checkout.path != expected_checkout.path
                || current_checkout.repository_id != expected_checkout.repository_id
                || current_checkout.archived != expected_checkout.archived
                || current_checkout.availability != expected_checkout.availability
                || current_checkout.conflict != expected_checkout.conflict
                || removal_flags(current_checkout) != removal_flags(expected_checkout)
            {
                return Err(format!(
                    "project {checkout_id} identity flags changed during recovery; retry diagnosis"
                ));
            }
            if project_row_path(&current, checkout_id)
                != observed_project_paths.get(checkout_id).cloned().flatten()
            {
                return Err(format!(
                    "project {checkout_id} changed its app-state row during recovery; retry diagnosis"
                ));
            }
            if current_checkout.path != observation.path {
                return Err(format!(
                    "project {checkout_id} changed path during recovery; retry diagnosis"
                ));
            }
        }
        let current_matching_ids = registry
            .checkouts
            .iter()
            .filter(|checkout| {
                checkout.repository_id.as_deref() == Some(&repository_id)
                    && checkout_is_recoverable(checkout)
                    && conflict_matches_fingerprint(checkout.conflict.as_deref(), expected_old)
            })
            .map(|checkout| checkout.project_id.clone())
            .collect::<Vec<_>>();
        // The state lock may have been contended since the Git probes. Verify
        // filesystem identity at the guarded commit point as well.
        recheck_recovery_fingerprints(&observations)?;
        if current_matching_ids.is_empty() && repository_is_current {
            return Ok(IdentityRecoveryReport {
                project_id: project_id.to_owned(),
                repository_id: repository_id.clone(),
                repaired_project_ids: Vec::new(),
                current_fingerprint: current_fingerprint.clone(),
                changed: false,
            });
        }
        if current_matching_ids
            .iter()
            .any(|checkout_id| !observations.contains_key(checkout_id))
        {
            return Err(
                "identity state changed during recovery; a new matching conflict appeared; retry diagnosis"
                    .to_owned(),
            );
        }
        let before = serde_json::to_value(&registry).map_err(|error| error.to_string())?;
        let selected_observation = observations
            .get(project_id)
            .ok_or_else(|| format!("project {project_id} was not freshly observed"))?;
        let repository = registry
            .repositories
            .iter_mut()
            .find(|repository| repository.id == repository_id)
            .ok_or_else(|| format!("repository {repository_id} disappeared during recovery"))?;
        if let Some(fingerprint) = selected_observation.common_dir_fingerprint.as_ref() {
            repository.common_dir_fingerprint = Some(fingerprint.clone());
        }
        if selected_observation.common_dir_stable_fingerprint.is_some() {
            repository.common_dir_stable_fingerprint =
                selected_observation.common_dir_stable_fingerprint.clone();
        }
        if !repository_is_current {
            repository.last_seen_unix_ms = Some(selected_observation.observed_at_unix_ms);
        }

        let mut repaired_project_ids = Vec::new();
        for checkout in &mut registry.checkouts {
            if checkout.repository_id.as_deref() != Some(&repository_id)
                || !checkout_is_recoverable(checkout)
                || !conflict_matches_fingerprint(checkout.conflict.as_deref(), expected_old)
            {
                continue;
            }
            let observation = observations.get(&checkout.project_id).ok_or_else(|| {
                format!(
                    "project {} was not freshly verified; refusing partial recovery",
                    checkout.project_id
                )
            })?;
            checkout.conflict = None;
            checkout.observed_common_dir = observation.common_dir.clone();
            checkout.availability = CheckoutAvailability::Available;
            if !repository_is_current {
                checkout.last_observed_unix_ms = Some(observation.observed_at_unix_ms);
            }
            repaired_project_ids.push(checkout.project_id.clone());
        }
        repaired_project_ids.sort();
        let after = serde_json::to_value(&registry).map_err(|error| error.to_string())?;
        let changed = before != after;
        state.insert(IDENTITY_KEY.to_owned(), after);
        Ok(IdentityRecoveryReport {
            project_id: project_id.to_owned(),
            repository_id: repository_id.clone(),
            repaired_project_ids,
            current_fingerprint: current_fingerprint.clone(),
            changed,
        })
    })?;
    if path == unpeel_core::app_paths::app_state_path() && report.changed {
        unpeel_core::app_state::announce_app_state_changed();
    }
    Ok(report)
}

/// Schedule a bounded reconciliation for the current state file. Bootstrap
/// polling must stay cheap, so the app invokes this from client construction
/// and a per-path TTL prevents one Git process per UI poll. Explicit actions
/// use [`reconcile_at`] synchronously when they need a fresh answer.
pub fn schedule_background_reconcile() {
    // Integration tests and library consumers can construct a client while
    // pointing UNPEEL_HOME at a fixture. Only the actual Comet executable may
    // start a background writer; explicit `reconcile_at` remains available to
    // those callers and is synchronous.
    if !crate::hook_migration::is_comet_application_process() {
        return;
    }
    struct BackgroundRun {
        started: Instant,
        running: bool,
    }

    const REFRESH_TTL: Duration = Duration::from_secs(30);
    static LAST_START: OnceLock<Mutex<BTreeMap<PathBuf, BackgroundRun>>> = OnceLock::new();
    let path = unpeel_core::app_paths::app_state_path();
    let now = Instant::now();
    let state = LAST_START.get_or_init(|| Mutex::new(BTreeMap::new()));
    let should_start = state
        .lock()
        .ok()
        .map(|mut entries| {
            let entry = entries.entry(path.clone()).or_insert(BackgroundRun {
                started: now.checked_sub(REFRESH_TTL).unwrap_or(now),
                running: false,
            });
            if entry.running || now.duration_since(entry.started) < REFRESH_TTL {
                return false;
            }
            entry.started = now;
            entry.running = true;
            true
        })
        .unwrap_or(false);
    if !should_start {
        return;
    }
    let worker_path = path.clone();
    let spawn = std::thread::Builder::new()
        .name("comet-project-identity-reconcile".to_owned())
        .spawn(move || {
            if let Err(error) = reconcile_at(&worker_path) {
                unpeel_core::hook_assets::append_trace_log_line(&format!(
                    "Comet project identity reconciliation failed: {error}"
                ));
            }
            if let Ok(mut entries) = state.lock() {
                if let Some(entry) = entries.get_mut(&worker_path) {
                    entry.running = false;
                }
            }
        });
    if let Err(error) = spawn {
        if let Ok(mut entries) = state.lock() {
            if let Some(entry) = entries.get_mut(&path) {
                entry.running = false;
            }
        }
        unpeel_core::hook_assets::append_trace_log_line(&format!(
            "Comet project identity reconciliation could not start: {error}"
        ));
    }
}

pub fn reconcile_at(path: &Path) -> Result<MigrationReport, String> {
    let state = unpeel_core::app_state::load_for_edit_at(path)?;
    let input_snapshot = reconciliation_input_snapshot(&state);
    let observations = observations_for_state(&state);
    let plan = plan_reconciliation(&state, &observations);
    if plan.unsupported_version.is_some() || plan.invalid_state.is_some() {
        let mut unchanged = state.clone();
        return apply_reconciliation(&mut unchanged, &plan).map_err(|error| error.to_string());
    }
    if !plan.changed() {
        return Ok(report(&plan, false));
    }
    let backup_path = path.with_extension("identity-backup.json");
    let journal_path = path.with_extension("identity-journal.jsonl");
    let result = unpeel_core::app_state::edit_at(path, |state| {
        let current = Value::Object(state.clone());
        validate_reconciliation_inputs(&current, &input_snapshot)?;
        for patch in &plan.patches {
            let current_path = state
                .get("projects")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .find(|project| {
                    project.get("id").and_then(Value::as_str) == Some(patch.project_id.as_str())
                })
                .and_then(|project| project.get("path").and_then(Value::as_str));
            if current_path.is_some() && current_path != Some(patch.observation.path.as_str()) {
                return Err(format!(
                    "project {} changed path while identity was being reconciled",
                    patch.project_id
                ));
            }
        }
        let current_plan = plan_reconciliation(&current, &observations);
        let mut current_value = current.clone();
        let report = apply_reconciliation(&mut current_value, &current_plan)
            .map_err(|error| error.to_string())?;
        if report.changed {
            if path.exists() {
                write_backup_if_absent(&backup_path, &current)?;
            }
            append_journal(&journal_path, &current, &current_plan)?;
        }
        *state = current_value
            .as_object()
            .cloned()
            .ok_or_else(|| "identity state is not an object".to_owned())?;
        Ok(report)
    });
    if path == unpeel_core::app_paths::app_state_path() {
        if result.as_ref().is_ok_and(|report| report.changed) {
            unpeel_core::app_state::announce_app_state_changed();
        }
    }
    result
}

/// Roll back the last successful reconciliation's identity namespace while
/// preserving every current project, session, and unknown top-level key.
///
/// The journal contains before/after identity snapshots specifically for this
/// compare-and-swap. If any identity decision changed after the reconciliation
/// (manual association, archive, a later probe, or a new checkout), rollback
/// refuses rather than reverting that newer decision.
pub fn rollback_identity_at(path: &Path) -> Result<bool, String> {
    let journal_path = path.with_extension("identity-journal.jsonl");
    let result = unpeel_core::app_state::edit_at(path, |state| {
        let record = latest_reconciliation_record(&journal_path)?;
        let after = record
            .get("afterIdentity")
            .ok_or_else(|| "identity journal has no after snapshot".to_owned())?;
        if state.get(IDENTITY_KEY) != Some(after) {
            return Err(
                "identity state changed after reconciliation; refusing rollback".to_owned(),
            );
        }
        let before_present = record
            .get("beforeIdentityPresent")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        if before_present {
            let before = record
                .get("beforeIdentity")
                .ok_or_else(|| "identity journal has no before snapshot".to_owned())?;
            state.insert(IDENTITY_KEY.to_owned(), before.clone());
        } else {
            state.remove(IDENTITY_KEY);
        }
        Ok(true)
    });
    if path == unpeel_core::app_paths::app_state_path()
        && result.as_ref().is_ok_and(|changed| *changed)
    {
        unpeel_core::app_state::announce_app_state_changed();
    }
    result
}

fn latest_reconciliation_record(path: &Path) -> Result<Value, String> {
    let raw = std::fs::read_to_string(path)
        .map_err(|error| format!("journal {}: {error}", path.display()))?;
    let mut latest = None;
    for line in raw.lines().filter(|line| !line.trim().is_empty()) {
        let value: Value = serde_json::from_str(line)
            .map_err(|error| format!("journal {}: {error}", path.display()))?;
        if value.get("operation").and_then(Value::as_str) == Some("reconcile") {
            latest = Some(value);
        }
    }
    latest.ok_or_else(|| "identity journal has no reconciliation record".to_owned())
}

fn write_backup_if_absent(path: &Path, state: &Value) -> Result<(), String> {
    use std::io::Write;
    let body = serde_json::to_vec_pretty(state).map_err(|error| error.to_string())?;
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    match options.open(path) {
        Ok(mut file) => {
            file.write_all(&body)
                .and_then(|_| file.sync_all())
                .map_err(|error| format!("backup {}: {error}", path.display()))?;
            restrict_backup_permissions(path);
            Ok(())
        }
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => Ok(()),
        Err(error) => Err(format!("backup {}: {error}", path.display())),
    }
}

fn append_journal(
    path: &Path,
    before_state: &Value,
    plan: &ReconciliationPlan,
) -> Result<(), String> {
    let entries: Vec<Value> = plan
        .patches
        .iter()
        .map(|patch| {
            serde_json::json!({
                "projectID": patch.project_id,
                "repositoryID": patch.repository_id,
                "path": patch.observation.path,
                "availability": patch.observation.availability,
                "conflict": patch.conflict,
            })
        })
        .collect();
    let line = serde_json::json!({
        "operation": "reconcile",
        "version": IDENTITY_SCHEMA_VERSION,
        "atUnixMs": now_unix_ms(),
        "beforeIdentityPresent": before_state.get(IDENTITY_KEY).is_some(),
        "beforeIdentity": before_state.get(IDENTITY_KEY),
        "afterIdentity": serde_json::to_value(&plan.next_registry)
            .map_err(|error| error.to_string())?,
        "patches": entries,
    });
    use std::io::Write;
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|error| format!("journal {}: {error}", path.display()))?;
    writeln!(file, "{line}").map_err(|error| format!("journal {}: {error}", path.display()))
}

fn restrict_backup_permissions(path: &Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::process::Command;
    use tempfile::TempDir;

    fn git(path: &Path, args: &[&str]) {
        let status = Command::new("git")
            .arg("-C")
            .arg(path)
            .args(args)
            .status()
            .expect("git starts");
        assert!(status.success(), "git {args:?} failed");
    }

    fn fixture() -> (TempDir, PathBuf, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let main = dir.path().join("main repo");
        let worktree = dir.path().join("feature checkout");
        fs::create_dir_all(&main).unwrap();
        git(&main, &["init", "-q"]);
        git(&main, &["config", "user.email", "test@example.com"]);
        git(&main, &["config", "user.name", "Test"]);
        fs::write(main.join("README"), "fixture\n").unwrap();
        git(&main, &["add", "."]);
        git(&main, &["commit", "-qm", "initial"]);
        git(
            &main,
            &[
                "worktree",
                "add",
                "-q",
                "-b",
                "feature/probe",
                worktree.to_str().unwrap(),
            ],
        );
        (dir, main, worktree)
    }

    #[test]
    fn probe_distinguishes_primary_and_linked_checkout_without_remote() {
        let (_dir, main, worktree) = fixture();
        let primary = probe_checkout(&main);
        let linked = probe_checkout(&worktree);
        assert_eq!(primary.availability, CheckoutAvailability::Available);
        assert_eq!(primary.kind, CheckoutKind::Primary);
        assert_eq!(linked.kind, CheckoutKind::Linked);
        assert_eq!(linked.branch.as_deref(), Some("feature/probe"));
        assert_eq!(linked.common_dir, primary.common_dir);
        assert!(linked.remotes.is_empty());
    }

    #[test]
    fn plan_preserves_unknown_state_and_sessions() {
        let (_dir, main, worktree) = fixture();
        let mut state = serde_json::json!({
            "projects": [
                {"id": "main", "name": "main", "path": main},
                {"id": "feature", "name": "feature", "path": worktree, "is_folder": true, "parent_project_id": "main", "worktree_branch": "feature/probe"}
            ],
            "sessions": [{"id": "session-main"}, {"id": "session-feature"}],
            "futureKey": {"kept": true}
        });
        let mut primary = probe_checkout(Path::new(state["projects"][0]["path"].as_str().unwrap()));
        primary.project_id = Some("main".to_owned());
        let mut linked = probe_checkout(Path::new(state["projects"][1]["path"].as_str().unwrap()));
        linked.project_id = Some("feature".to_owned());
        let plan = plan_reconciliation(&state, &[primary, linked]);
        let report = apply_reconciliation(&mut state, &plan).unwrap();
        assert_eq!(report.associated, 2);
        assert_eq!(state["sessions"][1]["id"], "session-feature");
        assert_eq!(state["futureKey"]["kept"], true);
        let registry: IdentityRegistry =
            serde_json::from_value(state[IDENTITY_KEY].clone()).unwrap();
        assert_eq!(
            registry.checkout("main").unwrap().repository_id,
            registry.checkout("feature").unwrap().repository_id
        );
    }

    #[test]
    fn missing_checkout_keeps_identity_and_last_branch() {
        let (_dir, _main, worktree) = fixture();
        let mut state = serde_json::json!({
            "projects": [{"id": "feature", "name": "feature", "path": worktree}],
            "comet_project_identity": {
                "version": 1,
                "repositories": [{"id": "repo-1", "commonDir": "/repo/.git", "projectIds": ["feature"]}],
                "checkouts": [{"projectID": "feature", "checkoutID": "feature", "repositoryID": "repo-1", "path": worktree, "kind": "linked", "ownership": "external", "availability": "available", "branch": "feature/probe", "lastKnownBranch": "feature/probe"}]
            },
            "sessions": [{"id": "kept"}]
        });
        fs::remove_dir_all(&worktree).unwrap();
        let observation = CheckoutObservation {
            project_id: Some("feature".to_owned()),
            ..probe_checkout(&worktree)
        };
        let plan = plan_reconciliation(&state, &[observation]);
        apply_reconciliation(&mut state, &plan).unwrap();
        let registry: IdentityRegistry =
            serde_json::from_value(state[IDENTITY_KEY].clone()).unwrap();
        let checkout = registry.checkout("feature").unwrap();
        assert_eq!(checkout.repository_id.as_deref(), Some("repo-1"));
        assert_eq!(checkout.availability, CheckoutAvailability::Missing);
        assert_eq!(checkout.last_known_branch.as_deref(), Some("feature/probe"));
        assert_eq!(state["sessions"][0]["id"], "kept");
    }

    #[test]
    fn repeated_plan_after_apply_is_stable() {
        let (_dir, main, _worktree) = fixture();
        let mut state = serde_json::json!({
            "projects": [{"id": "main", "name": "main", "path": main}]
        });
        let mut observation = probe_checkout(&main);
        observation.project_id = Some("main".to_owned());
        let plan = plan_reconciliation(&state, &[observation.clone()]);
        apply_reconciliation(&mut state, &plan).unwrap();
        let second = plan_reconciliation(&state, &[observation]);
        assert!(!second.changed());
    }

    #[test]
    fn stable_identity_recovers_all_legacy_siblings_but_blocks_replacement() {
        let (_dir, main, worktree) = fixture();
        let mut state = serde_json::json!({"projects": [
            {"id": "main", "path": main}, {"id": "child", "path": worktree}
        ]});
        let mut observations = vec![probe_checkout(&main), probe_checkout(&worktree)];
        for (observation, id) in observations.iter_mut().zip(["main", "child"]) {
            observation.project_id = Some(id.to_owned());
            observation.common_dir_fingerprint = Some("unix:1:9".into());
            observation.common_dir_stable_fingerprint = Some("macos:volume-a:9".into());
        }
        let initial = plan_reconciliation(&state, &observations);
        apply_reconciliation(&mut state, &initial).unwrap();
        let mut with_missing_sibling = observations.clone();
        with_missing_sibling[1].availability = CheckoutAvailability::Missing;
        with_missing_sibling[1].common_dir = None;
        with_missing_sibling[1].common_dir_fingerprint = None;
        with_missing_sibling[1].common_dir_stable_fingerprint = None;
        let missing = plan_reconciliation(&state, &with_missing_sibling);
        assert_eq!(
            missing.next_registry.repositories[0]
                .common_dir_stable_fingerprint
                .as_deref(),
            Some("macos:volume-a:9")
        );
        for checkout in state[IDENTITY_KEY]["checkouts"].as_array_mut().unwrap() {
            checkout["conflict"] =
                "common directory fingerprint changed from unix:1:9 to unix:2:9".into();
        }
        for observation in &mut observations {
            observation.common_dir_fingerprint = Some("unix:2:9".into());
        }
        let remounted = plan_reconciliation(&state, &observations);
        assert!(
            remounted
                .next_registry
                .checkouts
                .iter()
                .all(|c| c.conflict.is_none())
        );
        apply_reconciliation(&mut state, &remounted).unwrap();
        assert_eq!(
            state[IDENTITY_KEY]["repositories"][0]["commonDirFingerprint"],
            "unix:2:9"
        );
        for observation in &mut observations {
            observation.common_dir_stable_fingerprint = Some("macos:volume-b:9".into());
        }
        let replaced = plan_reconciliation(&state, &observations);
        assert!(replaced.next_registry.checkouts.iter().all(|c| {
            c.conflict
                .as_deref()
                .unwrap()
                .contains("stable common directory identity changed")
        }));
    }

    #[test]
    fn temporary_stable_probe_failure_clears_only_its_own_conflict() {
        let (_dir, main, _) = fixture();
        let mut state = serde_json::json!({"projects": [{"id": "main", "path": main}]});
        let mut observed = probe_checkout(&main);
        observed.project_id = Some("main".into());
        observed.common_dir_stable_fingerprint = Some("macos:volume-a:9".into());
        let initial = plan_reconciliation(&state, &[observed.clone()]);
        apply_reconciliation(&mut state, &initial).unwrap();
        let mut unavailable = observed.clone();
        unavailable.common_dir_stable_fingerprint = None;
        let failed = plan_reconciliation(&state, &[unavailable]);
        apply_reconciliation(&mut state, &failed).unwrap();
        assert_eq!(
            state[IDENTITY_KEY]["checkouts"][0]["conflict"],
            "stable common directory identity is unavailable"
        );
        let restored = plan_reconciliation(&state, &[observed.clone()]);
        assert!(
            restored
                .next_registry
                .checkout("main")
                .unwrap()
                .conflict
                .is_none()
        );
        state[IDENTITY_KEY]["checkouts"][0]["conflict"] = "manual conflict".into();
        let mut unavailable_again = observed.clone();
        unavailable_again.common_dir_stable_fingerprint = None;
        let manual_unavailable = plan_reconciliation(&state, &[unavailable_again]);
        apply_reconciliation(&mut state, &manual_unavailable).unwrap();
        assert_eq!(
            state[IDENTITY_KEY]["checkouts"][0]["conflict"],
            "manual conflict"
        );
        let manual = plan_reconciliation(&state, &[observed]);
        assert_eq!(
            manual
                .next_registry
                .checkout("main")
                .unwrap()
                .conflict
                .as_deref(),
            Some("manual conflict")
        );
    }

    #[test]
    fn read_only_diagnosis_can_recover_without_background_reconciliation() {
        let (dir, main, _) = fixture();
        let path = dir.path().join("app-state.json");
        let mut state =
            serde_json::json!({"projects": [{"id": "main", "path": main}], "sentinel": 42});
        let mut observed = probe_checkout(&main);
        observed.project_id = Some("main".into());
        let initial = plan_reconciliation(&state, &[observed]);
        apply_reconciliation(&mut state, &initial).unwrap();
        state[IDENTITY_KEY]["repositories"][0]["commonDirStableFingerprint"] = Value::Null;
        state[IDENTITY_KEY]["repositories"][0]["commonDirFingerprint"] = "unix:0:old".into();
        let raw = serde_json::to_vec(&state).unwrap();
        fs::write(&path, &raw).unwrap();
        let diagnosis = diagnose_at(&path).unwrap();
        assert_eq!(fs::read(&path).unwrap(), raw);
        let candidate = &diagnosis.recovery_candidates[0];
        let recovery = recover_project_identity_at(
            &path,
            "main",
            &candidate.expected_old_fingerprint,
            &candidate.expected_current_fingerprint,
        )
        .unwrap();
        assert!(recovery.changed);
        let saved: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        assert_eq!(saved["sentinel"], 42);
        assert!(diagnose_at(&path).unwrap().recovery_candidates.is_empty());
    }

    #[test]
    fn attached_observation_clears_previous_detached_marker() {
        let (_dir, main, _worktree) = fixture();
        let state = serde_json::json!({
            "projects": [{"id": "main", "name": "main", "path": main}],
            "comet_project_identity": {
                "version": 1,
                "repositories": [],
                "checkouts": [{
                    "projectID": "main",
                    "checkoutID": "main",
                    "kind": "primary",
                    "ownership": "external",
                    "availability": "available",
                    "detached": true
                }]
            }
        });
        let mut observation = probe_checkout(&main);
        observation.project_id = Some("main".to_owned());
        let plan = plan_reconciliation(&state, &[observation]);
        assert!(!plan.next_registry.checkout("main").unwrap().detached);
    }

    #[test]
    fn reconciliation_rejects_new_project_or_ledger_after_probe() {
        let mut state = serde_json::json!({
            "projects": [{"id": "main", "path": "/tmp/main"}],
            "comet_projects": [{"path": "/tmp/main"}],
            "sessions": [{"id": "updated-while-probing"}]
        });
        let expected = reconciliation_input_snapshot(&state);
        let mut with_session_update = state.clone();
        with_session_update["sessions"][0]["id"] = "new-session".into();
        assert!(validate_reconciliation_inputs(&with_session_update, &expected).is_ok());

        state["projects"]
            .as_array_mut()
            .unwrap()
            .push(serde_json::json!({"id": "new", "path": "/tmp/new"}));
        assert!(validate_reconciliation_inputs(&state, &expected).is_err());

        state["projects"].as_array_mut().unwrap().pop();
        state["comet_projects"]
            .as_array_mut()
            .unwrap()
            .push(serde_json::json!({"path": "/tmp/history-only"}));
        assert!(validate_reconciliation_inputs(&state, &expected).is_err());
    }

    #[test]
    fn future_identity_version_is_not_reinterpreted() {
        let state = serde_json::json!({
            "projects": [],
            "comet_project_identity": {"version": 99, "future": true}
        });
        let plan = plan_reconciliation(&state, &[]);
        assert_eq!(plan.next_registry.version, IDENTITY_SCHEMA_VERSION);
        // The pure planner is conservative for a version it cannot parse: no
        // caller should apply its fallback registry to this state.
        assert_eq!(state[IDENTITY_KEY]["version"], 99);
    }
}
