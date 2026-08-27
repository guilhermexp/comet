//! Best-effort Herdr status reporting for the outer, interactive Unpeel TUI.
//!
//! Herdr assigns agent authority to a pane, so this module reports one
//! aggregate `unpeel` supervisor rather than forwarding provider hooks or
//! provider session identities.  The caller feeds it the fully-derived
//! [`SidebarModel`] after a rescan; socket work stays on a single background
//! thread and failures never affect the terminal UI.

use std::ffi::{OsStr, OsString};
use std::io::{BufRead, BufReader, Read, Write};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::sync::{Arc, Condvar, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use serde_json::{json, Value};

use crate::sessions::{SidebarModel, Status};

const SOURCE: &str = "custom:unpeel";
const AGENT: &str = "unpeel";
const SOCKET_IO_TIMEOUT: Duration = Duration::from_millis(300);
// pane.list is the identity-safety check and can legitimately include a large
// restored Herdr session. Keep it bounded without making an ordinary
// many-pane setup fail closed merely because its JSON exceeds 64 KiB.
const MAX_RESPONSE_BYTES: u64 = 1024 * 1024;
const IDLE_DEBOUNCE: Duration = Duration::from_millis(250);
const INITIAL_RETRY: Duration = Duration::from_millis(250);
const MAX_RETRY: Duration = Duration::from_secs(30);
const REASSERT_INTERVAL: Duration = Duration::from_secs(30);
const FOREGROUND_POLL: Duration = Duration::from_millis(250);
// A healthy local release is effectively immediate. Leave enough room for
// one identity verification plus one release response before detaching a
// worker whose Herdr socket is wedged.
const SHUTDOWN_WAIT: Duration = Duration::from_secs(1);
const TRACE_INTERVAL: Duration = Duration::from_secs(30);

#[derive(Clone, Debug)]
struct Context {
    socket_path: PathBuf,
    pane_id: String,
}

impl Context {
    fn from_env() -> Option<Self> {
        Self::from_lookup(|key| std::env::var_os(key))
    }

    fn from_lookup(mut lookup: impl FnMut(&str) -> Option<OsString>) -> Option<Self> {
        if lookup("UNPEEL_HERDR_STATUS").as_deref() == Some(OsStr::new("off")) {
            return None;
        }
        if lookup("HERDR_ENV").as_deref() != Some(OsStr::new("1")) {
            return None;
        }

        let socket = lookup("HERDR_SOCKET_PATH")?;
        if socket.is_empty() {
            return None;
        }
        let pane_id = lookup("HERDR_PANE_ID")?.into_string().ok()?;
        if pane_id.is_empty() {
            return None;
        }

        Some(Self {
            socket_path: PathBuf::from(socket),
            pane_id,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum HerdrState {
    Idle,
    Working,
    Blocked,
}

impl HerdrState {
    fn as_str(self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::Working => "working",
            Self::Blocked => "blocked",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Aggregate {
    state: HerdrState,
    message: String,
}

impl Aggregate {
    fn from_model(model: &SidebarModel) -> Self {
        let mut attention = 0usize;
        let mut working = 0usize;
        let mut idle = 0usize;

        for row in model
            .rows
            .iter()
            .filter(|row| row.running && !row.archived && row.status != Status::Exited)
        {
            match row.status {
                Status::Attention => attention += 1,
                Status::Busy | Status::Starting => working += 1,
                Status::Idle => idle += 1,
                Status::Exited => {}
            }
        }

        if attention > 0 {
            let noun = if attention == 1 {
                "session"
            } else {
                "sessions"
            };
            let verb = if attention == 1 { "needs" } else { "need" };
            return Self {
                state: HerdrState::Blocked,
                message: format!("{attention} Unpeel {noun} {verb} attention"),
            };
        }

        if working > 0 {
            let message = if idle > 0 {
                format!("{working} working, {idle} idle")
            } else {
                let noun = if working == 1 { "session" } else { "sessions" };
                format!("{working} {noun} working")
            };
            return Self {
                state: HerdrState::Working,
                message,
            };
        }

        let message = if idle == 0 {
            "No running sessions".to_string()
        } else {
            let noun = if idle == 1 { "session" } else { "sessions" };
            format!("{idle} {noun} idle")
        };
        Self {
            state: HerdrState::Idle,
            message,
        }
    }
}

#[derive(Clone)]
struct Desired {
    aggregate: Aggregate,
    generation: u64,
    changed_at: Instant,
}

#[derive(Default)]
struct WorkerState {
    desired: Option<Desired>,
    generation: u64,
    shutdown: bool,
}

struct Shared {
    state: Mutex<WorkerState>,
    wake: Condvar,
}

type ForegroundCheck = Arc<dyn Fn() -> bool + Send + Sync + 'static>;

#[derive(Clone, Copy)]
struct WorkerConfig {
    idle_debounce: Duration,
    initial_retry: Duration,
    max_retry: Duration,
    reassert_interval: Duration,
    foreground_poll: Duration,
    shutdown_wait: Duration,
    trace_interval: Duration,
}

impl Default for WorkerConfig {
    fn default() -> Self {
        Self {
            idle_debounce: IDLE_DEBOUNCE,
            initial_retry: INITIAL_RETRY,
            max_retry: MAX_RETRY,
            reassert_interval: REASSERT_INTERVAL,
            foreground_poll: FOREGROUND_POLL,
            shutdown_wait: SHUTDOWN_WAIT,
            trace_interval: TRACE_INTERVAL,
        }
    }
}

/// Lifecycle guard for one aggregate Herdr pane occupant.
///
/// Construct it only for the interactive TUI. [`Self::from_env`] returns
/// `None` outside a complete inherited Herdr pane context. Calling
/// [`Self::update`] is cheap: it replaces a single pending value and wakes a
/// background worker; it never opens a socket on the caller's thread.
pub struct HerdrReporter {
    shared: Arc<Shared>,
    done: Option<std::sync::mpsc::Receiver<()>>,
    worker: Option<JoinHandle<()>>,
    shutdown_wait: Duration,
}

impl HerdrReporter {
    /// Start reporting when this process has an exact, inherited Herdr pane
    /// context. `UNPEEL_HERDR_STATUS=off` is an explicit diagnostic opt-out.
    pub fn from_env() -> Option<Self> {
        let context = Context::from_env()?;
        Some(Self::spawn(
            context,
            Arc::new(owns_terminal_foreground),
            WorkerConfig::default(),
        ))
    }

    /// Replace the desired aggregate with the state derived by the latest
    /// completed TUI rescan.
    pub fn update(&self, model: &SidebarModel) {
        let aggregate = Aggregate::from_model(model);
        let mut state = self.shared.state.lock().unwrap_or_else(|e| e.into_inner());
        if state.shutdown
            || state
                .desired
                .as_ref()
                .is_some_and(|desired| desired.aggregate == aggregate)
        {
            return;
        }

        state.generation = state.generation.wrapping_add(1).max(1);
        state.desired = Some(Desired {
            aggregate,
            generation: state.generation,
            changed_at: Instant::now(),
        });
        drop(state);
        self.shared.wake.notify_one();
    }

    fn spawn(context: Context, foreground: ForegroundCheck, config: WorkerConfig) -> Self {
        let shared = Arc::new(Shared {
            state: Mutex::new(WorkerState::default()),
            wake: Condvar::new(),
        });
        let worker_shared = Arc::clone(&shared);
        let (done_tx, done_rx) = std::sync::mpsc::channel();
        let worker = thread::Builder::new()
            .name("unpeel-herdr-status".into())
            .spawn(move || {
                run_worker(worker_shared, context, foreground, config);
                let _ = done_tx.send(());
            });
        let (worker, done) = match worker {
            Ok(worker) => (Some(worker), Some(done_rx)),
            Err(_) => (None, None),
        };

        Self {
            shared,
            done,
            worker,
            shutdown_wait: config.shutdown_wait,
        }
    }

    fn shutdown(&mut self) {
        {
            let mut state = self.shared.state.lock().unwrap_or_else(|e| e.into_inner());
            state.shutdown = true;
        }
        self.shared.wake.notify_one();

        let finished = self
            .done
            .take()
            .is_some_and(|done| done.recv_timeout(self.shutdown_wait).is_ok());
        if finished {
            if let Some(worker) = self.worker.take() {
                let _ = worker.join();
            }
        } else {
            // Dropping a JoinHandle detaches it. The bounded shutdown wait is
            // deliberate: a broken local supervisor must not hold TUI exit.
            self.worker.take();
        }
    }
}

impl Drop for HerdrReporter {
    fn drop(&mut self) {
        self.shutdown();
    }
}

fn owns_terminal_foreground() -> bool {
    // SAFETY: both libc calls take no borrowed pointers and have no memory
    // safety preconditions. A non-TTY returns -1 from tcgetpgrp and fails the
    // comparison, which is the conservative ownership answer.
    unsafe { libc::tcgetpgrp(libc::STDIN_FILENO) == libc::getpgrp() }
}

fn run_worker(
    shared: Arc<Shared>,
    context: Context,
    foreground: ForegroundCheck,
    config: WorkerConfig,
) {
    let mut acknowledged: Option<Aggregate> = None;
    let mut acknowledged_at: Option<Instant> = None;
    let mut generation = 0u64;
    let mut next_attempt = Instant::now();
    let mut retry = config.initial_retry;
    let mut was_foreground = false;
    let mut last_trace = None;
    // Public pane ids can change when Herdr moves the live terminal across
    // workspaces. `terminal_id` is the stable identity used to recover the
    // new public id without ever guessing from focus.
    let mut terminal_id: Option<String> = None;

    loop {
        let (shutdown, snapshot_generation, desired) = {
            let state = shared.state.lock().unwrap_or_else(|e| e.into_inner());
            (state.shutdown, state.generation, state.desired.clone())
        };

        if shutdown {
            // Re-check ownership immediately before release. A suspended or
            // superseded TUI must not clear a newer pane authority. Verify
            // the stable terminal again rather than releasing a cached public
            // id that could now belong to another pane.
            if foreground() {
                if let Some(stable_id) = terminal_id.as_deref() {
                    if let Ok(Some(verified)) = recover_pane_id(&context, stable_id, &foreground) {
                        let _ = send_release(&context, &verified, &foreground);
                    }
                }
            }
            return;
        }

        let is_foreground = foreground();
        if !is_foreground {
            acknowledged = None;
            acknowledged_at = None;
            was_foreground = false;
            wait_for_wake(&shared, snapshot_generation, config.foreground_poll);
            continue;
        }
        if !was_foreground {
            // Foreground resume is an authority transition: reassert even if
            // the semantic aggregate did not change while suspended.
            acknowledged = None;
            acknowledged_at = None;
            next_attempt = Instant::now();
            retry = config.initial_retry;
            was_foreground = true;
        }

        let Some(desired) = desired else {
            wait_for_wake(&shared, snapshot_generation, config.foreground_poll);
            continue;
        };

        if desired.generation != generation {
            let is_initial = generation == 0;
            generation = desired.generation;
            retry = config.initial_retry;
            next_attempt = if desired.aggregate.state == HerdrState::Idle && !is_initial {
                desired.changed_at + config.idle_debounce
            } else {
                Instant::now()
            };
        }

        let now = Instant::now();
        let due = if acknowledged.as_ref() == Some(&desired.aggregate) {
            acknowledged_at
                .map(|at| at + config.reassert_interval)
                .unwrap_or(now)
        } else {
            next_attempt
        };
        if now < due {
            // A long retry/reassert deadline must not hide a job-control
            // transition. Poll foreground ownership and reassert promptly
            // after resume.
            let wait = due
                .saturating_duration_since(now)
                .min(config.foreground_poll);
            wait_for_wake(&shared, snapshot_generation, wait);
            continue;
        }

        // A public pane id can become stale after a cross-workspace move or
        // server restore. Capture the exact inherited terminal identity, then
        // verify it before EVERY report. Reporting without that proof could
        // claim a stale-but-valid id belonging to a neighboring pane.
        let stable_id = match terminal_id.clone() {
            Some(stable_id) => stable_id,
            None => match capture_terminal_id(&context, &foreground) {
                Ok(Some(stable_id)) => {
                    terminal_id = Some(stable_id.clone());
                    stable_id
                }
                Ok(None) => {
                    acknowledged = None;
                    acknowledged_at = None;
                    if !config.trace_interval.is_zero() {
                        trace_rate_limited(
                            "pane.current returned no stable terminal identity",
                            &mut last_trace,
                            config.trace_interval,
                        );
                    }
                    next_attempt = Instant::now() + retry;
                    retry = (retry * 2).min(config.max_retry);
                    continue;
                }
                Err(SendError::NotForeground) => {
                    acknowledged = None;
                    acknowledged_at = None;
                    was_foreground = false;
                    continue;
                }
                Err(SendError::Protocol(error)) => {
                    acknowledged = None;
                    acknowledged_at = None;
                    if !config.trace_interval.is_zero() {
                        trace_rate_limited(&error, &mut last_trace, config.trace_interval);
                    }
                    next_attempt = Instant::now() + retry;
                    retry = (retry * 2).min(config.max_retry);
                    continue;
                }
            },
        };

        let pane_id = match recover_pane_id(&context, &stable_id, &foreground) {
            Ok(Some(verified)) => verified,
            Ok(None) => {
                acknowledged = None;
                acknowledged_at = None;
                if !config.trace_interval.is_zero() {
                    trace_rate_limited(
                        "stable terminal identity was missing or ambiguous in pane.list",
                        &mut last_trace,
                        config.trace_interval,
                    );
                }
                next_attempt = Instant::now() + retry;
                retry = (retry * 2).min(config.max_retry);
                continue;
            }
            Err(SendError::NotForeground) => {
                acknowledged = None;
                acknowledged_at = None;
                was_foreground = false;
                continue;
            }
            Err(SendError::Protocol(error)) => {
                acknowledged = None;
                acknowledged_at = None;
                if !config.trace_interval.is_zero() {
                    trace_rate_limited(&error, &mut last_trace, config.trace_interval);
                }
                next_attempt = Instant::now() + retry;
                retry = (retry * 2).min(config.max_retry);
                continue;
            }
        };

        // A shutdown or newer aggregate may have landed while
        // pane.current/pane.list was waiting on the socket. Return to the
        // latest slot instead of publishing the superseded lifecycle value.
        if worker_state_changed(&shared, snapshot_generation) {
            continue;
        }

        match send_report(&context, &pane_id, &desired.aggregate, &foreground) {
            Ok(()) => {
                acknowledged = Some(desired.aggregate);
                acknowledged_at = Some(Instant::now());
                retry = config.initial_retry;
            }
            Err(SendError::NotForeground) => {
                acknowledged = None;
                acknowledged_at = None;
                was_foreground = false;
            }
            Err(SendError::Protocol(error)) => {
                acknowledged = None;
                acknowledged_at = None;
                if !config.trace_interval.is_zero() {
                    trace_rate_limited(&error, &mut last_trace, config.trace_interval);
                }
                next_attempt = Instant::now() + retry;
                retry = (retry * 2).min(config.max_retry);
            }
        }
    }
}

fn worker_state_changed(shared: &Shared, observed_generation: u64) -> bool {
    let state = shared
        .state
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    state.shutdown || state.generation != observed_generation
}

fn wait_for_wake(shared: &Shared, observed_generation: u64, timeout: Duration) {
    let state = shared.state.lock().unwrap_or_else(|e| e.into_inner());
    // Close the usual condvar lost-wake window: an update or shutdown may
    // land after the worker took its snapshot but before it acquired this
    // lock. In that case there is already work to do, so do not sleep even
    // if the notification itself arrived before `wait_timeout` began.
    if state.shutdown || state.generation != observed_generation {
        return;
    }
    let _ = shared
        .wake
        .wait_timeout(state, timeout)
        .unwrap_or_else(|e| e.into_inner());
}

#[derive(Debug)]
enum SendError {
    NotForeground,
    Protocol(String),
}

fn send_report(
    context: &Context,
    pane_id: &str,
    aggregate: &Aggregate,
    foreground: &ForegroundCheck,
) -> Result<(), SendError> {
    if !foreground() {
        return Err(SendError::NotForeground);
    }
    let params = report_params(pane_id, aggregate);
    send_request(context, "pane.report_agent", params)
        .map(|_| ())
        .map_err(SendError::Protocol)
}

fn report_params(pane_id: &str, aggregate: &Aggregate) -> Value {
    json!({
        "pane_id": pane_id,
        "source": SOURCE,
        "agent": AGENT,
        "state": aggregate.state.as_str(),
        "message": aggregate.message,
    })
}

fn send_release(
    context: &Context,
    pane_id: &str,
    foreground: &ForegroundCheck,
) -> Result<(), SendError> {
    if !foreground() {
        return Err(SendError::NotForeground);
    }
    let params = release_params(pane_id);
    send_request(context, "pane.release_agent", params)
        .map(|_| ())
        .map_err(SendError::Protocol)
}

fn release_params(pane_id: &str) -> Value {
    json!({
        "pane_id": pane_id,
        "source": SOURCE,
        "agent": AGENT,
    })
}

fn capture_terminal_id(
    context: &Context,
    foreground: &ForegroundCheck,
) -> Result<Option<String>, SendError> {
    if !foreground() {
        return Err(SendError::NotForeground);
    }
    let result = send_request(
        context,
        "pane.current",
        json!({"caller_pane_id": context.pane_id}),
    )
    .map_err(SendError::Protocol)?;
    Ok(result
        .get("pane")
        .and_then(|pane| pane.get("terminal_id"))
        .and_then(Value::as_str)
        .filter(|terminal_id| !terminal_id.is_empty())
        .map(str::to_string))
}

fn recover_pane_id(
    context: &Context,
    terminal_id: &str,
    foreground: &ForegroundCheck,
) -> Result<Option<String>, SendError> {
    if !foreground() {
        return Err(SendError::NotForeground);
    }
    let result = send_request(context, "pane.list", json!({})).map_err(SendError::Protocol)?;
    Ok(unique_pane_id_for_terminal(&result, terminal_id))
}

fn unique_pane_id_for_terminal(result: &Value, terminal_id: &str) -> Option<String> {
    if terminal_id.is_empty() {
        return None;
    }
    let matches: Vec<&Value> = result
        .get("panes")?
        .as_array()?
        .iter()
        .filter(|pane| pane.get("terminal_id").and_then(Value::as_str) == Some(terminal_id))
        .collect();
    if matches.len() != 1 {
        return None;
    }
    matches[0]
        .get("pane_id")
        .and_then(Value::as_str)
        .filter(|pane_id| !pane_id.is_empty())
        .map(str::to_string)
}

fn send_request(context: &Context, method: &str, params: Value) -> Result<Value, String> {
    let request_id = format!(
        "unpeel-{}",
        REQUEST_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    );
    let request = json!({
        "id": request_id,
        "method": method,
        "params": params,
    });
    let mut encoded = serde_json::to_vec(&request).map_err(|error| error.to_string())?;
    encoded.push(b'\n');

    let mut stream = UnixStream::connect(&context.socket_path)
        .map_err(|error| format!("connect failed: {error}"))?;
    stream
        .set_read_timeout(Some(SOCKET_IO_TIMEOUT))
        .map_err(|error| format!("set read timeout failed: {error}"))?;
    stream
        .set_write_timeout(Some(SOCKET_IO_TIMEOUT))
        .map_err(|error| format!("set write timeout failed: {error}"))?;
    stream
        .write_all(&encoded)
        .map_err(|error| format!("write failed: {error}"))?;

    let mut response = Vec::new();
    let mut reader = BufReader::new(stream.take(MAX_RESPONSE_BYTES + 1));
    let read = reader
        .read_until(b'\n', &mut response)
        .map_err(|error| format!("read failed: {error}"))?;
    if read == 0 {
        return Err("server closed without a response".into());
    }
    if response.len() as u64 > MAX_RESPONSE_BYTES {
        return Err("response exceeded size limit".into());
    }

    let response: Value =
        serde_json::from_slice(&response).map_err(|error| format!("invalid response: {error}"))?;
    if response.get("id").and_then(Value::as_str) != Some(request_id.as_str()) {
        return Err("response id did not match request".into());
    }
    if let Some(error) = response.get("error") {
        if !error.is_null() {
            return Err(format!("Herdr API error: {error}"));
        }
    }
    response
        .get("result")
        .cloned()
        .ok_or_else(|| "response had neither result nor error".into())
}

/// One-time right-click tip. Herdr reserves plain right-click for its own
/// pane menu, so the TUI's context menus are unreachable in a Herdr pane
/// until the user sets Herdr's `right_click_passthrough_modifier`. Returns
/// the tip while running inside a Herdr pane and not yet dismissed; the
/// marker follows the `cli-update-dismissed` pattern.
pub fn right_click_hint() -> Option<crate::EnvHint> {
    let marker = "cli-herdr-hint-dismissed";
    if unpeel_core::app_paths::unpeel_home().join(marker).exists() {
        return None;
    }
    Some(crate::EnvHint {
        text: "Herdr tip: set right_click_passthrough_modifier = \"alt\" to use Unpeel's \
               right-click menus (alt+right-click) — click to dismiss"
            .into(),
        marker,
    })
}

static REQUEST_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

fn trace_rate_limited(error: &str, last: &mut Option<Instant>, interval: Duration) {
    if last.is_some_and(|at| at.elapsed() < interval) {
        return;
    }
    *last = Some(Instant::now());
    let compact = error.replace(['\r', '\n'], " ");
    unpeel_core::hook_assets::append_trace_log_line(&format!("herdr-status {compact}"));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sessions::SessionRow;
    use std::collections::HashMap;
    use std::os::unix::net::UnixListener;
    use std::sync::mpsc;

    fn context_from(entries: &[(&str, &str)]) -> Option<Context> {
        let env: HashMap<&str, OsString> = entries
            .iter()
            .map(|(key, value)| (*key, OsString::from(value)))
            .collect();
        Context::from_lookup(|key| env.get(key).cloned())
    }

    fn row(id: &str, status: Status) -> SessionRow {
        SessionRow {
            id: id.into(),
            project_id: "project-secret".into(),
            label: "private session title".into(),
            command: "agent --prompt private".into(),
            active_runtime_id: None,
            resume_available: false,
            resume_agent_available: false,
            running: status != Status::Exited,
            status,
            created_at: 0,
            pinned: false,
            archived: false,
            unread: false,
            cwd: "/private/path".into(),
            activity_at: 0,
            group_id: "private-group".into(),
            detected_local_urls: Vec::new(),
        }
    }

    fn model(rows: Vec<SessionRow>) -> SidebarModel {
        SidebarModel {
            rows,
            ..SidebarModel::default()
        }
    }

    #[test]
    fn activation_requires_exact_complete_context_and_honors_opt_out() {
        let valid = [
            ("HERDR_ENV", "1"),
            ("HERDR_SOCKET_PATH", "/tmp/herdr.sock"),
            ("HERDR_PANE_ID", "w1:p1"),
        ];
        let context = context_from(&valid).expect("valid Herdr context");
        assert_eq!(context.socket_path, PathBuf::from("/tmp/herdr.sock"));
        assert_eq!(context.pane_id, "w1:p1");

        assert!(context_from(&valid[..2]).is_none());
        assert!(context_from(&[
            ("HERDR_ENV", "true"),
            ("HERDR_SOCKET_PATH", "/tmp/herdr.sock"),
            ("HERDR_PANE_ID", "w1:p1"),
        ])
        .is_none());
        assert!(context_from(&[
            ("HERDR_ENV", "1"),
            ("HERDR_SOCKET_PATH", ""),
            ("HERDR_PANE_ID", "w1:p1"),
        ])
        .is_none());
        assert!(context_from(&[
            ("HERDR_ENV", "1"),
            ("HERDR_SOCKET_PATH", "/tmp/herdr.sock"),
            ("HERDR_PANE_ID", ""),
        ])
        .is_none());

        let mut opted_out = valid.to_vec();
        opted_out.push(("UNPEEL_HERDR_STATUS", "off"));
        assert!(context_from(&opted_out).is_none());
    }

    #[test]
    fn aggregate_uses_attention_then_working_then_idle_precedence() {
        let aggregate = Aggregate::from_model(&model(vec![
            row("idle", Status::Idle),
            row("busy", Status::Busy),
            row("starting", Status::Starting),
            row("attention", Status::Attention),
        ]));
        assert_eq!(aggregate.state, HerdrState::Blocked);
        assert_eq!(aggregate.message, "1 Unpeel session needs attention");

        let aggregate = Aggregate::from_model(&model(vec![
            row("idle", Status::Idle),
            row("busy", Status::Busy),
            row("starting", Status::Starting),
        ]));
        assert_eq!(aggregate.state, HerdrState::Working);
        assert_eq!(aggregate.message, "2 working, 1 idle");

        let aggregate = Aggregate::from_model(&model(vec![
            row("idle-1", Status::Idle),
            row("idle-2", Status::Idle),
        ]));
        assert_eq!(aggregate.state, HerdrState::Idle);
        assert_eq!(aggregate.message, "2 sessions idle");
    }

    #[test]
    fn aggregate_excludes_nonrunning_archived_and_exited_rows() {
        let mut nonrunning = row("not-running", Status::Attention);
        nonrunning.running = false;
        let mut archived = row("archived", Status::Attention);
        archived.archived = true;
        let exited = row("exited", Status::Exited);

        let aggregate = Aggregate::from_model(&model(vec![nonrunning, archived, exited]));
        assert_eq!(aggregate.state, HerdrState::Idle);
        assert_eq!(aggregate.message, "No running sessions");
    }

    #[test]
    fn aggregate_message_never_contains_session_content() {
        let aggregate = Aggregate::from_model(&model(vec![row("secret-id", Status::Busy)]));
        assert_eq!(aggregate.message, "1 session working");
        for secret in [
            "secret-id",
            "private session title",
            "agent --prompt private",
            "/private/path",
            "project-secret",
        ] {
            assert!(!aggregate.message.contains(secret));
        }
    }

    #[test]
    fn report_wire_shape_is_allowlisted_and_has_no_sequence_or_session_id() {
        let context = Context {
            socket_path: PathBuf::from("/unused"),
            pane_id: "w1:p1".into(),
        };
        let aggregate = Aggregate {
            state: HerdrState::Working,
            message: "2 working, 1 idle".into(),
        };
        let params = report_params(&context.pane_id, &aggregate);
        assert_eq!(
            params,
            json!({
                "pane_id": "w1:p1",
                "source": "custom:unpeel",
                "agent": "unpeel",
                "state": "working",
                "message": "2 working, 1 idle",
            })
        );
        assert!(params.get("seq").is_none());
        assert!(params.get("custom_status").is_none());
        assert!(params.get("agent_session_id").is_none());
    }

    #[test]
    fn pane_recovery_requires_one_unambiguous_terminal_match() {
        let result = json!({
            "type": "pane_list",
            "panes": [
                {"pane_id": "w1:p2", "terminal_id": "term-other"},
                {"pane_id": "w9:p7", "terminal_id": "term-unpeel"},
            ],
        });
        assert_eq!(
            unique_pane_id_for_terminal(&result, "term-unpeel").as_deref(),
            Some("w9:p7")
        );
        assert!(unique_pane_id_for_terminal(&result, "").is_none());
        assert!(unique_pane_id_for_terminal(&result, "missing").is_none());

        let ambiguous = json!({
            "panes": [
                {"pane_id": "w9:p7", "terminal_id": "term-unpeel"},
                {"pane_id": "w10:p1", "terminal_id": "term-unpeel"},
            ],
        });
        assert!(unique_pane_id_for_terminal(&ambiguous, "term-unpeel").is_none());
        assert!(unique_pane_id_for_terminal(
            &json!({"panes": [{"terminal_id": "term-unpeel"}]}),
            "term-unpeel"
        )
        .is_none());
    }

    fn test_config() -> WorkerConfig {
        WorkerConfig {
            idle_debounce: Duration::from_millis(15),
            initial_retry: Duration::from_millis(15),
            max_retry: Duration::from_millis(30),
            reassert_interval: Duration::from_secs(30),
            foreground_poll: Duration::from_millis(10),
            shutdown_wait: Duration::from_millis(300),
            // Unit tests must not append expected retry failures to the
            // user's real hook trace.
            trace_interval: Duration::ZERO,
        }
    }

    fn exchange_request(mut stream: UnixStream, response: impl FnOnce(&Value) -> Value) -> Value {
        stream
            .set_read_timeout(Some(Duration::from_secs(1)))
            .unwrap();
        let mut line = String::new();
        BufReader::new(stream.try_clone().unwrap())
            .read_line(&mut line)
            .unwrap();
        let request: Value = serde_json::from_str(&line).unwrap();
        let mut response = response(&request);
        response["id"] = request["id"].clone();
        writeln!(stream, "{response}").unwrap();
        request
    }

    fn receive_method(requests: &mpsc::Receiver<Value>, method: &str, timeout: Duration) -> Value {
        let deadline = Instant::now() + timeout;
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            let request = requests.recv_timeout(remaining).unwrap();
            if request.get("method").and_then(Value::as_str) == Some(method) {
                return request;
            }
        }
    }

    fn spawn_success_server(
        listener: UnixListener,
        request_tx: mpsc::Sender<Value>,
        pane_id: &'static str,
    ) -> JoinHandle<()> {
        thread::spawn(move || loop {
            let (stream, _) = listener.accept().unwrap();
            let request = exchange_request(stream, |request| {
                match request.get("method").and_then(Value::as_str) {
                    Some("pane.current") => json!({
                        "result": {
                            // Herdr 0.7.1 used pane_current; current builds
                            // use pane_info. The client intentionally keys on
                            // fields rather than this additive type label.
                            "type": "pane_current",
                            "pane": {
                                "pane_id": pane_id,
                                "terminal_id": "term-unpeel-test",
                            },
                        },
                    }),
                    Some("pane.list") => json!({
                        "result": {
                            "type": "pane_list",
                            "panes": [{
                                "pane_id": pane_id,
                                "terminal_id": "term-unpeel-test",
                            }],
                        },
                    }),
                    _ => json!({"result": {}}),
                }
            });
            let released =
                request.get("method").and_then(Value::as_str) == Some("pane.release_agent");
            request_tx.send(request).unwrap();
            if released {
                return;
            }
        })
    }

    #[test]
    fn worker_retries_latest_value_and_releases_on_clean_drop() {
        let socket_path = std::env::temp_dir().join(format!(
            "unpeel-herdr-test-{}-{}.sock",
            std::process::id(),
            REQUEST_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        let context = Context {
            socket_path: socket_path.clone(),
            pane_id: "w7:p4".into(),
        };
        let mut reporter = HerdrReporter::spawn(context, Arc::new(|| true), test_config());

        // Let the first desired state fail against the not-yet-created socket,
        // then replace it. Once Herdr appears, only the latest slot is retried.
        reporter.update(&model(vec![row("busy", Status::Busy)]));
        thread::sleep(Duration::from_millis(20));
        reporter.update(&model(vec![row("attention", Status::Attention)]));

        let listener = UnixListener::bind(&socket_path).unwrap();
        let (request_tx, request_rx) = mpsc::channel();
        let server = spawn_success_server(listener, request_tx, "w7:p4");

        let report = receive_method(&request_rx, "pane.report_agent", Duration::from_secs(1));
        assert_eq!(report["method"], "pane.report_agent");
        assert_eq!(report["params"]["state"], "blocked");
        assert_eq!(
            report["params"]["message"],
            "1 Unpeel session needs attention"
        );

        reporter.shutdown();
        let release = receive_method(&request_rx, "pane.release_agent", Duration::from_secs(1));
        assert_eq!(release["method"], "pane.release_agent");
        assert_eq!(
            release["params"],
            json!({
                "pane_id": "w7:p4",
                "source": "custom:unpeel",
                "agent": "unpeel",
            })
        );
        server.join().unwrap();
        let _ = std::fs::remove_file(socket_path);
    }

    #[test]
    fn worker_discards_a_desired_value_superseded_during_identity_io() {
        let socket_path = std::env::temp_dir().join(format!(
            "unpeel-herdr-latest-test-{}-{}.sock",
            std::process::id(),
            REQUEST_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        let listener = UnixListener::bind(&socket_path).unwrap();
        let (request_tx, request_rx) = mpsc::channel();
        let (list_entered_tx, list_entered_rx) = mpsc::channel();
        let (release_list_tx, release_list_rx) = mpsc::channel();
        let server = thread::spawn(move || {
            let mut held_first_list = false;
            loop {
                let (stream, _) = listener.accept().unwrap();
                let request = exchange_request(stream, |request| {
                    match request.get("method").and_then(Value::as_str) {
                        Some("pane.current") => json!({
                            "result": {
                                "pane": {
                                    "pane_id": "w3:p2",
                                    "terminal_id": "term-latest",
                                },
                            },
                        }),
                        Some("pane.list") => {
                            if !held_first_list {
                                held_first_list = true;
                                list_entered_tx.send(()).unwrap();
                                release_list_rx.recv().unwrap();
                            }
                            json!({
                                "result": {
                                    "panes": [{
                                        "pane_id": "w3:p2",
                                        "terminal_id": "term-latest",
                                    }],
                                },
                            })
                        }
                        _ => json!({"result": {}}),
                    }
                });
                let released =
                    request.get("method").and_then(Value::as_str) == Some("pane.release_agent");
                request_tx.send(request).unwrap();
                if released {
                    return;
                }
            }
        });

        let context = Context {
            socket_path: socket_path.clone(),
            pane_id: "w3:p2".into(),
        };
        let mut reporter = HerdrReporter::spawn(context, Arc::new(|| true), test_config());
        reporter.update(&model(vec![row("busy", Status::Busy)]));

        list_entered_rx
            .recv_timeout(Duration::from_secs(1))
            .unwrap();
        reporter.update(&model(vec![row("attention", Status::Attention)]));
        release_list_tx.send(()).unwrap();

        let report = receive_method(&request_rx, "pane.report_agent", Duration::from_secs(1));
        assert_eq!(report["params"]["state"], "blocked");
        assert_eq!(
            report["params"]["message"],
            "1 Unpeel session needs attention"
        );

        reporter.shutdown();
        let release = receive_method(&request_rx, "pane.release_agent", Duration::from_secs(1));
        assert_eq!(release["params"]["pane_id"], "w3:p2");
        server.join().unwrap();
        let _ = std::fs::remove_file(socket_path);
    }

    #[test]
    fn worker_periodically_reasserts_the_acknowledged_aggregate() {
        let socket_path = std::env::temp_dir().join(format!(
            "unpeel-herdr-reassert-test-{}-{}.sock",
            std::process::id(),
            REQUEST_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        let listener = UnixListener::bind(&socket_path).unwrap();
        let (request_tx, request_rx) = mpsc::channel();
        let server = spawn_success_server(listener, request_tx, "w2:p9");

        let mut config = test_config();
        config.reassert_interval = Duration::from_millis(20);
        let context = Context {
            socket_path: socket_path.clone(),
            pane_id: "w2:p9".into(),
        };
        let mut reporter = HerdrReporter::spawn(context, Arc::new(|| true), config);
        reporter.update(&model(vec![row("busy", Status::Busy)]));

        let first = receive_method(&request_rx, "pane.report_agent", Duration::from_secs(1));
        let second = receive_method(&request_rx, "pane.report_agent", Duration::from_secs(1));
        assert_eq!(first["params"], second["params"]);

        reporter.shutdown();
        let release = receive_method(&request_rx, "pane.release_agent", Duration::from_secs(1));
        assert_eq!(release["method"], "pane.release_agent");
        server.join().unwrap();
        let _ = std::fs::remove_file(socket_path);
    }

    #[test]
    fn worker_recovers_a_moved_pane_only_by_stable_terminal_identity() {
        let socket_path = std::env::temp_dir().join(format!(
            "unpeel-herdr-move-test-{}-{}.sock",
            std::process::id(),
            REQUEST_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        let listener = UnixListener::bind(&socket_path).unwrap();
        let (request_tx, request_rx) = mpsc::channel();
        let server = thread::spawn(move || loop {
            let (stream, _) = listener.accept().unwrap();
            let request = exchange_request(stream, |request| {
                let method = request.get("method").and_then(Value::as_str);
                let pane_id = request
                    .get("params")
                    .and_then(|params| params.get("pane_id"))
                    .and_then(Value::as_str);
                match (method, pane_id) {
                    (Some("pane.current"), _) => json!({
                        "result": {
                            "type": "pane_info",
                            "pane": {
                                "pane_id": "w1:p1",
                                "terminal_id": "term-unpeel",
                            },
                        },
                    }),
                    (Some("pane.list"), _) => json!({
                        "result": {
                            "type": "pane_list",
                            "panes": [
                                {
                                    "pane_id": "w2:p1",
                                    "terminal_id": "term-focused-neighbor",
                                    "focused": true,
                                },
                                {
                                    "pane_id": "w9:p7",
                                    "terminal_id": "term-unpeel",
                                    "focused": false,
                                },
                            ],
                        },
                    }),
                    (Some("pane.report_agent"), Some("w9:p7"))
                    | (Some("pane.release_agent"), Some("w9:p7")) => {
                        json!({"result": {"accepted": true}})
                    }
                    // A stale id deliberately succeeds in this fixture. The
                    // reporter must never send this request: terminal-id
                    // verification must retarget before lifecycle authority
                    // is claimed.
                    (Some("pane.report_agent"), Some("w1:p1")) => {
                        json!({"result": {"accepted": true}})
                    }
                    _ => json!({
                        "error": {"code": "invalid_request", "message": "unexpected test request"},
                    }),
                }
            });
            let released =
                request.get("method").and_then(Value::as_str) == Some("pane.release_agent");
            request_tx.send(request).unwrap();
            if released {
                return;
            }
        });

        let context = Context {
            socket_path: socket_path.clone(),
            pane_id: "w1:p1".into(),
        };
        let mut reporter = HerdrReporter::spawn(context, Arc::new(|| true), test_config());
        reporter.update(&model(vec![row("busy", Status::Busy)]));

        let deadline = Instant::now() + Duration::from_secs(1);
        let mut saw_exact_current = false;
        let mut saw_old_report = false;
        let mut saw_list = false;
        loop {
            let request = request_rx
                .recv_timeout(deadline.saturating_duration_since(Instant::now()))
                .unwrap();
            match request.get("method").and_then(Value::as_str) {
                Some("pane.current") => {
                    saw_exact_current = request["params"]["caller_pane_id"] == "w1:p1";
                }
                Some("pane.list") => saw_list = true,
                Some("pane.report_agent") if request["params"]["pane_id"] == "w1:p1" => {
                    saw_old_report = true;
                }
                Some("pane.report_agent") if request["params"]["pane_id"] == "w9:p7" => {
                    break;
                }
                _ => {}
            }
        }
        assert!(saw_exact_current);
        assert!(!saw_old_report);
        assert!(saw_list);

        reporter.shutdown();
        let release = receive_method(&request_rx, "pane.release_agent", Duration::from_secs(1));
        assert_eq!(release["params"]["pane_id"], "w9:p7");
        server.join().unwrap();
        let _ = std::fs::remove_file(socket_path);
    }
}
