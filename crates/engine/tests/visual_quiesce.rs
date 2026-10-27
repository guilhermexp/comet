//! An accepted `show_visual` is turn activity: the quiesce watchdog measures
//! its window from it, as it did when the visual came in through the run's
//! event channel. A refused one is not. Paused tokio time drives the window
//! deterministically (no wall-clock sleeps); the window is far longer than
//! any timer the engine arms meanwhile, and assertions keep a minute of
//! margin on either side of each deadline.
//!
//! Separate file: the window is a process-global env knob.

use std::sync::{Arc, Once};
use std::time::Duration;

use async_trait::async_trait;
use futures::StreamExt;
use futures::stream::BoxStream;
use tokio::sync::{Mutex, mpsc};
use tokio::time::Instant;

use zeron_engine::{EngineCore, HarnessRegistry, JournaledEvent, SteerOutcome};
use zeron_harness::{Harness, HarnessError, RunControls};
use zeron_proto::{
    AgentEvent, DoneStatus, HarnessId, Model, ReasoningLevel, RunRequest, SandboxLevel,
    SessionStatus, SteeringMode,
};

const CHAT: &str = "chat-visual-quiesce";
const WINDOW: Duration = Duration::from_secs(600);
const MARGIN: Duration = Duration::from_secs(60);

fn init_quiesce_env() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        // SAFETY: set before any engine (and thus any reader of the var)
        // exists in this test process.
        unsafe { std::env::set_var("ZERON_TURN_QUIESCE_MS", WINDOW.as_millis().to_string()) };
    });
}

fn run_request(prompt: &str) -> RunRequest {
    RunRequest {
        mcp: None,
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
        sessions: None,
        attachments: Vec::new(),
        worktree: None,
        resume: None,
        env: Default::default(),
    }
}

fn done(status: DoneStatus) -> AgentEvent {
    AgentEvent::Done {
        status,
        result: None,
        error: None,
        session_id: Some("hs-vq".into()),
    }
}

/// Feed-by-hand harness: the test pushes events through a channel, so it can
/// model turn boundaries, self-continuation, and a LOST turn-end exactly.
/// Accepted steers are confirmed with a `Steered` boundary, like the ACP
/// adapters do. The feed is served only to the test's own dispatch (matched
/// by prompt) — the engine's auto-titler also runs this harness, and gets an
/// immediately-completed empty stream instead.
struct FeedHarness {
    main_prompt: String,
    feed: Mutex<Option<mpsc::UnboundedReceiver<AgentEvent>>>,
}

#[async_trait]
impl Harness for FeedHarness {
    fn id(&self) -> HarnessId {
        HarnessId::Mock
    }
    fn display_name(&self) -> &str {
        "Feed"
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
        mut controls: RunControls,
    ) -> Result<BoxStream<'static, Result<AgentEvent, HarnessError>>, HarnessError> {
        if request.prompt != self.main_prompt {
            // The auto-titler's side run: complete instantly with nothing.
            let events = vec![Ok(done(DoneStatus::Completed))];
            return Ok(futures::stream::iter(events).boxed());
        }
        let mut feed = self
            .feed
            .lock()
            .await
            .take()
            .expect("FeedHarness serves the main dispatch once per test");
        let (tx, rx) = mpsc::channel::<Result<AgentEvent, HarnessError>>(64);
        tokio::spawn(async move {
            let mut steering_open = true;
            loop {
                tokio::select! {
                    // Steers first: a Steered boundary always precedes feed
                    // events the test sends after steering (determinism).
                    biased;
                    steer = controls.steering.recv(), if steering_open => match steer {
                        Some(_) => {
                            let boundary = AgentEvent::Steered {
                                assistant_message_id: None,
                                next_assistant_message_id: None,
                            };
                            if tx.send(Ok(boundary)).await.is_err() {
                                return;
                            }
                        }
                        None => steering_open = false,
                    },
                    event = feed.recv() => match event {
                        Some(event) => {
                            if tx.send(Ok(event)).await.is_err() {
                                return;
                            }
                        }
                        None => return,
                    },
                }
            }
        });
        Ok(futures::stream::unfold(rx, |mut rx| async move {
            rx.recv().await.map(|event| (event, rx))
        })
        .boxed())
    }
}

struct Rig {
    core: EngineCore,
    feed: mpsc::UnboundedSender<AgentEvent>,
    _dir: tempfile::TempDir,
}

fn assemble(main_prompt: &str) -> Rig {
    init_quiesce_env();
    let (feed, rx) = mpsc::unbounded_channel();
    let registry = HarnessRegistry::new();
    registry.register(Arc::new(FeedHarness {
        main_prompt: main_prompt.into(),
        feed: Mutex::new(Some(rx)),
    }));
    let dir = tempfile::tempdir().unwrap();
    let core = EngineCore::assemble(dir.path(), Arc::new(registry), HarnessId::Mock, None)
        .expect("engine core assembles");
    Rig {
        core,
        feed,
        _dir: dir,
    }
}

fn status(core: &EngineCore) -> Option<SessionStatus> {
    core.sessions.session_status(CHAT).map(|s| s.status)
}

async fn next_event(
    events: &mut tokio::sync::broadcast::Receiver<JournaledEvent>,
    want: impl Fn(&AgentEvent) -> bool,
) {
    while !want(&events.recv().await.unwrap().event) {}
}

/// Moves paused time to `at`, then lets every task woken by it run.
async fn advance_to(at: Instant) {
    tokio::time::advance(at.saturating_duration_since(Instant::now())).await;
    tokio::time::sleep(Duration::from_millis(1)).await;
}

fn visual(id: &str) -> AgentEvent {
    AgentEvent::Visual {
        id: id.into(),
        path: format!("/uploads/{id}-visual.luau"),
        title: id.into(),
    }
}

#[tokio::test(start_paused = true)]
async fn an_accepted_visual_renews_the_quiesce_window_and_a_refused_one_does_not() {
    let rig = assemble("chart it");
    let (_, mut events) = rig.core.sessions.subscribe(CHAT, 0).unwrap();
    rig.core
        .sessions
        .dispatch(CHAT, HarnessId::Mock, run_request("chart it"), None)
        .await
        .expect("dispatch");
    rig.feed
        .send(AgentEvent::TextDelta {
            text: "Here is the chart.".into(),
        })
        .unwrap();
    next_event(&mut events, |e| matches!(e, AgentEvent::TextDelta { .. })).await;
    // The watchdog measures from this text at the latest.
    let text_at = Instant::now();

    // Accepted shortly before the window ends: the window starts over.
    advance_to(text_at + WINDOW - MARGIN).await;
    assert_eq!(status(&rig.core), Some(SessionStatus::Working));
    let first = rig.core.sessions.visual_turn(CHAT).expect("live turn");
    assert!(
        rig.core
            .sessions
            .publish_visual(CHAT, &first, visual("v:near-end"))
            .await
    );
    let shown_at = Instant::now();
    advance_to(text_at + WINDOW + MARGIN).await;
    assert_eq!(
        status(&rig.core),
        Some(SessionStatus::Working),
        "quiesced on the window from before the accepted visual"
    );
    advance_to(shown_at + WINDOW + MARGIN).await;
    assert_eq!(
        status(&rig.core),
        Some(SessionStatus::Idle),
        "the renewed window still settles the turn"
    );

    // The next turn: a refused publication (the old turn's ticket) near
    // its end does not hold it open.
    let outcome = rig
        .core
        .sessions
        .steer(CHAT, "and now?", None)
        .await
        .expect("steer");
    assert!(matches!(outcome, SteerOutcome::Accepted));
    next_event(&mut events, |e| matches!(e, AgentEvent::Steered { .. })).await;
    let steered_at = Instant::now();
    assert_eq!(status(&rig.core), Some(SessionStatus::Working));
    advance_to(steered_at + WINDOW - MARGIN).await;
    assert!(
        !rig.core
            .sessions
            .publish_visual(CHAT, &first, visual("v:stale"))
            .await
    );
    advance_to(steered_at + WINDOW + MARGIN).await;
    assert_eq!(
        status(&rig.core),
        Some(SessionStatus::Idle),
        "a refused visual prolonged the turn"
    );
    rig.core.sessions.shutdown().await;
}
