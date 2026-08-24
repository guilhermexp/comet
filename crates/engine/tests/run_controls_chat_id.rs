//! `RunControls.chat_id` must survive the executor boundary.
//!
//! The comet MCP server is chat-scoped (`OpenTerminal` takes a chat id), and
//! the harness learns that id only from `RunControls`. A test that builds
//! `RunControls` by hand proves the struct has a field, not that `dispatch`
//! fills it — so this drives `sessions.dispatch` for a known chat and reads
//! back what the harness was actually handed.
//!
//! One dispatch starts TWO runs: the user's turn, and the auto-titling run the
//! executor fires off the first prompt. Both cross the same boundary and both
//! must carry the id, so the assertions wait for both rather than stopping at
//! whichever lands first. They are told apart by their sandbox level, which is
//! the one field the two paths set differently (`titles.rs` runs read-only).

use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use futures::StreamExt;
use futures::stream::BoxStream;

use zeron_engine::{EngineCore, HarnessRegistry};
use zeron_harness::{Harness, HarnessError, RunControls};
use zeron_proto::{
    AgentEvent, DoneStatus, HarnessId, Model, ReasoningLevel, RunRequest, SandboxLevel,
    SteeringMode,
};

const CHAT: &str = "chat-controls-1";
const SPACE: &str = "space-controls-1";

/// One crossing of the executor boundary.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Handed {
    chat_id: String,
    sandbox: SandboxLevel,
}

/// Records what every run it is handed was told, then completes.
struct RecordingHarness {
    seen: Arc<Mutex<Vec<Handed>>>,
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
        request: RunRequest,
        controls: RunControls,
    ) -> Result<BoxStream<'static, Result<AgentEvent, HarnessError>>, HarnessError> {
        self.seen.lock().unwrap().push(Handed {
            chat_id: controls.chat_id.clone(),
            sandbox: request.sandbox,
        });
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
    let tmp = tempfile::tempdir().expect("tempdir");
    let cwd = tmp.path().join("checkout");
    std::fs::create_dir_all(&cwd).expect("checkout dir");

    let seen = Arc::new(Mutex::new(Vec::new()));
    let registry = HarnessRegistry::new();
    registry.register(Arc::new(RecordingHarness { seen: seen.clone() }));
    let core = EngineCore::assemble(
        &tmp.path().join("data"),
        Arc::new(registry),
        HarnessId::Mock,
        None,
    )
    .expect("engine core assembles");

    // A real workspace row, so the titling path runs instead of bailing on a
    // missing chat — the second boundary crossing has to be reachable for the
    // assertion below to mean anything.
    core.workspace
        .create_space(SPACE, &core.device_id, &cwd.to_string_lossy(), None, true)
        .expect("create space");
    core.workspace
        .create_chat(
            CHAT,
            Some(SPACE),
            None,
            None,
            Some(cwd.to_string_lossy().into_owned()),
        )
        .expect("create chat");

    core.sessions
        .dispatch(
            CHAT,
            HarnessId::Mock,
            RunRequest {
                prompt: "hello".into(),
                harness: None,
                model: None,
                reasoning: None,
                model_options: serde_json::Map::new(),
                cwd: cwd.to_string_lossy().into_owned(),
                // Distinct from the titling run's read-only sandbox.
                sandbox: SandboxLevel::WorkspaceWrite,
                auto_approve: true,
                enable_workers_mcp: false,
                workers_parent_chat_id: None,
                attachments: Vec::new(),
                worktree: None,
                resume: None,
            },
            None,
        )
        .await
        .expect("dispatch");

    let deadline = tokio::time::Instant::now() + Duration::from_secs(20);
    let recorded = loop {
        let recorded = seen.lock().unwrap().clone();
        let has_turn = recorded
            .iter()
            .any(|h| h.sandbox == SandboxLevel::WorkspaceWrite);
        let has_titling = recorded.iter().any(|h| h.sandbox == SandboxLevel::ReadOnly);
        if has_turn && has_titling {
            break recorded;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "both dispatch paths must reach the harness; saw {recorded:?}"
        );
        tokio::time::sleep(Duration::from_millis(15)).await;
    };

    // Every crossing carries the dispatching chat, not just the first.
    for handed in &recorded {
        assert_eq!(
            handed.chat_id, CHAT,
            "a run crossed the executor boundary without its chat id: {handed:?} \
             (all: {recorded:?})"
        );
    }
}
