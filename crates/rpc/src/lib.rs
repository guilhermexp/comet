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

mod caller;
mod client;
pub mod device_room;
pub mod ipc_auth;
pub mod method;
mod server;
mod ws;

pub use caller::{Caller, caller, with_caller};
pub use client::{
    RpcClient, RpcSubscription, connect_ipc, connect_ipc_as_agent, connect_ipc_url, connect_ws,
};
pub use device_room::{
    CallerProof, CallerProofFn, DeviceFrameHeader, DeviceLink, HostRelay, HostRelayConfig,
    LinkCache, LinkCacheConfig, NudgeHandler, PeerLiveness, PeerLivenessProbe, StaticToken,
    TokenError, TokenSource, caller_proof_message, decode_device_frame, device_room_ws_url,
    encode_device_frame, is_liveness_probe,
};
pub use method::{ALL_METHOD_NAMES, MethodInfo, RpcMethod, info, methods};
pub use server::{serve_connection, serve_ws_listener, serve_ws_listener_scoped};

/// Methods answered only over this device's own local IPC, never to a peer
/// device through the device-room relay ([`HostRelay`] refuses them): they
/// hand the caller this device's credentials.
pub fn local_only(method: &str) -> bool {
    methods::is_local_only(method)
}

/// Methods a peer device may not have answered here even though they're
/// relay-forwardable: allowing a plugin is this device's person's decision,
/// made in this device's consent window; and a file path names a file on
/// the caller's device, never this one. Declining, turning off, removing,
/// and installing from a link (which lands waiting here) stay open to your
/// other devices (Settings → Plugins' device switcher).
pub fn peer_refused(method: &str, params: &serde_json::Value) -> bool {
    if local_only(method) {
        return true;
    }
    match method {
        methods::DECIDE_PLUGIN => params
            .get("allow")
            .is_none_or(|allow| allow != &serde_json::Value::Bool(false)),
        methods::INSTALL_PLUGIN => params.get("path").is_some_and(|path| !path.is_null()),
        _ => false,
    }
}

/// The methods an agent's `zeron mcp` server may call with the
/// agent-scoped IPC token: what its tools need and nothing that grants
/// power. An allow-list, so a method added later stays the app's until
/// it's listed here. Never here: deciding or reviewing (`Decide*`,
/// `Review*`), signing (`SignPlugin`, `PluginSigningKey`), updating
/// (`UpdatePlugin`, `UpdateTemplate`, the engine's and harnesses'),
/// credentials (`AccessToken`, sign-in, `StepUp`, `DeleteAccount`),
/// terminals, files, vault and agent logins, and a command's whole output
/// (`ReadToolOutput`: an agent reads a chat as its doc keeps it).
pub fn agent_allowed(method: &str) -> bool {
    matches!(
        method,
        // Discovery.
        methods::ENGINE_INFO
            | methods::LOCAL_DEVICE
            | methods::WATCH_DEVICES
            | methods::WATCH_SPACES
            | methods::WATCH_CHATS
            | methods::WATCH_SESSIONS
            | methods::WATCH_DOC_MESSAGES
            | methods::LIST_HARNESSES
            | methods::LIST_MODELS
            | methods::LIST_AGENT_ACCOUNTS
            // Chats: create, message, steer, archive (`Mutate`).
            | methods::MUTATE
            | methods::QUEUE_MESSAGE
            | methods::QUEUE_COMMAND
            // Showing the person something in the calling chat.
            | methods::PUBLISH_VISUAL
            | methods::PUBLISH_IMAGE
            // Plugins: authoring, installing (lands waiting for the person),
            // checking for updates, and their tools.
            | methods::LIST_PLUGINS
            | methods::PLUGIN_STATUS
            | methods::NEW_PLUGIN
            | methods::INSTALL_PLUGIN
            | methods::CHECK_PLUGIN_UPDATES
            | methods::LIST_PLUGIN_TOOLS
            | methods::WATCH_PLUGIN_TOOLS
            | methods::CALL_PLUGIN_TOOL
            | methods::DESCRIBE_ELEMENTS
            // Templates: authoring and switching (the app shows an
            // unapproved one as a preview until the person keeps it), and
            // importing (lands waiting for the person).
            | methods::LIST_TEMPLATES
            | methods::DESCRIBE_APP
            | methods::CREATE_TEMPLATE
            | methods::USE_TEMPLATE
            | methods::TEMPLATE_STATUS
            | methods::DELETE_TEMPLATE
            | methods::EXPORT_TEMPLATE
            | methods::IMPORT_TEMPLATE
            // Themes: data only (no consent: a theme can't run anything),
            // read, made, changed, deleted (yours only) and worn. Not the
            // window's watch.
            | methods::LIST_THEMES
            | methods::GET_THEME
            | methods::CREATE_THEME
            | methods::UPDATE_THEME
            | methods::DELETE_THEME
            | methods::USE_THEME
    )
}

/// Stable capability error codes transported without provider stderr.
pub mod capability_errors {
    pub const PULL_REQUESTS_CLI_UNAVAILABLE: &str = "pull_requests.cli_unavailable";
    pub const PULL_REQUESTS_AUTHENTICATION: &str = "pull_requests.authentication";
    pub const PULL_REQUESTS_RATE_LIMITED: &str = "pull_requests.rate_limited";
    pub const PULL_REQUESTS_TIMEOUT: &str = "pull_requests.timeout";
    pub const PULL_REQUESTS_DECODE: &str = "pull_requests.decode";
    pub const PULL_REQUESTS_COMMAND_FAILED: &str = "pull_requests.command_failed";
    /// `ReadToolOutput`: the host's journal doesn't hold that command (an
    /// old chat, or one whose journal is gone).
    pub const TOOL_OUTPUT_UNAVAILABLE: &str = "tool_output.unavailable";
    /// `ReadToolOutput`: the command has no result yet.
    pub const TOOL_OUTPUT_NOT_FINISHED: &str = "tool_output.not_finished";
}

#[derive(Debug, thiserror::Error)]
pub enum RpcError {
    #[error("unknown method: {0}")]
    UnknownMethod(String),
    #[error("bad params: {0}")]
    BadParams(String),
    /// Stable machine-readable capability failure preserved across transports.
    #[error("capability error: {0}")]
    Capability(String),
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
    /// `"ok": null` is a reply whose value is `null` (for example an absent
    /// `Option`), distinct from a frame without `ok`.
    #[serde(
        default,
        deserialize_with = "present_value",
        skip_serializing_if = "Option::is_none"
    )]
    pub ok: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub err: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub item: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub done: bool,
}

fn present_value<'de, D>(deserializer: D) -> Result<Option<serde_json::Value>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    serde_json::Value::deserialize(deserializer).map(Some)
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
// Chat Recap Wire Types
// ---------------------------------------------------------------------------

/// Request parameters for `GenerateChatRecap`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GenerateChatRecapParams {
    pub chat_id: String,
}

impl GenerateChatRecapParams {
    pub fn new(chat_id: impl Into<String>) -> Self {
        Self {
            chat_id: chat_id.into(),
        }
    }
}

/// Reply for `GenerateChatRecap`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GenerateChatRecapReply {
    pub recap: Option<String>,
}

impl GenerateChatRecapReply {
    pub fn some(recap: impl Into<String>) -> Self {
        Self {
            recap: Some(recap.into()),
        }
    }

    pub fn none() -> Self {
        Self { recap: None }
    }
}

/// Request parameters for `WorkerWaitState`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkerWaitStateParams {
    pub chat_id: String,
}

impl WorkerWaitStateParams {
    pub fn new(chat_id: impl Into<String>) -> Self {
        Self {
            chat_id: chat_id.into(),
        }
    }
}

/// Reply for `WorkerWaitState`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkerWaitStateReply {
    pub pending_message: bool,
}

impl WorkerWaitStateReply {
    pub fn new(pending_message: bool) -> Self {
        Self { pending_message }
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
            if !matches!(
                method,
                methods::WATCH_CHECKOUT_CHANGE_REQUEST | methods::WATCH_HARNESS_UPDATES | "Silent"
            ) {
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
            if method == methods::WATCH_HARNESS_UPDATES {
                // Update watches send an initial status snapshot, then may stay
                // quiet indefinitely. Legacy peers do not send a readiness frame.
                return Ok(RpcReply::Stream(
                    futures::stream::once(async { serde_json::json!([]) })
                        .chain(stream)
                        .boxed(),
                ));
            }
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
                "Capability" => Err(RpcError::Capability(
                    capability_errors::PULL_REQUESTS_AUTHENTICATION.into(),
                )),
                other => Err(RpcError::UnknownMethod(other.into())),
            }
        }
    }

    #[test]
    fn plugin_methods_are_local_only_but_managing_delivering_and_serving() {
        for method in [
            methods::NEW_PLUGIN,
            methods::SIGN_PLUGIN,
            methods::VERIFY_PLUGIN,
            methods::LIST_PLUGIN_TOOLS,
            methods::WATCH_PLUGIN_TOOLS,
            methods::CALL_PLUGIN_TOOL,
            methods::SERVE_PLUGIN_TOOLS,
            methods::PLUGIN_TOOL_RESULT,
            methods::PLUGIN_SEND,
            methods::CALL_PLUGIN_SERVICE,
            methods::WATCH_PLUGIN_CHANNEL,
            methods::PLUGIN_SIGNING_KEY,
            methods::LIST_TEMPLATES,
            methods::CREATE_TEMPLATE,
            methods::USE_TEMPLATE,
            methods::TEMPLATE_STATUS,
            methods::DELETE_TEMPLATE,
            methods::EXPORT_TEMPLATE,
            methods::IMPORT_TEMPLATE,
            methods::DECIDE_TEMPLATE,
            methods::CHECK_TEMPLATE_UPDATES,
            methods::UPDATE_TEMPLATE,
            methods::DESCRIBE_APP,
            methods::LIST_THEMES,
            methods::GET_THEME,
            methods::CREATE_THEME,
            methods::UPDATE_THEME,
            methods::DELETE_THEME,
            methods::USE_THEME,
            methods::WATCH_THEMES,
        ] {
            assert!(local_only(method), "{method}");
        }
        assert!(!local_only(methods::PLUGIN_DELIVER));
        assert!(!local_only(methods::SERVE_PLUGIN_CALL));
        // Your other devices manage this one's plugins (Settings → Plugins).
        for method in [
            methods::LIST_PLUGINS,
            methods::WATCH_PLUGINS,
            methods::DECIDE_PLUGIN,
            methods::SET_PLUGIN_OFF,
            methods::REMOVE_PLUGIN,
            methods::INSTALL_PLUGIN,
            methods::CHECK_PLUGIN_UPDATES,
            methods::UPDATE_PLUGIN,
            methods::PLUGIN_STATUS,
        ] {
            assert!(!local_only(method), "{method}");
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

        let err = client
            .call("Capability", serde_json::Value::Null)
            .await
            .unwrap_err();
        assert!(matches!(
            err,
            RpcError::Capability(code)
                if code == capability_errors::PULL_REQUESTS_AUTHENTICATION
        ));
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
    async fn scoped_subscription_returns_before_first_item_and_cancels_silence() {
        let (dropped_tx, dropped_rx) = tokio::sync::oneshot::channel();
        let service = Arc::new(CancelAwareService {
            dropped: Mutex::new(Some(dropped_tx)),
        });
        let client = memory_client(service.clone());
        let stream = tokio::time::timeout(
            std::time::Duration::from_secs(1),
            client.subscribe_scoped("Silent", serde_json::Value::Null),
        )
        .await
        .unwrap()
        .unwrap();
        while service.dropped.lock().unwrap().is_some() {
            tokio::task::yield_now().await;
        }
        drop(stream);
        tokio::time::timeout(std::time::Duration::from_secs(1), dropped_rx)
            .await
            .unwrap()
            .unwrap();
    }

    #[tokio::test]
    async fn dropping_one_device_update_watch_preserves_the_other() {
        let (first_tx, first_rx) = tokio::sync::oneshot::channel();
        let (second_tx, mut second_rx) = tokio::sync::oneshot::channel();
        let first = memory_client(Arc::new(CancelAwareService {
            dropped: Mutex::new(Some(first_tx)),
        }));
        let second = memory_client(Arc::new(CancelAwareService {
            dropped: Mutex::new(Some(second_tx)),
        }));
        let first_watch = first
            .subscribe_checked(methods::WATCH_HARNESS_UPDATES, serde_json::Value::Null)
            .await
            .unwrap();
        let second_watch = second
            .subscribe_checked(methods::WATCH_HARNESS_UPDATES, serde_json::Value::Null)
            .await
            .unwrap();

        drop(first_watch);
        tokio::time::timeout(std::time::Duration::from_secs(1), first_rx)
            .await
            .expect("first device's quiet stream cancelled")
            .expect("first drop signal");
        assert!(matches!(
            second_rx.try_recv(),
            Err(tokio::sync::oneshot::error::TryRecvError::Empty)
        ));

        drop(second_watch);
        tokio::time::timeout(std::time::Duration::from_secs(1), second_rx)
            .await
            .expect("second device's quiet stream cancelled")
            .expect("second drop signal");
    }

    #[tokio::test]
    async fn unary_null_reply_resolves_the_call() {
        let client = memory_client(Arc::new(TestService));
        let reply = tokio::time::timeout(
            std::time::Duration::from_secs(1),
            client.call("Echo", serde_json::Value::Null),
        )
        .await
        .expect("a null reply is a reply")
        .unwrap();
        assert_eq!(reply, serde_json::Value::Null);
        let frame: ServerFrame = serde_json::from_str(r#"{"id":1,"ok":null}"#).unwrap();
        assert_eq!(frame.ok, Some(serde_json::Value::Null));
        let frame: ServerFrame = serde_json::from_str(r#"{"id":1,"done":true}"#).unwrap();
        assert_eq!(frame.ok, None);
    }

    /// Serve `TestService` on an ephemeral port behind `token`.
    async fn serve_test(token: &str) -> u16 {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        tokio::spawn(serve_ws_listener(
            listener,
            Arc::new(TestService),
            token.to_string(),
        ));
        port
    }

    #[tokio::test]
    async fn websocket_round_trip() {
        let token = ipc_auth::generate_token().unwrap();
        let port = serve_test(&token).await;

        let client = connect_ws(&format!("ws://127.0.0.1:{port}"), Some(&token))
            .await
            .unwrap();
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

    /// A reply past tungstenite's default 16 MiB frame cap (a long chat's
    /// transcript watch) must arrive, not drop the connection.
    #[tokio::test]
    async fn websocket_carries_messages_past_the_default_frame_cap() {
        let token = ipc_auth::generate_token().unwrap();
        let port = serve_test(&token).await;
        // Multi-byte chars so fragment boundaries land mid-codepoint.
        let big = "é".repeat(9_500_000);

        let client = connect_ws(&format!("ws://127.0.0.1:{port}"), Some(&token))
            .await
            .unwrap();
        let echoed = client.call("Echo", serde_json::json!(big)).await.unwrap();
        assert_eq!(echoed.as_str().map(str::len), Some(big.len()));
        assert_eq!(client.close_reason(), None);

        // A peer still on tungstenite's default limits (an older app build)
        // reassembles the fragmented reply too.
        use futures::{SinkExt, StreamExt};
        use tokio_tungstenite::tungstenite::{Message, client::IntoClientRequest};
        let mut req = format!("ws://127.0.0.1:{port}")
            .into_client_request()
            .unwrap();
        req.headers_mut()
            .insert("authorization", format!("Bearer {token}").parse().unwrap());
        let (mut legacy, _) = tokio_tungstenite::connect_async(req).await.unwrap();
        let request = serde_json::to_string(&ClientFrame {
            id: 1,
            method: Some("Echo".into()),
            params: serde_json::json!(big),
            cancel: false,
        })
        .unwrap();
        // It sends one unfragmented frame past 16 MiB: the raised cap takes it.
        assert!(request.len() > 16 << 20);
        legacy.send(Message::Text(request)).await.unwrap();
        let reply = loop {
            match legacy.next().await.unwrap().unwrap() {
                Message::Text(text) => break text,
                _ => continue,
            }
        };
        let frame: ServerFrame = serde_json::from_str(&reply).unwrap();
        assert_eq!(
            frame.ok.and_then(|v| v.as_str().map(str::len)),
            Some(big.len())
        );
    }

    /// Raw handshake status for a dial with the given headers (`None` = served).
    async fn handshake_status(port: u16, headers: &[(&str, &str)]) -> Option<u16> {
        use tokio_tungstenite::tungstenite::client::IntoClientRequest;
        let mut req = format!("ws://127.0.0.1:{port}")
            .into_client_request()
            .unwrap();
        for (name, value) in headers {
            req.headers_mut().insert(
                tokio_tungstenite::tungstenite::http::HeaderName::from_bytes(name.as_bytes())
                    .unwrap(),
                value.parse().unwrap(),
            );
        }
        match tokio_tungstenite::connect_async(req).await {
            Ok(_) => None,
            Err(tokio_tungstenite::tungstenite::Error::Http(response)) => {
                Some(response.status().as_u16())
            }
            Err(other) => panic!("unexpected handshake failure: {other}"),
        }
    }

    #[tokio::test]
    async fn handshake_with_origin_header_is_rejected() {
        let token = ipc_auth::generate_token().unwrap();
        let port = serve_test(&token).await;
        let bearer = format!("Bearer {token}");

        // A browser page opening ws://127.0.0.1:{port} always sends Origin;
        // the server must refuse the handshake before serving any RPC — even
        // when the bearer is right.
        assert_eq!(
            handshake_status(
                port,
                &[
                    ("origin", "https://evil.example"),
                    ("authorization", &bearer)
                ]
            )
            .await,
            Some(403),
            "handshake carrying an Origin header must be rejected"
        );

        // A native viewport (no Origin) still connects and can call RPC — the
        // reject must not be a blanket denial.
        let client = connect_ws(&format!("ws://127.0.0.1:{port}"), Some(&token))
            .await
            .unwrap();
        let echoed = client.call("Echo", serde_json::json!("ok")).await.unwrap();
        assert_eq!(echoed, serde_json::json!("ok"));
    }

    /// Answers every method with the caller it ran as.
    struct WhoAsked;
    #[async_trait]
    impl RpcService for WhoAsked {
        async fn handle(
            &self,
            _method: &str,
            _params: serde_json::Value,
        ) -> Result<RpcReply, RpcError> {
            Ok(RpcReply::Value(serde_json::json!(format!(
                "{:?}",
                crate::caller()
            ))))
        }
    }

    /// R6/R16: the agent-scoped token (what `zeron mcp` holds) reaches its
    /// tools' methods as an agent, and nothing that decides, signs, updates
    /// or hands out credentials; the full token still reaches everything.
    #[tokio::test]
    async fn the_agent_token_refuses_everything_that_grants_power() {
        let tokens = ipc_auth::IpcTokens::generate().unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        tokio::spawn(serve_ws_listener_scoped(
            listener,
            Arc::new(WhoAsked),
            tokens.clone(),
        ));
        let url = format!("ws://127.0.0.1:{port}");

        let agent = connect_ws(&url, Some(&tokens.agent)).await.unwrap();
        for method in [
            methods::DECIDE_PLUGIN,
            methods::REVIEW_PLUGIN,
            methods::SIGN_PLUGIN,
            methods::PLUGIN_SIGNING_KEY,
            methods::DECIDE_TEMPLATE,
            methods::REVIEW_TEMPLATE,
            methods::UPDATE_PLUGIN,
            methods::UPDATE_TEMPLATE,
            methods::APPLY_UPDATE,
            methods::ACCESS_TOKEN,
            methods::STEP_UP,
            methods::DELETE_ACCOUNT,
            methods::SIGN_IN,
            methods::COMPLETE_SIGN_IN,
            methods::SIGN_OUT,
            methods::SET_PLUGIN_OFF,
            methods::REMOVE_PLUGIN,
            methods::SET_BUILTIN_PLUGINS,
            methods::OPEN_TERMINAL,
            methods::WRITE_TERMINAL,
            methods::WRITE_WORKSPACE_FILE,
            methods::EXPORT_PLUGIN,
            // The window's own watch of the theme files.
            methods::WATCH_THEMES,
            methods::READ_TOOL_OUTPUT,
        ] {
            assert!(!agent_allowed(method), "{method}");
            match agent.call(method, serde_json::json!({})).await {
                Err(RpcError::Failed(message)) => assert!(
                    message.contains("isn't open to agents"),
                    "{method}: {message}"
                ),
                other => panic!("an agent reached {method}: {other:?}"),
            }
        }
        let asked = agent
            .call(methods::LIST_PLUGINS, serde_json::json!({}))
            .await
            .unwrap();
        assert!(asked.as_str().unwrap().starts_with("Agent"), "{asked}");
        // The theme tools' methods, exactly: data only.
        for method in [
            methods::LIST_THEMES,
            methods::GET_THEME,
            methods::CREATE_THEME,
            methods::UPDATE_THEME,
            methods::DELETE_THEME,
            methods::USE_THEME,
        ] {
            assert!(agent_allowed(method), "{method}");
            let asked = agent.call(method, serde_json::json!({})).await.unwrap();
            assert!(
                asked.as_str().unwrap().starts_with("Agent"),
                "{method}: {asked}"
            );
        }

        let app = connect_ws(&url, Some(&tokens.full)).await.unwrap();
        for method in [methods::DECIDE_PLUGIN, methods::ACCESS_TOKEN] {
            let asked = app.call(method, serde_json::json!({})).await.unwrap();
            assert_eq!(asked, serde_json::json!("Local"), "{method}");
        }
    }

    #[test]
    fn peers_may_decline_but_never_allow() {
        let hash = "ab".repeat(32);
        assert!(peer_refused(
            methods::DECIDE_PLUGIN,
            &serde_json::json!({"id": "x", "allow": true, "hash": hash})
        ));
        assert!(peer_refused(
            methods::DECIDE_PLUGIN,
            &serde_json::json!({"id": "x", "allow": "yes"})
        ));
        assert!(!peer_refused(
            methods::DECIDE_PLUGIN,
            &serde_json::json!({"id": "x", "allow": false})
        ));
        assert!(peer_refused(
            methods::INSTALL_PLUGIN,
            &serde_json::json!({"path": "/x"})
        ));
        assert!(!peer_refused(
            methods::INSTALL_PLUGIN,
            &serde_json::json!({"url": "https://x", "path": null})
        ));
        assert!(peer_refused(methods::REVIEW_PLUGIN, &serde_json::json!({})));
        assert!(peer_refused(methods::SIGN_PLUGIN, &serde_json::json!({})));
        assert!(peer_refused(
            methods::DELETE_ACCOUNT,
            &serde_json::json!({})
        ));
    }

    #[tokio::test]
    async fn handshake_requires_the_engine_bearer() {
        let token = ipc_auth::generate_token().unwrap();
        let port = serve_test(&token).await;

        // Any local process can reach the port; without the token it gets 401.
        assert_eq!(handshake_status(port, &[]).await, Some(401));
        let wrong = format!("Bearer {}", ipc_auth::generate_token().unwrap());
        assert_eq!(
            handshake_status(port, &[("authorization", &wrong)]).await,
            Some(401)
        );
        assert_eq!(
            handshake_status(port, &[("authorization", &token)]).await,
            Some(401),
            "the Bearer scheme is required"
        );
        let url = format!("ws://127.0.0.1:{port}");
        assert!(connect_ws(&url, None).await.is_err());
        let error = match connect_ws(&url, Some("not-the-token")).await {
            Ok(_) => panic!("a wrong token must not be served"),
            Err(error) => error.to_string(),
        };
        assert!(error.contains("401"), "{error}");

        // The right token is served.
        let right = format!("Bearer {token}");
        assert_eq!(
            handshake_status(port, &[("authorization", &right)]).await,
            None
        );
        let client = connect_ws(&url, Some(&token)).await.unwrap();
        assert_eq!(
            client.call("Echo", serde_json::json!(7)).await.unwrap(),
            serde_json::json!(7)
        );
    }

    #[tokio::test]
    async fn connect_ipc_reads_the_published_token_at_dial_time() {
        let dir = tempfile::tempdir().unwrap();
        // Engine start #1.
        let first = ipc_auth::issue_token(dir.path()).unwrap();
        let first_port = serve_test(&first).await;
        let client = connect_ipc(first_port, Some(dir.path())).await.unwrap();
        assert_eq!(
            client.call("Echo", serde_json::json!(1)).await.unwrap(),
            serde_json::json!(1)
        );

        // Engine restart: a new token replaces the file. The next dial picks
        // it up without any client-side configuration.
        let second = ipc_auth::issue_token(dir.path()).unwrap();
        let second_port = serve_test(&second).await;
        let client = connect_ipc(second_port, Some(dir.path())).await.unwrap();
        assert_eq!(
            client.call("Echo", serde_json::json!(2)).await.unwrap(),
            serde_json::json!(2)
        );

        // A client holding only the stale file is refused with a clear reason.
        let stale = tempfile::tempdir().unwrap();
        ipc_auth::write_token(&ipc_auth::token_path(stale.path()), &first).unwrap();
        let error = match connect_ipc(second_port, Some(stale.path())).await {
            Ok(_) => panic!("a rotated token must not be served"),
            Err(error) => error.to_string(),
        };
        assert!(error.contains("rejected"), "{error}");
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
    fn test_rpc_methods_local_only() {
        assert!(methods::is_local_only(methods::LOCAL_DEVICE));
        assert!(methods::is_local_only(methods::STOP_ENGINE));
        assert!(!methods::is_local_only(methods::LIST_HARNESSES));
        assert!(!methods::is_local_only(methods::FETCH_TOOL_INPUT));
        assert!(methods::is_local_only(methods::WORKER_WAIT_STATE));
    }
}
