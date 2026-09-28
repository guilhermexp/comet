//! SpawnChat creates a child native chat from the parent row and the engine
//! stamps `RunRequest.sessions` only for human-originated orchestrator runs
//! that have a bound IPC endpoint.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use futures::StreamExt;
use futures::stream::BoxStream;
use zeron_doc::{
    MessagePart, MessageRole, MessageStatus, SessionCommandPayload, SessionMessageEntry,
};
use zeron_engine::{EngineCore, HarnessRegistry};
use zeron_harness::{Harness, HarnessError, RunControls};
use zeron_proto::{
    AgentEvent, ChatConfig, DoneStatus, HarnessId, Model, ReasoningLevel, RunRequest, SandboxLevel,
    SessionsGrant, SteeringMode,
};

type RequestLog = Arc<Mutex<Vec<(String, RunRequest)>>>;

struct RecordingHarness {
    requests: RequestLog,
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
        false
    }
    fn steering_mode(&self) -> SteeringMode {
        SteeringMode::TurnBoundary
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
        self.requests
            .lock()
            .expect("request log")
            .push((controls.chat_id, request.clone()));
        let events: Vec<Result<AgentEvent, HarnessError>> = vec![
            Ok(AgentEvent::SessionStarted {
                harness: HarnessId::Mock,
                model: request.model.clone().unwrap_or_else(|| "mock-1".into()),
                tools: vec![],
                cwd: request.cwd.clone(),
                session_id: "sess-spawn".into(),
                assistant_message_id: "a-1".into(),
            }),
            Ok(AgentEvent::TextDelta {
                text: format!("ack: {}", request.prompt),
            }),
            Ok(AgentEvent::Done {
                status: DoneStatus::Completed,
                result: None,
                error: None,
                session_id: Some("sess-spawn".into()),
            }),
        ];
        Ok(futures::stream::iter(events).boxed())
    }
}

fn parent_config() -> ChatConfig {
    ChatConfig {
        harness: HarnessId::Mock,
        model: Some("switched-model".into()),
        reasoning: Some(ReasoningLevel::High),
        model_options: serde_json::json!({"effort": "max"})
            .as_object()
            .unwrap()
            .clone(),
        sandbox: SandboxLevel::ReadOnly,
    }
}

fn run_request(prompt: &str, cwd: &str) -> RunRequest {
    RunRequest {
        prompt: prompt.into(),
        harness: Some(HarnessId::Mock),
        model: Some("switched-model".into()),
        reasoning: Some(ReasoningLevel::High),
        model_options: serde_json::json!({"effort": "max"})
            .as_object()
            .unwrap()
            .clone(),
        cwd: cwd.into(),
        sandbox: SandboxLevel::ReadOnly,
        auto_approve: true,
        enable_workers_mcp: true,
        workers_parent_chat_id: None,
        sessions: None,
        attachments: Vec::new(),
        worktree: None,
        resume: None,
        mcp: None,
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

fn requests_for(log: &RequestLog, chat_id: &str) -> Vec<RunRequest> {
    log.lock()
        .expect("request log")
        .iter()
        .filter(|(id, _)| id == chat_id)
        .map(|(_, request)| request.clone())
        .collect()
}

fn user_text(core: &EngineCore, chat_id: &str) -> Vec<String> {
    let entries: Vec<SessionMessageEntry> = core
        .doc_host
        .open(chat_id)
        .ok()
        .and_then(|handle| handle.doc().read_entries().ok())
        .unwrap_or_default();
    entries
        .into_iter()
        .filter(|entry| entry.role == MessageRole::User)
        .flat_map(|entry| entry.parts)
        .filter_map(|part| match part {
            MessagePart::Text { text, .. } => Some(text),
            _ => None,
        })
        .collect()
}

fn assistant_complete(core: &EngineCore, chat_id: &str) -> bool {
    let entries: Vec<SessionMessageEntry> = core
        .doc_host
        .open(chat_id)
        .ok()
        .and_then(|handle| handle.doc().read_entries().ok())
        .unwrap_or_default();
    entries.iter().any(|entry| {
        entry.role == MessageRole::Assistant && entry.status == Some(MessageStatus::Complete)
    })
}

fn assemble() -> (tempfile::TempDir, EngineCore, RequestLog) {
    let tmp = tempfile::tempdir().unwrap();
    let requests = RequestLog::default();
    let registry = HarnessRegistry::new();
    registry.register(Arc::new(RecordingHarness {
        requests: requests.clone(),
    }));
    let core = EngineCore::assemble(
        &tmp.path().join("data"),
        Arc::new(registry),
        HarnessId::Mock,
        None,
    )
    .expect("engine core assembles");
    (tmp, core, requests)
}

async fn spawn(
    client: &zeron_rpc::RpcClient,
    parent: &str,
    prompt: &str,
    space_id: Option<&str>,
) -> Result<serde_json::Value, zeron_rpc::RpcError> {
    let mut params = serde_json::json!({
        "parentChatId": parent,
        "prompt": prompt,
    });
    if let Some(space_id) = space_id {
        params["spaceId"] = serde_json::Value::String(space_id.into());
    }
    client.call(zeron_rpc::methods::SPAWN_CHAT, params).await
}

#[tokio::test(flavor = "multi_thread")]
async fn spawn_chat_inherits_config_and_stamps_sessions_only_on_the_parent() {
    let (_tmp, core, log) = assemble();
    core.note_local_ipc("ws://127.0.0.1:43111");
    core.workspace
        .create_space(
            "space-parent",
            &core.device_id,
            "/tmp/parent-space",
            Some("Parent".into()),
            false,
        )
        .unwrap();
    core.workspace
        .create_space(
            "space-other",
            &core.device_id,
            "/tmp/other-space",
            Some("Other".into()),
            false,
        )
        .unwrap();
    core.workspace
        .create_chat(
            "parent",
            Some("space-parent"),
            Some(&core.device_id),
            Some(ChatConfig {
                harness: HarnessId::Mock,
                model: Some("original-model".into()),
                reasoning: Some(ReasoningLevel::Low),
                model_options: Default::default(),
                sandbox: SandboxLevel::WorkspaceWrite,
            }),
            None,
        )
        .unwrap();
    core.workspace
        .set_chat_config("parent", &parent_config())
        .unwrap();
    core.workspace.rename_chat("parent", "Parent").unwrap();

    let client = zeron_rpc::memory_client(core.rpc_service());
    let mut chats = client
        .subscribe(zeron_rpc::methods::WATCH_CHATS, serde_json::Value::Null)
        .await
        .unwrap();
    let _ = tokio::time::timeout(Duration::from_secs(5), chats.recv())
        .await
        .unwrap();

    core.doc_host
        .queue_command(
            "parent",
            SessionCommandPayload::Run {
                request: run_request("parent turn", "/tmp/parent-space"),
                message_id: "parent-msg".into(),
            },
        )
        .unwrap();
    wait_for(|| requests_for(&log, "parent").len() == 1, "parent run").await;
    let parent_run = &requests_for(&log, "parent")[0];
    let grant = parent_run
        .sessions
        .clone()
        .expect("human chat receives sessions");
    assert_eq!(grant.parent_chat_id, "parent");
    assert_eq!(grant.endpoint, "ws://127.0.0.1:43111");
    assert_eq!(grant.engine_id, core.device_id);
    assert_eq!(parent_run.workers_parent_chat_id.as_deref(), Some("parent"));

    let created = spawn(&client, "parent", "child prompt", None)
        .await
        .expect("spawn in parent space");
    let child_id = created["chatId"].as_str().unwrap().to_owned();
    wait_for(
        || {
            core.workspace
                .chat(&child_id)
                .ok()
                .flatten()
                .is_some_and(|chat| chat.origin_chat_id.as_deref() == Some("parent"))
        },
        "watch child origin",
    )
    .await;
    let mut saw_origin = false;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    while tokio::time::Instant::now() < deadline {
        if let Ok(Some(frame)) =
            tokio::time::timeout(Duration::from_millis(200), chats.recv()).await
        {
            let rows: Vec<zeron_proto::Chat> = serde_json::from_value(frame).unwrap_or_default();
            if rows
                .iter()
                .any(|chat| chat.id == child_id && chat.origin_chat_id.as_deref() == Some("parent"))
            {
                saw_origin = true;
                break;
            }
        }
    }
    assert!(saw_origin, "WatchChats emits the child with origin");

    let child = core.workspace.chat(&child_id).unwrap().unwrap();
    assert_eq!(child.space_id.as_deref(), Some("space-parent"));
    assert_eq!(child.cwd.as_deref(), Some("/tmp/parent-space"));
    assert_eq!(child.config.as_ref(), Some(&parent_config()));
    wait_for(|| assistant_complete(&core, &child_id), "child first run").await;
    assert_eq!(user_text(&core, &child_id), vec!["child prompt".to_owned()]);
    let first = &requests_for(&log, &child_id)[0];
    assert!(
        first.sessions.is_none(),
        "child first run has no sessions grant"
    );
    assert!(first.enable_workers_mcp);
    assert_eq!(
        first.workers_parent_chat_id.as_deref(),
        Some(child_id.as_str())
    );
    assert_eq!(first.model.as_deref(), Some("switched-model"));
    assert_eq!(first.reasoning, Some(ReasoningLevel::High));
    assert_eq!(first.sandbox, SandboxLevel::ReadOnly);
    assert_eq!(first.harness, Some(HarnessId::Mock));

    core.doc_host
        .queue_command(
            &child_id,
            SessionCommandPayload::Run {
                request: RunRequest {
                    sessions: Some(SessionsGrant {
                        parent_chat_id: "forged".into(),
                        endpoint: "ws://127.0.0.1:1".into(),
                        engine_id: "forged-engine".into(),
                    }),
                    ..run_request("later turn", "/tmp/parent-space")
                },
                message_id: "child-later".into(),
            },
        )
        .unwrap();
    wait_for(
        || requests_for(&log, &child_id).len() >= 2,
        "child later run",
    )
    .await;
    let later = &requests_for(&log, &child_id)[1];
    assert!(
        later.sessions.is_none(),
        "forged sessions grant is overwritten"
    );
    assert_eq!(
        later.workers_parent_chat_id.as_deref(),
        Some(child_id.as_str())
    );

    let before = core.workspace.read_chats().unwrap().len();
    let unknown = spawn(&client, "parent", "nope", Some("missing-space"))
        .await
        .expect_err("unknown space");
    assert!(unknown.to_string().contains("no such space"));
    assert_eq!(core.workspace.read_chats().unwrap().len(), before);

    let other = spawn(&client, "parent", "other space", Some("space-other"))
        .await
        .unwrap();
    let other_id = other["chatId"].as_str().unwrap();
    let other_chat = core.workspace.chat(other_id).unwrap().unwrap();
    assert_eq!(other_chat.space_id.as_deref(), Some("space-other"));
    assert_eq!(other_chat.cwd.as_deref(), Some("/tmp/other-space"));
    assert_eq!(other["spaceId"], "space-other");

    core.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn projectless_parent_spawns_a_projectless_child_and_no_endpoint_drops_sessions() {
    let (_tmp, core, log) = assemble();
    core.workspace
        .create_chat(
            "loose",
            None,
            Some(&core.device_id),
            Some(parent_config()),
            None,
        )
        .unwrap();
    core.workspace.rename_chat("loose", "Loose").unwrap();
    core.doc_host
        .queue_command(
            "loose",
            SessionCommandPayload::Run {
                request: run_request("no endpoint", "~"),
                message_id: "loose-msg".into(),
            },
        )
        .unwrap();
    wait_for(|| requests_for(&log, "loose").len() == 1, "projectless run").await;
    assert!(requests_for(&log, "loose")[0].sessions.is_none());

    core.note_local_ipc("ws://127.0.0.1:43112");
    let client = zeron_rpc::memory_client(core.rpc_service());
    let created = spawn(&client, "loose", "projectless child", None)
        .await
        .unwrap();
    let child_id = created["chatId"].as_str().unwrap();
    assert!(created.get("spaceId").is_none() || created["spaceId"].is_null());
    assert_eq!(created["deviceId"], core.device_id);
    let child = core.workspace.chat(child_id).unwrap().unwrap();
    assert_eq!(child.space_id, None);
    assert_eq!(child.device_id, core.device_id);
    assert_eq!(child.cwd.as_deref(), Some("~"));
    assert_eq!(child.origin_chat_id.as_deref(), Some("loose"));
    core.shutdown().await;
}
