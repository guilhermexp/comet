//! M2 end-to-end tests: doc-queued commands → host executor → harness stream →
//! journal + broadcast + folded doc entries, plus interrupt/recovery/idempotence
//! and the RPC surface over the in-memory transport.

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use futures::StreamExt;
use futures::stream::BoxStream;
use loro::LoroDoc;

use zeron_doc::{
    MessagePart, MessageRole, MessageStatus, SegmentWriter, SessionCommandEntry,
    SessionCommandPayload, SessionCommandStatus, SessionDoc, SessionMessageEntry,
};
use zeron_engine::{EngineCore, HarnessRegistry, RunJournal};
use zeron_harness::mock::MockHarness;
use zeron_harness::{
    Harness, HarnessError, LiveVoiceContextKind, LiveVoiceControl, LiveVoiceEvent, LiveVoiceHandle,
    LiveVoiceRequest, RunControls,
};
use zeron_proto::{
    AgentEvent, ChatConfig, DoneStatus, HarnessId, LiveVoicePhase, Model, ReasoningLevel,
    RunRequest, SandboxLevel, SessionStatus, SteeringMode, ToolCall,
};
use zeron_sync::DocsStore;

const CHAT: &str = "chat-e2e";
const VIEWER: &str = "viewer-device";

fn run_request(prompt: &str) -> RunRequest {
    RunRequest {
        prompt: prompt.into(),
        harness: None,
        model: None,
        reasoning: None,
        model_options: Default::default(),
        cwd: "/tmp".into(),
        sandbox: SandboxLevel::WorkspaceWrite,
        auto_approve: true,
        enable_workers_mcp: false,
        workers_parent_chat_id: None,
        attachments: Vec::new(),
        worktree: None,
        resume: None,
    }
}

fn done(status: DoneStatus) -> AgentEvent {
    AgentEvent::Done {
        status,
        result: None,
        error: None,
        session_id: Some("hs-1".into()),
    }
}

fn mock_script() -> Vec<AgentEvent> {
    vec![
        AgentEvent::SessionStarted {
            harness: HarnessId::Mock,
            model: "mock-1".into(),
            tools: vec![],
            cwd: "/tmp".into(),
            session_id: "hs-1".into(),
            assistant_message_id: "a-1".into(),
        },
        AgentEvent::TextDelta { text: "Hel".into() },
        AgentEvent::TextDelta { text: "lo".into() },
        AgentEvent::ToolCall {
            id: "tool-1".into(),
            call: ToolCall::WriteFile {
                path: "/tmp/x".into(),
                content: Some("SECRET".into()),
            },
        },
        AgentEvent::ToolResult {
            id: "tool-1".into(),
            is_error: false,
            output: None,
            diff: None,
            execution: None,
        },
        done(DoneStatus::Completed),
    ]
}

/// Scripted harness with a per-event delay; optionally hangs after the script until its
/// interrupt token cancels, then ends with `Done{interrupted}`.
struct ScriptedHarness {
    script: Vec<AgentEvent>,
    step_delay: Duration,
    hang_until_interrupt: bool,
}

#[async_trait]
impl Harness for ScriptedHarness {
    fn id(&self) -> HarnessId {
        HarnessId::Mock
    }
    fn display_name(&self) -> &str {
        "Scripted"
    }
    fn supports_steering(&self) -> bool {
        true
    }
    fn steering_mode(&self) -> SteeringMode {
        SteeringMode::StepBoundary
    }
    fn reasoning_levels(&self) -> &[ReasoningLevel] {
        &[ReasoningLevel::Medium]
    }
    async fn models(&self) -> Result<Vec<Model>, HarnessError> {
        Ok(vec![])
    }
    async fn run(
        &self,
        _request: RunRequest,
        controls: RunControls,
    ) -> Result<BoxStream<'static, Result<AgentEvent, HarnessError>>, HarnessError> {
        let (tx, rx) = tokio::sync::mpsc::channel::<Result<AgentEvent, HarnessError>>(16);
        let script = self.script.clone();
        let delay = self.step_delay;
        let hang = self.hang_until_interrupt;
        let token = controls.interrupt.clone();
        tokio::spawn(async move {
            for event in script {
                if tx.send(Ok(event)).await.is_err() {
                    return;
                }
                tokio::time::sleep(delay).await;
            }
            if hang {
                token.cancelled().await;
                let _ = tx.send(Ok(done(DoneStatus::Interrupted))).await;
            }
        });
        Ok(futures::stream::unfold(rx, |mut rx| async move {
            rx.recv().await.map(|event| (event, rx))
        })
        .boxed())
    }
}

#[derive(Clone, Copy)]
enum LiveFixtureMode {
    Stable,
    Conflict,
    ExitAfterDelegation,
    Passive,
    ClosedControls,
}

struct LiveDelegationHarness {
    mode: LiveFixtureMode,
    controls: Arc<tokio::sync::Mutex<Vec<LiveVoiceControl>>>,
    order: Arc<tokio::sync::Mutex<Vec<&'static str>>>,
}

impl LiveDelegationHarness {
    fn new(mode: LiveFixtureMode) -> Self {
        Self {
            mode,
            controls: Arc::new(tokio::sync::Mutex::new(Vec::new())),
            order: Arc::new(tokio::sync::Mutex::new(Vec::new())),
        }
    }
}

#[async_trait]
impl Harness for LiveDelegationHarness {
    fn id(&self) -> HarnessId {
        HarnessId::Omp
    }

    fn display_name(&self) -> &str {
        "Live delegation fixture"
    }

    fn supports_steering(&self) -> bool {
        true
    }

    fn steering_mode(&self) -> SteeringMode {
        SteeringMode::StepBoundary
    }

    fn reasoning_levels(&self) -> &[ReasoningLevel] {
        &[ReasoningLevel::Medium]
    }

    async fn probe_live_voice(&self, _cwd: &std::path::Path) -> Result<bool, HarnessError> {
        Ok(true)
    }

    async fn start_live_voice(
        &self,
        _request: LiveVoiceRequest,
    ) -> Result<LiveVoiceHandle, HarnessError> {
        let (event_tx, event_rx) =
            tokio::sync::mpsc::channel::<Result<LiveVoiceEvent, HarnessError>>(16);
        let (control_tx, mut control_rx) = tokio::sync::mpsc::channel(16);
        let controls = Arc::clone(&self.controls);
        let order = Arc::clone(&self.order);
        let mode = self.mode;
        tokio::spawn(async move {
            let _ = event_tx
                .send(Ok(LiveVoiceEvent::Phase(LiveVoicePhase::Listening)))
                .await;
            if matches!(mode, LiveFixtureMode::ClosedControls) {
                drop(control_rx);
                std::future::pending::<()>().await;
                return;
            }
            if !matches!(mode, LiveFixtureMode::Passive) {
                let delegation = LiveVoiceEvent::Delegation {
                    delegation_id: "delegation-1".into(),
                    request: "Fix the durable bug".into(),
                };
                let _ = event_tx.send(Ok(delegation.clone())).await;
                let _ = event_tx.send(Ok(delegation)).await;
                if matches!(mode, LiveFixtureMode::Conflict) {
                    let _ = event_tx
                        .send(Ok(LiveVoiceEvent::Delegation {
                            delegation_id: "delegation-2".into(),
                            request: "Start conflicting work".into(),
                        }))
                        .await;
                }
            }
            if matches!(mode, LiveFixtureMode::ExitAfterDelegation) {
                return;
            }
            while let Some(control) = control_rx.recv().await {
                if control == LiveVoiceControl::Stop {
                    order.lock().await.push("stop");
                    let _ = event_tx
                        .send(Ok(LiveVoiceEvent::Ended { error: None }))
                        .await;
                }
                controls.lock().await.push(control);
            }
        });
        Ok(LiveVoiceHandle {
            session_id: "/tmp/live-fixture.jsonl".into(),
            events: futures::stream::unfold(event_rx, |mut rx| async move {
                rx.recv().await.map(|event| (event, rx))
            })
            .boxed(),
            controls: control_tx,
        })
    }

    async fn models(&self) -> Result<Vec<Model>, HarnessError> {
        Ok(Vec::new())
    }

    async fn run(
        &self,
        request: RunRequest,
        _controls: RunControls,
    ) -> Result<BoxStream<'static, Result<AgentEvent, HarnessError>>, HarnessError> {
        self.order.lock().await.push("run");
        let expected_prompt = if matches!(
            self.mode,
            LiveFixtureMode::Passive | LiveFixtureMode::ClosedControls
        ) {
            "manual command"
        } else {
            "Fix the durable bug"
        };
        assert_eq!(request.prompt, expected_prompt);
        let script = vec![
            AgentEvent::SessionStarted {
                harness: HarnessId::Omp,
                model: "omp-default".into(),
                tools: Vec::new(),
                cwd: request.cwd,
                session_id: "voice-session".into(),
                assistant_message_id: "voice-assistant".into(),
            },
            AgentEvent::TextDelta {
                text: "Inspecting".into(),
            },
            AgentEvent::AssistantMessageCompleted {
                assistant_message_id: "voice-assistant".into(),
            },
            AgentEvent::TextDelta {
                text: "Durable answer".into(),
            },
            AgentEvent::Done {
                status: DoneStatus::Completed,
                result: Some("Fixed final".into()),
                error: None,
                session_id: Some("voice-session".into()),
            },
        ];
        let mut script = script.into_iter();
        let first = script.next().expect("non-empty Live Voice script");
        let stream = futures::stream::once(async move {
            tokio::time::sleep(Duration::from_millis(50)).await;
            Ok(first)
        })
        .chain(futures::stream::iter(script.map(Ok)));
        Ok(stream.boxed())
    }
}

fn omp_chat_config() -> ChatConfig {
    ChatConfig {
        harness: HarnessId::Omp,
        model: Some("omp-default".into()),
        reasoning: Some(ReasoningLevel::Medium),
        model_options: Default::default(),
        sandbox: SandboxLevel::WorkspaceWrite,
    }
}

fn registry_with(harness: Arc<dyn Harness>) -> Arc<HarnessRegistry> {
    let registry = HarnessRegistry::new();
    registry.register(harness);
    Arc::new(registry)
}

fn assemble(dir: &std::path::Path, harness: Arc<dyn Harness>) -> EngineCore {
    EngineCore::assemble(dir, registry_with(harness), HarnessId::Mock, None)
        .expect("engine core assembles")
}

fn assemble_live(dir: &std::path::Path, harness: Arc<dyn Harness>) -> EngineCore {
    EngineCore::assemble(dir, registry_with(harness), HarnessId::Omp, None)
        .expect("live engine core assembles")
}

fn create_omp_chat(core: &EngineCore) {
    core.workspace
        .create_chat(
            CHAT,
            None,
            Some(&core.device_id),
            Some(omp_chat_config()),
            Some("/tmp".into()),
        )
        .expect("create OMP chat");
    core.workspace
        .rename_chat(CHAT, "Pre-titled")
        .expect("pre-title OMP chat");
}

/// Queue a command into the chat doc the way a REMOTE viewer device would: an immutable
/// pending entry appended under the viewer's device id (ledger rule 1).
fn queue_as_viewer(doc: &SessionDoc, id: &str, payload: SessionCommandPayload) {
    let now = chrono::Utc::now().timestamp_millis();
    let based_on =
        doc.read_entries()
            .expect("read entries")
            .last()
            .map(|m| zeron_doc::CommandBasedOn {
                turn_id: Some(m.id.clone()),
                frontier: None,
            });
    doc.queue_command(&SessionCommandEntry {
        id: id.into(),
        payload,
        issued_by: VIEWER.into(),
        issued_at: now,
        based_on,
        expires_at: None,
        status: SessionCommandStatus::Pending,
        resolution: None,
    })
    .expect("queue command");
}

async fn wait_for<F>(mut predicate: F, what: &str)
where
    F: FnMut() -> bool,
{
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    while !predicate() {
        assert!(
            tokio::time::Instant::now() < deadline,
            "timed out waiting for {what}"
        );
        tokio::time::sleep(Duration::from_millis(15)).await;
    }
}

fn entries(core: &EngineCore) -> Vec<SessionMessageEntry> {
    core.doc_host
        .open(CHAT)
        .expect("open chat")
        .doc()
        .read_entries()
        .expect("read entries")
}

/// Tolerant read for hot-polling predicates: a snapshot taken between a
/// segment writer's `push_container` and its field writes deserializes with
/// fields missing — treat that instant as "not yet" instead of panicking.
fn entries_now(core: &EngineCore) -> Vec<SessionMessageEntry> {
    core.doc_host
        .open(CHAT)
        .ok()
        .and_then(|h| h.doc().read_entries().ok())
        .unwrap_or_default()
}

fn command_status(core: &EngineCore, id: &str) -> Option<(SessionCommandStatus, Option<String>)> {
    core.doc_host
        .open(CHAT)
        .expect("open chat")
        .doc()
        .read_commands()
        .expect("read commands")
        .into_iter()
        .find(|c| c.id == id)
        .map(|c| (c.status, c.resolution))
}

#[tokio::test]
async fn queued_run_command_executes_end_to_end() {
    let dir = tempfile::tempdir().unwrap();
    let core = assemble(
        dir.path(),
        Arc::new(MockHarness {
            script: mock_script(),
        }),
    );
    let handle = core.doc_host.open(CHAT).unwrap();

    // Live event subscription (journal replay + broadcast) before anything runs.
    let (replayed, mut live) = core.sessions.subscribe(CHAT, 0).unwrap();
    assert!(replayed.is_empty());

    // A viewer device queues the run command into the doc.
    queue_as_viewer(
        handle.doc(),
        "cmd-run-1",
        SessionCommandPayload::Run {
            request: run_request("do the thing"),
            message_id: "msg-user-1".into(),
        },
    );

    // The host executor picks it up, runs the harness, and the doc settles.
    wait_for(
        || {
            entries(&core).iter().any(|e| {
                e.role == MessageRole::Assistant && e.status == Some(MessageStatus::Complete)
            })
        },
        "assistant entry to complete",
    )
    .await;

    let all = entries(&core);
    assert_eq!(all.len(), 2, "user + assistant entries, got {all:#?}");
    // User entry carries the command's client-minted message id.
    assert_eq!(all[0].id, "msg-user-1");
    assert_eq!(all[0].role, MessageRole::User);
    assert_eq!(
        all[0].parts,
        vec![MessagePart::Text {
            id: "t0".into(),
            text: "do the thing".into()
        }]
    );
    // Assistant entry: folded parts — merged text, then the resolved tool call with the
    // render-parts privacy policy applied (WriteFile content stripped).
    let assistant = &all[1];
    assert_eq!(assistant.status, Some(MessageStatus::Complete));
    assert_eq!(assistant.parts.len(), 2);
    match &assistant.parts[0] {
        MessagePart::Text { text, .. } => assert_eq!(text, "Hello"),
        other => panic!("unexpected first part {other:?}"),
    }
    match &assistant.parts[1] {
        MessagePart::Tool {
            call,
            resolved,
            is_error,
            ..
        } => {
            assert!(*resolved);
            assert!(!*is_error);
            assert_eq!(
                call,
                &ToolCall::WriteFile {
                    path: "/tmp/x".into(),
                    content: None
                }
            );
        }
        other => panic!("unexpected second part {other:?}"),
    }

    // Command outcome written by the host (sole outcome writer).
    assert_eq!(
        command_status(&core, "cmd-run-1"),
        Some((SessionCommandStatus::Applied, None))
    );

    // Journal replay: the full script in order, terminal Done last.
    let replay = core.sessions.subscribe(CHAT, 0).unwrap().0;
    assert_eq!(replay.len(), mock_script().len());
    assert!(matches!(
        replay.last().map(|j| &j.event),
        Some(AgentEvent::Done {
            status: DoneStatus::Completed,
            ..
        })
    ));
    let seqs: Vec<u64> = replay.iter().map(|j| j.seq).collect();
    assert_eq!(seqs, (1..=mock_script().len() as u64).collect::<Vec<_>>());

    // The live broadcast delivered the same events.
    let mut broadcast_count = 0usize;
    while let Ok(event) = live.try_recv() {
        assert!(event.seq >= 1);
        broadcast_count += 1;
    }
    assert_eq!(broadcast_count, mock_script().len());

    // Final session status: Idle.
    assert_eq!(
        core.sessions.session_status(CHAT).map(|s| s.status),
        Some(SessionStatus::Idle)
    );
}

#[tokio::test]
async fn session_status_transitions_idle_working_idle() {
    let dir = tempfile::tempdir().unwrap();
    let core = assemble(
        dir.path(),
        Arc::new(ScriptedHarness {
            script: mock_script(),
            step_delay: Duration::from_millis(40),
            hang_until_interrupt: false,
        }),
    );
    let mut watch = core.sessions.watch_sessions();
    assert!(watch.borrow().is_empty(), "no sessions before dispatch");

    let handle = core.doc_host.open(CHAT).unwrap();
    queue_as_viewer(
        handle.doc(),
        "cmd-run-status",
        SessionCommandPayload::Run {
            request: run_request("go"),
            message_id: "m-1".into(),
        },
    );

    let mut seen = Vec::new();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    loop {
        let status = tokio::time::timeout_at(deadline, watch.changed())
            .await
            .expect("status change before timeout")
            .map(|_| watch.borrow().first().map(|s| s.status))
            .expect("watch alive");
        if let Some(status) = status {
            if seen.last() != Some(&status) {
                seen.push(status);
            }
            if status == SessionStatus::Idle {
                break;
            }
        }
    }
    assert_eq!(seen, vec![SessionStatus::Working, SessionStatus::Idle]);
}

#[tokio::test]
async fn interrupt_stamps_streaming_entry_aborted() {
    let dir = tempfile::tempdir().unwrap();
    let core = assemble(
        dir.path(),
        Arc::new(ScriptedHarness {
            script: vec![AgentEvent::TextDelta {
                text: "partial output".into(),
            }],
            step_delay: Duration::from_millis(5),
            hang_until_interrupt: true,
        }),
    );
    let handle = core.doc_host.open(CHAT).unwrap();
    queue_as_viewer(
        handle.doc(),
        "cmd-run-hang",
        SessionCommandPayload::Run {
            request: run_request("hang"),
            message_id: "m-1".into(),
        },
    );

    // Wait until the streaming entry is visibly in the doc, then interrupt via a
    // viewer-queued durable command (based_on = the streaming entry = current turn).
    wait_for(
        || {
            entries(&core)
                .iter()
                .any(|e| e.status == Some(MessageStatus::Streaming))
        },
        "streaming entry",
    )
    .await;
    queue_as_viewer(
        handle.doc(),
        "cmd-int-1",
        SessionCommandPayload::Interrupt {},
    );

    wait_for(
        || {
            entries(&core)
                .iter()
                .any(|e| e.status == Some(MessageStatus::Aborted))
        },
        "aborted stamp",
    )
    .await;

    let all = entries(&core);
    let assistant = all
        .iter()
        .find(|e| e.role == MessageRole::Assistant)
        .unwrap();
    assert_eq!(assistant.status, Some(MessageStatus::Aborted));
    assert!(assistant.duration_ms.is_some_and(|duration| duration > 0));
    match &assistant.parts[0] {
        MessagePart::Text { text, .. } => assert_eq!(text, "partial output"),
        other => panic!("unexpected part {other:?}"),
    }
    assert_eq!(
        command_status(&core, "cmd-int-1"),
        Some((SessionCommandStatus::Applied, None))
    );
    // Journal closed with a Done — nothing left to recover.
    let journal = RunJournal::open(dir.path().join("orgs/dev-org/dev-user/journals")).unwrap();
    assert!(journal.stale_sessions().unwrap().is_empty());
    assert_eq!(
        core.sessions.session_status(CHAT).map(|s| s.status),
        Some(SessionStatus::Idle)
    );
}

#[tokio::test]
async fn errored_done_persists_assistant_duration() {
    let dir = tempfile::tempdir().unwrap();
    let core = assemble(
        dir.path(),
        Arc::new(ScriptedHarness {
            script: vec![
                AgentEvent::TextDelta {
                    text: "partial before error".into(),
                },
                done(DoneStatus::Errored),
            ],
            step_delay: Duration::from_millis(10),
            hang_until_interrupt: false,
        }),
    );
    let handle = core.doc_host.open(CHAT).unwrap();
    queue_as_viewer(
        handle.doc(),
        "cmd-run-error-duration",
        SessionCommandPayload::Run {
            request: run_request("fail after output"),
            message_id: "m-error-duration".into(),
        },
    );

    wait_for(
        || {
            entries(&core).iter().any(|entry| {
                entry.role == MessageRole::Assistant
                    && entry.status == Some(MessageStatus::Complete)
            })
        },
        "errored assistant entry",
    )
    .await;

    let assistant = entries(&core)
        .into_iter()
        .find(|entry| entry.role == MessageRole::Assistant)
        .expect("assistant entry");
    assert_eq!(assistant.status, Some(MessageStatus::Complete));
    assert!(assistant.duration_ms.is_some_and(|duration| duration > 0));
    assert_eq!(
        core.sessions
            .session_status(CHAT)
            .map(|session| session.status),
        Some(SessionStatus::Errored)
    );
}

#[tokio::test]
async fn steer_with_no_live_run_falls_back_to_new_turn() {
    let dir = tempfile::tempdir().unwrap();
    let core = assemble(
        dir.path(),
        Arc::new(MockHarness {
            script: mock_script(),
        }),
    );
    let handle = core.doc_host.open(CHAT).unwrap();

    queue_as_viewer(
        handle.doc(),
        "cmd-run-1",
        SessionCommandPayload::Run {
            request: run_request("first"),
            message_id: "m-1".into(),
        },
    );
    wait_for(
        || {
            matches!(
                command_status(&core, "cmd-run-1"),
                Some((SessionCommandStatus::Applied, _))
            )
        },
        "first run applied",
    )
    .await;
    wait_for(
        || core.sessions.session_status(CHAT).map(|s| s.status) == Some(SessionStatus::Idle),
        "first run settled",
    )
    .await;

    // No live run anymore (mock finishes instantly): a steer command must fall back to
    // dispatch-as-next-turn, per zeron's executor.
    queue_as_viewer(
        handle.doc(),
        "cmd-steer-1",
        SessionCommandPayload::Steer {
            prompt: "also do this".into(),
            message_id: Some("m-2".into()),
        },
    );
    wait_for(
        || {
            matches!(
                command_status(&core, "cmd-steer-1"),
                Some((SessionCommandStatus::Applied, Some(_)))
            )
        },
        "steer fallback applied",
    )
    .await;
    let (status, resolution) = command_status(&core, "cmd-steer-1").unwrap();
    assert_eq!(status, SessionCommandStatus::Applied);
    assert_eq!(resolution.as_deref(), Some("queued as new turn"));

    wait_for(
        || {
            entries(&core)
                .iter()
                .filter(|e| {
                    e.role == MessageRole::Assistant && e.status == Some(MessageStatus::Complete)
                })
                .count()
                == 2
        },
        "second assistant entry",
    )
    .await;
    // The steer prompt became a user entry with its client-minted id.
    assert!(
        entries(&core)
            .iter()
            .any(|e| e.id == "m-2" && e.role == MessageRole::User)
    );
}

#[tokio::test]
async fn processed_commands_are_skipped_on_redelivery() {
    let dir = tempfile::tempdir().unwrap();

    // Simulate a crash AFTER mark-processed but BEFORE execute/outcome: the ledger has
    // the id, the doc still says pending.
    {
        let store = DocsStore::open(dir.path().join("orgs/dev-org/dev-user")).unwrap();
        assert!(store.mark_processed("cmd-crashed").unwrap());
    }

    let core = assemble(
        dir.path(),
        Arc::new(MockHarness {
            script: mock_script(),
        }),
    );
    let handle = core.doc_host.open(CHAT).unwrap();
    queue_as_viewer(
        handle.doc(),
        "cmd-crashed",
        SessionCommandPayload::Run {
            request: run_request("never again"),
            message_id: "m-x".into(),
        },
    );

    // Give the drain a moment: the command must not EXECUTE — no user entry,
    // no run. But it must not stay a forever-Pending ghost either (v0.2.12
    // swallowed-send: "Sending…" forever, retry a no-op): the dead-command
    // sweep terminalizes it as Rejected so the doc tells the truth and a
    // retry can mint a fresh attempt.
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert!(
        entries(&core).is_empty(),
        "skipped command must not execute"
    );
    wait_for(
        || {
            matches!(
                command_status(&core, "cmd-crashed"),
                Some((SessionCommandStatus::Rejected, _))
            )
        },
        "crash-window command terminalized as Rejected",
    )
    .await;
    assert!(core.sessions.session_status(CHAT).is_none());

    // Direct ledger-evaluation check: re-evaluating a processed command = Skip.
    let store = DocsStore::open(dir.path().join("orgs/dev-org/dev-user")).unwrap();
    let commands = handle.doc().read_commands().unwrap();
    let entry = commands.iter().find(|c| c.id == "cmd-crashed").unwrap();
    let is_processed = |id: &str| store.is_processed(id).unwrap_or(false);
    let never_past = |_: &str| false;
    let verdict = zeron_doc::evaluate_command(
        entry,
        &zeron_doc::EvaluationContext {
            is_processed: &is_processed,
            now_ms: chrono::Utc::now().timestamp_millis(),
            entries: &commands,
            current_turn_id: None,
            turn_is_past: &never_past,
        },
    );
    assert_eq!(verdict, zeron_doc::CommandDisposition::Skip);
}

/// The v0.2.12 field report: a send whose command was consumed by the ledger
/// but never executed (crash between mark and resolve) was invisible to
/// every retry — the drain filters processed ids, so the session was dead
/// forever while new sessions worked. Retry must mint a FRESH attempt.
#[tokio::test]
async fn retry_reissues_a_swallowed_send() {
    let dir = tempfile::tempdir().unwrap();
    {
        let store = DocsStore::open(dir.path().join("orgs/dev-org/dev-user")).unwrap();
        assert!(store.mark_processed("cmd-dead").unwrap());
    }
    let core = assemble(
        dir.path(),
        Arc::new(MockHarness {
            script: mock_script(),
        }),
    );
    let handle = core.doc_host.open(CHAT).unwrap();
    queue_as_viewer(
        handle.doc(),
        "cmd-dead",
        SessionCommandPayload::Run {
            request: run_request("try again"),
            message_id: "m-retry".into(),
        },
    );
    // The sweep terminalizes the dead attempt without executing it…
    wait_for(
        || {
            matches!(
                command_status(&core, "cmd-dead"),
                Some((SessionCommandStatus::Rejected, _))
            )
        },
        "dead attempt rejected",
    )
    .await;
    assert!(entries(&core).is_empty(), "dead attempt must not execute");

    // …and the user's retry mints a fresh attempt that actually runs.
    core.doc_host.retry_delivery(CHAT).unwrap();
    wait_for(
        || {
            entries_now(&core)
                .iter()
                .any(|e| e.id == "m-retry" && e.role == MessageRole::User)
        },
        "re-issued send writes the user entry",
    )
    .await;
    wait_for(
        || {
            entries_now(&core).iter().any(|e| {
                e.role == MessageRole::Assistant && e.status == Some(MessageStatus::Complete)
            })
        },
        "re-issued send runs to completion",
    )
    .await;
    let run_attempts = |cmds: &[SessionCommandEntry]| {
        cmds.iter()
            .filter(|c| {
                matches!(&c.payload,
                    SessionCommandPayload::Run { message_id, .. } if message_id == "m-retry")
            })
            .count()
    };
    assert_eq!(
        run_attempts(&handle.doc().read_commands().unwrap()),
        2,
        "original + exactly one re-issue"
    );
    // A delivered message must never re-issue: retry while healthy is a no-op.
    core.doc_host.retry_delivery(CHAT).unwrap();
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert_eq!(
        run_attempts(&handle.doc().read_commands().unwrap()),
        2,
        "retry after delivery must not duplicate the send"
    );
}

#[tokio::test]
async fn deterministic_queue_command_id_is_returned_and_executes_once() {
    let dir = tempfile::tempdir().unwrap();
    let core = assemble(
        dir.path(),
        Arc::new(MockHarness {
            script: mock_script(),
        }),
    );
    core.workspace
        .create_chat(CHAT, None, Some(&core.device_id), None, Some("/tmp".into()))
        .unwrap();
    let client = zeron_rpc::memory_client(core.rpc_service());
    let command = serde_json::to_value(SessionCommandPayload::Steer {
        prompt: "worker finished".into(),
        message_id: Some("worker-notify-message:worker-1:7:completed".into()),
    })
    .unwrap();
    let params = serde_json::json!({
        "chatId": CHAT,
        "commandId": "worker-notify:worker-1:7:completed",
        "command": command
    });

    let first = client
        .call(
            zeron_rpc::methods::QUEUE_WORKER_NOTIFICATION,
            params.clone(),
        )
        .await
        .unwrap();
    let store = DocsStore::open(dir.path().join("orgs/dev-org/dev-user")).unwrap();
    let bytes = store
        .load_snapshot(CHAT)
        .unwrap()
        .expect("notification command is durable before RPC success");
    let restored = LoroDoc::new();
    restored.import(&bytes).unwrap();
    assert!(
        SessionDoc::from_doc(restored)
            .read_commands()
            .unwrap()
            .iter()
            .any(|entry| entry.id == "worker-notify:worker-1:7:completed")
    );
    let second = client
        .call(zeron_rpc::methods::QUEUE_WORKER_NOTIFICATION, params)
        .await
        .unwrap();
    assert_eq!(first["commandId"], "worker-notify:worker-1:7:completed");
    assert_eq!(second["commandId"], "worker-notify:worker-1:7:completed");
    // A retry under the same id must REUSE the queued entry: a second doc
    // entry would sit Pending forever (the ledger blocks its execution) and
    // the dead-command sweep would re-report it on every drain.
    assert_eq!(
        core.doc_host
            .open(CHAT)
            .unwrap()
            .doc()
            .read_commands()
            .unwrap()
            .iter()
            .filter(|entry| entry.id == "worker-notify:worker-1:7:completed")
            .count(),
        1,
        "deterministic id queues exactly one command entry"
    );

    wait_for(
        || {
            entries_now(&core)
                .iter()
                .filter(|entry| entry.role == MessageRole::Assistant)
                .count()
                == 1
        },
        "one deterministic command execution",
    )
    .await;
    tokio::time::sleep(Duration::from_millis(250)).await;
    assert_eq!(
        entries(&core)
            .iter()
            .filter(|entry| entry.role == MessageRole::Assistant)
            .count(),
        1
    );
}
#[tokio::test]
async fn live_voice_delegation_is_one_durable_run_with_transient_context() {
    let dir = tempfile::tempdir().unwrap();
    let harness = Arc::new(LiveDelegationHarness::new(LiveFixtureMode::Stable));
    let core = assemble_live(dir.path(), harness.clone());
    create_omp_chat(&core);

    core.sessions.start_live_voice(CHAT).await.unwrap();
    wait_for(
        || {
            let handle = core.doc_host.open(CHAT).expect("open live chat");
            let run_commands = handle
                .doc()
                .read_commands()
                .unwrap_or_default()
                .into_iter()
                .filter(|entry| matches!(entry.payload, SessionCommandPayload::Run { .. }))
                .count();
            let durable_messages = entries_now(&core)
                .into_iter()
                .filter(|entry| matches!(entry.role, MessageRole::User | MessageRole::Assistant))
                .count();
            run_commands == 1 && durable_messages == 2
        },
        "one durable Live Voice delegation",
    )
    .await;

    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    loop {
        let controls = harness.controls.lock().await.clone();
        let has_progress = controls.iter().any(|control| {
            matches!(
                control,
                LiveVoiceControl::AppendContext {
                    delegation_id,
                    kind: LiveVoiceContextKind::Progress,
                    text,
                } if delegation_id == "delegation-1" && text == "Inspecting"
            )
        });
        let has_final = controls.iter().any(|control| {
            matches!(
                control,
                LiveVoiceControl::AppendContext {
                    delegation_id,
                    kind: LiveVoiceContextKind::Final,
                    text,
                } if delegation_id == "delegation-1" && text == "Fixed final"
            )
        });
        if has_progress && has_final {
            assert!(
                !controls.contains(&LiveVoiceControl::Stop),
                "the owned durable command must not preempt its Live Voice call"
            );
            break;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "timed out waiting for Live Voice progress and final context"
        );
        tokio::time::sleep(Duration::from_millis(15)).await;
    }

    let handle = core.doc_host.open(CHAT).unwrap();
    let commands = handle.doc().read_commands().unwrap();
    let runs = commands
        .iter()
        .filter(|entry| matches!(entry.payload, SessionCommandPayload::Run { .. }))
        .collect::<Vec<_>>();
    assert_eq!(runs.len(), 1, "duplicate delegation id must be idempotent");
    assert!(!runs[0].id.is_empty());
    let SessionCommandPayload::Run { message_id, .. } = &runs[0].payload else {
        unreachable!()
    };
    assert!(!message_id.is_empty());
    assert_eq!(
        entries(&core)
            .iter()
            .filter(|entry| matches!(entry.role, MessageRole::User | MessageRole::Assistant))
            .count(),
        2,
        "spoken progress/final controls must not append extra chat messages"
    );
    core.sessions.stop_live_voice().await.unwrap();
}

#[tokio::test]
async fn live_voice_conflict_ends_call_but_backend_finishes_durably() {
    let dir = tempfile::tempdir().unwrap();
    let harness = Arc::new(LiveDelegationHarness::new(LiveFixtureMode::Conflict));
    let core = assemble_live(dir.path(), harness);
    create_omp_chat(&core);

    let mut state = core.sessions.watch_live_voice();
    core.sessions.start_live_voice(CHAT).await.unwrap();
    wait_for(
        || {
            entries_now(&core)
                .iter()
                .any(|entry| entry.role == MessageRole::Assistant)
        },
        "durable backend completion after Live Voice conflict",
    )
    .await;
    tokio::time::timeout(Duration::from_secs(10), async {
        while state.borrow().phase != LiveVoicePhase::Error {
            state.changed().await.unwrap();
        }
    })
    .await
    .expect("conflicting delegation ends Live Voice");
    assert!(
        state
            .borrow()
            .error
            .as_deref()
            .is_some_and(|error| error.contains("delegation")),
        "conflict reports a bounded delegation error"
    );
    assert_eq!(
        core.doc_host
            .open(CHAT)
            .unwrap()
            .doc()
            .read_commands()
            .unwrap()
            .iter()
            .filter(|entry| matches!(entry.payload, SessionCommandPayload::Run { .. }))
            .count(),
        1
    );
}

#[tokio::test]
async fn live_voice_child_exit_does_not_cancel_queued_backend_work() {
    let dir = tempfile::tempdir().unwrap();
    let harness = Arc::new(LiveDelegationHarness::new(
        LiveFixtureMode::ExitAfterDelegation,
    ));
    let core = assemble_live(dir.path(), harness);
    create_omp_chat(&core);

    core.sessions.start_live_voice(CHAT).await.unwrap();
    wait_for(
        || {
            entries_now(&core)
                .iter()
                .any(|entry| entry.role == MessageRole::Assistant)
        },
        "backend completion after Live Voice child exit",
    )
    .await;
    assert_eq!(
        core.sessions.watch_live_voice().borrow().phase,
        LiveVoicePhase::Error
    );
}

#[tokio::test]
async fn live_voice_closed_control_does_not_reject_unrelated_command() {
    let dir = tempfile::tempdir().unwrap();
    let harness = Arc::new(LiveDelegationHarness::new(LiveFixtureMode::ClosedControls));
    let core = assemble_live(dir.path(), harness);
    create_omp_chat(&core);
    core.sessions.start_live_voice(CHAT).await.unwrap();
    let command_id = "manual-after-closed-live";

    let mut request = run_request("manual command");
    request.harness = Some(HarnessId::Omp);
    core.doc_host
        .queue_command_with_id(
            CHAT,
            command_id.into(),
            SessionCommandPayload::Run {
                request,
                message_id: "manual-after-closed-live-message".into(),
            },
        )
        .unwrap();
    wait_for(
        || {
            command_status(&core, command_id)
                .is_some_and(|(status, _)| status != SessionCommandStatus::Pending)
        },
        "command resolution after closed Live control",
    )
    .await;

    assert_eq!(
        command_status(&core, command_id).unwrap().0,
        SessionCommandStatus::Applied
    );
    wait_for(
        || {
            entries_now(&core)
                .iter()
                .any(|entry| entry.role == MessageRole::Assistant)
        },
        "unrelated command execution after closed Live control",
    )
    .await;
}

#[tokio::test]
async fn unrelated_durable_command_stops_live_voice_before_dispatch() {
    let dir = tempfile::tempdir().unwrap();
    let harness = Arc::new(LiveDelegationHarness::new(LiveFixtureMode::Passive));
    let core = assemble_live(dir.path(), harness.clone());
    create_omp_chat(&core);
    core.sessions.start_live_voice(CHAT).await.unwrap();

    let mut request = run_request("manual command");
    request.harness = Some(HarnessId::Omp);
    core.doc_host
        .queue_command_with_id(
            CHAT,
            "manual-command".into(),
            SessionCommandPayload::Run {
                request,
                message_id: "manual-message".into(),
            },
        )
        .unwrap();
    wait_for(
        || {
            entries_now(&core)
                .iter()
                .any(|entry| entry.role == MessageRole::Assistant)
        },
        "manual command after Live Voice preemption",
    )
    .await;
    assert_eq!(
        harness.order.lock().await.as_slice(),
        ["stop", "run"],
        "Live Voice must stop before unrelated durable dispatch"
    );
}

#[tokio::test]
async fn live_voice_rpc_controls_are_local_exact_and_watchable() {
    let dir = tempfile::tempdir().unwrap();
    let harness = Arc::new(LiveDelegationHarness::new(LiveFixtureMode::Passive));
    let core = assemble_live(dir.path(), harness.clone());
    create_omp_chat(&core);
    core.workspace
        .create_chat(
            "remote-live-chat",
            None,
            Some("remote-device"),
            Some(omp_chat_config()),
            Some("/tmp".into()),
        )
        .unwrap();
    let client = zeron_rpc::memory_client(core.rpc_service());
    let mut states = client
        .subscribe_checked(
            zeron_rpc::methods::WATCH_LIVE_VOICE,
            serde_json::Value::Null,
        )
        .await
        .expect("watch Live Voice");
    assert_eq!(
        states.recv().await.unwrap()["phase"],
        serde_json::json!("idle")
    );

    let availability = client
        .call(
            zeron_rpc::methods::PROBE_LIVE_VOICE,
            serde_json::json!({ "chatId": CHAT }),
        )
        .await
        .expect("probe local OMP Chat");
    assert_eq!(
        availability,
        serde_json::json!({ "available": true, "reason": null })
    );
    let remote_availability = client
        .call(
            zeron_rpc::methods::PROBE_LIVE_VOICE,
            serde_json::json!({ "chatId": "remote-live-chat" }),
        )
        .await
        .expect("probe reports remote Chat unavailability");
    assert_eq!(
        remote_availability,
        serde_json::json!({ "available": false, "reason": "remoteChat" })
    );
    assert!(
        client
            .call(
                zeron_rpc::methods::START_LIVE_VOICE,
                serde_json::json!({ "chatId": "remote-live-chat" }),
            )
            .await
            .is_err(),
        "local-only Live Voice start rejects remote Chats"
    );

    assert_eq!(
        client
            .call(
                zeron_rpc::methods::START_LIVE_VOICE,
                serde_json::json!({ "chatId": CHAT }),
            )
            .await
            .expect("start Live Voice"),
        serde_json::json!({ "active": true })
    );
    let listening = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let state = states.recv().await.expect("Live Voice watch update");
            if state["phase"] == "listening" {
                return state;
            }
        }
    })
    .await
    .expect("watch reaches listening");
    assert_eq!(listening["chatId"], CHAT);

    assert_eq!(
        client
            .call(
                zeron_rpc::methods::SET_LIVE_VOICE_MUTED,
                serde_json::json!({ "muted": true }),
            )
            .await
            .expect("mute Live Voice"),
        serde_json::json!({ "muted": true })
    );
    assert!(
        harness
            .controls
            .lock()
            .await
            .contains(&LiveVoiceControl::SetMuted(true))
    );
    assert_eq!(
        client
            .call(zeron_rpc::methods::STOP_LIVE_VOICE, serde_json::Value::Null,)
            .await
            .expect("stop Live Voice"),
        serde_json::json!({ "active": false })
    );
    assert_eq!(
        client
            .call(zeron_rpc::methods::STOP_LIVE_VOICE, serde_json::Value::Null,)
            .await
            .expect("repeat stop Live Voice"),
        serde_json::json!({ "active": false })
    );
    let reset = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let state = states.recv().await.expect("Live Voice reset update");
            if state["phase"] == "idle" {
                return state;
            }
        }
    })
    .await
    .expect("watch resets to idle");
    assert_eq!(
        reset,
        serde_json::to_value(zeron_proto::LiveVoiceState::default()).unwrap()
    );
}

#[tokio::test]
async fn fetch_tool_input_returns_journal_body_only_for_the_local_chat_owner() {
    let dir = tempfile::tempdir().unwrap();
    let core = assemble(
        dir.path(),
        Arc::new(MockHarness {
            script: mock_script(),
        }),
    );
    core.workspace
        .create_chat(CHAT, None, Some(&core.device_id), None, Some("/tmp".into()))
        .unwrap();
    core.sessions
        .dispatch(
            CHAT,
            HarnessId::Mock,
            run_request("write the file"),
            Some("fetch-input-user".into()),
        )
        .await
        .unwrap();
    wait_for(
        || {
            entries_now(&core).iter().any(|entry| {
                entry.role == MessageRole::Assistant
                    && entry.status == Some(MessageStatus::Complete)
            })
        },
        "write tool to settle",
    )
    .await;

    let client = zeron_rpc::memory_client(core.rpc_service());
    let reply = client
        .call(
            zeron_rpc::methods::FETCH_TOOL_INPUT,
            serde_json::json!({
                "chatId": CHAT,
                "toolCallId": "tool-1",
                "targetDeviceId": core.device_id,
            }),
        )
        .await
        .unwrap();
    assert_eq!(reply["snapshot"]["path"], "/tmp/x");
    assert_eq!(reply["snapshot"]["content"], "SECRET");
    let missing = client
        .call(
            zeron_rpc::methods::FETCH_TOOL_INPUT,
            serde_json::json!({
                "chatId": CHAT,
                "toolCallId": "missing",
                "targetDeviceId": core.device_id,
            }),
        )
        .await
        .unwrap();
    assert!(missing["snapshot"].is_null());

    core.workspace
        .create_chat(
            "remote-chat",
            None,
            Some("other-device"),
            None,
            Some("/tmp".into()),
        )
        .unwrap();
    let error = client
        .call(
            zeron_rpc::methods::FETCH_TOOL_INPUT,
            serde_json::json!({
                "chatId": "remote-chat",
                "toolCallId": "tool-1",
            }),
        )
        .await
        .unwrap_err();
    assert!(error.to_string().contains("another device"));
}

#[tokio::test]
async fn worker_notification_rejects_a_deleted_parent_without_creating_an_orphan_doc() {
    let dir = tempfile::tempdir().unwrap();
    let core = assemble(
        dir.path(),
        Arc::new(MockHarness {
            script: mock_script(),
        }),
    );
    let client = zeron_rpc::memory_client(core.rpc_service());
    let command = serde_json::to_value(SessionCommandPayload::Steer {
        prompt: "worker finished".into(),
        message_id: Some("worker-notify-message:missing".into()),
    })
    .unwrap();

    let error = client
        .call(
            zeron_rpc::methods::QUEUE_WORKER_NOTIFICATION,
            serde_json::json!({
                "chatId": "deleted-parent",
                "commandId": "worker-notify:missing",
                "command": command
            }),
        )
        .await
        .expect_err("missing parent must be rejected");
    assert!(error.to_string().contains("parent chat does not exist"));
    let store = DocsStore::open(dir.path().join("orgs/dev-org/dev-user")).unwrap();
    assert!(store.load_snapshot("deleted-parent").unwrap().is_none());
}

#[tokio::test]
async fn recover_stale_journal_stamps_aborted_on_boot() {
    let dir = tempfile::tempdir().unwrap();
    let device_id = "dev-host-fixed";
    std::fs::create_dir_all(dir.path()).unwrap();
    std::fs::write(dir.path().join("device-id"), device_id).unwrap();

    // Craft the crash state: a journal without a terminal Done + a doc snapshot whose
    // assistant entry is still `streaming`.
    {
        let journal = RunJournal::open(dir.path().join("orgs/dev-org/dev-user/journals")).unwrap();
        journal
            .append(
                CHAT,
                &AgentEvent::TextDelta {
                    text: "doomed".into(),
                },
            )
            .unwrap();

        let doc = SessionDoc::init(CHAT).unwrap();
        doc.push_message(&SessionMessageEntry {
            id: "m-user".into(),
            role: MessageRole::User,
            parts: vec![MessagePart::Text {
                id: "t0".into(),
                text: "hi".into(),
            }],
            created_at: 1,
            device_id: device_id.into(),
            status: Some(MessageStatus::Complete),
            duration_ms: None,
            continuation_of: None,
        })
        .unwrap();
        let mut writer = SegmentWriter::begin(&doc, "m-assist", device_id, 2).unwrap();
        writer
            .sync(&[MessagePart::Text {
                id: "t0".into(),
                text: "doomed".into(),
            }])
            .unwrap();
        // No finish — the "process" dies here with the entry still streaming.
        let store = DocsStore::open(dir.path().join("orgs/dev-org/dev-user")).unwrap();
        store
            .save_snapshot(CHAT, &doc.export_snapshot().unwrap())
            .unwrap();
    }

    // Boot: EngineCore::assemble runs recover_stale.
    let core = assemble(
        dir.path(),
        Arc::new(MockHarness {
            script: mock_script(),
        }),
    );
    assert_eq!(core.device_id, device_id);

    let all = entries(&core);
    let assistant = all.iter().find(|e| e.id == "m-assist").unwrap();
    assert_eq!(assistant.status, Some(MessageStatus::Aborted));
    match &assistant.parts[0] {
        MessagePart::Text { text, .. } => assert_eq!(text, "doomed"),
        other => panic!("unexpected part {other:?}"),
    }

    // Journal closed with a synthetic Done{interrupted}; no longer stale.
    let journal = RunJournal::open(dir.path().join("orgs/dev-org/dev-user/journals")).unwrap();
    assert!(journal.stale_sessions().unwrap().is_empty());
    let (_, last) = journal.last_event(CHAT).unwrap().unwrap();
    assert!(matches!(
        last,
        AgentEvent::Done {
            status: DoneStatus::Interrupted,
            ..
        }
    ));
    assert_eq!(
        core.sessions.session_status(CHAT).map(|s| s.status),
        Some(SessionStatus::Idle)
    );
}

#[tokio::test]
async fn rpc_surface_over_in_memory_transport() {
    let dir = tempfile::tempdir().unwrap();
    let core = assemble(
        dir.path(),
        Arc::new(MockHarness {
            script: mock_script(),
        }),
    );
    let client = zeron_rpc::memory_client(core.rpc_service());

    // ListHarnesses + ListModels.
    let harnesses = client
        .call(zeron_rpc::methods::LIST_HARNESSES, serde_json::Value::Null)
        .await
        .unwrap();
    assert_eq!(harnesses[0]["id"], "mock");
    let models = client
        .call(
            zeron_rpc::methods::LIST_MODELS,
            serde_json::json!({"harness": "mock"}),
        )
        .await
        .unwrap();
    assert_eq!(models[0]["id"], "mock-1");

    // WatchSessions + WatchDocMessages streams.
    let mut sessions_stream = client
        .subscribe(zeron_rpc::methods::WATCH_SESSIONS, serde_json::Value::Null)
        .await
        .unwrap();
    let first_sessions = tokio::time::timeout(Duration::from_secs(5), sessions_stream.recv())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(first_sessions, serde_json::json!([]));

    let mut messages_stream = client
        .subscribe(
            zeron_rpc::methods::WATCH_DOC_MESSAGES,
            serde_json::json!({"chatId": CHAT}),
        )
        .await
        .unwrap();
    let initial = tokio::time::timeout(Duration::from_secs(5), messages_stream.recv())
        .await
        .unwrap()
        .unwrap();
    // Delta protocol: the stream opens with a full reset frame.
    assert_eq!(initial, serde_json::json!({ "reset": [] }));

    // QueueCommand (as this device's composer would over IPC).
    let command = serde_json::to_value(SessionCommandPayload::Run {
        request: run_request("via rpc"),
        message_id: "m-rpc-1".into(),
    })
    .unwrap();
    let queued = client
        .call(
            zeron_rpc::methods::QUEUE_COMMAND,
            serde_json::json!({"chatId": CHAT, "command": command}),
        )
        .await
        .unwrap();
    assert!(queued["commandId"].is_string());

    // The doc-messages stream emits delta frames until the transcript settles:
    // user entry + completed assistant entry with the folded parts. Applying
    // each frame client-side mirrors what both viewports do.
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    let mut materialized: Vec<SessionMessageEntry> = vec![];
    let settled = loop {
        let item = tokio::time::timeout_at(deadline, messages_stream.recv())
            .await
            .expect("doc messages before timeout")
            .expect("stream alive");
        let frame: zeron_doc::TranscriptFrame = serde_json::from_value(item).unwrap();
        zeron_doc::apply_transcript_frame(&mut materialized, frame).unwrap();
        if materialized.len() == 2 && materialized[1].status == Some(MessageStatus::Complete) {
            break materialized;
        }
    };
    assert_eq!(settled[0].id, "m-rpc-1");
    assert_eq!(settled[0].role, MessageRole::User);
    match &settled[1].parts[0] {
        MessagePart::Text { text, .. } => assert_eq!(text, "Hello"),
        other => panic!("unexpected part {other:?}"),
    }

    // WatchSessions eventually reports the settled Idle session.
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    loop {
        let item = tokio::time::timeout_at(deadline, sessions_stream.recv())
            .await
            .expect("session update before timeout")
            .expect("stream alive");
        let list: Vec<serde_json::Value> = serde_json::from_value(item).unwrap();
        if list.first().and_then(|s| s["status"].as_str()) == Some("idle") {
            break;
        }
    }
}

#[tokio::test]
async fn respond_input_resolves_pending_question() {
    // Harness that asks a question through RunControls and echoes the answer.
    struct AskingHarness;
    #[async_trait]
    impl Harness for AskingHarness {
        fn id(&self) -> HarnessId {
            HarnessId::Mock
        }
        fn display_name(&self) -> &str {
            "Asking"
        }
        fn supports_steering(&self) -> bool {
            false
        }
        fn steering_mode(&self) -> SteeringMode {
            SteeringMode::TurnBoundary
        }
        fn reasoning_levels(&self) -> &[ReasoningLevel] {
            &[]
        }
        async fn models(&self) -> Result<Vec<Model>, HarnessError> {
            Ok(vec![])
        }
        async fn run(
            &self,
            _request: RunRequest,
            controls: RunControls,
        ) -> Result<BoxStream<'static, Result<AgentEvent, HarnessError>>, HarnessError> {
            let (tx, rx) = tokio::sync::mpsc::channel::<Result<AgentEvent, HarnessError>>(16);
            tokio::spawn(async move {
                let answers = (controls.request_input)(vec![zeron_proto::UserInputQuestion {
                    id: "q1".into(),
                    header: "Pick".into(),
                    question: "Which one?".into(),
                    options: vec!["a".into(), "b".into()],
                    multi_select: false,
                }])
                .await
                .unwrap_or_default();
                let picked = answers
                    .first()
                    .and_then(|a| a.labels.first().cloned())
                    .unwrap_or_else(|| "none".into());
                let _ = tx
                    .send(Ok(AgentEvent::TextDelta {
                        text: format!("picked {picked}"),
                    }))
                    .await;
                let _ = tx.send(Ok(done(DoneStatus::Completed))).await;
            });
            Ok(futures::stream::unfold(rx, |mut rx| async move {
                rx.recv().await.map(|event| (event, rx))
            })
            .boxed())
        }
    }

    let dir = tempfile::tempdir().unwrap();
    let core = assemble(dir.path(), Arc::new(AskingHarness));
    let handle = core.doc_host.open(CHAT).unwrap();
    queue_as_viewer(
        handle.doc(),
        "cmd-run-ask",
        SessionCommandPayload::Run {
            request: run_request("ask me"),
            message_id: "m-1".into(),
        },
    );

    // The input request surfaces: status AwaitingInput + an unresolved input part.
    wait_for(
        || {
            core.sessions.session_status(CHAT).map(|s| s.status)
                == Some(SessionStatus::AwaitingInput)
        },
        "awaiting input",
    )
    .await;
    wait_for(
        || {
            entries(&core).iter().any(|e| {
                e.parts.iter().any(|p| {
                    matches!(
                        p,
                        MessagePart::Input {
                            resolved: false,
                            ..
                        }
                    )
                })
            })
        },
        "input part in doc",
    )
    .await;

    // A viewer answers through the durable command queue.
    let request_id = entries(&core)
        .iter()
        .find_map(|e| {
            e.parts.iter().find_map(|p| match p {
                MessagePart::Input { request_id, .. } => Some(request_id.clone()),
                _ => None,
            })
        })
        .unwrap();
    queue_as_viewer(
        handle.doc(),
        "cmd-answer-1",
        SessionCommandPayload::RespondInput {
            request_id,
            answers: vec![zeron_proto::UserInputAnswer {
                question_id: "q1".into(),
                labels: vec!["b".into()],
            }],
        },
    );

    wait_for(
        || {
            entries_now(&core).iter().any(|e| {
                e.status == Some(MessageStatus::Complete)
                    && e.parts
                        .iter()
                        .any(|p| matches!(p, MessagePart::Text { text, .. } if text == "picked b"))
            })
        },
        "answered turn to complete",
    )
    .await;
    assert_eq!(
        command_status(&core, "cmd-answer-1"),
        Some((SessionCommandStatus::Applied, None))
    );
    // The input part is marked resolved in the doc.
    assert!(entries(&core).iter().any(|e| {
        e.parts
            .iter()
            .any(|p| matches!(p, MessagePart::Input { resolved: true, .. }))
    }));
    // The run task writes the Complete entry BEFORE settling the status row —
    // wait for the transition instead of asserting the instant in between.
    wait_for(
        || core.sessions.session_status(CHAT).map(|s| s.status) == Some(SessionStatus::Idle),
        "session to settle idle",
    )
    .await;
}

/// Resilience: a RespondInput whose id matches no pending request is REJECTED
/// with a resolution (never silently dropped), the question stays live (the
/// panel persists), and a subsequent correct answer still resumes the run —
/// a wrong answer can never brick the session.
#[tokio::test(flavor = "multi_thread")]
async fn wrong_id_respond_is_rejected_and_correct_answer_still_resumes() {
    struct AskingHarness;
    #[async_trait]
    impl Harness for AskingHarness {
        fn id(&self) -> HarnessId {
            HarnessId::Mock
        }
        fn display_name(&self) -> &str {
            "Asking"
        }
        fn supports_steering(&self) -> bool {
            false
        }
        fn steering_mode(&self) -> SteeringMode {
            SteeringMode::TurnBoundary
        }
        fn reasoning_levels(&self) -> &[ReasoningLevel] {
            &[]
        }
        async fn models(&self) -> Result<Vec<Model>, HarnessError> {
            Ok(vec![])
        }
        async fn run(
            &self,
            _request: RunRequest,
            controls: RunControls,
        ) -> Result<BoxStream<'static, Result<AgentEvent, HarnessError>>, HarnessError> {
            let (tx, rx) = tokio::sync::mpsc::channel::<Result<AgentEvent, HarnessError>>(16);
            tokio::spawn(async move {
                let answers = (controls.request_input)(vec![zeron_proto::UserInputQuestion {
                    id: "q1".into(),
                    header: "Pick".into(),
                    question: "Which one?".into(),
                    options: vec!["a".into(), "b".into()],
                    multi_select: false,
                }])
                .await
                .unwrap_or_default();
                let picked = answers
                    .first()
                    .and_then(|a| a.labels.first().cloned())
                    .unwrap_or_else(|| "none".into());
                let _ = tx
                    .send(Ok(AgentEvent::TextDelta {
                        text: format!("picked {picked}"),
                    }))
                    .await;
                let _ = tx.send(Ok(done(DoneStatus::Completed))).await;
            });
            Ok(futures::stream::unfold(rx, |mut rx| async move {
                rx.recv().await.map(|event| (event, rx))
            })
            .boxed())
        }
    }

    let dir = tempfile::tempdir().unwrap();
    let core = assemble(dir.path(), Arc::new(AskingHarness));
    let handle = core.doc_host.open(CHAT).unwrap();
    queue_as_viewer(
        handle.doc(),
        "cmd-run-wrong",
        SessionCommandPayload::Run {
            request: run_request("ask me"),
            message_id: "m-1".into(),
        },
    );
    wait_for(
        || {
            core.sessions.session_status(CHAT).map(|s| s.status)
                == Some(SessionStatus::AwaitingInput)
        },
        "awaiting input",
    )
    .await;
    wait_for(
        || {
            entries_now(&core).iter().any(|e| {
                e.parts.iter().any(|p| {
                    matches!(
                        p,
                        MessagePart::Input {
                            resolved: false,
                            ..
                        }
                    )
                })
            })
        },
        "input part in doc",
    )
    .await;

    // A wrong-id answer: rejected with a resolution, question still live.
    queue_as_viewer(
        handle.doc(),
        "cmd-answer-bogus",
        SessionCommandPayload::RespondInput {
            request_id: "bogus-id".into(),
            answers: vec![zeron_proto::UserInputAnswer {
                question_id: "q1".into(),
                labels: vec!["a".into()],
            }],
        },
    );
    wait_for(
        || {
            command_status(&core, "cmd-answer-bogus")
                .is_some_and(|(s, _)| s != SessionCommandStatus::Pending)
        },
        "bogus answer processed",
    )
    .await;
    assert_eq!(
        command_status(&core, "cmd-answer-bogus"),
        Some((
            SessionCommandStatus::Rejected,
            Some("no pending input request".into())
        ))
    );
    // The run is still waiting and the part is still unresolved — the
    // QuestionPanel keeps presenting the real request.
    assert_eq!(
        core.sessions.session_status(CHAT).map(|s| s.status),
        Some(SessionStatus::AwaitingInput)
    );
    let request_id = entries(&core)
        .iter()
        .find_map(|e| {
            e.parts.iter().find_map(|p| match p {
                MessagePart::Input {
                    request_id,
                    resolved: false,
                    ..
                } => Some(request_id.clone()),
                _ => None,
            })
        })
        .expect("question still live after rejected answer");

    // The correct answer still resumes and completes the run.
    queue_as_viewer(
        handle.doc(),
        "cmd-answer-right",
        SessionCommandPayload::RespondInput {
            request_id,
            answers: vec![zeron_proto::UserInputAnswer {
                question_id: "q1".into(),
                labels: vec!["b".into()],
            }],
        },
    );
    wait_for(
        || {
            entries_now(&core).iter().any(|e| {
                e.status == Some(MessageStatus::Complete)
                    && e.parts
                        .iter()
                        .any(|p| matches!(p, MessagePart::Text { text, .. } if text == "picked b"))
            })
        },
        "answered turn to complete",
    )
    .await;
    assert_eq!(
        core.sessions.session_status(CHAT).map(|s| s.status),
        Some(SessionStatus::Idle)
    );
}

/// Resilience: interrupting a run that is BLOCKED on a question unparks the
/// harness immediately (the pending resolver is failed with empty answers),
/// the entry settles `aborted`, the chip flips terminal (never dangles
/// unresolved), and the next run works — a blocked question can never brick
/// the session.
#[tokio::test(flavor = "multi_thread")]
async fn interrupt_unblocks_a_run_awaiting_input() {
    struct BlockingHarness;
    #[async_trait]
    impl Harness for BlockingHarness {
        fn id(&self) -> HarnessId {
            HarnessId::Mock
        }
        fn display_name(&self) -> &str {
            "Blocking"
        }
        fn supports_steering(&self) -> bool {
            false
        }
        fn steering_mode(&self) -> SteeringMode {
            SteeringMode::TurnBoundary
        }
        fn reasoning_levels(&self) -> &[ReasoningLevel] {
            &[]
        }
        async fn models(&self) -> Result<Vec<Model>, HarnessError> {
            Ok(vec![])
        }
        async fn run(
            &self,
            request: RunRequest,
            controls: RunControls,
        ) -> Result<BoxStream<'static, Result<AgentEvent, HarnessError>>, HarnessError> {
            let (tx, rx) = tokio::sync::mpsc::channel::<Result<AgentEvent, HarnessError>>(16);
            if request.prompt == "second run" {
                // The post-interrupt turn: completes immediately.
                tokio::spawn(async move {
                    let _ = tx
                        .send(Ok(AgentEvent::TextDelta {
                            text: "second done".into(),
                        }))
                        .await;
                    let _ = tx.send(Ok(done(DoneStatus::Completed))).await;
                });
            } else {
                let interrupt = controls.interrupt.clone();
                tokio::spawn(async move {
                    // Blocks on the question; an interrupt fails the resolver
                    // (empty answers) and cancels the token — like a real CLI
                    // being torn down, the stream then ends WITHOUT a Done.
                    let _ = (controls.request_input)(vec![zeron_proto::UserInputQuestion {
                        id: "q1".into(),
                        header: "Pick".into(),
                        question: "Which one?".into(),
                        options: vec!["a".into(), "b".into()],
                        multi_select: false,
                    }])
                    .await;
                    interrupt.cancelled().await;
                    drop(tx);
                });
            }
            Ok(futures::stream::unfold(rx, |mut rx| async move {
                rx.recv().await.map(|event| (event, rx))
            })
            .boxed())
        }
    }

    let dir = tempfile::tempdir().unwrap();
    let core = assemble(dir.path(), Arc::new(BlockingHarness));
    let handle = core.doc_host.open(CHAT).unwrap();
    queue_as_viewer(
        handle.doc(),
        "cmd-run-block",
        SessionCommandPayload::Run {
            request: run_request("ask and block"),
            message_id: "m-1".into(),
        },
    );
    wait_for(
        || {
            core.sessions.session_status(CHAT).map(|s| s.status)
                == Some(SessionStatus::AwaitingInput)
        },
        "awaiting input",
    )
    .await;
    wait_for(
        || {
            entries_now(&core).iter().any(|e| {
                e.parts.iter().any(|p| {
                    matches!(
                        p,
                        MessagePart::Input {
                            resolved: false,
                            ..
                        }
                    )
                })
            })
        },
        "input part in doc",
    )
    .await;

    // Interrupt while blocked: settles promptly (well under the 3s grace —
    // the unparked resolver lets the harness wind down on its own).
    let start = std::time::Instant::now();
    core.sessions.interrupt(CHAT).await.unwrap();
    assert!(
        start.elapsed() < std::time::Duration::from_secs(3),
        "interrupt settled via the unparked resolver, not the grace timeout"
    );
    wait_for(
        || {
            entries_now(&core)
                .iter()
                .any(|e| e.status == Some(MessageStatus::Aborted))
        },
        "entry stamped aborted",
    )
    .await;
    // The chip is terminal — no dangling unresolved question survives the run.
    assert!(entries(&core).iter().all(|e| {
        e.parts.iter().all(|p| {
            !matches!(
                p,
                MessagePart::Input {
                    resolved: false,
                    ..
                }
            )
        })
    }));

    // And the session is usable: the next run completes.
    queue_as_viewer(
        handle.doc(),
        "cmd-run-second",
        SessionCommandPayload::Run {
            request: run_request("second run"),
            message_id: "m-2".into(),
        },
    );
    wait_for(
        || {
            entries_now(&core).iter().any(|e| {
                e.status == Some(MessageStatus::Complete)
                    && e.parts.iter().any(
                        |p| matches!(p, MessagePart::Text { text, .. } if text == "second done"),
                    )
            })
        },
        "second run to complete",
    )
    .await;
}

/// Regression (the "nothing happened after I answered" bug): a harness that
/// emits its OWN `InputRequested` (keyed by its internal id — Claude's
/// control-request id) *and* asks through `RunControls::request_input` used to
/// fold TWO input parts into the doc. The UI answers the LAST unresolved part;
/// the harness-emitted twin's id was unknown to `respond_input`'s pending map,
/// so the RespondInput doc command was rejected and the run never resumed.
/// The engine now drops harness-emitted `InputRequested` events (the input
/// bridge is the sole authority), so exactly one — answerable — part folds.
#[tokio::test(flavor = "multi_thread")]
async fn harness_emitted_input_twin_is_dropped_and_answer_resumes() {
    struct DoubleEmitHarness;
    #[async_trait]
    impl Harness for DoubleEmitHarness {
        fn id(&self) -> HarnessId {
            HarnessId::Mock
        }
        fn display_name(&self) -> &str {
            "DoubleEmit"
        }
        fn supports_steering(&self) -> bool {
            false
        }
        fn steering_mode(&self) -> SteeringMode {
            SteeringMode::TurnBoundary
        }
        fn reasoning_levels(&self) -> &[ReasoningLevel] {
            &[]
        }
        async fn models(&self) -> Result<Vec<Model>, HarnessError> {
            Ok(vec![])
        }
        async fn run(
            &self,
            _request: RunRequest,
            controls: RunControls,
        ) -> Result<BoxStream<'static, Result<AgentEvent, HarnessError>>, HarnessError> {
            let (tx, rx) = tokio::sync::mpsc::channel::<Result<AgentEvent, HarnessError>>(16);
            tokio::spawn(async move {
                let question = zeron_proto::UserInputQuestion {
                    id: "q1".into(),
                    header: "Pick".into(),
                    question: "Which one?".into(),
                    options: vec!["a".into(), "b".into()],
                    multi_select: false,
                };
                // The pre-fix Claude/Codex shape: surface the question under
                // the harness's own id BEFORE asking through the bridge.
                let _ = tx
                    .send(Ok(AgentEvent::InputRequested {
                        request_id: "claude-ctrl-1".into(),
                        questions: vec![question.clone()],
                    }))
                    .await;
                let answers = (controls.request_input)(vec![question])
                    .await
                    .unwrap_or_default();
                let picked = answers
                    .first()
                    .and_then(|a| a.labels.first().cloned())
                    .unwrap_or_else(|| "none".into());
                let _ = tx
                    .send(Ok(AgentEvent::TextDelta {
                        text: format!("picked {picked}"),
                    }))
                    .await;
                let _ = tx.send(Ok(done(DoneStatus::Completed))).await;
            });
            Ok(futures::stream::unfold(rx, |mut rx| async move {
                rx.recv().await.map(|event| (event, rx))
            })
            .boxed())
        }
    }

    let dir = tempfile::tempdir().unwrap();
    let core = assemble(dir.path(), Arc::new(DoubleEmitHarness));
    let handle = core.doc_host.open(CHAT).unwrap();
    queue_as_viewer(
        handle.doc(),
        "cmd-run-twin",
        SessionCommandPayload::Run {
            request: run_request("ask me twice"),
            message_id: "m-1".into(),
        },
    );

    wait_for(
        || {
            core.sessions.session_status(CHAT).map(|s| s.status)
                == Some(SessionStatus::AwaitingInput)
        },
        "awaiting input",
    )
    .await;
    wait_for(
        || {
            entries_now(&core).iter().any(|e| {
                e.parts.iter().any(|p| {
                    matches!(
                        p,
                        MessagePart::Input {
                            resolved: false,
                            ..
                        }
                    )
                })
            })
        },
        "input part in doc",
    )
    .await;

    // Exactly ONE input part folded, and not under the harness's own id.
    let input_ids: Vec<String> = entries(&core)
        .iter()
        .flat_map(|e| {
            e.parts.iter().filter_map(|p| match p {
                MessagePart::Input { request_id, .. } => Some(request_id.clone()),
                _ => None,
            })
        })
        .collect();
    assert_eq!(input_ids.len(), 1, "one chip, not a twin: {input_ids:?}");
    assert_ne!(input_ids[0], "claude-ctrl-1");

    // Answer the LAST unresolved part — exactly what the QuestionPanel does.
    let request_id = entries(&core)
        .iter()
        .rev()
        .find_map(|e| {
            e.parts.iter().rev().find_map(|p| match p {
                MessagePart::Input {
                    request_id,
                    resolved: false,
                    ..
                } => Some(request_id.clone()),
                _ => None,
            })
        })
        .unwrap();
    queue_as_viewer(
        handle.doc(),
        "cmd-answer-twin",
        SessionCommandPayload::RespondInput {
            request_id,
            answers: vec![zeron_proto::UserInputAnswer {
                question_id: "q1".into(),
                labels: vec!["a".into()],
            }],
        },
    );

    // The run resumes and completes; the chip flips to resolved.
    wait_for(
        || {
            entries_now(&core).iter().any(|e| {
                e.status == Some(MessageStatus::Complete)
                    && e.parts
                        .iter()
                        .any(|p| matches!(p, MessagePart::Text { text, .. } if text == "picked a"))
            })
        },
        "answered turn to complete",
    )
    .await;
    assert_eq!(
        command_status(&core, "cmd-answer-twin"),
        Some((SessionCommandStatus::Applied, None))
    );
    assert!(entries(&core).iter().any(|e| {
        e.parts
            .iter()
            .any(|p| matches!(p, MessagePart::Input { resolved: true, .. }))
    }));
    // The run task writes the Complete entry BEFORE settling the status row —
    // wait for the transition instead of asserting the instant in between.
    wait_for(
        || core.sessions.session_status(CHAT).map(|s| s.status) == Some(SessionStatus::Idle),
        "session to settle idle",
    )
    .await;
}

// ---------------------------------------------------------------------------
// Attachments (round 17): chunked upload → durable path → Run carrying both
// the prompt-embedded refs (the persisted transport) and the staged paths.
// ---------------------------------------------------------------------------

/// Delegates to a scripted mock but records every RunRequest the engine hands
/// over (the chat run AND the auto-title run share the harness) — proves
/// `attachments` survives doc-queue → executor → harness.
struct CapturingHarness {
    script: Vec<AgentEvent>,
    seen: Arc<std::sync::Mutex<Vec<RunRequest>>>,
}

#[async_trait]
impl Harness for CapturingHarness {
    fn id(&self) -> HarnessId {
        HarnessId::Mock
    }
    fn display_name(&self) -> &str {
        "Capturing"
    }
    fn supports_steering(&self) -> bool {
        true
    }
    fn steering_mode(&self) -> SteeringMode {
        SteeringMode::StepBoundary
    }
    fn reasoning_levels(&self) -> &[ReasoningLevel] {
        &[ReasoningLevel::Medium]
    }
    async fn models(&self) -> Result<Vec<Model>, HarnessError> {
        Ok(vec![])
    }
    async fn run(
        &self,
        request: RunRequest,
        controls: RunControls,
    ) -> Result<BoxStream<'static, Result<AgentEvent, HarnessError>>, HarnessError> {
        self.seen.lock().unwrap().push(request.clone());
        MockHarness {
            script: self.script.clone(),
        }
        .run(request, controls)
        .await
    }
}

#[tokio::test]
async fn attachment_upload_then_run_threads_refs_and_paths() {
    use base64::Engine as _;
    let b64 = base64::engine::general_purpose::STANDARD;
    let dir = tempfile::tempdir().unwrap();
    let seen: Arc<std::sync::Mutex<Vec<RunRequest>>> = Default::default();
    let core = assemble(
        dir.path(),
        Arc::new(CapturingHarness {
            script: mock_script(),
            seen: seen.clone(),
        }),
    );
    let client = zeron_rpc::memory_client(core.rpc_service());

    // Chunked upload exactly as the composer sends it: base64 split across
    // positional UploadChunk slots, then UploadCommit → the durable path.
    let payload: Vec<u8> = (0..=255u8).cycle().take(9_001).collect();
    let encoded = b64.encode(&payload);
    let (first, second) = encoded.split_at(encoded.len() / 2);
    for (seq, data) in [(0, first), (1, second)] {
        client
            .call(
                zeron_rpc::methods::UPLOAD_CHUNK,
                serde_json::json!({ "uploadId": "e2e-att", "seq": seq, "data": data }),
            )
            .await
            .expect("UploadChunk");
    }
    let committed = client
        .call(
            zeron_rpc::methods::UPLOAD_COMMIT,
            serde_json::json!({ "uploadId": "e2e-att", "fileName": "red.png" }),
        )
        .await
        .expect("UploadCommit");
    let path = committed["path"].as_str().expect("path").to_string();
    assert_eq!(
        std::fs::read(&path).expect("durable upload file"),
        payload,
        "committed file holds the exact reassembled bytes"
    );

    // Run with the zeron `withAttachments` transport: refs embedded in the
    // prompt text (this is what persists), paths on the additive field.
    let prompt = format!(
        "what color is this?\n\nAttached images (local files — open them to view):\n- {path}"
    );
    let mut request = run_request(&prompt);
    request.attachments = vec![path.clone()];
    let handle = core.doc_host.open(CHAT).unwrap();
    queue_as_viewer(
        handle.doc(),
        "cmd-att-1",
        SessionCommandPayload::Run {
            request,
            message_id: "msg-att-1".into(),
        },
    );
    wait_for(
        || {
            entries_now(&core).iter().any(|e| {
                e.role == MessageRole::Assistant && e.status == Some(MessageStatus::Complete)
            })
        },
        "assistant entry to complete",
    )
    .await;

    // Doc user entry: the message text carries the refs verbatim (render-back
    // parses them into thumbnails).
    let all = entries(&core);
    assert_eq!(all[0].id, "msg-att-1");
    assert_eq!(all[0].role, MessageRole::User);
    match &all[0].parts[0] {
        MessagePart::Text { text, .. } => {
            assert!(text.contains("Attached images (local files"));
            assert!(text.contains(&path));
        }
        other => panic!("unexpected user part {other:?}"),
    }

    // The harness saw the staged paths on the request itself (the chat run —
    // NOT the auto-title run, which fires at dispatch now, embeds the user
    // prompt in its wrapper, and legitimately carries no attachments).
    let requests = seen.lock().unwrap().clone();
    let chat_run = requests
        .iter()
        .find(|r| r.prompt == prompt)
        .expect("chat run reached the harness");
    assert_eq!(chat_run.attachments, vec![path.clone()]);
    assert!(chat_run.prompt.contains(&path));

    // Read-back over the same RPC surface the transcript uses.
    let chunk = client
        .call(
            zeron_rpc::methods::READ_ATTACHMENT_CHUNK,
            serde_json::json!({ "path": path, "offset": 0 }),
        )
        .await
        .expect("ReadAttachmentChunk");
    assert_eq!(chunk["mimeType"], "image/png");
    assert_eq!(chunk["name"], "e2e-att-red.png");
}

/// Real-CLI proof of the image pipeline: upload a tiny solid-red PNG through
/// the chunked RPC path, run claude (haiku) with the staged path on
/// `attachments` + the refs in the prompt, and check the reply names the
/// color — it can only know it by SEEING the inline image block (the sandbox
/// prompt forbids opening the file). Ignored by default: needs an installed,
/// authenticated `claude` CLI and spends real tokens.
/// Run with: `cargo test -p zeron-engine --test e2e -- --ignored`
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires installed+authenticated claude CLI; spends tokens"]
async fn real_claude_sees_uploaded_image_inline() {
    use base64::Engine as _;
    let b64 = base64::engine::general_purpose::STANDARD;
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("data");
    let cwd = tmp.path().join("project");
    std::fs::create_dir_all(&cwd).unwrap();

    let core = EngineCore::assemble(
        &dir,
        Arc::new(zeron_engine::default_registry()),
        HarnessId::ClaudeCode,
        None,
    )
    .expect("engine core assembles");
    // Pre-title the chat so the auto-titler doesn't spend a second model call.
    core.workspace
        .create_chat(CHAT, None, Some(&core.device_id), None, Some("/tmp".into()))
        .expect("create chat row");
    core.workspace
        .rename_chat(CHAT, "Pre-titled")
        .expect("rename chat");

    // 8×8 solid-red PNG, uploaded exactly as the composer does.
    const RED_PNG_B64: &str = "iVBORw0KGgoAAAANSUhEUgAAAAgAAAAICAIAAABLbSncAAAAEklEQVR4nGP4z8CAB+GTG2wAAJP0GeGuMDBnAAAAAElFTkSuQmCC";
    let client = zeron_rpc::memory_client(core.rpc_service());
    client
        .call(
            zeron_rpc::methods::UPLOAD_CHUNK,
            serde_json::json!({ "uploadId": "real-img", "seq": 0, "data": RED_PNG_B64 }),
        )
        .await
        .expect("UploadChunk");
    let committed = client
        .call(
            zeron_rpc::methods::UPLOAD_COMMIT,
            serde_json::json!({ "uploadId": "real-img", "fileName": "swatch.png" }),
        )
        .await
        .expect("UploadCommit");
    let path = committed["path"].as_str().expect("path").to_string();
    assert_eq!(
        std::fs::read(&path).expect("committed file"),
        b64.decode(RED_PNG_B64).unwrap()
    );

    let prompt = format!(
        "Without running any tools or opening any files, answer from the attached image alone: \
         what solid color is this image? Reply with exactly one lowercase word.\n\n\
         Attached images (local files — open them to view):\n- {path}"
    );
    let request = RunRequest {
        prompt,
        harness: None,
        model: Some("haiku".into()),
        reasoning: None,
        model_options: Default::default(),
        cwd: cwd.to_string_lossy().to_string(),
        sandbox: SandboxLevel::WorkspaceWrite,
        auto_approve: false,
        enable_workers_mcp: false,
        workers_parent_chat_id: None,
        attachments: vec![path],
        resume: None,
        worktree: None,
    };
    core.doc_host
        .queue_command(
            CHAT,
            SessionCommandPayload::Run {
                request,
                message_id: "msg-img-1".into(),
            },
        )
        .expect("queue real image run");
    wait_for_within_secs(
        || {
            entries_now(&core).iter().any(|e| {
                e.role == MessageRole::Assistant && e.status == Some(MessageStatus::Complete)
            })
        },
        "real claude image turn",
        120,
    )
    .await;

    let reply: String = entries(&core)
        .iter()
        .filter(|e| e.role == MessageRole::Assistant)
        .flat_map(|e| e.parts.iter())
        .filter_map(|p| match p {
            MessagePart::Text { text, .. } => Some(text.clone()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase();
    assert!(
        reply.contains("red"),
        "claude should name the image's color; got: {reply:?}"
    );
    core.shutdown().await;
}

async fn wait_for_within_secs<F>(mut predicate: F, what: &str, secs: u64)
where
    F: FnMut() -> bool,
{
    let deadline = tokio::time::Instant::now() + Duration::from_secs(secs);
    while !predicate() {
        assert!(
            tokio::time::Instant::now() < deadline,
            "timed out waiting for {what}"
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

// ---------------------------------------------------------------------------
// Liveness heartbeats: empty reasoning deltas keep the session fresh but
// never reach the journal (redacted thinking + tool-input-generation noise).
// ---------------------------------------------------------------------------

#[tokio::test]
async fn empty_reasoning_deltas_are_heartbeats_not_journal_noise() {
    let mut script = vec![AgentEvent::SessionStarted {
        harness: HarnessId::Mock,
        model: "mock-1".into(),
        tools: vec![],
        cwd: "/tmp".into(),
        session_id: "hs-hb".into(),
        assistant_message_id: "a-hb".into(),
    }];
    // A long "silent" stretch: redacted thinking / input_json_delta windows
    // stream as empty reasoning deltas.
    for _ in 0..40 {
        script.push(AgentEvent::ReasoningDelta {
            text: String::new(),
        });
    }
    script.push(AgentEvent::ReasoningDelta {
        text: "planning".into(),
    });
    script.push(AgentEvent::TextDelta {
        text: "done".into(),
    });
    script.push(AgentEvent::Done {
        status: DoneStatus::Completed,
        result: Some("done".into()),
        error: None,
        session_id: None,
    });
    let dir = tempfile::tempdir().unwrap();
    let core = assemble(dir.path(), Arc::new(MockHarness { script }));
    let handle = core.doc_host.open(CHAT).unwrap();
    queue_as_viewer(
        handle.doc(),
        "cmd-hb-1",
        SessionCommandPayload::Run {
            request: run_request("hb"),
            message_id: "msg-hb-1".into(),
        },
    );
    wait_for(
        || {
            entries(&core).iter().any(|e| {
                e.role == MessageRole::Assistant && e.status == Some(MessageStatus::Complete)
            })
        },
        "run completes",
    )
    .await;
    // Journal replay: the 40 empties were filtered; real content survived.
    let replay = core.sessions.subscribe(CHAT, 0).unwrap().0;
    let empties = replay
        .iter()
        .filter(|j| matches!(&j.event, AgentEvent::ReasoningDelta { text } if text.is_empty()))
        .count();
    let nonempty = replay
        .iter()
        .filter(|j| matches!(&j.event, AgentEvent::ReasoningDelta { text } if !text.is_empty()))
        .count();
    assert_eq!(empties, 0, "empty reasoning deltas never reach the journal");
    assert_eq!(nonempty, 1, "real reasoning text is preserved");
    assert!(
        replay
            .iter()
            .any(|j| matches!(&j.event, AgentEvent::TextDelta { text } if text == "done")),
        "text deltas unaffected"
    );
}

#[tokio::test]
async fn parked_session_ignores_trailing_frames_and_stays_idle() {
    // ACP children keep forwarding session/update frames after a turn's Done
    // (late tool_call_updates, flushed text). A parked session must treat
    // them as inert: no Working re-arm (the eternally-running-session bug),
    // no phantom assistant entry.
    let mut script = mock_script();
    script.push(AgentEvent::ToolCall {
        id: "tool-1".into(),
        call: ToolCall::Exec {
            command: "echo late-echo".into(),
        },
    });
    script.push(AgentEvent::TextDelta {
        text: "trailing flush".into(),
    });
    let dir = tempfile::tempdir().unwrap();
    let core = assemble(dir.path(), Arc::new(MockHarness { script }));
    let handle = core.doc_host.open(CHAT).unwrap();
    queue_as_viewer(
        handle.doc(),
        "cmd-run-parked",
        SessionCommandPayload::Run {
            request: run_request("go"),
            message_id: "m-parked".into(),
        },
    );

    wait_for(
        || core.sessions.session_status(CHAT).map(|s| s.status) == Some(SessionStatus::Idle),
        "session to complete",
    )
    .await;
    // The trailing frames land right after the park; hold the assertion open
    // past the 120ms flush window to catch a phantom segment or a Working
    // re-arm (both are what the old code did).
    let deadline = tokio::time::Instant::now() + Duration::from_millis(600);
    while tokio::time::Instant::now() < deadline {
        assert_eq!(
            core.sessions.session_status(CHAT).map(|s| s.status),
            Some(SessionStatus::Idle),
            "trailing frames must not re-arm Working"
        );
        let all = entries_now(&core);
        assert!(
            all.len() <= 2,
            "trailing frames must not open a phantom entry: {all:#?}"
        );
        tokio::time::sleep(Duration::from_millis(30)).await;
    }
    let all = entries(&core);
    assert_eq!(all.len(), 2, "user + one assistant entry");
    assert_eq!(all[1].status, Some(MessageStatus::Complete));
}

#[tokio::test]
async fn stale_tool_echo_after_steer_boundary_does_not_split_text() {
    // Adapters re-emit shape-bearing tool_call_updates as full ToolCall
    // events. Once a steer boundary reset the fold, such an echo for a
    // PRIOR segment's tool must not mint a chip mid-text in the new segment
    // (the mid-word transcript splits).
    let script = vec![
        AgentEvent::SessionStarted {
            harness: HarnessId::Mock,
            model: "mock-1".into(),
            tools: vec![],
            cwd: "/tmp".into(),
            session_id: "hs-steer".into(),
            assistant_message_id: "a-1".into(),
        },
        AgentEvent::TextDelta {
            text: "part one".into(),
        },
        AgentEvent::ToolCall {
            id: "tool-long".into(),
            call: ToolCall::Exec {
                command: "sleep 60".into(),
            },
        },
        AgentEvent::Steered {
            assistant_message_id: Some("a-1".into()),
            next_assistant_message_id: Some("a-2".into()),
        },
        AgentEvent::TextDelta {
            text: "part ".into(),
        },
        // The long-running exec from segment one completes mid-stream of the
        // next segment: a shape-bearing echo plus its result.
        AgentEvent::ToolCall {
            id: "tool-long".into(),
            call: ToolCall::Exec {
                command: "sleep 60".into(),
            },
        },
        AgentEvent::ToolResult {
            id: "tool-long".into(),
            is_error: false,
            output: None,
            diff: None,
            execution: None,
        },
        AgentEvent::TextDelta { text: "two".into() },
        done(DoneStatus::Completed),
    ];
    let dir = tempfile::tempdir().unwrap();
    let core = assemble(
        dir.path(),
        Arc::new(ScriptedHarness {
            script,
            step_delay: Duration::from_millis(10),
            hang_until_interrupt: false,
        }),
    );
    let handle = core.doc_host.open(CHAT).unwrap();
    queue_as_viewer(
        handle.doc(),
        "cmd-run-echo",
        SessionCommandPayload::Run {
            request: run_request("go"),
            message_id: "m-echo".into(),
        },
    );

    wait_for(
        || {
            entries_now(&core).len() == 3
                && core.sessions.session_status(CHAT).map(|s| s.status) == Some(SessionStatus::Idle)
        },
        "both segments to land",
    )
    .await;
    let all = entries(&core);
    // Segment one: text + the (unresolved-at-boundary) tool chip.
    assert_eq!(all[1].parts.len(), 2, "{:#?}", all[1].parts);
    assert!(
        all[1].duration_ms.is_some_and(|duration| duration > 0),
        "the segment finalized by Steered must persist its elapsed duration"
    );
    // Segment two: ONE contiguous text part, no spliced chip.
    assert_eq!(
        all[2].parts,
        vec![MessagePart::Text {
            id: "t0".into(),
            text: "part two".into()
        }],
        "stale echo must not split the streaming text"
    );
}

/// The elapsed timer's base (`started_at`) is per user message: a settled
/// session drops it (no reader can resurrect the previous turn's elapsed —
/// the "timer opens at 30:00 on send" bug), and a steer into a PARKED
/// persistent session restamps it fresh for the new turn.
#[tokio::test]
async fn parked_steer_restamps_started_at_and_idle_clears_it() {
    // Steerable harness whose stream stays open after the turn's Done — the
    // engine parks the session — and whose steering mailbox drives turn two.
    struct ParkingHarness;
    #[async_trait]
    impl Harness for ParkingHarness {
        fn id(&self) -> HarnessId {
            HarnessId::Mock
        }
        fn display_name(&self) -> &str {
            "Parking"
        }
        fn supports_steering(&self) -> bool {
            true
        }
        fn steering_mode(&self) -> SteeringMode {
            SteeringMode::StepBoundary
        }
        fn reasoning_levels(&self) -> &[ReasoningLevel] {
            &[]
        }
        async fn models(&self) -> Result<Vec<Model>, HarnessError> {
            Ok(vec![])
        }
        async fn run(
            &self,
            _request: RunRequest,
            mut controls: RunControls,
        ) -> Result<BoxStream<'static, Result<AgentEvent, HarnessError>>, HarnessError> {
            let (tx, rx) = tokio::sync::mpsc::channel::<Result<AgentEvent, HarnessError>>(16);
            tokio::spawn(async move {
                let _ = tx
                    .send(Ok(AgentEvent::TextDelta {
                        text: "turn one".into(),
                    }))
                    .await;
                let _ = tx.send(Ok(done(DoneStatus::Completed))).await;
                // Parked. The next steer is turn two.
                if let Some(_msg) = controls.steering.recv().await {
                    let _ = tx
                        .send(Ok(AgentEvent::Steered {
                            assistant_message_id: None,
                            next_assistant_message_id: None,
                        }))
                        .await;
                    let _ = tx
                        .send(Ok(AgentEvent::TextDelta {
                            text: "turn two".into(),
                        }))
                        .await;
                    // Hold the turn open so the test's poll observes Working
                    // (the transition is otherwise sub-millisecond).
                    tokio::time::sleep(Duration::from_millis(400)).await;
                    let _ = tx.send(Ok(done(DoneStatus::Completed))).await;
                }
            });
            Ok(futures::stream::unfold(rx, |mut rx| async move {
                rx.recv().await.map(|event| (event, rx))
            })
            .boxed())
        }
    }

    let dir = tempfile::tempdir().unwrap();
    let core = assemble(dir.path(), Arc::new(ParkingHarness));
    let handle = core.doc_host.open(CHAT).unwrap();
    queue_as_viewer(
        handle.doc(),
        "cmd-run-park-timer",
        SessionCommandPayload::Run {
            request: run_request("first"),
            message_id: "m-1".into(),
        },
    );
    wait_for(
        || core.sessions.session_status(CHAT).map(|s| s.status) == Some(SessionStatus::Idle),
        "turn one to park",
    )
    .await;
    let parked = core.sessions.session_status(CHAT).unwrap();
    assert_eq!(
        parked.started_at, None,
        "a settled session must drop its timer base"
    );

    let before_steer = chrono::Utc::now();
    queue_as_viewer(
        handle.doc(),
        "cmd-steer-park-timer",
        SessionCommandPayload::Steer {
            prompt: "next".into(),
            message_id: Some("m-2".into()),
        },
    );
    wait_for(
        || core.sessions.session_status(CHAT).map(|s| s.status) == Some(SessionStatus::Working),
        "turn two working",
    )
    .await;
    let working = core.sessions.session_status(CHAT).unwrap();
    let started = working.started_at.expect("Working carries a timer base");
    assert!(
        started >= before_steer,
        "steer into a parked session must restamp started_at (got {started}, steered at {before_steer})"
    );

    wait_for(
        || core.sessions.session_status(CHAT).map(|s| s.status) == Some(SessionStatus::Idle),
        "turn two to settle",
    )
    .await;
    assert_eq!(core.sessions.session_status(CHAT).unwrap().started_at, None);
    let assistant_entries = entries(&core)
        .into_iter()
        .filter(|entry| entry.role == MessageRole::Assistant)
        .collect::<Vec<_>>();
    assert_eq!(assistant_entries.len(), 2);
    assert!(
        assistant_entries[1]
            .duration_ms
            .is_some_and(|duration| duration > 0)
    );
}

/// Regression: turn 2 spawns a fresh runtime process that has not reported a
/// context snapshot yet. The session row feeding the composer gauge must keep
/// the last known measurement across the boundary — dropping it flipped the
/// indicator back to its neutral "no measurement yet" state mid-conversation.
#[tokio::test]
async fn context_usage_survives_the_turn_boundary_until_a_new_measurement() {
    /// Only the first turn reports usage; later turns are silent, like a
    /// restarted runtime that has not billed a snapshot yet.
    struct MeasuresOnlyOnFirstTurn;

    #[async_trait]
    impl Harness for MeasuresOnlyOnFirstTurn {
        fn id(&self) -> HarnessId {
            HarnessId::Mock
        }
        fn display_name(&self) -> &str {
            "Measures once"
        }
        fn supports_steering(&self) -> bool {
            false
        }
        fn steering_mode(&self) -> SteeringMode {
            SteeringMode::StepBoundary
        }
        fn reasoning_levels(&self) -> &[ReasoningLevel] {
            &[ReasoningLevel::Medium]
        }
        async fn models(&self) -> Result<Vec<Model>, HarnessError> {
            Ok(vec![])
        }
        async fn run(
            &self,
            request: RunRequest,
            _controls: RunControls,
        ) -> Result<BoxStream<'static, Result<AgentEvent, HarnessError>>, HarnessError> {
            let measures = request.prompt == "first";
            let mut script = vec![AgentEvent::SessionStarted {
                harness: HarnessId::Mock,
                model: "mock-1".into(),
                tools: vec![],
                cwd: "/tmp".into(),
                session_id: "hs-1".into(),
                assistant_message_id: format!("a-{}", request.prompt),
            }];
            if measures {
                script.push(AgentEvent::Usage {
                    input_tokens: 120_000,
                    output_tokens: 512,
                    context_usage: Some(zeron_proto::ContextUsage {
                        tokens: 120_000,
                        context_window: 200_000,
                    }),
                });
            }
            script.push(AgentEvent::TextDelta {
                text: format!("answering {}", request.prompt),
            });
            script.push(done(DoneStatus::Completed));
            Ok(futures::stream::iter(script.into_iter().map(Ok)).boxed())
        }
    }

    let measured = zeron_proto::ContextUsage {
        tokens: 120_000,
        context_window: 200_000,
    };
    let dir = tempfile::tempdir().unwrap();
    let core = assemble(dir.path(), Arc::new(MeasuresOnlyOnFirstTurn));
    let watch = core.sessions.watch_sessions();
    let usage_now = || watch.borrow().first().and_then(|s| s.context_usage);

    // Record every snapshot the UI could observe, so a momentary clear during
    // the second dispatch fails just as loudly as a permanent one.
    let observed = Arc::new(std::sync::Mutex::new(Vec::new()));
    let sampler = {
        let observed = observed.clone();
        let mut watch = core.sessions.watch_sessions();
        tokio::spawn(async move {
            while watch.changed().await.is_ok() {
                let snapshot = watch.borrow().first().and_then(|s| s.context_usage);
                observed.lock().unwrap().push(snapshot);
            }
        })
    };

    let handle = core.doc_host.open(CHAT).unwrap();
    queue_as_viewer(
        handle.doc(),
        "cmd-usage-turn-1",
        SessionCommandPayload::Run {
            request: run_request("first"),
            message_id: "m-usage-1".into(),
        },
    );
    wait_for(
        || usage_now() == Some(measured),
        "the first turn to report a context measurement",
    )
    .await;
    wait_for(
        || watch.borrow().first().map(|s| s.status) == Some(SessionStatus::Idle),
        "the first turn to settle",
    )
    .await;

    queue_as_viewer(
        handle.doc(),
        "cmd-usage-turn-2",
        SessionCommandPayload::Run {
            request: run_request("second"),
            message_id: "m-usage-2".into(),
        },
    );
    wait_for(
        || {
            entries_now(&core)
                .iter()
                .any(|e| e.id == "m-usage-2" && e.role == MessageRole::User)
        },
        "the second turn to dispatch",
    )
    .await;
    wait_for(
        || {
            watch.borrow().first().map(|s| s.status) == Some(SessionStatus::Idle)
                && entries_now(&core)
                    .iter()
                    .filter(|e| e.role == MessageRole::Assistant)
                    .count()
                    == 2
        },
        "the second turn to settle",
    )
    .await;

    assert_eq!(
        usage_now(),
        Some(measured),
        "a silent second turn must not erase the last known measurement"
    );
    sampler.abort();
    let observed = observed.lock().unwrap().clone();
    let first_measurement = observed
        .iter()
        .position(|snapshot| snapshot == &Some(measured))
        .expect("the first turn's measurement reaches the session row");
    assert!(
        observed[first_measurement..]
            .iter()
            .all(|snapshot| snapshot == &Some(measured)),
        "the gauge fell back to an unmeasured state between turns: {observed:?}"
    );
}
