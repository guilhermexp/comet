//! `comet-computer-use` stdio MCP: a proxy to the Codex Computer Use runtime
//! (`cua_repl`) that ChatGPT.app installs.
//!
//! Two tools, `computer_use` and `computer_use_reset`. The Settings switch
//! and scope are read on every request (see [`settings`]); a closed gate lists
//! no tools and refuses calls without touching the runtime.

pub mod runtime;
pub mod session;
pub mod settings;

use std::collections::VecDeque;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::PathBuf;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender, channel};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use serde_json::{Value, json};

use crate::session::SessionStore;
use crate::settings::Gate;

pub const COMPUTER_USE_MCP_ARG: &str = "__computer_use_mcp__";
/// Mounted by Worker launches, so the "Comet only" scope can refuse them.
pub const WORKER_FLAG: &str = "--worker";
pub const SERVER_NAME: &str = "comet-computer-use";
pub const TOOL: &str = "computer_use";
pub const RESET_TOOL: &str = "computer_use_reset";

const DEFAULT_TIMEOUT_MS: u64 = 30_000;
const MIN_TIMEOUT_MS: u64 = 1_000;
const MAX_TIMEOUT_MS: u64 = 300_000;
const RESET_TIMEOUT: Duration = Duration::from_secs(15);
const BOOT_TIMEOUT: Duration = Duration::from_secs(120);
const STDERR_TAIL: usize = 4_000;

/// Everything the server reads from its environment, explicit for tests.
pub struct Config {
    pub worker: bool,
    pub data_dir: Option<PathBuf>,
    pub plugin_dir: Option<PathBuf>,
    pub state_root: PathBuf,
    pub session_key: String,
    /// Slack over `timeout_ms` before the server gives up on the runtime.
    pub transport_margin: Duration,
}

impl Config {
    pub fn from_env(worker: bool) -> Self {
        Self {
            worker,
            data_dir: settings::data_dir(),
            plugin_dir: runtime::plugin_dir(),
            state_root: std::env::temp_dir().join("comet-computer-use"),
            session_key: session::session_key(),
            transport_margin: Duration::from_secs(5),
        }
    }
}

/// `zeron __computer_use_mcp__ [--worker]`.
pub fn run_stdio(args: &[String]) -> Result<(), String> {
    let worker = match args {
        [] => false,
        [flag] if flag == WORKER_FLAG => true,
        _ => {
            return Err(format!(
                "usage: zeron {COMPUTER_USE_MCP_ARG} [{WORKER_FLAG}]"
            ));
        }
    };
    let server = Server::new(Config::from_env(worker), std::io::stdout());
    server.events.spawn_client(std::io::stdin());
    server.run()
}

enum Event {
    Client(Value),
    ClientEof,
    Runtime { generation: u64, message: Value },
    RuntimeExit { generation: u64 },
}

/// One queue for the client and the runtime, so a call in flight still sees
/// the client's cancel.
#[derive(Clone)]
pub struct Events {
    tx: Sender<Event>,
}

impl Events {
    pub fn spawn_client<R: Read + Send + 'static>(&self, input: R) {
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            for line in BufReader::new(input).lines() {
                let Ok(line) = line else { break };
                if line.trim().is_empty() {
                    continue;
                }
                let message = serde_json::from_str(&line).unwrap_or(Value::Null);
                if tx.send(Event::Client(message)).is_err() {
                    return;
                }
            }
            let _ = tx.send(Event::ClientEof);
        });
    }
}

struct Runtime {
    child: Child,
    stdin: ChildStdin,
    generation: u64,
    stderr: Arc<Mutex<String>>,
    stderr_reader: std::thread::JoinHandle<()>,
}

enum CallError {
    Cancelled,
    Timeout,
    Failed(String),
}

pub struct Server<W: Write> {
    config: Config,
    out: W,
    pub events: Events,
    inbox: Receiver<Event>,
    deferred: VecDeque<Value>,
    runtime: Option<Runtime>,
    generation: u64,
    next_id: u64,
    last_exit: Option<String>,
    client_closed: bool,
    pending_notice: Option<String>,
    store: SessionStore,
}

impl<W: Write> Server<W> {
    pub fn new(config: Config, out: W) -> Self {
        let (tx, inbox) = channel();
        let store = SessionStore::new(&config.state_root, &config.session_key);
        Self {
            config,
            out,
            events: Events { tx },
            inbox,
            deferred: VecDeque::new(),
            runtime: None,
            generation: 0,
            next_id: 0,
            last_exit: None,
            client_closed: false,
            pending_notice: None,
            store,
        }
    }

    pub fn run(mut self) -> Result<(), String> {
        loop {
            let message = match self.deferred.pop_front() {
                Some(message) => message,
                None if self.client_closed => break,
                None => match self.inbox.recv() {
                    Ok(Event::Client(message)) => message,
                    Ok(Event::ClientEof) | Err(_) => break,
                    Ok(Event::RuntimeExit { generation }) => {
                        self.note_runtime_exit(generation);
                        continue;
                    }
                    Ok(Event::Runtime { .. }) => continue,
                },
            };
            if let Some(response) = self.handle(message) {
                self.send(&response)?;
            }
        }
        self.kill_runtime(None);
        Ok(())
    }

    fn send(&mut self, message: &Value) -> Result<(), String> {
        let mut line = serde_json::to_vec(message).map_err(|error| error.to_string())?;
        line.push(b'\n');
        self.out
            .write_all(&line)
            .and_then(|()| self.out.flush())
            .map_err(|error| error.to_string())
    }

    fn handle(&mut self, request: Value) -> Option<Value> {
        let id = request.get("id").cloned();
        let method = request.get("method").and_then(Value::as_str);
        if request.get("jsonrpc").and_then(Value::as_str) != Some("2.0") || method.is_none() {
            return Some(error_response(
                id.unwrap_or(Value::Null),
                -32600,
                "Invalid Request",
            ));
        }
        let method = method.expect("checked").to_owned();
        let id = id?;
        Some(match method.as_str() {
            "initialize" => result_response(
                id,
                json!({
                    "protocolVersion": request
                        .pointer("/params/protocolVersion")
                        .cloned()
                        .unwrap_or_else(|| json!("2024-11-05")),
                    "capabilities": { "tools": { "listChanged": false } },
                    "serverInfo": { "name": SERVER_NAME, "version": env!("CARGO_PKG_VERSION") },
                }),
            ),
            "ping" => result_response(id, json!({})),
            "tools/list" => result_response(id, json!({ "tools": self.list_tools() })),
            "tools/call" => {
                let result = self.call_tool(&id, &request);
                match result {
                    Some(result) => result_response(id, result),
                    None => error_response(id, -32800, "Request cancelled"),
                }
            }
            _ => error_response(id, -32601, "Method not found"),
        })
    }

    fn gate(&self) -> Gate {
        settings::gate(self.config.data_dir.as_deref(), self.config.worker)
    }

    fn list_tools(&self) -> Vec<Value> {
        if let Some(reason) = self.gate().refusal() {
            eprintln!("{SERVER_NAME}: no tools: {reason}");
            return Vec::new();
        }
        match self.resolve_runtime() {
            Ok(_) => tool_definitions(),
            Err(reason) => {
                eprintln!("{SERVER_NAME}: no tools: {reason}");
                Vec::new()
            }
        }
    }

    fn resolve_runtime(&self) -> Result<runtime::RuntimeCommand, String> {
        let dir = self
            .config
            .plugin_dir
            .as_deref()
            .ok_or("Computer Use runtime missing: no Codex home (HOME is unset)")?;
        runtime::resolve(dir)
    }

    /// `None` when the client cancelled the call.
    fn call_tool(&mut self, id: &Value, request: &Value) -> Option<Value> {
        let name = request
            .pointer("/params/name")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned();
        if name != TOOL && name != RESET_TOOL {
            return Some(tool_error(&format!(
                "Unknown tool. Use {TOOL} or {RESET_TOOL}."
            )));
        }
        if let Some(reason) = self.gate().refusal() {
            return Some(tool_error(reason));
        }
        let arguments = request
            .pointer("/params/arguments")
            .cloned()
            .unwrap_or_else(|| json!({}));
        if name == RESET_TOOL {
            return match self.runtime_call("js_reset", json!({}), RESET_TIMEOUT, id) {
                Ok(_) => Some(tool_text(
                    "Computer Use REPL reset; apps and windows stay open.",
                )),
                Err(CallError::Cancelled) => None,
                Err(CallError::Timeout) => Some(tool_error(&format!(
                    "{RESET_TOOL} timed out after {} s; the runtime was stopped and restarts on the next call",
                    RESET_TIMEOUT.as_secs()
                ))),
                Err(CallError::Failed(error)) => {
                    Some(tool_error(&format!("{RESET_TOOL} failed: {error}")))
                }
            };
        }
        let Some(code) = arguments.get("code").and_then(Value::as_str) else {
            return Some(tool_error("code (JavaScript string) is required"));
        };
        let timeout_ms = arguments
            .get("timeout_ms")
            .and_then(Value::as_u64)
            .unwrap_or(DEFAULT_TIMEOUT_MS)
            .clamp(MIN_TIMEOUT_MS, MAX_TIMEOUT_MS);
        let deadline = Duration::from_millis(timeout_ms) + self.config.transport_margin;
        let params = json!({ "code": code, "timeout_ms": timeout_ms });
        match self.runtime_call("js", params, deadline, id) {
            Ok(result) => {
                let notice = self.take_notice();
                Some(self.store.render(&result, notice))
            }
            Err(CallError::Cancelled) => None,
            Err(CallError::Timeout) => Some(tool_error(&format!(
                "{TOOL} timed out after timeout_ms={timeout_ms}; the runtime was stopped and restarts on the next call (bind apps and tabs again)"
            ))),
            Err(CallError::Failed(error)) => Some(tool_error(&format!("{TOOL} failed: {error}"))),
        }
    }

    /// Notice queued by a runtime start that replaced an earlier one.
    fn take_notice(&mut self) -> Option<String> {
        self.pending_notice.take()
    }

    fn runtime_call(
        &mut self,
        tool: &str,
        arguments: Value,
        deadline: Duration,
        client_id: &Value,
    ) -> Result<Value, CallError> {
        if self.runtime.is_none() {
            self.boot(client_id)?;
        }
        let result = self.request(
            "tools/call",
            json!({ "name": tool, "arguments": arguments }),
            deadline,
            Some(client_id),
        );
        if matches!(result, Err(CallError::Timeout)) {
            self.kill_runtime(Some(format!(
                "stopped after a call ran past its {} ms deadline",
                deadline.as_millis()
            )));
        }
        result
    }

    fn boot(&mut self, client_id: &Value) -> Result<(), CallError> {
        let command = self.resolve_runtime().map_err(CallError::Failed)?;
        let mut child = Command::new(&command.command)
            .args(&command.args)
            .envs(command.env.iter().map(|(name, value)| (name, value)))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|error| {
                CallError::Failed(format!(
                    "cannot start cua_repl {}: {error}",
                    command.command
                ))
            })?;
        self.generation += 1;
        let generation = self.generation;
        let stdin = child.stdin.take().expect("piped");
        let stdout = child.stdout.take().expect("piped");
        let stderr_pipe = child.stderr.take().expect("piped");
        let stderr = Arc::new(Mutex::new(String::new()));
        let stderr_reader = {
            let stderr = Arc::clone(&stderr);
            std::thread::spawn(move || {
                let mut reader = BufReader::new(stderr_pipe);
                let mut line = String::new();
                while reader.read_line(&mut line).is_ok_and(|read| read > 0) {
                    let mut tail = stderr.lock().unwrap_or_else(PoisonError::into_inner);
                    tail.push_str(&line);
                    if tail.len() > STDERR_TAIL {
                        let mut cut = tail.len() - STDERR_TAIL;
                        while !tail.is_char_boundary(cut) {
                            cut += 1;
                        }
                        tail.drain(..cut);
                    }
                    line.clear();
                }
            })
        };
        let tx = self.events.tx.clone();
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                let Ok(line) = line else { break };
                let Ok(message) = serde_json::from_str::<Value>(&line) else {
                    continue;
                };
                if tx
                    .send(Event::Runtime {
                        generation,
                        message,
                    })
                    .is_err()
                {
                    return;
                }
            }
            let _ = tx.send(Event::RuntimeExit { generation });
        });
        self.runtime = Some(Runtime {
            child,
            stdin,
            generation,
            stderr,
            stderr_reader,
        });
        let init = self.request(
            "initialize",
            json!({
                "protocolVersion": "2024-11-05",
                "capabilities": { "elicitation": { "form": {} } },
                "clientInfo": { "name": SERVER_NAME, "version": env!("CARGO_PKG_VERSION") },
            }),
            BOOT_TIMEOUT,
            Some(client_id),
        );
        if let Err(error) = init {
            self.kill_runtime(Some("its MCP handshake failed".into()));
            return Err(match error {
                CallError::Timeout => CallError::Failed(format!(
                    "cua_repl handshake timed out after {} s",
                    BOOT_TIMEOUT.as_secs()
                )),
                other => other,
            });
        }
        self.write_runtime(&json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }));
        if self.store.note_boot() {
            let why = self
                .last_exit
                .take()
                .unwrap_or_else(|| "the Computer Use server for this session restarted".into());
            self.pending_notice = Some(format!(
                "[Computer Use runtime restarted: {why}] REPL variables and app/tab handles from earlier calls are gone. Bind them again with cua.getApp/cua.getTab before acting."
            ));
        }
        Ok(())
    }

    fn write_runtime(&mut self, message: &Value) -> bool {
        let Some(runtime) = self.runtime.as_mut() else {
            return false;
        };
        let mut line = serde_json::to_vec(message).expect("serializable");
        line.push(b'\n');
        runtime
            .stdin
            .write_all(&line)
            .and_then(|()| runtime.stdin.flush())
            .is_ok()
    }

    /// One request to the runtime, waiting on the shared queue so a client
    /// cancel for `client_id` ends the wait. Other client messages wait in
    /// `deferred`; `ping` is answered right away.
    fn request(
        &mut self,
        method: &str,
        params: Value,
        deadline: Duration,
        client_id: Option<&Value>,
    ) -> Result<Value, CallError> {
        self.next_id += 1;
        let id = self.next_id;
        if !self.write_runtime(
            &json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params }),
        ) {
            return Err(CallError::Failed("cua_repl is not running".into()));
        }
        let until = Instant::now() + deadline;
        loop {
            let left = until.saturating_duration_since(Instant::now());
            let event = match self.inbox.recv_timeout(left) {
                Ok(event) => event,
                Err(RecvTimeoutError::Timeout) => return Err(CallError::Timeout),
                Err(RecvTimeoutError::Disconnected) => {
                    return Err(CallError::Failed("event queue closed".into()));
                }
            };
            let current = self.runtime.as_ref().map(|runtime| runtime.generation);
            match event {
                Event::Runtime {
                    generation,
                    message,
                } if Some(generation) == current => {
                    // The runtime confirms its policy through elicitation;
                    // accept on the session's behalf.
                    if message.get("method").and_then(Value::as_str) == Some("elicitation/create")
                        && let Some(request_id) = message.get("id").cloned()
                    {
                        self.write_runtime(&json!({
                            "jsonrpc": "2.0",
                            "id": request_id,
                            "result": {
                                "action": "accept",
                                "content": { "source": "computer-use-persisted-state", "persist": "always" },
                            },
                        }));
                        continue;
                    }
                    if message.get("id").and_then(Value::as_u64) != Some(id) {
                        continue;
                    }
                    if let Some(error) = message.get("error") {
                        let text = error
                            .get("message")
                            .and_then(Value::as_str)
                            .unwrap_or("error without a message");
                        return Err(CallError::Failed(text.to_owned()));
                    }
                    return Ok(message.get("result").cloned().unwrap_or(Value::Null));
                }
                Event::Runtime { .. } => {}
                Event::RuntimeExit { generation } => {
                    if Some(generation) == current {
                        let why = self.note_runtime_exit(generation);
                        return Err(CallError::Failed(format!("cua_repl exited ({why})")));
                    }
                }
                Event::Client(message) => {
                    let method = message.get("method").and_then(Value::as_str);
                    if method == Some("notifications/cancelled")
                        && client_id.is_some()
                        && message.pointer("/params/requestId") == client_id
                    {
                        self.write_runtime(&json!({
                            "jsonrpc": "2.0",
                            "method": "notifications/cancelled",
                            "params": { "requestId": id },
                        }));
                        return Err(CallError::Cancelled);
                    }
                    if method == Some("ping")
                        && let Some(ping_id) = message.get("id").cloned()
                    {
                        let _ = self.send(&result_response(ping_id, json!({})));
                        continue;
                    }
                    self.deferred.push_back(message);
                }
                Event::ClientEof => {
                    self.client_closed = true;
                    return Err(CallError::Cancelled);
                }
            }
        }
    }

    /// Record why the current runtime ended. Returns that reason.
    fn note_runtime_exit(&mut self, generation: u64) -> String {
        let Some(mut runtime) = self
            .runtime
            .take_if(|runtime| runtime.generation == generation)
        else {
            return "already replaced".into();
        };
        let status = runtime
            .child
            .wait()
            .map(|status| status.to_string())
            .unwrap_or_else(|error| error.to_string());
        // Give the stderr reader a moment to drain; a grandchild holding the
        // pipe open must not stall the server.
        let drained_by = Instant::now() + Duration::from_millis(200);
        while !runtime.stderr_reader.is_finished() && Instant::now() < drained_by {
            std::thread::sleep(Duration::from_millis(10));
        }
        let stderr = runtime
            .stderr
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .trim()
            .to_owned();
        let why = if stderr.is_empty() {
            format!("cua_repl ended: {status}")
        } else {
            format!("cua_repl ended: {status}; stderr: {stderr}")
        };
        self.last_exit = Some(why.clone());
        why
    }

    fn kill_runtime(&mut self, why: Option<String>) {
        if let Some(mut runtime) = self.runtime.take() {
            let _ = runtime.child.kill();
            let _ = runtime.child.wait();
            if why.is_some() {
                self.last_exit = why;
            }
        }
    }
}

fn result_response(id: Value, result: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "result": result })
}

fn error_response(id: Value, code: i64, message: &str) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } })
}

fn tool_text(text: &str) -> Value {
    json!({ "content": [{ "type": "text", "text": text }], "isError": false })
}

fn tool_error(text: &str) -> Value {
    json!({ "content": [{ "type": "text", "text": text }], "isError": true })
}

pub fn tool_definitions() -> Vec<Value> {
    vec![
        json!({
            "name": TOOL,
            "description": DESCRIPTION,
            "inputSchema": {
                "type": "object",
                "properties": {
                    "code": { "type": "string", "description": "JavaScript statements evaluated in the cua_repl runtime" },
                    "timeout_ms": {
                        "type": "integer",
                        "minimum": MIN_TIMEOUT_MS,
                        "maximum": MAX_TIMEOUT_MS,
                        "description": "Timeout in ms (default 30000)"
                    }
                },
                "required": ["code"],
                "additionalProperties": false
            }
        }),
        json!({
            "name": RESET_TOOL,
            "description": "Reset the persistent Computer Use REPL (clears variables and module cache) without closing apps or windows.",
            "inputSchema": { "type": "object", "properties": {}, "additionalProperties": false }
        }),
    ]
}

const DESCRIPTION: &str = r#"Run JavaScript in the Codex Computer Use runtime (cua_repl) to drive native macOS apps. REPL state (variables, app/tab handles) persists between calls while the runtime lives; if it restarted, the first line of the result says so, and you must bind handles again before acting.

macOS only. Requires ChatGPT.app and Accessibility permission for "Codex Computer Use.app".

Runtime API:
- await cua.getState() — running apps (displayName + bundle id)
- await cua.listApps() — installed apps
- let app = await cua.getApp("Name" | "bundle.id") — app handle plus its initial tree
- await app.getAXState({ disableDiffing: true }) — accessibility tree with an index and "ID:" per node
- await app.getScreenshot() — image of the app's window only
- await app.getAXStateAndScreenshot(opts) — tree and screenshot in one call
- await app.click(index | [x, y]) — prefer the node index
- await app.paste("text", { format }) — inserts the EXACT text
- await app.typeText("text") — key by key, subject to the field's autocorrect
- await app.setValue(index, "value") — only on "(settable)" nodes; may silently do nothing, so read the value back
- await app.selectText(index, "text", { prefix, suffix })
- await app.pressKey("Return" | "super+n" | "super+w" | "Tab")
- await app.scroll(index | [x, y], "up" | "down", pages)
- await app.drag([x1, y1], [x2, y2])
- await app.performSecondaryAction(index, "Raise" | "Collapse" | ...)
- nodeRepl.write(text) — add text to the tool result
- await nodeRepl.emitImage(bytes) — add an image to the tool result

Reading the tree, screenshots, clicks, scrolls, drags, key presses and typing do not activate the target app or move the user's pointer. Opening a window or launching a closed app does take focus. The runtime only sees the target app's front window: raise the right window first with performSecondaryAction(index, "Raise").

Rules that avoid errors:
- Node indexes expire after every action: call getAXState and act in the SAME call; never reuse an old index.
- Match nodes by "ID:" (e.g. DontSaveButton), not by label; macOS may run in another language.
- Prefer click(index) to coordinates; coordinates depend on the latest screenshot.
- For exact text use paste(), or setValue() on a "(settable)" node, and read the value back.
- A modal panel swallows super+q: cancel it (Escape or its CancelButton) before quitting the app.
- An iPad/iPhone app running on the Mac cannot be bound by name, bundle id or /Applications path. Get the running bundle path in a shell with `lsappinfo info -only bundlepath <bundle.id>` (the "bundle path=" line) and pass that path to cua.getApp; it changes every time the app reopens.
- Concurrency: two agents must not drive the same app at the same time, and must not call paste in parallel (it goes through the shared clipboard).
- The app reads the user's real data; never turn real data into a fixture. Close or quit only what this session opened."#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn description_carries_the_known_pitfalls() {
        let tools = tool_definitions();
        let description = tools[0]["description"].as_str().unwrap();
        assert!(description.contains("lsappinfo info -only bundlepath"));
        assert!(description.contains("must not call paste in parallel"));
        assert_eq!(tools[1]["name"], RESET_TOOL);
    }

    #[test]
    fn args_accept_only_the_worker_flag() {
        assert!(run_stdio(&["--other".into()]).is_err());
    }
}
