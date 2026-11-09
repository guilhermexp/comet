use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU8, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use crate::process::{Child, ChildStdout, Command, Stdio};
use serde_json::{Value, json};
use tokio::io::{AsyncBufRead, AsyncBufReadExt as _, AsyncWriteExt as _, BufReader};
use tokio::sync::{mpsc, oneshot, watch};

use super::protocol::{
    ChunkAssembler, MAX_INBOUND_BYTES, MAX_PENDING_REQUESTS, parse_frame, sanitize_diagnostic,
    serialize_frame,
};
use crate::HarnessError;

pub struct OmpLaunch {
    pub executable: PathBuf,
    pub cwd: PathBuf,
    pub ephemeral: bool,
    pub system_prompt_append: Option<String>,
    pub env: Option<HashMap<String, String>>,
    pub handshake_timeout: Duration,
    pub request_timeout: Duration,
}

enum WriteMessage {
    Frame(String),
    Close,
}

/// Stderr lines retained for the next transport failure. A dying OMP explains
/// itself on stderr; without this the user only ever saw "OMP RPC exited before
/// ready" while the child's own reason went to a debug log nobody opens.
const STDERR_TAIL_LINES: usize = 4;

/// How long a closed stdout waits for the stderr reader to drain. The child
/// closes both pipes as it exits, but the two readers race: without this wait
/// the stdout EOF could compose the failure before the reason reached the tail.
const STDERR_DRAIN_TIMEOUT: Duration = Duration::from_secs(1);

/// Config overlay that keeps USER-level skill roots out of every OMP run:
/// `~/.claude/skills`, `~/.agents/skills`, `~/.codex/skills`, `~/.pi/skills`.
/// A chat should see the skills of the repo it is standing in, not the personal
/// catalog — and because those roots are mirrors of one workspace's own skills,
/// they leaked that workspace's vocabulary into every other project (measured:
/// 98 of 200 available commands in an unrelated repo). Project-level roots stay
/// on, so a repo that ships skills keeps them.
///
/// Keys MUST be nested. OMP reads an overlay as `config.yml`-shaped settings and
/// silently ignores a dotted `skills.enableClaudeUser` key.
const SKILL_SCOPE_OVERLAY: &str = "skills:\n  enableClaudeUser: false\n  enableAgentsUser: false\n  enableCodexUser: false\n  enablePiUser: false\n";

/// Path to the materialized [`SKILL_SCOPE_OVERLAY`], written on first need.
///
/// The content is constant, so an existing match is reused as-is. Otherwise the
/// write goes through a staging file unique per call — pid AND a counter,
/// because two launches in the SAME process would otherwise stage onto one path
/// and the loser's rename fails with `ENOENT` — then renames into place, so a
/// concurrent reader sees the whole old file or the whole new one, never a torn
/// one.
fn skill_scope_overlay() -> std::io::Result<PathBuf> {
    static STAGING: AtomicU64 = AtomicU64::new(0);
    let path = std::env::temp_dir().join("zeron-omp-skill-scope.yml");
    if std::fs::read_to_string(&path).is_ok_and(|body| body == SKILL_SCOPE_OVERLAY) {
        return Ok(path);
    }
    let staging = std::env::temp_dir().join(format!(
        "zeron-omp-skill-scope.{}.{}.yml",
        std::process::id(),
        STAGING.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::write(&staging, SKILL_SCOPE_OVERLAY)?;
    std::fs::rename(&staging, &path)?;
    Ok(path)
}

/// Bounded event capacity shared by pre-ready, negotiation, and live startup.
/// Keeping one queue avoids a second copy/buffer during the transition.
const EVENT_BUFFER_CAPACITY: usize = 1024;

/// Per-request admission, available before send. A native rejection and a
/// failure before dispatch are distinct from losing the response after send.
#[derive(Clone, Default)]
// 0: not sent; 1: registered for send; 2: explicitly rejected by the peer.
pub(super) struct PromptAdmission(Arc<AtomicU8>);
impl PromptAdmission {
    pub(super) fn awaits_result(&self) -> bool {
        self.0.load(Ordering::Acquire) == 1
    }
}

struct Pending {
    command: String,
    resolve: oneshot::Sender<Result<Value, String>>,
    admission: Option<PromptAdmission>,
}

struct Inner {
    writer: mpsc::UnboundedSender<WriteMessage>,
    pending: Mutex<HashMap<String, Pending>>,
    sequence: AtomicU64,
    request_timeout: Duration,
    fatal: Mutex<Option<String>>,
    closed: AtomicBool,
    /// Most recent child stderr lines, oldest first (see [`STDERR_TAIL_LINES`]).
    stderr_tail: Arc<Mutex<VecDeque<String>>>,
    /// Frames de stdout vistos antes do `ready`. Zero num timeout significa
    /// que o processo nunca falou — outra investigacao que "falou e demorou".
    frames_before_ready: AtomicU64,
    /// True once the public event receiver is taken; protects the switch from
    /// startup buffering to the existing live-channel backpressure behavior.
    events_taken: Mutex<bool>,
    /// Includes raw frames retained by interactive startup, not just channel slots.
    startup_retained: AtomicU64,
    startup_input: watch::Sender<bool>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct OmpCapabilities {
    pub chunked_frames: bool,
}

pub struct OmpProcess {
    inner: Arc<Inner>,
    child: Arc<tokio::sync::Mutex<Child>>,
    events: Arc<Mutex<Option<mpsc::Receiver<Value>>>>,
    capabilities: Arc<Mutex<OmpCapabilities>>,
    modern_lifecycle: bool,
}

/// A spawned child is observable before readiness and protocol negotiation.
pub struct OmpStartup {
    ready: oneshot::Receiver<Result<OmpCapabilities, String>>,
    timeout: Duration,
    executable: PathBuf,
    cwd: PathBuf,
}

impl Clone for OmpProcess {
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
            child: Arc::clone(&self.child),
            events: Arc::clone(&self.events),
            capabilities: Arc::clone(&self.capabilities),
            modern_lifecycle: self.modern_lifecycle,
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
        let (process, startup) = Self::spawn(launch).await?;
        process.initialize(startup).await?;
        Ok(process)
    }

    pub(super) async fn start_discovery(launch: OmpLaunch) -> Result<Self, HarnessError> {
        let (process, startup) = Self::spawn_with_policy(launch, true).await?;
        process.initialize(startup).await?;
        Ok(process)
    }

    pub(super) async fn spawn(launch: OmpLaunch) -> Result<(Self, OmpStartup), HarnessError> {
        Self::spawn_with_policy(launch, false).await
    }

    async fn spawn_with_policy(
        launch: OmpLaunch,
        no_ui: bool,
    ) -> Result<(Self, OmpStartup), HarnessError> {
        let mut command = Command::new(&launch.executable);
        command
            .args(["--mode", "rpc-ui", "--auto-approve", "--allow-home"])
            .arg("--cwd")
            .arg(&launch.cwd);
        // Losing skill scoping must never cost the turn: without the overlay the
        // run proceeds with the personal catalog loaded.
        match skill_scope_overlay() {
            Ok(overlay) => {
                command.arg("--config").arg(overlay);
            }
            Err(error) => {
                tracing::warn!(
                    target: "zeron_harness::omp",
                    error = %error,
                    "skill scope overlay unavailable; user-level skills stay loaded"
                );
            }
        }
        if let Some(system_prompt_append) = launch.system_prompt_append.as_deref() {
            command
                .arg("--append-system-prompt")
                .arg(system_prompt_append);
        }
        if no_ui {
            command.arg("--no-ui");
        }
        if launch.ephemeral {
            command.arg("--no-session");
        }
        if let Some(environment) = &launch.env {
            for (key, value) in environment {
                command.env(key, value);
            }
        } else {
            crate::compose_child_path(&mut command, &launch.executable);
        }
        command
            .current_dir(&launch.cwd)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        // Probe the same wrapper under the actual child environment (including
        // composed PATH) and cwd. A source wrapper can choose a different build
        // from these inputs even when its own file metadata is unchanged.
        let executable = launch.executable.clone();
        let cwd = launch.cwd.clone();
        // Use std's native environment-key semantics (case-insensitive on
        // Windows), so an explicit PATH also replaces an inherited Path.
        let mut probe_environment = std::process::Command::new(&executable);
        probe_environment.envs(std::env::vars_os());
        for (key, value) in command.as_std_mut().get_envs() {
            if let Some(value) = value {
                probe_environment.env(key, value);
            } else {
                probe_environment.env_remove(key);
            }
        }
        let environment = probe_environment
            .get_envs()
            .filter_map(|(key, value)| value.map(|value| (key.to_owned(), value.to_owned())))
            .collect();
        let modern_lifecycle = tokio::task::spawn_blocking(move || {
            crate::executable::binary_version_for_launch(&executable, &cwd, environment)
        })
        .await
        .ok()
        .flatten()
        .is_some_and(|version| version >= semver::Version::new(18, 18, 0));
        // Which `omp` we resolved is the first thing a "before ready" failure
        // needs and the one thing the error never carried: PATH, the login
        // shell's PATH and `OMP_EXECUTABLE` can each point at a different
        // build, and a broken source checkout fails identically to a broken
        // install.
        tracing::debug!(
            target: "zeron_harness::omp",
            executable = %launch.executable.display(),
            cwd = %launch.cwd.display(),
            "spawning OMP RPC"
        );
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

        let stderr_tail: Arc<Mutex<VecDeque<String>>> = Arc::new(Mutex::new(VecDeque::new()));
        let stderr_reader = stderr.map(|stderr| {
            let tail = Arc::clone(&stderr_tail);
            tokio::spawn(async move {
                let mut reader = BufReader::new(stderr);
                loop {
                    match read_bounded_line(&mut reader, 64 * 1024).await {
                        Ok(Some(line)) => {
                            let diagnostic = sanitize_diagnostic(&String::from_utf8_lossy(&line));
                            if !diagnostic.is_empty() {
                                tracing::debug!(target: "zeron_harness::omp", "stderr: {diagnostic}");
                                let mut tail = lock(&tail);
                                if tail.len() == STDERR_TAIL_LINES {
                                    tail.pop_front();
                                }
                                tail.push_back(diagnostic);
                            }
                        }
                        Ok(None) => break,
                        Err(error) => {
                            tracing::debug!(target: "zeron_harness::omp", "stderr reader stopped: {}", sanitize_diagnostic(&error));
                            break;
                        }
                    }
                }
            })
        });

        let inner = Arc::new(Inner {
            writer: write_tx,
            pending: Mutex::new(HashMap::new()),
            sequence: AtomicU64::new(0),
            request_timeout: launch.request_timeout,
            fatal: Mutex::new(None),
            closed: AtomicBool::new(false),
            stderr_tail,
            frames_before_ready: AtomicU64::new(0),
            events_taken: Mutex::new(false),
            startup_retained: AtomicU64::new(0),
            startup_input: watch::channel(false).0,
        });
        let (event_tx, event_rx) = mpsc::channel(EVENT_BUFFER_CAPACITY);
        let (ready_tx, ready_rx) = oneshot::channel::<Result<OmpCapabilities, String>>();
        let reader_inner = Arc::clone(&inner);
        tokio::spawn(read_stdout(
            stdout,
            stderr_reader,
            reader_inner,
            event_tx,
            ready_tx,
        ));

        let process = Self {
            inner,
            child: Arc::new(tokio::sync::Mutex::new(child)),
            events: Arc::new(Mutex::new(Some(event_rx))),
            capabilities: Arc::new(Mutex::new(OmpCapabilities::default())),
            modern_lifecycle,
        };
        Ok((
            process,
            OmpStartup {
                ready: ready_rx,
                timeout: launch.handshake_timeout,
                executable: launch.executable,
                cwd: launch.cwd,
            },
        ))
    }

    pub(super) async fn initialize(&self, startup: OmpStartup) -> Result<(), HarnessError> {
        let result = self.with_input_budget(startup.timeout, startup.ready).await;
        let error = match result {
            Some(Ok(Ok(capabilities))) => {
                *lock(&self.capabilities) = capabilities;
                if capabilities.chunked_frames {
                    self.negotiate_chunked_frames().await;
                }
                lock(&self.inner.fatal).clone()
            }
            Some(Ok(Err(message))) => Some(format!(
                "{message} (executable: {})",
                startup.executable.display()
            )),
            Some(Err(_)) => Some(fatal_message(&self.inner, "OMP RPC exited before ready")),
            None => {
                let alive = self.child_state().await;
                tracing::warn!(target: "zeron_harness::omp", executable=%startup.executable.display(), cwd=%startup.cwd.display(), child=%alive,
                    waited_ms=startup.timeout.as_millis() as u64,
                    stdout_frames_before_deadline=self.inner.frames_before_ready.load(Ordering::SeqCst),
                    "OMP RPC handshake timed out");
                Some(fatal_message(&self.inner, "OMP RPC handshake timed out"))
            }
        };
        if let Some(message) = error {
            let _ = self.shutdown().await;
            Err(HarnessError::Protocol(message))
        } else {
            Ok(())
        }
    }

    pub fn capabilities(&self) -> OmpCapabilities {
        *lock(&self.capabilities)
    }

    pub(super) fn modern_lifecycle(&self) -> bool {
        self.modern_lifecycle
    }

    pub(super) fn startup_input_pending(&self, pending: bool) {
        self.inner.startup_input.send_replace(pending);
    }

    /// Pause a transport budget only while a startup dialog awaits a person.
    /// No timer or polling is added to the event loop.
    async fn with_input_budget<F: std::future::Future>(
        &self,
        mut budget: Duration,
        future: F,
    ) -> Option<F::Output> {
        tokio::pin!(future);
        let mut input = self.inner.startup_input.subscribe();
        loop {
            let waiting_for_input = *input.borrow_and_update();
            if waiting_for_input {
                tokio::select! {
                    result = &mut future => return Some(result),
                    _ = input.changed() => {},
                }
            } else {
                let started = tokio::time::Instant::now();
                tokio::select! {
                    result = &mut future => return Some(result),
                    _ = input.changed() => { budget = budget.saturating_sub(started.elapsed()); },
                    _ = tokio::time::sleep(budget) => {
                        if !*input.borrow() { return None; }
                        budget = Duration::ZERO;
                    },
                }
            }
        }
    }

    /// Allocate identity before registering prompt state or constructing its send future.
    pub fn request_id(&self) -> String {
        format!(
            "comet-{}",
            self.inner.sequence.fetch_add(1, Ordering::Relaxed) + 1
        )
    }

    /// Pede a v2 do protocolo, a que parte um frame acima de 1 MiB em
    /// `rpc_chunk` em vez de degradar a resposta para "RPC response exceeded
    /// the transport limit" (ver [`ChunkAssembler`]). Uma recusa nao custa a
    /// sessao: o resto do protocolo e identico, e so os frames grandes voltam a
    /// ser perdidos — por isso um aviso, nao um erro.
    async fn negotiate_chunked_frames(&self) {
        if let Err(error) = self
            .request(json!({ "type": "negotiate_protocol", "protocolVersion": 2 }))
            .await
        {
            tracing::warn!(
                target: "zeron_harness::omp",
                %error,
                "OMP RPC chunked-frame negotiation refused; responses over 1 MiB stay dropped"
            );
        }
    }

    pub async fn request(&self, command: Value) -> Result<Value, HarnessError> {
        self.request_with_timeout(command, self.inner.request_timeout)
            .await
    }

    pub async fn request_with_timeout(
        &self,
        command: Value,
        timeout: Duration,
    ) -> Result<Value, HarnessError> {
        self.request_with_admission(command, timeout, None).await
    }

    pub(super) async fn request_prompt(
        &self,
        command: Value,
        timeout: Duration,
        admission: PromptAdmission,
    ) -> Result<Value, HarnessError> {
        self.request_with_admission(command, timeout, Some(admission))
            .await
    }

    async fn request_with_admission(
        &self,
        command: Value,
        timeout: Duration,
        admission: Option<PromptAdmission>,
    ) -> Result<Value, HarnessError> {
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
        let id = match command.get("id") {
            None => self.request_id(),
            Some(Value::String(id)) if !id.is_empty() && id.len() <= 256 => id.clone(),
            _ => {
                return Err(HarnessError::Protocol(
                    "OMP RPC request id is invalid".into(),
                ));
            }
        };
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
            if pending.contains_key(&id) {
                return Err(HarnessError::Protocol(
                    "OMP RPC duplicate pending request id".into(),
                ));
            }
            if let Some(admission) = &admission {
                admission.0.store(1, Ordering::Release);
            }
            pending.insert(
                id.clone(),
                Pending {
                    command: command_name.clone(),
                    resolve: tx,
                    admission: admission.clone(),
                },
            );
        }
        if self
            .inner
            .writer
            .send(WriteMessage::Frame(serialized))
            .is_err()
        {
            if let Some(admission) = &admission {
                admission.0.store(0, Ordering::Release);
            }
            lock(&self.inner.pending).remove(&id);
            return Err(HarnessError::Protocol("OMP RPC stdin is closed".into()));
        }
        struct PendingGuard<'a> {
            pending: &'a Mutex<HashMap<String, Pending>>,
            id: &'a str,
            active: bool,
        }
        impl Drop for PendingGuard<'_> {
            fn drop(&mut self) {
                if self.active {
                    lock(self.pending).remove(self.id);
                }
            }
        }
        let mut guard = PendingGuard {
            pending: &self.inner.pending,
            id: &id,
            active: true,
        };
        match self.with_input_budget(timeout, rx).await {
            Some(Ok(Ok(value))) => {
                guard.active = false;
                Ok(value)
            }
            Some(Ok(Err(message))) => {
                guard.active = false;
                Err(HarnessError::Protocol(message))
            }
            Some(Err(_)) => {
                guard.active = false;
                Err(HarnessError::Protocol(format!(
                    "OMP RPC {command_name} response channel closed"
                )))
            }
            None => Err(HarnessError::Protocol(format!(
                "OMP RPC {command_name} request timed out"
            ))),
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
        let mut taken = lock(&self.inner.events_taken);
        let receiver = self.take_startup_events()?;
        *taken = true;
        Ok(receiver)
    }

    pub(super) fn take_startup_events(&self) -> Result<mpsc::Receiver<Value>, HarnessError> {
        let receiver = lock(&self.events)
            .take()
            .ok_or_else(|| HarnessError::Protocol("OMP RPC events already taken".into()))?;
        Ok(receiver)
    }

    pub(super) fn activate_events(&self) {
        *lock(&self.inner.events_taken) = true;
    }

    pub(super) fn consumed_startup_input(&self) {
        self.inner.startup_retained.fetch_sub(1, Ordering::Relaxed);
    }

    /// Por que o reader de stdout parou, nas palavras dele (teto de frame,
    /// JSONL invalido, chunk quebrado, EOF com a cauda do stderr). Gravado
    /// ANTES de o canal de eventos fechar, entao quem ve `recv() == None` pode
    /// consultar. `None` enquanto o reader esta vivo.
    pub fn fatal_reason(&self) -> Option<String> {
        lock(&self.inner.fatal).clone()
    }

    /// Estado do filho para o log de timeout: "vivo" separa um boot lento de
    /// um processo que morreu calado.
    async fn child_state(&self) -> String {
        match self.child.lock().await.try_wait() {
            Ok(None) => "alive".to_owned(),
            Ok(Some(status)) => format!("exited({status})"),
            Err(error) => format!("unknown({error})"),
        }
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
    stdout: ChildStdout,
    stderr_reader: Option<tokio::task::JoinHandle<()>>,
    inner: Arc<Inner>,
    event_tx: mpsc::Sender<Value>,
    ready_tx: oneshot::Sender<Result<OmpCapabilities, String>>,
) {
    let mut ready_tx = Some(ready_tx);
    // The public event receiver is not taken until after `start` returns. Its
    // bounded queue holds startup events, and try_send keeps this stdout reader
    // free to route negotiation and RPC responses while the consumer is absent.
    let mut chunks = ChunkAssembler::default();
    let mut reader = BufReader::new(stdout);
    loop {
        let line = match read_bounded_line(&mut reader, MAX_INBOUND_BYTES).await {
            Ok(Some(line)) => line,
            Ok(None) => break,
            Err(message) => {
                let message = fatal_message(&inner, &message);
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
                let message = fatal_message(&inner, "OMP RPC emitted non-UTF-8 JSONL");
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
        let frame = match chunks.push(frame) {
            Ok(Some(frame)) => frame,
            Ok(None) => continue,
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
                    let _ = ready.send(Ok(parse_capabilities(&frame)));
                }
            }
            Some("response") => route_response(&inner, frame),
            _ => {
                if ready_tx.is_some() {
                    inner.frames_before_ready.fetch_add(1, Ordering::SeqCst);
                }
                let live_frame = {
                    let events_taken = lock(&inner.events_taken);
                    if *events_taken {
                        Some(frame)
                    } else {
                        if inner.startup_retained.fetch_add(1, Ordering::Relaxed)
                            >= EVENT_BUFFER_CAPACITY as u64
                        {
                            let message = format!(
                                "OMP RPC event buffer exceeded its limit of {EVENT_BUFFER_CAPACITY} frames"
                            );
                            fail(&inner, message.clone());
                            if let Some(ready) = ready_tx.take() {
                                let _ = ready.send(Err(message));
                            }
                            return;
                        }
                        match event_tx.try_send(frame) {
                            Ok(()) => None,
                            Err(mpsc::error::TrySendError::Closed(_)) => return,
                            Err(mpsc::error::TrySendError::Full(_)) => {
                                let message = format!(
                                    "OMP RPC event buffer exceeded its limit of {EVENT_BUFFER_CAPACITY} frames"
                                );
                                fail(&inner, message.clone());
                                if let Some(ready) = ready_tx.take() {
                                    let _ = ready.send(Err(message));
                                }
                                return;
                            }
                        }
                    }
                };
                if let Some(frame) = live_frame {
                    if event_tx.send(frame).await.is_err() {
                        return;
                    }
                }
            }
        }
    }
    if let Some(stderr_reader) = stderr_reader {
        let _ = tokio::time::timeout(STDERR_DRAIN_TIMEOUT, stderr_reader).await;
    }
    let message = fatal_message(
        &inner,
        if ready_tx.is_some() {
            "OMP RPC exited before ready"
        } else {
            "OMP RPC stdout closed"
        },
    );
    fail(&inner, message.clone());
    if let Some(ready) = ready_tx.take() {
        let _ = ready.send(Err(message));
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

/// Protocol v2 enables chunked frames; older OMPs retain the v1 transport.
fn parse_capabilities(ready: &Value) -> OmpCapabilities {
    OmpCapabilities {
        chunked_frames: ready
            .get("supportedProtocolVersions")
            .and_then(Value::as_array)
            .is_some_and(|versions| versions.iter().any(|version| version.as_u64() == Some(2))),
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
        if let Some(admission) = &pending.admission {
            admission.0.store(2, Ordering::Release);
        }
        let _ = pending.resolve.send(Err(message));
        return;
    }
    let _ = pending
        .resolve
        .send(Ok(frame.get("data").cloned().unwrap_or(Value::Null)));
}

/// A transport failure in the child's own words: the retained stderr tail is
/// appended when there is one. "OMP RPC exited before ready" alone tells the
/// user nothing; the reason the child printed does.
fn fatal_message(inner: &Inner, message: &str) -> String {
    let tail = lock(&inner.stderr_tail)
        .iter()
        .cloned()
        .collect::<Vec<_>>()
        .join("; ");
    if tail.is_empty() {
        message.to_owned()
    } else {
        format!("{message}: {tail}")
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_skill_scope_overlay_is_nested_yaml_and_idempotent() {
        let first = skill_scope_overlay().unwrap();
        let second = skill_scope_overlay().unwrap();
        assert_eq!(first, second, "a stable path, not one file per launch");
        let body = std::fs::read_to_string(&first).unwrap();
        assert_eq!(body, SKILL_SCOPE_OVERLAY);

        // Every USER-level skill root off, every PROJECT root untouched: a repo
        // that ships skills keeps them.
        for key in [
            "enableClaudeUser",
            "enableAgentsUser",
            "enableCodexUser",
            "enablePiUser",
        ] {
            assert!(body.contains(&format!("  {key}: false")), "{key} missing");
        }
        assert!(!body.contains("Project"), "project roots must stay enabled");

        // The shape is load-bearing: OMP reads an overlay as config.yml-shaped
        // settings and silently IGNORES a dotted key, which looks like it works.
        assert!(body.starts_with("skills:\n"));
        assert!(!body.contains("skills.enable"));

        // No staging leftovers next to the overlay.
        let stem = format!("zeron-omp-skill-scope.{}", std::process::id());
        let leftovers = std::fs::read_dir(std::env::temp_dir())
            .unwrap()
            .filter_map(Result::ok)
            .filter(|entry| entry.file_name().to_string_lossy().starts_with(&stem))
            .count();
        assert_eq!(
            leftovers, 0,
            "staging files must be renamed, not left behind"
        );
    }
}
