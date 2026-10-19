use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use futures::StreamExt;
use futures::stream::BoxStream;
use zeron_engine::{EngineCore, HarnessRegistry, sessions::SteerOutcome};
use zeron_harness::{Harness, HarnessError, RunControls};
use zeron_proto::{
    AgentEvent, HarnessId, Model, ReasoningLevel, RunRequest, SandboxLevel, SessionStatus,
    SteeringMode,
};

struct ControlledHarness(Mutex<Option<tokio::sync::mpsc::UnboundedReceiver<AgentEvent>>>);

#[async_trait]
impl Harness for ControlledHarness {
    fn id(&self) -> HarnessId {
        HarnessId::Mock
    }
    fn display_name(&self) -> &str {
        "Controlled"
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
        _: RunRequest,
        controls: RunControls,
    ) -> Result<BoxStream<'static, Result<AgentEvent, HarnessError>>, HarnessError> {
        let rx = self.0.lock().unwrap().take().unwrap();
        Ok(
            futures::stream::unfold((rx, controls), |(mut rx, controls)| async move {
                tokio::select! {
                    _ = controls.interrupt.cancelled() => None,
                    event = rx.recv() => event.map(|e| (Ok(e), (rx, controls))),
                }
            })
            .boxed(),
        )
    }
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
    }
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

#[tokio::test]
async fn worker_wait_state_observes_steer_lifecycle_and_isolates_chats() {
    const CHAT_A: &str = "chat-wait-state-a";
    const CHAT_B: &str = "chat-wait-state-b";
    const CHAT_QUEUED_ONLY: &str = "chat-wait-state-queued";
    const CHAT_NO_RUN: &str = "chat-wait-state-no-run";
    const CHAT_REMOTE: &str = "chat-wait-state-remote";

    let (tx_a, rx_a) = tokio::sync::mpsc::unbounded_channel();
    let dir = tempfile::tempdir().unwrap();

    let registry = HarnessRegistry::new();
    registry.register(Arc::new(ControlledHarness(Mutex::new(Some(rx_a)))));
    let core = EngineCore::assemble(dir.path(), Arc::new(registry), HarnessId::Mock, None)
        .expect("assemble engine core");

    for chat_id in [CHAT_A, CHAT_B, CHAT_QUEUED_ONLY, CHAT_NO_RUN] {
        core.workspace
            .create_chat(chat_id, None, Some(&core.device_id), None, None)
            .expect("register local chat in workspace");
    }
    core.workspace
        .create_chat(CHAT_REMOTE, None, Some("remote-device-id"), None, None)
        .expect("register remote chat in workspace");

    let client = zeron_rpc::memory_client(core.rpc_service());

    // Chat with no active run observes false.
    let res = client
        .call(
            "WorkerWaitState",
            serde_json::json!({ "chatId": CHAT_NO_RUN }),
        )
        .await
        .expect("WorkerWaitState query on no-run chat");
    assert_eq!(
        res["pendingMessage"].as_bool(),
        Some(false),
        "no-run chat must have pendingMessage=false"
    );

    // Active run before any steer observes false.
    core.sessions
        .dispatch(CHAT_A, HarnessId::Mock, run_request("opening"), None)
        .await
        .expect("dispatch run on CHAT_A");

    tx_a.send(AgentEvent::SessionStarted {
        harness: HarnessId::Mock,
        model: "mock-1".into(),
        tools: vec![],
        cwd: "/tmp".into(),
        session_id: "hs-1".into(),
        assistant_message_id: "a-1".into(),
    })
    .expect("send SessionStarted");

    wait_for(
        || core.sessions.session_status(CHAT_A).map(|s| s.status) == Some(SessionStatus::Working),
        "CHAT_A reaching Working status",
    )
    .await;

    let res = client
        .call("WorkerWaitState", serde_json::json!({ "chatId": CHAT_A }))
        .await
        .expect("WorkerWaitState on CHAT_A before steer");
    assert_eq!(
        res["pendingMessage"].as_bool(),
        Some(false),
        "running chat without steer must have pendingMessage=false"
    );

    // Accepted steer held in mailbox ledger produces true while unconsumed.
    let outcome = core
        .sessions
        .steer(CHAT_A, "steer redirect", Some("user-steer-1".into()))
        .await
        .expect("steer CHAT_A");
    assert_eq!(outcome, SteerOutcome::Accepted);

    let res = client
        .call("WorkerWaitState", serde_json::json!({ "chatId": CHAT_A }))
        .await
        .expect("WorkerWaitState on CHAT_A with unconsumed steer");
    assert_eq!(
        res["pendingMessage"].as_bool(),
        Some(true),
        "accepted unconsumed steer must produce pendingMessage=true"
    );

    let typed_res: zeron_rpc::WorkerWaitStateReply = client
        .call_as(
            zeron_rpc::methods::WORKER_WAIT_STATE,
            serde_json::to_value(zeron_rpc::WorkerWaitStateParams::new(CHAT_A)).unwrap(),
        )
        .await
        .expect("typed WorkerWaitState call");
    assert!(
        typed_res.pending_message,
        "typed WorkerWaitState call must also observe pending_message=true"
    );

    // Unrelated Chat isolation.
    let res = client
        .call("WorkerWaitState", serde_json::json!({ "chatId": CHAT_B }))
        .await
        .expect("WorkerWaitState on CHAT_B");
    assert_eq!(
        res["pendingMessage"].as_bool(),
        Some(false),
        "unrelated CHAT_B must observe pendingMessage=false"
    );

    // Queued-only row remains held in doc queue without engine dispatch, observing false.
    let handle_queued = core
        .doc_host
        .open(CHAT_QUEUED_ONLY)
        .expect("open queued doc");
    let queued_id = core
        .doc_host
        .queue_message(CHAT_QUEUED_ONLY, "queued only prompt", Vec::new())
        .expect("queue message in doc");
    let queued_rows = handle_queued.doc().read_queue().expect("read doc queue");
    assert!(
        queued_rows.iter().any(|q| q.id == queued_id),
        "queued message must remain held in doc queue"
    );
    assert!(
        !core.sessions.turn_in_flight(CHAT_QUEUED_ONLY),
        "queued message must not auto-dispatch to engine turn"
    );

    let res = client
        .call(
            "WorkerWaitState",
            serde_json::json!({ "chatId": CHAT_QUEUED_ONLY }),
        )
        .await
        .expect("WorkerWaitState on queued-only chat");
    assert_eq!(
        res["pendingMessage"].as_bool(),
        Some(false),
        "held queued-only row must observe pendingMessage=false"
    );
    drop(handle_queued);

    // Provider consumption via Steered event clears ledger back to false.
    tx_a.send(AgentEvent::Steered {
        assistant_message_id: Some("a-1".into()),
        next_assistant_message_id: Some("a-steered-1".into()),
    })
    .expect("send AgentEvent::Steered");

    let mut consumed = false;
    for _ in 0..100 {
        if let Ok(res) = client
            .call("WorkerWaitState", serde_json::json!({ "chatId": CHAT_A }))
            .await
        {
            if res["pendingMessage"].as_bool() == Some(false) {
                consumed = true;
                break;
            }
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert!(consumed, "steer must be consumed by AgentEvent::Steered");

    // Steer followed by turn termination clears to false.
    let outcome = core
        .sessions
        .steer(CHAT_A, "second steer", Some("user-steer-2".into()))
        .await
        .expect("second steer on CHAT_A");
    assert_eq!(outcome, SteerOutcome::Accepted);

    let res = client
        .call("WorkerWaitState", serde_json::json!({ "chatId": CHAT_A }))
        .await
        .expect("WorkerWaitState with second steer");
    assert_eq!(res["pendingMessage"].as_bool(), Some(true));

    core.sessions
        .terminate(CHAT_A)
        .await
        .expect("terminate CHAT_A");

    let res = client
        .call("WorkerWaitState", serde_json::json!({ "chatId": CHAT_A }))
        .await
        .expect("WorkerWaitState on stopped CHAT_A");
    assert_eq!(
        res["pendingMessage"].as_bool(),
        Some(false),
        "stopped run must have pendingMessage=false"
    );

    // Foreign device chat ownership check.
    let err = client
        .call(
            "WorkerWaitState",
            serde_json::json!({ "chatId": CHAT_REMOTE }),
        )
        .await
        .expect_err("WorkerWaitState must reject chat belonging to another device");
    assert!(
        err.to_string().contains("another device"),
        "error should indicate chat belongs to another device, got: {err}"
    );

    core.sessions.shutdown().await;
}
