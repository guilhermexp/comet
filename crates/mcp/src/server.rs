//! The MCP surface: four tools over stdio, backed by [`WorkerTools`].
//!
//! The server ships its own [`INSTRUCTIONS`] through the `initialize` reply, so
//! the agent discovers the tools without an entry in the repo's `CLAUDE.md` or
//! in the user's prompt. Tool descriptions state the bounds plainly — the
//! output is a tail, `nextSeq` is how you resume, a not-found means the worker
//! aged out — because the agent has nothing else to read.
//!
//! Errors reach the agent as tool-level errors (`CallToolResult::error`), never
//! as JSON-RPC protocol errors: a protocol error is rendered opaquely by MCP
//! clients, so the agent would see "internal error" instead of "that worker
//! aged past its 30-minute window".

use std::borrow::Cow;
use std::sync::Arc;

use rmcp::model::{
    CallToolRequestParams, CallToolResponse, CallToolResult, ContentBlock, ErrorData,
    Implementation, ListToolsResult, PaginatedRequestParams, ServerCapabilities, ServerInfo, Tool,
};
use rmcp::service::RequestContext;
use rmcp::{RoleServer, ServerHandler, ServiceExt};
use serde::Deserialize;
use serde_json::{Map, Value, json};

use crate::client::EngineClient;
use crate::tools::{
    DEFAULT_READ_TIMEOUT_MS, DEFAULT_WAIT_TIMEOUT_MS, MAX_OUTPUT_BYTES, WorkerTools,
};

/// The system prompt the server hands the agent at `initialize`.
pub const INSTRUCTIONS: &str = "\
Worker tools let you delegate work to CLI processes that outlive your turn. A worker \
is a real PTY owned by the comet engine: it keeps running after you answer, it shows up \
in the user's Terminal pane, and the user can watch or kill it by hand.

Use `spawn_worker` to start one, then `wait_worker` to block on it and collect its exit \
code — that is the point of these tools, and it is what your own Bash tool cannot do, \
because a Bash call dies with the turn. Use `read_worker` to check on a worker without \
committing your turn to it, and `kill_worker` to stop one.

Every read is bounded and resumable. Each call takes an optional `timeoutMs` and returns \
`nextSeq`; pass that back as `afterSeq` to continue exactly where the previous call \
stopped, including after a `wait_worker` that timed out. Output is a tail, not a \
transcript — if you need the whole thing, redirect to a file inside the command.

Pass `cwd` with a fresh worktree (create one with the worktree tools) when the worker \
writes code: two agents in one checkout is the failure that costs the most to unpick. \
Pass `targetDevice` only at spawn; every later call for that worker is routed to the \
same device automatically.";

// ---------------------------------------------------------------------------
// Tool parameters
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SpawnParams {
    command: String,
    #[serde(default)]
    cwd: Option<String>,
    #[serde(default)]
    target_device: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ReadParams {
    worker_id: String,
    #[serde(default)]
    after_seq: Option<u64>,
    #[serde(default)]
    timeout_ms: Option<u64>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct KillParams {
    worker_id: String,
}

/// One JSON Schema property.
fn prop(ty: &str, description: &str) -> Value {
    json!({ "type": ty, "description": description })
}

fn schema(properties: Vec<(&str, Value)>, required: &[&str]) -> Arc<Map<String, Value>> {
    let props: Map<String, Value> = properties
        .into_iter()
        .map(|(name, value)| (name.to_string(), value))
        .collect();
    let Value::Object(object) = json!({
        "type": "object",
        "properties": props,
        "required": required,
    }) else {
        unreachable!("json! of an object literal is an object")
    };
    Arc::new(object)
}

fn cursor_prop() -> Value {
    prop(
        "integer",
        "Resume cursor: the `nextSeq` a previous call returned. Everything up to and \
         including it is skipped, so no output is delivered twice and none is lost.",
    )
}

fn timeout_prop(default_ms: u64) -> Value {
    json!({
        "type": "integer",
        "description": format!(
            "Absolute bound on this call in milliseconds (default {default_ms}). The call \
             returns when it elapses even if the worker is still producing output; whatever \
             was read is returned with a `nextSeq` to resume from."
        ),
    })
}

/// The four tools, in the order they are useful.
pub fn tool_definitions() -> Vec<Tool> {
    vec![
        Tool::new(
            "spawn_worker",
            "Start a CLI worker in a PTY owned by the engine and return its `workerId`. \
             The worker outlives this turn and this tool server; it appears in the user's \
             Terminal pane. Nothing is read back here — use wait_worker or read_worker.",
            schema(
                vec![
                    (
                        "command",
                        prop(
                            "string",
                            "Shell command line to run. It is written into a login shell, so \
                             pipelines, redirection and `&&` all work. Redirect to a file when \
                             the worker is chatty and you need the whole log.",
                        ),
                    ),
                    (
                        "cwd",
                        prop(
                            "string",
                            "Directory to run in. Pass a dedicated worktree path when the \
                             worker writes code — two agents in one checkout is the failure \
                             this avoids. Defaults to the chat's own checkout.",
                        ),
                    ),
                    (
                        "targetDevice",
                        prop(
                            "string",
                            "Device to run the worker on. Choose it once, here: every later \
                             call for this worker is routed to the same device for you.",
                        ),
                    ),
                ],
                &["command"],
            ),
        ),
        Tool::new(
            "wait_worker",
            "Block until the worker exits, and return its exit code with everything it \
             printed. This is how you wait for a worker. If the timeout elapses first the \
             result says `running: true` and carries the output read so far plus a \
             `nextSeq` — resume with it, nothing is lost.",
            schema(
                vec![
                    ("workerId", prop("string", "The id spawn_worker returned.")),
                    ("afterSeq", cursor_prop()),
                    ("timeoutMs", timeout_prop(DEFAULT_WAIT_TIMEOUT_MS)),
                ],
                &["workerId"],
            ),
        ),
        Tool::new(
            "read_worker",
            format!(
                "Read a worker's output without waiting for it to finish. Returns the tail of \
                 what it printed after `afterSeq` (at most {} bytes, oldest dropped first), \
                 the `nextSeq` to resume from, and whether it is still running. A not-found \
                 means the worker was killed or aged past the engine's 30-minute \
                 exited-worker window — its output is gone.",
                MAX_OUTPUT_BYTES
            ),
            schema(
                vec![
                    ("workerId", prop("string", "The id spawn_worker returned.")),
                    ("afterSeq", cursor_prop()),
                    ("timeoutMs", timeout_prop(DEFAULT_READ_TIMEOUT_MS)),
                ],
                &["workerId"],
            ),
        ),
        Tool::new(
            "kill_worker",
            "Kill a worker and forget it. The engine drops the PTY and its output buffer \
             together, so no exit code and no output survive the call — read what you need \
             first.",
            schema(
                vec![("workerId", prop("string", "The id spawn_worker returned."))],
                &["workerId"],
            ),
        ),
    ]
}

/// The `rmcp` handler. Cheap to clone: everything is behind an `Arc`.
#[derive(Clone)]
pub struct WorkerToolsServer<C: EngineClient + 'static> {
    tools: Arc<WorkerTools<C>>,
}

impl<C: EngineClient + 'static> WorkerToolsServer<C> {
    pub fn new(tools: Arc<WorkerTools<C>>) -> Self {
        Self { tools }
    }

    /// Serve MCP on the process's stdio until the peer disconnects.
    ///
    /// Nothing else may write to stdout — it is the transport.
    pub async fn serve_stdio(self) -> Result<(), std::io::Error> {
        let service = self
            .serve(rmcp::transport::stdio())
            .await
            .map_err(|err| std::io::Error::other(format!("mcp handshake failed: {err}")))?;
        service
            .waiting()
            .await
            .map(|_| ())
            .map_err(|err| std::io::Error::other(format!("mcp server stopped: {err}")))
    }
}

fn parse<T: serde::de::DeserializeOwned>(
    arguments: Option<Map<String, Value>>,
) -> Result<T, CallToolResult> {
    serde_json::from_value(Value::Object(arguments.unwrap_or_default())).map_err(|err| {
        CallToolResult::error(vec![ContentBlock::text(format!(
            "invalid arguments: {err}"
        ))])
    })
}

/// Tool results ride `structuredContent` *and* a text mirror of the same JSON:
/// `CallToolResult::structured` fills both, so a client that reads only
/// `content` still sees the exit code.
fn ok<T: serde::Serialize>(value: T) -> CallToolResult {
    match serde_json::to_value(&value) {
        Ok(value) => CallToolResult::structured(value),
        // Unreachable for the tool results, all of which are plain structs.
        Err(err) => CallToolResult::error(vec![ContentBlock::text(format!(
            "could not encode the tool result: {err}"
        ))]),
    }
}

impl<C: EngineClient + 'static> ServerHandler for WorkerToolsServer<C> {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new("comet", env!("CARGO_PKG_VERSION")))
            .with_instructions(INSTRUCTIONS)
    }

    async fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, ErrorData> {
        Ok(ListToolsResult::with_all_items(tool_definitions()))
    }

    fn get_tool(&self, name: &str) -> Option<Tool> {
        tool_definitions()
            .into_iter()
            .find(|tool| tool.name == name)
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, ErrorData> {
        let name: Cow<'static, str> = request.name.clone();
        let arguments = request.arguments;
        let result = match name.as_ref() {
            "spawn_worker" => match parse::<SpawnParams>(arguments) {
                Ok(params) => self
                    .tools
                    .spawn(
                        &params.command,
                        params.cwd.as_deref(),
                        params.target_device.as_deref(),
                    )
                    .await
                    .map(ok),
                Err(result) => Ok(result),
            },
            "read_worker" => match parse::<ReadParams>(arguments) {
                Ok(params) => self
                    .tools
                    .read(&params.worker_id, params.after_seq, params.timeout_ms)
                    .await
                    .map(ok),
                Err(result) => Ok(result),
            },
            "wait_worker" => match parse::<ReadParams>(arguments) {
                Ok(params) => self
                    .tools
                    .wait(&params.worker_id, params.after_seq, params.timeout_ms)
                    .await
                    .map(ok),
                Err(result) => Ok(result),
            },
            "kill_worker" => match parse::<KillParams>(arguments) {
                Ok(params) => self.tools.kill(&params.worker_id).await.map(ok),
                Err(result) => Ok(result),
            },
            unknown => {
                return Err(ErrorData::invalid_params(
                    format!("unknown tool: {unknown}"),
                    None,
                ));
            }
        };
        // A failed tool is the agent's problem to route around, not a broken
        // server: it reads the message and decides what to do next.
        Ok(result
            .unwrap_or_else(|err| CallToolResult::error(vec![ContentBlock::text(err.to_string())]))
            .into())
    }
}
