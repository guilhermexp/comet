//! `RunControls.chat_id` must survive the executor boundary.
//!
//! The comet MCP server is chat-scoped (`OpenTerminal` takes a chat id), and
//! the harness learns that id only from `RunControls`. A test that builds
//! `RunControls` by hand proves the struct has a field, not that `dispatch`
//! fills it — so this drives `sessions.dispatch` for a known chat and reads
//! back what the harness was actually handed.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use futures::StreamExt;
use futures::stream::BoxStream;

use comet_engine::{EngineCore, HarnessRegistry};
use comet_harness::{Harness, HarnessError, RunControls};
use comet_proto::{
    AgentEvent, DoneStatus, HarnessId, Model, ReasoningLevel, RunRequest, SandboxLevel,
    SteeringMode,
};

/// Records the `chat_id` of every run it is handed, then completes.
struct RecordingHarness {
    seen: Arc<Mutex<Vec<String>>>,
}

#[async_trait]
impl Harness for RecordingHarness {
    fn id(&self) -> HarnessId {
        HarnessId::Mock
    }
    fn display_name(&self) -> &str {
        "Recording"
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
        self.seen.lock().unwrap().push(controls.chat_id.clone());
        Ok(futures::stream::iter(vec![Ok(AgentEvent::Done {
            status: DoneStatus::Completed,
            result: None,
            error: None,
            session_id: None,
        })])
        .boxed())
    }
}

#[tokio::test]
async fn dispatch_hands_the_chat_id_to_the_harness() {
    let dir = tempfile::tempdir().expect("tempdir");
    let seen = Arc::new(Mutex::new(Vec::new()));
    let registry = HarnessRegistry::new();
    registry.register(Arc::new(RecordingHarness { seen: seen.clone() }));
    let core = EngineCore::assemble(dir.path(), Arc::new(registry), HarnessId::Mock, None)
        .expect("engine core assembles");

    let chat_id = "chat-controls-1";
    core.sessions
        .dispatch(
            chat_id,
            HarnessId::Mock,
            RunRequest {
                prompt: "hello".into(),
                model: None,
                reasoning: None,
                model_options: serde_json::Map::new(),
                cwd: "/tmp".into(),
                sandbox: SandboxLevel::WorkspaceWrite,
                auto_approve: true,
                attachments: Vec::new(),
                resume: None,
            },
            None,
        )
        .await
        .expect("dispatch");

    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    loop {
        if !seen.lock().unwrap().is_empty() {
            break;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "harness never ran for {chat_id}"
        );
        tokio::time::sleep(Duration::from_millis(15)).await;
    }
    // The dispatch also kicks off the auto-titling run, which is a second run
    // for the same chat: every run the executor starts must carry the id, so
    // assert on all of them rather than only the first.
    let recorded = seen.lock().unwrap().clone();
    assert!(
        recorded.iter().all(|id| id == chat_id),
        "every run must carry the dispatching chat id, got {recorded:?}"
    );
}
