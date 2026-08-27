use std::fs::OpenOptions;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
#[cfg(unix)]
use std::os::fd::AsRawFd;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::JoinHandle;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use unpeel_core::session_host::SessionHostLaunch;

pub(crate) const JOURNAL_FILE: &str = "comet-hook-events.jsonl";
const MAX_BODY_BYTES: usize = 256 * 1024;
const IO_TIMEOUT: Duration = Duration::from_secs(3);
#[cfg(not(test))]
const SNAPSHOT_RECONCILE_GRACE: Duration = Duration::from_secs(2);
#[cfg(test)]
const SNAPSHOT_RECONCILE_GRACE: Duration = Duration::from_millis(20);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct SessionHookJournalEntry {
    pub sequence: u64,
    pub hook_event_name: String,
    #[serde(default)]
    pub tool_name: Option<String>,
    #[serde(default)]
    pub runtime_generation: Option<u64>,
    #[serde(default)]
    pub task_episode: Option<u64>,
    pub occurred_at_unix_ms: u64,
    #[serde(default)]
    pub source_modified_unix_ns: Option<u128>,
}

pub(crate) struct SessionHookJournalIngress {
    port: u16,
    shutdown: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl Drop for SessionHookJournalIngress {
    fn drop(&mut self) {
        self.shutdown.store(true, Ordering::Release);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

pub(crate) fn read_entries(
    session_dir: &Path,
) -> Result<Option<Vec<SessionHookJournalEntry>>, String> {
    let path = session_dir.join(JOURNAL_FILE);
    let raw = match std::fs::read_to_string(&path) {
        Ok(raw) => raw,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(format!("Failed to read {}: {error}", path.display())),
    };
    let mut entries = Vec::new();
    let lines = raw.lines().collect::<Vec<_>>();
    for (index, line) in lines.iter().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let entry = match serde_json::from_str(line) {
            Ok(entry) => entry,
            Err(_) if index + 1 == lines.len() => break,
            Err(error) => {
                return Err(format!(
                    "Invalid worker hook journal entry {} at line {}: {error}",
                    path.display(),
                    index + 1
                ));
            }
        };
        entries.push(entry);
    }
    Ok(Some(entries))
}

pub(crate) fn install_for_session_host(
    host_args: &[String],
) -> Result<SessionHookJournalIngress, String> {
    let [launch_path] = host_args else {
        return Err("Missing launch file path for session host".into());
    };
    let raw = std::fs::read(launch_path)
        .map_err(|error| format!("Failed to read launch file for hook journal: {error}"))?;
    let mut launch: SessionHostLaunch = serde_json::from_slice(&raw)
        .map_err(|error| format!("Invalid launch file for hook journal: {error}"))?;
    // Generic provider hooks honor this switch and do not return until the
    // host-owned endpoint has fsynced the event. Provider-specific hooks that
    // still post in the background are reconciled from last-hook-event below.
    unsafe { std::env::set_var("UNPEEL_HOOK_POST_SYNC", "1") };
    let session_dir = unpeel_core::session_host::session_dir(&launch.session.id);
    let ingress = start(&launch.session.id, &session_dir)?;
    launch.hook_port = Some(ingress.port);
    rewrite_launch(Path::new(launch_path), &launch)?;
    Ok(ingress)
}

fn rewrite_launch(path: &Path, launch: &SessionHostLaunch) -> Result<(), String> {
    let bytes = serde_json::to_vec(launch).map_err(|error| error.to_string())?;
    let temporary = path.with_extension(format!("{}.tmp", std::process::id()));
    std::fs::write(&temporary, bytes)
        .map_err(|error| format!("Failed to stage session launch hook endpoint: {error}"))?;
    std::fs::rename(&temporary, path)
        .map_err(|error| format!("Failed to publish session launch hook endpoint: {error}"))
}

fn start(session_id: &str, session_dir: &Path) -> Result<SessionHookJournalIngress, String> {
    std::fs::create_dir_all(session_dir)
        .map_err(|error| format!("Failed to create worker session journal directory: {error}"))?;
    let listener = TcpListener::bind("127.0.0.1:0")
        .map_err(|error| format!("Failed to bind worker hook journal: {error}"))?;
    listener
        .set_nonblocking(true)
        .map_err(|error| format!("Failed to configure worker hook journal: {error}"))?;
    let port = listener
        .local_addr()
        .map_err(|error| error.to_string())?
        .port();
    let path = session_dir.join(JOURNAL_FILE);
    repair_torn_tail(&path)?;
    let next_sequence = read_entries(session_dir)?
        .unwrap_or_default()
        .last()
        .map(|entry| entry.sequence.saturating_add(1))
        .unwrap_or(1);
    let expected_session_id = session_id.to_owned();
    let shutdown = Arc::new(AtomicBool::new(false));
    let thread_shutdown = Arc::clone(&shutdown);
    let thread = std::thread::spawn(move || {
        let mut sequence = next_sequence;
        let mut last_entry = read_entries(path.parent().unwrap_or(Path::new(".")))
            .ok()
            .flatten()
            .and_then(|entries| entries.last().cloned());
        let mut reconciled_snapshot_identity = None;
        while !thread_shutdown.load(Ordering::Acquire) {
            if reconcile_last_hook_snapshot(
                &path,
                &mut sequence,
                &mut last_entry,
                &mut reconciled_snapshot_identity,
            )
            .is_ok()
            {
                // The snapshot is a recovery seed for provider hooks that post
                // asynchronously. HTTP delivery below dedupes against it.
            }
            match listener.accept() {
                Ok((stream, _)) => {
                    if let Ok(Some(entry)) = handle_connection(
                        stream,
                        &expected_session_id,
                        &path,
                        sequence,
                        last_entry.as_ref(),
                    ) {
                        sequence = sequence.saturating_add(1);
                        last_entry = Some(entry);
                    }
                }
                Err(error) if accept_error_is_retryable(error.kind()) => {
                    std::thread::sleep(Duration::from_millis(10));
                }
                Err(_) => break,
            }
        }
    });
    Ok(SessionHookJournalIngress {
        port,
        shutdown,
        thread: Some(thread),
    })
}

fn accept_error_is_retryable(kind: std::io::ErrorKind) -> bool {
    matches!(
        kind,
        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::Interrupted
    )
}

fn repair_torn_tail(path: &Path) -> Result<(), String> {
    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(format!("Failed to inspect worker hook journal: {error}")),
    };
    if bytes.is_empty() || bytes.ends_with(b"\n") {
        return Ok(());
    }
    let retained_len = bytes
        .iter()
        .rposition(|byte| *byte == b'\n')
        .map(|index| index + 1)
        .unwrap_or(0);
    let file = OpenOptions::new()
        .write(true)
        .open(path)
        .map_err(|error| format!("Failed to repair worker hook journal: {error}"))?;
    file.set_len(retained_len as u64)
        .map_err(|error| format!("Failed to truncate torn worker hook journal tail: {error}"))?;
    file.sync_data()
        .map_err(|error| format!("Failed to persist worker hook journal repair: {error}"))
}

fn handle_connection(
    mut stream: TcpStream,
    expected_session_id: &str,
    journal_path: &Path,
    sequence: u64,
    last_entry: Option<&SessionHookJournalEntry>,
) -> Result<Option<SessionHookJournalEntry>, String> {
    stream.set_read_timeout(Some(IO_TIMEOUT)).ok();
    stream.set_write_timeout(Some(IO_TIMEOUT)).ok();
    let (path, body) = read_request(&mut stream)?;
    if path != format!("/hook/{expected_session_id}") {
        respond(&mut stream, "404 Not Found", r#"{"error":"not found"}"#);
        return Err("hook session did not match host".into());
    }
    let value: serde_json::Value = serde_json::from_slice(&body)
        .map_err(|error| format!("Invalid hook journal payload: {error}"))?;
    let hook_event_name = value
        .get("hook_event_name")
        .or_else(|| value.get("hookEventName"))
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .ok_or_else(|| "Hook journal payload is missing hook_event_name".to_string())?;
    let runtime_generation = value
        .get("unpeel_runtime_generation")
        .or_else(|| value.get("unpeelRuntimeGeneration"))
        .and_then(serde_json::Value::as_u64);
    let task_episode = value
        .get("comet_task_episode")
        .or_else(|| value.get("cometTaskEpisode"))
        .and_then(serde_json::Value::as_u64);
    let current_snapshot = last_hook_snapshot(journal_path.parent().unwrap_or(Path::new(".")))
        .ok()
        .flatten();
    if let (Some(last), Some(snapshot)) = (last_entry, current_snapshot.as_ref())
        && last.source_modified_unix_ns.is_some()
        && last.source_modified_unix_ns == snapshot.source_modified_unix_ns
        && snapshot.hook_event_name == hook_event_name
        && snapshot.runtime_generation == runtime_generation
        && snapshot.task_episode == task_episode
    {
        // The snapshot-backed record was already fsynced (and may already be
        // acknowledged) before this provider's background curl arrived.
        respond(&mut stream, "200 OK", r#"{"ok":true}"#);
        return Ok(None);
    }
    let entry = SessionHookJournalEntry {
        sequence,
        hook_event_name: hook_event_name.to_owned(),
        tool_name: value
            .get("tool_name")
            .or_else(|| value.get("toolName"))
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned),
        runtime_generation,
        task_episode,
        occurred_at_unix_ms: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64,
        source_modified_unix_ns: None,
    };
    append_entry(journal_path, &entry)?;
    respond(&mut stream, "200 OK", r#"{"ok":true}"#);
    Ok(Some(entry))
}

fn reconcile_last_hook_snapshot(
    journal_path: &Path,
    sequence: &mut u64,
    last_entry: &mut Option<SessionHookJournalEntry>,
    reconciled_snapshot_identity: &mut Option<u128>,
) -> Result<(), String> {
    let Some(mut snapshot) = last_hook_snapshot(journal_path.parent().unwrap_or(Path::new(".")))?
    else {
        return Ok(());
    };
    let identity = snapshot.source_modified_unix_ns;
    if identity.is_some() && *reconciled_snapshot_identity == identity {
        return Ok(());
    }
    let now_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;
    if now_ms.saturating_sub(snapshot.occurred_at_unix_ms)
        < SNAPSHOT_RECONCILE_GRACE.as_millis() as u64
    {
        return Ok(());
    }
    if last_entry.as_ref().is_some_and(|entry| {
        entry.hook_event_name == snapshot.hook_event_name
            && entry.runtime_generation == snapshot.runtime_generation
            && entry.task_episode == snapshot.task_episode
            && entry.occurred_at_unix_ms >= snapshot.occurred_at_unix_ms
    }) {
        *reconciled_snapshot_identity = identity;
        return Ok(());
    }
    snapshot.sequence = *sequence;
    append_entry(journal_path, &snapshot)?;
    *sequence = sequence.saturating_add(1);
    *last_entry = Some(snapshot);
    *reconciled_snapshot_identity = identity;
    Ok(())
}

fn last_hook_snapshot(session_dir: &Path) -> Result<Option<SessionHookJournalEntry>, String> {
    let path = session_dir.join("last-hook-event.json");
    let mut file = match std::fs::File::open(&path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.to_string()),
    };
    let modified = file
        .metadata()
        .map_err(|error| error.to_string())?
        .modified()
        .map_err(|error| error.to_string())?
        .duration_since(UNIX_EPOCH)
        .map_err(|error| error.to_string())?;
    let mut raw = Vec::new();
    file.read_to_end(&mut raw)
        .map_err(|error| error.to_string())?;
    let value: serde_json::Value =
        serde_json::from_slice(&raw).map_err(|error| error.to_string())?;
    let Some(hook_event_name) = value
        .get("hook_event_name")
        .and_then(serde_json::Value::as_str)
    else {
        return Ok(None);
    };
    Ok(Some(SessionHookJournalEntry {
        sequence: 0,
        hook_event_name: hook_event_name.to_owned(),
        tool_name: value
            .get("tool_name")
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned),
        runtime_generation: value
            .get("unpeel_runtime_generation")
            .or_else(|| value.get("unpeelRuntimeGeneration"))
            .and_then(serde_json::Value::as_u64),
        task_episode: value
            .get("comet_task_episode")
            .or_else(|| value.get("cometTaskEpisode"))
            .and_then(serde_json::Value::as_u64),
        occurred_at_unix_ms: modified.as_millis() as u64,
        source_modified_unix_ns: Some(modified.as_nanos()),
    }))
}

pub(crate) fn compact_to_latest(session_dir: &Path) -> Result<(), String> {
    with_journal_lock(session_dir, || {
        let Some(entries) = read_entries(session_dir)? else {
            return Ok(());
        };
        let Some(latest) = entries.last() else {
            return Ok(());
        };
        let path = session_dir.join(JOURNAL_FILE);
        let mut encoded = serde_json::to_vec(latest).map_err(|error| error.to_string())?;
        encoded.push(b'\n');
        let temporary = session_dir.join(format!(".{JOURNAL_FILE}.{}.tmp", std::process::id()));
        std::fs::write(&temporary, encoded).map_err(|error| error.to_string())?;
        std::fs::rename(&temporary, path).map_err(|error| error.to_string())
    })
}

fn append_entry(path: &Path, entry: &SessionHookJournalEntry) -> Result<(), String> {
    let mut encoded = serde_json::to_vec(entry).map_err(|error| error.to_string())?;
    encoded.push(b'\n');
    with_journal_lock(path.parent().unwrap_or(Path::new(".")), || {
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .map_err(|error| format!("Failed to open worker hook journal: {error}"))?;
        file.write_all(&encoded)
            .map_err(|error| error.to_string())?;
        file.sync_data()
            .map_err(|error| format!("Failed to persist worker hook journal: {error}"))
    })
}

fn with_journal_lock<T>(
    session_dir: &Path,
    operation: impl FnOnce() -> Result<T, String>,
) -> Result<T, String> {
    let lock = OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .open(session_dir.join("comet-hook-events.lock"))
        .map_err(|error| format!("Failed to open worker hook journal lock: {error}"))?;
    #[cfg(unix)]
    if unsafe { libc::flock(lock.as_raw_fd(), libc::LOCK_EX) } != 0 {
        return Err(format!(
            "Failed to lock worker hook journal: {}",
            std::io::Error::last_os_error()
        ));
    }
    let result = operation();
    #[cfg(unix)]
    unsafe {
        libc::flock(lock.as_raw_fd(), libc::LOCK_UN);
    }
    result
}

fn read_request(stream: &mut TcpStream) -> Result<(String, Vec<u8>), String> {
    let mut buffer = Vec::new();
    let mut chunk = [0u8; 8192];
    let header_end = loop {
        let read = stream.read(&mut chunk).map_err(|error| error.to_string())?;
        if read == 0 {
            return Err("Hook journal request ended before headers".into());
        }
        buffer.extend_from_slice(&chunk[..read]);
        if let Some(position) = buffer.windows(4).position(|window| window == b"\r\n\r\n") {
            break position + 4;
        }
        if buffer.len() > MAX_BODY_BYTES {
            return Err("Hook journal request headers are too large".into());
        }
    };
    let head = String::from_utf8_lossy(&buffer[..header_end]);
    let mut lines = head.lines();
    let mut request = lines.next().unwrap_or_default().split_whitespace();
    if request.next() != Some("POST") {
        return Err("Hook journal only accepts POST".into());
    }
    let path = request.next().unwrap_or_default().to_owned();
    let content_length = lines
        .find_map(|line| {
            let (name, value) = line.split_once(':')?;
            name.trim()
                .eq_ignore_ascii_case("content-length")
                .then(|| value.trim().parse::<usize>().ok())?
        })
        .unwrap_or(0);
    if content_length > MAX_BODY_BYTES {
        return Err("Hook journal request body is too large".into());
    }
    let mut body = buffer[header_end..].to_vec();
    while body.len() < content_length {
        let read = stream.read(&mut chunk).map_err(|error| error.to_string())?;
        if read == 0 {
            break;
        }
        body.extend_from_slice(&chunk[..read]);
    }
    body.truncate(content_length);
    Ok((path, body))
}

fn respond(stream: &mut TcpStream, status: &str, body: &str) {
    let _ = write!(
        stream,
        "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    let _ = stream.flush();
    let _ = stream.shutdown(std::net::Shutdown::Write);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// WouldBlock e Interrupted sao estados transitórios do accept loop; tirar
    /// qualquer um derruba o endpoint sob polling ou sinais de filhos.
    #[test]
    fn ingress_retries_transient_accept_errors() {
        assert!(accept_error_is_retryable(std::io::ErrorKind::WouldBlock));
        assert!(accept_error_is_retryable(std::io::ErrorKind::Interrupted));
        assert!(!accept_error_is_retryable(
            std::io::ErrorKind::ConnectionAborted
        ));
    }

    #[test]
    fn ingress_appends_every_hook_episode_in_order() {
        let directory = tempfile::tempdir().unwrap();
        let ingress = start("worker-1", directory.path()).unwrap();
        for event in ["Start", "Stop", "Stop"] {
            let body = format!(
                r#"{{"hook_event_name":"{event}","unpeel_runtime_generation":7,"comet_task_episode":3}}"#
            );
            let mut stream = TcpStream::connect(("127.0.0.1", ingress.port)).unwrap();
            write!(
                stream,
                "POST /hook/worker-1 HTTP/1.1\r\nContent-Length: {}\r\n\r\n{body}",
                body.len()
            )
            .unwrap();
            stream.shutdown(std::net::Shutdown::Write).unwrap();
            let mut response = String::new();
            stream.read_to_string(&mut response).unwrap();
            assert!(response.contains("200 OK"));
        }
        drop(ingress);
        let entries = read_entries(directory.path()).unwrap().unwrap();
        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0].sequence, 1);
        assert_eq!(entries[1].hook_event_name, "Stop");
        assert_eq!(entries[2].sequence, 3);
        assert!(entries.iter().all(|entry| entry.task_episode == Some(3)));
    }

    #[test]
    fn ingress_continues_sequence_after_restart() {
        let directory = tempfile::tempdir().unwrap();
        append_entry(
            &directory.path().join(JOURNAL_FILE),
            &SessionHookJournalEntry {
                sequence: 9,
                hook_event_name: "Start".into(),
                tool_name: None,
                runtime_generation: Some(2),
                task_episode: None,
                occurred_at_unix_ms: 1,
                source_modified_unix_ns: None,
            },
        )
        .unwrap();
        let ingress = start("worker-1", directory.path()).unwrap();
        let body = r#"{"hook_event_name":"Stop","unpeel_runtime_generation":2}"#;
        let mut stream = TcpStream::connect(("127.0.0.1", ingress.port)).unwrap();
        write!(
            stream,
            "POST /hook/worker-1 HTTP/1.1\r\nContent-Length: {}\r\n\r\n{body}",
            body.len()
        )
        .unwrap();
        stream.shutdown(std::net::Shutdown::Write).unwrap();
        stream.read_to_end(&mut Vec::new()).unwrap();
        drop(ingress);
        let entries = read_entries(directory.path()).unwrap().unwrap();
        assert_eq!(entries.last().unwrap().sequence, 10);
    }

    #[test]
    fn reader_ignores_a_torn_trailing_record_and_keeps_the_last_sequence() {
        let directory = tempfile::tempdir().unwrap();
        append_entry(
            &directory.path().join(JOURNAL_FILE),
            &SessionHookJournalEntry {
                sequence: 4,
                hook_event_name: "Start".into(),
                tool_name: None,
                runtime_generation: Some(1),
                task_episode: None,
                occurred_at_unix_ms: 1,
                source_modified_unix_ns: None,
            },
        )
        .unwrap();
        let mut file = OpenOptions::new()
            .append(true)
            .open(directory.path().join(JOURNAL_FILE))
            .unwrap();
        file.write_all(br#"{"sequence":5,"hook_event"#).unwrap();
        drop(file);

        let entries = read_entries(directory.path()).unwrap().unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].sequence, 4);
        let ingress = start("worker-1", directory.path()).unwrap();
        assert!(ingress.port > 0);
        drop(ingress);
        let repaired = std::fs::read(directory.path().join(JOURNAL_FILE)).unwrap();
        assert!(repaired.ends_with(b"\n"));
        assert!(!String::from_utf8_lossy(&repaired).contains("\"sequence\":5"));
    }

    #[test]
    fn host_reconciles_a_snapshot_when_async_http_delivery_is_lost() {
        let directory = tempfile::tempdir().unwrap();
        std::fs::write(
            directory.path().join("last-hook-event.json"),
            br#"{"hook_event_name":"Stop","unpeel_runtime_generation":3}"#,
        )
        .unwrap();
        let ingress = start("worker-1", directory.path()).unwrap();
        std::thread::sleep(Duration::from_millis(30));
        drop(ingress);
        let entries = read_entries(directory.path()).unwrap().unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].hook_event_name, "Stop");
        assert_eq!(entries[0].runtime_generation, Some(3));
        assert!(entries[0].source_modified_unix_ns.is_some());
    }

    #[test]
    fn late_http_for_an_already_reconciled_permission_is_deduplicated() {
        let directory = tempfile::tempdir().unwrap();
        std::fs::write(
            directory.path().join("last-hook-event.json"),
            br#"{"hook_event_name":"PermissionRequest","unpeel_runtime_generation":3}"#,
        )
        .unwrap();
        let ingress = start("worker-1", directory.path()).unwrap();
        std::thread::sleep(Duration::from_millis(30));
        let body = r#"{"hook_event_name":"PermissionRequest","unpeel_runtime_generation":3}"#;
        let mut stream = TcpStream::connect(("127.0.0.1", ingress.port)).unwrap();
        write!(
            stream,
            "POST /hook/worker-1 HTTP/1.1\r\nContent-Length: {}\r\n\r\n{body}",
            body.len()
        )
        .unwrap();
        stream.shutdown(std::net::Shutdown::Write).unwrap();
        let mut response = String::new();
        stream.read_to_string(&mut response).unwrap();
        assert!(response.contains("200 OK"));
        drop(ingress);
        let entries = read_entries(directory.path()).unwrap().unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].hook_event_name, "PermissionRequest");
        assert!(entries[0].source_modified_unix_ns.is_some());
    }

    #[test]
    fn acknowledged_journal_compaction_retains_only_latch_context() {
        let directory = tempfile::tempdir().unwrap();
        for (sequence, event) in [(1, "Start"), (2, "PermissionRequest"), (3, "Stop")] {
            append_entry(
                &directory.path().join(JOURNAL_FILE),
                &SessionHookJournalEntry {
                    sequence,
                    hook_event_name: event.into(),
                    tool_name: None,
                    runtime_generation: Some(4),
                    task_episode: None,
                    occurred_at_unix_ms: sequence,
                    source_modified_unix_ns: None,
                },
            )
            .unwrap();
        }
        compact_to_latest(directory.path()).unwrap();
        let entries = read_entries(directory.path()).unwrap().unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].sequence, 3);
        assert_eq!(entries[0].hook_event_name, "Stop");
    }
}
