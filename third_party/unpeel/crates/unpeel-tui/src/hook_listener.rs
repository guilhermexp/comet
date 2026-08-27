//! Live hook-event ingestion, the TUI's half of the multi-instance hook
//! broadcast: provider hook scripts POST every lifecycle event to every port
//! in `~/.unpeel/app-ports`, so the TUI registers its own port there and runs
//! the same minimal HTTP contract as the native `HookServer.swift` — 200
//! `{"ok":true}` for sessions whose manifest exists in this home, 404
//! otherwise (foreign instances must not swallow events).

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::sync::mpsc::{Receiver, Sender};
use std::time::{Duration, SystemTime};

use std::sync::Arc;

use unpeel_core::app_paths;

use crate::approvals::{already_granted, persist_grant, ApprovalHub};

const APPROVAL_TIMEOUT: Duration = Duration::from_secs(125);

const IO_TIMEOUT: Duration = Duration::from_secs(5);
const MAX_BODY_BYTES: usize = 4 * 1024 * 1024;
const PORT_REGISTRY_CAP: usize = 16;

pub struct HookEventMessage {
    pub session_id: String,
    pub event_name: String,
    pub tool_name: Option<String>,
    /// Host-owned managed-runtime generation carried by current Unpeel hook
    /// assets. `None` is the compatibility shape from older installed hooks.
    pub runtime_generation: Option<u64>,
    /// Captured as soon as the complete HTTP request reached this listener.
    /// Main-loop scheduling must not make an old runtime's Stop look newer
    /// than an in-place replacement launch recorded in the manifest.
    pub received_at: SystemTime,
}

impl HookEventMessage {
    /// The cross-frontend "shared state changed" ping (`/state-changed`),
    /// carried on the hook channel rather than a second one: it means the
    /// same thing to the run loop — refresh now, don't wait for the poll.
    pub fn state_change(kind: &str) -> Self {
        Self {
            session_id: String::new(),
            event_name: format!("__state__:{kind}"),
            tool_name: None,
            runtime_generation: None,
            received_at: SystemTime::now(),
        }
    }

    pub fn is_state_change(&self) -> bool {
        self.event_name.starts_with("__state__:")
    }
}

fn runtime_generation_from_json(json: &serde_json::Value) -> Option<u64> {
    let value = json
        .get("unpeel_runtime_generation")
        .or_else(|| json.get("unpeelRuntimeGeneration"))?;
    value.as_u64()
}

pub struct HookListener {
    pub port: u16,
    pub events: Receiver<HookEventMessage>,
}

fn registry_path() -> std::path::PathBuf {
    app_paths::unpeel_home().join("app-ports")
}

fn read_registry_at(path: &std::path::Path) -> Vec<u16> {
    std::fs::read_to_string(path)
        .unwrap_or_default()
        .lines()
        .filter_map(|line| line.trim().parse::<u16>().ok())
        .collect()
}

fn write_registry_at(path: &std::path::Path, ports: &[u16]) {
    if ports.is_empty() {
        let _ = std::fs::remove_file(path);
        return;
    }
    let body = ports
        .iter()
        .map(|p| p.to_string())
        .collect::<Vec<_>>()
        .join("\n")
        + "\n";
    let Some(parent) = path.parent() else { return };
    if std::fs::create_dir_all(parent).is_err() {
        return;
    }
    let tmp = parent.join(format!(
        ".app-ports.{}.{}.tmp",
        std::process::id(),
        uuid::Uuid::new_v4()
    ));
    let written = (|| -> std::io::Result<()> {
        let mut file = std::fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .mode(0o600)
            .open(&tmp)?;
        file.write_all(body.as_bytes())?;
        file.sync_all()?;
        std::fs::rename(&tmp, path)?;
        Ok(())
    })();
    if written.is_err() {
        let _ = std::fs::remove_file(tmp);
    }
}

fn update_registry_at(path: &std::path::Path, port: u16, register: bool) {
    let Some(parent) = path.parent() else { return };
    if std::fs::create_dir_all(parent).is_err() {
        return;
    }
    let lock_path = parent.join("app-ports.lock");
    let Ok(lock) = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .mode(0o600)
        .open(lock_path)
    else {
        return;
    };
    let _ = std::fs::set_permissions(
        parent.join("app-ports.lock"),
        std::fs::Permissions::from_mode(0o600),
    );
    if unsafe { libc::flock(std::os::fd::AsRawFd::as_raw_fd(&lock), libc::LOCK_EX) } != 0 {
        return;
    }
    let mut ports: Vec<u16> = read_registry_at(path)
        .into_iter()
        .filter(|existing| *existing != port)
        .collect();
    if register {
        ports.push(port);
        if ports.len() > PORT_REGISTRY_CAP {
            ports.drain(..ports.len() - PORT_REGISTRY_CAP);
        }
    }
    write_registry_at(path, &ports);
    unsafe {
        libc::flock(std::os::fd::AsRawFd::as_raw_fd(&lock), libc::LOCK_UN);
    }
}

/// Same semantics as `HookServer.registerPort`: dedupe, append last (newest),
/// cap at 16 by dropping oldest.
fn register_port(port: u16) {
    update_registry_at(&registry_path(), port, true);
}

pub fn unregister_port(port: u16) {
    update_registry_at(&registry_path(), port, false);
}

fn respond(stream: &mut TcpStream, status: &str, body: &str) {
    let _ = write!(
        stream,
        "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nX-Unpeel-Frontend: tui\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
}

/// `/mcp/approve-*` from the MCP host (which found our port in the
/// registry because no app is running). Auth: the shared x-unpeel-auth
/// token; blocking until the user answers in the TUI or from a phone.
fn handle_mcp(
    stream: &mut TcpStream,
    path: &str,
    headers: &std::collections::HashMap<String, String>,
    body: &[u8],
    hub: &Arc<ApprovalHub>,
) {
    if !unpeel_core::mcp_auth::verify_auth(headers.get("x-unpeel-auth").map(String::as_str)) {
        respond(stream, "401 Unauthorized", r#"{"error":"unauthorized"}"#);
        return;
    }
    let json: serde_json::Value = serde_json::from_slice(body).unwrap_or(serde_json::Value::Null);
    let field = |key: &str| {
        json.get(key)
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_owned)
    };
    let approve = |ok: bool, stream: &mut TcpStream| {
        respond(
            stream,
            "200 OK",
            &serde_json::json!({ "approved": ok }).to_string(),
        );
    };
    match path {
        "/mcp/approve-write" => {
            let (Some(caller), Some(target)) =
                (field("caller_session_id"), field("target_session_id"))
            else {
                respond(
                    stream,
                    "400 Bad Request",
                    r#"{"error":"caller_session_id and target_session_id are required"}"#,
                );
                return;
            };
            if already_granted("write", &caller, Some(&target)) {
                approve(true, stream);
                return;
            }
            let ok = hub.request(
                "write",
                format!(
                    "Allow session {} to write to {}?",
                    &caller[..8.min(caller.len())],
                    &target[..8.min(target.len())]
                ),
                format!("{caller} → {target}"),
                caller.clone(),
                Some(target.clone()),
                APPROVAL_TIMEOUT,
            );
            if ok {
                persist_grant("write", &caller, Some(&target));
            }
            approve(ok, stream);
        }
        "/mcp/approve-browser" | "/mcp/approve-computer" => {
            let kind = if path.ends_with("browser") {
                "browser"
            } else {
                "computer"
            };
            let Some(session_id) = field("session_id") else {
                respond(
                    stream,
                    "400 Bad Request",
                    r#"{"error":"session_id is required"}"#,
                );
                return;
            };
            if already_granted(kind, &session_id, None) {
                approve(true, stream);
                return;
            }
            let ok = hub.request(
                kind,
                format!(
                    "Allow {kind} access for session {}?",
                    &session_id[..8.min(session_id.len())]
                ),
                session_id.clone(),
                session_id.clone(),
                None,
                APPROVAL_TIMEOUT,
            );
            if ok {
                persist_grant(kind, &session_id, None);
            }
            approve(ok, stream);
        }
        "/mcp/computer-permissions-needed" => {
            respond(stream, "200 OK", r#"{"ok":true}"#);
        }
        _ => respond(stream, "404 Not Found", r#"{"error":"not found"}"#),
    }
}

fn handle_connection(
    mut stream: TcpStream,
    events: &Sender<HookEventMessage>,
    hub: &Arc<ApprovalHub>,
) {
    let _ = stream.set_read_timeout(Some(IO_TIMEOUT));
    let _ = stream.set_write_timeout(Some(IO_TIMEOUT));

    let mut buffer = Vec::new();
    let mut chunk = [0u8; 16 * 1024];
    let header_end;
    loop {
        match stream.read(&mut chunk) {
            Ok(0) => return,
            Ok(n) => buffer.extend_from_slice(&chunk[..n]),
            Err(_) => return,
        }
        if let Some(pos) = buffer.windows(4).position(|w| w == b"\r\n\r\n") {
            header_end = pos + 4;
            break;
        }
        if buffer.len() > MAX_BODY_BYTES {
            return;
        }
    }

    let head = String::from_utf8_lossy(&buffer[..header_end]).into_owned();
    let mut lines = head.lines();
    let request_line = lines.next().unwrap_or_default();
    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or_default();
    let path = parts.next().unwrap_or_default();

    if method != "POST" {
        respond(
            &mut stream,
            "405 Method Not Allowed",
            r#"{"error":"method not allowed"}"#,
        );
        return;
    }
    let is_mcp = path.starts_with("/mcp/");
    // The cross-frontend change ping: another Unpeel wrote shared state and
    // is telling us to re-read it now rather than on our next poll. Carries
    // no session id.
    let is_state = path == unpeel_core::state_bus::ROUTE;
    let session_id = if is_mcp || is_state {
        String::new()
    } else {
        match path.strip_prefix("/hook/").filter(|id| !id.is_empty()) {
            Some(id) if !id.contains('/') && !id.contains("..") => id.to_string(),
            _ => {
                respond(&mut stream, "404 Not Found", r#"{"error":"not found"}"#);
                return;
            }
        }
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
            Ok(n) => body.extend_from_slice(&chunk[..n]),
            Err(_) => return,
        }
    }
    // Timestamp receipt before JSON decoding, provider metadata persistence,
    // or Main-loop queueing can delay delivery. Restart-generation cutoffs
    // compare against when the old provider actually reached this socket.
    let received_at = SystemTime::now();

    if is_state {
        let kind = serde_json::from_slice::<serde_json::Value>(&body)
            .ok()
            .and_then(|value| value.get("change")?.as_str().map(str::to_owned))
            .unwrap_or_else(|| "unknown".into());
        let _ = events.send(HookEventMessage::state_change(&kind));
        respond(&mut stream, "200 OK", r#"{"ok":true}"#);
        return;
    }

    if is_mcp {
        let mut headers = std::collections::HashMap::new();
        for line in head.lines().skip(1) {
            if let Some((name, value)) = line.split_once(':') {
                headers.insert(name.trim().to_lowercase(), value.trim().to_string());
            }
        }
        let path = path.to_string();
        handle_mcp(&mut stream, &path, &headers, &body, hub);
        return;
    }
    let Ok(json) = serde_json::from_slice::<serde_json::Value>(&body) else {
        respond(
            &mut stream,
            "400 Bad Request",
            r#"{"error":"invalid json"}"#,
        );
        return;
    };
    let event_name = json
        .get("hook_event_name")
        .or_else(|| json.get("hookEventName"))
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty());
    let Some(event_name) = event_name else {
        respond(
            &mut stream,
            "400 Bad Request",
            r#"{"error":"missing hook_event_name"}"#,
        );
        return;
    };

    let runtime_generation = runtime_generation_from_json(&json);

    let Some(manifest) = unpeel_core::session_host::load_manifest(&session_id) else {
        respond(
            &mut stream,
            "404 Not Found",
            r#"{"error":"unknown session"}"#,
        );
        return;
    };
    if runtime_generation.is_some_and(|generation| generation < manifest.runtime_launch_generation)
    {
        // A departed runtime can finish a background hook after Resume Agent
        // has committed its replacement. Acknowledge it so providers do not
        // retry, but never let it overwrite conversation metadata or enter the
        // activity queue for the new generation.
        respond(&mut stream, "200 OK", r#"{"ok":true}"#);
        return;
    }

    // Capture provider conversation metadata into the shared marker — the
    // same key candidates the app's HookServer accepts, so a session's
    // resume id lands on disk whichever frontend received the broadcast.
    let first_string = |keys: &[&str]| {
        keys.iter().find_map(|key| {
            json.get(*key)
                .and_then(|v| v.as_str())
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_owned)
        })
    };
    let provider_id = first_string(&[
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
    ]);
    let transcript = first_string(&[
        "transcript_path",
        "transcriptPath",
        "provider_transcript_path",
        "providerTranscriptPath",
    ]);
    if provider_id.is_some() || transcript.is_some() {
        let changed = unpeel_core::session_ops::set_provider_session(
            &session_id,
            provider_id.as_deref(),
            transcript.as_deref(),
        )
        .unwrap_or(false);
        if changed {
            // The session's conversation identity moved (in-tool /resume or
            // /clear): if it is still untitled, title it from the resumed
            // conversation's transcript. Off-thread — this reads provider
            // storage and must not stall the hook listener.
            let session_id = session_id.clone();
            std::thread::spawn(move || {
                let _ = unpeel_core::transcripts::auto_title_session_from_transcript(&session_id);
            });
        }
    }

    let _ = events.send(HookEventMessage {
        session_id: session_id.to_string(),
        event_name: event_name.to_string(),
        tool_name: json
            .get("tool_name")
            .and_then(|v| v.as_str())
            .map(str::to_owned),
        runtime_generation,
        received_at,
    });
    respond(&mut stream, "200 OK", r#"{"ok":true}"#);
}

pub fn start(hub: Arc<ApprovalHub>) -> Result<HookListener, String> {
    let listener =
        TcpListener::bind("127.0.0.1:0").map_err(|e| format!("hook listener bind: {e}"))?;
    let port = listener.local_addr().map_err(|e| e.to_string())?.port();
    register_port(port);

    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let tx = tx.clone();
            let hub = Arc::clone(&hub);
            std::thread::spawn(move || handle_connection(stream, &tx, &hub));
        }
    });

    Ok(HookListener { port, events: rx })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn concurrent_registry_updates_preserve_every_frontend() {
        let dir =
            std::env::temp_dir().join(format!("unpeel-app-ports-race-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("app-ports");
        let barrier = Arc::new(std::sync::Barrier::new(8));
        let mut workers = Vec::new();
        for port in 41_000..41_008 {
            let path = path.clone();
            let barrier = Arc::clone(&barrier);
            workers.push(std::thread::spawn(move || {
                barrier.wait();
                update_registry_at(&path, port, true);
            }));
        }
        for worker in workers {
            worker.join().unwrap();
        }

        let mut ports = read_registry_at(&path);
        ports.sort_unstable();
        assert_eq!(ports, (41_000..41_008).collect::<Vec<_>>());
        assert_eq!(
            std::fs::metadata(dir.join("app-ports.lock"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
        assert_eq!(
            std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn parses_numeric_runtime_generation_and_legacy_shapes() {
        assert_eq!(
            runtime_generation_from_json(&serde_json::json!({"unpeel_runtime_generation": 7})),
            Some(7)
        );
        assert_eq!(
            runtime_generation_from_json(&serde_json::json!({"unpeelRuntimeGeneration": 8})),
            Some(8)
        );
        assert_eq!(
            runtime_generation_from_json(&serde_json::json!({"unpeel_runtime_generation": "8"})),
            None
        );
        assert_eq!(
            runtime_generation_from_json(&serde_json::json!({"hook_event_name": "Stop"})),
            None
        );
    }
}
