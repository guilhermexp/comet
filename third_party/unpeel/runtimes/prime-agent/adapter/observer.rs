use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

#[cfg(unix)]
use std::io::{self, BufReader, Read, Write};
#[cfg(unix)]
use std::os::unix::net::UnixStream;

const PROTOCOL_NAME: &str = "prime-agent.daemon";
const PROTOCOL_VERSION: u32 = 7;
const READ_INTERVAL: Duration = Duration::from_millis(100);

pub(crate) fn private_daemon_socket_path(session_dir: &Path) -> Result<PathBuf, String> {
    #[cfg(not(unix))]
    {
        let _ = session_dir;
        Err("Persistent Prime Agent Workers require Unix".into())
    }
    #[cfg(unix)]
    {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        use std::os::unix::fs::{DirBuilderExt, MetadataExt, PermissionsExt};

        let uid = unsafe { libc::geteuid() };
        let verify = |directory: &Path| -> Result<(), String> {
            let metadata = std::fs::symlink_metadata(directory).map_err(|error| {
                format!(
                    "Inspect Prime socket directory {}: {error}",
                    directory.display()
                )
            })?;
            if !metadata.file_type().is_dir() || metadata.uid() != uid {
                return Err(format!(
                    "Prime socket directory {} is not an owned directory",
                    directory.display()
                ));
            }
            if metadata.mode() & 0o077 != 0 {
                std::fs::set_permissions(directory, std::fs::Permissions::from_mode(0o700))
                    .map_err(|error| {
                        format!(
                            "Protect Prime socket directory {}: {error}",
                            directory.display()
                        )
                    })?;
            }
            Ok(())
        };
        verify(session_dir)?;
        let candidate = session_dir.join("p.sock");
        if candidate.as_os_str().len() < 104 {
            return Ok(candidate);
        }
        let mut hash = DefaultHasher::new();
        session_dir.hash(&mut hash);
        uid.hash(&mut hash);
        let directory = PathBuf::from(format!("/tmp/.comet-pa-{:016x}", hash.finish()));
        match std::fs::DirBuilder::new().mode(0o700).create(&directory) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
            Err(error) => {
                return Err(format!(
                    "Create Prime socket directory {}: {error}",
                    directory.display()
                ));
            }
        }
        verify(&directory)?;
        Ok(directory.join("d.sock"))
    }
}

#[derive(Debug, Default, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct Summary {
    session_id: Option<String>,
    active_session_id: Option<String>,
    runtime_kind: Option<String>,
    rlm_child_id: Option<String>,
    parent_session_path: Option<String>,
    parent_active_session_id: Option<String>,
    #[serde(default)]
    rlm_depth: u64,
    activity: Option<String>,
    #[serde(default)]
    is_session_active: bool,
}

impl Summary {
    fn is_root(&self) -> bool {
        self.runtime_kind.as_deref() != Some("subagent")
            && self.rlm_depth == 0
            && self.rlm_child_id.is_none()
            && self.parent_session_path.is_none()
            && self.parent_active_session_id.is_none()
    }
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct RosterEntry {
    #[serde(default)]
    queued_child: bool,
    summary: Option<Summary>,
    status: Option<String>,
}

#[derive(Debug, serde::Deserialize)]
struct Protocol {
    name: String,
    version: u32,
}

#[derive(Debug, Default, serde::Deserialize)]
struct ResponseData {
    roster: Option<Vec<RosterEntry>>,
}

#[derive(Debug, serde::Deserialize)]
struct AgentEvent {
    #[serde(rename = "type")]
    kind: String,
    // Serde skips every undeclared field, including complete messages/images.
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct Frame {
    #[serde(rename = "type")]
    kind: String,
    protocol: Option<Protocol>,
    client_id: Option<String>,
    active_session_id: Option<String>,
    changed: Option<Vec<RosterEntry>>,
    data: Option<ResponseData>,
    event: Option<AgentEvent>,
    success: Option<bool>,
}

#[derive(Debug, PartialEq, Eq)]
enum Transition {
    Seen,
    Start,
    Stop,
    QueryRoster,
}

#[derive(Debug, Default)]
struct Observation {
    provider_session_id: Option<String>,
    active_session_id: Option<String>,
    working: bool,
}

impl Observation {
    fn entry(&mut self, entry: &RosterEntry) -> Option<Transition> {
        if entry.queued_child {
            return None;
        }
        let summary = entry.summary.as_ref()?;
        if !summary.is_root() {
            return None;
        }
        // Saved/inactive rows from the user's catalog cannot claim the Worker.
        let provider = summary.session_id.as_deref().filter(|id| !id.is_empty())?;
        let active = summary
            .active_session_id
            .as_deref()
            .filter(|id| !id.is_empty())?;
        let claimed = self.provider_session_id.is_none();
        match self.provider_session_id.as_deref() {
            Some(owned) if owned != provider => return None,
            None => self.provider_session_id = Some(provider.to_owned()),
            _ => {}
        }
        if self.active_session_id.as_deref() != Some(active) {
            self.active_session_id = Some(active.to_owned());
        }
        let busy = summary.is_session_active || summary.activity.as_deref() == Some("working");
        if !self.working && (entry.status.as_deref() == Some("running") || busy) {
            self.working = true;
            Some(Transition::Start)
        } else if self.working && entry.status.as_deref() == Some("idle") && !busy {
            self.working = false;
            Some(Transition::Stop)
        } else if claimed {
            Some(Transition::Seen)
        } else {
            None
        }
    }

    fn frame(&mut self, frame: &Frame) -> Option<Transition> {
        let entries = if frame.kind == "roster_update" {
            frame.changed.as_deref()
        } else if frame.kind == "response" && frame.success == Some(true) {
            frame.data.as_ref().and_then(|data| data.roster.as_deref())
        } else {
            None
        };
        if let Some(entries) = entries {
            return entries.iter().find_map(|entry| self.entry(entry));
        }
        if frame.kind != "session_event"
            || self.provider_session_id.is_none()
            || self.active_session_id.is_none()
            || frame.active_session_id != self.active_session_id
        {
            return None;
        }
        match frame.event.as_ref()?.kind.as_str() {
            "agent_start" if !self.working => {
                self.working = true;
                Some(Transition::Start)
            }
            // Prime's agent_end has no terminal/continuation flag. Ask for the
            // canonical outer-loop state rather than finishing queued work early.
            "agent_end" => Some(Transition::QueryRoster),
            _ => None,
        }
    }
}

pub(crate) struct ObserverLaunchContext {
    worker_session_id: String,
    session_dir: PathBuf,
    hook_port: Option<u16>,
    notify_script: PathBuf,
    registry: PathBuf,
    trace: PathBuf,
}

impl ObserverLaunchContext {
    pub(crate) fn new(
        worker_session_id: String,
        session_dir: PathBuf,
        hook_port: Option<u16>,
    ) -> Self {
        let home = crate::app_paths::unpeel_home();
        let notify_script = crate::hook_assets::notify_hook_script_path();
        let trace = notify_script.with_file_name("trace.log");
        Self {
            worker_session_id,
            session_dir,
            hook_port,
            notify_script,
            registry: home.join("app-ports"),
            trace,
        }
    }

    fn publish(&self, event: &str, provider: &str, generation: u64) {
        #[derive(serde::Serialize)]
        struct Payload<'a> {
            unpeel_runtime_id: &'static str,
            hook_event_name: &'a str,
            session_id: &'a str,
            unpeel_agent_kind: &'static str,
        }
        let payload = Payload {
            unpeel_runtime_id: "prime-agent",
            unpeel_agent_kind: "main",
            hook_event_name: event,
            session_id: provider,
        };
        let Ok(payload) = serde_json::to_string(&payload) else {
            return;
        };
        let mut command = std::process::Command::new("/bin/bash");
        command
            .arg(&self.notify_script)
            .arg(payload)
            .env("UNPEEL_SESSION_ID", &self.worker_session_id)
            .env("UNPEEL_SESSION_DIR", &self.session_dir)
            .env("UNPEEL_RUNTIME_GENERATION", generation.to_string())
            .env("UNPEEL_APP_PORT_REGISTRY_FILE", &self.registry)
            .env("UNPEEL_HOOK_TRACE_FILE", &self.trace)
            .env("UNPEEL_HOOK_POST_SYNC", "1")
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null());
        if let Some(port) = self.hook_port {
            command.env("UNPEEL_APP_PORT", port.to_string());
        } else {
            command.env_remove("UNPEEL_APP_PORT");
        }
        match command.status() {
            Ok(status) if status.success() => {}
            Ok(status) => log::warn!(
                "Prime Worker {} lifecycle transport {} exited with {status}",
                self.worker_session_id,
                self.notify_script.display()
            ),
            Err(error) => log::warn!(
                "Prime Worker {} could not run lifecycle transport: {error}",
                self.worker_session_id
            ),
        }
    }
}

#[cfg(unix)]
#[derive(serde::Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum Command<'a> {
    RosterSubscribe,
    Attach {
        #[serde(rename = "activeSessionId")]
        active_session_id: &'a str,
    },
    Shutdown {
        force: bool,
    },
}

#[cfg(unix)]
fn send_command(
    stream: &mut UnixStream,
    client: &str,
    command: Command<'_>,
    buffer: &mut Vec<u8>,
) -> io::Result<()> {
    #[derive(serde::Serialize)]
    struct ProtocolHeader {
        name: &'static str,
        version: u32,
    }
    #[derive(serde::Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Envelope<'a> {
        #[serde(rename = "type")]
        kind: &'static str,
        id: &'static str,
        protocol: ProtocolHeader,
        client_id: &'a str,
        command: Command<'a>,
    }
    buffer.clear();
    serde_json::to_writer(
        &mut *buffer,
        &Envelope {
            kind: "command",
            id: "comet_worker_observer",
            protocol: ProtocolHeader {
                name: PROTOCOL_NAME,
                version: PROTOCOL_VERSION,
            },
            client_id: client,
            command,
        },
    )?;
    buffer.push(b'\n');
    stream.write_all(buffer)
}

#[cfg(unix)]
fn hello_client(frame: Frame) -> io::Result<String> {
    if frame.kind != "daemon_hello"
        || !frame.protocol.is_some_and(|protocol| {
            protocol.name == PROTOCOL_NAME && protocol.version == PROTOCOL_VERSION
        })
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Prime Worker requires native daemon protocol 7",
        ));
    }
    frame.client_id.filter(|id| !id.is_empty()).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            "Prime daemon omitted observer client identity",
        )
    })
}

#[cfg(unix)]
fn connect_owned(path: &Path) -> io::Result<UnixStream> {
    use std::os::unix::fs::{FileTypeExt, MetadataExt};
    let metadata = std::fs::symlink_metadata(path)?;
    if !metadata.file_type().is_socket() || metadata.uid() != unsafe { libc::geteuid() } {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "Prime endpoint is not an owned socket",
        ));
    }
    UnixStream::connect(path)
}

#[cfg(unix)]
struct CancellableRead<'a> {
    stream: UnixStream,
    running: &'a AtomicBool,
}

#[cfg(unix)]
impl Read for CancellableRead<'_> {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        loop {
            if !self.running.load(Ordering::Acquire) {
                return Err(io::Error::new(
                    io::ErrorKind::ConnectionAborted,
                    "Prime observation scope ended",
                ));
            }
            match self.stream.read(buffer) {
                Err(error)
                    if matches!(
                        error.kind(),
                        io::ErrorKind::TimedOut | io::ErrorKind::WouldBlock
                    ) =>
                {
                    continue;
                }
                result => return result,
            }
        }
    }
}

#[cfg(unix)]
fn shutdown_owned_daemon(path: &Path) -> Result<(), String> {
    let mut stream = match connect_owned(path) {
        Ok(stream) => stream,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(error) if error.kind() == io::ErrorKind::ConnectionRefused => {
            // The endpoint is an owned socket with no accepting peer, not a
            // live daemon. Recover an orphan left by a killed Host.
            return std::fs::remove_file(path)
                .map_err(|error| format!("Remove stale owned Prime endpoint: {error}"));
        }
        Err(error) => {
            return Err(format!(
                "Connect to owned Prime daemon for shutdown: {error}"
            ));
        }
    };
    stream
        .set_read_timeout(Some(Duration::from_millis(500)))
        .map_err(|error| error.to_string())?;
    stream
        .set_write_timeout(Some(Duration::from_millis(500)))
        .map_err(|error| error.to_string())?;
    let reader = BufReader::new(stream.try_clone().map_err(|error| error.to_string())?);
    let mut frames = serde_json::Deserializer::from_reader(reader).into_iter::<Frame>();
    let hello = frames
        .next()
        .ok_or("Prime daemon closed before shutdown handshake")?
        .map_err(|error| error.to_string())?;
    let client = hello_client(hello).map_err(|error| error.to_string())?;
    send_command(
        &mut stream,
        &client,
        Command::Shutdown { force: true },
        &mut Vec::with_capacity(256),
    )
    .map_err(|error| error.to_string())?;
    let deadline = Instant::now() + Duration::from_secs(3);
    while path.exists() && Instant::now() < deadline {
        match frames.next() {
            Some(Ok(frame)) if frame.kind == "response" && frame.success == Some(false) => {
                return Err("Owned Prime daemon refused shutdown".into());
            }
            Some(Ok(_)) => {}
            None => std::thread::sleep(Duration::from_millis(25)),
            Some(Err(error)) if error.is_io() => {
                if !path.exists() {
                    break;
                }
                std::thread::sleep(Duration::from_millis(25));
            }
            Some(Err(error)) => return Err(format!("Read owned Prime shutdown: {error}")),
        }
    }
    // Prime can stop its listener without unlinking the socket inode. Only
    // remove that owned endpoint after the kernel confirms no peer accepts it.
    match connect_owned(path) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::ConnectionRefused => {
            std::fs::remove_file(path)
                .map_err(|error| format!("Remove stale owned Prime endpoint: {error}"))
        }
        Err(error) => Err(format!("Verify owned Prime daemon shutdown: {error}")),
        Ok(_) => Err(
            "Owned Prime daemon still accepts connections after shutdown; refusing to reuse it"
                .into(),
        ),
    }
}

pub(crate) struct PrimeGuard {
    context: Arc<ObserverLaunchContext>,
    socket: Arc<Path>,
    generation: u64,
    running: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
    scope_ended: bool,
}

impl PrimeGuard {
    fn start(&mut self) -> Result<(), String> {
        #[cfg(not(unix))]
        return Err("Persistent Prime Agent Workers require Unix".into());
        #[cfg(unix)]
        {
            self.running.store(true, Ordering::Release);
            let context = Arc::clone(&self.context);
            let socket = Arc::clone(&self.socket);
            let running = Arc::clone(&self.running);
            let generation = self.generation;
            self.thread = Some(
                std::thread::Builder::new()
                    .spawn(move || observe(context, socket, running, generation))
                    .map_err(|error| format!("Start Prime lifecycle observer: {error}"))?,
            );
            self.scope_ended = false;
            Ok(())
        }
    }

    fn stop(&mut self) -> Result<(), String> {
        self.running.store(false, Ordering::Release);
        let observer = self.thread.take().map(|thread| thread.join());
        #[cfg(unix)]
        shutdown_owned_daemon(&self.socket)?;
        if let Some(result) = observer {
            // Ending a scope or transport cannot fabricate successful completion.
            result.map_err(|_| "Prime lifecycle observer panicked".to_owned())?;
        }
        Ok(())
    }
}

impl crate::integrations::HostRuntimeControl for PrimeGuard {
    fn prepare_launch(&mut self, generation: u64) -> Result<(), String> {
        self.stop()?;
        self.generation = generation;
        self.start()
    }

    fn on_scope_end(&mut self, generation: u64) {
        if self.generation == generation && !self.scope_ended {
            self.scope_ended = true;
            if let Err(error) = self.stop() {
                log::warn!(
                    "Prime Worker {} scope cleanup failed: {error}",
                    self.context.worker_session_id
                );
            }
        }
    }
}

impl Drop for PrimeGuard {
    fn drop(&mut self) {
        if let Err(error) = self.stop() {
            log::warn!(
                "Prime Worker {} cleanup failed: {error}",
                self.context.worker_session_id
            );
        }
    }
}

pub(crate) fn start_observer(
    context: ObserverLaunchContext,
    socket: PathBuf,
    generation: u64,
) -> Result<PrimeGuard, String> {
    #[cfg(unix)]
    shutdown_owned_daemon(&socket)?;
    let mut guard = PrimeGuard {
        context: Arc::new(context),
        socket: Arc::from(socket),
        generation,
        running: Arc::new(AtomicBool::new(false)),
        thread: None,
        scope_ended: false,
    };
    guard.start()?;
    Ok(guard)
}

#[cfg(unix)]
fn observe(
    context: Arc<ObserverLaunchContext>,
    socket: Arc<Path>,
    running: Arc<AtomicBool>,
    generation: u64,
) {
    let mut observation = Observation::default();
    while running.load(Ordering::Acquire) {
        let result = (|| -> io::Result<()> {
            let mut stream = connect_owned(&socket)?;
            stream.set_read_timeout(Some(READ_INTERVAL))?;
            stream.set_write_timeout(Some(Duration::from_millis(500)))?;
            let reader = BufReader::new(CancellableRead {
                stream: stream.try_clone()?,
                running: &running,
            });
            let mut frames = serde_json::Deserializer::from_reader(reader).into_iter::<Frame>();
            let hello = frames.next().ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "Prime daemon closed before hello",
                )
            })??;
            let client = hello_client(hello)?;
            let mut wire = Vec::with_capacity(256);
            let mut attached: Option<String> = None;
            send_command(&mut stream, &client, Command::RosterSubscribe, &mut wire)?;
            for frame in frames {
                let frame = frame?;
                if !running.load(Ordering::Acquire) {
                    break;
                }
                match observation.frame(&frame) {
                    Some(Transition::Seen) => context.publish(
                        "HookSeen",
                        observation
                            .provider_session_id
                            .as_deref()
                            .unwrap_or_default(),
                        generation,
                    ),
                    Some(Transition::Start) => context.publish(
                        "Start",
                        observation
                            .provider_session_id
                            .as_deref()
                            .unwrap_or_default(),
                        generation,
                    ),
                    Some(Transition::Stop) => context.publish(
                        "Stop",
                        observation
                            .provider_session_id
                            .as_deref()
                            .unwrap_or_default(),
                        generation,
                    ),
                    Some(Transition::QueryRoster) => {
                        send_command(&mut stream, &client, Command::RosterSubscribe, &mut wire)?
                    }
                    None => {}
                }
                if observation.active_session_id.is_some()
                    && observation.active_session_id != attached
                {
                    let active = observation.active_session_id.as_deref().unwrap();
                    send_command(
                        &mut stream,
                        &client,
                        Command::Attach {
                            active_session_id: active,
                        },
                        &mut wire,
                    )?;
                    attached = observation.active_session_id.clone();
                }
            }
            Ok(())
        })();
        if let Err(error) = result {
            if error.kind() == io::ErrorKind::InvalidData {
                log::warn!(
                    "Prime Worker {} protocol error: {error}",
                    context.worker_session_id
                );
                break;
            }
        }
        // A disconnect is not a terminal event. Reattach and resnapshot, keeping
        // the last owned state until the daemon supplies new authoritative data.
        if running.load(Ordering::Acquire) {
            std::thread::sleep(READ_INTERVAL);
        }
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use serde_json::json;

    fn frame(value: serde_json::Value) -> Frame {
        serde_json::from_value(value).unwrap()
    }
    fn roster(provider: &str, active: Option<&str>, status: &str) -> Frame {
        frame(
            json!({"type":"roster_update","changed":[{"status":status,"summary":{
                "sessionId":provider,"activeSessionId":active,"runtimeKind":"top-level",
                "activity":if status=="running" {"working"} else {"idle"},"isSessionActive":status=="running"
            }}]}),
        )
    }

    #[test]
    fn children_and_foreign_roots_cannot_finish_the_owned_turn() {
        let mut state = Observation::default();
        let child = frame(
            json!({"type":"roster_update","changed":[{"status":"running","summary":{
                "sessionId":"child","activeSessionId":"child-live","rlmDepth":1
            }}]}),
        );
        assert_eq!(state.frame(&child), None);
        assert_eq!(
            state.frame(&roster("owner", Some("owner-live"), "running")),
            Some(Transition::Start)
        );
        assert_eq!(
            state.frame(&roster("other", Some("other-live"), "idle")),
            None
        );
        assert_eq!(state.frame(&frame(json!({"type":"session_event","activeSessionId":"child-live","event":{"type":"agent_end"}}))),None);
        assert!(state.working);
        assert_eq!(
            state.frame(&roster("owner", Some("owner-live"), "idle")),
            Some(Transition::Stop)
        );
    }

    #[test]
    fn saved_rows_and_unbound_events_cannot_claim_the_primary() {
        let mut state = Observation::default();
        assert_eq!(state.frame(&roster("saved", None, "inactive")), None);
        assert_eq!(state.frame(&frame(json!({"type":"session_event","activeSessionId":"foreign","event":{"type":"agent_start"}}))),None);
        assert!(state.provider_session_id.is_none());
        assert_eq!(
            state.frame(&roster("owner", Some("owner-live"), "running")),
            Some(Transition::Start)
        );
        assert_eq!(state.provider_session_id.as_deref(), Some("owner"));
    }

    #[test]
    fn unavailable_root_is_not_a_successful_idle_snapshot() {
        let mut state = Observation::default();
        state.frame(&roster("owner", Some("owner-live"), "running"));
        assert_eq!(
            state.frame(&roster("owner", Some("owner-live"), "inactive")),
            None
        );
        assert!(state.working);
    }

    #[test]
    fn huge_terminal_payload_preserves_framing_and_waits_for_outer_loop_idle() {
        let mut state = Observation::default();
        state.frame(&roster("owner", Some("owner-live"), "running"));
        let mut input = format!(
            r#"{{"type":"session_event","activeSessionId":"owner-live","event":{{"type":"agent_end","messages":[{{"content":"{}"}}]}}}}"#,
            "x".repeat(3 * 1024 * 1024)
        );
        input.push('\n');
        input.push_str(r#"{"type":"roster_update","changed":[{"status":"idle","summary":{"sessionId":"owner","activeSessionId":"owner-live","runtimeKind":"top-level","activity":"idle"}}]}"#);
        let mut frames =
            serde_json::Deserializer::from_reader(input.as_bytes()).into_iter::<Frame>();
        assert_eq!(
            state.frame(&frames.next().unwrap().unwrap()),
            Some(Transition::QueryRoster)
        );
        assert!(
            state.working,
            "an inner agent_end must not complete queued outer-loop work"
        );
        assert_eq!(
            state.frame(&frames.next().unwrap().unwrap()),
            Some(Transition::Stop)
        );
    }

    #[test]
    fn isolated_profiles_do_not_share_long_path_endpoints() {
        let temp = tempfile::tempdir().unwrap();
        let first = temp.path().join("a".repeat(110));
        let second = temp.path().join("b".repeat(110));
        std::fs::create_dir(&first).unwrap();
        std::fs::create_dir(&second).unwrap();
        let first_socket = private_daemon_socket_path(&first).unwrap();
        let second_socket = private_daemon_socket_path(&second).unwrap();
        assert_ne!(first_socket, second_socket);
        assert!(first_socket.as_os_str().len() < 104 && second_socket.as_os_str().len() < 104);
        std::fs::remove_dir(first_socket.parent().unwrap()).unwrap();
        std::fs::remove_dir(second_socket.parent().unwrap()).unwrap();
    }

    #[test]
    fn symlinked_session_directory_is_not_an_owned_endpoint() {
        let temp = tempfile::tempdir().unwrap();
        let alias = temp.path().join("alias");
        std::os::unix::fs::symlink(temp.path(), &alias).unwrap();
        assert!(private_daemon_socket_path(&alias).is_err());
    }

    #[test]
    fn shutdown_releases_a_stale_socket_for_the_next_scope() {
        use std::io::BufRead as _;

        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("prime.sock");
        let listener = std::os::unix::net::UnixListener::bind(&path).unwrap();
        let daemon = std::thread::spawn(move || {
            let (mut peer, _) = listener.accept().unwrap();
            writeln!(
                peer,
                "{}",
                json!({"type":"daemon_hello","protocol":{"name":PROTOCOL_NAME,"version":PROTOCOL_VERSION},"clientId":"shutdown-regression"})
            )
            .unwrap();
            let mut request = String::new();
            BufReader::new(peer).read_line(&mut request).unwrap();
            // UnixListener leaves its filesystem inode behind on drop.
        });
        shutdown_owned_daemon(&path).unwrap();
        daemon.join().unwrap();
        assert!(
            !path.exists(),
            "the next scope must not inherit a stale endpoint"
        );
    }

    #[test]
    fn cancellation_interrupts_a_quiet_partial_json_without_losing_the_reader() {
        let (reader, mut writer) = UnixStream::pair().unwrap();
        reader.set_read_timeout(Some(READ_INTERVAL)).unwrap();
        writer.write_all(b"{\"type\":\"session_").unwrap();
        let running = Arc::new(AtomicBool::new(true));
        let token = Arc::clone(&running);
        let (sent, received) = std::sync::mpsc::channel();
        let thread = std::thread::spawn(move || {
            let reader = BufReader::new(CancellableRead {
                stream: reader,
                running: &token,
            });
            let result = serde_json::Deserializer::from_reader(reader)
                .into_iter::<Frame>()
                .next();
            sent.send(result.is_some_and(|result| result.is_err()))
                .unwrap();
        });
        running.store(false, Ordering::Release);
        assert!(received.recv_timeout(Duration::from_secs(2)).unwrap());
        thread.join().unwrap();
    }
}
