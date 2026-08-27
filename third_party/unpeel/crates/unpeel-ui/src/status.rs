//! Sidebar presence for plugins: activity (busy/idle/attention) and a short
//! status line — the Horizon A surfaces from `docs/plans/unpeel-plugins.md`.
//!
//! Activity is pure reuse of the hook system: this module POSTs the same
//! canonical events the provider hook scripts do (`UserPromptSubmit` busy,
//! `Stop` idle, `PermissionRequest` attention) to `/hook/<session_id>` on
//! every registered port, and mirrors them into the durable
//! `last-hook-event.json` seed so the hook-owned latch survives frontend
//! restarts.
//!
//! Status text is the `status.json` marker in the session dir (like
//! `title.json`): atomic whole-file overwrite, last-writer-wins, announced
//! on the state bus as a session-markers change. Writes are debounced so a
//! chatty plugin can't treat the sidebar like a progress bar; call
//! [`StatusReporter::flush`] before exit (Drop does it too) so a trailing
//! debounced write lands.
//!
//! Outside Unpeel every call is a silent no-op.

use std::io::Write as _;
use std::net::TcpStream;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use crate::host::Host;

const IO_TIMEOUT: Duration = Duration::from_millis(250);
const DEBOUNCE: Duration = Duration::from_millis(250);

/// Handle for reporting this plugin's sidebar presence. Construct once via
/// [`StatusReporter::detect`] and keep it for the process lifetime.
pub struct StatusReporter {
    host: Option<Host>,
    last_written: Option<(Instant, String)>,
    pending: Option<String>,
}

impl StatusReporter {
    pub fn detect() -> Self {
        Self::new(Host::detect())
    }

    pub fn new(host: Option<Host>) -> Self {
        StatusReporter {
            host,
            last_written: None,
            pending: None,
        }
    }

    /// Whether an Unpeel host is present (purely informational — calls are
    /// safe either way).
    pub fn connected(&self) -> bool {
        self.host.is_some()
    }

    /// The session is working — sidebar spinner.
    pub fn busy(&self) {
        self.post_hook_event("UserPromptSubmit");
    }

    /// The session settled — spinner clears, unread integrates.
    pub fn idle(&self) {
        self.post_hook_event("Stop");
    }

    /// The session needs the user — attention accent.
    pub fn attention(&self) {
        self.post_hook_event("PermissionRequest");
    }

    /// Set the sidebar status line: short, single-line, plain text — a
    /// status ("3 tasks due"), not a log. Identical and rapid-fire writes
    /// are coalesced; the latest text always wins.
    pub fn set_status(&mut self, text: &str) {
        if self.host.is_none() {
            return;
        }
        let text = text.trim().replace(['\n', '\r'], " ");
        if let Some((_, last)) = &self.last_written {
            if *last == text && self.pending.is_none() {
                return;
            }
        }
        if let Some((at, _)) = &self.last_written {
            if at.elapsed() < DEBOUNCE {
                self.pending = Some(text);
                return;
            }
        }
        self.write_status(&text);
    }

    /// Write any debounced trailing status. One-shot exits must call this
    /// (or drop the reporter) or the last status is lost.
    pub fn flush(&mut self) {
        if let Some(text) = self.pending.take() {
            self.write_status(&text);
        }
    }

    fn write_status(&mut self, text: &str) {
        let Some(host) = &self.host else { return };
        // Never create the session dir — the session may already be gone.
        if !host.session_dir.is_dir() {
            return;
        }
        let body = serde_json::json!({ "text": text, "updated_at": now_ms() });
        let tmp = host.session_dir.join(".status.json.tmp");
        let ok = serde_json::to_vec(&body)
            .ok()
            .and_then(|bytes| std::fs::write(&tmp, bytes).ok())
            .and_then(|_| std::fs::rename(&tmp, host.session_dir.join("status.json")).ok())
            .is_some();
        if ok {
            // The ping is an optimisation (frontends still poll); a plugin
            // has no listener port of its own, so nothing to skip.
            post_json(host, "/state-changed", r#"{"change":"session-markers"}"#);
        }
        self.pending = None;
        self.last_written = Some((Instant::now(), text.to_string()));
    }

    fn post_hook_event(&self, event: &str) {
        let Some(host) = &self.host else { return };
        // Durable seed first, so the latch survives even if no instance is
        // listening right now. Same recorded set as the hook scripts.
        if host.session_dir.is_dir() {
            let tmp = host
                .session_dir
                .join(format!(".last-hook-event.json.{}", std::process::id()));
            let body = format!(r#"{{"hook_event_name":"{event}"}}"#);
            if std::fs::write(&tmp, body).is_ok()
                && std::fs::rename(&tmp, host.session_dir.join("last-hook-event.json")).is_err()
            {
                let _ = std::fs::remove_file(&tmp);
            }
        }
        let path = format!("/hook/{}", host.session_id);
        let body = format!(r#"{{"hook_event_name":"{event}"}}"#);
        post_json(host, &path, &body);
    }
}

impl Drop for StatusReporter {
    fn drop(&mut self) {
        self.flush();
    }
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

/// Fire-and-forget POST to every registered instance, inline with short
/// timeouts — the hook server answers 404 for session ids it doesn't own,
/// which is how foreign instances decline events. Failures are ignored:
/// a port whose owner has gone is normal.
fn post_json(host: &Host, path: &str, body: &str) {
    for port in host.ports() {
        let address = format!("127.0.0.1:{port}");
        let Ok(target) = address.parse() else {
            continue;
        };
        let Ok(mut stream) = TcpStream::connect_timeout(&target, IO_TIMEOUT) else {
            continue;
        };
        let _ = stream.set_write_timeout(Some(IO_TIMEOUT));
        let request = format!(
            "POST {path} HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Type: application/json\r\n\
             Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        let _ = stream.write_all(request.as_bytes());
        let _ = stream.flush();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;
    use std::net::TcpListener;
    use std::path::{Path, PathBuf};
    use std::sync::mpsc;

    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "unpeel-ui-{tag}-{}-{}",
            std::process::id(),
            now_ms()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn listener_capturing(tx: mpsc::Sender<String>) -> u16 {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        std::thread::spawn(move || {
            while let Ok((mut stream, _)) = listener.accept() {
                let mut buffer = [0u8; 1024];
                let read = stream.read(&mut buffer).unwrap_or(0);
                let _ = tx.send(String::from_utf8_lossy(&buffer[..read]).into_owned());
            }
        });
        port
    }

    fn host_in(dir: &Path, port: u16) -> Host {
        let registry = dir.join("app-ports");
        std::fs::write(&registry, format!("{port}\n")).unwrap();
        let session_dir = dir.join("session");
        std::fs::create_dir_all(&session_dir).unwrap();
        Host {
            session_id: "s1".into(),
            session_dir,
            app_port: None,
            port_registry: registry,
        }
    }

    #[test]
    fn standalone_calls_are_noops() {
        let mut reporter = StatusReporter::new(None);
        reporter.busy();
        reporter.set_status("anything");
        reporter.idle();
        reporter.flush();
        assert!(!reporter.connected());
    }

    #[test]
    fn status_writes_marker_and_announces() {
        let dir = temp_dir("status");
        let (tx, rx) = mpsc::channel();
        let port = listener_capturing(tx);
        let host = host_in(&dir, port);
        let session_dir = host.session_dir.clone();

        let mut reporter = StatusReporter::new(Some(host));
        reporter.set_status("3 open · 1 done");

        let raw = std::fs::read_to_string(session_dir.join("status.json")).unwrap();
        let json: serde_json::Value = serde_json::from_str(&raw).unwrap();
        assert_eq!(json["text"], "3 open · 1 done");
        assert!(json["updated_at"].as_u64().is_some());

        let request = rx.recv_timeout(Duration::from_secs(2)).unwrap();
        assert!(request.starts_with("POST /state-changed "), "{request}");
        assert!(
            request.contains(r#"{"change":"session-markers"}"#),
            "{request}"
        );
    }

    #[test]
    fn rapid_writes_debounce_and_flush_lands_the_last() {
        let dir = temp_dir("debounce");
        let (tx, _rx) = mpsc::channel();
        let port = listener_capturing(tx);
        let host = host_in(&dir, port);
        let session_dir = host.session_dir.clone();

        let mut reporter = StatusReporter::new(Some(host));
        reporter.set_status("1 open");
        reporter.set_status("2 open");
        reporter.set_status("3 open");
        // The burst coalesced: file still holds the first write.
        let raw = std::fs::read_to_string(session_dir.join("status.json")).unwrap();
        assert!(raw.contains("1 open"), "{raw}");
        reporter.flush();
        let raw = std::fs::read_to_string(session_dir.join("status.json")).unwrap();
        assert!(raw.contains("3 open"), "{raw}");
    }

    #[test]
    fn activity_posts_hook_event_and_records_seed() {
        let dir = temp_dir("activity");
        let (tx, rx) = mpsc::channel();
        let port = listener_capturing(tx);
        let host = host_in(&dir, port);
        let session_dir = host.session_dir.clone();

        let reporter = StatusReporter::new(Some(host));
        reporter.idle();

        let seed = std::fs::read_to_string(session_dir.join("last-hook-event.json")).unwrap();
        assert_eq!(seed, r#"{"hook_event_name":"Stop"}"#);
        let request = rx.recv_timeout(Duration::from_secs(2)).unwrap();
        assert!(request.starts_with("POST /hook/s1 "), "{request}");
        assert!(
            request.contains(r#"{"hook_event_name":"Stop"}"#),
            "{request}"
        );
    }
}
