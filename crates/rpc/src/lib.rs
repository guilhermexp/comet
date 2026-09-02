//! zeron-rpc — the typed control plane (UiRpc / ControlRpc) over WebSocket + in-memory
//! transports, plus the device-room relay transport ({s,k,to,from} frames — [`device_room`]).
//!
//! Framing: ndjson envelopes, one JSON object per WebSocket text message (or per line on
//! byte transports), matching the shape of zeron's Effect RPC without the Effect runtime:
//!
//! - client → server: `{id, method, params}` to invoke, `{id, cancel: true}` to stop a stream;
//! - server → client: `{id, ok}` / `{id, err}` for unary calls,
//!   `{id, item}`* then `{id, done: true}` (or `{id, err}`) for streams.
//!
//! The server dispatches into an [`RpcService`]; the [`RpcClient`] offers `call` and
//! `subscribe`. Both ends run over any pair of string channels, so the in-memory transport
//! ([`memory_client`]) exercises the exact same code path as the WebSocket one.

use std::sync::Arc;

use async_trait::async_trait;
use futures::stream::BoxStream;
use serde::{Deserialize, Serialize};
pub use zeron_proto::trajectory::{
    TrajectoryDegradedInterval, TrajectoryRawField, TrajectoryRawRef, TrajectoryRecord,
    TrajectoryRecordId,
};
mod client;
pub mod device_room;
mod server;

pub use client::{RpcClient, RpcSubscription, connect_ws};
pub use device_room::{
    DeviceFrameHeader, DeviceLink, HostRelay, HostRelayConfig, LinkCache, LinkCacheConfig,
    NudgeHandler, PeerLiveness, PeerLivenessProbe, StaticToken, TokenSource, decode_device_frame,
    device_room_ws_url, encode_device_frame,
};
pub use server::{serve_connection, serve_ws_listener};

/// RPC method names — single source of truth for both ends.
/// Full surface: docs/research/feature-inventory.md §2.
pub mod methods {
    pub const LIST_HARNESSES: &str = "ListHarnesses";
    /// Flip a harness's enablement on the target device (Settings → Agents);
    /// replies with the device's fresh `ListHarnesses` catalog.
    pub const SET_HARNESS_ENABLED: &str = "SetHarnessEnabled";
    pub const LIST_MODELS: &str = "ListModels";
    pub const LIST_COMMANDS: &str = "ListCommands";
    pub const QUEUE_COMMAND: &str = "QueueCommand";
    /// App-owned durable delivery of a Worker lifecycle event to its existing
    /// parent chat. Uses a deterministic command id and fsync-equivalent store
    /// persistence before acknowledging the RPC.
    pub const QUEUE_WORKER_NOTIFICATION: &str = "QueueWorkerNotification";
    /// Peer-to-peer delivery fallback: the SENDER's engine forwards a queued
    /// command entry (client-minted id and all) straight over the device-room
    /// link when its chat2 rows can't reach the edge but the host's peer link
    /// is alive. The host claims the id in its processed ledger before
    /// executing, so the doc row arriving later dedupes to a no-op —
    /// exactly-once by construction. Params `{chatId, entry}`.
    pub const RELAY_COMMAND: &str = "RelayCommand";
    /// User-driven delivery retry for a chat with unadopted queued sends:
    /// fresh chat2 socket, host nudge, drain pass, and a new delivery escort
    /// per pending command. Params `{chatId}`; IPC-only.
    pub const RETRY_DELIVERY: &str = "RetryDelivery";
    pub const WATCH_DOC_MESSAGES: &str = "WatchDocMessages";
    /// Nudge every open room client to verify liveness NOW (window focus,
    /// app foregrounded). No params; IPC-only. Each room ignores the hint
    /// unless it has been broadcast-quiet ≥30s, so this is cheap to spam.
    pub const PROBE_SYNC: &str = "ProbeSync";
    /// Live sync introspection (`zeron sync` / debug surfaces): per-room
    /// connection state, last pushed-frame/ack ages, rejoin/probe/resync
    /// counters for the workspace room and every open chat doc. No params;
    /// IPC-only.
    pub const SYNC_STATUS: &str = "SyncStatus";
    /// Pushed edge-connectivity posture (`zeron_proto::Connectivity`):
    /// current value first, then every change — the connection pill /
    /// composer-honesty / queued-badge feed. No params; IPC-only.
    pub const WATCH_CONNECTIVITY: &str = "WatchConnectivity";
    /// In-flight queued-attachment transfers (`zeron_proto::TransferProgress`
    /// list): current set first, then a fresh snapshot per landed chunk —
    /// the sending thumbnail's percent-ring feed. No params; IPC-only.
    pub const WATCH_TRANSFERS: &str = "WatchTransfers";
    pub const WATCH_CHATS: &str = "WatchChats";
    pub const WATCH_DEVICES: &str = "WatchDevices";
    pub const WATCH_SESSIONS: &str = "WatchSessions";
    /// Spaces registry (device+folder pairs) from the workspace doc.
    pub const WATCH_SPACES: &str = "WatchSpaces";
    /// Local-only OMP Live Voice lifecycle. Media remains inside OMP; Comet
    /// exposes only control, state, and transcript metadata.
    pub const PROBE_LIVE_VOICE: &str = "ProbeLiveVoice";
    pub const START_LIVE_VOICE: &str = "StartLiveVoice";
    pub const SET_LIVE_VOICE_MUTED: &str = "SetLiveVoiceMuted";
    pub const STOP_LIVE_VOICE: &str = "StopLiveVoice";
    pub const WATCH_LIVE_VOICE: &str = "WatchLiveVoice";
    /// Entity mutations against the workspace doc (feature-inventory §2 DataRpc).
    /// Params are tagged `{op: createChat|createSpace|renameSpace|deleteSpace|
    /// renameChat|setChatArchived|deleteChat|renameDevice|markChatSeen, …}`.
    pub const MUTATE: &str = "Mutate";
    /// This engine's identity → `{deviceId}` (IPC-only; never relay-forwarded —
    /// the answer is about whichever engine you are directly connected to).
    pub const LOCAL_DEVICE: &str = "LocalDevice";
    /// This engine runtime's fixed device and workspace identity.
    pub const ENGINE_INFO: &str = "EngineInfo";
    /// Readiness barrier for the engine runtime. The call completes once stores
    /// and journals are assembled, or fails with the assembly error.
    pub const ENGINE_READY: &str = "EngineReady";
    /// Ask a headless IPC owner to drain its runtime and exit successfully.
    /// Headed IPC owners do not implement this method: closing another app's
    /// engine behind its windows would leave that process unusable.
    pub const STOP_ENGINE: &str = "StopEngine";
    pub const AUTH_STATUS: &str = "AuthStatus";
    // AuthRpc mutations (feature-inventory §2 AuthRpc; IPC-only).
    pub const SIGN_IN: &str = "SignIn";
    pub const SIGN_IN_HEADLESS: &str = "SignInHeadless";
    pub const COMPLETE_SIGN_IN: &str = "CompleteSignIn";
    pub const SIGN_OUT: &str = "SignOut";
    pub const LIST_ORGS: &str = "ListOrgs";
    pub const CREATE_ORG: &str = "CreateOrg";
    pub const SELECT_ORG: &str = "SelectOrg";
    /// One-time local→synced profile import: what's importable (unary).
    pub const LOCAL_IMPORT_STATUS: &str = "LocalImportStatus";
    /// One-time local→synced profile import: run it (stream of progress items).
    pub const IMPORT_LOCAL_WORKSPACE: &str = "ImportLocalWorkspace";
    // Repos / worktrees / folders (ControlRpc, relay-forwardable).
    pub const LIST_REPOS: &str = "ListRepos";
    pub const ADD_REPO: &str = "AddRepo";
    pub const CLONE_REPO: &str = "CloneRepo";
    pub const CREATE_REPO: &str = "CreateRepo";
    pub const LIST_BRANCHES: &str = "ListBranches";
    pub const LIST_REFS: &str = "ListRefs";
    pub const LIST_GIT_HISTORY: &str = "ListGitHistory";
    /// Update remote-tracking refs without changing HEAD, the index, or files.
    pub const FETCH_ALL: &str = "FetchAll";
    pub const SWITCH_REF: &str = "SwitchRef";
    pub const LIST_FOLDERS: &str = "ListFolders";
    /// The device's browse roots: home plus mounted drives/volumes.
    pub const LIST_DRIVES: &str = "ListDrives";
    /// Fuzzy relative-path search rooted in a known chat or space checkout.
    pub const SEARCH_FILES: &str = "SearchFiles";
    pub const CREATE_WORKTREE: &str = "CreateWorktree";
    pub const DELETE_WORKTREE: &str = "DeleteWorktree";
    // Terminals (ControlRpc, relay-forwardable; SubscribeTerminal streams).
    pub const OPEN_TERMINAL: &str = "OpenTerminal";
    pub const SUBSCRIBE_TERMINAL: &str = "SubscribeTerminal";
    pub const WRITE_TERMINAL: &str = "WriteTerminal";
    pub const RESIZE_TERMINAL: &str = "ResizeTerminal";
    pub const CLOSE_TERMINAL: &str = "CloseTerminal";
    /// Checkout-diff stream for the target device's chats (DataRpc,
    /// relay-forwardable — diffs are produced where the checkout lives).
    pub const WATCH_CHECKOUT_DIFFS: &str = "WatchCheckoutDiffs";
    /// Current pull request for one checkout, resolved on the checkout's host device.
    pub const WATCH_CHECKOUT_CHANGE_REQUEST: &str = "WatchCheckoutChangeRequest";
    pub const GET_CHECKOUT_DIFF: &str = "GetCheckoutDiff";
    pub const GET_CHECKOUT_FILE_DIFF_TEXT: &str = "GetCheckoutFileDiffText";
    // Agent accounts (ControlRpc, relay-forwardable — CLI logins are per-device).
    pub const LIST_AGENT_ACCOUNTS: &str = "ListAgentAccounts";
    pub const ACTIVATE_AGENT_ACCOUNT: &str = "ActivateAgentAccount";
    pub const FORGET_AGENT_ACCOUNT: &str = "ForgetAgentAccount";
    pub const START_AGENT_LOGIN: &str = "StartAgentLogin";
    pub const COMPLETE_AGENT_LOGIN: &str = "CompleteAgentLogin";
    pub const POLL_AGENT_LOGIN: &str = "PollAgentLogin";
    pub const CANCEL_AGENT_LOGIN: &str = "CancelAgentLogin";
    // Uploads / attachments (ControlRpc, relay-forwardable — target the chat's host device).
    pub const UPLOAD_CHUNK: &str = "UploadChunk";
    pub const UPLOAD_COMMIT: &str = "UploadCommit";
    pub const READ_ATTACHMENT_CHUNK: &str = "ReadAttachmentChunk";
    /// Lazy full-tool-output fetch from the R2 sidecar by doc-resident ref
    /// (chat2-sync A3). Edge-direct from any device — never relay-forwarded.
    pub const FETCH_TOOL_BLOB: &str = "FetchToolBlob";
    /// Fetch sanitized historical Write/Edit input from the chat host's local
    /// run journal. Relay-forwardable via `targetDeviceId`.
    pub const FETCH_TOOL_INPUT: &str = "FetchToolInput";
    // Updates (ControlRpc, relay-forwardable — a device reports/applies its own
    // binary's update). Stream: current UpdateStatus, then every change.
    pub const UPDATE_STATUS: &str = "UpdateStatus";
    /// Download + apply the newest release on the target device (symlink-managed
    /// installs; the service restart is scheduled after the reply flushes).
    pub const APPLY_UPDATE: &str = "ApplyUpdate";
    // Trajectory (device-local read model & explicit raw reveal; strictly IPC-only, never relay-forwarded).
    /// Stream of bounded Trajectory snapshot frames and ordered live deltas.
    pub const WATCH_TRAJECTORY: &str = "WatchTrajectory";
    /// Device-local unary lookup to reveal one raw field from Run Journal.
    pub const REVEAL_TRAJECTORY_RAW: &str = "RevealTrajectoryRaw";

    /// Whether an RPC method is strictly device-local (IPC local only;
    /// rejected at relay ingress on peer virtual connections and non-forwardable).
    pub fn is_local_only(method: &str) -> bool {
        matches!(
            method,
            WATCH_TRAJECTORY
                | REVEAL_TRAJECTORY_RAW
                | PROBE_LIVE_VOICE
                | START_LIVE_VOICE
                | SET_LIVE_VOICE_MUTED
                | STOP_LIVE_VOICE
                | WATCH_LIVE_VOICE
                | LOCAL_DEVICE
                | STOP_ENGINE
        )
    }
}

#[derive(Debug, thiserror::Error)]
pub enum RpcError {
    #[error("unknown method: {0}")]
    UnknownMethod(String),
    #[error("bad params: {0}")]
    BadParams(String),
    #[error("{0}")]
    Failed(String),
    #[error("transport: {0}")]
    Transport(String),
    #[error("connection closed")]
    Closed,
}

/// A client-originated frame.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientFrame {
    pub id: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub method: Option<String>,
    #[serde(default, skip_serializing_if = "serde_json::Value::is_null")]
    pub params: serde_json::Value,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub cancel: bool,
}

/// A server-originated frame. Exactly one of `ok` / `err` / `item` / `done` is meaningful.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ServerFrame {
    pub id: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ok: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub err: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub item: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub done: bool,
}

/// What a service returns for one invocation.
pub enum RpcReply {
    /// Unary response — sent as `{id, ok}`.
    Value(serde_json::Value),
    /// Stream — each item sent as `{id, item}`, then `{id, done: true}` when it ends.
    Stream(BoxStream<'static, serde_json::Value>),
}

impl RpcReply {
    /// Serialize a value into a unary reply.
    pub fn value<T: Serialize>(value: &T) -> Result<Self, RpcError> {
        serde_json::to_value(value)
            .map(RpcReply::Value)
            .map_err(|e| RpcError::Failed(format!("serialize response: {e}")))
    }
}

/// Server-side dispatch: one implementation serves every transport.
#[async_trait]
pub trait RpcService: Send + Sync + 'static {
    async fn handle(&self, method: &str, params: serde_json::Value) -> Result<RpcReply, RpcError>;
}

/// Deserialize typed params out of the envelope's `params` value.
pub fn parse_params<T: serde::de::DeserializeOwned>(
    params: serde_json::Value,
) -> Result<T, RpcError> {
    serde_json::from_value(params).map_err(|e| RpcError::BadParams(e.to_string()))
}

/// Spawn an in-memory server for `service` and return a connected client.
/// Same envelopes, same dispatch loop as the WebSocket path — the in-process UI
/// transport (ARCHITECTURE §1 "zero serialization shortcuts").
pub fn memory_client(service: Arc<dyn RpcService>) -> RpcClient {
    let (client_out, server_in) = tokio::sync::mpsc::channel::<String>(256);
    let (server_out, client_in) = tokio::sync::mpsc::channel::<String>(256);
    tokio::spawn(serve_connection(service, server_out, server_in));
    RpcClient::new(client_out, client_in)
}

// ---------------------------------------------------------------------------
// Trajectory Wire Types
// ---------------------------------------------------------------------------

/// Full `(source_seq, sub_seq)` position plus store revision for Trajectory watch and paging.
///
/// Critical invariant: `source_seq` alone is insufficient because legacy Interrupted records
/// can share `source_seq` with a prefix record at `sub_seq = u32::MAX`.
///
/// `rev` is the store's monotonic per-commit revision observed by the holder of this cursor.
/// Position alone cannot express an in-place replacement (a partial record finalized under the
/// same `(source_seq, sub_seq)`), so resume asks for "everything past this position OR written
/// after this revision". `rev == 0` means "no revision knowledge" and disables that clause.
/// Ordering stays position-first: `rev` only breaks ties.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrajectoryCursor {
    pub source_seq: u64,
    pub sub_seq: u32,
    #[serde(default)]
    pub rev: u64,
}

impl TrajectoryCursor {
    pub const fn new(source_seq: u64, sub_seq: u32) -> Self {
        Self {
            source_seq,
            sub_seq,
            rev: 0,
        }
    }

    /// Same position, carrying the store revision the holder has already observed.
    pub const fn with_rev(self, rev: u64) -> Self {
        Self { rev, ..self }
    }
}

impl From<(u64, u32)> for TrajectoryCursor {
    fn from((source_seq, sub_seq): (u64, u32)) -> Self {
        Self::new(source_seq, sub_seq)
    }
}

impl From<&TrajectoryRecord> for TrajectoryCursor {
    fn from(r: &TrajectoryRecord) -> Self {
        Self::new(r.source_seq, r.sub_seq)
    }
}

impl From<TrajectoryRecordId> for TrajectoryCursor {
    fn from(id: TrajectoryRecordId) -> Self {
        Self::new(id.source_seq, id.sub_seq)
    }
}

impl From<&TrajectoryRecordId> for TrajectoryCursor {
    fn from(id: &TrajectoryRecordId) -> Self {
        Self::new(id.source_seq, id.sub_seq)
    }
}

/// Request parameters for `WatchTrajectory`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WatchTrajectoryParams {
    pub chat_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub after_cursor: Option<TrajectoryCursor>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<usize>,
}

impl WatchTrajectoryParams {
    pub fn new(chat_id: impl Into<String>) -> Self {
        Self {
            chat_id: chat_id.into(),
            after_cursor: None,
            limit: None,
        }
    }

    pub fn with_cursor(mut self, cursor: impl Into<TrajectoryCursor>) -> Self {
        self.after_cursor = Some(cursor.into());
        self
    }

    pub fn with_limit(mut self, limit: usize) -> Self {
        self.limit = Some(limit);
        self
    }
}

/// Reason why a Trajectory watch stream reached a terminal state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TrajectoryTerminalReason {
    ChatDeleted,
    StoreUnavailable,
}

/// Stream items emitted by `WatchTrajectory`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum TrajectoryWatchItem {
    /// Initial snapshot of historical records up to `watermark`.
    /// May be delivered in multiple bounded frames if `has_more` is true.
    Snapshot {
        records: Vec<TrajectoryRecord>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        watermark: Option<TrajectoryCursor>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        degraded: Vec<TrajectoryDegradedInterval>,
        #[serde(default)]
        has_more: bool,
    },
    /// Live deltas emitted strictly after the snapshot watermark or previous delta watermark.
    Deltas {
        records: Vec<TrajectoryRecord>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        watermark: Option<TrajectoryCursor>,
    },
    /// Notification of a new degraded interval during active watch.
    Degraded {
        intervals: Vec<TrajectoryDegradedInterval>,
    },
    /// Explicit instruction to client to clear local state and resubscribe / resnapshot
    /// when an unrecoverable gap is detected.
    ResyncRequired { reason: String },
    /// Stream closure due to terminal lifecycle events (e.g. Chat deleted).
    Terminal {
        reason: TrajectoryTerminalReason,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        message: Option<String>,
    },
}

pub const DEFAULT_TRAJECTORY_PAGE_SIZE: usize = 250;
pub const MAX_TRAJECTORY_PAGE_SIZE: usize = 1000;
pub const MIN_TRAJECTORY_PAGE_SIZE: usize = 1;

pub const CURRENT_RAW_SOURCE_VERSION: u32 = 1;

fn default_raw_source_version() -> u32 {
    CURRENT_RAW_SOURCE_VERSION
}

/// Request parameters for `RevealTrajectoryRaw`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RevealTrajectoryRawParams {
    pub chat_id: String,
    pub source_seq: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_tool_use_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub call_id: Option<String>,
    pub field: TrajectoryRawField,
    #[serde(default = "default_raw_source_version")]
    pub source_version: u32,
}

impl RevealTrajectoryRawParams {
    pub fn new(
        chat_id: impl Into<String>,
        source_seq: u64,
        parent_tool_use_id: Option<String>,
        call_id: Option<String>,
        field: TrajectoryRawField,
    ) -> Self {
        Self {
            chat_id: chat_id.into(),
            source_seq,
            parent_tool_use_id,
            call_id,
            field,
            source_version: CURRENT_RAW_SOURCE_VERSION,
        }
    }

    pub fn with_version(mut self, version: u32) -> Self {
        self.source_version = version;
        self
    }

    pub fn to_raw_ref(&self) -> TrajectoryRawRef {
        TrajectoryRawRef {
            chat_id: self.chat_id.clone(),
            source_seq: self.source_seq,
            parent_tool_use_id: self.parent_tool_use_id.clone(),
            call_id: self.call_id.clone(),
            field: self.field,
            source_version: self.source_version,
        }
    }
}

impl From<TrajectoryRawRef> for RevealTrajectoryRawParams {
    fn from(r: TrajectoryRawRef) -> Self {
        Self {
            chat_id: r.chat_id,
            source_seq: r.source_seq,
            parent_tool_use_id: r.parent_tool_use_id,
            call_id: r.call_id,
            field: r.field,
            source_version: r.source_version,
        }
    }
}

impl From<&TrajectoryRawRef> for RevealTrajectoryRawParams {
    fn from(r: &TrajectoryRawRef) -> Self {
        Self {
            chat_id: r.chat_id.clone(),
            source_seq: r.source_seq,
            parent_tool_use_id: r.parent_tool_use_id.clone(),
            call_id: r.call_id.clone(),
            field: r.field,
            source_version: r.source_version,
        }
    }
}

impl From<RevealTrajectoryRawParams> for TrajectoryRawRef {
    fn from(p: RevealTrajectoryRawParams) -> Self {
        p.to_raw_ref()
    }
}

impl From<&RevealTrajectoryRawParams> for TrajectoryRawRef {
    fn from(p: &RevealTrajectoryRawParams) -> Self {
        p.to_raw_ref()
    }
}

/// Reason why a raw reveal lookup resulted in unavailable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TrajectoryUnavailableReason {
    NotFound,
    ForeignDevice,
    ChatDeleted,
    SourceCorrupt,
    SourceOversized,
    MismatchedReference,
    UnsupportedSourceVersion,
    StoreUnavailable,
}

/// Typed result returned by `RevealTrajectoryRaw`.
///
/// Invariant: Raw text is ephemeral response data and must NEVER be persisted or synced.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "camelCase")]
pub enum TrajectoryRawRevealResult {
    Available {
        field: TrajectoryRawField,
        text: String,
    },
    Unavailable {
        field: TrajectoryRawField,
        reason: TrajectoryUnavailableReason,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        message: Option<String>,
    },
}

impl TrajectoryRawRevealResult {
    pub fn available(field: TrajectoryRawField, text: impl Into<String>) -> Self {
        Self::Available {
            field,
            text: text.into(),
        }
    }

    pub fn unavailable(
        field: TrajectoryRawField,
        reason: TrajectoryUnavailableReason,
        message: Option<String>,
    ) -> Self {
        Self::Unavailable {
            field,
            reason,
            message,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::StreamExt;
    use std::sync::Mutex;

    struct TestService;

    struct CancelAwareService {
        dropped: Mutex<Option<tokio::sync::oneshot::Sender<()>>>,
    }

    struct DropSignal(Option<tokio::sync::oneshot::Sender<()>>);

    impl Drop for DropSignal {
        fn drop(&mut self) {
            if let Some(dropped) = self.0.take() {
                let _ = dropped.send(());
            }
        }
    }

    #[async_trait]
    impl RpcService for CancelAwareService {
        async fn handle(
            &self,
            method: &str,
            _params: serde_json::Value,
        ) -> Result<RpcReply, RpcError> {
            if method != methods::WATCH_CHECKOUT_CHANGE_REQUEST
                && method != methods::WATCH_TRAJECTORY
            {
                if method == "Echo" {
                    return Ok(RpcReply::Value(_params));
                }
                return Err(RpcError::UnknownMethod(method.into()));
            }
            let guard = DropSignal(self.dropped.lock().unwrap().take());
            let stream = futures::stream::unfold(guard, |guard| async move {
                let item = std::future::pending::<Option<(serde_json::Value, DropSignal)>>().await;
                drop(guard);
                item
            });
            Ok(RpcReply::Stream(stream.boxed()))
        }
    }

    #[async_trait]
    impl RpcService for TestService {
        async fn handle(
            &self,
            method: &str,
            params: serde_json::Value,
        ) -> Result<RpcReply, RpcError> {
            match method {
                "Echo" => Ok(RpcReply::Value(params)),
                "Count" => {
                    let n = params.get("n").and_then(|v| v.as_u64()).unwrap_or(0);
                    Ok(RpcReply::Stream(
                        futures::stream::iter((0..n).map(|i| serde_json::json!(i))).boxed(),
                    ))
                }
                "Never" => Ok(RpcReply::Stream(futures::stream::pending().boxed())),
                "Boom" => Err(RpcError::Failed("boom".into())),
                other => Err(RpcError::UnknownMethod(other.into())),
            }
        }
    }

    #[tokio::test]
    async fn memory_call_stream_and_error() {
        let client = memory_client(Arc::new(TestService));

        let echoed = client
            .call("Echo", serde_json::json!({"x": 1}))
            .await
            .unwrap();
        assert_eq!(echoed, serde_json::json!({"x": 1}));

        let mut items = client
            .subscribe("Count", serde_json::json!({"n": 3}))
            .await
            .unwrap();
        let mut seen = Vec::new();
        while let Some(v) = items.recv().await {
            seen.push(v);
        }
        assert_eq!(
            seen,
            vec![
                serde_json::json!(0),
                serde_json::json!(1),
                serde_json::json!(2)
            ]
        );

        let err = client
            .call("Boom", serde_json::Value::Null)
            .await
            .unwrap_err();
        assert!(matches!(err, RpcError::Failed(m) if m == "boom"));
    }

    #[tokio::test]
    async fn checked_stream_acknowledges_support_and_preserves_unknown_method() {
        let client = memory_client(Arc::new(TestService));

        let mut items = client
            .subscribe_checked("Count", serde_json::json!({"n": 1}))
            .await
            .unwrap();
        assert_eq!(items.recv().await, Some(serde_json::json!(0)));
        assert_eq!(items.recv().await, None);

        let error = match client
            .subscribe_checked("FutureStream", serde_json::Value::Null)
            .await
        {
            Ok(_) => panic!("old service must reject unknown stream"),
            Err(error) => error,
        };
        assert!(matches!(error, RpcError::UnknownMethod(method) if method == "FutureStream"));
    }

    #[tokio::test]
    async fn dropping_checked_subscription_cancels_pending_server_stream() {
        let (dropped_tx, dropped_rx) = tokio::sync::oneshot::channel();
        let client = memory_client(Arc::new(CancelAwareService {
            dropped: Mutex::new(Some(dropped_tx)),
        }));
        let stream = client
            .subscribe_checked(
                methods::WATCH_CHECKOUT_CHANGE_REQUEST,
                serde_json::Value::Null,
            )
            .await
            .unwrap();

        drop(stream);

        tokio::time::timeout(std::time::Duration::from_secs(1), dropped_rx)
            .await
            .expect("server stream cancelled")
            .expect("drop signal");
    }

    #[tokio::test]
    async fn websocket_round_trip() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        tokio::spawn(serve_ws_listener(listener, Arc::new(TestService)));

        let client = connect_ws(&format!("ws://127.0.0.1:{port}")).await.unwrap();
        let echoed = client
            .call("Echo", serde_json::json!("hello"))
            .await
            .unwrap();
        assert_eq!(echoed, serde_json::json!("hello"));

        let mut items = client
            .subscribe("Count", serde_json::json!({"n": 2}))
            .await
            .unwrap();
        assert_eq!(items.recv().await, Some(serde_json::json!(0)));
        assert_eq!(items.recv().await, Some(serde_json::json!(1)));
        assert_eq!(items.recv().await, None);
    }

    #[tokio::test]
    async fn handshake_with_origin_header_is_rejected() {
        use tokio_tungstenite::tungstenite::client::IntoClientRequest;

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        tokio::spawn(serve_ws_listener(listener, Arc::new(TestService)));

        // A browser page opening ws://127.0.0.1:{port} always sends Origin;
        // the server must refuse the handshake before serving any RPC.
        let mut req = format!("ws://127.0.0.1:{port}")
            .into_client_request()
            .unwrap();
        req.headers_mut()
            .insert("origin", "https://evil.example".parse().unwrap());
        let result = tokio_tungstenite::connect_async(req).await;
        assert!(
            result.is_err(),
            "handshake carrying an Origin header must be rejected"
        );

        // A native viewport (no Origin) still connects and can call RPC — the
        // reject must not be a blanket denial.
        let client = connect_ws(&format!("ws://127.0.0.1:{port}")).await.unwrap();
        let echoed = client.call("Echo", serde_json::json!("ok")).await.unwrap();
        assert_eq!(echoed, serde_json::json!("ok"));
    }

    #[tokio::test]
    async fn dropping_stream_receiver_cancels_server_side() {
        let (dropped_tx, dropped_rx) = tokio::sync::oneshot::channel();
        let service = Arc::new(CancelAwareService {
            dropped: Mutex::new(Some(dropped_tx)),
        });
        let client = memory_client(service);
        let subscription = client
            .subscribe_checked(
                methods::WATCH_CHECKOUT_CHANGE_REQUEST,
                serde_json::Value::Null,
            )
            .await
            .expect("subscribe_checked");
        drop(subscription);

        let dropped = tokio::time::timeout(std::time::Duration::from_secs(2), dropped_rx).await;
        assert!(
            dropped.is_ok(),
            "dropping stream subscription must cancel and tear down server task"
        );
        // The next unary call still works — the dead stream didn't wedge the connection.
        let echoed = client.call("Echo", serde_json::json!(2)).await.unwrap();
        assert_eq!(echoed, serde_json::json!(2));
    }

    #[test]
    fn test_trajectory_cursor_ordering_revision_and_legacy_serialization() {
        let c1 = TrajectoryCursor::new(1, 0);
        let c2 = TrajectoryCursor::new(1, 1);
        let c3 = TrajectoryCursor::new(2, 0);
        let c4 = TrajectoryCursor::new(1, u32::MAX);

        assert!(c1 < c2);
        assert!(c2 < c4);
        assert!(c4.with_rev(u64::MAX) < c3);
        assert!(c2.with_rev(1) < c2.with_rev(2));

        let json = serde_json::to_string(&c2.with_rev(7)).unwrap();
        assert_eq!(json, r#"{"sourceSeq":1,"subSeq":1,"rev":7}"#);
        let parsed: TrajectoryCursor = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, c2.with_rev(7));

        let legacy: TrajectoryCursor =
            serde_json::from_str(r#"{"sourceSeq":1,"subSeq":1}"#).unwrap();
        assert_eq!(legacy, c2);
    }

    #[test]
    fn test_watch_trajectory_params_and_items_serde() {
        let params = WatchTrajectoryParams::new("chat-123")
            .with_cursor((5, 2))
            .with_limit(100);
        let json = serde_json::to_string(&params).unwrap();
        assert!(json.contains(r#""chatId":"chat-123""#));
        assert!(json.contains(r#""afterCursor":{"sourceSeq":5,"subSeq":2,"rev":0}"#));
        assert!(json.contains(r#""limit":100"#));

        let parsed: WatchTrajectoryParams = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, params);

        // Snapshot item
        let snap_raw = r#"{
            "kind": "snapshot",
            "records": [],
            "watermark": {"sourceSeq": 10, "subSeq": 0},
            "degraded": [{
                "chatId": "chat-123",
                "runId": "run-1",
                "fromSeq": 1,
                "toSeq": 3,
                "reason": "storage fault",
                "recordedAt": "2026-09-01T12:00:00Z"
            }],
            "hasMore": false
        }"#;
        let snap: TrajectoryWatchItem = serde_json::from_str(snap_raw).unwrap();
        let snap_json = serde_json::to_string(&snap).unwrap();
        assert!(snap_json.contains(r#""kind":"snapshot""#));
        let parsed_snap: TrajectoryWatchItem = serde_json::from_str(&snap_json).unwrap();
        assert_eq!(parsed_snap, snap);

        // Deltas item
        let deltas = TrajectoryWatchItem::Deltas {
            records: Vec::new(),
            watermark: Some(TrajectoryCursor::new(11, 0)),
        };
        let deltas_json = serde_json::to_string(&deltas).unwrap();
        assert!(deltas_json.contains(r#""kind":"deltas""#));

        // Degraded item
        let deg = TrajectoryWatchItem::Degraded {
            intervals: Vec::new(),
        };
        let deg_json = serde_json::to_string(&deg).unwrap();
        assert!(deg_json.contains(r#""kind":"degraded""#));

        // ResyncRequired item
        let resync = TrajectoryWatchItem::ResyncRequired {
            reason: "gap detected".into(),
        };
        let resync_json = serde_json::to_string(&resync).unwrap();
        assert!(resync_json.contains(r#""kind":"resyncRequired""#));
        assert!(resync_json.contains(r#""reason":"gap detected""#));

        // Terminal item
        let term = TrajectoryWatchItem::Terminal {
            reason: TrajectoryTerminalReason::ChatDeleted,
            message: Some("Chat was deleted".into()),
        };
        let term_json = serde_json::to_string(&term).unwrap();
        assert!(term_json.contains(r#""kind":"terminal""#));
        assert!(term_json.contains(r#""reason":"chatDeleted""#));
    }

    #[test]
    fn test_reveal_trajectory_raw_params_and_result_serde() {
        let params = RevealTrajectoryRawParams::new(
            "chat-456",
            42,
            Some("parent-tool-1".into()),
            Some("call-99".into()),
            TrajectoryRawField::Payload,
        );
        let json = serde_json::to_string(&params).unwrap();
        assert!(json.contains(r#""chatId":"chat-456""#));
        assert!(json.contains(r#""sourceSeq":42"#));
        assert!(json.contains(r#""parentToolUseId":"parent-tool-1""#));
        assert!(json.contains(r#""callId":"call-99""#));
        assert!(json.contains(r#""field":"payload""#));
        assert!(json.contains(r#""sourceVersion":1"#));

        let parsed: RevealTrajectoryRawParams = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.source_version, CURRENT_RAW_SOURCE_VERSION);

        // Omitted sourceVersion defaults to CURRENT_RAW_SOURCE_VERSION
        let legacy_json = r#"{"chatId":"chat-456","sourceSeq":42,"field":"payload"}"#;
        let parsed_legacy: RevealTrajectoryRawParams = serde_json::from_str(legacy_json).unwrap();
        assert_eq!(parsed_legacy.source_version, CURRENT_RAW_SOURCE_VERSION);

        // Explicit unknown version is parsed accurately (no silent rewrite)
        let unknown_json =
            r#"{"chatId":"chat-456","sourceSeq":42,"field":"payload","sourceVersion":99}"#;
        let parsed_unknown: RevealTrajectoryRawParams = serde_json::from_str(unknown_json).unwrap();
        assert_eq!(parsed_unknown.source_version, 99);

        let raw_ref = params.to_raw_ref();
        assert_eq!(raw_ref.chat_id, "chat-456");
        assert_eq!(raw_ref.source_seq, 42);
        assert_eq!(raw_ref.source_version, CURRENT_RAW_SOURCE_VERSION);

        // Version 99 preserves across all From and to_raw_ref conversions
        let v99_params =
            RevealTrajectoryRawParams::new("chat-99", 100, None, None, TrajectoryRawField::Payload)
                .with_version(99);
        assert_eq!(v99_params.source_version, 99);

        let v99_raw_ref = v99_params.to_raw_ref();
        assert_eq!(v99_raw_ref.source_version, 99);

        let round_trip_params: RevealTrajectoryRawParams = v99_raw_ref.clone().into();
        assert_eq!(round_trip_params.source_version, 99);

        let round_trip_ref_params: RevealTrajectoryRawParams = (&v99_raw_ref).into();
        assert_eq!(round_trip_ref_params.source_version, 99);

        let round_trip_raw_ref: TrajectoryRawRef = round_trip_params.into();
        assert_eq!(round_trip_raw_ref.source_version, 99);

        let avail = TrajectoryRawRevealResult::available(
            TrajectoryRawField::Payload,
            "const secret = 'raw';",
        );
        let avail_json = serde_json::to_string(&avail).unwrap();
        assert!(avail_json.contains(r#""status":"available""#));
        assert!(avail_json.contains(r#""text":"const secret = 'raw';""#));

        let unavail = TrajectoryRawRevealResult::unavailable(
            TrajectoryRawField::Result,
            TrajectoryUnavailableReason::ForeignDevice,
            Some("chat is on another device".into()),
        );
        let unavail_json = serde_json::to_string(&unavail).unwrap();
        assert!(unavail_json.contains(r#""status":"unavailable""#));
        assert!(unavail_json.contains(r#""reason":"foreignDevice""#));
    }

    #[test]
    fn test_trajectory_rpc_methods_local_only() {
        assert_eq!(methods::WATCH_TRAJECTORY, "WatchTrajectory");
        assert_eq!(methods::REVEAL_TRAJECTORY_RAW, "RevealTrajectoryRaw");
        assert!(methods::is_local_only(methods::WATCH_TRAJECTORY));
        assert!(methods::is_local_only(methods::REVEAL_TRAJECTORY_RAW));
        assert!(methods::is_local_only(methods::PROBE_LIVE_VOICE));
        assert!(methods::is_local_only(methods::LOCAL_DEVICE));
        assert!(methods::is_local_only(methods::STOP_ENGINE));
        assert!(!methods::is_local_only(methods::LIST_HARNESSES));
        assert!(!methods::is_local_only(methods::FETCH_TOOL_INPUT));
    }
}
