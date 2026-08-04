//! comet-mcp — worker tools for the agent running inside a comet session.
//!
//! The agent can start a CLI worker, read it, block on it for an exit code, and
//! kill it, without spending its own turn on a shell call that times out. Four
//! MCP tools do it, each a thin wrapper over a terminal RPC the engine already
//! serves:
//!
//! | tool | RPCs |
//! | --- | --- |
//! | `spawn_worker` | `OpenTerminal` + `WriteTerminal` |
//! | `read_worker` | `SubscribeTerminal`, drain, cancel |
//! | `wait_worker` | `SubscribeTerminal` until `Exit` |
//! | `kill_worker` | `CloseTerminal` |
//!
//! The layering is the point: [`tools::WorkerTools`] is pure logic over the
//! [`client::EngineClient`] seam, so the bounded and resumable behavior is
//! pinned by unit tests with no socket. `comet mcp-server` is the only place a
//! real `RpcClient` and an `rmcp` stdio transport appear.
//!
//! Design: `docs/plans/2026-08-04-worker-tools-mcp-design.md`.

pub mod client;
pub mod server;
pub mod tools;

pub use client::{EngineClient, RpcEngineClient, ToolError};
pub use server::{INSTRUCTIONS, WorkerToolsServer, tool_definitions};
pub use tools::{
    DEFAULT_MAX_WORKER_DEPTH, DEFAULT_READ_TIMEOUT_MS, DEFAULT_WAIT_TIMEOUT_MS, KillResult,
    MAX_OUTPUT_BYTES, MAX_TIMEOUT_MS, ReadResult, SpawnResult, WaitResult, WorkerConfig,
    WorkerTools,
};
