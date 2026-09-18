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
pub struct MigrationReport {
    pub schema_version: u32,
    pub examined: usize,
    pub associated: usize,
    pub preserved: usize,
    pub missing: usize,
    pub pending: usize,
    pub conflicts: usize,
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
        let previous_fingerprint = registry
            .repository(repository_id)
            .and_then(|repository| repository.common_dir_fingerprint.as_deref());
        if let (Some(previous), Some(current)) = (
            previous_fingerprint,
            observation.common_dir_fingerprint.as_deref(),
        ) {
            if previous != current {
                conflict = Some(format!(
                    "common directory fingerprint changed from {previous} to {current}"
                ));
            }
        }
    }
    if repository_id.is_none() && observation.common_dir.is_some() && !manually_unlinked {
        let id = generated_id("repository");
        registry.repositories.push(RepositoryIdentity {
            id: id.clone(),
            common_dir: observation.common_dir.clone(),
            common_dir_fingerprint: observation.common_dir_fingerprint.clone(),
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
