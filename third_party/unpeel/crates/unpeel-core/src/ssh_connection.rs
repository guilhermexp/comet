//! Controller-side system-SSH transport for the shared Host contract.
//!
//! Product callers always launch `/usr/bin/ssh` with structured arguments;
//! no target or remote command is ever interpolated through a shell. A
//! background reader correlates out-of-order stdio responses, while writes
//! are serialized into complete frames. Process loss fails every call in that
//! generation. Only an unconstrained call may reconnect lazily. Semantic
//! backends must bootstrap that new transport generation before binding later
//! reads or effects to it; the transport never replays a call itself.

use std::collections::HashMap;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStderr, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex, Weak};
use std::time::Duration;

use crate::host_connection::{
    ConnectionGeneration, DeliveryState, HostCall, HostConnection, HostConnectionError, HostReply,
    PreparedHostCall, RequestSemantics,
};
use crate::relay_crypto::{self, TunnelRequest, TunnelResponse};
use crate::remote_stdio::{self, FRAME_KIND_REQUEST, FRAME_KIND_RESPONSE, REMOTE_STDIO_ARG};

pub const SYSTEM_SSH_PATH: &str = "/usr/bin/ssh";
const STDERR_TAIL_BYTES: usize = 16 * 1024;
const MAX_IN_FLIGHT_REQUESTS: usize = 64;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SshTarget {
    uri: String,
    destination: String,
}

impl SshTarget {
    /// Parse `ssh://alias` or `ssh://user@host`. V1 intentionally leaves
    /// ports, ProxyJump, identities, and other policy in `~/.ssh/config`.
    pub fn parse(uri: &str) -> Result<Self, HostConnectionError> {
        let destination = uri.strip_prefix("ssh://").ok_or_else(|| {
            HostConnectionError::InvalidTarget("expected ssh://alias".to_string())
        })?;
        if destination.is_empty() {
            return Err(HostConnectionError::InvalidTarget(
                "the destination is empty".to_string(),
            ));
        }
        if destination.len() > 255 {
            return Err(HostConnectionError::InvalidTarget(
                "the destination is too long".to_string(),
            ));
        }
        if destination.starts_with('-') {
            return Err(HostConnectionError::InvalidTarget(
                "the destination cannot begin with '-'".to_string(),
            ));
        }
        if destination.chars().any(|character| {
            character.is_whitespace()
                || character.is_control()
                || matches!(
                    character,
                    '/' | '\\' | '?' | '#' | ':' | ';' | '|' | '&' | '$' | '`'
                )
        }) {
            return Err(HostConnectionError::InvalidTarget(
                "use an SSH config alias or user@host without a path, port, or shell characters"
                    .to_string(),
            ));
        }
        let at_count = destination.bytes().filter(|byte| *byte == b'@').count();
        if at_count > 1 || destination.starts_with('@') || destination.ends_with('@') {
            return Err(HostConnectionError::InvalidTarget(
                "invalid user@host destination".to_string(),
            ));
        }
        Ok(Self {
            uri: uri.to_owned(),
            destination: destination.to_owned(),
        })
    }

    pub fn uri(&self) -> &str {
        &self.uri
    }

    pub fn destination(&self) -> &str {
        &self.destination
    }
}

enum PendingFailure {
    Disconnected(String),
    TimedOut(DeliveryState),
}

type PendingResult = Result<TunnelResponse, PendingFailure>;

struct PendingCall {
    sender: Sender<PendingResult>,
    wrote_any: Arc<AtomicBool>,
}

struct ProgressWriter<'a> {
    writer: &'a mut ChildStdin,
    wrote_any: &'a AtomicBool,
}

impl Write for ProgressWriter<'_> {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        let count = self.writer.write(bytes)?;
        if count > 0 {
            self.wrote_any.store(true, Ordering::Release);
        }
        Ok(count)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.writer.flush()
    }
}

struct WatchdogCancel(Option<mpsc::SyncSender<()>>);

impl Drop for WatchdogCancel {
    fn drop(&mut self) {
        if let Some(sender) = self.0.take() {
            let _ = sender.try_send(());
        }
    }
}

struct DiagnosticTail {
    bytes: Vec<u8>,
}

impl DiagnosticTail {
    fn new() -> Self {
        Self { bytes: Vec::new() }
    }

    fn append(&mut self, bytes: &[u8]) {
        if bytes.len() >= STDERR_TAIL_BYTES {
            self.bytes.clear();
            self.bytes
                .extend_from_slice(&bytes[bytes.len() - STDERR_TAIL_BYTES..]);
            return;
        }
        let overflow = self
            .bytes
            .len()
            .saturating_add(bytes.len())
            .saturating_sub(STDERR_TAIL_BYTES);
        if overflow > 0 {
            self.bytes.drain(..overflow);
        }
        self.bytes.extend_from_slice(bytes);
    }

    fn message(&self) -> Option<String> {
        let message = String::from_utf8_lossy(&self.bytes).trim().to_owned();
        (!message.is_empty()).then_some(message)
    }
}

struct ProcessGeneration {
    id: u64,
    stdin: Mutex<Option<ChildStdin>>,
    child: Mutex<Option<Child>>,
    pending: Mutex<HashMap<u64, PendingCall>>,
    dead: AtomicBool,
    diagnostics: Arc<Mutex<DiagnosticTail>>,
}

impl ProcessGeneration {
    fn is_alive(&self) -> bool {
        !self.dead.load(Ordering::Acquire)
    }

    fn diagnostic_message(&self, fallback: &str) -> String {
        self.diagnostics
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .message()
            .unwrap_or_else(|| fallback.to_owned())
    }

    fn complete(&self, response: TunnelResponse) -> bool {
        let pending = self
            .pending
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .remove(&response.id);
        if let Some(pending) = pending {
            let _ = pending.sender.send(Ok(response));
            true
        } else {
            false
        }
    }

    fn fail(&self, message: impl Into<String>) {
        if self.dead.swap(true, Ordering::AcqRel) {
            return;
        }
        let message = self.diagnostic_message(&message.into());
        let pending: Vec<PendingCall> = self
            .pending
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .drain()
            .map(|(_, pending)| pending)
            .collect();
        for pending in pending {
            let _ = pending
                .sender
                .send(Err(PendingFailure::Disconnected(message.clone())));
        }
        self.terminate();
    }

    fn timeout_request(&self, request_id: u64) {
        let pending = self
            .pending
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .remove(&request_id);
        let Some(pending) = pending else {
            return;
        };
        let delivery = if pending.wrote_any.load(Ordering::Acquire) {
            DeliveryState::OutcomeUnknown
        } else {
            DeliveryState::NotSent
        };
        let _ = pending.sender.send(Err(PendingFailure::TimedOut(delivery)));
        self.fail(format!("request {request_id} timed out"));
    }

    fn terminate(&self) {
        // Kill before taking the writer lock. If a malformed peer stops
        // reading while a large frame fills the pipe, waiting for stdin first
        // would deadlock the response reader that is trying to invalidate the
        // generation. Killing the owned child unblocks that write.
        let child = self
            .child
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .take();
        if let Some(mut child) = child {
            match child.try_wait() {
                Ok(Some(_)) => {}
                Ok(None) | Err(_) => {
                    let _ = child.kill();
                    let _ = child.wait();
                }
            }
        }
        drop(
            self.stdin
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .take(),
        );
    }

    fn round_trip(
        self: &Arc<Self>,
        request: &TunnelRequest,
        payload: &[u8],
        timeout: Duration,
    ) -> Result<TunnelResponse, AttemptFailure> {
        if !self.is_alive() {
            return Err(AttemptFailure::disconnected(
                DeliveryState::NotSent,
                self.diagnostic_message("SSH process is not running"),
            ));
        }

        let (sender, receiver): (Sender<PendingResult>, Receiver<PendingResult>) = mpsc::channel();
        let wrote_any = Arc::new(AtomicBool::new(false));
        {
            let mut pending = self
                .pending
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            if !self.is_alive() {
                return Err(AttemptFailure::disconnected(
                    DeliveryState::NotSent,
                    self.diagnostic_message("SSH process stopped before request dispatch"),
                ));
            }
            if pending.len() >= MAX_IN_FLIGHT_REQUESTS {
                return Err(AttemptFailure::TooManyInFlight);
            }
            if pending.contains_key(&request.id) {
                return Err(AttemptFailure::DuplicateRequestId);
            }
            pending.insert(
                request.id,
                PendingCall {
                    sender,
                    wrote_any: Arc::clone(&wrote_any),
                },
            );
        }

        let (cancel_watchdog, watchdog_cancelled) = mpsc::sync_channel(1);
        let weak = Arc::downgrade(self);
        let request_id = request.id;
        if let Err(error) = std::thread::Builder::new()
            .name(format!("unpeel-ssh-timeout-{request_id}"))
            .spawn(move || {
                if matches!(
                    watchdog_cancelled.recv_timeout(timeout),
                    Err(RecvTimeoutError::Timeout)
                ) {
                    if let Some(generation) = weak.upgrade() {
                        generation.timeout_request(request_id);
                    }
                }
            })
        {
            self.pending
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .remove(&request.id);
            return Err(AttemptFailure::disconnected(
                DeliveryState::NotSent,
                format!("start SSH request watchdog: {error}"),
            ));
        }
        let _watchdog = WatchdogCancel(Some(cancel_watchdog));

        let write_result = {
            let mut stdin = self
                .stdin
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            if !self.is_alive() || stdin.is_none() {
                Err((
                    DeliveryState::NotSent,
                    "SSH process stopped before request write".to_string(),
                ))
            } else {
                let mut writer = ProgressWriter {
                    writer: stdin.as_mut().expect("checked SSH stdin"),
                    wrote_any: &wrote_any,
                };
                remote_stdio::write_frame(&mut writer, FRAME_KIND_REQUEST, payload).map_err(
                    |message| {
                        let delivery = if wrote_any.load(Ordering::Acquire) {
                            DeliveryState::OutcomeUnknown
                        } else {
                            DeliveryState::NotSent
                        };
                        (delivery, message)
                    },
                )
            }
        };
        if let Err((delivery, message)) = write_result {
            self.pending
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .remove(&request.id);
            self.fail(message.clone());
            if let Ok(Err(failure)) = receiver.try_recv() {
                return match failure {
                    PendingFailure::Disconnected(message) => Err(AttemptFailure::disconnected(
                        if wrote_any.load(Ordering::Acquire) {
                            DeliveryState::OutcomeUnknown
                        } else {
                            DeliveryState::NotSent
                        },
                        message,
                    )),
                    PendingFailure::TimedOut(delivery) => Err(AttemptFailure::TimedOut(delivery)),
                };
            }
            return Err(AttemptFailure::disconnected(delivery, message));
        }

        match receiver.recv() {
            Ok(Ok(response)) => Ok(response),
            Ok(Err(PendingFailure::Disconnected(message))) => Err(AttemptFailure::disconnected(
                if wrote_any.load(Ordering::Acquire) {
                    DeliveryState::OutcomeUnknown
                } else {
                    DeliveryState::NotSent
                },
                message,
            )),
            Ok(Err(PendingFailure::TimedOut(delivery))) => Err(AttemptFailure::TimedOut(delivery)),
            Err(_) => {
                let message = self.diagnostic_message("SSH response channel closed");
                self.fail(message.clone());
                Err(AttemptFailure::disconnected(
                    DeliveryState::OutcomeUnknown,
                    message,
                ))
            }
        }
    }
}

impl Drop for ProcessGeneration {
    fn drop(&mut self) {
        self.dead.store(true, Ordering::Release);
        self.terminate();
    }
}

enum AttemptFailure {
    TooManyInFlight,
    DuplicateRequestId,
    Disconnected {
        delivery: DeliveryState,
        message: String,
    },
    TimedOut(DeliveryState),
}

impl AttemptFailure {
    fn disconnected(delivery: DeliveryState, message: String) -> Self {
        Self::Disconnected { delivery, message }
    }

    fn invalidates_generation(&self) -> bool {
        matches!(self, Self::Disconnected { .. } | Self::TimedOut(_))
    }

    fn into_public(self, request_id: u64, semantics: RequestSemantics) -> HostConnectionError {
        match self {
            Self::TooManyInFlight => HostConnectionError::TooManyInFlight {
                request_id,
                limit: MAX_IN_FLIGHT_REQUESTS,
            },
            Self::DuplicateRequestId => HostConnectionError::DuplicateRequestId(request_id),
            Self::Disconnected { delivery, message } => HostConnectionError::Disconnected {
                request_id,
                semantics,
                delivery,
                message,
            },
            Self::TimedOut(delivery) => HostConnectionError::TimedOut {
                request_id,
                semantics,
                delivery,
            },
        }
    }
}

struct ConnectionState {
    generation: Option<Arc<ProcessGeneration>>,
}

pub struct SshHostConnection {
    connection_id: uuid::Uuid,
    target: SshTarget,
    ssh_program: PathBuf,
    closed: AtomicBool,
    state: Mutex<ConnectionState>,
    next_generation: AtomicU64,
    next_request_id: AtomicU64,
}

impl SshHostConnection {
    pub fn new(target: SshTarget) -> Self {
        Self::with_ssh_program(target, SYSTEM_SSH_PATH)
            .expect("the fixed system SSH path is absolute")
    }

    /// Alternate executable injection for transport conformance tests. The
    /// shipped App/TUI path always uses [`SshHostConnection::new`].
    #[doc(hidden)]
    pub fn with_ssh_program(
        target: SshTarget,
        ssh_program: impl AsRef<Path>,
    ) -> Result<Self, HostConnectionError> {
        let ssh_program = ssh_program.as_ref();
        if !ssh_program.is_absolute() {
            return Err(HostConnectionError::Configuration(
                "SSH executable must be an absolute path".to_string(),
            ));
        }
        Ok(Self {
            connection_id: uuid::Uuid::new_v4(),
            target,
            ssh_program: ssh_program.to_owned(),
            closed: AtomicBool::new(false),
            state: Mutex::new(ConnectionState { generation: None }),
            next_generation: AtomicU64::new(1),
            next_request_id: AtomicU64::new(1),
        })
    }

    pub fn target(&self) -> &SshTarget {
        &self.target
    }

    fn allocate_request_id(&self) -> Result<u64, HostConnectionError> {
        let mut current = self.next_request_id.load(Ordering::Relaxed);
        loop {
            if current == u64::MAX {
                return Err(HostConnectionError::RequestIdExhausted);
            }
            match self.next_request_id.compare_exchange_weak(
                current,
                current + 1,
                Ordering::Relaxed,
                Ordering::Relaxed,
            ) {
                Ok(_) => return Ok(current),
                Err(actual) => current = actual,
            }
        }
    }

    fn allocate_generation_id(&self) -> Result<u64, String> {
        let mut current = self.next_generation.load(Ordering::Relaxed);
        loop {
            if current == u64::MAX {
                return Err("SSH generation id space is exhausted".to_string());
            }
            match self.next_generation.compare_exchange_weak(
                current,
                current + 1,
                Ordering::Relaxed,
                Ordering::Relaxed,
            ) {
                Ok(_) => return Ok(current),
                Err(actual) => current = actual,
            }
        }
    }

    fn current_live_generation(&self) -> Option<Arc<ProcessGeneration>> {
        self.state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .generation
            .as_ref()
            .filter(|generation| generation.is_alive())
            .map(Arc::clone)
    }

    fn generation_token(&self, generation: &ProcessGeneration) -> ConnectionGeneration {
        ConnectionGeneration {
            connection_id: self.connection_id,
            sequence: generation.id,
        }
    }

    pub fn disconnect(&self) {
        self.closed.store(true, Ordering::Release);
        let generation = self
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .generation
            .take();
        if let Some(generation) = generation {
            generation.fail("Controller disconnected");
        }
    }

    fn command(&self) -> Command {
        let mut command = Command::new(&self.ssh_program);
        command
            .arg("-T")
            // Disable credential/display forwarding regardless of the user's
            // ssh_config. ClearAllForwardings only clears Local/Remote/Dynamic
            // port forwards; agent, X11, and GSSAPI credential delegation must
            // be turned off explicitly so a compromised Host cannot reach the
            // Controller's SSH agent, X display, or Kerberos credentials.
            .arg("-a")
            .arg("-x")
            .arg("-o")
            .arg("BatchMode=yes")
            .arg("-o")
            .arg("ClearAllForwardings=yes")
            .arg("-o")
            .arg("ForwardAgent=no")
            .arg("-o")
            .arg("ForwardX11=no")
            .arg("-o")
            .arg("ForwardX11Trusted=no")
            .arg("-o")
            .arg("GSSAPIDelegateCredentials=no")
            .arg("-o")
            .arg("ControlMaster=no")
            .arg("-o")
            .arg("ControlPath=none")
            .arg("-o")
            .arg("PermitLocalCommand=no")
            .arg("-o")
            .arg("EscapeChar=none")
            .arg("-o")
            .arg("RemoteCommand=none")
            .arg("-o")
            .arg("StdinNull=no")
            .arg("--")
            .arg(self.target.destination())
            .arg("unpeel-host")
            .arg(REMOTE_STDIO_ARG)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        command
    }

    fn generation(&self) -> Result<Arc<ProcessGeneration>, String> {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if self.closed.load(Ordering::Acquire) {
            return Err("Host connection is closed".to_string());
        }
        if let Some(generation) = state.generation.as_ref() {
            if generation.is_alive() {
                return Ok(Arc::clone(generation));
            }
        }
        state.generation = None;
        let generation_id = self.allocate_generation_id()?;
        let generation = self.spawn_generation(generation_id)?;
        state.generation = Some(Arc::clone(&generation));
        Ok(generation)
    }

    fn spawn_generation(&self, generation_id: u64) -> Result<Arc<ProcessGeneration>, String> {
        let mut child = self.command().spawn().map_err(|error| error.to_string())?;
        let stdin = child.stdin.take().ok_or_else(|| {
            let _ = child.kill();
            let _ = child.wait();
            "SSH stdin was unavailable".to_string()
        })?;
        let stdout = child.stdout.take().ok_or_else(|| {
            let _ = child.kill();
            let _ = child.wait();
            "SSH stdout was unavailable".to_string()
        })?;
        let stderr = child.stderr.take().ok_or_else(|| {
            let _ = child.kill();
            let _ = child.wait();
            "SSH stderr was unavailable".to_string()
        })?;
        let generation = Arc::new(ProcessGeneration {
            id: generation_id,
            stdin: Mutex::new(Some(stdin)),
            child: Mutex::new(Some(child)),
            pending: Mutex::new(HashMap::new()),
            dead: AtomicBool::new(false),
            diagnostics: Arc::new(Mutex::new(DiagnosticTail::new())),
        });

        let weak = Arc::downgrade(&generation);
        std::thread::Builder::new()
            .name(format!("unpeel-ssh-read-{generation_id}"))
            .spawn(move || read_responses(stdout, weak))
            .map_err(|error| {
                generation.fail(format!("start SSH response reader: {error}"));
                error.to_string()
            })?;

        let diagnostics = Arc::clone(&generation.diagnostics);
        if let Err(error) = std::thread::Builder::new()
            .name(format!("unpeel-ssh-stderr-{generation_id}"))
            .spawn(move || drain_stderr(stderr, diagnostics))
        {
            generation.fail(format!("start SSH diagnostics reader: {error}"));
            return Err(error.to_string());
        }

        Ok(generation)
    }

    fn invalidate(&self, generation: &Arc<ProcessGeneration>, message: &str) {
        generation.fail(message.to_owned());
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if state
            .generation
            .as_ref()
            .is_some_and(|current| current.id == generation.id)
        {
            state.generation = None;
        }
    }
}

impl HostConnection for SshHostConnection {
    fn prepare(&self, call: HostCall) -> Result<PreparedHostCall, HostConnectionError> {
        if self.closed.load(Ordering::Acquire) {
            return Err(HostConnectionError::Closed);
        }
        Ok(PreparedHostCall {
            connection_id: self.connection_id,
            request_id: self.allocate_request_id()?,
            required_generation: None,
            call,
        })
    }

    fn prepare_in_generation(
        &self,
        generation: ConnectionGeneration,
        call: HostCall,
    ) -> Result<PreparedHostCall, HostConnectionError> {
        if self.closed.load(Ordering::Acquire) {
            return Err(HostConnectionError::Closed);
        }
        if generation.connection_id != self.connection_id {
            return Err(HostConnectionError::WrongGeneration(generation));
        }
        let request_id = self.allocate_request_id()?;
        let current = self.current_live_generation();
        let generation_is_current = current
            .as_ref()
            .map(|current| current.id == generation.sequence)
            .unwrap_or(false);
        if !generation_is_current {
            return Err(HostConnectionError::GenerationChanged {
                request_id,
                expected: generation,
            });
        }
        Ok(PreparedHostCall {
            connection_id: self.connection_id,
            request_id,
            required_generation: Some(generation),
            call,
        })
    }

    fn request(
        &self,
        call: PreparedHostCall,
        timeout: Duration,
    ) -> Result<HostReply, HostConnectionError> {
        let request_id = call.request_id;
        let semantics = call.call.semantics;
        let required_generation = call.required_generation;
        if call.connection_id != self.connection_id {
            return Err(HostConnectionError::WrongConnection(request_id));
        }
        if self.closed.load(Ordering::Acquire) {
            return Err(HostConnectionError::ClosedRequest(request_id));
        }
        let request = call.into_tunnel();
        let payload = relay_crypto::encode_tunnel_request(&request);
        if !relay_crypto::plaintext_frame_fits(payload.len()) {
            return Err(HostConnectionError::RequestTooLarge {
                request_id,
                encoded_bytes: payload.len(),
                max_bytes: relay_crypto::MAX_PLAINTEXT_BYTES,
            });
        }

        let generation = match required_generation {
            Some(expected) => {
                let current = self.current_live_generation();
                match current.as_ref() {
                    Some(current) if current.id == expected.sequence => Arc::clone(current),
                    _ => {
                        return Err(HostConnectionError::GenerationChanged {
                            request_id,
                            expected,
                        });
                    }
                }
            }
            None => match self.generation() {
                Ok(generation) => generation,
                Err(_) if self.closed.load(Ordering::Acquire) => {
                    return Err(HostConnectionError::ClosedRequest(request_id));
                }
                Err(message) => {
                    return Err(HostConnectionError::Launch {
                        request_id,
                        message,
                    });
                }
            },
        };
        match generation.round_trip(&request, &payload, timeout) {
            Ok(response) => Ok(HostReply {
                request_id: response.id,
                generation: self.generation_token(&generation),
                status: response.status,
                body: response.body,
            }),
            Err(failure) => {
                if failure.invalidates_generation() {
                    self.invalidate(&generation, "SSH request failed");
                }
                Err(failure.into_public(request_id, semantics))
            }
        }
    }

    fn disconnect(&self) {
        SshHostConnection::disconnect(self);
    }
}

impl Drop for SshHostConnection {
    fn drop(&mut self) {
        self.disconnect();
    }
}

fn read_responses(mut stdout: ChildStdout, generation: Weak<ProcessGeneration>) {
    loop {
        let frame = match remote_stdio::read_frame(&mut stdout) {
            Ok(Some(frame)) => frame,
            Ok(None) => {
                if let Some(generation) = generation.upgrade() {
                    generation.fail("SSH process closed its response stream");
                }
                return;
            }
            Err(message) => {
                if let Some(generation) = generation.upgrade() {
                    generation.fail(message);
                }
                return;
            }
        };
        if frame.kind != FRAME_KIND_RESPONSE {
            if let Some(generation) = generation.upgrade() {
                generation.fail(format!(
                    "unexpected SSH stdio response frame kind {}",
                    frame.kind
                ));
            }
            return;
        }
        let response = match relay_crypto::parse_tunnel_response(&frame.payload) {
            Ok(response) => response,
            Err(message) => {
                if let Some(generation) = generation.upgrade() {
                    generation.fail(format!("invalid SSH Host response: {message}"));
                }
                return;
            }
        };
        let Some(generation) = generation.upgrade() else {
            return;
        };
        let response_id = response.id;
        if !generation.complete(response) {
            generation.fail(format!(
                "SSH Host returned unknown or duplicate response id {response_id}"
            ));
            return;
        }
    }
}

fn drain_stderr(mut stderr: ChildStderr, diagnostics: Arc<Mutex<DiagnosticTail>>) {
    let mut buffer = [0u8; 4096];
    loop {
        match stderr.read(&mut buffer) {
            Ok(0) | Err(_) => return,
            Ok(count) => diagnostics
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .append(&buffer[..count]),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn target_is_an_opaque_safe_ssh_destination() {
        let target = SshTarget::parse("ssh://tommy@studio").unwrap();
        assert_eq!(target.destination(), "tommy@studio");
        assert_eq!(target.uri(), "ssh://tommy@studio");

        for invalid in [
            "studio",
            "ssh://",
            "ssh://-oProxyCommand=bad",
            "ssh://studio/path",
            "ssh://studio:2222",
            "ssh://studio;touch-bad",
            "ssh://user@@studio",
        ] {
            assert!(SshTarget::parse(invalid).is_err(), "accepted {invalid}");
        }
    }

    #[test]
    fn system_command_is_structured_and_noninteractive() {
        let connection = SshHostConnection::new(SshTarget::parse("ssh://studio").unwrap());
        let command = connection.command();
        assert_eq!(command.get_program(), Path::new(SYSTEM_SSH_PATH));
        let arguments: Vec<String> = command
            .get_args()
            .map(|argument| argument.to_string_lossy().into_owned())
            .collect();
        assert_eq!(
            arguments,
            [
                "-T",
                "-a",
                "-x",
                "-o",
                "BatchMode=yes",
                "-o",
                "ClearAllForwardings=yes",
                "-o",
                "ForwardAgent=no",
                "-o",
                "ForwardX11=no",
                "-o",
                "ForwardX11Trusted=no",
                "-o",
                "GSSAPIDelegateCredentials=no",
                "-o",
                "ControlMaster=no",
                "-o",
                "ControlPath=none",
                "-o",
                "PermitLocalCommand=no",
                "-o",
                "EscapeChar=none",
                "-o",
                "RemoteCommand=none",
                "-o",
                "StdinNull=no",
                "--",
                "studio",
                "unpeel-host",
                REMOTE_STDIO_ARG,
            ]
        );
    }

    #[test]
    fn prepared_calls_are_connection_owned_and_monotonic() {
        let first = SshHostConnection::new(SshTarget::parse("ssh://studio").unwrap());
        let second = SshHostConnection::new(SshTarget::parse("ssh://studio").unwrap());
        let call = || HostCall::new("GET", "/mobile/bootstrap", RequestSemantics::ReadOnly);
        let first_call = first.prepare(call()).unwrap();
        let second_call = first.prepare(call()).unwrap();
        assert_eq!(first_call.request_id(), 1);
        assert_eq!(second_call.request_id(), 2);

        let error = second
            .request(first_call, Duration::from_millis(1))
            .unwrap_err();
        assert_eq!(error, HostConnectionError::WrongConnection(1));

        let first_generation = ConnectionGeneration {
            connection_id: first.connection_id,
            sequence: 1,
        };
        assert_eq!(
            second
                .prepare_in_generation(first_generation, call())
                .unwrap_err(),
            HostConnectionError::WrongGeneration(first_generation)
        );
        let error = first
            .prepare_in_generation(first_generation, call())
            .unwrap_err();
        assert!(matches!(
            &error,
            HostConnectionError::GenerationChanged {
                request_id: 3,
                expected,
            } if *expected == first_generation
        ));
        assert_eq!(error.delivery(), Some(DeliveryState::NotSent));
    }

    #[test]
    fn request_ids_never_wrap_and_oversize_never_spawns() {
        let target = SshTarget::parse("ssh://studio").unwrap();
        let connection =
            SshHostConnection::with_ssh_program(target, "/definitely/missing/ssh").unwrap();
        connection
            .next_request_id
            .store(u64::MAX, Ordering::Relaxed);
        let call = HostCall::new("GET", "/mobile/bootstrap", RequestSemantics::ReadOnly);
        assert_eq!(
            connection.prepare(call).unwrap_err(),
            HostConnectionError::RequestIdExhausted
        );

        connection.next_request_id.store(1, Ordering::Relaxed);
        let oversized = HostCall::new("POST", "/mobile/write", RequestSemantics::Effect).with_body(
            "application/octet-stream",
            vec![0; relay_crypto::MAX_PLAINTEXT_BYTES],
        );
        let prepared = connection.prepare(oversized).unwrap();
        let error = connection
            .request(prepared, Duration::from_millis(1))
            .unwrap_err();
        assert!(matches!(
            &error,
            HostConnectionError::RequestTooLarge { request_id: 1, .. }
        ));
        assert_eq!(error.delivery(), Some(DeliveryState::NotSent));

        connection
            .next_generation
            .store(u64::MAX, Ordering::Relaxed);
        let prepared = connection
            .prepare(HostCall::new(
                "GET",
                "/mobile/bootstrap",
                RequestSemantics::ReadOnly,
            ))
            .unwrap();
        let error = connection
            .request(prepared, Duration::from_millis(1))
            .unwrap_err();
        assert!(matches!(
            &error,
            HostConnectionError::Launch {
                request_id: 2,
                message,
            } if message.contains("generation id space is exhausted")
        ));
        assert_eq!(error.delivery(), Some(DeliveryState::NotSent));
    }
}
