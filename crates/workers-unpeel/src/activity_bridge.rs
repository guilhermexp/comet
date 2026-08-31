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
            if let Some(derived) = derived {
                session.activity =
                    merge_derived_activity(&session.activity, session.unread, derived).to_owned();
            }
        }
    }

    pub(crate) fn clear_attention(&self, session_id: &str) {
        let mut engine = self.engine.lock().unwrap_or_else(|lock| lock.into_inner());
        engine.apply_hook_event(session_id, "Stop", None, SystemTime::now());
        self.change_epoch.fetch_add(1, Ordering::Release);
    }
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

fn persist_provider_binding(event: &HookEvent) -> bool {
    unpeel_core::session_ops::set_provider_session(
        &event.session_id,
        event.provider_session_id.as_deref(),
        event.provider_transcript_path.as_deref(),
    )
    .unwrap_or(false)
}

fn update_provider_telemetry(
    event: &HookEvent,
    manifest: &unpeel_core::session_host::HostedSessionManifest,
) {
    let binding_changed = persist_provider_binding(event);
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

        assert!(persist_provider_binding(&event));
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

    #[test]
    fn provider_telemetry_refreshes_on_binding_change_and_stop() {
        let _lock = ENV_LOCK.lock().expect("environment test lock");
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
            "{\"type\":\"message\",\"message\":{\"role\":\"assistant\",\"provider\":\"p\",\"model\":\"m\",\"usage\":{\"totalTokens\":10}}}\n",
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
        assert_eq!(
            derive_activity(&mut engine, input("s1", "pi", directory.path(), 1), now),
            None
        );
        assert_eq!(
            derive_activity(&mut engine, input("s1", "pi", directory.path(), 2), now),
            None
        );
    }

    #[test]
    fn pi_family_workers_stop_spinning_immediately_after_agent_end() {
        for command in ["omp", "prime-agent"] {
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
