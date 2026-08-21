use std::path::PathBuf;
use std::process::Stdio;

use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt as _, BufReader};
use tokio::process::{Child, Command};

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
    _incoming: tokio::sync::Mutex<tokio::sync::mpsc::Receiver<Incoming>>,
}

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
                    tracing::debug!(target: "zeron_harness::omp", "workers stderr: {line}");
                }
            });
        }
        let (client, incoming) = RpcClient::new(stdin, stdout);
        client
            .request(
                "initialize",
                json!({
                    "protocolVersion": "2024-11-05",
                    "capabilities": {},
                    "clientInfo": {
                        "name": "comet-omp",
                        "version": env!("CARGO_PKG_VERSION")
                    }
                }),
            )
            .await?;
        let tools = client.request("tools/list", json!({})).await?;
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
            _incoming: tokio::sync::Mutex::new(incoming),
        }))
    }

    pub fn definition(&self) -> &Value {
        &self.definition
    }

    pub async fn handle_call(&self, id: &str, tool_name: &str, arguments: Value) -> Value {
        if tool_name != "workers" {
            return error_result(id, "Unknown OMP host tool");
        }
        match self
            .client
            .request(
                "tools/call",
                json!({ "name": tool_name, "arguments": arguments }),
            )
            .await
        {
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
            Err(error) => error_result(id, &sanitize_diagnostic(&error.to_string())),
        }
    }

    pub async fn shutdown(&self) -> Result<(), HarnessError> {
        let mut child = self.child.lock().await;
        if child.try_wait()?.is_some() {
            return Ok(());
        }
        child.kill().await?;
        let _ = child.wait().await;
        Ok(())
    }
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
