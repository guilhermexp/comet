//! Opt-in LIVE proof that an OMP conversation keeps its provider prompt cache
//! when its process ends and a new one resumes it (openspec change
//! `preserve-omp-prompt-cache-across-turns`, spec `omp-turn-continuity`).
//!
//! Skipped unless `COMET_OMP_LIVE_CACHE=1`: it drives the installed, signed-in
//! `omp` binary and spends real tokens (small prompts, one tool call each).
//!
//! * `COMET_OMP_LIVE_TURNS`   turns, one process each (default 4, minimum 2)
//! * `COMET_OMP_LIVE_MODEL`   `provider/model` (default `anthropic/claude-sonnet-5-5`)
//! * `COMET_OMP_LIVE_OUT`     directory for `report.txt` and the session copy
//! * `COMET_OMP_LIVE_CAPTURE` `1` sets `PI_REQ_DEBUG=1`: omp then writes every
//!   provider request/response to `rr-session-N.json` in `<out>/capture/`
//!   (its working directory). No omp change; the request-diff evidence uses it.
//!
//! The harness registers host tools through the fixture workers controller, so
//! `set_host_tools` reaches the real omp exactly as in a Comet chat, and each
//! turn is a new `OmpHarness::run` with `resume` = the previous session file:
//! a process restart between every pair of turns.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use futures::StreamExt as _;
use serde_json::Value;
use zeron_harness::{
    CancellationToken, Harness, HarnessError, OmpHarness, RunControls, SteerMessage,
};
use zeron_proto::{AgentEvent, DoneStatus, HarnessId, ReasoningLevel, RunRequest, SandboxLevel};

const CACHE_SLACK_TOKENS: u64 = 5_000;

fn workers_controller() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/fake-workers-controller-mcp.sh")
}

#[derive(Debug, Default)]
struct TurnStat {
    /// Prompt tokens (input + cacheRead + cacheWrite) of the turn's first request.
    first_prompt: u64,
    first_cache_read: u64,
    first_cache_write: u64,
    /// Prompt tokens of the turn's last request.
    last_prompt: u64,
    requests: usize,
    roster_notices: usize,
    thinking_dropped: usize,
    /// Signed thinking blocks the provider returned in this turn: without
    /// any, "no thinking was dropped" proves nothing.
    thinking_blocks: usize,
}

#[derive(Debug, Default)]
struct Analysis {
    digests: Vec<String>,
    turns: Vec<TurnStat>,
    exits: usize,
}

fn prompt_tokens(usage: &Value) -> (u64, u64, u64) {
    let get = |key: &str| usage.get(key).and_then(Value::as_u64).unwrap_or(0);
    (get("input"), get("cacheRead"), get("cacheWrite"))
}

/// Reads one omp session JSONL: digests, per-turn cache usage, and the two
/// markers of a broken prefix (`tool-roster-notice`, `thinking_dropped`).
fn analyze(path: &Path) -> Analysis {
    let text = std::fs::read_to_string(path).expect("session file readable");
    let mut analysis = Analysis::default();
    let mut pending_notices = 0usize;
    for line in text.lines() {
        let Ok(entry) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        match entry.get("type").and_then(Value::as_str) {
            Some("custom") => match entry.get("customType").and_then(Value::as_str) {
                Some("system-prompt-digest") => {
                    if let Some(digest) = entry.get("data").and_then(Value::as_str) {
                        analysis.digests.push(digest.to_owned());
                    }
                }
                Some("session_exit") => analysis.exits += 1,
                _ => {}
            },
            Some("custom_message") => {
                // The notice is queued BEFORE the user message it precedes.
                if entry.get("customType").and_then(Value::as_str) == Some("tool-roster-notice") {
                    pending_notices += 1;
                }
            }
            Some("message") => {
                let message = &entry["message"];
                match message.get("role").and_then(Value::as_str) {
                    Some("user") => {
                        analysis.turns.push(TurnStat {
                            roster_notices: std::mem::take(&mut pending_notices),
                            ..TurnStat::default()
                        });
                    }
                    Some("assistant") => {
                        let Some(turn) = analysis.turns.last_mut() else {
                            continue;
                        };
                        if let Some(usage) = message.get("usage") {
                            let (input, read, write) = prompt_tokens(usage);
                            let prompt = input + read + write;
                            if turn.requests == 0 {
                                turn.first_prompt = prompt;
                                turn.first_cache_read = read;
                                turn.first_cache_write = write;
                            }
                            turn.last_prompt = prompt;
                            turn.requests += 1;
                        }
                        turn.thinking_blocks += message
                            .get("content")
                            .and_then(Value::as_array)
                            .map_or(0, |parts| {
                                parts
                                    .iter()
                                    .filter(|part| {
                                        part.get("type").and_then(Value::as_str) == Some("thinking")
                                    })
                                    .count()
                            });
                        if message
                            .get("inputTransformations")
                            .and_then(Value::as_array)
                            .is_some_and(|items| {
                                items.iter().any(|item| {
                                    item.get("type").and_then(Value::as_str)
                                        == Some("thinking_dropped")
                                })
                            })
                        {
                            turn.thinking_dropped += 1;
                        }
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }
    analysis
}

fn report(analysis: &Analysis) -> String {
    let mut out = String::new();
    let distinct: std::collections::BTreeSet<_> = analysis.digests.iter().collect();
    out.push_str(&format!(
        "system-prompt-digest entries: {:?} (distinct: {})\nsession_exit entries: {}\n",
        analysis.digests,
        distinct.len(),
        analysis.exits
    ));
    out.push_str("turn | requests | first prompt | cacheRead | cacheWrite | prev last prompt | headroom | roster notices | thinking blocks | thinking_dropped\n");
    let mut previous_last = None::<u64>;
    for (index, turn) in analysis.turns.iter().enumerate() {
        let (prev, headroom) = match previous_last {
            Some(prev) => (
                prev.to_string(),
                (turn.first_cache_read as i64 - prev as i64).to_string(),
            ),
            None => ("-".into(), "-".into()),
        };
        out.push_str(&format!(
            "{} | {} | {} | {} | {} | {} | {} | {} | {} | {}\n",
            index + 1,
            turn.requests,
            turn.first_prompt,
            turn.first_cache_read,
            turn.first_cache_write,
            prev,
            headroom,
            turn.roster_notices,
            turn.thinking_blocks,
            turn.thinking_dropped
        ));
        previous_last = Some(turn.last_prompt);
    }
    out
}

fn request(model: &str, cwd: &Path, resume: Option<String>, prompt: String) -> RunRequest {
    RunRequest {
        access: Default::default(),
        extra_mcp: Default::default(),
        plan: Default::default(),
        prompt,
        harness: Some(HarnessId::Omp),
        model: Some(model.to_owned()),
        reasoning: Some(ReasoningLevel::High),
        model_options: serde_json::Map::new(),
        cwd: cwd.to_string_lossy().into_owned(),
        sandbox: SandboxLevel::WorkspaceWrite,
        auto_approve: true,
        // Registers the `workers` host tool through the fixture controller.
        enable_workers_mcp: true,
        workers_parent_chat_id: Some("live-cache-chat".into()),
        sessions: None,
        resume,
        attachments: Vec::new(),
        worktree: None,
        mcp: None,
        env: Default::default(),
    }
}

/// Records the highest `rr-session-N.json` the process that just ended wrote,
/// so a diff can tell which requests belong to which process.
fn note_process_boundary(capture: &Path) {
    let last = std::fs::read_dir(capture)
        .unwrap()
        .filter_map(|entry| {
            let name = entry.ok()?.file_name().into_string().ok()?;
            name.strip_prefix("rr-session-")?
                .strip_suffix(".json")?
                .parse::<u64>()
                .ok()
        })
        .max()
        .unwrap_or(0);
    let path = capture.join("boundaries.json");
    let mut ends: Vec<u64> = std::fs::read_to_string(&path)
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default();
    ends.push(last);
    std::fs::write(path, serde_json::to_string(&ends).unwrap()).unwrap();
}

/// Waits until the session file holds `expected` `session_exit` entries: the
/// file accumulates across the processes that resumed it, so an earlier
/// process's marker must not satisfy the wait for the one that just ended.
async fn wait_for_session_exits(path: &Path, expected: usize) {
    let exits = |path: &Path| {
        std::fs::read_to_string(path)
            .map(|text| text.matches(r#""customType":"session_exit""#).count())
            .unwrap_or(0)
    };
    let deadline = tokio::time::Instant::now() + Duration::from_secs(15);
    while tokio::time::Instant::now() < deadline {
        if exits(path) >= expected {
            return;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    panic!(
        "the omp process of turn {expected} never wrote its session_exit to {} ({} found)",
        path.display(),
        exits(path)
    );
}

struct Live {
    turns: usize,
    model: String,
    out: PathBuf,
    capture: bool,
    /// omp's working directory; with `PI_REQ_DEBUG=1` it also receives
    /// `rr-session-N.json`.
    cwd: PathBuf,
    nonce: String,
    harness: OmpHarness,
}

/// `None` unless `COMET_OMP_LIVE_CACHE=1`.
fn live() -> Option<Live> {
    if std::env::var("COMET_OMP_LIVE_CACHE").as_deref() != Ok("1") {
        eprintln!("skipped: set COMET_OMP_LIVE_CACHE=1 to run the live OMP cache tests");
        return None;
    }
    let turns: usize = std::env::var("COMET_OMP_LIVE_TURNS")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(4)
        .max(2);
    let model = std::env::var("COMET_OMP_LIVE_MODEL")
        .unwrap_or_else(|_| "anthropic/claude-sonnet-5-5".into());
    let out = std::env::var_os("COMET_OMP_LIVE_OUT")
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::temp_dir().join("comet-omp-live-cache"));
    let capture = std::env::var("COMET_OMP_LIVE_CAPTURE").as_deref() == Ok("1");
    let cwd = out.join("capture");
    std::fs::create_dir_all(&cwd).unwrap();
    let mut env = HashMap::new();
    if capture {
        env.insert("PI_REQ_DEBUG".to_owned(), "1".to_owned());
    }
    let harness = OmpHarness::new()
        .with_env(env)
        .with_workers_mcp_executable(workers_controller());
    Some(Live {
        turns,
        model,
        out,
        capture,
        cwd,
        nonce: uuid::Uuid::new_v4().simple().to_string(),
        harness,
    })
}

fn controls() -> (RunControls, tokio::sync::mpsc::Sender<SteerMessage>) {
    let (steer_tx, steer_rx) = tokio::sync::mpsc::channel(4);
    let controls = RunControls {
        realtime: None,
        execution_lease: None,
        request_input: Box::new(|_| {
            let (tx, rx) = tokio::sync::oneshot::channel();
            let _ = tx.send(Vec::new());
            rx
        }),
        steering: steer_rx,
        interrupt: CancellationToken::new(),
        chat_id: String::new(),
        turn: Default::default(),
    };
    (controls, steer_tx)
}

impl Live {
    /// The nonce makes every conversation unique: a rerun cannot borrow the
    /// provider cache an earlier identical run just wrote (1h TTL). The
    /// problem needs reasoning on purpose: the check that no signed thinking
    /// is dropped needs thinking blocks to exist in the earlier turns.
    fn prompt(&self, turn: usize) -> String {
        let limit = 1_000 + turn * 37;
        format!(
            "Run {nonce}, turn {turn}. Work out by inclusion-exclusion, step by step, how many \
             positive integers below {limit} are divisible by 3 or by 5 but not by both. Then \
             verify your count by running a one-line shell loop with the bash tool, and reply \
             with the verified count only.",
            nonce = self.nonce
        )
    }

    /// Reads one turn to its `Done`: the session file it ended with.
    async fn finish_turn(
        &self,
        stream: &mut futures::stream::BoxStream<'static, Result<AgentEvent, HarnessError>>,
        turn: usize,
    ) -> String {
        let mut done = None;
        let mut failure = None;
        let finished = tokio::time::timeout(Duration::from_secs(240), async {
            while let Some(event) = stream.next().await {
                match event.expect("stream item") {
                    AgentEvent::Error { message } => failure = Some(message),
                    AgentEvent::Done {
                        status, session_id, ..
                    } => {
                        done = Some((status, session_id));
                        break;
                    }
                    _ => {}
                }
            }
        })
        .await;
        assert!(finished.is_ok(), "turn {turn} timed out");
        let (status, session_id) = done.expect("turn ended with Done");
        assert_eq!(
            status,
            DoneStatus::Completed,
            "turn {turn} failed: {failure:?}"
        );
        session_id.expect("Done carries the session file")
    }

    fn conclude(&self, session: &Path, shape: &str, first_notice_allowed: bool) {
        std::fs::copy(session, self.out.join("session.jsonl")).unwrap();
        let analysis = analyze(session);
        let text = format!(
            "model: {}\nturns: {} ({shape})\nsession: {}\n{}",
            self.model,
            self.turns,
            session.display(),
            report(&analysis)
        );
        std::fs::write(self.out.join("report.txt"), &text).unwrap();
        println!("{text}");

        assert_eq!(analysis.turns.len(), self.turns, "{text}");
        let distinct: std::collections::BTreeSet<_> = analysis.digests.iter().collect();
        assert_eq!(distinct.len(), 1, "system prompt digest changed:\n{text}");
        let earlier_thinking: usize = analysis
            .turns
            .iter()
            .take(self.turns - 1)
            .map(|turn| turn.thinking_blocks)
            .sum();
        assert!(
            earlier_thinking > 0,
            "no thinking block was produced before the last turn, so the dropped-thinking check \
             is vacuous (raise COMET_OMP_LIVE_MODEL's reasoning or change the prompt):\n{text}"
        );
        for (index, turn) in analysis.turns.iter().enumerate() {
            assert_eq!(
                turn.thinking_dropped,
                0,
                "turn {} dropped thinking:\n{text}",
                index + 1
            );
            if index > 0 || !first_notice_allowed {
                assert_eq!(
                    turn.roster_notices,
                    0,
                    "turn {} queued a tool-roster notice:\n{text}",
                    index + 1
                );
            }
        }
        for pair in analysis.turns.windows(2) {
            let floor = pair[0].last_prompt.saturating_sub(CACHE_SLACK_TOKENS);
            assert!(
                pair[1].first_cache_read >= floor,
                "turn start read {} < previous prompt {} - {CACHE_SLACK_TOKENS}:\n{text}",
                pair[1].first_cache_read,
                pair[0].last_prompt
            );
        }
    }
}

/// A process restart between every pair of turns: each turn is a new
/// `OmpHarness::run` resuming the previous session file.
#[tokio::test]
async fn a_resumed_omp_process_keeps_the_prompt_cache() {
    let Some(live) = live() else { return };
    let mut session: Option<String> = None;
    for turn in 1..=live.turns {
        let (controls, steer_tx) = controls();
        let mut stream = live
            .harness
            .run(
                request(&live.model, &live.cwd, session.clone(), live.prompt(turn)),
                controls,
            )
            .await
            .expect("omp run starts");
        let session_file = live.finish_turn(&mut stream, turn).await;
        drop(stream);
        drop(steer_tx);
        wait_for_session_exits(Path::new(&session_file), turn).await;
        if live.capture {
            note_process_boundary(&live.cwd);
        }
        session = Some(session_file);
    }
    live.conclude(
        Path::new(&session.expect("at least one turn ran")),
        "one omp process each",
        true,
    );
}

/// No restart at all: every later turn is a message to the warm runtime, sent
/// as a `prompt` into the same real omp process.
#[tokio::test]
async fn a_warm_omp_process_takes_every_next_message_as_a_prompt() {
    let Some(live) = live() else { return };
    let (controls, steer_tx) = controls();
    let mut stream = live
        .harness
        .run(
            request(&live.model, &live.cwd, None, live.prompt(1)),
            controls,
        )
        .await
        .expect("omp run starts");
    let session_file = live.finish_turn(&mut stream, 1).await;
    for turn in 2..=live.turns {
        steer_tx
            .send(SteerMessage {
                prompt: live.prompt(turn),
                message_id: Some(format!("live-{turn}")),
                attachments: Vec::new(),
                config: None,
            })
            .await
            .unwrap();
        assert_eq!(live.finish_turn(&mut stream, turn).await, session_file);
    }
    let exits_while_warm = std::fs::read_to_string(&session_file)
        .unwrap()
        .matches(r#""customType":"session_exit""#)
        .count();
    assert_eq!(
        exits_while_warm, 0,
        "the process must stay up between turns"
    );
    drop(stream);
    drop(steer_tx);
    wait_for_session_exits(Path::new(&session_file), 1).await;
    live.conclude(Path::new(&session_file), "ONE warm omp process", false);
}
