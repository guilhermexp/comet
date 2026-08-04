//! The four worker tools: spawn, read, wait, kill.
//!
//! A worker is an ordinary engine terminal, so the engine owns it: it outlives
//! this server and the agent turn that created it, it shows up in the Terminal
//! pane, and the engine's own bounds apply unchanged (32 terminals per device,
//! a 30-minute TTL on an exited worker's replay buffer).
//!
//! Two bounds are load-bearing on both readers and neither is optional:
//!
//! - **One absolute deadline** per call, computed before the loop. A per-event
//!   timeout never fires against a stream that is always ready, so a flooding
//!   worker would pin the call forever.
//! - **[`MAX_OUTPUT_BYTES`] of accumulated output**, dropped oldest-first while
//!   `next_seq` still advances for what was dropped. The engine caps its own
//!   replay window at the same figure; a live tail has no such cap, so holding
//!   it whole would be an unbounded allocation driven by the worker.
//!
//! Output is therefore a **tail, not a transcript**. `next_seq` is the cursor:
//! feeding it back as `after_seq` resumes exactly where the last call stopped,
//! which is why a timed-out [`WorkerTools::wait`] still returns what it
//! consumed — otherwise every timeout would punch a hole in the log.

use std::collections::{HashMap, VecDeque};
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64;
use futures::StreamExt;
use serde::Serialize;
use tokio::time::Instant;

use comet_proto::TerminalEvent;

use crate::client::{EngineClient, ToolError};

/// Ceiling on the output one call accumulates, matching the engine's own
/// `MAX_REPLAY_BYTES`. Oldest bytes are dropped first.
pub const MAX_OUTPUT_BYTES: usize = 1024 * 1024;
/// `read_worker`'s default bound — short, because reading is "check on it
/// without committing the turn".
pub const DEFAULT_READ_TIMEOUT_MS: u64 = 2_000;
/// `wait_worker`'s default bound — long, because waiting is the point.
pub const DEFAULT_WAIT_TIMEOUT_MS: u64 = 60_000;
/// Nothing blocks longer than this, however large a `timeout_ms` asks for.
pub const MAX_TIMEOUT_MS: u64 = 600_000;
/// How deep a chain of servers-handing-tools-to-workers may go.
pub const DEFAULT_MAX_WORKER_DEPTH: usize = 2;

/// The PTY-visible depth marker. Observability only: whatever runs in the shell
/// can rewrite it, so it can never be the thing that stops recursion — that
/// check is [`WorkerConfig::max_depth`], which no command string can reach.
pub const DEPTH_ENV_VAR: &str = "COMET_WORKER_DEPTH";

/// How this server was launched.
#[derive(Debug, Clone)]
pub struct WorkerConfig {
    /// The chat every worker is opened under; `OpenTerminal` is chat-scoped.
    pub chat_id: String,
    /// This server's own depth — 0 for the session the harness starts.
    pub depth: usize,
    /// The depth at which `spawn` is refused.
    pub max_depth: usize,
}

impl WorkerConfig {
    pub fn new(chat_id: impl Into<String>, depth: usize) -> Self {
        Self {
            chat_id: chat_id.into(),
            depth,
            max_depth: DEFAULT_MAX_WORKER_DEPTH,
        }
    }
}

/// `spawn_worker`'s reply.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SpawnResult {
    pub worker_id: String,
}

/// `read_worker`'s reply.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReadResult {
    /// Tail of what the worker printed after `after_seq`, capped at
    /// [`MAX_OUTPUT_BYTES`].
    pub output: String,
    /// Last sequence included in `output`; pass it back as `after_seq`.
    pub next_seq: u64,
    /// False once the worker exited or was closed.
    pub running: bool,
}

/// `wait_worker`'s reply: `running` is true only when the call hit its
/// deadline with the worker still alive.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WaitResult {
    pub running: bool,
    /// The worker's exit status; absent while it runs, and also when it was
    /// killed (`CloseTerminal` drops the session before it can be stamped).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signal: Option<String>,
    pub output: String,
    pub next_seq: u64,
}

/// `kill_worker`'s reply.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct KillResult {
    pub ok: bool,
}

/// The tool layer. Owns the `worker_id -> device` map so the agent picks
/// `target_device` once, at spawn, and never repeats it: a worker read on the
/// wrong device is a not-found, so this is not a convenience.
pub struct WorkerTools<C: EngineClient> {
    client: C,
    config: WorkerConfig,
    workers: Mutex<HashMap<String, Option<String>>>,
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

impl<C: EngineClient> WorkerTools<C> {
    pub fn new(client: C, config: WorkerConfig) -> Self {
        Self {
            client,
            config,
            workers: Mutex::new(HashMap::new()),
        }
    }

    pub fn config(&self) -> &WorkerConfig {
        &self.config
    }

    /// Open a PTY under the chat and write `command` into it.
    ///
    /// Two RPCs, and it can fail between them: a `WriteTerminal` failure after
    /// a successful `OpenTerminal` closes the terminal before returning, or the
    /// failure would leave a live PTY whose id the agent never learns, holding
    /// one of the device's 32 slots until the reaper takes it.
    pub async fn spawn(
        &self,
        command: &str,
        cwd: Option<&str>,
        target_device: Option<&str>,
    ) -> Result<SpawnResult, ToolError> {
        if self.config.depth >= self.config.max_depth {
            return Err(ToolError::Refused(format!(
                "worker depth {} is at the maximum of {}: this worker may not spawn \
                 workers of its own",
                self.config.depth, self.config.max_depth
            )));
        }

        let worker_id = self
            .client
            .open_terminal(&self.config.chat_id, target_device)
            .await?;

        let line = worker_line(command, cwd, self.config.depth + 1);
        if let Err(err) = self
            .client
            .write_terminal(&worker_id, &line, target_device)
            .await
        {
            // Best effort: the write error is what the agent needs to see, and
            // a failing close would only bury it.
            if let Err(close_err) = self.client.close_terminal(&worker_id, target_device).await {
                tracing::warn!(worker = %worker_id, error = %close_err,
                    "could not close the terminal of a failed spawn");
            }
            return Err(err);
        }

        // Committed only now: an id mapped by a spawn that then failed is a
        // worker that does not exist, and every later call for it must be a
        // clean not-found.
        lock(&self.workers).insert(worker_id.clone(), target_device.map(str::to_string));
        Ok(SpawnResult { worker_id })
    }

    /// Drain whatever the worker produced after `after_seq`, bounded by
    /// `timeout_ms`.
    pub async fn read(
        &self,
        worker_id: &str,
        after_seq: Option<u64>,
        timeout_ms: Option<u64>,
    ) -> Result<ReadResult, ToolError> {
        let device = self.device_for(worker_id)?;
        let drained = self
            .drain(
                worker_id,
                device.as_deref(),
                after_seq,
                timeout_ms.unwrap_or(DEFAULT_READ_TIMEOUT_MS),
            )
            .await?;
        Ok(ReadResult {
            running: drained.still_running(),
            next_seq: drained.next_seq,
            output: drained.output(),
        })
    }

    /// Block until the worker exits, or until `timeout_ms` elapses.
    ///
    /// A worker that already exited returns immediately: the engine replays its
    /// buffered events and the subscription ends on the `Exit` it carries. A
    /// timed-out wait still returns its output and cursor, so resuming from
    /// `next_seq` loses nothing.
    pub async fn wait(
        &self,
        worker_id: &str,
        after_seq: Option<u64>,
        timeout_ms: Option<u64>,
    ) -> Result<WaitResult, ToolError> {
        let device = self.device_for(worker_id)?;
        let drained = self
            .drain(
                worker_id,
                device.as_deref(),
                after_seq,
                timeout_ms.unwrap_or(DEFAULT_WAIT_TIMEOUT_MS),
            )
            .await?;
        let (exit_code, signal) = match &drained.exit {
            Some((code, signal)) => (Some(*code), signal.clone()),
            None => (None, None),
        };
        Ok(WaitResult {
            running: drained.still_running(),
            exit_code,
            signal,
            next_seq: drained.next_seq,
            output: drained.output(),
        })
    }

    /// Kill the worker and forget it. The engine drops the PTY *and* its replay
    /// buffer, so every later call for this id is a clean not-found.
    pub async fn kill(&self, worker_id: &str) -> Result<KillResult, ToolError> {
        let device = self.device_for(worker_id)?;
        self.client
            .close_terminal(worker_id, device.as_deref())
            .await?;
        lock(&self.workers).remove(worker_id);
        Ok(KillResult { ok: true })
    }

    /// The device a worker was spawned on. A miss is the answer, not a lookup
    /// failure: the tools mint no identity of their own, so an id this server
    /// never handed out cannot be routed anywhere.
    fn device_for(&self, worker_id: &str) -> Result<Option<String>, ToolError> {
        lock(&self.workers)
            .get(worker_id)
            .cloned()
            .ok_or_else(|| ToolError::NotFound(worker_id.to_string()))
    }

    async fn drain(
        &self,
        worker_id: &str,
        device: Option<&str>,
        after_seq: Option<u64>,
        timeout_ms: u64,
    ) -> Result<Drained, ToolError> {
        let mut stream = self
            .client
            .subscribe_terminal(worker_id, after_seq, device)
            .await?;
        // Computed once: a per-event timeout never fires against a stream that
        // is always ready.
        let deadline = Instant::now() + Duration::from_millis(timeout_ms.min(MAX_TIMEOUT_MS));

        let mut drained = Drained {
            buffer: VecDeque::new(),
            next_seq: after_seq.unwrap_or(0),
            exit: None,
            ended: false,
        };
        loop {
            // Explicit, because `timeout_at` polls the inner future first and
            // an always-ready stream would never let the timer be observed.
            if Instant::now() >= deadline {
                break;
            }
            match tokio::time::timeout_at(deadline, stream.next()).await {
                Err(_) => break,
                Ok(None) => {
                    // The engine dropped the subscription without an `Exit`:
                    // the terminal was closed out from under us.
                    drained.ended = true;
                    break;
                }
                Ok(Some(TerminalEvent::Data { seq, data })) => {
                    drained.next_seq = drained.next_seq.max(seq);
                    let bytes = BASE64
                        .decode(&data)
                        .unwrap_or_else(|_| data.as_bytes().to_vec());
                    drained.buffer.extend(bytes);
                    // Oldest-first, and `next_seq` keeps advancing for what was
                    // dropped: the cursor tracks what the engine sent, not what
                    // survived the cap.
                    let overflow = drained.buffer.len().saturating_sub(MAX_OUTPUT_BYTES);
                    if overflow > 0 {
                        drained.buffer.drain(..overflow);
                    }
                }
                Ok(Some(TerminalEvent::Exit {
                    seq,
                    exit_code,
                    signal,
                })) => {
                    drained.next_seq = drained.next_seq.max(seq);
                    drained.exit = Some((exit_code, signal));
                    break;
                }
            }
        }
        Ok(drained)
    }
}

struct Drained {
    buffer: VecDeque<u8>,
    next_seq: u64,
    exit: Option<(i32, Option<String>)>,
    ended: bool,
}

impl Drained {
    fn still_running(&self) -> bool {
        self.exit.is_none() && !self.ended
    }

    fn output(mut self) -> String {
        String::from_utf8_lossy(self.buffer.make_contiguous()).into_owned()
    }
}

/// Single-quoting that survives every shell the engine might have spawned:
/// everything is literal inside `'…'`, and an embedded quote closes, escapes
/// and reopens. `'\''` is read the same way by sh, bash, zsh, fish and csh.
fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', r"'\''"))
}

/// The one line written into a worker's PTY.
///
/// Three properties, and the shape is chosen for all three at once:
///
/// - **The interactive shell must parse nothing of `command`.** `OpenTerminal`
///   runs `$SHELL` verbatim (`crates/engine/src/terminals.rs:109-123`), which
///   may be fish or tcsh, where POSIX glue does not parse; and even in bash a
///   trailing `&`, a dangling `\` or an unbalanced quote in `command` would
///   break a line it was pasted into. So `command` travels as ONE quoted argv
///   element to an explicit `/bin/sh`, and a malformed command is `sh`'s
///   nonzero exit instead of a line the outer shell rejects.
/// - **The PTY must die when the worker does.** The engine stamps
///   [`TerminalEvent::Exit`] only when the pty child exits, and only EXITED
///   sessions are reaped — a worker that returned the shell to its prompt would
///   hold one of the device's 32 terminal slots forever. `exec` replaces the
///   interactive shell, so the worker *is* the pty child and its status is the
///   exit code the agent reads.
/// - **`cd` must not read its operand as an option.** `cwd = "-P"` without the
///   `--` would silently succeed into `$HOME` and run the worker in the wrong
///   checkout, which is the isolation `cwd` exists to provide.
///
/// `exec` and a quoted simple command are all the outer shell ever sees; every
/// POSIX construct lives inside the `/bin/sh` script. Workers therefore need a
/// POSIX `/bin/sh`, which is every platform the engine's `cd`-based cwd
/// handling already assumed.
fn worker_line(command: &str, cwd: Option<&str>, worker_depth: usize) -> String {
    let script = match cwd {
        Some(cwd) if !cwd.is_empty() => format!(
            "export {DEPTH_ENV_VAR}={worker_depth}; cd -- {} && {command}",
            shell_quote(cwd)
        ),
        _ => format!("export {DEPTH_ENV_VAR}={worker_depth}; {command}"),
    };
    format!("exec /bin/sh -c {}\n", shell_quote(&script))
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU64, Ordering};

    use async_trait::async_trait;
    use futures::stream::{self, BoxStream};

    use super::*;

    #[derive(Debug, Default, Clone, PartialEq, Eq)]
    struct Calls {
        opened: Vec<(String, Option<String>)>,
        writes: Vec<(String, String, Option<String>)>,
        subscribes: Vec<(String, Option<u64>, Option<String>)>,
        closes: Vec<(String, Option<String>)>,
    }

    /// Replays a scripted event list, filtered by `after_seq` the way the
    /// engine's replay window does.
    struct Stub {
        calls: Mutex<Calls>,
        terminal_id: String,
        write_error: Option<ToolError>,
        script: Vec<TerminalEvent>,
        /// Stay pending after the script instead of ending the stream — a live
        /// worker that has gone quiet.
        tail_forever: bool,
        /// Ignore the script and emit `Data` events that are always ready.
        flood: bool,
    }

    impl Stub {
        fn new() -> Self {
            Self {
                calls: Mutex::new(Calls::default()),
                terminal_id: "term-1".into(),
                write_error: None,
                script: Vec::new(),
                tail_forever: false,
                flood: false,
            }
        }
        fn script(mut self, script: Vec<TerminalEvent>) -> Self {
            self.script = script;
            self
        }
        fn tail_forever(mut self) -> Self {
            self.tail_forever = true;
            self
        }
        fn flood(mut self) -> Self {
            self.flood = true;
            self
        }
        fn write_error(mut self, message: &str) -> Self {
            self.write_error = Some(ToolError::Engine(message.into()));
            self
        }
        fn calls(&self) -> Calls {
            lock(&self.calls).clone()
        }
    }

    fn data(seq: u64, text: &str) -> TerminalEvent {
        TerminalEvent::Data {
            seq,
            data: BASE64.encode(text.as_bytes()),
        }
    }

    fn exit(seq: u64, code: i32, signal: Option<&str>) -> TerminalEvent {
        TerminalEvent::Exit {
            seq,
            exit_code: code,
            signal: signal.map(str::to_string),
        }
    }

    fn seq_of(event: &TerminalEvent) -> u64 {
        match event {
            TerminalEvent::Data { seq, .. } | TerminalEvent::Exit { seq, .. } => *seq,
        }
    }

    #[async_trait]
    impl EngineClient for Stub {
        async fn open_terminal(
            &self,
            chat: &str,
            device: Option<&str>,
        ) -> Result<String, ToolError> {
            lock(&self.calls)
                .opened
                .push((chat.into(), device.map(str::to_string)));
            Ok(self.terminal_id.clone())
        }

        async fn write_terminal(
            &self,
            id: &str,
            data: &str,
            device: Option<&str>,
        ) -> Result<(), ToolError> {
            lock(&self.calls)
                .writes
                .push((id.into(), data.into(), device.map(str::to_string)));
            match &self.write_error {
                Some(err) => Err(err.clone()),
                None => Ok(()),
            }
        }

        async fn subscribe_terminal(
            &self,
            id: &str,
            after_seq: Option<u64>,
            device: Option<&str>,
        ) -> Result<BoxStream<'static, TerminalEvent>, ToolError> {
            lock(&self.calls)
                .subscribes
                .push((id.into(), after_seq, device.map(str::to_string)));
            if self.flood {
                let seq = AtomicU64::new(after_seq.unwrap_or(0));
                let chunk = "x".repeat(4096);
                return Ok(stream::repeat_with(move || {
                    data(seq.fetch_add(1, Ordering::Relaxed) + 1, &chunk)
                })
                .boxed());
            }
            let after = after_seq.unwrap_or(0);
            let replay: Vec<TerminalEvent> = self
                .script
                .iter()
                .filter(|event| seq_of(event) > after)
                .cloned()
                .collect();
            let replay = stream::iter(replay);
            Ok(if self.tail_forever {
                replay.chain(stream::pending()).boxed()
            } else {
                replay.boxed()
            })
        }

        async fn close_terminal(&self, id: &str, device: Option<&str>) -> Result<(), ToolError> {
            lock(&self.calls)
                .closes
                .push((id.into(), device.map(str::to_string)));
            Ok(())
        }
    }

    fn tools(stub: Stub) -> WorkerTools<Stub> {
        WorkerTools::new(stub, WorkerConfig::new("chat-1", 0))
    }

    // ---- spawn ------------------------------------------------------------

    #[tokio::test]
    async fn spawn_returns_the_terminal_id() {
        let tools = tools(Stub::new());
        let spawned = tools.spawn("echo hi", None, None).await.expect("spawn");
        assert_eq!(spawned.worker_id, "term-1");
        let calls = tools.client.calls();
        assert_eq!(calls.opened, [("chat-1".to_string(), None)]);
        assert_eq!(
            calls.writes,
            [(
                "term-1".to_string(),
                "exec /bin/sh -c 'export COMET_WORKER_DEPTH=1; echo hi'\n".to_string(),
                None
            )]
        );
        assert!(calls.closes.is_empty());
    }

    #[tokio::test]
    async fn spawn_closes_the_terminal_when_the_write_fails() {
        let tools = tools(Stub::new().write_error("pty is gone"));
        let err = tools
            .spawn("echo hi", None, None)
            .await
            .expect_err("write fails");
        // The original write error reaches the agent, not a close error.
        assert_eq!(err, ToolError::Engine("pty is gone".into()));
        // No orphan PTY: the terminal opened by the failed spawn is closed.
        assert_eq!(tools.client.calls().closes, [("term-1".to_string(), None)]);
    }

    #[tokio::test]
    async fn spawn_that_failed_leaves_no_worker() {
        let tools = tools(Stub::new().write_error("pty is gone"));
        tools
            .spawn("echo hi", None, None)
            .await
            .expect_err("write fails");
        let not_found = ToolError::NotFound("term-1".into());
        assert_eq!(
            tools.read("term-1", None, Some(10)).await,
            Err(not_found.clone())
        );
        assert_eq!(
            tools.wait("term-1", None, Some(10)).await,
            Err(not_found.clone())
        );
        assert_eq!(tools.kill("term-1").await, Err(not_found));
        // And nothing was subscribed or closed a second time for it.
        assert!(tools.client.calls().subscribes.is_empty());
    }

    #[tokio::test]
    async fn spawn_records_the_device_for_later_calls() {
        let tools = tools(Stub::new().script(vec![exit(1, 0, None)]));
        let spawned = tools
            .spawn("echo hi", None, Some("device-b"))
            .await
            .expect("spawn");
        tools
            .read(&spawned.worker_id, None, Some(50))
            .await
            .expect("read");
        tools
            .wait(&spawned.worker_id, None, Some(50))
            .await
            .expect("wait");
        tools.kill(&spawned.worker_id).await.expect("kill");

        let calls = tools.client.calls();
        let device = Some("device-b".to_string());
        assert_eq!(calls.opened, [("chat-1".to_string(), device.clone())]);
        // The agent named the device once, at spawn; every later call reused it.
        assert_eq!(
            calls.subscribes,
            [
                ("term-1".to_string(), None, device.clone()),
                ("term-1".to_string(), None, device.clone()),
            ]
        );
        assert_eq!(calls.closes, [("term-1".to_string(), device)]);
    }

    #[tokio::test]
    async fn spawn_quotes_a_cwd_containing_spaces() {
        let tools = tools(Stub::new());
        tools
            .spawn("cargo test", Some("/Users/First Last/wt"), None)
            .await
            .expect("spawn");
        let calls = tools.client.calls();
        assert_eq!(
            calls.writes[0].1,
            "exec /bin/sh -c 'export COMET_WORKER_DEPTH=1; \
             cd -- '\\''/Users/First Last/wt'\\'' && cargo test'\n"
        );
    }

    #[test]
    fn the_worker_line_hides_every_shell_construct_from_the_outer_shell() {
        // A command full of glue that would break the line it was pasted into:
        // a background `&`, an unbalanced quote, a trailing backslash.
        let line = worker_line("npm run dev & echo 'x \\", None, 1);
        let (prefix, rest) = line.split_once(" -c ").expect("an argv-shaped line");
        assert_eq!(prefix, "exec /bin/sh");
        // Everything after `-c` is ONE single-quoted element, so the
        // interactive shell parses none of it: every `'` inside is escaped.
        let body = rest.trim_end_matches('\n');
        assert!(body.starts_with('\'') && body.ends_with('\''), "{body}");
        assert!(
            !body[1..body.len() - 1].contains("'") || body.contains(r"'\''"),
            "embedded quotes must be escaped, not left bare: {body}"
        );
        assert!(line.ends_with('\n'));
    }

    #[test]
    fn the_worker_line_ends_cd_option_parsing() {
        // `cd -P` would succeed into $HOME and run the worker in the wrong
        // checkout while still looking like a success.
        let line = worker_line("pwd", Some("-P"), 1);
        assert!(line.contains(r"cd -- '\''-P'\''"), "{line}");
    }

    #[tokio::test]
    async fn spawn_past_max_depth_is_refused() {
        let tools = WorkerTools::new(
            Stub::new(),
            WorkerConfig {
                chat_id: "chat-1".into(),
                depth: 2,
                max_depth: 2,
            },
        );
        let err = tools
            .spawn("echo hi", None, None)
            .await
            .expect_err("refused at the ceiling");
        assert!(matches!(err, ToolError::Refused(_)), "got {err:?}");
        // Refused before the spawn, not cleaned up after it.
        assert!(tools.client.calls().opened.is_empty());
    }

    // ---- read -------------------------------------------------------------

    #[tokio::test]
    async fn read_returns_within_the_timeout_on_a_silent_worker() {
        let tools = tools(Stub::new().tail_forever());
        let spawned = tools.spawn("sleep 30", None, None).await.expect("spawn");
        let started = std::time::Instant::now();
        let read = tools
            .read(&spawned.worker_id, None, Some(60))
            .await
            .expect("read");
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "a caught-up subscription must not hang"
        );
        assert_eq!(read.output, "");
        assert_eq!(read.next_seq, 0);
        assert!(read.running);
    }

    #[tokio::test]
    async fn read_returns_within_the_timeout_on_a_chatty_worker() {
        // An always-ready stream: only an absolute deadline ends this call.
        let tools = tools(Stub::new().flood());
        let spawned = tools.spawn("yes", None, None).await.expect("spawn");
        let started = std::time::Instant::now();
        let read = tools
            .read(&spawned.worker_id, None, Some(60))
            .await
            .expect("read");
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "a flooding worker must not pin the call"
        );
        assert!(read.running);
        assert!(read.next_seq > 0);
    }

    #[tokio::test]
    async fn read_resumes_from_after_seq_without_gaps() {
        let tools = tools(
            Stub::new()
                .script(vec![data(1, "a"), data(2, "b"), data(3, "c")])
                .tail_forever(),
        );
        let spawned = tools.spawn("echo abc", None, None).await.expect("spawn");
        let first = tools
            .read(&spawned.worker_id, None, Some(60))
            .await
            .expect("read");
        assert_eq!(first.output, "abc");
        assert_eq!(first.next_seq, 3);

        let resumed = tools
            .read(&spawned.worker_id, Some(1), Some(60))
            .await
            .expect("resumed read");
        assert_eq!(resumed.output, "bc");
        assert_eq!(resumed.next_seq, 3);
    }

    #[tokio::test]
    async fn read_caps_output_and_still_advances_next_seq() {
        // 4 KiB per event, always ready: far past the cap inside the deadline.
        let tools = tools(Stub::new().flood());
        let spawned = tools.spawn("yes", None, None).await.expect("spawn");
        let read = tools
            .read(&spawned.worker_id, None, Some(80))
            .await
            .expect("read");
        assert_eq!(
            read.output.len(),
            MAX_OUTPUT_BYTES,
            "the flood must be trimmed to the cap, not grow the heap"
        );
        // The cursor tracks what the engine sent, not what survived the cap.
        assert!(
            read.next_seq as usize > MAX_OUTPUT_BYTES / 4096,
            "next_seq must advance for dropped bytes too (got {})",
            read.next_seq
        );
    }

    // ---- wait -------------------------------------------------------------

    #[tokio::test]
    async fn wait_returns_the_exit_code_and_signal_on_exit() {
        let tools = tools(
            Stub::new()
                .script(vec![data(1, "hi\n"), exit(2, 3, Some("SIGTERM"))])
                .tail_forever(),
        );
        let spawned = tools.spawn("false", None, None).await.expect("spawn");
        let waited = tools
            .wait(&spawned.worker_id, None, Some(5_000))
            .await
            .expect("wait");
        assert!(!waited.running);
        assert_eq!(waited.exit_code, Some(3));
        assert_eq!(waited.signal.as_deref(), Some("SIGTERM"));
        assert_eq!(waited.output, "hi\n");
        assert_eq!(waited.next_seq, 2);
    }

    #[tokio::test]
    async fn wait_on_an_already_exited_worker_returns_immediately() {
        // No live tail: the engine replays the buffer and drops the sender
        // without ever registering a subscriber.
        let tools = tools(Stub::new().script(vec![data(1, "done\n"), exit(2, 0, None)]));
        let spawned = tools.spawn("true", None, None).await.expect("spawn");
        let started = std::time::Instant::now();
        let waited = tools
            .wait(&spawned.worker_id, None, Some(600_000))
            .await
            .expect("wait");
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "a replayed Exit must end the wait at once"
        );
        assert!(!waited.running);
        assert_eq!(waited.exit_code, Some(0));
    }

    #[tokio::test]
    async fn wait_that_times_out_returns_its_output_and_cursor() {
        let tools = tools(Stub::new().script(vec![data(1, "partial")]).tail_forever());
        let spawned = tools.spawn("sleep 30", None, None).await.expect("spawn");
        let waited = tools
            .wait(&spawned.worker_id, None, Some(60))
            .await
            .expect("wait");
        assert!(waited.running);
        assert_eq!(waited.exit_code, None);
        // The output read during a timed-out wait is not lost.
        assert_eq!(waited.output, "partial");
        assert_eq!(waited.next_seq, 1);
    }

    #[tokio::test]
    async fn wait_resumed_from_next_seq_yields_the_rest_exactly_once() {
        let tools = tools(
            Stub::new()
                .script(vec![data(1, "a"), data(2, "b"), exit(3, 0, None)])
                .tail_forever(),
        );
        let spawned = tools.spawn("worker", None, None).await.expect("spawn");
        // A short wait that ends mid-output.
        let first = tools
            .wait(&spawned.worker_id, None, Some(60))
            .await
            .expect("wait");
        assert!(!first.running, "the scripted Exit is in the replay");
        assert_eq!(first.output, "ab");

        let resumed = tools
            .wait(&spawned.worker_id, Some(1), Some(60))
            .await
            .expect("resumed wait");
        // No dupe of "a", no hole where "b" was.
        assert_eq!(resumed.output, "b");
        assert_eq!(resumed.exit_code, Some(0));
        assert_eq!(resumed.next_seq, 3);
    }

    // ---- kill -------------------------------------------------------------

    #[tokio::test]
    async fn kill_closes_and_forgets_the_worker() {
        let tools = tools(Stub::new().tail_forever());
        let spawned = tools.spawn("sleep 30", None, None).await.expect("spawn");
        assert_eq!(
            tools.kill(&spawned.worker_id).await,
            Ok(KillResult { ok: true })
        );
        assert_eq!(tools.client.calls().closes, [("term-1".to_string(), None)]);
        // The engine drops the replay buffer with the session, so the id is
        // dead to every later call.
        assert_eq!(
            tools.read(&spawned.worker_id, None, Some(10)).await,
            Err(ToolError::NotFound("term-1".into()))
        );
        assert_eq!(
            tools.kill(&spawned.worker_id).await,
            Err(ToolError::NotFound("term-1".into()))
        );
    }
}
