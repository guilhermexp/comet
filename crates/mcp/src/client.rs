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

use async_trait::async_trait;
use futures::stream::BoxStream;

use comet_proto::TerminalEvent;

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
