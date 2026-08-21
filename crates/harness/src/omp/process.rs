use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use serde_json::Value;
use tokio::io::{AsyncBufRead, AsyncBufReadExt as _, AsyncWriteExt as _, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::{mpsc, oneshot};

use super::protocol::{
    MAX_INBOUND_BYTES, MAX_PENDING_REQUESTS, parse_frame, sanitize_diagnostic, serialize_frame,
};
use crate::HarnessError;

pub struct OmpLaunch {
    pub executable: PathBuf,
    pub cwd: PathBuf,
    pub ephemeral: bool,
    pub env: Option<HashMap<String, String>>,
    pub handshake_timeout: Duration,
    pub request_timeout: Duration,
}

enum WriteMessage {
    Frame(String),
    Close,
}

struct Pending {
    command: String,
    resolve: oneshot::Sender<Result<Value, String>>,
}

struct Inner {
    writer: mpsc::UnboundedSender<WriteMessage>,
    pending: Mutex<HashMap<String, Pending>>,
    sequence: AtomicU64,
    request_timeout: Duration,
    fatal: Mutex<Option<String>>,
    closed: AtomicBool,
}

pub struct OmpProcess {
    inner: Arc<Inner>,
    child: Arc<tokio::sync::Mutex<Child>>,
    events: Arc<Mutex<Option<mpsc::Receiver<Value>>>>,
}

impl Clone for OmpProcess {
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
            child: Arc::clone(&self.child),
            events: Arc::clone(&self.events),
        }
    }
}

impl std::fmt::Debug for OmpProcess {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.debug_struct("OmpProcess").finish_non_exhaustive()
    }
}

impl OmpProcess {
    pub async fn start(launch: OmpLaunch) -> Result<Self, HarnessError> {
        let mut command = Command::new(&launch.executable);
        command
            .args([
                "--mode",
                "rpc-ui",
                "--auto-approve",
                "--no-extensions",
                "--allow-home",
            ])
            .arg("--cwd")
            .arg(&launch.cwd);
        if launch.ephemeral {
            command.arg("--no-session");
        }
        if let Some(environment) = &launch.env {
            command.envs(environment);
        } else {
            crate::compose_child_path(&mut command, &launch.executable);
        }
        command
            .current_dir(&launch.cwd)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        let mut child = command.spawn().map_err(|error| {
            if error.kind() == std::io::ErrorKind::NotFound {
                HarnessError::NotInstalled(launch.executable.display().to_string())
            } else {
                HarnessError::Io(error)
            }
        })?;
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| HarnessError::Protocol("OMP RPC child has no stdin".into()))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| HarnessError::Protocol("OMP RPC child has no stdout".into()))?;
        let stderr = child.stderr.take();

        let (write_tx, mut write_rx) = mpsc::unbounded_channel::<WriteMessage>();
        tokio::spawn(async move {
            let mut stdin = stdin;
            while let Some(message) = write_rx.recv().await {
                match message {
                    WriteMessage::Frame(frame) => {
                        if stdin.write_all(frame.as_bytes()).await.is_err()
                            || stdin.write_all(b"\n").await.is_err()
                            || stdin.flush().await.is_err()
                        {
                            break;
                        }
                    }
                    WriteMessage::Close => {
                        let _ = stdin.shutdown().await;
                        break;
                    }
                }
            }
        });

        if let Some(stderr) = stderr {
            tokio::spawn(async move {
                let mut reader = BufReader::new(stderr);
                loop {
                    match read_bounded_line(&mut reader, 64 * 1024).await {
                        Ok(Some(line)) => {
                            let diagnostic = sanitize_diagnostic(&String::from_utf8_lossy(&line));
                            if !diagnostic.is_empty() {
                                tracing::debug!(target: "zeron_harness::omp", "stderr: {diagnostic}");
                            }
                        }
                        Ok(None) => break,
                        Err(error) => {
                            tracing::debug!(target: "zeron_harness::omp", "stderr reader stopped: {}", sanitize_diagnostic(&error));
                            break;
                        }
                    }
                }
            });
        }

        let inner = Arc::new(Inner {
            writer: write_tx,
            pending: Mutex::new(HashMap::new()),
            sequence: AtomicU64::new(0),
            request_timeout: launch.request_timeout,
            fatal: Mutex::new(None),
            closed: AtomicBool::new(false),
        });
        let (event_tx, event_rx) = mpsc::channel(256);
        let (ready_tx, ready_rx) = oneshot::channel::<Result<(), String>>();
        let reader_inner = Arc::clone(&inner);
        tokio::spawn(read_stdout(stdout, reader_inner, event_tx, ready_tx));

        let process = Self {
            inner,
            child: Arc::new(tokio::sync::Mutex::new(child)),
            events: Arc::new(Mutex::new(Some(event_rx))),
        };
        match tokio::time::timeout(launch.handshake_timeout, ready_rx).await {
            Ok(Ok(Ok(()))) => Ok(process),
            Ok(Ok(Err(message))) => {
                let _ = process.shutdown().await;
                Err(HarnessError::Protocol(message))
            }
            Ok(Err(_)) => {
                let _ = process.shutdown().await;
                Err(HarnessError::Protocol("OMP RPC exited before ready".into()))
            }
            Err(_) => {
                let _ = process.shutdown().await;
                Err(HarnessError::Protocol("OMP RPC handshake timed out".into()))
            }
        }
    }

    pub async fn request(&self, command: Value) -> Result<Value, HarnessError> {
        if self.inner.closed.load(Ordering::SeqCst) {
            return Err(HarnessError::Protocol("OMP RPC process is closed".into()));
        }
        if let Some(message) = lock(&self.inner.fatal).clone() {
            return Err(HarnessError::Protocol(message));
        }
        let command_name = command
            .get("type")
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| HarnessError::Protocol("OMP RPC command has no type".into()))?
            .to_owned();
        let id = format!(
            "comet-{}",
            self.inner.sequence.fetch_add(1, Ordering::Relaxed) + 1
        );
        let mut frame = command;
        frame
            .as_object_mut()
            .expect("command type check requires an object")
            .insert("id".into(), Value::String(id.clone()));
        let serialized = serialize_frame(&frame)?;
        let (tx, rx) = oneshot::channel();
        {
            let mut pending = lock(&self.inner.pending);
            if pending.len() >= MAX_PENDING_REQUESTS {
                return Err(HarnessError::Protocol(
                    "OMP RPC pending request limit exceeded".into(),
                ));
            }
            pending.insert(
                id.clone(),
                Pending {
                    command: command_name.clone(),
                    resolve: tx,
                },
            );
        }
        if self
            .inner
            .writer
            .send(WriteMessage::Frame(serialized))
            .is_err()
        {
            lock(&self.inner.pending).remove(&id);
            return Err(HarnessError::Protocol("OMP RPC stdin is closed".into()));
        }
        match tokio::time::timeout(self.inner.request_timeout, rx).await {
            Ok(Ok(Ok(value))) => Ok(value),
            Ok(Ok(Err(message))) => Err(HarnessError::Protocol(message)),
            Ok(Err(_)) => Err(HarnessError::Protocol(format!(
                "OMP RPC {command_name} response channel closed"
            ))),
            Err(_) => {
                lock(&self.inner.pending).remove(&id);
                Err(HarnessError::Protocol(format!(
                    "OMP RPC {command_name} request timed out"
                )))
            }
        }
    }

    pub fn send_control(&self, frame: Value) -> Result<(), HarnessError> {
        let serialized = serialize_frame(&frame)?;
        self.inner
            .writer
            .send(WriteMessage::Frame(serialized))
            .map_err(|_| HarnessError::Protocol("OMP RPC stdin is closed".into()))
    }

    pub fn take_events(&self) -> Result<mpsc::Receiver<Value>, HarnessError> {
        lock(&self.events)
            .take()
            .ok_or_else(|| HarnessError::Protocol("OMP RPC events already taken".into()))
    }

    pub async fn shutdown(&self) -> Result<(), HarnessError> {
        if self.inner.closed.swap(true, Ordering::SeqCst) {
            return Ok(());
        }
        let _ = self.inner.writer.send(WriteMessage::Close);
        fail_pending(&self.inner, "OMP RPC process closed");
        let mut child = self.child.lock().await;
        if child.try_wait()?.is_some() {
            return Ok(());
        }
        if tokio::time::timeout(Duration::from_secs(1), child.wait())
            .await
            .is_err()
        {
            child.kill().await?;
            let _ = child.wait().await;
        }
        Ok(())
    }
}

async fn read_stdout(
    stdout: tokio::process::ChildStdout,
    inner: Arc<Inner>,
    event_tx: mpsc::Sender<Value>,
    ready_tx: oneshot::Sender<Result<(), String>>,
) {
    let mut ready_tx = Some(ready_tx);
    let mut reader = BufReader::new(stdout);
    loop {
        let line = match read_bounded_line(&mut reader, MAX_INBOUND_BYTES).await {
            Ok(Some(line)) => line,
            Ok(None) => break,
            Err(message) => {
                fail(&inner, message.clone());
                if let Some(ready) = ready_tx.take() {
                    let _ = ready.send(Err(message));
                }
                return;
            }
        };
        let line = match std::str::from_utf8(&line) {
            Ok(line) => line,
            Err(_) => {
                let message = "OMP RPC emitted non-UTF-8 JSONL".to_owned();
                fail(&inner, message.clone());
                if let Some(ready) = ready_tx.take() {
                    let _ = ready.send(Err(message));
                }
                return;
            }
        };
        let frame = match parse_frame(line.trim()) {
            Ok(frame) => frame,
            Err(error) => {
                fail(&inner, error.to_string());
                if let Some(ready) = ready_tx.take() {
                    let _ = ready.send(Err(error.to_string()));
                }
                return;
            }
        };
        match frame.get("type").and_then(Value::as_str) {
            Some("ready") => {
                if let Some(ready) = ready_tx.take() {
                    let _ = ready.send(Ok(()));
                }
            }
            Some("response") => route_response(&inner, frame),
            _ => {
                if event_tx.send(frame).await.is_err() {
                    return;
                }
            }
        }
    }
    let message = if ready_tx.is_some() {
        "OMP RPC exited before ready"
    } else {
        "OMP RPC stdout closed"
    };
    fail(&inner, message.into());
    if let Some(ready) = ready_tx.take() {
        let _ = ready.send(Err(message.into()));
    }
}

async fn read_bounded_line<R>(reader: &mut R, max_bytes: usize) -> Result<Option<Vec<u8>>, String>
where
    R: AsyncBufRead + Unpin,
{
    let mut line = Vec::new();
    loop {
        let (consumed, complete) = {
            let available = reader
                .fill_buf()
                .await
                .map_err(|error| format!("OMP RPC read failed: {error}"))?;
            if available.is_empty() {
                if line.is_empty() {
                    return Ok(None);
                }
                return Ok(Some(line));
            }
            let newline = available.iter().position(|byte| *byte == b'\n');
            let consumed = newline.map_or(available.len(), |position| position + 1);
            let payload_bytes = consumed.saturating_sub(usize::from(newline.is_some()));
            if line.len().saturating_add(payload_bytes) > max_bytes {
                return Err(format!("OMP RPC frame exceeded {max_bytes} bytes"));
            }
            line.extend_from_slice(&available[..consumed]);
            (consumed, newline.is_some())
        };
        reader.consume(consumed);
        if complete {
            if line.last() == Some(&b'\n') {
                line.pop();
            }
            if line.last() == Some(&b'\r') {
                line.pop();
            }
            return Ok(Some(line));
        }
    }
}

fn route_response(inner: &Inner, frame: Value) {
    let Some(id) = frame.get("id").and_then(Value::as_str) else {
        return;
    };
    let Some(pending) = lock(&inner.pending).remove(id) else {
        return;
    };
    let command = frame.get("command").and_then(Value::as_str).unwrap_or("");
    if command != pending.command {
        let _ = pending
            .resolve
            .send(Err("OMP RPC response command mismatch".into()));
        return;
    }
    if frame.get("success").and_then(Value::as_bool) != Some(true) {
        let message = sanitize_diagnostic(
            frame
                .get("error")
                .and_then(Value::as_str)
                .unwrap_or("OMP RPC request failed"),
        );
        let _ = pending.resolve.send(Err(message));
        return;
    }
    let _ = pending
        .resolve
        .send(Ok(frame.get("data").cloned().unwrap_or(Value::Null)));
}

fn fail(inner: &Inner, message: String) {
    *lock(&inner.fatal) = Some(message.clone());
    fail_pending(inner, &message);
}

fn fail_pending(inner: &Inner, message: &str) {
    for (_, pending) in lock(&inner.pending).drain() {
        let _ = pending.resolve.send(Err(message.to_owned()));
    }
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}
