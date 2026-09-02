//! The single registry of RPC methods.
//!
//! One entry per method carries everything both ends need to agree on: the wire
//! name, the params/reply types, whether the device room may relay it, whether
//! its reply is a stream, and the reply deadline a relay-forwarded unary call
//! gets. Adding an RPC is one line in [`rpc_methods!`] plus the engine handler —
//! there is no second list to keep in sync (the engine used to carry three:
//! `forwardable`, `is_stream_method`, `forward_deadline`).
//!
//! The const names and their values are wire: `methods::LIST_REPOS` is what the
//! UI, the engine and the relay all spell out, so neither may be renamed. The
//! marker type is the same name in CamelCase ([`ListRepos`]) and exists so a
//! call site can name a method as a type: `call_typed::<ListRepos>(params)`.

use std::time::Duration;

/// One RPC method as a type. `NAME` is the wire method string; the rest is what
/// the transport needs to know before it has seen a single byte of payload.
pub trait RpcMethod {
    /// Wire method name.
    const NAME: &'static str;
    /// Whether the device room may relay this call to `targetDeviceId`.
    const FORWARDABLE: bool;
    /// Whether the relay proxies the reply as a stream of items. Local-only
    /// watch methods stream on the wire too but are never forwarded, so they
    /// stay `false`: this flag drives relay routing, not reply shape.
    const STREAM: bool;
    /// Reply deadline for a relay-forwarded unary call.
    const DEADLINE: Duration;
    /// Request payload.
    type Params: serde::Serialize + serde::de::DeserializeOwned + Send;
    /// Reply payload (one item, for a stream).
    type Reply: serde::Serialize + serde::de::DeserializeOwned + Send;
}

/// The same facts as [`RpcMethod`], looked up by wire name at runtime.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MethodInfo {
    /// Wire method name.
    pub name: &'static str,
    /// Whether the device room may relay this call to `targetDeviceId`.
    pub forwardable: bool,
    /// Whether the relay proxies the reply as a stream of items (see
    /// [`RpcMethod::STREAM`]); implies `forwardable`.
    pub stream: bool,
    /// Reply deadline for a relay-forwarded unary call.
    pub deadline: Duration,
}

/// Declare the whole method surface: the `methods` consts (wire names), one
/// marker type per method, [`ALL_METHOD_NAMES`], and [`info`].
///
/// The const identifier and the marker identifier are both given because the
/// wire name is SCREAMING_CASE as a const and CamelCase as a type:
/// `LIST_REPOS / ListRepos = "ListRepos"`.
macro_rules! rpc_methods {
    ($(
        $(#[$meta:meta])*
        $konst:ident / $ty:ident = $name:literal {
            params: $p:ty,
            reply: $r:ty
            $(, forwardable: $fwd:literal)?
            $(, stream: $st:literal)?
            $(, deadline_secs: $dl:literal)?
        }
    ),* $(,)?) => {
        /// RPC method names — single source of truth for both ends.
        /// Full surface: docs/research/feature-inventory.md §2.
        pub mod methods {
            $(
                $(#[$meta])*
                pub const $konst: &str = $name;
            )*
        }

        /// Every wire name in declaration order.
        pub const ALL_METHOD_NAMES: &[&str] = &[ $( $name ),* ];

        $(
            #[doc = concat!("Marker type for the `", $name, "` RPC method.")]
            pub struct $ty;

            impl RpcMethod for $ty {
                const NAME: &'static str = $name;
                const FORWARDABLE: bool = false $(|| $fwd)?;
                const STREAM: bool = false $(|| $st)?;
                const DEADLINE: Duration = Duration::from_secs(30 $(* 0 + $dl)?);
                type Params = $p;
                type Reply = $r;
            }
        )*

        /// Look a method up by wire name. `None` means the name is not ours.
        pub fn info(name: &str) -> Option<MethodInfo> {
            match name {
                $(
                    $name => Some(MethodInfo {
                        name: $name,
                        forwardable: <$ty as RpcMethod>::FORWARDABLE,
                        stream: <$ty as RpcMethod>::STREAM,
                        deadline: <$ty as RpcMethod>::DEADLINE,
                    }),
                )*
                _ => None,
            }
        }
    };
}

// Deadlines: the relay is WebSocket frames through a DO, and a dropped frame
// (host socket replaced mid-call, DO restart) loses the reply SILENTLY — the
// DO's auto-pong keeps the client socket looking healthy — so an unbounded
// await wedged callers forever (the composer's permanent "Sending…",
// 2026-08-18). Network-bound git and update methods get a long leash;
// worktree creation checks out a full tree; everything else is interactive
// and must fail fast on the default 30s.
rpc_methods! {
    LIST_HARNESSES / ListHarnesses = "ListHarnesses" { params: serde_json::Value, reply: serde_json::Value, forwardable: true },
    /// Flip a harness's enablement on the target device (Settings → Agents);
    /// replies with the device's fresh `ListHarnesses` catalog.
    SET_HARNESS_ENABLED / SetHarnessEnabled = "SetHarnessEnabled" { params: serde_json::Value, reply: serde_json::Value, forwardable: true },
    LIST_MODELS / ListModels = "ListModels" { params: serde_json::Value, reply: serde_json::Value, forwardable: true },
    LIST_COMMANDS / ListCommands = "ListCommands" { params: serde_json::Value, reply: serde_json::Value, forwardable: true },
    QUEUE_COMMAND / QueueCommand = "QueueCommand" { params: serde_json::Value, reply: serde_json::Value, forwardable: true },
    /// App-owned durable delivery of a Worker lifecycle event to its existing
    /// parent chat. Uses a deterministic command id and fsync-equivalent store
    /// persistence before acknowledging the RPC.
    QUEUE_WORKER_NOTIFICATION / QueueWorkerNotification = "QueueWorkerNotification" { params: serde_json::Value, reply: serde_json::Value, forwardable: true },
    /// Peer-to-peer delivery fallback: the SENDER's engine forwards a queued
    /// command entry (client-minted id and all) straight over the device-room
    /// link when its chat2 rows can't reach the edge but the host's peer link
    /// is alive. The host claims the id in its processed ledger before
    /// executing, so the doc row arriving later dedupes to a no-op —
    /// exactly-once by construction. Params `{chatId, entry}`.
    RELAY_COMMAND / RelayCommand = "RelayCommand" { params: serde_json::Value, reply: serde_json::Value },
    /// User-driven delivery retry for a chat with unadopted queued sends:
    /// fresh chat2 socket, host nudge, drain pass, and a new delivery escort
    /// per pending command. Params `{chatId}`; IPC-only.
    RETRY_DELIVERY / RetryDelivery = "RetryDelivery" { params: serde_json::Value, reply: serde_json::Value },
    WATCH_DOC_MESSAGES / WatchDocMessages = "WatchDocMessages" { params: serde_json::Value, reply: serde_json::Value, forwardable: true, stream: true },
    /// Nudge every open room client to verify liveness NOW (window focus,
    /// app foregrounded). No params; IPC-only. Each room ignores the hint
    /// unless it has been broadcast-quiet ≥30s, so this is cheap to spam.
    PROBE_SYNC / ProbeSync = "ProbeSync" { params: serde_json::Value, reply: serde_json::Value },
    /// Live sync introspection (`zeron sync` / debug surfaces): per-room
    /// connection state, last pushed-frame/ack ages, rejoin/probe/resync
    /// counters for the workspace room and every open chat doc. No params;
    /// IPC-only.
    SYNC_STATUS / SyncStatus = "SyncStatus" { params: serde_json::Value, reply: serde_json::Value },
    /// Pushed edge-connectivity posture (`zeron_proto::Connectivity`):
    /// current value first, then every change — the connection pill /
    /// composer-honesty / queued-badge feed. No params; IPC-only.
    WATCH_CONNECTIVITY / WatchConnectivity = "WatchConnectivity" { params: serde_json::Value, reply: serde_json::Value },
    /// In-flight queued-attachment transfers (`zeron_proto::TransferProgress`
    /// list): current set first, then a fresh snapshot per landed chunk —
    /// the sending thumbnail's percent-ring feed. No params; IPC-only.
    WATCH_TRANSFERS / WatchTransfers = "WatchTransfers" { params: serde_json::Value, reply: serde_json::Value },
    WATCH_CHATS / WatchChats = "WatchChats" { params: serde_json::Value, reply: serde_json::Value },
    WATCH_DEVICES / WatchDevices = "WatchDevices" { params: serde_json::Value, reply: serde_json::Value },
    WATCH_SESSIONS / WatchSessions = "WatchSessions" { params: serde_json::Value, reply: serde_json::Value },
    /// Spaces registry (device+folder pairs) from the workspace doc.
    WATCH_SPACES / WatchSpaces = "WatchSpaces" { params: serde_json::Value, reply: serde_json::Value },
    /// Local-only OMP Live Voice lifecycle. Media remains inside OMP; Comet
    /// exposes only control, state, and transcript metadata.
    PROBE_LIVE_VOICE / ProbeLiveVoice = "ProbeLiveVoice" { params: serde_json::Value, reply: serde_json::Value },
    START_LIVE_VOICE / StartLiveVoice = "StartLiveVoice" { params: serde_json::Value, reply: serde_json::Value },
    SET_LIVE_VOICE_MUTED / SetLiveVoiceMuted = "SetLiveVoiceMuted" { params: serde_json::Value, reply: serde_json::Value },
    STOP_LIVE_VOICE / StopLiveVoice = "StopLiveVoice" { params: serde_json::Value, reply: serde_json::Value },
    WATCH_LIVE_VOICE / WatchLiveVoice = "WatchLiveVoice" { params: serde_json::Value, reply: serde_json::Value },
    /// Entity mutations against the workspace doc (feature-inventory §2 DataRpc).
    /// Params are tagged `{op: createChat|createSpace|renameSpace|deleteSpace|
    /// renameChat|setChatArchived|deleteChat|renameDevice|markChatSeen, …}`.
    MUTATE / Mutate = "Mutate" { params: serde_json::Value, reply: serde_json::Value },
    /// This engine's identity → `{deviceId}` (IPC-only; never relay-forwarded —
    /// the answer is about whichever engine you are directly connected to).
    LOCAL_DEVICE / LocalDevice = "LocalDevice" { params: serde_json::Value, reply: serde_json::Value },
    /// This engine runtime's fixed device and workspace identity.
    ENGINE_INFO / EngineInfo = "EngineInfo" { params: serde_json::Value, reply: serde_json::Value },
    /// Readiness barrier for the engine runtime. The call completes once stores
    /// and journals are assembled, or fails with the assembly error.
    ENGINE_READY / EngineReady = "EngineReady" { params: serde_json::Value, reply: serde_json::Value },
    /// Ask a headless IPC owner to drain its runtime and exit successfully.
    /// Headed IPC owners do not implement this method: closing another app's
    /// engine behind its windows would leave that process unusable.
    STOP_ENGINE / StopEngine = "StopEngine" { params: serde_json::Value, reply: serde_json::Value },
    AUTH_STATUS / AuthStatus = "AuthStatus" { params: serde_json::Value, reply: serde_json::Value },
    // AuthRpc mutations (feature-inventory §2 AuthRpc; IPC-only).
    SIGN_IN / SignIn = "SignIn" { params: serde_json::Value, reply: serde_json::Value },
    SIGN_IN_HEADLESS / SignInHeadless = "SignInHeadless" { params: serde_json::Value, reply: serde_json::Value },
    COMPLETE_SIGN_IN / CompleteSignIn = "CompleteSignIn" { params: serde_json::Value, reply: serde_json::Value },
    SIGN_OUT / SignOut = "SignOut" { params: serde_json::Value, reply: serde_json::Value },
    LIST_ORGS / ListOrgs = "ListOrgs" { params: serde_json::Value, reply: serde_json::Value },
    CREATE_ORG / CreateOrg = "CreateOrg" { params: serde_json::Value, reply: serde_json::Value },
    SELECT_ORG / SelectOrg = "SelectOrg" { params: serde_json::Value, reply: serde_json::Value },
    /// One-time local→synced profile import: what's importable (unary).
    LOCAL_IMPORT_STATUS / LocalImportStatus = "LocalImportStatus" { params: serde_json::Value, reply: serde_json::Value },
    /// One-time local→synced profile import: run it (stream of progress items).
    IMPORT_LOCAL_WORKSPACE / ImportLocalWorkspace = "ImportLocalWorkspace" { params: serde_json::Value, reply: serde_json::Value },
    // Repos / worktrees / folders (ControlRpc, relay-forwardable — device-local
    // filesystem state).
    LIST_REPOS / ListRepos = "ListRepos" { params: serde_json::Value, reply: serde_json::Value, forwardable: true },
    ADD_REPO / AddRepo = "AddRepo" { params: serde_json::Value, reply: serde_json::Value, forwardable: true },
    CLONE_REPO / CloneRepo = "CloneRepo" { params: serde_json::Value, reply: serde_json::Value, forwardable: true, deadline_secs: 900 },
    CREATE_REPO / CreateRepo = "CreateRepo" { params: serde_json::Value, reply: serde_json::Value, forwardable: true },
    LIST_BRANCHES / ListBranches = "ListBranches" { params: serde_json::Value, reply: serde_json::Value, forwardable: true },
    LIST_REFS / ListRefs = "ListRefs" { params: serde_json::Value, reply: serde_json::Value, forwardable: true },
    LIST_GIT_HISTORY / ListGitHistory = "ListGitHistory" { params: serde_json::Value, reply: serde_json::Value, forwardable: true },
    /// Update remote-tracking refs without changing HEAD, the index, or files.
    FETCH_ALL / FetchAll = "FetchAll" { params: serde_json::Value, reply: serde_json::Value, forwardable: true, deadline_secs: 900 },
    SWITCH_REF / SwitchRef = "SwitchRef" { params: serde_json::Value, reply: serde_json::Value, forwardable: true },
    LIST_FOLDERS / ListFolders = "ListFolders" { params: serde_json::Value, reply: serde_json::Value, forwardable: true },
    /// The device's browse roots: home plus mounted drives/volumes.
    LIST_DRIVES / ListDrives = "ListDrives" { params: serde_json::Value, reply: serde_json::Value, forwardable: true },
    /// Fuzzy relative-path search rooted in a known chat or space checkout.
    SEARCH_FILES / SearchFiles = "SearchFiles" { params: serde_json::Value, reply: serde_json::Value, forwardable: true },
    CREATE_WORKTREE / CreateWorktree = "CreateWorktree" { params: serde_json::Value, reply: serde_json::Value, forwardable: true, deadline_secs: 120 },
    DELETE_WORKTREE / DeleteWorktree = "DeleteWorktree" { params: serde_json::Value, reply: serde_json::Value, forwardable: true },
    // Terminals (ControlRpc, relay-forwardable — a terminal lives on the chat's
    // host device; SubscribeTerminal streams).
    OPEN_TERMINAL / OpenTerminal = "OpenTerminal" { params: serde_json::Value, reply: serde_json::Value, forwardable: true },
    SUBSCRIBE_TERMINAL / SubscribeTerminal = "SubscribeTerminal" { params: serde_json::Value, reply: serde_json::Value, forwardable: true, stream: true },
    WRITE_TERMINAL / WriteTerminal = "WriteTerminal" { params: serde_json::Value, reply: serde_json::Value, forwardable: true },
    RESIZE_TERMINAL / ResizeTerminal = "ResizeTerminal" { params: serde_json::Value, reply: serde_json::Value, forwardable: true },
    CLOSE_TERMINAL / CloseTerminal = "CloseTerminal" { params: serde_json::Value, reply: serde_json::Value, forwardable: true },
    /// Checkout-diff stream for the target device's chats (DataRpc,
    /// relay-forwardable — diffs are produced where the checkout lives).
    WATCH_CHECKOUT_DIFFS / WatchCheckoutDiffs = "WatchCheckoutDiffs" { params: serde_json::Value, reply: serde_json::Value, forwardable: true, stream: true },
    /// Current pull request for one checkout, resolved on the checkout's host device.
    WATCH_CHECKOUT_CHANGE_REQUEST / WatchCheckoutChangeRequest = "WatchCheckoutChangeRequest" { params: serde_json::Value, reply: serde_json::Value, forwardable: true, stream: true },
    GET_CHECKOUT_DIFF / GetCheckoutDiff = "GetCheckoutDiff" { params: serde_json::Value, reply: serde_json::Value, forwardable: true },
    GET_CHECKOUT_FILE_DIFF_TEXT / GetCheckoutFileDiffText = "GetCheckoutFileDiffText" { params: serde_json::Value, reply: serde_json::Value, forwardable: true },
    // Agent accounts (ControlRpc, relay-forwardable — CLI logins are per-device).
    LIST_AGENT_ACCOUNTS / ListAgentAccounts = "ListAgentAccounts" { params: serde_json::Value, reply: serde_json::Value, forwardable: true },
    ACTIVATE_AGENT_ACCOUNT / ActivateAgentAccount = "ActivateAgentAccount" { params: serde_json::Value, reply: serde_json::Value, forwardable: true },
    FORGET_AGENT_ACCOUNT / ForgetAgentAccount = "ForgetAgentAccount" { params: serde_json::Value, reply: serde_json::Value, forwardable: true },
    START_AGENT_LOGIN / StartAgentLogin = "StartAgentLogin" { params: serde_json::Value, reply: serde_json::Value, forwardable: true },
    COMPLETE_AGENT_LOGIN / CompleteAgentLogin = "CompleteAgentLogin" { params: serde_json::Value, reply: serde_json::Value, forwardable: true },
    POLL_AGENT_LOGIN / PollAgentLogin = "PollAgentLogin" { params: serde_json::Value, reply: serde_json::Value, forwardable: true },
    CANCEL_AGENT_LOGIN / CancelAgentLogin = "CancelAgentLogin" { params: serde_json::Value, reply: serde_json::Value, forwardable: true },
    // Uploads / attachments (ControlRpc, relay-forwardable — target the chat's host device).
    UPLOAD_CHUNK / UploadChunk = "UploadChunk" { params: serde_json::Value, reply: serde_json::Value, forwardable: true },
    UPLOAD_COMMIT / UploadCommit = "UploadCommit" { params: serde_json::Value, reply: serde_json::Value, forwardable: true },
    READ_ATTACHMENT_CHUNK / ReadAttachmentChunk = "ReadAttachmentChunk" { params: serde_json::Value, reply: serde_json::Value, forwardable: true },
    /// Lazy full-tool-output fetch from the R2 sidecar by doc-resident ref
    /// (chat2-sync A3). Edge-direct from any device — never relay-forwarded.
    FETCH_TOOL_BLOB / FetchToolBlob = "FetchToolBlob" { params: serde_json::Value, reply: serde_json::Value },
    /// Fetch sanitized historical Write/Edit input from the chat host's local
    /// run journal. Relay-forwardable via `targetDeviceId`.
    FETCH_TOOL_INPUT / FetchToolInput = "FetchToolInput" { params: serde_json::Value, reply: serde_json::Value, forwardable: true, deadline_secs: 20 },
    // Updates (ControlRpc, relay-forwardable — a device reports/applies its own
    // binary's update). Stream: current UpdateStatus, then every change.
    UPDATE_STATUS / UpdateStatus = "UpdateStatus" { params: serde_json::Value, reply: serde_json::Value, forwardable: true, stream: true },
    /// Download + apply the newest release on the target device (symlink-managed
    /// installs; the service restart is scheduled after the reply flushes).
    APPLY_UPDATE / ApplyUpdate = "ApplyUpdate" { params: serde_json::Value, reply: serde_json::Value, forwardable: true, deadline_secs: 900 },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_method_has_info_and_stream_implies_forwardable() {
        for name in ALL_METHOD_NAMES {
            let info = info(name).unwrap_or_else(|| panic!("{name} missing from registry"));
            if info.stream {
                assert!(info.forwardable, "{name}: stream methods are relay-proxied");
            }
        }
        assert!(info("Nope").is_none());
        assert_eq!(
            info(methods::CLONE_REPO).unwrap().deadline,
            std::time::Duration::from_secs(15 * 60)
        );
    }
}
