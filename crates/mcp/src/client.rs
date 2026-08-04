//! The engine seam the worker tools sit on.
//!
//! [`WorkerTools`](crate::WorkerTools) never touches a socket: it drives four
//! terminal RPCs through [`EngineClient`], so the whole bounded/resumable
//! behavior is pinned by fast unit tests against a scripted stub. The real
//! implementation over `comet_rpc::RpcClient` lives beside it in
//! [`RpcEngineClient`].
//!
//! Encoding lives at the wire boundary, not in the tool layer: `data` handed to
//! [`EngineClient::write_terminal`] is plain text (the implementation base64s
//! it, which is what `WriteTerminal` canonically takes), while
//! [`TerminalEvent::Data`] arrives base64 as the engine minted it and the tool
//! layer decodes it.

use std::sync::Arc;

use async_trait::async_trait;
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64;
use futures::StreamExt;
use futures::stream::BoxStream;
use serde_json::json;

use comet_proto::TerminalEvent;
use comet_rpc::{RpcClient, RpcError, methods};

/// Everything a worker tool can fail with. Each variant carries a sentence the
/// agent can act on — the MCP layer renders these as tool errors, never panics.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ToolError {
    /// No such worker: never spawned, already killed, or aged out of the
    /// engine's 30-minute exited-session window.
    #[error(
        "no worker {0} in this session: it was never spawned, was killed, or \
         aged past the engine's 30-minute exited-worker window"
    )]
    NotFound(String),
    /// The spawn was refused before it happened (worker-depth ceiling).
    #[error("{0}")]
    Refused(String),
    /// The engine reported a failure.
    #[error("{0}")]
    Engine(String),
}

/// The four terminal RPCs the worker tools need, and nothing else.
#[async_trait]
pub trait EngineClient: Send + Sync {
    /// `OpenTerminal` for `chat` on `device` (the local engine when `None`).
    /// Returns the engine-minted terminal id, which is the `worker_id`.
    async fn open_terminal(&self, chat: &str, device: Option<&str>) -> Result<String, ToolError>;
    /// `WriteTerminal`: `data` is plain text, written verbatim into the PTY.
    async fn write_terminal(
        &self,
        id: &str,
        data: &str,
        device: Option<&str>,
    ) -> Result<(), ToolError>;
    /// `SubscribeTerminal`: replay of everything with `seq > after_seq`, then
    /// the live tail. The stream ends after [`TerminalEvent::Exit`].
    async fn subscribe_terminal(
        &self,
        id: &str,
        after_seq: Option<u64>,
        device: Option<&str>,
    ) -> Result<BoxStream<'static, TerminalEvent>, ToolError>;
    /// `CloseTerminal`: kills the PTY and drops its replay buffer.
    async fn close_terminal(&self, id: &str, device: Option<&str>) -> Result<(), ToolError>;
}

/// The real seam: the four terminal RPCs over the engine's IPC WebSocket.
///
/// `target_device` rides as `targetDeviceId`, which the engine's relay honors
/// for every one of these methods (`crates/engine/src/rpc.rs` `forwardable`),
/// so a worker can live on another device without this layer knowing how.
pub struct RpcEngineClient {
    rpc: Arc<RpcClient>,
    /// PTY geometry for a worker's shell. A worker is not a viewport, but the
    /// PTY still needs a size, and a wide one keeps line-wrapping out of the
    /// output the agent reads back.
    cols: u16,
    rows: u16,
}

impl RpcEngineClient {
    pub fn new(rpc: Arc<RpcClient>) -> Self {
        Self {
            rpc,
            cols: 200,
            rows: 50,
        }
    }
}

/// `targetDeviceId` is omitted entirely when unset — the engine treats an
/// absent target as "this device", and a `null` would be a different thing.
fn with_device(mut params: serde_json::Value, device: Option<&str>) -> serde_json::Value {
    if let (Some(device), Some(object)) = (device, params.as_object_mut()) {
        object.insert("targetDeviceId".into(), json!(device));
    }
    params
}

/// The engine reports a missing terminal as a plain `Failed` message; the tool
/// layer owes the agent the difference between "gone" and "broken".
fn map_rpc_error(err: RpcError) -> ToolError {
    let message = err.to_string();
    if message.to_lowercase().contains("terminal not found") {
        ToolError::NotFound(message)
    } else {
        ToolError::Engine(message)
    }
}

#[async_trait]
impl EngineClient for RpcEngineClient {
    async fn open_terminal(&self, chat: &str, device: Option<&str>) -> Result<String, ToolError> {
        let session: comet_proto::TerminalSession = self
            .rpc
            .call_as(
                methods::OPEN_TERMINAL,
                with_device(
                    json!({ "chatId": chat, "cols": self.cols, "rows": self.rows }),
                    device,
                ),
            )
            .await
            .map_err(map_rpc_error)?;
        Ok(session.id)
    }

    async fn write_terminal(
        &self,
        id: &str,
        data: &str,
        device: Option<&str>,
    ) -> Result<(), ToolError> {
        // Base64 is `WriteTerminal`'s canonical encoding; the engine's plain
        // UTF-8 fallback only fires when the payload fails to decode, which a
        // short command can accidentally survive.
        self.rpc
            .call(
                methods::WRITE_TERMINAL,
                with_device(
                    json!({ "terminalId": id, "data": BASE64.encode(data.as_bytes()) }),
                    device,
                ),
            )
            .await
            .map(|_| ())
            .map_err(map_rpc_error)
    }

    async fn subscribe_terminal(
        &self,
        id: &str,
        after_seq: Option<u64>,
        device: Option<&str>,
    ) -> Result<BoxStream<'static, TerminalEvent>, ToolError> {
        let mut params = json!({ "terminalId": id });
        if let Some(after_seq) = after_seq {
            params["afterSeq"] = json!(after_seq);
        }
        let rx = self
            .rpc
            .subscribe(methods::SUBSCRIBE_TERMINAL, with_device(params, device))
            .await
            .map_err(map_rpc_error)?;
        // A frame we cannot parse is dropped, not fatal: the stream still has
        // to deliver the `Exit` that ends a `wait_worker`.
        Ok(futures::stream::unfold(rx, |mut rx| async move {
            loop {
                let value = rx.recv().await?;
                match serde_json::from_value::<TerminalEvent>(value) {
                    Ok(event) => return Some((event, rx)),
                    Err(err) => {
                        tracing::warn!(error = %err, "dropping malformed terminal event")
                    }
                }
            }
        })
        .boxed())
    }

    async fn close_terminal(&self, id: &str, device: Option<&str>) -> Result<(), ToolError> {
        self.rpc
            .call(
                methods::CLOSE_TERMINAL,
                with_device(json!({ "terminalId": id }), device),
            )
            .await
            .map(|_| ())
            .map_err(map_rpc_error)
    }
}
