//! Comet frontend bridge for Unpeel's hook-owned session lifecycle.
//!
//! The state machine is included directly from the pinned Unpeel source so
//! Start/Stop/PermissionRequest, durable seeds, runtime generations and
//! output fallbacks cannot drift from the upstream TUI frontend.

#[allow(dead_code)]
#[path = "../../../third_party/unpeel/crates/unpeel-tui/src/activity.rs"]
mod upstream_activity;

use std::collections::HashSet;
use std::fs::OpenOptions;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::os::fd::AsRawFd;
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, Sender};
use std::sync::{Arc, Mutex, OnceLock, Weak};
use std::time::{Duration, SystemTime};

use upstream_activity::{ActivityEngine, HookState};

use crate::WorkersSession;

const IO_TIMEOUT: Duration = Duration::from_secs(5);
const MAX_BODY_BYTES: usize = 4 * 1024 * 1024;
const PORT_REGISTRY_CAP: usize = 16;

#[derive(Debug)]
struct HookEvent {
    session_id: String,
    event_name: String,
    tool_name: Option<String>,
    provider_session_id: Option<String>,
    provider_transcript_path: Option<String>,
    runtime_generation: Option<u64>,
    received_at: SystemTime,
}

struct HookIngress {
    port: u16,
    events: Mutex<Receiver<HookEvent>>,
}

pub(crate) struct ActivityBridge {
    engine: Mutex<ActivityEngine>,
    ingress: Option<HookIngress>,
    change_epoch: Arc<AtomicU64>,
}

impl ActivityBridge {
    fn start() -> Arc<Self> {
        let change_epoch = Arc::new(AtomicU64::new(0));
        let ingress = start_hook_ingress(Arc::clone(&change_epoch)).ok();
        migrate_legacy_telemetry(Arc::clone(&change_epoch));
        Arc::new(Self {
            engine: Mutex::new(ActivityEngine::default()),
            ingress,
            change_epoch,
        })
    }

    pub(crate) fn hook_port(&self) -> Option<u16> {
        self.ingress.as_ref().map(|ingress| ingress.port)
    }

    pub(crate) fn change_epoch(&self) -> u64 {
        self.change_epoch.load(Ordering::Acquire)
    }

    pub(crate) fn enrich(&self, sessions: &mut [WorkersSession]) {
        let mut engine = self.engine.lock().unwrap_or_else(|lock| lock.into_inner());
        if let Some(ingress) = &self.ingress {
            let events = ingress
                .events
                .lock()
                .unwrap_or_else(|lock| lock.into_inner());
            while let Ok(event) = events.try_recv() {
                let Some(manifest) = unpeel_core::session_host::load_manifest(&event.session_id)
                else {
                    continue;
                };
                update_provider_telemetry(&event, &manifest);
                engine.apply_hook_event_for_runtime(
                    &event.session_id,
                    &event.event_name,
                    event.tool_name.as_deref(),
                    event.received_at,
                    event.runtime_generation,
                    manifest.runtime_launch_generation,
                    manifest.runtime_launched_at,
                );
            }
        }

        let live_ids = sessions
            .iter()
            .map(|session| session.id.clone())
            .collect::<HashSet<_>>();
        engine.retain_sessions(&live_ids);
        let now = SystemTime::now();
        for session in sessions {
            let Some(manifest) = unpeel_core::session_host::load_manifest(&session.id) else {
                continue;
            };
            session.runtime_generation = manifest.runtime_launch_generation;
            if session.state != "running" {
                continue;
            }
            let session_dir = unpeel_core::session_host::session_dir(&session.id);
            session.idle_since_unix_ms = idle_since_unix_ms(&session.id, &manifest);
            session.resumable_conversation = resumable_conversation(&session.id, &manifest);
            session.updated_at_unix_ms = latest_activity_timestamp(
                session.updated_at_unix_ms,
                file_modified_unix_ms(&session_dir.join("output.bin")),
                file_modified_unix_ms(
                    &session_dir.join(crate::session_event_journal::JOURNAL_FILE),
                ),
            );
            let derived = derive_activity(
                &mut engine,
                ActivityInput {
                    session_id: &session.id,
                    command: &manifest.session.command,
                    active_runtime_id: session.active_runtime_id.as_deref(),
                    menu_prompt_active: manifest.menu_prompt_active,
                    runtime_launch_generation: manifest.runtime_launch_generation,
                    runtime_launched_at: manifest.runtime_launched_at,
                    activity_signal: manifest.screen_changed_at.unwrap_or_else(|| {
                        std::fs::metadata(session_dir.join("output.bin"))
                            .map(|metadata| metadata.len())
                            .unwrap_or(0)
                    }),
                    session_dir: &session_dir,
                },
                now,
            );
            // Read AFTER `derive_activity`, which is what runs the sweep for
            // this tick: a Stop-confirmed idle and a swept idle are the same
            // `HookState::Idle`, and only the former may be acted on
            // destructively (see `WorkersSession::idle_confirmed_by_hook`).
            session.idle_confirmed_by_hook = engine.hook_confirmed_idle(&session.id);
            if let Some(derived) = derived {
                session.activity =
                    merge_derived_activity(&session.activity, session.unread, derived).to_owned();
            }
        }
    }

    pub(crate) fn clear_attention(&self, session_id: &str) {
        let mut engine = self.engine.lock().unwrap_or_else(|lock| lock.into_inner());
        engine.clear_attention_unconfirmed(session_id, SystemTime::now());
        self.change_epoch.fetch_add(1, Ordering::Release);
    }
}

fn migrate_legacy_telemetry(change_epoch: Arc<AtomicU64>) {
    std::thread::spawn(move || {
        let migrated = unpeel_core::session_host::list_manifests()
            .iter()
            .filter(|manifest| migrate_legacy_telemetry_for_manifest(manifest))
            .count();
        if migrated > 0 {
            change_epoch.fetch_add(1, Ordering::Release);
        }
    });
}

fn migrate_legacy_telemetry_for_manifest(
    manifest: &unpeel_core::session_host::HostedSessionManifest,
) -> bool {
    unpeel_core::session_telemetry::refresh_legacy(manifest).unwrap_or(false)
}

fn file_modified_unix_ms(path: &Path) -> Option<u64> {
    std::fs::metadata(path)
        .ok()?
        .modified()
        .ok()?
        .duration_since(std::time::UNIX_EPOCH)
        .ok()
        .map(|duration| duration.as_millis().min(u128::from(u64::MAX)) as u64)
}

fn latest_activity_timestamp(
    manifest_updated_at: u64,
    output_updated_at: Option<u64>,
    hook_updated_at: Option<u64>,
) -> u64 {
    output_updated_at
        .into_iter()
        .chain(hook_updated_at)
        .fold(manifest_updated_at, u64::max)
}

/// The idle clock for hibernation: the newest of the host's parsed-screen
/// change stamp (a text hash, so an identical repaint does not move it) and
/// Unpeel's own command-aware activity signal (the durable last-hook event
/// for hook-capable runtimes). The manifest heartbeat and `output.bin` mtime
/// are deliberately absent — both advance on a Worker parked at its prompt.
fn idle_since_unix_ms(
    session_id: &str,
    manifest: &unpeel_core::session_host::HostedSessionManifest,
) -> Option<u64> {
    manifest
        .screen_changed_at
        .max(unpeel_core::session_ops::last_activity_ms(
            session_id,
            &manifest.session.command,
        ))
}

/// Whether relaunching this Worker would really resume ITS conversation:
/// a fact about this Worker's identity, checked directly, never inferred from
/// how the recipe rewrites a string. One of three pieces of evidence must
/// exist: a provider conversation id captured in the marker, a managed
/// session dir fixed in the command (pi family), or an explicit conversation
/// id already in the command (`codex resume <id>`, `--resume <id>`). A recipe
/// that only knows "the newest conversation of the directory" (`codex resume
/// --last`, `gemini --resume latest`, bare `--continue`) with none of these
/// does not qualify — two Workers in the same cwd would resume each other's
/// conversation, which is worse than not hibernating.
fn resumable_conversation(
    session_id: &str,
    manifest: &unpeel_core::session_host::HostedSessionManifest,
) -> bool {
    let command = manifest.session.command.trim();
    if command.is_empty() || !unpeel_core::resume::can_resume(command) {
        return false;
    }
    let (provider_session_id, _) = unpeel_core::session_ops::provider_session_marker(session_id);
    let unpeel_home = unpeel_core::app_paths::unpeel_home();
    let managed_root = unpeel_home.join("pi-sessions").canonicalize().ok();
    let managed_worker_directory = unpeel_core::resume::managed_storage_path(command, &unpeel_home)
        .and_then(|path| path.canonicalize().ok())
        .zip(
            unpeel_home
                .join("pi-sessions")
                .join(session_id)
                .canonicalize()
                .ok(),
        )
        .zip(managed_root)
        .is_some_and(|((actual, expected), root)| actual == expected && actual.starts_with(root));
    provider_session_id.is_some()
        || managed_worker_directory
        || unpeel_core::resume::embedded_conversation_id(command).is_some()
}

impl Drop for ActivityBridge {
    fn drop(&mut self) {
        if let Some(ingress) = &self.ingress {
            update_registry(ingress.port, false);
        }
    }
}

static SHARED_BRIDGE: OnceLock<Mutex<Weak<ActivityBridge>>> = OnceLock::new();

pub(crate) fn shared_activity_bridge() -> Arc<ActivityBridge> {
    let shared = SHARED_BRIDGE.get_or_init(|| Mutex::new(Weak::new()));
    let mut weak = shared.lock().unwrap_or_else(|lock| lock.into_inner());
    if let Some(bridge) = weak.upgrade() {
        return bridge;
    }
    let bridge = ActivityBridge::start();
    *weak = Arc::downgrade(&bridge);
    bridge
}

struct ActivityInput<'a> {
    session_id: &'a str,
    command: &'a str,
    active_runtime_id: Option<&'a str>,
    menu_prompt_active: bool,
    runtime_launch_generation: u64,
    runtime_launched_at: Option<u64>,
    activity_signal: u64,
    session_dir: &'a Path,
}

fn derive_activity(
    engine: &mut ActivityEngine,
    input: ActivityInput<'_>,
    now: SystemTime,
) -> Option<&'static str> {
    let catalog = unpeel_core::runtime_catalog::builtin_runtime_catalog();
    let command_head = unpeel_core::integrations::command_head(input.command);
    let lifecycle = input
        .active_runtime_id
        .and_then(|runtime_id| {
            catalog
                .by_id(runtime_id)
                .or_else(|| catalog.by_slug(runtime_id))
                .or_else(|| catalog.by_legacy_slug(runtime_id))
        })
        .or_else(|| catalog.by_command_alias_for_current_platform(command_head))
        .map(|runtime| &runtime.lifecycle);
    let uses_lifecycle_hooks = lifecycle.is_some_and(|policy| policy.uses_hook_port());
    if !uses_lifecycle_hooks {
        return input.menu_prompt_active.then_some("blocked");
    }
    let anchor_start_to_output = lifecycle
        .map(|policy| policy.anchor_start_event_to_output)
        .unwrap_or(true);
    let attention_clears_on_output = lifecycle
        .map(|policy| policy.attention_clears_on_output)
        .unwrap_or(true);
    let distrust_stops = lifecycle
        .map(|policy| policy.distrust_stops_while_output_grows)
        .unwrap_or(false);

    engine.observe_runtime_launch(
        input.session_id,
        input.runtime_launch_generation,
        input.runtime_launched_at,
    );
    engine.seed_from_disk(
        input.session_id,
        input.session_dir,
        anchor_start_to_output,
        input.runtime_launched_at,
        input.runtime_launch_generation,
    );
    engine.note_output_and_sweep(
        input.session_id,
        input.activity_signal,
        attention_clears_on_output,
        distrust_stops,
        now,
    );
    let state = engine
        .hook_owned_state(input.session_id)
        .unwrap_or(HookState::Idle);

    if input.menu_prompt_active && matches!(state, HookState::Busy | HookState::Idle) {
        return Some("blocked");
    }
    Some(match state {
        HookState::Busy => "working",
        HookState::Attention => "blocked",
        HookState::Idle => "idle",
    })
}

fn merge_derived_activity<'a>(wire_activity: &'a str, unread: bool, derived: &'a str) -> &'a str {
    if wire_activity == "done" && unread && derived == "idle" {
        wire_activity
    } else {
        derived
    }
}

fn registry_path() -> PathBuf {
    unpeel_core::app_paths::unpeel_home().join("app-ports")
}

fn update_registry(port: u16, register: bool) {
    let path = registry_path();
    let Some(parent) = path.parent() else { return };
    if std::fs::create_dir_all(parent).is_err() {
        return;
    }
    let lock_path = parent.join("app-ports.lock");
    let Ok(lock) = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .mode(0o600)
        .open(lock_path)
    else {
        return;
    };
    if unsafe { libc::flock(lock.as_raw_fd(), libc::LOCK_EX) } != 0 {
        return;
    }
    let mut ports = std::fs::read_to_string(&path)
        .unwrap_or_default()
        .lines()
        .filter_map(|line| line.trim().parse::<u16>().ok())
        .filter(|existing| *existing != port)
        .collect::<Vec<_>>();
    if register {
        ports.push(port);
        if ports.len() > PORT_REGISTRY_CAP {
            ports.drain(..ports.len() - PORT_REGISTRY_CAP);
        }
    }
    if ports.is_empty() {
        let _ = std::fs::remove_file(&path);
    } else {
        let body = ports
            .iter()
            .map(u16::to_string)
            .collect::<Vec<_>>()
            .join("\n")
            + "\n";
        let temporary = parent.join(format!(
            ".app-ports.{}.{}.tmp",
            std::process::id(),
            uuid::Uuid::new_v4()
        ));
        if std::fs::write(&temporary, body).is_ok() {
            if std::fs::rename(&temporary, &path).is_err() {
                let _ = std::fs::remove_file(temporary);
            }
        }
    }
    unsafe {
        libc::flock(lock.as_raw_fd(), libc::LOCK_UN);
    }
}

fn start_hook_ingress(change_epoch: Arc<AtomicU64>) -> Result<HookIngress, String> {
    let listener =
        TcpListener::bind("127.0.0.1:0").map_err(|error| format!("hook listener bind: {error}"))?;
    let port = listener
        .local_addr()
        .map_err(|error| error.to_string())?
        .port();
    update_registry(port, true);
    let (sender, receiver) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let sender = sender.clone();
            let change_epoch = Arc::clone(&change_epoch);
            std::thread::spawn(move || handle_connection(stream, &sender, &change_epoch));
        }
    });
    Ok(HookIngress {
        port,
        events: Mutex::new(receiver),
    })
}

fn respond(stream: &mut TcpStream, status: &str, body: &str) {
    let _ = write!(
        stream,
        "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nX-Unpeel-Frontend: comet\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
}

fn first_json_string(json: &serde_json::Value, keys: &[&str]) -> Option<String> {
    keys.iter().find_map(|key| {
        json.get(*key)
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_owned)
    })
}

fn hook_event_from_json(
    worker_session_id: &str,
    event_name: &str,
    json: &serde_json::Value,
    runtime_generation: Option<u64>,
    received_at: SystemTime,
) -> HookEvent {
    HookEvent {
        session_id: worker_session_id.to_owned(),
        event_name: event_name.to_owned(),
        tool_name: json
            .get("tool_name")
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned),
        provider_session_id: first_json_string(
            json,
            &[
                "session_id",
                "chatId",
                "chat_id",
                "provider_session_id",
                "providerSessionID",
                "providerSessionId",
                "thread_id",
                "threadID",
                "threadId",
                "conversation_id",
                "conversationID",
                "conversationId",
            ],
        ),
        provider_transcript_path: first_json_string(
            json,
            &[
                "transcript_path",
                "transcriptPath",
                "provider_transcript_path",
                "providerTranscriptPath",
            ],
        ),
        runtime_generation,
        received_at,
    }
}

fn persist_provider_binding(event: &HookEvent) -> Result<bool, String> {
    unpeel_core::session_ops::set_provider_session(
        &event.session_id,
        event.provider_session_id.as_deref(),
        event.provider_transcript_path.as_deref(),
    )
}

fn update_provider_telemetry(
    event: &HookEvent,
    manifest: &unpeel_core::session_host::HostedSessionManifest,
) {
    let binding_changed = match persist_provider_binding(event) {
        Ok(binding_changed) => binding_changed,
        Err(_) => {
            let _ = unpeel_core::session_telemetry::invalidate(&event.session_id);
            return;
        }
    };
    if binding_changed || event.event_name.eq_ignore_ascii_case("Stop") {
        let _ = unpeel_core::session_telemetry::refresh(manifest);
    }
}

fn handle_connection(mut stream: TcpStream, events: &Sender<HookEvent>, change_epoch: &AtomicU64) {
    let _ = stream.set_read_timeout(Some(IO_TIMEOUT));
    let _ = stream.set_write_timeout(Some(IO_TIMEOUT));
    let mut buffer = Vec::new();
    let mut chunk = [0u8; 16 * 1024];
    let header_end = loop {
        match stream.read(&mut chunk) {
            Ok(0) | Err(_) => return,
            Ok(read) => buffer.extend_from_slice(&chunk[..read]),
        }
        if let Some(position) = buffer.windows(4).position(|window| window == b"\r\n\r\n") {
            break position + 4;
        }
        if buffer.len() > MAX_BODY_BYTES {
            return;
        }
    };
    let head = String::from_utf8_lossy(&buffer[..header_end]).into_owned();
    let mut lines = head.lines();
    let mut request = lines.next().unwrap_or_default().split_whitespace();
    if request.next() != Some("POST") {
        respond(
            &mut stream,
            "405 Method Not Allowed",
            r#"{"error":"method not allowed"}"#,
        );
        return;
    }
    let path = request.next().unwrap_or_default();
    if path == unpeel_core::state_bus::ROUTE {
        respond(&mut stream, "200 OK", r#"{"ok":true}"#);
        return;
    }
    let Some(session_id) = path
        .strip_prefix("/hook/")
        .filter(|id| !id.is_empty() && !id.contains('/') && !id.contains(".."))
    else {
        respond(&mut stream, "404 Not Found", r#"{"error":"not found"}"#);
        return;
    };
    let content_length = lines
        .filter_map(|line| {
            let (name, value) = line.split_once(':')?;
            name.trim()
                .eq_ignore_ascii_case("content-length")
                .then(|| value.trim().parse::<usize>().ok())?
        })
        .next()
        .unwrap_or(0);
    if content_length > MAX_BODY_BYTES {
        respond(
            &mut stream,
            "400 Bad Request",
            r#"{"error":"body too large"}"#,
        );
        return;
    }
    let mut body = buffer[header_end..].to_vec();
    while body.len() < content_length {
        match stream.read(&mut chunk) {
            Ok(0) => break,
            Ok(read) => body.extend_from_slice(&chunk[..read]),
            Err(_) => return,
        }
    }
    let received_at = SystemTime::now();
    let Ok(json) = serde_json::from_slice::<serde_json::Value>(&body) else {
        respond(
            &mut stream,
            "400 Bad Request",
            r#"{"error":"invalid json"}"#,
        );
        return;
    };
    let Some(event_name) = json
        .get("hook_event_name")
        .or_else(|| json.get("hookEventName"))
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|event| !event.is_empty())
    else {
        respond(
            &mut stream,
            "400 Bad Request",
            r#"{"error":"missing hook_event_name"}"#,
        );
        return;
    };
    let Some(manifest) = unpeel_core::session_host::load_manifest(session_id) else {
        respond(
            &mut stream,
            "404 Not Found",
            r#"{"error":"unknown session"}"#,
        );
        return;
    };
    let runtime_generation = json
        .get("unpeel_runtime_generation")
        .or_else(|| json.get("unpeelRuntimeGeneration"))
        .and_then(serde_json::Value::as_u64);
    if runtime_generation.is_some_and(|generation| generation < manifest.runtime_launch_generation)
    {
        respond(&mut stream, "200 OK", r#"{"ok":true}"#);
        return;
    }
    if events
        .send(hook_event_from_json(
            session_id,
            event_name,
            &json,
            runtime_generation,
            received_at,
        ))
        .is_ok()
    {
        change_epoch.fetch_add(1, Ordering::Release);
    }
    respond(&mut stream, "200 OK", r#"{"ok":true}"#);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsString;

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    struct UnpeelHomeGuard {
        previous: Option<OsString>,
        previous_home: Option<OsString>,
    }

    impl UnpeelHomeGuard {
        fn set(path: &Path, user_home: &Path) -> Self {
            let previous = std::env::var_os("UNPEEL_HOME");
            let previous_home = std::env::var_os("HOME");
            // SAFETY: activity_bridge unit tests serialize process environment changes.
            unsafe {
                std::env::set_var("UNPEEL_HOME", path);
                std::env::set_var("HOME", user_home);
            }
            Self {
                previous,
                previous_home,
            }
        }
    }

    impl Drop for UnpeelHomeGuard {
        fn drop(&mut self) {
            // SAFETY: the guard is dropped while its caller holds ENV_LOCK.
            unsafe {
                match self.previous.take() {
                    Some(previous) => std::env::set_var("UNPEEL_HOME", previous),
                    None => std::env::remove_var("UNPEEL_HOME"),
                }
                match self.previous_home.take() {
                    Some(previous) => std::env::set_var("HOME", previous),
                    None => std::env::remove_var("HOME"),
                }
            }
        }
    }

    fn input<'a>(id: &'a str, command: &'a str, dir: &'a Path, signal: u64) -> ActivityInput<'a> {
        ActivityInput {
            session_id: id,
            command,
            active_runtime_id: None,
            menu_prompt_active: false,
            runtime_launch_generation: 1,
            runtime_launched_at: None,
            activity_signal: signal,
            session_dir: dir,
        }
    }

    #[test]
    fn hook_provider_identity_is_not_the_worker_identity() {
        let payload = serde_json::json!({
            "hook_event_name": "Stop",
            "session_id": "omp-provider-1",
            "provider_transcript_path": "/trusted/omp-provider-1.jsonl"
        });

        let event =
            hook_event_from_json("worker-1", "Stop", &payload, None, SystemTime::UNIX_EPOCH);

        assert_eq!(event.session_id, "worker-1");
        assert_eq!(event.provider_session_id.as_deref(), Some("omp-provider-1"));
        assert_eq!(
            event.provider_transcript_path.as_deref(),
            Some("/trusted/omp-provider-1.jsonl")
        );
    }

    #[test]
    fn provider_binding_is_persisted_for_the_url_worker() {
        let _lock = ENV_LOCK.lock().expect("UNPEEL_HOME test lock");
        let home = tempfile::tempdir().expect("temporary Unpeel home");
        let _home = UnpeelHomeGuard::set(home.path(), home.path());
        std::fs::create_dir_all(home.path().join("app-sessions/worker-1"))
            .expect("worker session directory");
        let event = HookEvent {
            session_id: "worker-1".into(),
            event_name: "Stop".into(),
            tool_name: None,
            provider_session_id: Some("omp-provider-1".into()),
            provider_transcript_path: Some("/trusted/omp-provider-1.jsonl".into()),
            runtime_generation: None,
            received_at: SystemTime::UNIX_EPOCH,
        };

        assert!(persist_provider_binding(&event).expect("persist provider binding"));
        assert_eq!(
            unpeel_core::session_ops::provider_session_marker("worker-1"),
            (
                Some("omp-provider-1".into()),
                Some("/trusted/omp-provider-1.jsonl".into())
            )
        );
        assert_eq!(
            unpeel_core::session_ops::provider_session_marker("omp-provider-1"),
            (None, None)
        );
    }

    fn omp_manifest(session_id: &str) -> unpeel_core::session_host::HostedSessionManifest {
        unpeel_core::session_host::HostedSessionManifest {
            session: unpeel_core::state::SessionInfo {
                id: session_id.into(),
                project_id: "project-1".into(),
                label: "OMP".into(),
                custom_title: false,
                command: "omp".into(),
                created_at: 1,
                tag_id: None,
                worktree_path: None,
                worktree_branch: None,
                parent_session_id: None,
                spawned_by: None,
                role: None,
                task: None,
            },
            cwd: "/tmp".into(),
            state: unpeel_core::session_host::HostedSessionState::Running,
            pid: None,
            pid_started_at: None,
            exit_code: None,
            host_build_id: None,
            host_protocol_version: None,
            has_been_written_to: true,
            provider_session_id: None,
            provider_transcript_path: None,
            managed_storage_path: None,
            resume_failure_markers: Vec::new(),
            runtime: None,
            runtime_launch_generation: 1,
            runtime_launch_pending: false,
            runtime_launched_at: Some(1),
            runtime_launch_output_offset: 0,
            mcp_enabled: None,
            browser_mcp_enabled: None,
            computer_mcp_enabled: None,
            mcp_client_registered: false,
            browser_client_registered: false,
            computer_client_registered: false,
            menu_prompt_active: false,
            screen_changed_at: None,
            detected_local_urls: Vec::new(),
            heartbeat_at: 1,
            updated_at: 1,
        }
    }

    /// A Worker parked at its prompt keeps writing: the host heartbeats every
    /// 60 s and the TUI repaints the same screen. Measured on an `omp` idle for
    /// 24 h, both `updated_at` and `output.bin` looked 0 h old — which is why
    /// the idle clock reads the screen-text stamp and the durable hook event
    /// instead.
    #[test]
    fn the_idle_clock_ignores_heartbeat_and_identical_repaint() {
        let _lock = ENV_LOCK.lock().unwrap_or_else(|error| error.into_inner());
        let home = tempfile::tempdir().expect("temporary Unpeel home");
        let _home = UnpeelHomeGuard::set(home.path(), home.path());
        let session_dir = home.path().join("app-sessions/worker-1");
        std::fs::create_dir_all(&session_dir).expect("worker session directory");
        std::fs::write(session_dir.join("output.bin"), b"same screen repainted")
            .expect("write output");
        let mut manifest = omp_manifest("worker-1");
        manifest.screen_changed_at = Some(1_000);
        manifest.heartbeat_at = 9_000_000;
        manifest.updated_at = 9_000_000;

        assert_eq!(idle_since_unix_ms("worker-1", &manifest), Some(1_000));

        std::fs::write(session_dir.join("last-hook-event.json"), b"{}").expect("write hook seed");
        let after_hook =
            idle_since_unix_ms("worker-1", &manifest).expect("hook advances the clock");
        assert!(
            after_hook > 1_000,
            "a real lifecycle event must advance the idle clock, got {after_hook}"
        );
    }

    #[test]
    fn a_pi_family_worker_is_only_resumable_once_there_is_something_to_resume_from() {
        let _lock = ENV_LOCK.lock().unwrap_or_else(|error| error.into_inner());
        let home = tempfile::tempdir().expect("temporary Unpeel home");
        let _home = UnpeelHomeGuard::set(home.path(), home.path());
        std::fs::create_dir_all(home.path().join("app-sessions/worker-1"))
            .expect("worker session directory");
        let manifest = omp_manifest("worker-1");

        assert!(
            !resumable_conversation("worker-1", &manifest),
            "a bare `omp` command with no captured conversation relaunches clean"
        );

        unpeel_core::session_ops::set_provider_session("worker-1", Some("omp-provider-1"), None)
            .expect("capture the provider conversation");

        assert!(resumable_conversation("worker-1", &manifest));
    }

    #[test]
    fn a_managed_session_dir_alone_makes_the_conversation_resumable() {
        let _lock = ENV_LOCK.lock().unwrap_or_else(|error| error.into_inner());
        let home = tempfile::tempdir().expect("temporary Unpeel home");
        let _home = UnpeelHomeGuard::set(home.path(), home.path());
        std::fs::create_dir_all(home.path().join("app-sessions/worker-1"))
            .expect("worker session directory");
        let mut manifest = omp_manifest("worker-1");
        let managed = home.path().join("pi-sessions/worker-1");
        std::fs::create_dir_all(&managed).expect("managed Worker directory");
        manifest.session.command = format!("omp --session-dir '{}'", managed.display());

        assert!(resumable_conversation("worker-1", &manifest));

        let shared = home.path().join("shared");
        std::fs::create_dir_all(&shared).expect("shared directory");
        manifest.session.command = format!("omp --session-dir '{}'", shared.display());
        assert!(!resumable_conversation("worker-1", &manifest));

        let other_worker = home.path().join("pi-sessions/worker-2");
        std::fs::create_dir_all(&other_worker).expect("other Worker directory");
        manifest.session.command = format!("omp --session-dir '{}'", other_worker.display());
        assert!(!resumable_conversation("worker-1", &manifest));

        manifest.session.command = format!(
            "omp --session-dir '{}'",
            home.path().join("pi-sessions").display()
        );
        assert!(!resumable_conversation("worker-1", &manifest));

        let descendant = managed.join("nested");
        std::fs::create_dir_all(&descendant).expect("managed descendant");
        manifest.session.command = format!("omp --session-dir '{}'", descendant.display());
        assert!(!resumable_conversation("worker-1", &manifest));

        manifest.session.command = format!(
            "omp --session-dir '{}/pi-sessions/worker-2/../worker-1'",
            home.path().display()
        );
        assert!(!resumable_conversation("worker-1", &manifest));

        manifest.session.command = "omp --session-dir '/tmp/shared-pi-sessions'".into();
        assert!(
            !resumable_conversation("worker-1", &manifest),
            "a session dir outside the managed root is not evidence of THIS Worker"
        );
    }

    #[test]
    fn an_already_resumed_command_keeps_its_worker_evidence() {
        let _lock = ENV_LOCK.lock().unwrap_or_else(|error| error.into_inner());
        let home = tempfile::tempdir().expect("temporary Unpeel home");
        let _home = UnpeelHomeGuard::set(home.path(), home.path());
        std::fs::create_dir_all(home.path().join("app-sessions/worker-1"))
            .expect("worker session directory");
        let mut manifest = omp_manifest("worker-1");
        let managed = home.path().join("pi-sessions/worker-1");
        std::fs::create_dir_all(&managed).expect("managed Worker directory");
        manifest.session.command = format!("omp --session-dir '{}' --continue", managed.display());
        assert!(
            resumable_conversation("worker-1", &manifest),
            "the recipe is idempotent on its own output; the managed dir is still the evidence"
        );

        manifest.session.command = "codex resume 't1' --full-auto".into();
        assert!(
            resumable_conversation("worker-1", &manifest),
            "an explicit resume target in the command is evidence of THIS Worker"
        );

        manifest.session.command = "codex resume --last --full-auto".into();
        assert!(!resumable_conversation("worker-1", &manifest));
        manifest.session.command = "gemini --resume latest".into();
        assert!(!resumable_conversation("worker-1", &manifest));
        manifest.session.command = "omp --continue".into();
        assert!(!resumable_conversation("worker-1", &manifest));
    }

    #[test]
    fn a_newest_of_the_directory_recipe_needs_a_captured_id_to_be_resumable() {
        let _lock = ENV_LOCK.lock().unwrap_or_else(|error| error.into_inner());
        let home = tempfile::tempdir().expect("temporary Unpeel home");
        let _home = UnpeelHomeGuard::set(home.path(), home.path());
        std::fs::create_dir_all(home.path().join("app-sessions/worker-1"))
            .expect("worker session directory");
        let mut manifest = omp_manifest("worker-1");
        manifest.session.command = "codex --full-auto".into();

        assert!(
            !resumable_conversation("worker-1", &manifest),
            "`codex resume --last` resumes whichever Worker last wrote to the cwd"
        );

        unpeel_core::session_ops::set_provider_session("worker-1", Some("codex-thread-1"), None)
            .expect("capture the provider conversation");

        assert!(resumable_conversation("worker-1", &manifest));
    }

    #[test]
    fn a_terminal_conversation_is_never_resumable() {
        let _lock = ENV_LOCK.lock().unwrap_or_else(|error| error.into_inner());
        let home = tempfile::tempdir().expect("temporary Unpeel home");
        let _home = UnpeelHomeGuard::set(home.path(), home.path());
        let mut manifest = omp_manifest("worker-1");
        manifest.session.command = "bash".into();

        assert!(!resumable_conversation("worker-1", &manifest));
    }

    #[test]
    fn only_a_stop_hook_confirms_idleness_the_sweep_does_not() {
        let mut engine = ActivityEngine::default();
        let t0 = SystemTime::UNIX_EPOCH + Duration::from_secs(1_000);
        engine.apply_hook_event("worker-1", "Start", None, t0);
        engine.note_output_and_sweep("worker-1", 1, true, false, t0);

        // Screen frozen past the sweep timeout: the state machine reports idle
        // even though the turn never ended.
        let swept_at = t0 + upstream_activity::HOOK_IDLE_TIMEOUT + Duration::from_secs(1);
        engine.note_output_and_sweep("worker-1", 1, true, false, swept_at);
        assert_eq!(
            engine.hook_owned_state("worker-1"),
            Some(HookState::Idle),
            "the sweep is what this finding is about"
        );
        assert!(
            !engine.hook_confirmed_idle("worker-1"),
            "a frozen screen is not an end of turn"
        );

        engine.apply_hook_event("worker-1", "Stop", None, swept_at);
        assert!(engine.hook_confirmed_idle("worker-1"));

        engine.apply_hook_event("worker-1", "UserPromptSubmit", None, swept_at);
        assert!(
            !engine.hook_confirmed_idle("worker-1"),
            "a new turn revokes the confirmation"
        );
    }

    #[test]
    fn clearing_attention_from_the_menu_is_not_a_confirmed_end_of_turn() {
        let mut engine = ActivityEngine::default();
        let t0 = SystemTime::UNIX_EPOCH + Duration::from_secs(1_000);
        engine.apply_hook_event("grok-1", "UserPromptSubmit", None, t0);
        engine.apply_hook_event(
            "grok-1",
            "PermissionRequest",
            None,
            t0 + Duration::from_secs(5),
        );
        assert_eq!(
            engine.hook_owned_state("grok-1"),
            Some(HookState::Attention)
        );

        engine.clear_attention_unconfirmed("grok-1", t0 + Duration::from_secs(10));
        assert_eq!(engine.hook_owned_state("grok-1"), Some(HookState::Idle));
        assert!(
            !engine.hook_confirmed_idle("grok-1"),
            "a click in Comet is not the runtime saying the turn ended"
        );

        engine.apply_hook_event("grok-1", "Stop", None, t0 + Duration::from_secs(20));
        assert!(engine.hook_confirmed_idle("grok-1"));
    }

    #[test]
    fn output_growth_after_a_stop_revokes_the_confirmation_until_the_next_stop() {
        let mut engine = ActivityEngine::default();
        let t0 = SystemTime::UNIX_EPOCH + Duration::from_secs(1_000);
        engine.apply_hook_event("cursor-1", "Stop", None, t0);
        engine.note_output_and_sweep("cursor-1", 1, true, false, t0 + Duration::from_secs(1));
        assert!(engine.hook_confirmed_idle("cursor-1"));

        // The orchestrator sends text five minutes later; cursor-agent's hook
        // only posts Stop, so nothing revokes the old confirmation but growth.
        let typed_at = t0 + Duration::from_secs(300);
        engine.note_output_and_sweep("cursor-1", 2, true, false, typed_at);
        assert_eq!(engine.hook_owned_state("cursor-1"), Some(HookState::Idle));
        assert!(
            !engine.hook_confirmed_idle("cursor-1"),
            "a new turn without a start hook must still kill the stale Stop"
        );

        let swept_at = typed_at + upstream_activity::HOOK_IDLE_TIMEOUT + Duration::from_secs(1);
        engine.note_output_and_sweep("cursor-1", 2, true, false, swept_at);
        assert!(!engine.hook_confirmed_idle("cursor-1"));

        engine.apply_hook_event("cursor-1", "Stop", None, swept_at);
        assert!(engine.hook_confirmed_idle("cursor-1"));
    }

    #[test]
    fn output_growth_inside_the_grace_revokes_idle_confirmation() {
        let mut engine = ActivityEngine::default();
        let t0 = SystemTime::UNIX_EPOCH + Duration::from_secs(1_000);
        engine.apply_hook_event("cursor-1", "Stop", None, t0);
        engine.note_output_and_sweep("cursor-1", 1, true, false, t0 + Duration::from_secs(1));
        engine.note_output_and_sweep("cursor-1", 2, true, false, t0 + Duration::from_secs(2));
        assert!(!engine.hook_confirmed_idle("cursor-1"));
    }

    #[test]
    fn an_unchanged_signal_keeps_idle_confirmation_across_sweeps() {
        let mut engine = ActivityEngine::default();
        let t0 = SystemTime::UNIX_EPOCH + Duration::from_secs(1_000);
        engine.apply_hook_event("cursor-1", "Stop", None, t0);
        engine.note_output_and_sweep("cursor-1", 1, true, false, t0 + Duration::from_secs(1));
        for tick in 1..=6 {
            let at = t0 + upstream_activity::HOOK_IDLE_TIMEOUT * tick;
            engine.note_output_and_sweep("cursor-1", 1, true, false, at);
            assert!(
                engine.hook_confirmed_idle("cursor-1"),
                "an unchanged signal must not erode the confirmation (tick {tick})"
            );
        }
    }

    #[test]
    fn a_distrusted_stop_rearmed_by_output_no_longer_confirms_idleness() {
        let mut engine = ActivityEngine::default();
        let t0 = SystemTime::UNIX_EPOCH + Duration::from_secs(1_000);
        engine.apply_hook_event("codex-1", "UserPromptSubmit", None, t0);
        engine.note_output_and_sweep("codex-1", 1, true, true, t0);

        // codex fires Stop mid-turn.
        let stopped_at = t0 + Duration::from_secs(30);
        engine.apply_hook_event("codex-1", "Stop", None, stopped_at);
        assert!(engine.hook_confirmed_idle("codex-1"));

        // Output keeps growing inside the re-arm window: the turn is alive.
        let rearmed_at = stopped_at + Duration::from_secs(10);
        engine.note_output_and_sweep("codex-1", 2, true, true, rearmed_at);
        assert_eq!(engine.hook_owned_state("codex-1"), Some(HookState::Busy));
        assert!(!engine.hook_confirmed_idle("codex-1"));

        // Then a long silent subprocess until the sweep gives up.
        let swept_at = rearmed_at + upstream_activity::HOOK_IDLE_TIMEOUT + Duration::from_secs(1);
        engine.note_output_and_sweep("codex-1", 2, true, true, swept_at);
        assert_eq!(engine.hook_owned_state("codex-1"), Some(HookState::Idle));
        assert!(
            !engine.hook_confirmed_idle("codex-1"),
            "the pre-re-arm Stop is no proof the re-armed turn ended"
        );

        engine.apply_hook_event("codex-1", "Stop", None, swept_at);
        assert!(engine.hook_confirmed_idle("codex-1"));
    }

    #[test]
    fn provider_telemetry_refreshes_on_binding_change_and_stop() {
        let _lock = ENV_LOCK.lock().unwrap_or_else(|error| error.into_inner());
        let state_home = tempfile::tempdir().expect("temporary Unpeel home");
        let user_home = tempfile::tempdir().expect("temporary user home");
        let _home = UnpeelHomeGuard::set(state_home.path(), user_home.path());
        std::fs::create_dir_all(state_home.path().join("app-sessions/worker-1"))
            .expect("worker session directory");
        let omp_root = user_home.path().join(".omp/agent/sessions/project");
        std::fs::create_dir_all(&omp_root).expect("OMP Session root");
        let transcript = omp_root.join("provider.jsonl");
        std::fs::write(
            &transcript,
            "{\"type\":\"session\",\"id\":\"omp-provider-1\"}\n{\"type\":\"message\",\"message\":{\"role\":\"assistant\",\"provider\":\"p\",\"model\":\"m\",\"usage\":{\"totalTokens\":10}}}\n",
        )
        .expect("write OMP transcript");
        let manifest = omp_manifest("worker-1");
        let mut event = HookEvent {
            session_id: "worker-1".into(),
            event_name: "Start".into(),
            tool_name: None,
            provider_session_id: Some("omp-provider-1".into()),
            provider_transcript_path: Some(transcript.to_string_lossy().into_owned()),
            runtime_generation: Some(1),
            received_at: SystemTime::UNIX_EPOCH,
        };

        update_provider_telemetry(&event, &manifest);
        assert_eq!(
            unpeel_core::session_telemetry::load("worker-1")
                .expect("telemetry after provider binding")
                .total_tokens,
            10
        );

        std::fs::OpenOptions::new()
            .append(true)
            .open(&transcript)
            .expect("open OMP transcript")
            .write_all(b"{\"type\":\"message\",\"message\":{\"role\":\"assistant\",\"provider\":\"p\",\"model\":\"m\",\"usage\":{\"totalTokens\":5}}}\n")
            .expect("append OMP transcript");
        event.event_name = "Stop".into();

        update_provider_telemetry(&event, &manifest);
        assert_eq!(
            unpeel_core::session_telemetry::load("worker-1")
                .expect("telemetry after Stop")
                .total_tokens,
            15
        );
    }

    #[test]
    fn startup_migrates_legacy_unbound_telemetry_from_provider_evidence() {
        let _lock = ENV_LOCK.lock().unwrap_or_else(|error| error.into_inner());
        let state_home = tempfile::tempdir().expect("temporary Unpeel home");
        let user_home = tempfile::tempdir().expect("temporary user home");
        let _home = UnpeelHomeGuard::set(state_home.path(), user_home.path());
        let session_dir = state_home.path().join("app-sessions/worker-1");
        std::fs::create_dir_all(&session_dir).expect("worker session directory");
        let omp_root = user_home.path().join(".omp/agent/sessions/project");
        std::fs::create_dir_all(&omp_root).expect("OMP Session root");
        let transcript = omp_root.join("provider.jsonl");
        std::fs::write(
            &transcript,
            "{\"type\":\"session\",\"id\":\"omp-provider-1\"}\n{\"type\":\"message\",\"message\":{\"role\":\"assistant\",\"provider\":\"p\",\"model\":\"m\",\"usage\":{\"totalTokens\":10}}}\n",
        )
        .expect("write OMP transcript");
        unpeel_core::session_ops::set_provider_session(
            "worker-1",
            Some("omp-provider-1"),
            Some(&transcript.to_string_lossy()),
        )
        .expect("persist provider binding");
        std::fs::write(
            session_dir.join("session-telemetry.json"),
            r#"{"totalTokens":1,"models":[{"model":"stale/model","totalTokens":1,"active":true}]}"#,
        )
        .expect("write legacy telemetry marker");

        let migrated = migrate_legacy_telemetry_for_manifest(&omp_manifest("worker-1"));

        assert!(migrated);
        let telemetry = unpeel_core::session_telemetry::load("worker-1")
            .expect("bound telemetry after startup migration");
        assert_eq!(telemetry.total_tokens, 10);
        assert_eq!(telemetry.models[0].model, "p/m");
    }

    #[test]
    fn provider_telemetry_hard_rejection_removes_current_marker() {
        let _lock = ENV_LOCK.lock().unwrap_or_else(|error| error.into_inner());
        let state_home = tempfile::tempdir().expect("temporary Unpeel home");
        let user_home = tempfile::tempdir().expect("temporary user home");
        let _home = UnpeelHomeGuard::set(state_home.path(), user_home.path());
        std::fs::create_dir_all(state_home.path().join("app-sessions/worker-1"))
            .expect("worker session directory");
        let omp_root = user_home.path().join(".omp/agent/sessions/project");
        std::fs::create_dir_all(&omp_root).expect("OMP Session root");
        let transcript = omp_root.join("provider.jsonl");
        std::fs::write(
            &transcript,
            "{\"type\":\"session\",\"id\":\"omp-provider-1\"}\n{\"type\":\"message\",\"message\":{\"role\":\"assistant\",\"provider\":\"p\",\"model\":\"m\",\"usage\":{\"totalTokens\":10}}}\n",
        )
        .expect("write valid OMP transcript");
        let manifest = omp_manifest("worker-1");
        let event = HookEvent {
            session_id: "worker-1".into(),
            event_name: "Stop".into(),
            tool_name: None,
            provider_session_id: Some("omp-provider-1".into()),
            provider_transcript_path: Some(transcript.to_string_lossy().into_owned()),
            runtime_generation: Some(1),
            received_at: SystemTime::UNIX_EPOCH,
        };
        update_provider_telemetry(&event, &manifest);
        assert!(unpeel_core::session_telemetry::load("worker-1").is_some());

        let padding = "x".repeat(1024 * 1024 - 32);
        let mut oversized = String::from("{\"type\":\"session\",\"id\":\"omp-provider-1\"}\n");
        for _ in 0..17 {
            oversized.push_str(&format!("{{\"padding\":\"{padding}\"}}\n"));
        }
        std::fs::write(&transcript, oversized).expect("write oversized OMP transcript");

        update_provider_telemetry(&event, &manifest);

        assert!(unpeel_core::session_telemetry::load("worker-1").is_none());
    }

    #[test]
    fn provider_telemetry_is_bound_to_the_canonical_transcript_path() {
        let _lock = ENV_LOCK.lock().unwrap_or_else(|error| error.into_inner());
        let state_home = tempfile::tempdir().expect("temporary Unpeel home");
        let user_home = tempfile::tempdir().expect("temporary user home");
        let _home = UnpeelHomeGuard::set(state_home.path(), user_home.path());
        std::fs::create_dir_all(state_home.path().join("app-sessions/worker-1"))
            .expect("worker session directory");
        let omp_root = user_home.path().join(".omp/agent/sessions/project");
        std::fs::create_dir_all(&omp_root).expect("OMP Session root");
        let first = omp_root.join("first.jsonl");
        let second = omp_root.join("second.jsonl");
        let body = "{\"type\":\"session\",\"id\":\"omp-provider-1\"}\n{\"type\":\"message\",\"message\":{\"role\":\"assistant\",\"provider\":\"p\",\"model\":\"m\",\"usage\":{\"totalTokens\":10}}}\n";
        std::fs::write(&first, body).expect("write first OMP transcript");
        std::fs::write(&second, body).expect("write second OMP transcript");
        let manifest = omp_manifest("worker-1");
        let first_event = HookEvent {
            session_id: "worker-1".into(),
            event_name: "Stop".into(),
            tool_name: None,
            provider_session_id: Some("omp-provider-1".into()),
            provider_transcript_path: Some(first.to_string_lossy().into_owned()),
            runtime_generation: Some(1),
            received_at: SystemTime::UNIX_EPOCH,
        };
        update_provider_telemetry(&first_event, &manifest);
        assert!(unpeel_core::session_telemetry::load("worker-1").is_some());

        unpeel_core::session_ops::set_provider_session(
            "worker-1",
            Some("omp-provider-1"),
            Some(&second.to_string_lossy()),
        )
        .expect("change provider transcript path");

        assert!(unpeel_core::session_telemetry::load("worker-1").is_none());
    }

    #[test]
    fn provider_binding_persist_failure_invalidates_previous_telemetry() {
        let _lock = ENV_LOCK.lock().unwrap_or_else(|error| error.into_inner());
        let state_home = tempfile::tempdir().expect("temporary Unpeel home");
        let user_home = tempfile::tempdir().expect("temporary user home");
        let _home = UnpeelHomeGuard::set(state_home.path(), user_home.path());
        let session_dir = state_home.path().join("app-sessions/worker-1");
        std::fs::create_dir_all(&session_dir).expect("worker session directory");
        let omp_root = user_home.path().join(".omp/agent/sessions/project");
        std::fs::create_dir_all(&omp_root).expect("OMP Session root");
        let first = omp_root.join("first.jsonl");
        let second = omp_root.join("second.jsonl");
        std::fs::write(
            &first,
            "{\"type\":\"session\",\"id\":\"omp-provider-1\"}\n{\"type\":\"message\",\"message\":{\"role\":\"assistant\",\"provider\":\"p\",\"model\":\"m\",\"usage\":{\"totalTokens\":10}}}\n",
        )
        .expect("write first OMP transcript");
        std::fs::write(
            &second,
            "{\"type\":\"session\",\"id\":\"omp-provider-2\"}\n{\"type\":\"message\",\"message\":{\"role\":\"assistant\",\"provider\":\"p\",\"model\":\"m\",\"usage\":{\"totalTokens\":20}}}\n",
        )
        .expect("write second OMP transcript");
        let manifest = omp_manifest("worker-1");
        let first_event = HookEvent {
            session_id: "worker-1".into(),
            event_name: "Stop".into(),
            tool_name: None,
            provider_session_id: Some("omp-provider-1".into()),
            provider_transcript_path: Some(first.to_string_lossy().into_owned()),
            runtime_generation: Some(1),
            received_at: SystemTime::UNIX_EPOCH,
        };
        update_provider_telemetry(&first_event, &manifest);
        assert!(unpeel_core::session_telemetry::load("worker-1").is_some());
        std::fs::create_dir(session_dir.join(".provider-session.json.tmp"))
            .expect("block provider binding temporary file");
        let second_event = HookEvent {
            provider_session_id: Some("omp-provider-2".into()),
            provider_transcript_path: Some(second.to_string_lossy().into_owned()),
            ..first_event
        };

        update_provider_telemetry(&second_event, &manifest);

        assert!(unpeel_core::session_telemetry::load("worker-1").is_none());
    }

    #[test]
    fn active_runtime_identity_uses_hooks_even_when_command_is_wrapped() {
        let directory = tempfile::tempdir().unwrap();
        std::fs::write(
            directory.path().join("last-hook-event.json"),
            r#"{"hook_event_name":"SessionStart"}"#,
        )
        .unwrap();
        let mut engine = ActivityEngine::default();
        let mut activity = input("s1", "custom-wrapper", directory.path(), 1);
        activity.active_runtime_id = Some("claude");

        assert_eq!(
            derive_activity(&mut engine, activity, SystemTime::now()),
            Some("idle")
        );
    }

    #[test]
    fn hook_start_and_stop_drive_the_same_spinner_state_as_unpeel() {
        let directory = tempfile::tempdir().unwrap();
        std::fs::write(
            directory.path().join("last-hook-event.json"),
            r#"{"hook_event_name":"Start"}"#,
        )
        .unwrap();
        let mut engine = ActivityEngine::default();
        let now = SystemTime::now();
        assert_eq!(
            derive_activity(&mut engine, input("s1", "claude", directory.path(), 1), now),
            Some("working")
        );
        engine.apply_hook_event("s1", "Stop", None, now);
        assert_eq!(
            derive_activity(&mut engine, input("s1", "claude", directory.path(), 1), now),
            Some("idle")
        );
    }

    #[test]
    fn hook_capable_runtime_stays_idle_before_first_hook_even_when_output_changes() {
        let directory = tempfile::tempdir().unwrap();
        let mut engine = ActivityEngine::default();
        let now = SystemTime::now();

        assert_eq!(
            derive_activity(&mut engine, input("s1", "claude", directory.path(), 1), now,),
            Some("idle")
        );
        assert_eq!(
            derive_activity(
                &mut engine,
                input("s1", "claude", directory.path(), 2),
                now + Duration::from_secs(1),
            ),
            Some("idle")
        );
    }

    #[test]
    fn session_start_latches_hooks_without_starting_the_spinner() {
        let directory = tempfile::tempdir().unwrap();
        std::fs::write(
            directory.path().join("last-hook-event.json"),
            r#"{"hook_event_name":"SessionStart"}"#,
        )
        .unwrap();
        let mut engine = ActivityEngine::default();

        assert_eq!(
            derive_activity(
                &mut engine,
                input("s1", "codex", directory.path(), 1),
                SystemTime::now(),
            ),
            Some("idle")
        );
    }

    #[test]
    fn hookless_runtime_preserves_the_authoritative_wire_activity() {
        let directory = tempfile::tempdir().unwrap();
        let mut engine = ActivityEngine::default();
        let now = SystemTime::now();
        // `agy` is output-driven by descriptor; `pi` now ships the pi-family
        // lifecycle extension and is hook-owned.
        assert_eq!(
            derive_activity(&mut engine, input("s1", "agy", directory.path(), 1), now),
            None
        );
        assert_eq!(
            derive_activity(&mut engine, input("s1", "agy", directory.path(), 2), now),
            None
        );
    }

    #[test]
    fn pi_family_workers_stop_spinning_immediately_after_agent_end() {
        for command in ["pi", "omp", "prime-agent"] {
            let directory = tempfile::tempdir().unwrap();
            let mut engine = ActivityEngine::default();
            let now = SystemTime::now();

            assert_eq!(
                derive_activity(
                    &mut engine,
                    input("session", command, directory.path(), 1),
                    now,
                ),
                Some("idle"),
            );
            engine.apply_hook_event("session", "Start", None, now);
            assert_eq!(
                derive_activity(
                    &mut engine,
                    input("session", command, directory.path(), 2),
                    now,
                ),
                Some("working"),
                "{command} must use its agent_start hook instead of the output fallback",
            );

            engine.apply_hook_event("session", "Stop", None, now);
            assert_eq!(
                derive_activity(
                    &mut engine,
                    input("session", command, directory.path(), 3),
                    now,
                ),
                Some("idle"),
                "{command} must stop immediately even when output changed at agent_end",
            );
        }
    }

    #[test]
    fn menu_prompt_replaces_spinner_with_attention() {
        let directory = tempfile::tempdir().unwrap();
        let mut engine = ActivityEngine::default();
        let now = SystemTime::now();
        let mut activity = input("s1", "pi", directory.path(), 1);
        activity.menu_prompt_active = true;
        assert_eq!(derive_activity(&mut engine, activity, now), Some("blocked"));
    }

    #[test]
    fn authoritative_unread_completion_survives_idle_hook_state() {
        assert_eq!(merge_derived_activity("done", true, "idle"), "done");
    }

    #[test]
    fn live_hook_state_replaces_non_terminal_wire_activity() {
        assert_eq!(
            merge_derived_activity("working", false, "blocked"),
            "blocked"
        );
        assert_eq!(merge_derived_activity("done", false, "idle"), "idle");
    }

    #[test]
    fn activity_timestamp_tracks_output_and_hook_progress() {
        assert_eq!(latest_activity_timestamp(100, Some(300), Some(200)), 300);
        assert_eq!(latest_activity_timestamp(400, Some(300), Some(200)), 400);
    }
}
