use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt as _, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::oneshot;
use tokio_util::sync::CancellationToken;

use super::protocol::sanitize_diagnostic;
use crate::HarnessError;
use crate::acp::workers_mcp_servers_for;
use crate::jsonrpc::{Incoming, RpcClient};

pub struct WorkersBridgeOptions {
    pub enabled: bool,
    pub executable: PathBuf,
    pub parent_chat_id: Option<String>,
}

pub struct WorkersBridge {
    client: RpcClient,
    child: tokio::sync::Mutex<Child>,
    definition: Value,
    pending: Arc<Mutex<HashMap<String, Arc<CancellationToken>>>>,
    request_timeout: Duration,
}

const MAX_PENDING_CALLS: usize = 64;
const STARTUP_TIMEOUT: Duration = Duration::from_secs(10);
// 900s deliberately keeps transport from timing out a healthy wait; its 780s slack exceeds the 60s IPC and scheduling margin floor.
pub const TOOL_CALL_TIMEOUT: Duration = Duration::from_secs(15 * 60);

impl WorkersBridge {
    pub async fn start(options: WorkersBridgeOptions) -> Result<Option<Self>, HarnessError> {
        if !options.enabled {
            return Ok(None);
        }
        let rows = workers_mcp_servers_for(
            &options.executable,
            true,
            false,
            options.parent_chat_id.as_deref(),
        );
        let server = rows.first().ok_or_else(|| {
            HarnessError::Protocol("Workers controller sidecar is unavailable".into())
        })?;
        let executable = server
            .get("command")
            .and_then(Value::as_str)
            .ok_or_else(|| HarnessError::Protocol("Workers sidecar has no command".into()))?;
        let mut command = Command::new(executable);
        if let Some(args) = server.get("args").and_then(Value::as_array) {
            command.args(args.iter().filter_map(Value::as_str));
        }
        if let Some(environment) = server.get("env").and_then(Value::as_array) {
            for row in environment {
                if let (Some(name), Some(value)) = (
                    row.get("name").and_then(Value::as_str),
                    row.get("value").and_then(Value::as_str),
                ) {
                    command.env(name, value);
                }
            }
        }
        command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        let mut child = command.spawn().map_err(|error| {
            if error.kind() == std::io::ErrorKind::NotFound {
                HarnessError::NotInstalled(executable.to_owned())
            } else {
                HarnessError::Io(error)
            }
        })?;
        let stdin = child.stdin.take().ok_or_else(|| {
            HarnessError::Protocol("Workers controller sidecar has no stdin".into())
        })?;
        let stdout = child.stdout.take().ok_or_else(|| {
            HarnessError::Protocol("Workers controller sidecar has no stdout".into())
        })?;
        if let Some(stderr) = child.stderr.take() {
            tokio::spawn(async move {
                let mut lines = BufReader::new(stderr).lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    let diagnostic = sanitize_diagnostic(&line);
                    if !diagnostic.is_empty() {
                        tracing::debug!(target: "zeron_harness::omp", "workers stderr: {diagnostic}");
                    }
                }
            });
        }
        let (client, mut incoming) = RpcClient::new(stdin, stdout);
        let incoming_client = client.clone();
        tokio::spawn(async move {
            while let Some(frame) = incoming.recv().await {
                match frame {
                    Incoming::Request { id, method, .. } => incoming_client.respond_error(
                        &id,
                        -32601,
                        &format!("Unsupported Workers controller request: {method}"),
                    ),
                    Incoming::Notification { .. } => {}
                    Incoming::Eof => break,
                }
            }
        });
        tokio::time::timeout(
            STARTUP_TIMEOUT,
            client.request(
                "initialize",
                json!({
                    "protocolVersion": "2024-11-05",
                    "capabilities": {},
                    "clientInfo": {
                        "name": "comet-omp",
                        "version": env!("CARGO_PKG_VERSION")
                    }
                }),
            ),
        )
        .await
        .map_err(|_| HarnessError::Protocol("Workers controller initialize timed out".into()))??;
        let tools = tokio::time::timeout(STARTUP_TIMEOUT, client.request("tools/list", json!({})))
            .await
            .map_err(|_| {
                HarnessError::Protocol("Workers controller tools/list timed out".into())
            })??;
        let tool = tools
            .get("tools")
            .and_then(Value::as_array)
            .and_then(|tools| tools.first())
            .ok_or_else(|| {
                HarnessError::Protocol("Workers controller advertised no tool".into())
            })?;
        let name = tool
            .get("name")
            .and_then(Value::as_str)
            .filter(|name| *name == "workers")
            .ok_or_else(|| {
                HarnessError::Protocol("Workers controller advertised an unexpected tool".into())
            })?;
        let definition = json!({
            "name": name,
            "loadMode": "essential",
            "description": tool
                .get("description")
                .and_then(Value::as_str)
                .unwrap_or("Coordinate Comet Workers"),
            "parameters": tool
                .get("inputSchema")
                .cloned()
                .unwrap_or_else(|| json!({ "type": "object" })),
        });
        Ok(Some(Self {
            client,
            child: tokio::sync::Mutex::new(child),
            definition,
            pending: Arc::new(Mutex::new(HashMap::new())),
            request_timeout: TOOL_CALL_TIMEOUT,
        }))
    }

    pub fn definition(&self) -> &Value {
        &self.definition
    }

    pub async fn handle_call(&self, id: &str, tool_name: &str, arguments: Value) -> Value {
        match self.begin_call(id, tool_name, arguments) {
            Ok(receiver) => receiver
                .await
                .unwrap_or_else(|_| error_result(id, "OMP host tool was cancelled")),
            Err(result) => result,
        }
    }

    pub fn begin_call(
        &self,
        id: &str,
        tool_name: &str,
        arguments: Value,
    ) -> Result<oneshot::Receiver<Value>, Value> {
        if id.is_empty() || id.len() > 256 {
            return Err(error_result(id, "OMP host tool request has an invalid id"));
        }
        if tool_name != "workers" {
            return Err(error_result(id, "Unknown OMP host tool"));
        }
        if !arguments.is_object() {
            return Err(error_result(id, "Invalid Workers tool arguments"));
        }

        let cancellation = Arc::new(CancellationToken::new());
        {
            let mut pending = lock(&self.pending);
            if let Some(previous) = pending.remove(id) {
                previous.cancel();
                return Err(error_result(id, "Duplicate OMP host-tool request id"));
            }
            if pending.len() >= MAX_PENDING_CALLS {
                return Err(error_result(
                    id,
                    "OMP host-tool pending-call limit exceeded",
                ));
            }
            pending.insert(id.to_owned(), Arc::clone(&cancellation));
        }

        let client = self.client.clone();
        let pending = Arc::clone(&self.pending);
        let request_timeout = self.request_timeout;
        let id = id.to_owned();
        let tool_name = tool_name.to_owned();
        let (resolved, receiver) = oneshot::channel();
        tokio::spawn(async move {
            let outcome = client
                .request_bounded(
                    "tools/call",
                    json!({ "name": tool_name, "arguments": arguments }),
                    cancellation.as_ref().clone(),
                    request_timeout,
                )
                .await;
            let outcome = Some(match outcome {
                Ok(result) => {
                    let content = result.get("content").cloned().unwrap_or_else(|| json!([]));
                    let is_error = result.get("isError").and_then(Value::as_bool) == Some(true);
                    json!({
                        "type": "host_tool_result",
                        "id": id,
                        "result": { "content": content },
                        "isError": is_error,
                    })
                }
                Err(error) => error_result(&id, &sanitize_diagnostic(&error.to_string())),
            });
            let active = {
                let mut pending = lock(&pending);
                let active = pending
                    .get(&id)
                    .is_some_and(|current| Arc::ptr_eq(current, &cancellation));
                if active {
                    pending.remove(&id);
                }
                active
            };
            if active && let Some(result) = outcome {
                let _ = resolved.send(result);
            }
        });
        Ok(receiver)
    }

    pub fn cancel_call(&self, id: &str) -> bool {
        let cancellation = lock(&self.pending).remove(id);
        if let Some(cancellation) = cancellation {
            cancellation.cancel();
            true
        } else {
            false
        }
    }

    pub async fn shutdown(&self) -> Result<(), HarnessError> {
        for (_, cancellation) in lock(&self.pending).drain() {
            cancellation.cancel();
        }
        let mut child = self.child.lock().await;
        if child.try_wait()?.is_some() {
            return Ok(());
        }
        child.kill().await?;
        let _ = child.wait().await;
        Ok(())
    }
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

fn error_result(id: &str, message: &str) -> Value {
    json!({
        "type": "host_tool_result",
        "id": id,
        "result": {
            "content": [{ "type": "text", "text": message }]
        },
        "isError": true,
    })
}
