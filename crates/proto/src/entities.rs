//! Synced entity rows (workspace doc) and local projections.
//!
//! In zeron these were synced Postgres rows; in zeron they live in the per-org
//! workspace Loro doc (see ARCHITECTURE.md §2.2) with the same field surface.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::{ContextUsage, HarnessId, ReasoningLevel, SandboxLevel};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Device {
    pub id: String,
    pub name: String,
    pub platform: String,
    pub last_seen_at: Option<DateTime<Utc>>,
    /// First registration time (zeron devices.created_at — the Devices page
    /// "Added …" fragment). Optional so pre-existing docs stay readable.
    #[serde(default)]
    pub created_at: Option<DateTime<Utc>>,
    /// App version the device's engine last booted with — fleet staleness at a
    /// glance (Devices page). Optional so pre-existing docs stay readable.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
}

/// A synced (device, folder) pair — the unit of organization in the sidebar.
/// Sessions belong to exactly one space; the space fixes their host device and
/// base cwd. Folders need not be git repos: `git_detected` is stamped by the
/// owning device (SpacesSync) and gates branch pickers / the diff sidebar on
/// every device without an RPC.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Space {
    pub id: String,
    /// Owning device — fixed at create, immutable.
    pub device_id: String,
    /// Absolute folder path on the owning device.
    pub path: String,
    /// User rename; absent ⇒ display = basename(path).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Owner-stamped: is `path` inside a git work tree?
    #[serde(default)]
    pub git_detected: bool,
    /// Owner-stamped freshness timestamp of the last git check.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub git_checked_at: Option<DateTime<Utc>>,
    /// Owner-stamped when git: canonical checkout identity of the space root
    /// (sha256(deviceId ‖ NUL ‖ git_dir)) — diff grouping key for root sessions.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub checkout_id: Option<String>,
    pub created_at: DateTime<Utc>,
}

impl Space {
    /// Name override, else basename(path), else the path itself.
    /// Lives here (proto) so UI and engine agree on the derivation.
    pub fn display_name(&self) -> &str {
        if let Some(name) = self.name.as_deref()
            && !name.trim().is_empty()
        {
            return name;
        }
        let trimmed = self.path.trim_end_matches(['/', '\\']);
        trimmed
            .rsplit(['/', '\\'])
            .next()
            .filter(|s| !s.is_empty())
            .unwrap_or(&self.path)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatConfig {
    pub harness: HarnessId,
    pub model: Option<String>,
    pub reasoning: Option<ReasoningLevel>,
    #[serde(default)]
    pub model_options: serde_json::Map<String, serde_json::Value>,
    pub sandbox: SandboxLevel,
}

/// Immutable-at-run-start repository context owned by one conversation.
///
/// This is deliberately separate from the live checkout snapshot: another
/// chat may change the branch at the same checkout without changing which
/// branch this conversation belongs to.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConversationSourceContext {
    pub checkout_id: String,
    pub repo_root: String,
    pub cwd: String,
    pub branch: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub head_sha: Option<String>,
    pub observed_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Chat {
    pub id: String,
    /// Owning (host) device.
    pub device_id: String,
    pub title: Option<String>,
    pub archived: bool,
    pub cwd: Option<String>,
    pub branch: Option<String>,
    /// Canonical id of the repo checkout/worktree this chat operates in.
    pub checkout_id: Option<String>,
    /// Repository identity captured for this conversation immediately before
    /// its harness run. Unlike `branch`, this is never inferred from another
    /// chat sharing the same checkout.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_context: Option<ConversationSourceContext>,
    pub config: Option<ChatConfig>,
    pub last_message_preview: Option<String>,
    pub last_message_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    /// Harness-native session id of the chat's latest run — engine-owned resume
    /// continuity across engine restarts (zeron's `chats.harness_session_id`).
    /// Empty string = explicit
    /// "do not resume" tombstone after a rejected resume.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub harness_session_id: Option<String>,
    /// Cwd the harness session was created under. Harness session stores are
    /// cwd-scoped (claude keys conversations by project directory), so resume
    /// is only injected when the next run launches from the same cwd.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub harness_session_cwd: Option<String>,
    /// The space this chat belongs to. Invariant: `Some` for every UI-created
    /// chat; rows with a missing/dangling space id are not rendered (the host
    /// device's repair sweep deletes its own danglers).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub space_id: Option<String>,
    /// Synced LWW seen marker — compared against `last_message_at` to derive
    /// the "completed (finished but unseen)" indicator. Reading a chat on any
    /// device clears the badge everywhere.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_seen_at: Option<DateTime<Utc>>,
    /// Which sync room generation serves this chat (docs/chat2-sync.md M2):
    /// `None`/1 = legacy s2 loro room, 2 = chat2 dumb relay. The HOST flips
    /// this in the same breath as seeding the chat2 checkpoint; every device
    /// dials the room the registry names. Per-chat and instantly revertible.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub room_gen: Option<u32>,
}

impl Chat {
    /// True when this chat syncs over the chat2 dumb relay.
    pub fn on_chat2(&self) -> bool {
        self.room_gen.unwrap_or(1) >= 2
    }
}

impl Chat {
    /// True when the chat has activity the user hasn't seen on any device.
    pub fn unseen(&self) -> bool {
        match (self.last_message_at, self.last_seen_at) {
            (Some(msg), Some(seen)) => msg > seen,
            (Some(_), None) => true,
            (None, _) => false,
        }
    }
}

/// Display status for a chat row/tab: the four user-facing states plus a
/// distinct Errored. Derived — never stored.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ChatIndicator {
    Working,
    AwaitingInput,
    Errored,
    /// Finished running (or errored out) but not seen yet on any device.
    Completed,
    Idle,
}

/// Derive the display status. `live` must already be staleness-gated by the
/// caller (the UI's 45s window) — pass `None` for a stale/absent session row.
pub fn chat_indicator(chat: &Chat, live: Option<&Session>) -> ChatIndicator {
    match live.map(|s| s.status) {
        Some(SessionStatus::Working) => ChatIndicator::Working,
        Some(SessionStatus::AwaitingInput) => ChatIndicator::AwaitingInput,
        Some(SessionStatus::Errored) if chat.unseen() => ChatIndicator::Errored,
        _ if chat.unseen() => ChatIndicator::Completed,
        _ => ChatIndicator::Idle,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SessionStatus {
    Idle,
    Working,
    AwaitingInput,
    Errored,
}

/// Live run status for a chat — drives the Working indicator and sidebar status dots.
/// Staleness-checked client-side against `updated_at` so a crashed backend never shows
/// an eternal "Working".
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Session {
    /// Last successfully completed assistant turn. Retained while the next turn
    /// runs so coalesced status watches do not lose normal queue completions.
    /// Interrupts, failures and liveness expiry never advance this marker.
    #[serde(default)]
    pub last_completed_turn: Option<String>,
    pub chat_id: String,
    pub device_id: String,
    pub status: SessionStatus,
    pub started_at: Option<DateTime<Utc>>,
    pub updated_at: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_usage: Option<ContextUsage>,
    /// Why the last run ended in [`SessionStatus::Errored`] — the harness's own
    /// message, mirrored onto the row for the status strip. Any non-error
    /// transition clears it, so a stale reason never outlives its run.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Repo {
    pub path: String,
    pub name: String,
    pub default_branch: Option<String>,
}

/// One row of `ListRefs`: a branch plus its checkout state — whether it is
/// the repo's current (main-checkout) branch and whether it is materialized
/// as a linked worktree. Drives the composer's ref picker (`current` /
/// `worktree` tags) and the checkout-kind selector.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RepoRef {
    pub name: String,
    /// Checked out in the repo's MAIN folder right now.
    #[serde(default)]
    pub current: bool,
    /// Path of the linked worktree this branch is checked out in, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub worktree_path: Option<String>,
    /// Whether this ref is from a remote tracking branch.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub is_remote: Option<bool>,
    /// Whether this ref is the default branch of the repository.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub is_default: Option<bool>,
}

/// Public Git reference attached to a commit in the history graph.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum GitHistoryRefKind {
    Branch,
    Remote,
    Tag,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GitHistoryRef {
    pub kind: GitHistoryRefKind,
    pub label: String,
}

/// One topologically ordered row in the repository history graph.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GitHistoryCommit {
    pub sha: String,
    pub parent_shas: Vec<String>,
    pub subject: String,
    pub author_name: String,
    pub author_email: String,
    pub authored_at: String,
    #[serde(default)]
    pub refs: Vec<GitHistoryRef>,
}

/// Read-only search over the public history on the target device.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchGitHistoryParams {
    pub cwd: String,
    pub query: String,
    #[serde(default)]
    pub cursor: usize,
    #[serde(default = "default_history_page_limit")]
    pub limit: usize,
}

fn default_history_page_limit() -> usize {
    100
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitAvatarAuthor {
    pub sha: String,
    pub email: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolveGitAvatarsParams {
    pub cwd: String,
    pub authors: Vec<GitAvatarAuthor>,
    #[serde(default)]
    pub cursor: usize,
    #[serde(default = "default_history_page_limit")]
    pub limit: usize,
}

/// Divergence between the checked-out branch and the repository's integration
/// branch. Counts are computed only from locally available refs; callers must
/// fetch explicitly when they want newer remote state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GitHistoryComparison {
    /// The local remote-tracking ref used as the comparison base, e.g.
    /// `upstream/main`.
    pub base: String,
    /// Commits reachable from HEAD but not from [`Self::base`].
    pub ahead: usize,
    /// Commits reachable from [`Self::base`] but not from HEAD.
    pub behind: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GitHistoryPage {
    pub commits: Vec<GitHistoryCommit>,
    /// Deduplicated tips of every public local/remote branch. Populated with
    /// the first page so clients can switch to the compact overview without
    /// another round trip or loading the complete history.
    #[serde(default)]
    pub branch_tips: Vec<GitHistoryCommit>,
    pub head_sha: Option<String>,
    pub next_cursor: Option<usize>,
    pub total_count: Option<usize>,
    /// Number of commits reachable from the active checkout's HEAD.
    #[serde(default)]
    pub head_commit_count: Option<usize>,
    /// Current branch divergence from the preferred integration branch.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub comparison: Option<GitHistoryComparison>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Worktree {
    pub repo_path: String,
    pub path: String,
    pub branch: String,
    /// Generated worktree folder name (`zeron/<name>` is its branch).
    #[serde(default)]
    pub name: String,
    /// Canonical checkout identity (device-scoped hash of the git dir).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub checkout_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FolderEntry {
    pub name: String,
    pub is_dir: bool,
    pub is_repo: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FolderListing {
    pub path: String,
    pub entries: Vec<FolderEntry>,
    /// True when the listing hit the entry cap.
    #[serde(default)]
    pub truncated: bool,
}

/// A browse root beyond home: a mounted drive/volume (or the system root).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DriveEntry {
    /// Display name (volume label / mount folder name; "System" for `/`).
    pub name: String,
    /// Absolute mount point.
    pub path: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DriveListing {
    pub drives: Vec<DriveEntry>,
}

/// A workspace-relative file or directory returned by `SearchFiles`.
/// Contents deliberately never cross this boundary: mentioning a path leaves
/// the harness to read it through its normal workspace tools when needed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileSearchMatch {
    pub path: String,
    pub is_dir: bool,
}

/// Identifies the local checkout used by workspace file operations.
/// Exactly one of `chat_id` and `space_id` must be present.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceTarget {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chat_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub space_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub checkout_path: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListWorkspaceDirectoryRequest {
    #[serde(flatten)]
    pub target: WorkspaceTarget,
    #[serde(default)]
    pub directory: String,
    #[serde(default)]
    pub include_ignored: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cursor: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceDirectoryPage {
    pub directory: String,
    pub entries: Vec<WorkspaceEntry>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
    pub truncated: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceEntry {
    pub path: String,
    pub name: String,
    pub kind: WorkspaceEntryKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub modified_at: Option<DateTime<Utc>>,
    pub ignored: bool,
    pub read_only: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum WorkspaceEntryKind {
    File,
    Directory,
    Symlink,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchWorkspaceFilesRequest {
    #[serde(flatten)]
    pub target: WorkspaceTarget,
    pub query: String,
    #[serde(default)]
    pub include_ignored: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<u16>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceFileSearchMatch {
    pub path: String,
    pub name: String,
    pub kind: WorkspaceEntryKind,
    pub score: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReadWorkspaceFileRequest {
    #[serde(flatten)]
    pub target: WorkspaceTarget,
    pub path: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceFileText {
    /// Identity of the checkout this snapshot was read from.
    pub checkout_id: String,
    pub path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_hash: Option<String>,
    pub size: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub modified_at: Option<DateTime<Utc>>,
    pub encoding: WorkspaceTextEncoding,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line_ending: Option<WorkspaceLineEnding>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub read_only_reason: Option<WorkspaceReadOnlyReason>,
    pub truncated: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum WorkspaceTextEncoding {
    Utf8,
    Utf8Bom,
    Binary,
    Unsupported,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum WorkspaceLineEnding {
    Lf,
    Crlf,
    Mixed,
    None,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum WorkspaceReadOnlyReason {
    Binary,
    UnsupportedEncoding,
    MixedLineEndings,
    Symlink,
    TooLarge,
    PermissionDenied,
    NotRegularFile,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WatchWorkspaceFilesRequest {
    #[serde(flatten)]
    pub target: WorkspaceTarget,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceFileChanges {
    pub sequence: u64,
    pub resync_required: bool,
    pub changes: Vec<WorkspaceFileChange>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceFileChange {
    pub kind: WorkspaceFileChangeKind,
    pub path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub old_path: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum WorkspaceFileChangeKind {
    Created,
    Modified,
    Removed,
    Renamed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateWorkspaceEntryRequest {
    #[serde(flatten)]
    pub target: WorkspaceTarget,
    #[serde(default)]
    pub parent_path: String,
    #[serde(default)]
    pub name: String,
    pub kind: WorkspaceEntryKind,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RenameWorkspaceEntryRequest {
    #[serde(flatten)]
    pub target: WorkspaceTarget,
    pub path: String,
    pub new_name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeleteWorkspaceEntryRequest {
    #[serde(flatten)]
    pub target: WorkspaceTarget,
    pub path: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MoveWorkspaceEntryRequest {
    #[serde(flatten)]
    pub target: WorkspaceTarget,
    pub source_path: String,
    #[serde(default)]
    pub destination_directory: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CopyWorkspaceEntryRequest {
    #[serde(flatten)]
    pub target: WorkspaceTarget,
    pub source_path: String,
    #[serde(default)]
    pub destination_directory: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceEntryMutation {
    pub path: String,
    pub is_directory: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkspaceNameError {
    Empty,
    InvalidComponent,
}

impl WorkspaceNameError {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Empty => "name must not be empty",
            Self::InvalidComponent => "name contains an invalid component",
        }
    }
}

pub fn validate_workspace_component(name: &str) -> Result<(), WorkspaceNameError> {
    if name.is_empty() {
        return Err(WorkspaceNameError::Empty);
    }
    if name == "."
        || name == ".."
        || name.contains(['/', '\\', ':', '\0'])
        || name.eq_ignore_ascii_case(".git")
    {
        return Err(WorkspaceNameError::InvalidComponent);
    }
    Ok(())
}

pub fn validate_workspace_create_name(name: &str) -> Result<(), WorkspaceNameError> {
    if name.is_empty() {
        return Err(WorkspaceNameError::Empty);
    }
    if name.starts_with('/') {
        return Err(WorkspaceNameError::InvalidComponent);
    }
    for component in name.split('/') {
        validate_workspace_component(component)?;
    }
    Ok(())
}

pub fn join_workspace_relative(parent: &str, name: &str) -> String {
    match (parent.is_empty(), name.is_empty()) {
        (true, _) => name.to_string(),
        (_, true) => parent.to_string(),
        _ => format!("{parent}/{name}"),
    }
}

pub fn sibling_name_taken<'a>(existing: impl IntoIterator<Item = &'a str>, name: &str) -> bool {
    existing
        .into_iter()
        .any(|entry| entry.eq_ignore_ascii_case(name))
}

pub fn unique_copy_name<'a>(
    original: &str,
    is_directory: bool,
    existing: impl IntoIterator<Item = &'a str>,
) -> String {
    let taken: Vec<String> = existing
        .into_iter()
        .map(|name| name.to_ascii_lowercase())
        .collect();
    let is_taken = |candidate: &str| {
        taken
            .iter()
            .any(|name| name == &candidate.to_ascii_lowercase())
    };
    if !is_taken(original) {
        return original.to_string();
    }
    let (stem, ext) = copy_stem_and_ext(original, is_directory);
    let first = format!("{stem} copy{ext}");
    if !is_taken(&first) {
        return first;
    }
    let mut index = 2u32;
    loop {
        let candidate = format!("{stem} copy {index}{ext}");
        if !is_taken(&candidate) {
            return candidate;
        }
        index += 1;
    }
}

fn copy_stem_and_ext(name: &str, is_directory: bool) -> (String, String) {
    if is_directory {
        return (name.to_string(), String::new());
    }
    match name.rsplit_once('.') {
        Some((stem, ext)) if !stem.is_empty() => (stem.to_string(), format!(".{ext}")),
        _ => (name.to_string(), String::new()),
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiffFileSummary {
    pub path: String,
    /// Previous path for renames/copies.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub old_path: Option<String>,
    pub status: String,
    pub additions: u32,
    pub deletions: u32,
    #[serde(default)]
    pub binary: bool,
}

/// Working-tree diff for a checkout — latest-only sidecar, 3MiB patch cap.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CheckoutDiff {
    pub checkout_id: String,
    pub device_id: String,
    pub cwd: String,
    pub patch: String,
    pub files: Vec<DiffFileSummary>,
    pub additions: u32,
    pub deletions: u32,
    /// True when the patch was truncated at the byte cap ("Partial snapshot").
    pub truncated: bool,
    pub checksum: String,
    pub updated_at: DateTime<Utc>,
}

/// Provider-neutral lifecycle state for a code change request.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ChangeRequestState {
    Open,
    Closed,
    Merged,
}

/// Compact provider-neutral change request metadata for checkout surfaces.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChangeRequestSummary {
    pub provider: String,
    pub number: u64,
    pub title: String,
    pub url: String,
    pub state: ChangeRequestState,
    pub base_ref: String,
    pub head_ref: String,
}

/// Latest successful change request resolution for one checkout and branch.
///
/// `change_request: None` is an authoritative successful lookup with no match;
/// resolution failures must retain the previous successful snapshot instead.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CheckoutChangeRequestStatus {
    pub checkout_id: String,
    pub device_id: String,
    pub cwd: String,
    pub branch: String,
    pub change_request: Option<ChangeRequestSummary>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetCheckoutFileDiffTextRequest {
    pub checkout_id: String,
    pub cwd: String,
    pub path: String,
    #[serde(default)]
    pub mode: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_ref: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chat_id: Option<String>,
    /// Pinned commit for History's per-commit diff scope. When present, the
    /// source pair is read from the commit parent and this commit, never from
    /// the live working tree.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub commit_sha: Option<String>,
    pub diff_checksum: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CheckoutFileDiffText {
    pub diff_checksum: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub old_text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub new_text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub old_content_hash: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub new_content_hash: Option<String>,
    pub binary: bool,
    pub truncated: bool,
    #[serde(default)]
    pub stale: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserProfile {
    pub id: String,
    pub email: String,
    pub name: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "camelCase")]
pub enum AuthState {
    SignedOut,
    NeedsOrganization {
        user: UserProfile,
    },
    #[serde(rename_all = "camelCase")]
    SignedIn {
        user: UserProfile,
        org_id: Option<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentAccount {
    pub id: String,
    pub harness: HarnessId,
    pub email: Option<String>,
    pub plan_label: Option<String>,
    pub active: bool,
    #[serde(default)]
    pub usage_windows: Vec<AgentUsageWindow>,
    /// Local provider transcript/session archive totals (24h, 7d, 30d).
    #[serde(default)]
    pub usage_lines: Vec<AgentUsageLine>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub organization: Option<String>,
    /// How the CLI is signed in (`oauth` account vs raw `api-key`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auth_kind: Option<AgentAuthKind>,
    /// False for a live login whose credentials we could not read (e.g. macOS
    /// Keychain denied) — shown, but not re-activatable.
    #[serde(default)]
    pub switchable: bool,
    /// Epoch millis of the slot's last snapshot.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub saved_at: Option<i64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum AgentAuthKind {
    Oauth,
    ApiKey,
}

/// Everything the Accounts settings page renders, rebuilt after every mutation.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentAccountsSnapshot {
    pub accounts: Vec<AgentAccount>,
    pub warnings: Vec<AgentAccountWarning>,
}

/// A per-harness detection warning (e.g. Keychain denied reading the live login).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentAccountWarning {
    pub harness: HarnessId,
    pub message: String,
}

/// `StartAgentLogin` reply: open `url`, then either paste the code back
/// (`CompleteAgentLogin`) or poll until the browser flow lands (`PollAgentLogin`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentLoginStart {
    pub login_id: String,
    pub url: String,
    pub mode: AgentLoginMode,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum AgentLoginMode {
    /// Claude: the user pastes the OAuth code back into the app.
    PasteCode,
    /// Codex: the CLI's loopback callback completes in the browser; poll until done.
    Browser,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentLoginPoll {
    pub status: AgentLoginStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AgentLoginStatus {
    Pending,
    Done,
    Error,
}

/// CLI plan rate-limit window (accounts settings meters) — NOT app token accounting.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentUsageWindow {
    pub label: String,
    /// 0.0..=1.0
    pub used_fraction: f32,
    pub resets_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentUsageLine {
    pub label: String,
    pub value: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subtitle: Option<String>,
}

/// An open PTY session on the owning device (`OpenTerminal` reply).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalSession {
    pub id: String,
    pub cwd: String,
    /// Shell basename (`zsh`, `bash`, …) for the tab label.
    pub shell: String,
}

/// One `SubscribeTerminal` stream item. `seq` is a per-terminal monotonic counter
/// used for replay resumption (`afterSeq`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum TerminalEvent {
    /// Output chunk; `data` is base64 (PTY output is raw bytes, not valid UTF-8).
    Data { seq: u64, data: String },
    #[serde(rename_all = "camelCase")]
    Exit {
        seq: u64,
        exit_code: i32,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        signal: Option<String>,
    },
}

/// One in-flight queued-attachment transfer (the `WatchTransfers` stream):
/// raw-byte progress of the engine-side relay leg pushing staged bytes to a
/// remote host. An entry appears when a file's chunks start moving, updates
/// per landed chunk, and disappears when the host commits it (or the attempt
/// fails — the retry re-adds it). Keyed by the send-minted uploadId, so the
/// sender's thumbnails can resolve their `pending://{uploadId}/…` refs to a
/// real percent instead of an indeterminate spinner.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TransferProgress {
    pub upload_id: String,
    pub file_name: String,
    /// Raw bytes the host has acknowledged so far.
    pub done: u64,
    /// Total raw bytes of the staged file.
    pub total: u64,
}

/// Live edge-connectivity posture (the `WatchConnectivity` stream): the truth
/// the connection pill, composer honesty, and queued-send badges render.
/// Derived engine-side from the registry room's reconnect state, the OS
/// network-path monitor, and each open chat room's stats.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Connectivity {
    pub state: ConnectivityState,
    /// Epoch ms of the next scheduled registry dial while reconnecting
    /// (0 = none pending / dialing right now). The countdown renders
    /// client-side from this.
    #[serde(default)]
    pub retry_at_ms: i64,
    /// The failure that started the current outage — sticky through the next
    /// attempt (no flicker back to a bare "connecting…"), cleared on rejoin.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_failure: Option<String>,
    /// Per-OPEN-chat room state; a chat absent here is unknown (consumers
    /// fall back to the global state).
    #[serde(default)]
    pub chats: Vec<ChatConnectivity>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ConnectivityState {
    /// No edge transports on this profile (local scope) — hide the pill.
    #[default]
    Disabled,
    /// The OS reports no network path.
    Offline,
    /// Edge expected but the registry room is down (dialing/backing off).
    Reconnecting,
    Connected,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatConnectivity {
    pub chat_id: String,
    pub connected: bool,
    /// Local update batches not yet acked by the chat's edge room.
    #[serde(default)]
    pub pending_pushes: u64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn session_context_is_additive_and_round_trips() {
        let old: Session = serde_json::from_value(serde_json::json!({
            "chatId": "chat-1",
            "deviceId": "device-1",
            "status": "idle",
            "startedAt": null,
            "updatedAt": "2026-08-21T12:00:00Z"
        }))
        .unwrap();
        assert_eq!(old.context_usage, None);

        let session = Session {
            last_completed_turn: None,
            context_usage: Some(crate::ContextUsage {
                tokens: 392_000,
                context_window: 828_000,
            }),
            ..old
        };
        let value = serde_json::to_value(&session).unwrap();
        assert_eq!(value["contextUsage"]["tokens"], 392_000);
        assert_eq!(value["contextUsage"]["contextWindow"], 828_000);
        assert_eq!(serde_json::from_value::<Session>(value).unwrap(), session);
    }

    #[test]
    fn kimi_account_snapshot_serializes_only_normalized_device_local_fields() {
        let snapshot = AgentAccountsSnapshot {
            accounts: vec![AgentAccount {
                id: "kimi-code-managed".into(),
                harness: HarnessId::Kimi,
                email: None,
                plan_label: Some("Managed".into()),
                active: true,
                usage_windows: vec![AgentUsageWindow {
                    label: "Weekly".into(),
                    used_fraction: 0.4,
                    resets_at: None,
                }],
                usage_lines: Vec::new(),
                display_name: Some("Kimi Code".into()),
                organization: None,
                auth_kind: Some(AgentAuthKind::Oauth),
                switchable: false,
                saved_at: None,
            }],
            warnings: Vec::new(),
        };

        let value = serde_json::to_value(&snapshot).unwrap();
        assert_eq!(value["accounts"][0]["harness"], "kimi");
        assert_eq!(value["accounts"][0]["usageWindows"][0]["label"], "Weekly");
        assert!(value["accounts"][0].get("credentials").is_none());
        assert!(value["accounts"][0].get("accessToken").is_none());
        assert!(value["accounts"][0].get("refreshToken").is_none());
        assert_eq!(
            serde_json::from_value::<AgentAccountsSnapshot>(value).unwrap(),
            snapshot
        );
    }

    #[test]
    fn checkout_change_request_status_round_trips_all_states_as_camel_case() {
        for (state, encoded_state) in [
            (ChangeRequestState::Open, "open"),
            (ChangeRequestState::Closed, "closed"),
            (ChangeRequestState::Merged, "merged"),
        ] {
            let status = CheckoutChangeRequestStatus {
                checkout_id: "checkout-1".into(),
                device_id: "device-1".into(),
                cwd: "/repo".into(),
                branch: "feature/change".into(),
                change_request: Some(ChangeRequestSummary {
                    provider: "github".into(),
                    number: 90,
                    title: "Model checkout change request status".into(),
                    url: "https://github.com/acme/zeron/pull/90".into(),
                    state,
                    base_ref: "main".into(),
                    head_ref: "feature/change".into(),
                }),
                updated_at: Utc.with_ymd_and_hms(2026, 8, 15, 12, 30, 0).unwrap(),
            };

            let value = serde_json::to_value(&status).unwrap();
            assert_eq!(value["checkoutId"], "checkout-1");
            assert_eq!(value["deviceId"], "device-1");
            assert_eq!(value["changeRequest"]["state"], encoded_state);
            assert_eq!(value["changeRequest"]["baseRef"], "main");
            assert_eq!(value["changeRequest"]["headRef"], "feature/change");
            assert_eq!(
                serde_json::from_value::<CheckoutChangeRequestStatus>(value).unwrap(),
                status
            );
        }
    }

    #[test]
    fn checkout_file_diff_text_contract_is_camel_case() {
        let request = GetCheckoutFileDiffTextRequest {
            checkout_id: "checkout".into(),
            cwd: "/repo".into(),
            path: "src/lib.rs".into(),
            mode: "branch".into(),
            base_ref: Some("main".into()),
            chat_id: None,
            commit_sha: Some("deadbeef".into()),
            diff_checksum: "abc".into(),
        };
        let value = serde_json::to_value(&request).unwrap();
        assert_eq!(value["checkoutId"], "checkout");
        assert_eq!(value["diffChecksum"], "abc");
        assert_eq!(value["commitSha"], "deadbeef");
        assert_eq!(
            serde_json::from_value::<GetCheckoutFileDiffTextRequest>(value).unwrap(),
            request
        );
    }

    #[test]
    fn workspace_file_requests_flatten_target_and_omit_options() {
        let request = ListWorkspaceDirectoryRequest {
            target: WorkspaceTarget {
                chat_id: Some("chat-1".into()),
                space_id: None,
                checkout_path: None,
            },
            directory: "src/日本語".into(),
            include_ignored: false,
            cursor: None,
        };
        let value = serde_json::to_value(&request).unwrap();
        assert_eq!(
            value,
            serde_json::json!({
                "chatId": "chat-1",
                "directory": "src/日本語",
                "includeIgnored": false,
            })
        );
        assert_eq!(
            serde_json::from_value::<ListWorkspaceDirectoryRequest>(value).unwrap(),
            request
        );

        let requests = [
            serde_json::to_value(SearchWorkspaceFilesRequest {
                target: request.target.clone(),
                query: "main".into(),
                include_ignored: false,
                limit: None,
            })
            .unwrap(),
            serde_json::to_value(ReadWorkspaceFileRequest {
                target: request.target.clone(),
                path: "src/main.rs".into(),
            })
            .unwrap(),
            serde_json::to_value(WatchWorkspaceFilesRequest {
                target: request.target,
            })
            .unwrap(),
        ];
        assert!(requests.iter().all(|value| value["chatId"] == "chat-1"));
    }

    #[test]
    fn workspace_file_change_contract_is_camel_case() {
        let changes = WorkspaceFileChanges {
            sequence: 4,
            resync_required: false,
            changes: vec![WorkspaceFileChange {
                kind: WorkspaceFileChangeKind::Renamed,
                path: "src/new.rs".into(),
                old_path: Some("src/old.rs".into()),
            }],
        };
        let value = serde_json::to_value(&changes).unwrap();
        assert_eq!(value["resyncRequired"], false);
        assert_eq!(value["changes"][0]["kind"], "renamed");
        assert_eq!(value["changes"][0]["oldPath"], "src/old.rs");
        assert_eq!(
            serde_json::from_value::<WorkspaceFileChanges>(value).unwrap(),
            changes
        );
    }

    #[test]
    fn workspace_entry_mutation_contract_is_camel_case() {
        let target = WorkspaceTarget {
            chat_id: Some("chat-1".into()),
            space_id: None,
            checkout_path: None,
        };
        let create = serde_json::to_value(CreateWorkspaceEntryRequest {
            target: target.clone(),
            parent_path: "src".into(),
            name: "notes.md".into(),
            kind: WorkspaceEntryKind::File,
        })
        .unwrap();
        assert_eq!(create["chatId"], "chat-1");
        assert_eq!(create["parentPath"], "src");
        assert_eq!(create["name"], "notes.md");
        assert_eq!(create["kind"], "file");
        let rename = serde_json::to_value(RenameWorkspaceEntryRequest {
            target: target.clone(),
            path: "src/a.txt".into(),
            new_name: "b.txt".into(),
        })
        .unwrap();
        assert_eq!(rename["path"], "src/a.txt");
        assert_eq!(rename["newName"], "b.txt");
        let delete = serde_json::to_value(DeleteWorkspaceEntryRequest {
            target: target.clone(),
            path: "src/a.txt".into(),
        })
        .unwrap();
        assert_eq!(delete["path"], "src/a.txt");
        let move_req = serde_json::to_value(MoveWorkspaceEntryRequest {
            target: target.clone(),
            source_path: "src/a.rs".into(),
            destination_directory: "src/util".into(),
        })
        .unwrap();
        assert_eq!(move_req["sourcePath"], "src/a.rs");
        assert_eq!(move_req["destinationDirectory"], "src/util");
        let copy = serde_json::to_value(CopyWorkspaceEntryRequest {
            target,
            source_path: "a.txt".into(),
            destination_directory: String::new(),
        })
        .unwrap();
        assert_eq!(copy["sourcePath"], "a.txt");
        assert_eq!(copy["destinationDirectory"], "");
        let mutation = serde_json::to_value(WorkspaceEntryMutation {
            path: "src/notes.md".into(),
            is_directory: false,
        })
        .unwrap();
        assert_eq!(mutation["path"], "src/notes.md");
        assert_eq!(mutation["isDirectory"], false);
    }

    #[test]
    fn unique_copy_name_derives_case_insensitive_collisions() {
        assert_eq!(
            unique_copy_name("a.txt", false, ["b.txt"].into_iter()),
            "a.txt"
        );
        assert_eq!(
            unique_copy_name("a.txt", false, ["a.txt"].into_iter()),
            "a copy.txt"
        );
        assert_eq!(
            unique_copy_name("a.txt", false, ["a.txt", "A copy.txt"].into_iter()),
            "a copy 2.txt"
        );
        assert_eq!(
            unique_copy_name("docs", true, ["docs"].into_iter()),
            "docs copy"
        );
        assert!(validate_workspace_create_name("docs/adr/0001.md").is_ok());
        assert!(validate_workspace_component("..").is_err());
        assert!(validate_workspace_create_name("").is_err());
        assert!(sibling_name_taken(["Readme.md"].into_iter(), "readme.md"));
    }
}
