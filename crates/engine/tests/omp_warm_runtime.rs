//! An OMP chat runtime stays warm between turns (openspec change
//! `preserve-omp-prompt-cache-across-turns`, spec `omp-turn-continuity`).
//!
//! The adapter used to end its stream right after every `Done`, so the
//! engine's PERSISTENT SESSION parking retired the runtime and each message
//! respawned `omp`, re-registered host tools and rewrote the provider cache.
//! The adapter now keeps the stream open: a `Done` settles the turn, the next
//! message is a new `prompt` in the same process (confirmed by a `Steered`
//! boundary), and the engine's parking, reaper, Stop and reconfiguration
//! paths own the runtime's end.
//!
//! The double below is that adapter's contract and nothing else: one process
//! per `run()`, a stream that survives `Done`, `stops_turn_in_place` and
//! `reconfigures_in_place` both false (the OMP choices), and the default
//! `same_runtime` rule (model, reasoning, options, cwd). The real adapter's
//! side of the contract is `crates/harness/tests/omp_rpc.rs`.
//!
//! The idle window is process-global, so it is set once for the whole file.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, Once};
use std::time::Duration;

use async_trait::async_trait;
use futures::StreamExt;
use futures::stream::BoxStream;
use tokio::sync::{Notify, mpsc};

use zeron_engine::{EngineCore, HarnessRegistry};
use zeron_harness::{Harness, HarnessError, RunControls};
use zeron_proto::{
    AgentEvent, DoneStatus, HarnessId, Model, ReasoningLevel, RunRequest, SandboxLevel,
    SessionStatus, SteeringMode,
};

const CHAT: &str = "chat-omp-warm";
/// Long enough for every test here except the reaper's, which waits it out.
const IDLE_MS: u64 = 3_000;

fn init_env() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        // SAFETY: called before any engine (and thus any reader of the var)
        // exists in this test process.
        unsafe { std::env::set_var("ZERON_SESSION_IDLE_MS", IDLE_MS.to_string()) };
    });
}

#[derive(Default)]
struct Ledger {
    spawns: AtomicUsize,
    /// Processes ended through the `interrupt` token (reaper, Stop, config
    /// change, update): the double's stand-in for `process.shutdown()`.
    kills: AtomicUsize,
    /// Every message the live process was given, in order.
    prompts: Mutex<Vec<String>>,
    /// The model each process was spawned with.
    spawned_models: Mutex<Vec<Option<String>>>,
    /// Makes the live process wake on its own (an async result after Done).
    wake: Notify,
    /// Lets a held wake turn finish.
    release_wake: Notify,
    dones: AtomicUsize,
}

struct WarmOmp {
    ledger: Arc<Ledger>,
}

fn done() -> AgentEvent {
    AgentEvent::Done {
        status: DoneStatus::Completed,
        result: None,
        error: None,
        session_id: Some("omp-session".into()),
    }
}

#[async_trait]
impl Harness for WarmOmp {
    fn id(&self) -> HarnessId {
        HarnessId::Omp
    }
    fn display_name(&self) -> &str {
        "OMP"
    }
    fn supports_steering(&self) -> bool {
        true
    }
    fn steering_mode(&self) -> SteeringMode {
        SteeringMode::StepBoundary
    }
    fn reasoning_levels(&self) -> &[ReasoningLevel] {
        &[ReasoningLevel::Medium, ReasoningLevel::High]
    }
    fn deterministic_turn_end(&self) -> bool {
        true
    }
    async fn models(&self) -> Result<Vec<Model>, HarnessError> {
        Ok(vec![])
    }
    async fn run(
        &self,
        request: RunRequest,
        mut controls: RunControls,
    ) -> Result<BoxStream<'static, Result<AgentEvent, HarnessError>>, HarnessError> {
        let ledger = self.ledger.clone();
        ledger.spawns.fetch_add(1, Ordering::SeqCst);
        ledger
            .spawned_models
            .lock()
            .unwrap()
            .push(request.model.clone());
        let (tx, rx) = mpsc::channel::<Result<AgentEvent, HarnessError>>(64);
        tokio::spawn(async move {
            let send = |event: AgentEvent| {
                let tx = tx.clone();
                let ledger = ledger.clone();
                async move {
                    if matches!(event, AgentEvent::Done { .. }) {
                        ledger.dones.fetch_add(1, Ordering::SeqCst);
                    }
                    tx.send(Ok(event)).await.is_ok()
                }
            };
            let _ = send(AgentEvent::SessionStarted {
                harness: HarnessId::Omp,
                model: "omp".into(),
                tools: vec![],
                cwd: request.cwd.clone(),
                session_id: "omp-session".into(),
                assistant_message_id: "a0".into(),
            })
            .await;
            ledger.prompts.lock().unwrap().push(request.prompt.clone());
            let _ = send(AgentEvent::TextDelta {
                text: format!("on it: {}", request.prompt),
            })
            .await;
            if !send(done()).await {
                return;
            }
            // Warm: the stream outlives the Done.
            loop {
                tokio::select! {
                    biased;
                    _ = controls.interrupt.cancelled() => {
                        // Idle: nothing to abort, no Done owed.
                        ledger.kills.fetch_add(1, Ordering::SeqCst);
                        return;
                    }
                    _ = ledger.wake.notified() => {
                        // OMP woke on its own: the turn the engine did not start.
                        if !send(AgentEvent::TextDelta { text: "woke on its own".into() }).await {
                            return;
                        }
                        ledger.release_wake.notified().await;
                        if !send(done()).await {
                            return;
                        }
                    }
                    steer = controls.steering.recv() => {
                        let Some(steer) = steer else { return };
                        ledger.prompts.lock().unwrap().push(steer.prompt.clone());
                        // The adapter's idle-prompt boundary, then the turn.
                        let boundary = AgentEvent::Steered {
                            assistant_message_id: steer.message_id.clone(),
                            next_assistant_message_id: None,
                        };
                        if !send(boundary).await
                            || !send(AgentEvent::TextDelta { text: format!("on it: {}", steer.prompt) }).await
                            || !send(done()).await
                        {
                            return;
                        }
                    }
                }
            }
        });
        Ok(
            futures::stream::unfold(rx, |mut rx| async move { rx.recv().await.map(|e| (e, rx)) })
                .boxed(),
        )
    }
}

struct Rig {
    core: EngineCore,
    ledger: Arc<Ledger>,
    _dir: tempfile::TempDir,
}

fn rig() -> Rig {
    init_env();
    let ledger = Arc::new(Ledger::default());
    let registry = HarnessRegistry::new();
    registry.register(Arc::new(WarmOmp {
        ledger: ledger.clone(),
    }));
    let dir = tempfile::tempdir().unwrap();
    let core = EngineCore::assemble(dir.path(), Arc::new(registry), HarnessId::Omp, None)
        .expect("engine core assembles");
    Rig {
        core,
        ledger,
        _dir: dir,
    }
}

fn request(prompt: &str) -> RunRequest {
    RunRequest {
        access: Default::default(),
        extra_mcp: Default::default(),
        plan: Default::default(),
        mcp: None,
        enable_workers_mcp: true,
        workers_parent_chat_id: Some(CHAT.into()),
        sessions: None,
        env: Default::default(),
        prompt: prompt.into(),
        harness: Some(HarnessId::Omp),
        model: Some("anthropic/claude-opus-5-5".into()),
        reasoning: Some(ReasoningLevel::High),
        model_options: serde_json::Map::new(),
        cwd: "/tmp".into(),
        sandbox: SandboxLevel::WorkspaceWrite,
        auto_approve: false,
        attachments: Vec::new(),
        worktree: None,
        resume: None,
    }
}

fn status(core: &EngineCore) -> Option<SessionStatus> {
    core.sessions.session_status(CHAT).map(|s| s.status)
}

async fn wait_for(mut predicate: impl FnMut() -> bool, what: &str) {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(15);
    while !predicate() {
        assert!(
            tokio::time::Instant::now() < deadline,
            "timed out waiting for {what}"
        );
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

async fn parked(rig: &Rig, prompts: usize) {
    wait_for(
        || {
            status(&rig.core) == Some(SessionStatus::Idle)
                && rig.ledger.prompts.lock().unwrap().len() >= prompts
        },
        "the turn to park",
    )
    .await;
}

async fn dispatch(rig: &Rig, request: RunRequest) {
    rig.core
        .sessions
        .dispatch(CHAT, HarnessId::Omp, request, None)
        .await
        .unwrap();
}

#[tokio::test]
async fn the_second_turn_routes_into_the_parked_omp_runtime() {
    let rig = rig();
    dispatch(&rig, request("first")).await;
    parked(&rig, 1).await;
    dispatch(&rig, request("second")).await;
    parked(&rig, 2).await;
    dispatch(&rig, request("third")).await;
    parked(&rig, 3).await;

    assert_eq!(rig.ledger.spawns.load(Ordering::SeqCst), 1, "one process");
    assert_eq!(rig.ledger.kills.load(Ordering::SeqCst), 0);
    assert_eq!(
        *rig.ledger.prompts.lock().unwrap(),
        ["first", "second", "third"]
    );
    assert_eq!(
        rig.ledger.dones.load(Ordering::SeqCst),
        3,
        "exactly one Done per turn"
    );
    rig.core.sessions.shutdown().await;
}

#[tokio::test]
async fn the_idle_reaper_retires_a_warm_runtime_and_the_next_message_starts_a_new_one() {
    let rig = rig();
    dispatch(&rig, request("first")).await;
    parked(&rig, 1).await;
    assert_eq!(rig.ledger.kills.load(Ordering::SeqCst), 0);

    wait_for(
        || rig.ledger.kills.load(Ordering::SeqCst) == 1,
        "the idle reaper to retire the warm runtime",
    )
    .await;
    wait_for(
        || status(&rig.core) == Some(SessionStatus::Idle),
        "the session to settle idle",
    )
    .await;

    dispatch(&rig, request("after the reap")).await;
    wait_for(
        || rig.ledger.spawns.load(Ordering::SeqCst) == 2,
        "a new runtime for the next message",
    )
    .await;
    parked(&rig, 2).await;
    assert_eq!(
        *rig.ledger.prompts.lock().unwrap(),
        ["first", "after the reap"]
    );
    rig.core.sessions.shutdown().await;
}

#[tokio::test]
async fn a_model_change_replaces_the_warm_omp_runtime() {
    let rig = rig();
    dispatch(&rig, request("first")).await;
    parked(&rig, 1).await;

    let mut other = request("on another model");
    other.model = Some("anthropic/claude-sonnet-5-5".into());
    dispatch(&rig, other).await;
    wait_for(
        || rig.ledger.spawns.load(Ordering::SeqCst) == 2,
        "the replacement runtime",
    )
    .await;
    parked(&rig, 2).await;

    assert_eq!(
        rig.ledger.kills.load(Ordering::SeqCst),
        1,
        "old one retired"
    );
    assert_eq!(
        *rig.ledger.spawned_models.lock().unwrap(),
        [
            Some("anthropic/claude-opus-5-5".to_owned()),
            Some("anthropic/claude-sonnet-5-5".to_owned())
        ]
    );

    // A reasoning change is a different runtime too.
    let mut lower = request("lower reasoning");
    lower.model = Some("anthropic/claude-sonnet-5-5".into());
    lower.reasoning = Some(ReasoningLevel::Medium);
    dispatch(&rig, lower).await;
    wait_for(
        || rig.ledger.spawns.load(Ordering::SeqCst) == 3,
        "a runtime for the new reasoning level",
    )
    .await;
    parked(&rig, 3).await;
    assert_eq!(rig.ledger.kills.load(Ordering::SeqCst), 2);
    rig.core.sessions.shutdown().await;
}

#[tokio::test]
async fn stop_retires_the_warm_omp_runtime() {
    let rig = rig();
    dispatch(&rig, request("first")).await;
    parked(&rig, 1).await;

    rig.core.sessions.interrupt(CHAT).await.unwrap();
    wait_for(
        || rig.ledger.kills.load(Ordering::SeqCst) == 1,
        "Stop to retire the runtime",
    )
    .await;

    dispatch(&rig, request("after stop")).await;
    wait_for(
        || rig.ledger.spawns.load(Ordering::SeqCst) == 2,
        "a new runtime after Stop",
    )
    .await;
    parked(&rig, 2).await;
    rig.core.sessions.shutdown().await;
}

#[tokio::test]
async fn an_autonomous_wake_after_done_settles_once_and_leaves_the_runtime_warm() {
    let rig = rig();
    dispatch(&rig, request("first")).await;
    parked(&rig, 1).await;

    // The engine's self-continued path ignores output that lands within a
    // second of the park (a finished turn's tail flush).
    tokio::time::sleep(Duration::from_millis(1_150)).await;
    rig.ledger.wake.notify_one();
    wait_for(
        || status(&rig.core) == Some(SessionStatus::Working),
        "the engine to take the wake as a turn",
    )
    .await;
    rig.ledger.release_wake.notify_one();
    wait_for(
        || {
            status(&rig.core) == Some(SessionStatus::Idle)
                && rig.ledger.dones.load(Ordering::SeqCst) == 2
        },
        "the wake turn to settle on its single Done",
    )
    .await;

    // Still the same warm runtime, and it still takes the next message.
    dispatch(&rig, request("after the wake")).await;
    parked(&rig, 2).await;
    assert_eq!(rig.ledger.spawns.load(Ordering::SeqCst), 1);
    assert_eq!(rig.ledger.kills.load(Ordering::SeqCst), 0);
    assert_eq!(rig.ledger.dones.load(Ordering::SeqCst), 3);
    rig.core.sessions.shutdown().await;
}

/// The real adapter supplies pending activity before the engine parks the turn.
struct NativeOmp {
    harness: zeron_harness::OmpHarness,
    turn: Arc<Mutex<Option<zeron_harness::TurnControl>>>,
    dones: Arc<AtomicUsize>,
}
#[async_trait]
impl Harness for NativeOmp {
    fn id(&self) -> HarnessId {
        HarnessId::Omp
    }
    fn display_name(&self) -> &str {
        "OMP"
    }
    fn supports_steering(&self) -> bool {
        true
    }
    fn steering_mode(&self) -> SteeringMode {
        SteeringMode::StepBoundary
    }
    fn reasoning_levels(&self) -> &[ReasoningLevel] {
        &[ReasoningLevel::High]
    }
    fn deterministic_turn_end(&self) -> bool {
        true
    }
    async fn models(&self) -> Result<Vec<Model>, HarnessError> {
        Ok(vec![])
    }
    async fn run(
        &self,
        request: RunRequest,
        controls: RunControls,
    ) -> Result<BoxStream<'static, Result<AgentEvent, HarnessError>>, HarnessError> {
        *self.turn.lock().unwrap() = Some(controls.turn.clone());
        let dones = self.dones.clone();
        Ok(self
            .harness
            .run(request, controls)
            .await?
            .inspect(move |event| {
                if matches!(event, Ok(AgentEvent::Done { .. })) {
                    dones.fetch_add(1, Ordering::SeqCst);
                }
            })
            .boxed())
    }
}

#[cfg(unix)]
#[tokio::test]
async fn omp_background_pending_blocks_reaper_then_settles_without_another_run() {
    use std::os::unix::fs::PermissionsExt;
    init_env();
    let dir = tempfile::tempdir().unwrap();
    let executable = dir.path().join("omp");
    let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../harness/tests/fixtures/fake-omp-rpc.sh");
    std::fs::write(&executable,format!("#!/bin/sh\nif [ \"$1\" = --version ]; then echo 18.18.0; exit 0; fi\nexec '{}' \"$@\"\n",fixture.display())).unwrap();
    std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o755)).unwrap();
    let log = dir.path().join("commands.log");
    let turn = Arc::new(Mutex::new(None));
    let dones = Arc::new(AtomicUsize::new(0));
    let registry = HarnessRegistry::new();
    registry.register(Arc::new(NativeOmp {
        harness: zeron_harness::OmpHarness::new()
            .with_executable(executable)
            .with_env(std::collections::HashMap::from([
                ("FAKE_OMP_SCENARIO".into(), "yield-lifecycle-settled".into()),
                ("FAKE_OMP_LOG".into(), log.to_string_lossy().into_owned()),
            ]))
            .with_timeouts(Duration::from_secs(2), Duration::from_secs(2)),
        turn: turn.clone(),
        dones: dones.clone(),
    }));
    let core = EngineCore::assemble(dir.path(), Arc::new(registry), HarnessId::Omp, None).unwrap();
    let mut req = request("first");
    req.model = Some("openai-codex/gpt-5.6-sol".into());
    req.enable_workers_mcp = false;
    req.cwd = dir.path().to_string_lossy().into_owned();
    core.sessions
        .dispatch(CHAT, HarnessId::Omp, req, None)
        .await
        .unwrap();
    wait_for(
        || status(&core) == Some(SessionStatus::Idle) && dones.load(Ordering::SeqCst) == 1,
        "native yield",
    )
    .await;
    let turn = turn.lock().unwrap().clone().unwrap();
    assert!(
        turn.background_live(),
        "native async delivery was discarded at Done"
    );
    let pid = std::fs::read_to_string(&log)
        .unwrap()
        .lines()
        .next()
        .unwrap()
        .split_whitespace()
        .next()
        .unwrap()
        .to_owned();
    tokio::time::sleep(Duration::from_millis(IDLE_MS + 500)).await;
    assert!(
        std::process::Command::new("kill")
            .args(["-0", &pid])
            .status()
            .unwrap()
            .success(),
        "reaper killed pending native delivery"
    );
    let _ = turn.inspect_context().await;
    wait_for(|| !turn.background_live(), "native session settlement").await;
    assert_eq!(
        dones.load(Ordering::SeqCst),
        1,
        "settlement added a completion"
    );
    core.sessions.shutdown().await;
    assert!(
        !std::process::Command::new("kill")
            .args(["-0", &pid])
            .status()
            .unwrap()
            .success()
    );
}
