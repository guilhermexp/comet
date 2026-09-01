//! EngineRpc — the engine-side `RpcService`: sessions + docs + the workspace-doc
//! entity surface.
//!
//! Methods (feature-inventory §2):
//! - `ListHarnesses` → `[HarnessDescriptor]`
//! - `ListModels {harness}` → `[Model]`
//! - `QueueCommand {chatId, command}` → `{commandId}` (durable doc command)
//! - `WatchDocMessages {chatId}` → stream of joined `SessionMessageEntry[]`,
//!   re-emitted on every doc change
//! - `WatchChats` / `WatchDevices` → streams of the workspace doc's entity rows
//! - `WatchSessions` → stream of `Session[]`: this engine's live statuses merged with
//!   remote devices' workspace session rows
//! - `Mutate {op, …}` → `{ok}` — workspace entity mutations (createChat, renameChat,
//!   setChatArchived, deleteChat, renameDevice, markChatSeen)
//! - `EngineInfo` → `{deviceId, workspaceScope}` — this runtime's fixed identity
//!   and data boundary (never forwarded)
//! - `LocalDevice` → `{deviceId}` — legacy engine identity (never forwarded)
//! - AuthRpc (feature-inventory §2): `AuthStatus` (stream), `SignIn`/`SignInHeadless` →
//!   `{url}`, `CompleteSignIn {code}`, `SignOut`, `ListOrgs`, `CreateOrg {name}`,
//!   `SelectOrg {organizationId}`
//! - Repos (§3.5): `ListRepos`, `AddRepo {path}`, `CloneRepo {url}`,
//!   `CreateRepo {name}`, `ListBranches {repoPath}` (default branch first),
//!   `ListFolders {path?}`, `CreateWorktree {repoPath, branch}`, `DeleteWorktree
//!   {repoPath, worktreePath}`; `WatchCheckoutDiffs` → stream of `CheckoutDiff[]`
//! - Terminals (§3.4): `OpenTerminal {chatId, cols, rows}` → `TerminalSession`,
//!   `SubscribeTerminal {terminalId, afterSeq?}` → stream of `TerminalEvent`
//!   (replay then live tail), `WriteTerminal {terminalId, data}`, `ResizeTerminal`,
//!   `CloseTerminal`. M5 is single-user local: per-user owner checks land with
//!   real multi-account auth in M6.
//! - Agent accounts (§3.7): `ListAgentAccounts {forceUsage?}` →
//!   `AgentAccountsSnapshot`, `ActivateAgentAccount`/`ForgetAgentAccount`
//!   `{harness, accountId}` → snapshot, `StartAgentLogin {harness}` →
//!   `{loginId, url, mode}`, `CompleteAgentLogin {loginId, code}` → snapshot,
//!   `PollAgentLogin {loginId}`, `CancelAgentLogin {loginId}`.
//! - Uploads (§3.7): `UploadChunk {uploadId, data, seq?}`,
//!   `UploadCommit {uploadId, fileName}` → `{path}`,
//!   `ReadAttachmentChunk {path, offset}` → `{name, mimeType, data, nextOffset,
//!   done}` (path-jailed to the uploads dir + workspace-known chat cwds).
//!
//! ## Device-addressed routing (`targetDeviceId`, feature-inventory §2.1)
//!
//! ControlRpc methods are relay-forwardable: params may carry `targetDeviceId`. When it
//! names another device, the call is forwarded verbatim over that device's relay DO via
//! the [`LinkCache`] — the remote engine sees its own id and handles locally, so the
//! forward can never loop. Streaming methods are proxied by re-subscribing remotely and
//! piping items. To make another method device-addressable, nothing per-method is needed
//! beyond listing it in [`forwardable`] (and [`is_stream_method`] if it streams);
//! handlers stay transport-agnostic. Currently routed: `ListHarnesses`, `ListModels`,
//! `QueueCommand`, and `WatchDocMessages`.

use async_trait::async_trait;
use futures::StreamExt;
use futures::stream::BoxStream;
use serde::Deserialize;
use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::watch;

use zeron_doc::{MessagePart, SessionCommandPayload};
use zeron_proto::{ChatConfig, EngineInfo, HarnessId, ToolCall, WorkspaceScope};
use zeron_rpc::{
    LinkCache, RevealTrajectoryRawParams, RpcError, RpcReply, RpcService, TrajectoryCursor,
    TrajectoryRawRevealResult, TrajectoryTerminalReason, TrajectoryUnavailableReason,
    TrajectoryWatchItem, WatchTrajectoryParams, methods, parse_params,
};

use crate::agent_accounts::AgentAccounts;
use crate::auth::Auth;
use crate::change_requests::CheckoutChangeRequests;
use crate::diff_sync::CheckoutDiffSync;
use crate::doc_host::DocHost;
use crate::registry::HarnessRegistry;
use crate::repos::{Repos, home_dir};
use crate::run_journal::RunJournal;
use crate::sessions::SessionsEngine;
use crate::terminals::Terminals;
use crate::trajectory_store::{TrajectoryStore, TrajectoryStoreEvent};
use crate::uploads::Uploads;
use crate::workspace_host::WorkspaceHost;

const FILE_SEARCH_RPC_TIMEOUT: Duration = Duration::from_secs(6);
const FILE_SEARCH_FEATURED_PATHS: usize = 32;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ChatParams {
    chat_id: String,
}

#[derive(Debug, Deserialize)]
struct SetLiveVoiceMutedParams {
    muted: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ListModelsParams {
    harness: HarnessId,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SetHarnessEnabledParams {
    harness: HarnessId,
    enabled: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct QueueCommandParams {
    chat_id: String,
    command: SessionCommandPayload,
    /// Queued attachments (bytes already committed locally as `pending://`
    /// refs) the engine delivers to a remote host AFTER the command is
    /// durably queued — never as a gate in front of it.
    #[serde(default)]
    transfers: Vec<crate::uploads::AttachmentTransfer>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct QueueWorkerNotificationParams {
    chat_id: String,
    command_id: String,
    command: SessionCommandPayload,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RelayCommandParams {
    chat_id: String,
    /// The full command entry, client-minted id included — the exactly-once
    /// key the host claims in its processed ledger before executing.
    entry: zeron_doc::SessionCommandEntry,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RepoPathParams {
    /// `repoPath` per §3.5 (the §2.1 shorthand `repo` is accepted as an alias).
    #[serde(alias = "repo")]
    repo_path: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CheckoutChangeRequestParams {
    cwd: String,
    #[serde(default)]
    branch: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SwitchRefParams {
    /// The checkout to switch — a session's cwd (main folder or worktree).
    repo_path: String,
    ref_name: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreateWorktreeParams {
    #[serde(alias = "repo")]
    repo_path: String,
    branch: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DeleteWorktreeParams {
    #[serde(alias = "repo")]
    repo_path: String,
    #[serde(alias = "path")]
    worktree_path: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ListFoldersParams {
    #[serde(default)]
    path: Option<String>,
    /// Serde-defaulted so an older viewport keeps the hidden-free listing it
    /// has always had.
    #[serde(default)]
    include_hidden: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FileSearchParams {
    query: String,
    #[serde(default)]
    chat_id: Option<String>,
    #[serde(default)]
    space_id: Option<String>,
    /// Existing linked worktree selected for a new chat. The engine accepts it
    /// only after verifying it against the space repository's worktree list.
    #[serde(default)]
    path: Option<String>,
}

fn tool_file_path(call: &ToolCall) -> Option<&str> {
    match call {
        ToolCall::ReadFile { path }
        | ToolCall::WriteFile { path, .. }
        | ToolCall::EditFile { path, .. } => Some(path),
        ToolCall::ApplyPatch { path } | ToolCall::Search { path, .. } => path.as_deref(),
        ToolCall::Exec { .. }
        | ToolCall::Glob { .. }
        | ToolCall::WebFetch { .. }
        | ToolCall::WebSearch { .. }
        | ToolCall::Todo { .. }
        | ToolCall::Mcp { .. }
        | ToolCall::Unknown { .. } => None,
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct OpenTerminalParams {
    #[serde(default)]
    chat_id: Option<String>,
    #[serde(default)]
    cwd: Option<String>,
    cols: u16,
    rows: u16,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TerminalIdParams {
    terminal_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SubscribeTerminalParams {
    terminal_id: String,
    #[serde(default)]
    after_seq: Option<u64>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct WriteTerminalParams {
    terminal_id: String,
    /// Base64 input bytes (plain UTF-8 accepted leniently).
    data: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ResizeTerminalParams {
    terminal_id: String,
    cols: u16,
    rows: u16,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ListAgentAccountsParams {
    #[serde(default)]
    force_usage: Option<bool>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AgentAccountParams {
    harness: HarnessId,
    account_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StartAgentLoginParams {
    harness: HarnessId,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LoginIdParams {
    login_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CompleteAgentLoginParams {
    login_id: String,
    code: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct UploadChunkParams {
    upload_id: String,
    /// Base64 payload chunk.
    data: String,
    #[serde(default)]
    seq: Option<u64>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct UploadCommitParams {
    upload_id: String,
    file_name: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ReadAttachmentChunkParams {
    path: String,
    #[serde(default)]
    offset: u64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FetchToolBlobParams {
    /// Doc-resident sidecar ref (`{chatId}/{partId}` or `…​.diff`).
    blob_ref: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FetchToolInputParams {
    chat_id: String,
    tool_call_id: String,
    #[serde(default)]
    parent_tool_use_id: Option<String>,
}

const FILE_TOOL_INPUT_RESPONSE_MAX_BYTES: usize = 1024 * 1024;
const FILE_TOOL_INPUT_ENVELOPE_RESERVE: usize = 128;

fn require_file_tool_input_owner(
    chat_device_id: Option<&str>,
    local_device_id: &str,
) -> Result<(), RpcError> {
    match chat_device_id {
        Some(device_id) if device_id == local_device_id => Ok(()),
        Some(_) => Err(RpcError::Failed("chat belongs to another device".into())),
        None => Err(RpcError::Failed("chat not found".into())),
    }
}

/// The Mutate surface (feature-inventory §2 DataRpc), tagged by `op`.
#[derive(Debug, Deserialize)]
#[serde(tag = "op", rename_all = "camelCase")]
enum MutateParams {
    #[serde(rename_all = "camelCase")]
    CreateChat {
        chat_id: String,
        /// The project the chat is created in — fixes host device + base cwd.
        /// `None` mints a project-less chat: `deviceId` picks the host and the
        /// cwd defaults to `~` (expanded on the host at run time).
        #[serde(default)]
        space_id: Option<String>,
        /// Host device for a project-less chat; ignored when `spaceId` is set.
        #[serde(default)]
        device_id: Option<String>,
        #[serde(default)]
        config: Option<ChatConfig>,
        /// The picked ref, named on the row from the first frame (the footer
        /// read "Select ref" until the diff reconciler stamped it).
        #[serde(default)]
        branch: Option<String>,
        /// Cwd override (isolated-worktree path); default = the space's folder.
        #[serde(default)]
        cwd: Option<String>,
    },
    /// Create a space (device + folder pair). Idempotent by id; a live
    /// duplicate `(deviceId, path)` no-ops. `gitDetected` is seeded from the
    /// picker's FolderEntry — the owning device's SpacesSync re-verifies.
    #[serde(rename_all = "camelCase")]
    CreateSpace {
        space_id: String,
        device_id: String,
        path: String,
        #[serde(default)]
        name: Option<String>,
        #[serde(default)]
        git_detected: bool,
    },
    /// LWW display-name set; `name: None` clears back to basename(path).
    #[serde(rename_all = "camelCase")]
    RenameSpace {
        space_id: String,
        #[serde(default)]
        name: Option<String>,
    },
    /// Hard delete: cascades to every chat (and session row) in the space.
    /// Live runs hosted here are interrupted best-effort.
    #[serde(rename_all = "camelCase")]
    DeleteSpace { space_id: String },
    #[serde(rename_all = "camelCase")]
    RenameChat { chat_id: String, title: String },
    /// Set the chat's checkout branch label — the sidebar's
    /// "project · branch" sub-line.
    #[serde(rename_all = "camelCase")]
    SetChatBranch { chat_id: String, branch: String },
    /// Retarget a chat onto another folder — mid-session switch to an
    /// EXISTING worktree (the picked ref's checkout). Next run starts a
    /// fresh harness conversation there (resume is cwd-scoped).
    #[serde(rename_all = "camelCase")]
    SetChatCwd { chat_id: String, cwd: String },
    /// Backdate a chat's activity timestamps (epoch ms) — the sidebar's
    /// relative-time column. Used by tooling/seeds; the doc fold sets these on
    /// real message traffic.
    #[serde(rename_all = "camelCase")]
    SetChatActivity {
        chat_id: String,
        #[serde(default)]
        last_message_at: Option<i64>,
        #[serde(default)]
        created_at: Option<i64>,
    },
    /// Re-home a chat to another device (tooling/seeds; device migration later).
    #[serde(rename_all = "camelCase")]
    SetChatHost { chat_id: String, device_id: String },
    #[serde(rename_all = "camelCase")]
    SetChatArchived { chat_id: String, archived: bool },
    /// Full-config replace on the chat row (zeron `SetChatConfig`): the
    /// composer's mid-session model / reasoning / options changes, LWW-synced
    /// so they survive restarts and reach every device.
    #[serde(rename_all = "camelCase")]
    SetChatConfig { chat_id: String, config: ChatConfig },
    /// Tombstone: removes the chats-map row; the session doc remains.
    #[serde(rename_all = "camelCase")]
    DeleteChat { chat_id: String },
    #[serde(rename_all = "camelCase")]
    RenameDevice { device_id: String, name: String },
    /// Synced seen marker (LWW + monotonic guard): clears the "completed"
    /// badge on every device. `at` is epoch ms; default = now.
    #[serde(rename_all = "camelCase")]
    MarkChatSeen {
        chat_id: String,
        #[serde(default)]
        at: Option<i64>,
    },
}

pub struct EngineRpc {
    sessions: SessionsEngine,
    doc_host: DocHost,
    workspace: WorkspaceHost,
    registry: std::sync::Arc<HarnessRegistry>,
    repos: Repos,
    terminals: Terminals,
    change_requests: CheckoutChangeRequests,
    diff_sync: CheckoutDiffSync,
    uploads: Uploads,
    agent_accounts: AgentAccounts,
    auth: Option<Auth>,
    links: Option<std::sync::Arc<LinkCache>>,
    updater: Option<zeron_update::Updater>,
    local_import: Option<crate::local_import::LocalImporter>,
    trajectory: Option<Arc<TrajectoryStore>>,
    run_journal: Option<Arc<RunJournal>>,
    engine_info: EngineInfo,
}

impl EngineRpc {
    #[allow(clippy::too_many_arguments)] // engine assembly seam, not a public API
    pub fn new(
        sessions: SessionsEngine,
        doc_host: DocHost,
        workspace: WorkspaceHost,
        registry: std::sync::Arc<HarnessRegistry>,
        repos: Repos,
        terminals: Terminals,
        change_requests: CheckoutChangeRequests,
        diff_sync: CheckoutDiffSync,
        uploads: Uploads,
        agent_accounts: AgentAccounts,
        workspace_scope: WorkspaceScope,
    ) -> Self {
        let engine_info = EngineInfo {
            device_id: doc_host.device_id().to_string(),
            workspace_scope,
        };
        Self {
            sessions,
            doc_host,
            workspace,
            registry,
            repos,
            terminals,
            change_requests,
            diff_sync,
            uploads,
            agent_accounts,
            auth: None,
            links: None,
            updater: None,
            local_import: None,
            trajectory: None,
            run_journal: None,
            engine_info,
        }
    }

    /// Attach a specific trajectory store (used in tests or profile setup).
    pub fn with_trajectory_store(mut self, store: Arc<TrajectoryStore>) -> Self {
        self.trajectory = Some(store);
        self
    }

    /// Attach a specific run journal (used in tests).
    pub fn with_run_journal(mut self, journal: Arc<RunJournal>) -> Self {
        self.run_journal = Some(journal);
        self
    }

    pub fn trajectory_store(&self) -> Option<Arc<TrajectoryStore>> {
        self.trajectory.clone()
    }

    pub fn run_journal(&self) -> Option<Arc<RunJournal>> {
        self.run_journal.clone()
    }

    /// Attach the auth service (AuthStatus + AuthRpc mutations).
    pub fn with_auth(mut self, auth: Auth) -> Self {
        self.auth = Some(auth);
        self
    }

    /// Attach the peer link cache — enables `targetDeviceId` relay forwarding.
    pub fn with_links(mut self, links: std::sync::Arc<LinkCache>) -> Self {
        self.links = Some(links);
        self
    }

    /// Attach the release checker (UpdateStatus stream + ApplyUpdate).
    pub fn with_updater(mut self, updater: zeron_update::Updater) -> Self {
        self.updater = Some(updater);
        self
    }

    /// Attach the local→synced profile importer (synced runtimes only).
    pub fn with_local_import(mut self, importer: crate::local_import::LocalImporter) -> Self {
        self.local_import = Some(importer);
        self
    }

    fn auth(&self) -> Result<&Auth, RpcError> {
        self.auth
            .as_ref()
            .ok_or_else(|| RpcError::Failed("auth unavailable".into()))
    }

    fn updater(&self) -> Result<&zeron_update::Updater, RpcError> {
        self.updater
            .as_ref()
            .ok_or_else(|| RpcError::Failed("updates unavailable".into()))
    }

    fn local_importer(&self) -> Result<&crate::local_import::LocalImporter, RpcError> {
        self.local_import
            .as_ref()
            .ok_or_else(|| RpcError::Failed("local import requires a synced workspace".into()))
    }

    /// Resolve a mention-search root from synced workspace rows. A client may
    /// name an existing linked worktree for a new chat, but it is verified
    /// against the space repository before any filesystem walk begins.
    async fn file_search_root(&self, p: &FileSearchParams) -> Result<std::path::PathBuf, RpcError> {
        let local_device = self.doc_host.device_id();
        match (&p.chat_id, &p.space_id) {
            (Some(_), Some(_)) | (None, None) => Err(RpcError::BadParams(
                "SearchFiles needs exactly one of chatId or spaceId".into(),
            )),
            (Some(chat_id), None) => {
                if p.path.is_some() {
                    return Err(RpcError::BadParams(
                        "SearchFiles path applies only to a space".into(),
                    ));
                }
                let chat = self
                    .workspace
                    .chat(chat_id)
                    .map_err(|e| RpcError::Failed(e.to_string()))?
                    .ok_or_else(|| RpcError::Failed("chat not found".into()))?;
                if chat.device_id != local_device {
                    return Err(RpcError::Failed("chat belongs to another device".into()));
                }
                let cwd = chat
                    .cwd
                    .map(std::path::PathBuf::from)
                    .ok_or_else(|| RpcError::Failed("chat has no workspace folder".into()))?;
                let space_id = chat
                    .space_id
                    .ok_or_else(|| RpcError::Failed("chat has no workspace space".into()))?;
                let space = self
                    .workspace
                    .space(&space_id)
                    .map_err(|e| RpcError::Failed(e.to_string()))?
                    .ok_or_else(|| RpcError::Failed("chat workspace space not found".into()))?;
                if space.device_id != local_device {
                    return Err(RpcError::Failed(
                        "chat space belongs to another device".into(),
                    ));
                }
                if let Some(cwd) = self
                    .repos
                    .workspace_checkout(std::path::Path::new(&space.path), &cwd)
                    .await
                {
                    Ok(cwd)
                } else {
                    Err(RpcError::Failed(
                        "chat folder is not a workspace checkout".into(),
                    ))
                }
            }
            (None, Some(space_id)) => {
                let space = self
                    .workspace
                    .space(space_id)
                    .map_err(|e| RpcError::Failed(e.to_string()))?
                    .ok_or_else(|| RpcError::Failed("space not found".into()))?;
                if space.device_id != local_device {
                    return Err(RpcError::Failed("space belongs to another device".into()));
                }
                let space_path = std::path::PathBuf::from(&space.path);
                let requested = p
                    .path
                    .as_deref()
                    .map_or_else(|| space_path.clone(), std::path::PathBuf::from);
                if let Some(requested) =
                    self.repos.workspace_checkout(&space_path, &requested).await
                {
                    Ok(requested)
                } else {
                    Err(RpcError::BadParams(
                        "SearchFiles path is not a workspace checkout".into(),
                    ))
                }
            }
        }
    }

    /// Accept only a checkout already named by a local chat or contained in a
    /// local space. Remote clients must not turn this RPC into an arbitrary path probe.
    async fn change_request_root(&self, cwd: &str) -> Result<std::path::PathBuf, RpcError> {
        let requested = std::path::PathBuf::from(cwd);
        let local_device = self.doc_host.device_id();
        let mut chats_rx = self.workspace.watch_chats();
        let mut spaces_rx = self.workspace.watch_spaces();
        let deadline = tokio::time::Instant::now() + Duration::from_millis(250);
        loop {
            let chats = chats_rx.borrow_and_update().clone();
            if chats.iter().any(|chat| {
                chat.device_id == local_device
                    && chat.cwd.as_deref().map(std::path::Path::new) == Some(requested.as_path())
            }) {
                return Ok(requested);
            }

            let spaces = spaces_rx.borrow_and_update().clone();
            for space in spaces
                .iter()
                .filter(|space| space.device_id == local_device)
            {
                if let Some(checkout) = self
                    .repos
                    .workspace_checkout(std::path::Path::new(&space.path), &requested)
                    .await
                {
                    return Ok(checkout);
                }
            }

            if tokio::time::Instant::now() >= deadline {
                break;
            }
            tokio::select! {
                _ = chats_rx.changed() => {}
                _ = spaces_rx.changed() => {}
                _ = tokio::time::sleep_until(deadline) => break,
            }
        }
        Err(RpcError::BadParams(
            "cwd is not a known checkout on this device".into(),
        ))
    }

    /// Most-recent-first paths the current chat actually touched, followed by
    /// files still changed in its checkout. The search worker validates and
    /// normalizes them against the resolved root before using them as ranking
    /// hints, so stale or out-of-workspace tool paths simply disappear.
    fn featured_file_paths(&self, chat_id: &str) -> Vec<String> {
        let mut paths = Vec::new();
        let mut seen = HashSet::new();
        if let Ok(handle) = self.doc_host.open(chat_id)
            && let Ok(entries) = handle.doc().read_entries()
        {
            for entry in entries.into_iter().rev() {
                for part in entry.parts.into_iter().rev() {
                    if let MessagePart::Tool { call, .. } = part
                        && let Some(path) = tool_file_path(&call)
                        && !path.trim().is_empty()
                        && seen.insert(path.to_string())
                    {
                        paths.push(path.to_string());
                        if paths.len() == FILE_SEARCH_FEATURED_PATHS {
                            break;
                        }
                    }
                }
                if paths.len() == FILE_SEARCH_FEATURED_PATHS {
                    break;
                }
            }
        }

        if let Ok(Some(chat)) = self.workspace.chat(chat_id) {
            let diffs = self.diff_sync.watch_diffs().borrow().clone();
            let diff = chat
                .checkout_id
                .as_deref()
                .and_then(|id| diffs.iter().find(|diff| diff.checkout_id == id))
                .or_else(|| {
                    chat.cwd
                        .as_deref()
                        .and_then(|cwd| diffs.iter().find(|diff| diff.cwd == cwd))
                });
            if let Some(diff) = diff {
                for file in &diff.files {
                    if paths.len() == FILE_SEARCH_FEATURED_PATHS {
                        break;
                    }
                    if seen.insert(file.path.clone()) {
                        paths.push(file.path.clone());
                    }
                }
            }
        }
        paths
    }

    /// Forward a device-addressed call over the target device's relay. On transport
    /// failure the cached link is invalidated so the next call re-dials.
    async fn forward(
        &self,
        target: &str,
        method: &str,
        params: serde_json::Value,
    ) -> Result<RpcReply, RpcError> {
        let Some(links) = &self.links else {
            return Err(RpcError::Failed(format!(
                "cannot reach device {target}: remote routing unavailable (offline)"
            )));
        };
        let client = links.client(target).await?;
        if is_stream_method(method) {
            // Streams are unbounded by design (a quiet WATCH_* is healthy);
            // only unary calls below get the reply deadline.
            if method == methods::WATCH_CHECKOUT_CHANGE_REQUEST {
                let rx = match client.subscribe_checked(method, params).await {
                    Ok(rx) => rx,
                    Err(err) => {
                        if should_invalidate_link(&err) {
                            links.invalidate(target);
                        }
                        return Err(err);
                    }
                };
                let stream = futures::stream::unfold((rx, client), |(mut rx, client)| async move {
                    rx.recv().await.map(|item| (item, (rx, client)))
                });
                return Ok(RpcReply::Stream(stream.boxed()));
            }
            let rx = match client.subscribe(method, params).await {
                Ok(rx) => rx,
                Err(err) => {
                    if should_invalidate_link(&err) {
                        links.invalidate(target);
                    }
                    return Err(err);
                }
            };
            // Pipe remote items; the held client keeps the link's RpcClient alive for
            // the stream's lifetime. A remote error just ends the stream (the relay
            // link-down path fails pending calls; stream receivers close).
            let stream = futures::stream::unfold((rx, client), |(mut rx, client)| async move {
                rx.recv().await.map(|item| (item, (rx, client)))
            });
            return Ok(RpcReply::Stream(stream.boxed()));
        }
        let deadline = forward_deadline(method);
        match tokio::time::timeout(deadline, client.call(method, params)).await {
            Ok(Ok(value)) => Ok(RpcReply::Value(value)),
            Ok(Err(err)) => {
                if should_invalidate_link(&err) {
                    links.invalidate(target);
                }
                Err(err)
            }
            Err(_) => {
                // No reply inside the deadline. The link may be a zombie — the
                // relay's auto-pong keeps a dead host socket looking alive
                // (ws3 auto-pong incident) — so drop it; the next call re-dials.
                // NOTE: the remote may still complete the forwarded work; the
                // caller sees a retryable failure instead of hanging forever
                // (the "Sending…" wedge, 2026-08-18).
                links.invalidate(target);
                Err(RpcError::Transport(format!(
                    "no reply from device {target} for {method} within {}s",
                    deadline.as_secs()
                )))
            }
        }
    }

    fn mutate(&self, params: MutateParams) -> Result<(), RpcError> {
        let failed = |e: crate::EngineError| RpcError::Failed(e.to_string());
        match params {
            MutateParams::CreateChat {
                chat_id,
                space_id,
                device_id,
                config,
                branch,
                cwd,
            } => {
                self.workspace
                    .create_chat(
                        &chat_id,
                        space_id.as_deref(),
                        device_id.as_deref(),
                        config,
                        cwd,
                    )
                    .map_err(failed)?;
                if let Some(branch) = branch.as_deref().filter(|b| !b.is_empty()) {
                    self.workspace
                        .set_chat_branch(&chat_id, branch)
                        .map_err(failed)?;
                }
                Ok(())
            }
            MutateParams::CreateSpace {
                space_id,
                device_id,
                path,
                name,
                git_detected,
            } => self
                .workspace
                .create_space(&space_id, &device_id, &path, name, git_detected)
                .map_err(failed),
            MutateParams::RenameSpace { space_id, name } => self
                .workspace
                .rename_space(&space_id, name.as_deref())
                .map_err(failed)
                .map(drop),
            MutateParams::DeleteSpace { space_id } => {
                let deleted = self.workspace.delete_space(&space_id).map_err(failed)?;
                // Best-effort teardown of live runs we host for the deleted chats
                // (the doc rows are already tombstoned; a straggler run would only
                // write into an orphaned session doc).
                let sessions = self.sessions.clone();
                let doc_host = self.doc_host.clone();
                let chat_ids = deleted.chat_ids;
                tokio::spawn(async move {
                    for chat_id in chat_ids {
                        if let Err(err) = sessions.interrupt(&chat_id).await {
                            tracing::debug!(chat = %chat_id, error = %err, "deleteSpace interrupt skipped");
                        }
                        doc_host.purge_chat(&chat_id);
                    }
                });
                Ok(())
            }
            MutateParams::RenameChat { chat_id, title } => self
                .workspace
                .rename_chat(&chat_id, &title)
                .map_err(failed)
                .map(drop),
            MutateParams::SetChatBranch { chat_id, branch } => self
                .workspace
                .set_chat_branch(&chat_id, &branch)
                .map_err(failed)
                .map(drop),
            MutateParams::SetChatCwd { chat_id, cwd } => self
                .workspace
                .set_chat_cwd(&chat_id, &cwd)
                .map_err(failed)
                .map(drop),
            MutateParams::SetChatActivity {
                chat_id,
                last_message_at,
                created_at,
            } => self
                .workspace
                .set_chat_activity(&chat_id, last_message_at, created_at)
                .map_err(failed)
                .map(drop),
            MutateParams::SetChatHost { chat_id, device_id } => self
                .workspace
                .set_chat_host(&chat_id, &device_id)
                .map_err(failed)
                .map(drop),
            MutateParams::SetChatArchived { chat_id, archived } => self
                .workspace
                .set_chat_archived(&chat_id, archived)
                .map_err(failed)
                .map(drop),
            MutateParams::SetChatConfig { chat_id, config } => self
                .workspace
                .set_chat_config(&chat_id, &config)
                .map_err(failed)
                .map(drop),
            MutateParams::DeleteChat { chat_id } => {
                self.workspace.delete_chat(&chat_id).map_err(failed)?;
                self.doc_host.purge_chat(&chat_id);
                Ok(())
            }
            MutateParams::RenameDevice { device_id, name } => self
                .workspace
                .rename_device(&device_id, &name)
                .map_err(failed)
                .map(drop),
            MutateParams::MarkChatSeen { chat_id, at } => {
                let at = at
                    .and_then(chrono::DateTime::<chrono::Utc>::from_timestamp_millis)
                    .unwrap_or_else(chrono::Utc::now);
                self.workspace
                    .mark_chat_seen(&chat_id, at)
                    .map_err(failed)
                    .map(drop)
            }
        }
    }
}

/// Reply deadline for a relay-forwarded unary call. The relay is WebSocket
/// frames through a DO: a dropped frame (host socket replaced mid-call, DO
/// restart) loses the reply SILENTLY — the DO's auto-pong keeps the client
/// socket looking healthy — and an unbounded await wedged callers forever
/// (the composer's permanent "Sending…", 2026-08-18). Network-bound git and
/// update methods get a long leash; worktree creation checks out a full tree;
/// everything else is interactive and must fail fast.
fn forward_deadline(method: &str) -> std::time::Duration {
    use std::time::Duration;
    match method {
        methods::CLONE_REPO | methods::FETCH_ALL | methods::APPLY_UPDATE => {
            Duration::from_secs(15 * 60)
        }
        methods::CREATE_WORKTREE => Duration::from_secs(120),
        methods::FETCH_TOOL_INPUT => Duration::from_secs(20),
        _ => Duration::from_secs(30),
    }
}

/// An RPC rejection is scoped to the requested capability. Only a broken
/// transport means the shared device link itself cannot carry other calls.
fn should_invalidate_link(error: &RpcError) -> bool {
    matches!(error, RpcError::Closed | RpcError::Transport(_))
}

/// ControlRpc methods that honor `targetDeviceId` (feature-inventory §2.1). Extend this
/// list (plus [`is_stream_method`] for streams) to make more of the surface
/// device-addressable — the handlers themselves need no changes.
fn forwardable(method: &str) -> bool {
    matches!(
        method,
        methods::LIST_HARNESSES
            | methods::SET_HARNESS_ENABLED
            | methods::LIST_MODELS
            | methods::LIST_COMMANDS
            | methods::QUEUE_COMMAND
            | methods::QUEUE_WORKER_NOTIFICATION
            | methods::WATCH_DOC_MESSAGES
            // Repos/worktrees/folders are device-local filesystem state.
            | methods::LIST_REPOS
            | methods::ADD_REPO
            | methods::CLONE_REPO
            | methods::CREATE_REPO
            | methods::LIST_BRANCHES
            | methods::LIST_REFS
            | methods::LIST_GIT_HISTORY
            | methods::FETCH_ALL
            | methods::SWITCH_REF
            | methods::LIST_FOLDERS
            | methods::LIST_DRIVES
            | methods::SEARCH_FILES
            | methods::CREATE_WORKTREE
            | methods::DELETE_WORKTREE
            // Checkout diffs are produced on the device holding the checkout.
            | methods::WATCH_CHECKOUT_DIFFS
            | methods::WATCH_CHECKOUT_CHANGE_REQUEST
            | methods::GET_CHECKOUT_DIFF
            | methods::GET_CHECKOUT_FILE_DIFF_TEXT
            // Terminals live on the chat's host device.
            | methods::OPEN_TERMINAL
            | methods::SUBSCRIBE_TERMINAL
            | methods::WRITE_TERMINAL
            | methods::RESIZE_TERMINAL
            | methods::CLOSE_TERMINAL
            // Agent accounts are per-device CLI logins (the device switcher
            // retargets which device's logins are shown).
            | methods::LIST_AGENT_ACCOUNTS
            | methods::ACTIVATE_AGENT_ACCOUNT
            | methods::FORGET_AGENT_ACCOUNT
            | methods::START_AGENT_LOGIN
            | methods::COMPLETE_AGENT_LOGIN
            | methods::POLL_AGENT_LOGIN
            | methods::CANCEL_AGENT_LOGIN
            // Uploads/attachments target the chat's host device (the agent reads
            // the committed file from that device's disk).
            | methods::UPLOAD_CHUNK
            | methods::UPLOAD_COMMIT
            | methods::READ_ATTACHMENT_CHUNK
            | methods::FETCH_TOOL_INPUT
            // Updates report/apply on the device whose binary they concern.
            | methods::UPDATE_STATUS
            | methods::APPLY_UPDATE
    )
}

/// Forwardable methods whose reply is a stream (proxied item-by-item).
fn is_stream_method(method: &str) -> bool {
    matches!(
        method,
        methods::WATCH_DOC_MESSAGES
            | methods::SUBSCRIBE_TERMINAL
            | methods::WATCH_CHECKOUT_DIFFS
            | methods::WATCH_CHECKOUT_CHANGE_REQUEST
            | methods::UPDATE_STATUS
    )
}

/// A watch receiver as a stream: current value first, then every change.
fn watch_stream<T>(rx: watch::Receiver<T>) -> BoxStream<'static, serde_json::Value>
where
    T: serde::Serialize + Clone + Send + Sync + 'static,
{
    futures::stream::unfold((rx, false), |(mut rx, emitted)| async move {
        if emitted {
            rx.changed().await.ok()?;
        }
        let value = {
            let borrowed = rx.borrow_and_update();
            serde_json::to_value(&*borrowed).ok()?
        };
        Some((value, (rx, true)))
    })
    .boxed()
}

/// The transcript watch as delta frames (`zeron_doc::transcript_delta`): a
/// full `reset` first, then only changed entries per commit — the whole-Vec
/// serialization here was the per-tick cost that scaled with transcript size.
fn doc_messages_stream(
    rx: watch::Receiver<Vec<zeron_doc::SessionMessageEntry>>,
) -> BoxStream<'static, serde_json::Value> {
    use zeron_doc::transcript_delta::{TranscriptFrame, diff_transcript};
    futures::stream::unfold(
        (rx, None::<Vec<zeron_doc::SessionMessageEntry>>),
        |(mut rx, mut prev)| async move {
            loop {
                if prev.is_some() {
                    rx.changed().await.ok()?;
                }
                let current: Vec<_> = rx.borrow_and_update().clone();
                let frame = match prev.as_deref() {
                    None => TranscriptFrame::reset(&current),
                    Some(prev) => diff_transcript(prev, &current),
                };
                prev = Some(current);
                // No-op commits (a second watcher attaching, command-only
                // changes) produce empty deltas — skip the frame entirely.
                if frame.is_empty_delta() {
                    continue;
                }
                let value = serde_json::to_value(&frame).ok()?;
                return Some((value, (rx, prev)));
            }
        },
    )
    .boxed()
}

fn watch_trajectory_stream(
    store: Arc<TrajectoryStore>,
    chat_id: String,
    after_cursor: Option<TrajectoryCursor>,
    limit: Option<usize>,
) -> BoxStream<'static, serde_json::Value> {
    let (tx, rx) = tokio::sync::mpsc::channel::<serde_json::Value>(256);
    let page_size = limit
        .map(|l| l.clamp(1, zeron_rpc::MAX_TRAJECTORY_PAGE_SIZE))
        .unwrap_or(zeron_rpc::DEFAULT_TRAJECTORY_PAGE_SIZE);

    tokio::spawn(async move {
        // 1. Subscribe to broadcast events BEFORE starting snapshot transaction
        let mut events_rx = store.subscribe_events();

        // 2. Stream paged snapshot under a single SQLite WAL read transaction in spawn_blocking
        let store_clone = store.clone();
        let chat_id_for_snap = chat_id.clone();
        let tx_for_snap = tx.clone();
        let snapshot_res = tokio::task::spawn_blocking(move || {
            let degraded = store_clone.get_degraded_intervals(&chat_id_for_snap)?;
            let mut is_first_page = true;
            let mut final_watermark = after_cursor;

            store_clone.stream_snapshot_pages(
                &chat_id_for_snap,
                after_cursor,
                page_size,
                |records, watermark, has_more| {
                    final_watermark = watermark;
                    let snap_item = TrajectoryWatchItem::Snapshot {
                        records,
                        watermark,
                        degraded: if is_first_page {
                            degraded.clone()
                        } else {
                            Vec::new()
                        },
                        has_more,
                    };
                    is_first_page = false;
                    if let Ok(val) = serde_json::to_value(&snap_item) {
                        tx_for_snap.blocking_send(val).is_ok()
                    } else {
                        false
                    }
                },
            )?;

            Ok::<_, crate::trajectory_store::TrajectoryStoreError>(final_watermark)
        })
        .await;

        let mut current_watermark = match snapshot_res {
            Ok(Ok(wm)) => wm,
            Ok(Err(err)) => {
                let _ = tx
                    .send(
                        serde_json::to_value(&TrajectoryWatchItem::Terminal {
                            reason: TrajectoryTerminalReason::StoreUnavailable,
                            message: Some(err.to_string()),
                        })
                        .unwrap(),
                    )
                    .await;
                return;
            }
            Err(err) => {
                let _ = tx
                    .send(
                        serde_json::to_value(&TrajectoryWatchItem::Terminal {
                            reason: TrajectoryTerminalReason::StoreUnavailable,
                            message: Some(err.to_string()),
                        })
                        .unwrap(),
                    )
                    .await;
                return;
            }
        };

        // 3. Consume live events with select! on tx.closed()
        loop {
            tokio::select! {
                _ = tx.closed() => {
                    break;
                }
                event_res = events_rx.recv() => {
                    match event_res {
                        Ok(TrajectoryStoreEvent::RecordsCommitted {
                            chat_id: event_chat_id,
                            records: committed_records,
                            watermark: _,
                        }) => {
                            if event_chat_id != chat_id {
                                continue;
                            }
                            if !committed_records.is_empty() {
                                let batch_max_cursor = committed_records
                                    .iter()
                                    .map(TrajectoryCursor::from)
                                    .max();
                                current_watermark = match (current_watermark, batch_max_cursor) {
                                    (Some(wm), Some(bm)) => Some(std::cmp::max(wm, bm)),
                                    (None, Some(bm)) => Some(bm),
                                    (Some(wm), None) => Some(wm),
                                    (None, None) => None,
                                };

                                let delta_item = TrajectoryWatchItem::Deltas {
                                    records: committed_records.as_ref().clone(),
                                    watermark: current_watermark,
                                };
                                if let Ok(val) = serde_json::to_value(&delta_item) {
                                    if tx.send(val).await.is_err() {
                                        break;
                                    }
                                }
                            }
                        }
                        Ok(TrajectoryStoreEvent::DegradedRecorded {
                            chat_id: event_chat_id,
                            interval,
                        }) => {
                            if event_chat_id != chat_id {
                                continue;
                            }
                            let deg_item = TrajectoryWatchItem::Degraded {
                                intervals: vec![interval],
                            };
                            if let Ok(val) = serde_json::to_value(&deg_item) {
                                if tx.send(val).await.is_err() {
                                    break;
                                }
                            }
                        }
                        Ok(TrajectoryStoreEvent::ChatDeleted {
                            chat_id: event_chat_id,
                        }) => {
                            if event_chat_id != chat_id {
                                continue;
                            }
                            let term_item = TrajectoryWatchItem::Terminal {
                                reason: TrajectoryTerminalReason::ChatDeleted,
                                message: Some("Chat deleted".into()),
                            };
                            if let Ok(val) = serde_json::to_value(&term_item) {
                                let _ = tx.send(val).await;
                            }
                            break;
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(skipped)) => {
                            let resync_item = TrajectoryWatchItem::ResyncRequired {
                                reason: format!("Watch stream lagged by {} broadcast events", skipped),
                            };
                            if let Ok(val) = serde_json::to_value(&resync_item) {
                                if tx.send(val).await.is_err() {
                                    break;
                                }
                            }
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                            break;
                        }
                    }
                }
            }
        }
    });

    let stream = futures::stream::unfold(rx, |mut rx| async move {
        let val = rx.recv().await?;
        Some((val, rx))
    });
    stream.boxed()
}

/// Authentication-only RPC surface used while the headed app is waiting for a
/// production WorkOS session. Keeping this independent from [`EngineRpc`] lets
/// the UI show its sign-in and organization gates before identity-scoped Loro
/// stores are opened.
#[derive(Clone)]
pub struct AuthRpc {
    auth: Auth,
}

impl AuthRpc {
    pub fn new(auth: Auth) -> Self {
        Self { auth }
    }

    pub fn handles(method: &str) -> bool {
        matches!(
            method,
            methods::AUTH_STATUS
                | methods::SIGN_IN
                | methods::SIGN_IN_HEADLESS
                | methods::COMPLETE_SIGN_IN
                | methods::SIGN_OUT
                | methods::LIST_ORGS
                | methods::CREATE_ORG
                | methods::SELECT_ORG
        )
    }
}

#[async_trait]
impl RpcService for AuthRpc {
    async fn handle(&self, method: &str, params: serde_json::Value) -> Result<RpcReply, RpcError> {
        match method {
            methods::AUTH_STATUS => Ok(RpcReply::Stream(watch_stream(self.auth.watch_state()))),
            methods::SIGN_IN => {
                let url = self
                    .auth
                    .start_sign_in()
                    .await
                    .map_err(|e| RpcError::Failed(e.to_string()))?;
                RpcReply::value(&serde_json::json!({ "url": url }))
            }
            methods::SIGN_IN_HEADLESS => {
                let url = self.auth.start_headless_sign_in();
                RpcReply::value(&serde_json::json!({ "url": url }))
            }
            methods::COMPLETE_SIGN_IN => {
                #[derive(Deserialize)]
                struct P {
                    code: String,
                }
                let p: P = parse_params(params)?;
                self.auth
                    .complete_sign_in(&p.code)
                    .await
                    .map_err(|e| RpcError::Failed(e.to_string()))?;
                RpcReply::value(&serde_json::json!({ "ok": true }))
            }
            methods::SIGN_OUT => {
                self.auth.sign_out();
                RpcReply::value(&serde_json::json!({ "ok": true }))
            }
            methods::LIST_ORGS => {
                let orgs = self
                    .auth
                    .list_orgs()
                    .await
                    .map_err(|e| RpcError::Failed(e.to_string()))?;
                RpcReply::value(&serde_json::json!({ "orgs": orgs }))
            }
            methods::CREATE_ORG => {
                #[derive(Deserialize)]
                struct P {
                    name: String,
                }
                let p: P = parse_params(params)?;
                self.auth
                    .create_org(&p.name)
                    .await
                    .map_err(|e| RpcError::Failed(e.to_string()))?;
                RpcReply::value(&serde_json::json!({ "ok": true }))
            }
            methods::SELECT_ORG => {
                #[derive(Deserialize)]
                #[serde(rename_all = "camelCase")]
                struct P {
                    organization_id: String,
                }
                let p: P = parse_params(params)?;
                self.auth
                    .select_org(&p.organization_id)
                    .await
                    .map_err(|e| RpcError::Failed(e.to_string()))?;
                RpcReply::value(&serde_json::json!({ "ok": true }))
            }
            _ => Err(RpcError::UnknownMethod(method.to_string())),
        }
    }
}

#[async_trait]
impl RpcService for EngineRpc {
    async fn handle(&self, method: &str, params: serde_json::Value) -> Result<RpcReply, RpcError> {
        // Device-addressed routing: forward calls that target another device over its
        // relay. The target compares the id to its own, so forwards cannot loop.
        if forwardable(method)
            && let Some(target) = params.get("targetDeviceId").and_then(|v| v.as_str())
            && target != self.doc_host.device_id()
        {
            let target = target.to_string();
            return self.forward(&target, method, params).await;
        }
        if AuthRpc::handles(method) {
            return AuthRpc::new(self.auth()?.clone())
                .handle(method, params)
                .await;
        }
        match method {
            methods::ENGINE_INFO => RpcReply::value(&self.engine_info),
            methods::ENGINE_READY => RpcReply::value(&serde_json::json!({ "ready": true })),
            methods::LIST_HARNESSES => RpcReply::value(&self.registry.descriptors()),
            methods::SET_HARNESS_ENABLED => {
                let p: SetHarnessEnabledParams = parse_params(params)?;
                self.registry
                    .set_enabled(p.harness, p.enabled)
                    .map_err(RpcError::Failed)?;
                // Fresh catalog in the reply: the page repaints from it in one
                // round trip, and a refused/raced toggle self-corrects.
                RpcReply::value(&self.registry.descriptors())
            }
            methods::LIST_MODELS => {
                let p: ListModelsParams = parse_params(params)?;
                let harness = self
                    .registry
                    .resolve(p.harness)
                    .map_err(|e| RpcError::Failed(e.to_string()))?;
                let models = harness
                    .models()
                    .await
                    .map_err(|e| RpcError::Failed(e.to_string()))?;
                RpcReply::value(&models)
            }
            methods::LIST_COMMANDS => {
                // Same shape as ListModels: forces a lazy resolve, then the
                // harness's own (cached) discovery — ACP agents advertise
                // availableCommands, claude answers the initialize control
                // request, codex lists skills; only harnesses whose wire has
                // no listing (cursor, mock) fall through to the trait's
                // empty default.
                let p: ListModelsParams = parse_params(params)?;
                let harness = self
                    .registry
                    .resolve(p.harness)
                    .map_err(|e| RpcError::Failed(e.to_string()))?;
                let commands = harness
                    .commands()
                    .await
                    .map_err(|e| RpcError::Failed(e.to_string()))?;
                RpcReply::value(&commands)
            }
            methods::QUEUE_COMMAND => {
                let p: QueueCommandParams = parse_params(params)?;
                let command_id = self
                    .doc_host
                    .queue_command_with_transfers(&p.chat_id, p.command, p.transfers)
                    .map_err(|e| RpcError::Failed(e.to_string()))?;
                RpcReply::value(&serde_json::json!({ "commandId": command_id }))
            }
            methods::QUEUE_WORKER_NOTIFICATION => {
                let p: QueueWorkerNotificationParams = parse_params(params)?;
                if p.command_id.is_empty()
                    || p.command_id.len() > 512
                    || p.command_id.chars().any(char::is_control)
                {
                    return Err(RpcError::Failed(
                        "commandId must be 1-512 characters without control characters".into(),
                    ));
                }
                if !matches!(p.command, SessionCommandPayload::Steer { .. }) {
                    return Err(RpcError::Failed(
                        "worker notification command must be a steer".into(),
                    ));
                }
                let command_id = self
                    .doc_host
                    .queue_worker_notification(&p.chat_id, p.command_id, p.command)
                    .map_err(|e| RpcError::Failed(e.to_string()))?;
                RpcReply::value(&serde_json::json!({ "commandId": command_id }))
            }
            methods::RETRY_DELIVERY => {
                let p: ChatParams = parse_params(params)?;
                self.doc_host
                    .retry_delivery(&p.chat_id)
                    .map_err(|e| RpcError::Failed(e.to_string()))?;
                RpcReply::value(&serde_json::json!({}))
            }
            methods::RELAY_COMMAND => {
                let p: RelayCommandParams = parse_params(params)?;
                let outcome = self
                    .doc_host
                    .ingest_relayed_command(&p.chat_id, p.entry)
                    .await
                    .map_err(|e| RpcError::Failed(e.to_string()))?;
                RpcReply::value(&serde_json::json!({ "outcome": outcome }))
            }
            methods::WATCH_DOC_MESSAGES => {
                let p: ChatParams = parse_params(params)?;
                let handle = self
                    .doc_host
                    .open(&p.chat_id)
                    .map_err(|e| RpcError::Failed(e.to_string()))?;
                Ok(RpcReply::Stream(doc_messages_stream(
                    handle.watch_messages(),
                )))
            }
            methods::PROBE_LIVE_VOICE => {
                let p: ChatParams = parse_params(params)?;
                let availability = self
                    .sessions
                    .probe_live_voice(&p.chat_id)
                    .await
                    .map_err(|error| RpcError::Failed(error.to_string()))?;
                RpcReply::value(&availability)
            }
            methods::START_LIVE_VOICE => {
                let p: ChatParams = parse_params(params)?;
                self.sessions
                    .start_live_voice(&p.chat_id)
                    .await
                    .map_err(|error| RpcError::Failed(error.to_string()))?;
                RpcReply::value(&serde_json::json!({ "active": true }))
            }
            methods::SET_LIVE_VOICE_MUTED => {
                let p: SetLiveVoiceMutedParams = parse_params(params)?;
                self.sessions
                    .set_live_voice_muted(p.muted)
                    .await
                    .map_err(|error| RpcError::Failed(error.to_string()))?;
                RpcReply::value(&serde_json::json!({ "muted": p.muted }))
            }
            methods::STOP_LIVE_VOICE => {
                self.sessions
                    .stop_live_voice()
                    .await
                    .map_err(|error| RpcError::Failed(error.to_string()))?;
                RpcReply::value(&serde_json::json!({ "active": false }))
            }
            methods::WATCH_LIVE_VOICE => Ok(RpcReply::Stream(watch_stream(
                self.sessions.watch_live_voice(),
            ))),
            methods::PROBE_SYNC => {
                self.workspace.probe();
                self.doc_host.probe_open_chats();
                self.doc_host.probe_edge_reachability();
                RpcReply::value(&serde_json::json!({}))
            }
            methods::SYNC_STATUS => {
                fn room_json(s: &zeron_sync::RoomStatsSnapshot) -> serde_json::Value {
                    serde_json::json!({
                        "connected": s.connected,
                        "synced": s.synced,
                        "lastPushedMs": s.last_pushed_ms,
                        "lastAckMs": s.last_ack_ms,
                        "rejoins": s.rejoins,
                        "probes": s.probes,
                        "fullResyncs": s.full_resyncs,
                        "disconnects": s.disconnects,
                        "rejected": s.rejected,
                    })
                }
                fn chat2_json(s: &zeron_sync::ChatStatsSnapshot) -> serde_json::Value {
                    serde_json::json!({
                        "connected": s.connected,
                        "cursor": s.cursor,
                        "headSeq": s.head_seq,
                        "seqFloor": s.seq_floor,
                        "checkpointSeq": s.checkpoint_seq,
                        "checkpointSize": s.checkpoint_size,
                        "rowCount": s.row_count,
                        "rowBytes": s.row_bytes,
                        "pendingPushes": s.pending_pushes,
                        "rejoins": s.rejoins,
                        "disconnects": s.disconnects,
                        "rejected": s.rejected,
                        "serverResets": s.server_resets,
                    })
                }
                let workspace = self.workspace.sync_status();
                let chats: Vec<serde_json::Value> = self
                    .doc_host
                    .sync_statuses()
                    .iter()
                    .map(|(chat_id, room)| {
                        serde_json::json!({
                            "chatId": chat_id,
                            "room": room.as_ref().map(chat2_json),
                        })
                    })
                    .collect();
                RpcReply::value(&serde_json::json!({
                    "deviceId": self.doc_host.device_id(),
                    "nowMs": crate::now_ms(),
                    "workspace": workspace.as_ref().map(room_json),
                    "chats": chats,
                }))
            }
            methods::WATCH_CONNECTIVITY => Ok(RpcReply::Stream(watch_stream(
                self.doc_host.watch_connectivity(),
            ))),
            methods::WATCH_TRANSFERS => Ok(RpcReply::Stream(watch_stream(
                self.doc_host.watch_transfers(),
            ))),
            methods::WATCH_CHATS => {
                Ok(RpcReply::Stream(watch_stream(self.workspace.watch_chats())))
            }
            methods::WATCH_DEVICES => Ok(RpcReply::Stream(watch_stream(
                self.workspace.watch_devices(),
            ))),
            methods::WATCH_SPACES => Ok(RpcReply::Stream(watch_stream(
                self.workspace.watch_spaces(),
            ))),
            methods::WATCH_SESSIONS => {
                // Local live statuses merged with remote devices' workspace rows.
                let merged = self
                    .workspace
                    .merged_sessions_watch(self.sessions.watch_sessions());
                Ok(RpcReply::Stream(watch_stream(merged)))
            }
            methods::LOCAL_DEVICE => {
                RpcReply::value(&serde_json::json!({ "deviceId": self.doc_host.device_id() }))
            }
            methods::LOCAL_IMPORT_STATUS => {
                let importer = self.local_importer()?.clone();
                let status = tokio::task::spawn_blocking(move || importer.status())
                    .await
                    .map_err(|e| RpcError::Failed(e.to_string()))?
                    .map_err(|e| RpcError::Failed(e.to_string()))?;
                RpcReply::value(&status)
            }
            methods::IMPORT_LOCAL_WORKSPACE => {
                let importer = self.local_importer()?.clone();
                // Progress rides an unbounded channel: the importer is
                // blocking (sqlite + fs) and must never wedge on a slow
                // viewer; items are tiny and bounded by the chat count.
                let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<serde_json::Value>();
                tokio::task::spawn_blocking(move || {
                    let emit = |event: crate::local_import::ImportEvent| {
                        if let Ok(item) = serde_json::to_value(&event) {
                            let _ = tx.send(item);
                        }
                    };
                    if let Err(err) = importer.run(emit) {
                        tracing::error!(error = %err, "local import failed");
                        let _ = tx.send(serde_json::json!({
                            "kind": "summary",
                            "importedChats": 0, "importedSpaces": 0,
                            "skippedChats": 0, "skippedSpaces": 0,
                            "journalsCopied": 0, "ledgerRowsMerged": 0,
                            "errors": [format!("{err}")],
                        }));
                    }
                    // tx drops here — the stream ends after the summary item.
                });
                Ok(RpcReply::Stream(Box::pin(futures::stream::poll_fn(
                    move |cx| rx.poll_recv(cx),
                ))))
            }
            methods::UPDATE_STATUS => Ok(RpcReply::Stream(watch_stream(self.updater()?.watch()))),
            methods::APPLY_UPDATE => {
                let version = self
                    .updater()?
                    .apply()
                    .await
                    .map_err(|e| RpcError::Failed(format!("{e:#}")))?;
                RpcReply::value(&serde_json::json!({ "ok": true, "version": version }))
            }
            methods::MUTATE => {
                let p: MutateParams = parse_params(params)?;
                self.mutate(p)?;
                RpcReply::value(&serde_json::json!({ "ok": true }))
            }
            methods::WATCH_CHECKOUT_DIFFS => {
                Ok(RpcReply::Stream(watch_stream(self.diff_sync.watch_diffs())))
            }
            methods::WATCH_CHECKOUT_CHANGE_REQUEST => {
                let p: CheckoutChangeRequestParams = parse_params(params)?;
                let cwd = self.change_request_root(&p.cwd).await?;
                let stream = self
                    .change_requests
                    .watch_for_branch(&cwd, p.branch.as_deref())
                    .await
                    .map_err(|error| RpcError::Failed(error.to_string()))?
                    .filter_map(|status| async move { serde_json::to_value(status).ok() });
                Ok(RpcReply::Stream(stream.boxed()))
            }
            // One-shot scoped capture for the Changes pane: `branch` diffs the
            // working tree against merge-base(baseRef, HEAD); `turn` diffs the
            // turn-start tree snapshot against the current tree; anything else
            // is the plain working-tree capture.
            methods::GET_CHECKOUT_DIFF => {
                #[derive(Deserialize)]
                #[serde(rename_all = "camelCase")]
                struct P {
                    cwd: String,
                    #[serde(default)]
                    mode: String,
                    base_ref: Option<String>,
                    chat_id: Option<String>,
                    commit_sha: Option<String>,
                }
                let p: P = parse_params(params)?;
                let identity = self
                    .repos
                    .checkout_identity(std::path::Path::new(&p.cwd))
                    .await
                    .map_err(|e| RpcError::Failed(e.to_string()))?;
                let root = identity.root.as_path();
                let snapshot = match p.mode.as_str() {
                    "branch" => {
                        let base_ref = p
                            .base_ref
                            .as_deref()
                            .ok_or_else(|| RpcError::Failed("baseRef required".into()))?;
                        let base = crate::diff_sync::merge_base(root, base_ref)
                            .await
                            .map_err(|e| RpcError::Failed(e.to_string()))?;
                        crate::diff_sync::capture_diff_against(&self.repos, root, Some(&base)).await
                    }
                    // One commit's own changes (History → per-commit tab):
                    // parent (or the empty tree) vs the commit itself.
                    "commit" => {
                        let sha = p
                            .commit_sha
                            .as_deref()
                            .ok_or_else(|| RpcError::Failed("commitSha required".into()))?;
                        crate::diff_sync::capture_commit_diff(&self.repos, root, sha).await
                    }
                    "turn" => {
                        let chat_id = p
                            .chat_id
                            .as_deref()
                            .ok_or_else(|| RpcError::Failed("chatId required".into()))?;
                        let snapshot = self
                            .diff_sync
                            .turn_snapshot(chat_id)
                            .filter(|s| s.root == identity.root)
                            .ok_or_else(|| RpcError::Failed("no turn recorded".into()))?;
                        crate::diff_sync::capture_turn_diff(&self.repos, root, &snapshot.tree).await
                    }
                    _ => crate::diff_sync::capture_diff(&self.repos, root).await,
                }
                .map_err(|e| RpcError::Failed(e.to_string()))?;
                RpcReply::value(&zeron_proto::CheckoutDiff {
                    checkout_id: identity.id,
                    device_id: self.doc_host.device_id().to_string(),
                    cwd: identity.root.to_string_lossy().to_string(),
                    patch: snapshot.patch,
                    files: snapshot.files,
                    additions: snapshot.additions,
                    deletions: snapshot.deletions,
                    truncated: snapshot.truncated,
                    checksum: snapshot.checksum,
                    updated_at: chrono::Utc::now(),
                })
            }
            methods::GET_CHECKOUT_FILE_DIFF_TEXT => {
                // This branch contains several large nested async futures. Keep it
                // behind an allocation so every unrelated RPC does not carry that
                // state in `EngineRpc::handle`'s stack frame.
                Box::pin(async move {
                    let p: zeron_proto::GetCheckoutFileDiffTextRequest = parse_params(params)?;
                    let identity =
                        Box::pin(self.repos.checkout_identity(std::path::Path::new(&p.cwd)))
                            .await
                            .map_err(|error| RpcError::Failed(error.to_string()))?;
                    if identity.id != p.checkout_id {
                        return Err(RpcError::Failed("checkoutId does not match cwd".into()));
                    }
                    let root = identity.root.as_path();
                    let (snapshot, base, target) = match p.mode.as_str() {
                        "branch" => {
                            let base_ref = p
                                .base_ref
                                .as_deref()
                                .ok_or_else(|| RpcError::Failed("baseRef required".into()))?;
                            let base = Box::pin(crate::diff_sync::merge_base(root, base_ref))
                                .await
                                .map_err(|error| RpcError::Failed(error.to_string()))?;
                            let snapshot = Box::pin(crate::diff_sync::capture_diff_against(
                                &self.repos,
                                root,
                                Some(&base),
                            ))
                            .await
                            .map_err(|error| RpcError::Failed(error.to_string()))?;
                            (snapshot, base, None)
                        }
                        "commit" => {
                            let sha = p
                                .commit_sha
                                .as_deref()
                                .ok_or_else(|| RpcError::Failed("commitSha required".into()))?;
                            let base =
                                Box::pin(crate::diff_sync::commit_diff_base(root, sha)).await;
                            let snapshot = Box::pin(crate::diff_sync::capture_commit_diff(
                                &self.repos,
                                root,
                                sha,
                            ))
                            .await
                            .map_err(|error| RpcError::Failed(error.to_string()))?;
                            (snapshot, base, Some(sha.to_string()))
                        }
                        "turn" => {
                            let chat_id = p
                                .chat_id
                                .as_deref()
                                .ok_or_else(|| RpcError::Failed("chatId required".into()))?;
                            let turn = self
                                .diff_sync
                                .turn_snapshot(chat_id)
                                .filter(|snapshot| snapshot.root == identity.root)
                                .ok_or_else(|| RpcError::Failed("no turn recorded".into()))?;
                            let snapshot = Box::pin(crate::diff_sync::capture_turn_diff(
                                &self.repos,
                                root,
                                &turn.tree,
                            ))
                            .await
                            .map_err(|error| RpcError::Failed(error.to_string()))?;
                            (snapshot, turn.tree, None)
                        }
                        _ => {
                            let base = Box::pin(crate::diff_sync::working_diff_base(root))
                                .await
                                .map_err(|error| RpcError::Failed(error.to_string()))?;
                            let snapshot =
                                Box::pin(crate::diff_sync::capture_diff(&self.repos, root))
                                    .await
                                    .map_err(|error| RpcError::Failed(error.to_string()))?;
                            (snapshot, base, None)
                        }
                    };
                    let stale = || zeron_proto::CheckoutFileDiffText {
                        diff_checksum: p.diff_checksum.clone(),
                        old_text: None,
                        new_text: None,
                        old_content_hash: None,
                        new_content_hash: None,
                        binary: false,
                        truncated: false,
                        stale: true,
                    };
                    if snapshot.checksum != p.diff_checksum {
                        return RpcReply::value(&stale());
                    }
                    let file = snapshot
                        .files
                        .iter()
                        .find(|file| file.path == p.path)
                        .ok_or_else(|| {
                            RpcError::Failed("path is not part of diff snapshot".into())
                        })?;
                    let pair = Box::pin(crate::diff_sync::read_diff_file_text_at(
                        root,
                        &base,
                        target.as_deref(),
                        file,
                    ))
                    .await
                    .map_err(|error| RpcError::Failed(error.to_string()))?;
                    let current = match p.mode.as_str() {
                        "branch" => {
                            Box::pin(crate::diff_sync::capture_diff_against(
                                &self.repos,
                                root,
                                Some(&base),
                            ))
                            .await
                        }
                        "turn" => {
                            Box::pin(crate::diff_sync::capture_turn_diff(
                                &self.repos,
                                root,
                                &base,
                            ))
                            .await
                        }
                        "commit" => {
                            let sha = p
                                .commit_sha
                                .as_deref()
                                .ok_or_else(|| RpcError::Failed("commitSha required".into()))?;
                            Box::pin(crate::diff_sync::capture_commit_diff(
                                &self.repos,
                                root,
                                sha,
                            ))
                            .await
                        }
                        _ => Box::pin(crate::diff_sync::capture_diff(&self.repos, root)).await,
                    }
                    .map_err(|error| RpcError::Failed(error.to_string()))?;
                    if current.checksum != p.diff_checksum {
                        return RpcReply::value(&stale());
                    }
                    RpcReply::value(&zeron_proto::CheckoutFileDiffText {
                        diff_checksum: p.diff_checksum,
                        old_text: pair.old_text,
                        new_text: pair.new_text,
                        old_content_hash: pair.old_content_hash,
                        new_content_hash: pair.new_content_hash,
                        binary: pair.binary,
                        truncated: pair.truncated,
                        stale: false,
                    })
                })
                .await
            }
            methods::LIST_REPOS => RpcReply::value(&self.repos.list().await),
            methods::ADD_REPO => {
                #[derive(Deserialize)]
                struct P {
                    path: String,
                }
                let p: P = parse_params(params)?;
                let repo = self
                    .repos
                    .add(&p.path)
                    .await
                    .map_err(|e| RpcError::Failed(e.to_string()))?;
                RpcReply::value(&repo)
            }
            methods::CLONE_REPO => {
                #[derive(Deserialize)]
                struct P {
                    url: String,
                }
                let p: P = parse_params(params)?;
                let repo = self
                    .repos
                    .clone_repo(&p.url)
                    .await
                    .map_err(|e| RpcError::Failed(e.to_string()))?;
                RpcReply::value(&repo)
            }
            methods::CREATE_REPO => {
                #[derive(Deserialize)]
                struct P {
                    name: String,
                }
                let p: P = parse_params(params)?;
                let repo = self
                    .repos
                    .create(&p.name)
                    .await
                    .map_err(|e| RpcError::Failed(e.to_string()))?;
                RpcReply::value(&repo)
            }
            methods::LIST_BRANCHES => {
                let p: RepoPathParams = parse_params(params)?;
                let branches = self
                    .repos
                    .branches(std::path::Path::new(&p.repo_path))
                    .await
                    .map_err(|e| RpcError::Failed(e.to_string()))?;
                RpcReply::value(&branches)
            }
            methods::LIST_REFS => {
                let p: RepoPathParams = parse_params(params)?;
                let refs = self
                    .repos
                    .refs(std::path::Path::new(&p.repo_path))
                    .await
                    .map_err(|e| RpcError::Failed(e.to_string()))?;
                RpcReply::value(&refs)
            }
            methods::LIST_GIT_HISTORY => {
                #[derive(Deserialize)]
                #[serde(rename_all = "camelCase")]
                struct P {
                    cwd: String,
                    #[serde(default)]
                    cursor: usize,
                    #[serde(default = "default_git_history_limit")]
                    limit: usize,
                }
                fn default_git_history_limit() -> usize {
                    crate::repos::GIT_HISTORY_DEFAULT_LIMIT
                }
                let p: P = parse_params(params)?;
                let history = self
                    .repos
                    .history(std::path::Path::new(&p.cwd), p.cursor, p.limit)
                    .await
                    .map_err(|e| RpcError::Failed(e.to_string()))?;
                RpcReply::value(&history)
            }
            methods::FETCH_ALL => {
                let p: RepoPathParams = parse_params(params)?;
                self.repos
                    .fetch_all(std::path::Path::new(&p.repo_path))
                    .await
                    .map_err(|e| RpcError::Failed(e.to_string()))?;
                // Remote refs are repository state too. Force the checkout
                // watchers to publish a fresh snapshot instead of waiting for
                // the repair tick (some platforms do not report packed-refs).
                self.diff_sync.sync_all();
                RpcReply::value(&serde_json::json!({ "ok": true }))
            }
            methods::SWITCH_REF => {
                let p: SwitchRefParams = parse_params(params)?;
                let branch = self
                    .repos
                    .switch_ref(std::path::Path::new(&p.repo_path), &p.ref_name)
                    .await
                    .map_err(|e| RpcError::Failed(e.to_string()))?;
                RpcReply::value(&serde_json::json!({ "branch": branch }))
            }
            methods::LIST_FOLDERS => {
                let p: ListFoldersParams = parse_params(params)?;
                let listing = self
                    .repos
                    .list_folders(p.path, p.include_hidden)
                    .await
                    .map_err(|e| RpcError::Failed(e.to_string()))?;
                RpcReply::value(&listing)
            }
            methods::LIST_DRIVES => {
                let drives = self
                    .repos
                    .list_drives()
                    .await
                    .map_err(|e| RpcError::Failed(e.to_string()))?;
                RpcReply::value(&zeron_proto::DriveListing { drives })
            }
            methods::SEARCH_FILES => {
                let p: FileSearchParams = parse_params(params)?;
                if p.query.chars().count() > 256 {
                    return Err(RpcError::BadParams(
                        "SearchFiles query must not exceed 256 characters".into(),
                    ));
                }
                let matches = tokio::time::timeout(FILE_SEARCH_RPC_TIMEOUT, async {
                    let root = self.file_search_root(&p).await?;
                    let featured_paths = p
                        .chat_id
                        .as_deref()
                        .filter(|_| p.query.is_empty())
                        .map(|chat_id| self.featured_file_paths(chat_id))
                        .unwrap_or_default();
                    self.repos
                        .search_files(root, p.query, featured_paths)
                        .await
                        .map_err(|e| RpcError::Failed(e.to_string()))
                })
                .await
                .map_err(|_| RpcError::Failed("file search timed out".into()))??;
                RpcReply::value(&matches)
            }
            methods::CREATE_WORKTREE => {
                let p: CreateWorktreeParams = parse_params(params)?;
                let worktree = self
                    .repos
                    .create_worktree(std::path::Path::new(&p.repo_path), &p.branch)
                    .await
                    .map_err(|e| RpcError::Failed(e.to_string()))?;
                RpcReply::value(&worktree)
            }
            methods::DELETE_WORKTREE => {
                let p: DeleteWorktreeParams = parse_params(params)?;
                self.repos
                    .delete_worktree(
                        std::path::Path::new(&p.repo_path),
                        std::path::Path::new(&p.worktree_path),
                    )
                    .await
                    .map_err(|e| RpcError::Failed(e.to_string()))?;
                RpcReply::value(&serde_json::json!({ "ok": true }))
            }
            methods::OPEN_TERMINAL => {
                let p: OpenTerminalParams = parse_params(params)?;
                let cwd = match (p.chat_id, p.cwd) {
                    (Some(chat_id), None) => self
                        .workspace
                        .chat(&chat_id)
                        .ok()
                        .flatten()
                        .and_then(|chat| chat.cwd)
                        .unwrap_or_else(|| home_dir().to_string_lossy().to_string()),
                    (None, Some(cwd)) if !cwd.trim().is_empty() => cwd,
                    _ => {
                        return Err(RpcError::BadParams(
                            "OpenTerminal requires exactly one non-empty chatId or cwd".into(),
                        ));
                    }
                };
                let session = self
                    .terminals
                    .open(&cwd, p.cols, p.rows)
                    .map_err(|e| RpcError::Failed(e.to_string()))?;
                RpcReply::value(&session)
            }
            methods::SUBSCRIBE_TERMINAL => {
                let p: SubscribeTerminalParams = parse_params(params)?;
                let rx = self
                    .terminals
                    .subscribe(&p.terminal_id, p.after_seq)
                    .map_err(|e| RpcError::Failed(e.to_string()))?;
                let stream = futures::stream::unfold(rx, |mut rx| async move {
                    let event = rx.recv().await?;
                    let value = serde_json::to_value(&event).ok()?;
                    Some((value, rx))
                });
                Ok(RpcReply::Stream(stream.boxed()))
            }
            methods::WRITE_TERMINAL => {
                let p: WriteTerminalParams = parse_params(params)?;
                self.terminals
                    .write(&p.terminal_id, &p.data)
                    .map_err(|e| RpcError::Failed(e.to_string()))?;
                RpcReply::value(&serde_json::json!({ "ok": true }))
            }
            methods::RESIZE_TERMINAL => {
                let p: ResizeTerminalParams = parse_params(params)?;
                self.terminals
                    .resize(&p.terminal_id, p.cols, p.rows)
                    .map_err(|e| RpcError::Failed(e.to_string()))?;
                RpcReply::value(&serde_json::json!({ "ok": true }))
            }
            methods::CLOSE_TERMINAL => {
                let p: TerminalIdParams = parse_params(params)?;
                self.terminals
                    .close(&p.terminal_id)
                    .map_err(|e| RpcError::Failed(e.to_string()))?;
                RpcReply::value(&serde_json::json!({ "ok": true }))
            }
            methods::LIST_AGENT_ACCOUNTS => {
                let p: ListAgentAccountsParams = parse_params(params)?;
                let snapshot = self
                    .agent_accounts
                    .list(p.force_usage.unwrap_or(false))
                    .await
                    .map_err(|e| RpcError::Failed(e.to_string()))?;
                RpcReply::value(&snapshot)
            }
            methods::ACTIVATE_AGENT_ACCOUNT => {
                let p: AgentAccountParams = parse_params(params)?;
                let snapshot = self
                    .agent_accounts
                    .activate(p.harness, &p.account_id)
                    .await
                    .map_err(|e| RpcError::Failed(e.to_string()))?;
                RpcReply::value(&snapshot)
            }
            methods::FORGET_AGENT_ACCOUNT => {
                let p: AgentAccountParams = parse_params(params)?;
                let snapshot = self
                    .agent_accounts
                    .forget(p.harness, &p.account_id)
                    .await
                    .map_err(|e| RpcError::Failed(e.to_string()))?;
                RpcReply::value(&snapshot)
            }
            methods::START_AGENT_LOGIN => {
                let p: StartAgentLoginParams = parse_params(params)?;
                let start = self
                    .agent_accounts
                    .start_login(p.harness)
                    .await
                    .map_err(|e| RpcError::Failed(e.to_string()))?;
                RpcReply::value(&start)
            }
            methods::COMPLETE_AGENT_LOGIN => {
                let p: CompleteAgentLoginParams = parse_params(params)?;
                let snapshot = self
                    .agent_accounts
                    .complete_login(&p.login_id, &p.code)
                    .await
                    .map_err(|e| RpcError::Failed(e.to_string()))?;
                RpcReply::value(&snapshot)
            }
            methods::POLL_AGENT_LOGIN => {
                let p: LoginIdParams = parse_params(params)?;
                let poll = self
                    .agent_accounts
                    .poll_login(&p.login_id)
                    .await
                    .map_err(|e| RpcError::Failed(e.to_string()))?;
                RpcReply::value(&poll)
            }
            methods::CANCEL_AGENT_LOGIN => {
                let p: LoginIdParams = parse_params(params)?;
                self.agent_accounts.cancel_login(&p.login_id);
                RpcReply::value(&serde_json::json!({ "ok": true }))
            }
            methods::UPLOAD_CHUNK => {
                let p: UploadChunkParams = parse_params(params)?;
                self.uploads
                    .append(&p.upload_id, &p.data, p.seq)
                    .map_err(|e| RpcError::Failed(e.to_string()))?;
                RpcReply::value(&serde_json::json!({ "ok": true }))
            }
            methods::UPLOAD_COMMIT => {
                let p: UploadCommitParams = parse_params(params)?;
                let path = self
                    .uploads
                    .commit(&p.upload_id, &p.file_name)
                    .map_err(|e| RpcError::Failed(e.to_string()))?;
                // Bytes just landed on this device: any command deferred on
                // them (queued-attachment refs) is executable NOW.
                self.doc_host.kick_drains();
                RpcReply::value(&serde_json::json!({ "path": path }))
            }
            methods::READ_ATTACHMENT_CHUNK => {
                let p: ReadAttachmentChunkParams = parse_params(params)?;
                // Path jail: the uploads dir plus every workspace-known chat cwd.
                let roots: Vec<std::path::PathBuf> = self
                    .workspace
                    .read_chats()
                    .unwrap_or_default()
                    .into_iter()
                    .filter_map(|chat| chat.cwd)
                    .map(std::path::PathBuf::from)
                    .collect();
                let chunk = self
                    .uploads
                    .read_chunk(&p.path, p.offset, &roots)
                    .map_err(|e| RpcError::Failed(e.to_string()))?;
                RpcReply::value(&chunk)
            }
            methods::FETCH_TOOL_BLOB => {
                let p: FetchToolBlobParams = parse_params(params)?;
                let text = self
                    .doc_host
                    .fetch_tool_blob(&p.blob_ref)
                    .await
                    .map_err(|e| RpcError::Failed(e.to_string()))?;
                RpcReply::value(&serde_json::json!({ "text": text }))
            }
            methods::FETCH_TOOL_INPUT => {
                let p: FetchToolInputParams = parse_params(params)?;
                let chat = self
                    .workspace
                    .chat(&p.chat_id)
                    .map_err(|e| RpcError::Failed(e.to_string()))?;
                require_file_tool_input_owner(
                    chat.as_ref().map(|chat| chat.device_id.as_str()),
                    self.doc_host.device_id(),
                )?;
                let sessions = self.sessions.clone();
                let chat_id = p.chat_id.clone();
                let tool_call_id = p.tool_call_id.clone();
                let parent_tool_use_id = p.parent_tool_use_id.clone();
                let snapshot = tokio::task::spawn_blocking(move || {
                    sessions.file_tool_input(
                        &chat_id,
                        &tool_call_id,
                        parent_tool_use_id.as_deref(),
                        FILE_TOOL_INPUT_RESPONSE_MAX_BYTES - FILE_TOOL_INPUT_ENVELOPE_RESERVE,
                    )
                })
                .await
                .map_err(|error| RpcError::Failed(format!("journal lookup task failed: {error}")))?
                .map_err(|error| RpcError::Failed(error.to_string()))?;
                RpcReply::value(&serde_json::json!({ "snapshot": snapshot }))
            }
            methods::WATCH_TRAJECTORY => {
                let p: WatchTrajectoryParams = parse_params(params)?;
                let chat = self
                    .workspace
                    .chat(&p.chat_id)
                    .map_err(|e| RpcError::Failed(e.to_string()))?;

                let local_device = self.doc_host.device_id();
                if let Some(c) = &chat {
                    if c.device_id != local_device {
                        let term = TrajectoryWatchItem::Terminal {
                            reason: TrajectoryTerminalReason::StoreUnavailable,
                            message: Some("Chat belongs to another device".into()),
                        };
                        let val = serde_json::to_value(&term)
                            .map_err(|e| RpcError::Failed(e.to_string()))?;
                        return Ok(RpcReply::Stream(
                            futures::stream::once(async move { val }).boxed(),
                        ));
                    }
                } else {
                    let term = TrajectoryWatchItem::Terminal {
                        reason: TrajectoryTerminalReason::ChatDeleted,
                        message: Some("Chat not found".into()),
                    };
                    let val =
                        serde_json::to_value(&term).map_err(|e| RpcError::Failed(e.to_string()))?;
                    return Ok(RpcReply::Stream(
                        futures::stream::once(async move { val }).boxed(),
                    ));
                }

                let Some(store) = self.trajectory_store() else {
                    let term = TrajectoryWatchItem::Terminal {
                        reason: TrajectoryTerminalReason::StoreUnavailable,
                        message: Some("Trajectory store unavailable".into()),
                    };
                    let val =
                        serde_json::to_value(&term).map_err(|e| RpcError::Failed(e.to_string()))?;
                    return Ok(RpcReply::Stream(
                        futures::stream::once(async move { val }).boxed(),
                    ));
                };

                let stream = watch_trajectory_stream(store, p.chat_id, p.after_cursor, p.limit);
                Ok(RpcReply::Stream(stream))
            }
            methods::REVEAL_TRAJECTORY_RAW => {
                let p: RevealTrajectoryRawParams = parse_params(params)?;
                if p.source_version != zeron_rpc::CURRENT_RAW_SOURCE_VERSION {
                    let result = TrajectoryRawRevealResult::unavailable(
                        p.field,
                        TrajectoryUnavailableReason::UnsupportedSourceVersion,
                        Some(format!("Unsupported source version {}", p.source_version)),
                    );
                    return RpcReply::value(&result);
                }

                let chat = self
                    .workspace
                    .chat(&p.chat_id)
                    .map_err(|e| RpcError::Failed(e.to_string()))?;

                let local_device = self.doc_host.device_id();
                if let Some(c) = &chat {
                    if c.device_id != local_device {
                        let result = TrajectoryRawRevealResult::unavailable(
                            p.field,
                            TrajectoryUnavailableReason::ForeignDevice,
                            Some("Chat is on a foreign device".into()),
                        );
                        return RpcReply::value(&result);
                    }
                } else {
                    let result = TrajectoryRawRevealResult::unavailable(
                        p.field,
                        TrajectoryUnavailableReason::ChatDeleted,
                        Some("Chat not found".into()),
                    );
                    return RpcReply::value(&result);
                }

                let Some(journal) = self.run_journal() else {
                    let result = TrajectoryRawRevealResult::unavailable(
                        p.field,
                        TrajectoryUnavailableReason::StoreUnavailable,
                        Some("Run journal unavailable".into()),
                    );
                    return RpcReply::value(&result);
                };

                let Some(store) = self.trajectory_store() else {
                    let result = TrajectoryRawRevealResult::unavailable(
                        p.field,
                        TrajectoryUnavailableReason::StoreUnavailable,
                        Some("Trajectory store unavailable".into()),
                    );
                    return RpcReply::value(&result);
                };

                let raw_ref = p.to_raw_ref();
                let chat_id = p.chat_id.clone();
                let source_seq = p.source_seq;
                let parent_tool_use_id = p.parent_tool_use_id.clone();
                let call_id = p.call_id.clone();
                let field = p.field;

                let reveal_res = tokio::task::spawn_blocking(move || {
                    let is_attached = store.validate_raw_ref(&raw_ref).map_err(|e| {
                        crate::run_journal::JournalError::Io(std::io::Error::other(e.to_string()))
                    })?;
                    if !is_attached {
                        return Ok(TrajectoryRawRevealResult::unavailable(
                            field,
                            TrajectoryUnavailableReason::NotFound,
                            Some(
                                "Raw reference is not attached to any stored trajectory record"
                                    .into(),
                            ),
                        ));
                    }
                    journal.raw_reveal(
                        &chat_id,
                        source_seq,
                        parent_tool_use_id.as_deref(),
                        call_id.as_deref(),
                        field,
                    )
                })
                .await
                .map_err(|e| RpcError::Failed(format!("raw reveal task failed: {e}")))?
                .map_err(|e| RpcError::Failed(e.to_string()))?;

                RpcReply::value(&reveal_res)
            }
            other => Err(RpcError::UnknownMethod(other.to_string())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The UI's Switch/Forget calls send `{id, accountId, harness}` (+ optional
    /// `targetDeviceId`); the extra fields must be tolerated, `accountId` wins.
    #[test]
    fn agent_account_params_accept_ui_shape() {
        let p: AgentAccountParams = parse_params(serde_json::json!({
            "id": "acct-1",
            "accountId": "acct-1",
            "harness": "claude-code",
            "targetDeviceId": "dev-2",
        }))
        .expect("ui param shape");
        assert_eq!(p.account_id, "acct-1");
        assert_eq!(p.harness, HarnessId::ClaudeCode);
    }

    #[test]
    fn local_device_is_not_forwardable() {
        assert!(!forwardable(methods::LOCAL_DEVICE));
        assert!(!forwardable(methods::ENGINE_INFO));
        assert!(!forwardable(methods::ENGINE_READY));
        assert!(forwardable(methods::QUEUE_COMMAND));
        assert!(forwardable(methods::QUEUE_WORKER_NOTIFICATION));
        assert!(forwardable(methods::SEARCH_FILES));
        assert!(forwardable(methods::FETCH_ALL));
        assert!(forwardable(methods::WATCH_CHECKOUT_CHANGE_REQUEST));
        assert!(forwardable(methods::FETCH_TOOL_INPUT));
        assert!(is_stream_method(methods::WATCH_CHECKOUT_CHANGE_REQUEST));
    }

    #[test]
    fn live_voice_rpc_methods_are_local_only() {
        assert!(!forwardable(methods::PROBE_LIVE_VOICE));
        assert!(!forwardable(methods::START_LIVE_VOICE));
        assert!(!forwardable(methods::SET_LIVE_VOICE_MUTED));
        assert!(!forwardable(methods::STOP_LIVE_VOICE));
        assert!(!forwardable(methods::WATCH_LIVE_VOICE));
    }

    /// Every forwardable unary method gets a bounded reply deadline —
    /// interactive calls fail fast, network-bound git/update calls get the
    /// long leash, and nothing awaits forever (the "Sending…" wedge).
    #[test]
    fn forward_deadlines_are_tiered_and_bounded() {
        use std::time::Duration;
        assert_eq!(
            forward_deadline(methods::CREATE_WORKTREE),
            Duration::from_secs(120)
        );
        assert_eq!(
            forward_deadline(methods::CLONE_REPO),
            Duration::from_secs(15 * 60)
        );
        assert_eq!(
            forward_deadline(methods::LIST_BRANCHES),
            Duration::from_secs(30)
        );
        assert_eq!(
            forward_deadline(methods::QUEUE_COMMAND),
            Duration::from_secs(30)
        );
        assert_eq!(
            forward_deadline(methods::FETCH_TOOL_INPUT),
            Duration::from_secs(20)
        );
    }

    #[test]
    fn fetch_tool_input_owner_requires_an_existing_local_chat() {
        assert!(require_file_tool_input_owner(Some("dev-a"), "dev-a").is_ok());
        assert!(require_file_tool_input_owner(Some("dev-b"), "dev-a").is_err());
        assert!(require_file_tool_input_owner(None, "dev-a").is_err());
    }

    #[test]
    fn fetch_tool_input_reserve_covers_the_complete_max_id_server_frame() {
        assert!(FILE_TOOL_INPUT_ENVELOPE_RESERVE >= 64);
        let snapshot = zeron_proto::FileToolInputSnapshot {
            path: "quoted\"\\\u{0001}.txt".into(),
            content: Some("\u{0001}\"\\".repeat(32)),
            old_string: None,
            new_string: None,
            truncated: true,
        };
        let snapshot_len = serde_json::to_vec(&snapshot).unwrap().len();
        let frame = zeron_rpc::ServerFrame {
            id: u64::MAX,
            ok: Some(serde_json::json!({ "snapshot": snapshot })),
            ..Default::default()
        };
        let frame_len = serde_json::to_vec(&frame).unwrap().len();
        let envelope = frame_len.saturating_sub(snapshot_len);
        assert!(envelope <= FILE_TOOL_INPUT_ENVELOPE_RESERVE);
        assert!(
            FILE_TOOL_INPUT_RESPONSE_MAX_BYTES - FILE_TOOL_INPUT_ENVELOPE_RESERVE + envelope
                <= FILE_TOOL_INPUT_RESPONSE_MAX_BYTES
        );
    }

    #[test]
    fn tool_file_paths_keep_workspace_activity_only() {
        assert_eq!(
            tool_file_path(&ToolCall::EditFile {
                path: "src/main.rs".into(),
                old_string: None,
                new_string: None,
            }),
            Some("src/main.rs")
        );
        assert_eq!(
            tool_file_path(&ToolCall::Exec {
                command: "cargo test".into(),
            }),
            None
        );
    }

    // -----------------------------------------------------------------------
    // Trajectory Transport Tests (F2)
    // -----------------------------------------------------------------------

    use zeron_proto::trajectory::*;

    fn sample_record(chat_id: &str, run_id: &str, seq: u64, sub: u32) -> TrajectoryRecord {
        TrajectoryRecord {
            id: TrajectoryRecordId::new(run_id, seq, sub),
            chat_id: chat_id.to_string(),
            run_id: run_id.to_string(),
            source_seq: seq,
            sub_seq: sub,
            lane: TrajectoryLane::Input,
            kind: TrajectoryRecordKind::UserMessage,
            status: TrajectoryStatus::Completed,
            is_partial: false,
            title: "User".into(),
            summary: "Hello".into(),
            turn_id: None,
            step_id: None,
            call_id: None,
            parent_tool_use_id: None,
            timing: None,
            usage: None,
            payload: None,
            result: None,
            error_message: None,
            is_degraded: false,
        }
    }

    fn sample_record_with_payload_raw_ref(
        chat_id: &str,
        run_id: &str,
        seq: u64,
        sub: u32,
        parent_tool_use_id: Option<String>,
        call_id: Option<String>,
    ) -> TrajectoryRecord {
        let mut r = sample_record(chat_id, run_id, seq, sub);
        r.parent_tool_use_id = parent_tool_use_id.clone();
        r.call_id = call_id.clone();
        r.payload = Some(TrajectoryPayloadPreview {
            summary: "summary".into(),
            sanitized_text: None,
            schema_info: None,
            raw_ref: Some(TrajectoryRawRef::new(
                chat_id,
                seq,
                parent_tool_use_id,
                call_id,
                TrajectoryRawField::Payload,
            )),
        });
        r
    }

    fn sample_record_with_result_raw_ref(
        chat_id: &str,
        run_id: &str,
        seq: u64,
        sub: u32,
        parent_tool_use_id: Option<String>,
        call_id: Option<String>,
    ) -> TrajectoryRecord {
        let mut r = sample_record(chat_id, run_id, seq, sub);
        r.parent_tool_use_id = parent_tool_use_id.clone();
        r.call_id = call_id.clone();
        r.result = Some(TrajectoryResultPreview {
            summary: "summary".into(),
            sanitized_text: None,
            is_error: false,
            exit_code: None,
            raw_ref: Some(TrajectoryRawRef::new(
                chat_id,
                seq,
                parent_tool_use_id,
                call_id,
                TrajectoryRawField::Result,
            )),
        });
        r
    }

    async fn setup_test_engine(temp: &tempfile::TempDir) -> (crate::EngineCore, Arc<EngineRpc>) {
        let core = crate::EngineCore::assemble(
            temp.path(),
            Arc::new(HarnessRegistry::new()),
            HarnessId::Mock,
            None,
        )
        .expect("assemble engine");

        let rpc = core.rpc_service();

        (core, rpc)
    }

    #[test]
    fn test_trajectory_rpc_methods_non_forwardable() {
        assert!(!forwardable(methods::WATCH_TRAJECTORY));
        assert!(!forwardable(methods::REVEAL_TRAJECTORY_RAW));
    }

    #[tokio::test]
    async fn test_trajectory_watch_partial_to_final_replacement_regression() {
        let temp = tempfile::TempDir::new().unwrap();
        let (core, _rpc) = setup_test_engine(&temp).await;
        let store = core.trajectory.clone();

        let chat_id = "chat_partial_final_replacement";
        let mut r_partial = sample_record(chat_id, "run1", 5, 0);
        r_partial.is_partial = true;
        r_partial.summary = "In-flight assistant response".into();

        store.try_enqueue(r_partial.clone()).unwrap();
        store.sync_flush().unwrap();

        let mut stream = watch_trajectory_stream(store.clone(), chat_id.into(), None, None);

        // Snapshot delivers partial record
        let snap_val = stream.next().await.expect("snapshot item");
        let snap_item: TrajectoryWatchItem = serde_json::from_value(snap_val).unwrap();
        match snap_item {
            TrajectoryWatchItem::Snapshot {
                records,
                watermark,
                has_more,
                ..
            } => {
                assert_eq!(records.len(), 1);
                assert!(records[0].is_partial);
                assert_eq!(records[0].source_seq, 5);
                assert_eq!(records[0].sub_seq, 0);
                assert_eq!(watermark, Some(TrajectoryCursor::new(5, 0)));
                assert!(!has_more);
            }
            other => panic!("expected Snapshot, got {:?}", other),
        }

        // Now final replacement record at (5, 0) is committed with is_partial = false
        let mut r_final = sample_record(chat_id, "run1", 5, 0);
        r_final.is_partial = false;
        r_final.summary = "Completed assistant response".into();

        store.try_enqueue(r_final.clone()).unwrap();
        store.sync_flush().unwrap();

        // Must receive Deltas with the final replacement record (not suppressed!)
        let delta_val = stream.next().await.expect("delta item");
        let delta_item: TrajectoryWatchItem = serde_json::from_value(delta_val).unwrap();
        match delta_item {
            TrajectoryWatchItem::Deltas { records, watermark } => {
                assert_eq!(records.len(), 1);
                assert!(!records[0].is_partial);
                assert_eq!(records[0].source_seq, 5);
                assert_eq!(records[0].sub_seq, 0);
                assert_eq!(records[0].summary, "Completed assistant response");
                assert_eq!(watermark, Some(TrajectoryCursor::new(5, 0)));
            }
            other => panic!("expected Deltas, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_trajectory_watch_bounded_multi_frame_paging() {
        let temp = tempfile::TempDir::new().unwrap();
        let (core, _rpc) = setup_test_engine(&temp).await;
        let store = core.trajectory.clone();

        let chat_id = "chat_paging_5_records";
        let mut recs = Vec::new();
        for seq in 1..=5 {
            recs.push(sample_record(chat_id, "run1", seq, 0));
        }
        store.try_enqueue_batch(recs).unwrap();
        store.sync_flush().unwrap();

        // Request limit = 2 on 5 historical records
        let mut stream = watch_trajectory_stream(store.clone(), chat_id.into(), None, Some(2));

        // Frame 1: 2 records, has_more: true, watermark (2, 0)
        let f1_val = stream.next().await.expect("frame 1");
        let f1: TrajectoryWatchItem = serde_json::from_value(f1_val).unwrap();
        match f1 {
            TrajectoryWatchItem::Snapshot {
                records,
                watermark,
                has_more,
                ..
            } => {
                assert_eq!(records.len(), 2);
                assert_eq!(records[0].source_seq, 1);
                assert_eq!(records[1].source_seq, 2);
                assert_eq!(watermark, Some(TrajectoryCursor::new(2, 0)));
                assert!(has_more, "frame 1 must have has_more: true");
            }
            other => panic!("expected Snapshot frame 1, got {:?}", other),
        }

        // Frame 2: 2 records, has_more: true, watermark (4, 0)
        let f2_val = stream.next().await.expect("frame 2");
        let f2: TrajectoryWatchItem = serde_json::from_value(f2_val).unwrap();
        match f2 {
            TrajectoryWatchItem::Snapshot {
                records,
                watermark,
                has_more,
                ..
            } => {
                assert_eq!(records.len(), 2);
                assert_eq!(records[0].source_seq, 3);
                assert_eq!(records[1].source_seq, 4);
                assert_eq!(watermark, Some(TrajectoryCursor::new(4, 0)));
                assert!(has_more, "frame 2 must have has_more: true");
            }
            other => panic!("expected Snapshot frame 2, got {:?}", other),
        }

        // Frame 3: 1 record, has_more: false, watermark (5, 0)
        let f3_val = stream.next().await.expect("frame 3");
        let f3: TrajectoryWatchItem = serde_json::from_value(f3_val).unwrap();
        match f3 {
            TrajectoryWatchItem::Snapshot {
                records,
                watermark,
                has_more,
                ..
            } => {
                assert_eq!(records.len(), 1);
                assert_eq!(records[0].source_seq, 5);
                assert_eq!(watermark, Some(TrajectoryCursor::new(5, 0)));
                assert!(!has_more, "frame 3 must have has_more: false");
            }
            other => panic!("expected Snapshot frame 3, got {:?}", other),
        }

        // Live commit arrives after paging
        let r6 = sample_record(chat_id, "run1", 6, 0);
        store.try_enqueue(r6).unwrap();
        store.sync_flush().unwrap();

        let delta_val = stream.next().await.expect("live delta item");
        let delta_item: TrajectoryWatchItem = serde_json::from_value(delta_val).unwrap();
        match delta_item {
            TrajectoryWatchItem::Deltas { records, watermark } => {
                assert_eq!(records.len(), 1);
                assert_eq!(records[0].source_seq, 6);
                assert_eq!(watermark, Some(TrajectoryCursor::new(6, 0)));
            }
            other => panic!("expected Deltas, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_trajectory_watch_commit_during_paging_and_late_earlier_cursor() {
        let temp = tempfile::TempDir::new().unwrap();
        let (core, _rpc) = setup_test_engine(&temp).await;
        let store = core.trajectory.clone();

        let chat_id = "chat_commit_during_paging";
        let mut recs = Vec::new();
        for seq in 1..=4 {
            recs.push(sample_record(chat_id, "run1", seq, 0));
        }
        store.try_enqueue_batch(recs).unwrap();
        store.sync_flush().unwrap();

        // Start stream with page_size = 2
        let mut stream = watch_trajectory_stream(store.clone(), chat_id.into(), None, Some(2));

        // Read Frame 1 (records 1 and 2)
        let _f1 = stream.next().await.expect("frame 1");

        // Concurrent commit happens during paging:
        // Record at earlier sequence (e.g. coalesced reasoning at (2, 1)) AND new record at (5, 0)
        let r_late_earlier = sample_record(chat_id, "run1", 2, 1);
        let r5 = sample_record(chat_id, "run1", 5, 0);
        store
            .try_enqueue_batch(vec![r_late_earlier.clone(), r5.clone()])
            .unwrap();
        store.sync_flush().unwrap();

        // Frame 2 (records 3 and 4 with has_more: false, isolated from concurrent commits)
        let f2_val = stream.next().await.expect("frame 2");
        let f2: TrajectoryWatchItem = serde_json::from_value(f2_val).unwrap();
        match f2 {
            TrajectoryWatchItem::Snapshot {
                records,
                watermark,
                has_more,
                ..
            } => {
                assert_eq!(records.len(), 2);
                assert_eq!(records[0].source_seq, 3);
                assert_eq!(records[1].source_seq, 4);
                assert_eq!(watermark, Some(TrajectoryCursor::new(4, 0)));
                assert!(
                    !has_more,
                    "snapshot transaction had 4 records, so frame 2 has has_more: false"
                );
            }
            other => panic!("expected Snapshot frame 2, got {:?}", other),
        }

        // Live Deltas receive the committed batch (including late earlier cursor and new record)
        let delta_val = stream.next().await.expect("delta item");
        let delta_item: TrajectoryWatchItem = serde_json::from_value(delta_val).unwrap();
        match delta_item {
            TrajectoryWatchItem::Deltas { records, watermark } => {
                assert_eq!(records.len(), 2);
                assert_eq!(records[0].source_seq, 2);
                assert_eq!(records[0].sub_seq, 1);
                assert_eq!(records[1].source_seq, 5);
                assert_eq!(records[1].sub_seq, 0);
                assert_eq!(watermark, Some(TrajectoryCursor::new(5, 0)));
            }
            other => panic!("expected Deltas, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_trajectory_watch_reconnect_with_cursor_and_pagination() {
        let temp = tempfile::TempDir::new().unwrap();
        let (core, _rpc) = setup_test_engine(&temp).await;
        let store = core.trajectory.clone();

        let chat_id = "chat_reconnect";
        let r1 = sample_record(chat_id, "run1", 1, 0);
        let r2 = sample_record(chat_id, "run1", 1, 1);
        let r3 = sample_record(chat_id, "run1", 2, 0);
        store
            .try_enqueue_batch(vec![r1.clone(), r2.clone(), r3.clone()])
            .unwrap();
        store.sync_flush().unwrap();

        // Reconnect after cursor (1, 0) -> should receive (1, 1) and (2, 0) in snapshot
        let mut stream = watch_trajectory_stream(
            store.clone(),
            chat_id.into(),
            Some(TrajectoryCursor::new(1, 0)),
            None,
        );

        let snap_val = stream.next().await.expect("snapshot item");
        let snap_item: TrajectoryWatchItem = serde_json::from_value(snap_val).unwrap();
        match snap_item {
            TrajectoryWatchItem::Snapshot {
                records, watermark, ..
            } => {
                assert_eq!(records.len(), 2);
                assert_eq!(records[0].source_seq, 1);
                assert_eq!(records[0].sub_seq, 1);
                assert_eq!(records[1].source_seq, 2);
                assert_eq!(records[1].sub_seq, 0);
                assert_eq!(watermark, Some(TrajectoryCursor::new(2, 0)));
            }
            other => panic!("expected Snapshot, got {:?}", other),
        }

        // Live record arrives at (2, 1)
        let r4 = sample_record(chat_id, "run1", 2, 1);
        store.try_enqueue_batch(vec![r4.clone()]).unwrap();
        store.sync_flush().unwrap();

        let delta_val = stream.next().await.expect("delta item");
        let delta_item: TrajectoryWatchItem = serde_json::from_value(delta_val).unwrap();
        match delta_item {
            TrajectoryWatchItem::Deltas { records, watermark } => {
                assert_eq!(records.len(), 1);
                assert_eq!(records[0].source_seq, 2);
                assert_eq!(records[0].sub_seq, 1);
                assert_eq!(watermark, Some(TrajectoryCursor::new(2, 1)));
            }
            other => panic!("expected Deltas, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_trajectory_watch_same_source_different_sub_seq_ordering() {
        let temp = tempfile::TempDir::new().unwrap();
        let (core, _rpc) = setup_test_engine(&temp).await;
        let store = core.trajectory.clone();

        let chat_id = "chat_same_source_sub";
        let r_prefix = sample_record(chat_id, "run1", 10, 0);
        let r_mid = sample_record(chat_id, "run1", 10, 1);
        let r_interrupted = sample_record(chat_id, "run1", 10, u32::MAX);

        store
            .try_enqueue_batch(vec![r_prefix.clone(), r_mid.clone(), r_interrupted.clone()])
            .unwrap();
        store.sync_flush().unwrap();

        let records = store
            .list_records_after_cursor(chat_id, Some(TrajectoryCursor::new(10, 0)), None)
            .unwrap();
        assert_eq!(records.len(), 2);
        assert_eq!(records[0].sub_seq, 1);
        assert_eq!(records[1].sub_seq, u32::MAX);

        let records_from_mid = store
            .list_records_after_cursor(chat_id, Some(TrajectoryCursor::new(10, 1)), None)
            .unwrap();
        assert_eq!(records_from_mid.len(), 1);
        assert_eq!(records_from_mid[0].sub_seq, u32::MAX);
    }

    #[tokio::test]
    async fn test_trajectory_watch_cancellation_does_not_affect_capture() {
        let temp = tempfile::TempDir::new().unwrap();
        let (core, _rpc) = setup_test_engine(&temp).await;
        let store = core.trajectory.clone();

        let chat_id = "chat_cancel";
        let r1 = sample_record(chat_id, "run1", 1, 0);
        store.try_enqueue(r1).unwrap();
        store.sync_flush().unwrap();

        let stream = watch_trajectory_stream(store.clone(), chat_id.into(), None, None);
        // Explicitly drop stream to simulate client cancellation / closing surface
        drop(stream);

        // Capture continues uninterrupted
        let r2 = sample_record(chat_id, "run1", 2, 0);
        let r3 = sample_record(chat_id, "run1", 3, 0);
        assert!(store.try_enqueue_batch(vec![r2, r3]).is_ok());
        assert!(store.sync_flush().is_ok());

        let all = store.list_records(chat_id, None, None).unwrap();
        assert_eq!(all.len(), 3);
    }

    #[tokio::test]
    async fn test_trajectory_watch_chat_deleted_terminal() {
        let temp = tempfile::TempDir::new().unwrap();
        let (core, _rpc) = setup_test_engine(&temp).await;
        let store = core.trajectory.clone();

        let chat_id = "chat_watch_delete";
        let r1 = sample_record(chat_id, "run1", 1, 0);
        store.try_enqueue(r1).unwrap();
        store.sync_flush().unwrap();

        let mut stream = watch_trajectory_stream(store.clone(), chat_id.into(), None, None);
        let _snap = stream.next().await.expect("snapshot");

        // Delete chat
        store.delete_chat(chat_id).await.unwrap();

        // Terminal item must arrive
        let term_val = stream.next().await.expect("terminal event");
        let term_item: TrajectoryWatchItem = serde_json::from_value(term_val).unwrap();
        match term_item {
            TrajectoryWatchItem::Terminal { reason, .. } => {
                assert_eq!(reason, TrajectoryTerminalReason::ChatDeleted);
            }
            other => panic!("expected Terminal, got {:?}", other),
        }

        // Stream must end
        assert!(stream.next().await.is_none());
    }

    #[tokio::test]
    async fn test_trajectory_watch_degraded_store_reporting() {
        let temp = tempfile::TempDir::new().unwrap();
        let deg_store = Arc::new(TrajectoryStore::degraded(temp.path(), "disk failure"));

        let mut stream = watch_trajectory_stream(deg_store, "chat_deg".into(), None, None);
        let snap_val = stream.next().await.expect("snapshot");
        let snap_item: TrajectoryWatchItem = serde_json::from_value(snap_val).unwrap();
        match snap_item {
            TrajectoryWatchItem::Snapshot { records, .. } => {
                assert!(records.is_empty());
            }
            other => panic!("expected Snapshot, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_trajectory_watch_foreign_chat_terminal() {
        let temp = tempfile::TempDir::new().unwrap();
        let (core, rpc) = setup_test_engine(&temp).await;

        // Create a chat hosted on another device
        core.workspace
            .create_chat("chat_foreign", None, Some("other-dev-99"), None, None)
            .unwrap();

        let reply = rpc
            .handle(
                methods::WATCH_TRAJECTORY,
                serde_json::json!({
                    "chatId": "chat_foreign",
                }),
            )
            .await
            .unwrap();

        match reply {
            RpcReply::Stream(mut stream) => {
                let val = stream.next().await.unwrap();
                let item: TrajectoryWatchItem = serde_json::from_value(val).unwrap();
                match item {
                    TrajectoryWatchItem::Terminal { reason, .. } => {
                        assert_eq!(reason, TrajectoryTerminalReason::StoreUnavailable);
                    }
                    other => panic!("expected Terminal, got {:?}", other),
                }
            }
            _ => panic!("expected stream reply"),
        }
    }

    #[tokio::test]
    async fn test_trajectory_reveal_ownership_and_local_only() {
        let temp = tempfile::TempDir::new().unwrap();
        let (core, rpc) = setup_test_engine(&temp).await;

        let local_device = core.doc_host.device_id().to_string();
        core.workspace
            .create_chat("chat_local", None, Some(&local_device), None, None)
            .unwrap();
        core.workspace
            .create_chat("chat_remote", None, Some("foreign-dev"), None, None)
            .unwrap();

        // Foreign chat reveal -> Unavailable(ForeignDevice)
        let reply_foreign = rpc
            .handle(
                methods::REVEAL_TRAJECTORY_RAW,
                serde_json::json!({
                    "chatId": "chat_remote",
                    "sourceSeq": 1,
                    "field": "payload",
                }),
            )
            .await
            .unwrap();

        match reply_foreign {
            RpcReply::Value(val) => {
                let res: TrajectoryRawRevealResult = serde_json::from_value(val).unwrap();
                match res {
                    TrajectoryRawRevealResult::Unavailable { reason, .. } => {
                        assert_eq!(reason, TrajectoryUnavailableReason::ForeignDevice);
                    }
                    _ => panic!("expected unavailable foreign device"),
                }
            }
            _ => panic!("expected value reply"),
        }

        // Nonexistent chat reveal -> Unavailable(ChatDeleted / NotFound)
        let reply_missing = rpc
            .handle(
                methods::REVEAL_TRAJECTORY_RAW,
                serde_json::json!({
                    "chatId": "chat_unknown",
                    "sourceSeq": 1,
                    "field": "payload",
                }),
            )
            .await
            .unwrap();

        match reply_missing {
            RpcReply::Value(val) => {
                let res: TrajectoryRawRevealResult = serde_json::from_value(val).unwrap();
                match res {
                    TrajectoryRawRevealResult::Unavailable { reason, .. } => {
                        assert_eq!(reason, TrajectoryUnavailableReason::ChatDeleted);
                    }
                    _ => panic!("expected unavailable chat deleted"),
                }
            }
            _ => panic!("expected value reply"),
        }
    }

    #[tokio::test]
    async fn test_trajectory_reveal_payload_and_result_fields() {
        let temp = tempfile::TempDir::new().unwrap();
        let (core, rpc) = setup_test_engine(&temp).await;
        let local_device = core.doc_host.device_id().to_string();
        let chat_id = "chat_reveal_fields";

        core.workspace
            .create_chat(chat_id, None, Some(&local_device), None, None)
            .unwrap();

        let journal = rpc.run_journal().expect("run journal");
        let store = rpc.trajectory_store().expect("trajectory store");

        let seq1 = journal
            .append(
                chat_id,
                &zeron_proto::AgentEvent::UserMessage {
                    text: "Prompt text with sensitive credentials".into(),
                },
            )
            .unwrap();

        let seq2 = journal
            .append(
                chat_id,
                &zeron_proto::AgentEvent::ToolCall {
                    id: "tool-call-100".into(),
                    call: ToolCall::WriteFile {
                        path: "config.json".into(),
                        content: Some("{\n  \"apiKey\": \"secret-12345\"\n}".into()),
                    },
                },
            )
            .unwrap();

        let seq3 = journal
            .append(
                chat_id,
                &zeron_proto::AgentEvent::ToolResult {
                    id: "tool-call-100".into(),
                    is_error: false,
                    output: Some("File written successfully (100 bytes)".into()),
                    diff: None,
                    execution: None,
                },
            )
            .unwrap();

        let r1 = sample_record_with_payload_raw_ref(chat_id, "run1", seq1, 0, None, None);
        let r2 = sample_record_with_payload_raw_ref(
            chat_id,
            "run1",
            seq2,
            0,
            None,
            Some("tool-call-100".into()),
        );
        let r3 = sample_record_with_result_raw_ref(
            chat_id,
            "run1",
            seq3,
            0,
            None,
            Some("tool-call-100".into()),
        );
        store.try_enqueue_batch(vec![r1, r2, r3]).unwrap();
        store.sync_flush().unwrap();

        // 1. Reveal UserMessage Payload
        let r1 = rpc
            .handle(
                methods::REVEAL_TRAJECTORY_RAW,
                serde_json::json!({
                    "chatId": chat_id,
                    "sourceSeq": seq1,
                    "field": "payload",
                }),
            )
            .await
            .unwrap();
        if let RpcReply::Value(val) = r1 {
            let res: TrajectoryRawRevealResult = serde_json::from_value(val).unwrap();
            match res {
                TrajectoryRawRevealResult::Available { text, field } => {
                    assert_eq!(field, TrajectoryRawField::Payload);
                    assert_eq!(text, "Prompt text with sensitive credentials");
                }
                other => panic!("expected Available, got {:?}", other),
            }
        }

        // 2. Reveal ToolCall Payload
        let r2 = rpc
            .handle(
                methods::REVEAL_TRAJECTORY_RAW,
                serde_json::json!({
                    "chatId": chat_id,
                    "sourceSeq": seq2,
                    "callId": "tool-call-100",
                    "field": "payload",
                }),
            )
            .await
            .unwrap();
        if let RpcReply::Value(val) = r2 {
            let res: TrajectoryRawRevealResult = serde_json::from_value(val).unwrap();
            match res {
                TrajectoryRawRevealResult::Available { text, field } => {
                    assert_eq!(field, TrajectoryRawField::Payload);
                    assert_eq!(text, "{\n  \"apiKey\": \"secret-12345\"\n}");
                }
                other => panic!("expected Available, got {:?}", other),
            }
        }

        // 3. Reveal ToolResult Result
        let r3 = rpc
            .handle(
                methods::REVEAL_TRAJECTORY_RAW,
                serde_json::json!({
                    "chatId": chat_id,
                    "sourceSeq": seq3,
                    "callId": "tool-call-100",
                    "field": "result",
                }),
            )
            .await
            .unwrap();
        if let RpcReply::Value(val) = r3 {
            let res: TrajectoryRawRevealResult = serde_json::from_value(val).unwrap();
            match res {
                TrajectoryRawRevealResult::Available { text, field } => {
                    assert_eq!(field, TrajectoryRawField::Result);
                    assert_eq!(text, "File written successfully (100 bytes)");
                }
                other => panic!("expected Available, got {:?}", other),
            }
        }
    }

    #[tokio::test]
    async fn test_trajectory_reveal_nested_subagent_scoping() {
        let temp = tempfile::TempDir::new().unwrap();
        let (core, rpc) = setup_test_engine(&temp).await;
        let local_device = core.doc_host.device_id().to_string();
        let chat_id = "chat_subagent_reveal";

        core.workspace
            .create_chat(chat_id, None, Some(&local_device), None, None)
            .unwrap();

        let journal = rpc.run_journal().expect("run journal");
        let store = rpc.trajectory_store().expect("trajectory store");

        let subagent_seq = journal
            .append(
                chat_id,
                &zeron_proto::AgentEvent::Subagent {
                    parent_tool_use_id: "parent-tool-spawn-1".into(),
                    event: Box::new(zeron_proto::AgentEvent::UserMessage {
                        text: "Inner subagent directive text".into(),
                    }),
                },
            )
            .unwrap();

        let r_sub = sample_record_with_payload_raw_ref(
            chat_id,
            "run1",
            subagent_seq,
            0,
            Some("parent-tool-spawn-1".into()),
            None,
        );
        store.try_enqueue(r_sub).unwrap();
        store.sync_flush().unwrap();

        // Reveal with matching parentToolUseId
        let r_match = rpc
            .handle(
                methods::REVEAL_TRAJECTORY_RAW,
                serde_json::json!({
                    "chatId": chat_id,
                    "sourceSeq": subagent_seq,
                    "parentToolUseId": "parent-tool-spawn-1",
                    "field": "payload",
                }),
            )
            .await
            .unwrap();
        if let RpcReply::Value(val) = r_match {
            let res: TrajectoryRawRevealResult = serde_json::from_value(val).unwrap();
            match res {
                TrajectoryRawRevealResult::Available { text, field } => {
                    assert_eq!(field, TrajectoryRawField::Payload);
                    assert_eq!(text, "Inner subagent directive text");
                }
                other => panic!("expected Available, got {:?}", other),
            }
        }

        // Reveal with missing parentToolUseId -> MismatchedReference
        let r_mismatch = rpc
            .handle(
                methods::REVEAL_TRAJECTORY_RAW,
                serde_json::json!({
                    "chatId": chat_id,
                    "sourceSeq": subagent_seq,
                    "field": "payload",
                }),
            )
            .await
            .unwrap();
        if let RpcReply::Value(val) = r_mismatch {
            let res: TrajectoryRawRevealResult = serde_json::from_value(val).unwrap();
            match res {
                TrajectoryRawRevealResult::Unavailable { reason, .. } => {
                    assert_eq!(reason, TrajectoryUnavailableReason::NotFound);
                }
                other => panic!("expected Unavailable NotFound, got {:?}", other),
            }
        }
    }

    #[tokio::test]
    async fn test_trajectory_reveal_mismatched_reference() {
        let temp = tempfile::TempDir::new().unwrap();
        let (core, rpc) = setup_test_engine(&temp).await;
        let local_device = core.doc_host.device_id().to_string();
        let chat_id = "chat_mismatched_ref";

        core.workspace
            .create_chat(chat_id, None, Some(&local_device), None, None)
            .unwrap();

        let journal = rpc.run_journal().expect("run journal");
        let store = rpc.trajectory_store().expect("trajectory store");

        let seq = journal
            .append(
                chat_id,
                &zeron_proto::AgentEvent::ToolCall {
                    id: "tool-expected-1".into(),
                    call: ToolCall::Exec {
                        command: "ls -la".into(),
                    },
                },
            )
            .unwrap();

        let r_tool = sample_record_with_payload_raw_ref(
            chat_id,
            "run1",
            seq,
            0,
            None,
            Some("tool-expected-1".into()),
        );
        store.try_enqueue(r_tool).unwrap();
        store.sync_flush().unwrap();

        // 1. Wrong call ID -> NotFound (unattached)
        let r_wrong_call = rpc
            .handle(
                methods::REVEAL_TRAJECTORY_RAW,
                serde_json::json!({
                    "chatId": chat_id,
                    "sourceSeq": seq,
                    "callId": "tool-wrong-2",
                    "field": "payload",
                }),
            )
            .await
            .unwrap();
        if let RpcReply::Value(val) = r_wrong_call {
            let res: TrajectoryRawRevealResult = serde_json::from_value(val).unwrap();
            match res {
                TrajectoryRawRevealResult::Unavailable { reason, .. } => {
                    assert_eq!(reason, TrajectoryUnavailableReason::NotFound);
                }
                other => panic!("expected Unavailable NotFound, got {:?}", other),
            }
        }

        // 2. Wrong field (ToolCall has Payload, but requesting Result) -> NotFound (unattached)
        let r_wrong_field = rpc
            .handle(
                methods::REVEAL_TRAJECTORY_RAW,
                serde_json::json!({
                    "chatId": chat_id,
                    "sourceSeq": seq,
                    "callId": "tool-expected-1",
                    "field": "result",
                }),
            )
            .await
            .unwrap();
        if let RpcReply::Value(val) = r_wrong_field {
            let res: TrajectoryRawRevealResult = serde_json::from_value(val).unwrap();
            match res {
                TrajectoryRawRevealResult::Unavailable { reason, .. } => {
                    assert_eq!(reason, TrajectoryUnavailableReason::NotFound);
                }
                other => panic!("expected Unavailable NotFound, got {:?}", other),
            }
        }
    }

    #[tokio::test]
    async fn test_trajectory_reveal_oversized_line() {
        let temp = tempfile::TempDir::new().unwrap();
        let (core, rpc) = setup_test_engine(&temp).await;
        let local_device = core.doc_host.device_id().to_string();
        let chat_id = "chat_oversized";

        core.workspace
            .create_chat(chat_id, None, Some(&local_device), None, None)
            .unwrap();

        let store = rpc.trajectory_store().expect("trajectory store");
        let r_over = sample_record_with_payload_raw_ref(chat_id, "run1", 1, 0, None, None);
        store.try_enqueue(r_over).unwrap();
        store.sync_flush().unwrap();

        // Write an oversized raw line (> 8 MiB) directly to the chat's JSONL journal
        let journal_path = core.sessions.run_journal().path_for(chat_id);
        if let Some(parent) = journal_path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&journal_path)
            .unwrap();

        use std::io::Write;
        file.write_all(&vec![b'x'; 9 * 1024 * 1024]).unwrap();
        file.write_all(b"\n").unwrap();
        drop(file);

        let r_over = rpc
            .handle(
                methods::REVEAL_TRAJECTORY_RAW,
                serde_json::json!({
                    "chatId": chat_id,
                    "sourceSeq": 1,
                    "field": "payload",
                }),
            )
            .await
            .unwrap();
        if let RpcReply::Value(val) = r_over {
            let res: TrajectoryRawRevealResult = serde_json::from_value(val).unwrap();
            match res {
                TrajectoryRawRevealResult::Unavailable { reason, .. } => {
                    assert_eq!(reason, TrajectoryUnavailableReason::SourceOversized);
                }
                other => panic!("expected Unavailable SourceOversized, got {:?}", other),
            }
        }
    }

    #[tokio::test]
    async fn test_trajectory_reveal_async_responsiveness() {
        let temp = tempfile::TempDir::new().unwrap();
        let (core, rpc) = setup_test_engine(&temp).await;
        let local_device = core.doc_host.device_id().to_string();
        let chat_id = "chat_responsive";

        core.workspace
            .create_chat(chat_id, None, Some(&local_device), None, None)
            .unwrap();

        let journal = rpc.run_journal().expect("run journal");
        let store = rpc.trajectory_store().expect("trajectory store");

        let mut recs = Vec::new();
        for i in 1..=50 {
            journal
                .append(
                    chat_id,
                    &zeron_proto::AgentEvent::UserMessage {
                        text: format!("Message index {i}"),
                    },
                )
                .unwrap();
            recs.push(sample_record_with_payload_raw_ref(
                chat_id, "run1", i, 0, None, None,
            ));
        }
        store.try_enqueue_batch(recs).unwrap();
        store.sync_flush().unwrap();

        // Run 20 concurrent raw reveal requests across multiple tokio tasks
        let mut handles = Vec::new();
        for i in 1..=20 {
            let rpc_clone = rpc.clone();
            let c_id = chat_id.to_string();
            handles.push(tokio::spawn(async move {
                let start = std::time::Instant::now();
                let reply = rpc_clone
                    .handle(
                        methods::REVEAL_TRAJECTORY_RAW,
                        serde_json::json!({
                            "chatId": c_id,
                            "sourceSeq": i,
                            "field": "payload",
                        }),
                    )
                    .await
                    .unwrap();
                let duration = start.elapsed();
                (reply, duration)
            }));
        }

        for h in handles {
            let (reply, duration) = h.await.unwrap();
            assert!(duration < Duration::from_secs(2));
            if let RpcReply::Value(val) = reply {
                let res: TrajectoryRawRevealResult = serde_json::from_value(val).unwrap();
                assert!(matches!(res, TrajectoryRawRevealResult::Available { .. }));
            }
        }
    }

    #[tokio::test]
    async fn test_trajectory_rpc_end_to_end_memory_client() {
        let temp = tempfile::TempDir::new().unwrap();
        let (core, rpc) = setup_test_engine(&temp).await;
        let local_device = core.doc_host.device_id().to_string();
        let chat_id = "chat_e2e_rpc";

        core.workspace
            .create_chat(chat_id, None, Some(&local_device), None, None)
            .unwrap();

        let journal = rpc.run_journal().expect("run journal");
        let store = rpc.trajectory_store().expect("trajectory store");

        let seq = journal
            .append(
                chat_id,
                &zeron_proto::AgentEvent::UserMessage {
                    text: "End-to-end memory client test payload".into(),
                },
            )
            .unwrap();

        let r = sample_record_with_payload_raw_ref(chat_id, "run1", seq, 0, None, None);
        store.try_enqueue(r).unwrap();
        store.sync_flush().unwrap();

        let client = zeron_rpc::memory_client(rpc);

        let reveal_val = client
            .call(
                methods::REVEAL_TRAJECTORY_RAW,
                serde_json::json!({
                    "chatId": chat_id,
                    "sourceSeq": seq,
                    "field": "payload",
                }),
            )
            .await
            .expect("reveal call succeeded");

        let reveal_res: TrajectoryRawRevealResult = serde_json::from_value(reveal_val).unwrap();
        match reveal_res {
            TrajectoryRawRevealResult::Available { text, field } => {
                assert_eq!(field, TrajectoryRawField::Payload);
                assert_eq!(text, "End-to-end memory client test payload");
            }
            other => panic!("expected Available, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_trajectory_reveal_fabricated_reference_rejected() {
        let temp = tempfile::TempDir::new().unwrap();
        let (core, rpc) = setup_test_engine(&temp).await;
        let local_device = core.doc_host.device_id().to_string();
        let chat_id = "chat_fabricated_ref";

        core.workspace
            .create_chat(chat_id, None, Some(&local_device), None, None)
            .unwrap();

        // Write directly to Run Journal only (NOT in TrajectoryStore, no prior watch)
        let journal = rpc.run_journal().expect("run journal");
        let seq = journal
            .append(
                chat_id,
                &zeron_proto::AgentEvent::UserMessage {
                    text: "Secret text in journal but not in trajectory store".into(),
                },
            )
            .unwrap();

        // Requesting raw reveal with fabricated coordinate must return NotFound (unattached)
        // and must NOT self-authorize by triggering legacy import
        let reply = rpc
            .handle(
                methods::REVEAL_TRAJECTORY_RAW,
                serde_json::json!({
                    "chatId": chat_id,
                    "sourceSeq": seq,
                    "field": "payload",
                }),
            )
            .await
            .unwrap();

        if let RpcReply::Value(val) = reply {
            let res: TrajectoryRawRevealResult = serde_json::from_value(val).unwrap();
            match res {
                TrajectoryRawRevealResult::Unavailable {
                    reason, message, ..
                } => {
                    assert_eq!(reason, TrajectoryUnavailableReason::NotFound);
                    assert!(message.unwrap().contains("not attached"));
                }
                other => panic!("expected Unavailable NotFound, got {:?}", other),
            }
        }
    }

    #[tokio::test]
    async fn test_trajectory_reveal_unsupported_version_rejected() {
        let temp = tempfile::TempDir::new().unwrap();
        let (core, rpc) = setup_test_engine(&temp).await;
        let local_device = core.doc_host.device_id().to_string();
        let chat_id = "chat_version_reject";

        core.workspace
            .create_chat(chat_id, None, Some(&local_device), None, None)
            .unwrap();

        let reply = rpc
            .handle(
                methods::REVEAL_TRAJECTORY_RAW,
                serde_json::json!({
                    "chatId": chat_id,
                    "sourceSeq": 1,
                    "field": "payload",
                    "sourceVersion": 99,
                }),
            )
            .await
            .unwrap();

        if let RpcReply::Value(val) = reply {
            let res: TrajectoryRawRevealResult = serde_json::from_value(val).unwrap();
            match res {
                TrajectoryRawRevealResult::Unavailable { reason, .. } => {
                    assert_eq!(
                        reason,
                        TrajectoryUnavailableReason::UnsupportedSourceVersion
                    );
                }
                other => panic!(
                    "expected Unavailable UnsupportedSourceVersion, got {:?}",
                    other
                ),
            }
        }
    }

    #[tokio::test]
    async fn test_trajectory_reveal_missing_tool_result_and_done_fields_typed_unavailable() {
        let temp = tempfile::TempDir::new().unwrap();
        let (core, rpc) = setup_test_engine(&temp).await;
        let local_device = core.doc_host.device_id().to_string();
        let chat_id = "chat_missing_result_fields";

        core.workspace
            .create_chat(chat_id, None, Some(&local_device), None, None)
            .unwrap();

        let journal = rpc.run_journal().expect("run journal");
        let store = rpc.trajectory_store().expect("trajectory store");

        // 1. ToolResult with output: None, diff: None
        let seq1 = journal
            .append(
                chat_id,
                &zeron_proto::AgentEvent::ToolResult {
                    id: "tool-no-out".into(),
                    output: None,
                    diff: None,
                    is_error: false,
                    execution: None,
                },
            )
            .unwrap();

        let r1 = sample_record_with_result_raw_ref(
            chat_id,
            "run1",
            seq1,
            0,
            None,
            Some("tool-no-out".into()),
        );
        store.try_enqueue(r1).unwrap();

        // 2. Done with result: None, error: None
        let seq2 = journal
            .append(
                chat_id,
                &zeron_proto::AgentEvent::Done {
                    status: zeron_proto::DoneStatus::Completed,
                    result: None,
                    error: None,
                    session_id: None,
                },
            )
            .unwrap();

        let r2 = sample_record_with_result_raw_ref(chat_id, "run1", seq2, 0, None, None);
        store.try_enqueue(r2).unwrap();
        store.sync_flush().unwrap();

        // 1. Reveal missing ToolResult result -> Unavailable(NotFound), never empty string
        let reply1 = rpc
            .handle(
                methods::REVEAL_TRAJECTORY_RAW,
                serde_json::json!({
                    "chatId": chat_id,
                    "sourceSeq": seq1,
                    "callId": "tool-no-out",
                    "field": "result",
                }),
            )
            .await
            .unwrap();

        if let RpcReply::Value(val) = reply1 {
            let res: TrajectoryRawRevealResult = serde_json::from_value(val).unwrap();
            match res {
                TrajectoryRawRevealResult::Unavailable {
                    reason, message, ..
                } => {
                    assert_eq!(reason, TrajectoryUnavailableReason::NotFound);
                    assert!(message.unwrap().contains("no raw output or diff"));
                }
                other => panic!("expected Unavailable NotFound, got {:?}", other),
            }
        }

        // 2. Reveal missing Done result -> Unavailable(NotFound), never Debug format
        let reply2 = rpc
            .handle(
                methods::REVEAL_TRAJECTORY_RAW,
                serde_json::json!({
                    "chatId": chat_id,
                    "sourceSeq": seq2,
                    "field": "result",
                }),
            )
            .await
            .unwrap();

        if let RpcReply::Value(val) = reply2 {
            let res: TrajectoryRawRevealResult = serde_json::from_value(val).unwrap();
            match res {
                TrajectoryRawRevealResult::Unavailable {
                    reason, message, ..
                } => {
                    assert_eq!(reason, TrajectoryUnavailableReason::NotFound);
                    assert!(message.unwrap().contains("no raw result or error text"));
                }
                other => panic!("expected Unavailable NotFound, got {:?}", other),
            }
        }
    }

    #[tokio::test]
    async fn test_trajectory_reveal_coalesced_streaming_deltas() {
        let temp = tempfile::TempDir::new().unwrap();
        let (core, rpc) = setup_test_engine(&temp).await;
        let local_device = core.doc_host.device_id().to_string();
        let chat_id = "chat_coalesced_deltas";

        core.workspace
            .create_chat(chat_id, None, Some(&local_device), None, None)
            .unwrap();

        let journal = rpc.run_journal().expect("run journal");
        let store = rpc.trajectory_store().expect("trajectory store");

        // Streaming text deltas appended across multiple journal lines
        let seq_start = journal
            .append(
                chat_id,
                &zeron_proto::AgentEvent::TextDelta {
                    text: "The quick brown fox ".into(),
                },
            )
            .unwrap();
        journal
            .append(
                chat_id,
                &zeron_proto::AgentEvent::TextDelta {
                    text: "jumps over ".into(),
                },
            )
            .unwrap();
        journal
            .append(
                chat_id,
                &zeron_proto::AgentEvent::TextDelta {
                    text: "the lazy dog.".into(),
                },
            )
            .unwrap();

        // Terminal event closes streaming
        journal
            .append(
                chat_id,
                &zeron_proto::AgentEvent::Done {
                    status: zeron_proto::DoneStatus::Completed,
                    result: Some("done".into()),
                    error: None,
                    session_id: None,
                },
            )
            .unwrap();

        // Trajectory record points at start_seq
        let r = sample_record_with_payload_raw_ref(chat_id, "run1", seq_start, 0, None, None);
        store.try_enqueue(r).unwrap();
        store.sync_flush().unwrap();

        let reply = rpc
            .handle(
                methods::REVEAL_TRAJECTORY_RAW,
                serde_json::json!({
                    "chatId": chat_id,
                    "sourceSeq": seq_start,
                    "field": "payload",
                }),
            )
            .await
            .unwrap();

        if let RpcReply::Value(val) = reply {
            let res: TrajectoryRawRevealResult = serde_json::from_value(val).unwrap();
            match res {
                TrajectoryRawRevealResult::Available { text, field } => {
                    assert_eq!(field, TrajectoryRawField::Payload);
                    assert_eq!(text, "The quick brown fox jumps over the lazy dog.");
                }
                other => panic!("expected Available with assembled deltas, got {:?}", other),
            }
        }
    }

    #[tokio::test]
    async fn test_trajectory_store_multi_chat_degraded_notification() {
        let temp = tempfile::TempDir::new().unwrap();
        let store = Arc::new(TrajectoryStore::open(temp.path()).unwrap());

        let mut events_rx = store.subscribe_events();

        // Create records for chat-A and chat-B
        let r_a1 = sample_record("chat_a", "run_a", 1, 0);
        let r_b1 = sample_record("chat_b", "run_b", 1, 0);

        // Corrupt table schema to force durable write failure in writer thread
        let db_path = temp.path().join("trajectory.sqlite3");
        let conn = rusqlite::Connection::open(&db_path).unwrap();
        conn.execute("DROP TABLE trajectory_records", []).unwrap();
        drop(conn);

        store.try_enqueue_batch(vec![r_a1, r_b1]).unwrap();

        // Must receive DegradedRecorded for both chat_a and chat_b
        let mut degraded_chats = std::collections::HashSet::new();
        let timeout = std::time::Instant::now();
        while degraded_chats.len() < 2 && timeout.elapsed() < std::time::Duration::from_secs(3) {
            if let Ok(event) = events_rx.try_recv() {
                if let TrajectoryStoreEvent::DegradedRecorded { chat_id, .. } = event {
                    degraded_chats.insert(chat_id);
                }
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }

        assert!(
            degraded_chats.contains("chat_a"),
            "chat_a must receive degraded notification"
        );
        assert!(
            degraded_chats.contains("chat_b"),
            "chat_b must receive degraded notification"
        );
    }

    #[tokio::test]
    async fn test_trajectory_reveal_missing_or_corrupt_journal() {
        let temp = tempfile::TempDir::new().unwrap();
        let (core, rpc) = setup_test_engine(&temp).await;
        let local_device = core.doc_host.device_id().to_string();
        let chat_id = "chat_missing_corrupt";

        core.workspace
            .create_chat(chat_id, None, Some(&local_device), None, None)
            .unwrap();

        // 1. Attached record in store, but journal file does not contain that seq
        let store = rpc.trajectory_store().expect("trajectory store");
        let r = sample_record_with_payload_raw_ref(chat_id, "run1", 9999, 0, None, None);
        store.try_enqueue(r).unwrap();
        store.sync_flush().unwrap();

        let r_missing = rpc
            .handle(
                methods::REVEAL_TRAJECTORY_RAW,
                serde_json::json!({
                    "chatId": chat_id,
                    "sourceSeq": 9999,
                    "field": "payload",
                }),
            )
            .await
            .unwrap();
        if let RpcReply::Value(val) = r_missing {
            let res: TrajectoryRawRevealResult = serde_json::from_value(val).unwrap();
            match res {
                TrajectoryRawRevealResult::Unavailable { reason, .. } => {
                    assert_eq!(reason, TrajectoryUnavailableReason::NotFound);
                }
                other => panic!("expected Unavailable NotFound, got {:?}", other),
            }
        }
    }
}
