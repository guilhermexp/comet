//! Retiring a stale duplicate device over the RPC surface: its projects and
//! their chats move onto the local device with ids intact, the device leaves
//! every listing (including the chat MCP), and every refusal changes nothing.
//! Folders are temporary directories created here.

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use chrono::{TimeZone, Utc};
use futures::StreamExt;
use futures::stream::BoxStream;
use serde_json::{Value, json};

use zeron_doc::{MessageRole, MessageStatus, SessionCommandPayload};
use zeron_engine::{EngineCore, HarnessRegistry};
use zeron_harness::{Harness, HarnessError, RunControls};
use zeron_proto::{
    AgentEvent, Device, DoneStatus, HarnessId, Model, ReasoningLevel, RunRequest, SandboxLevel,
    SteeringMode,
};
use zeron_rpc::methods;

const LEGACY: &str = "fdd7f43c-legacy";

/// Completes a one-line turn.
struct OneLinerHarness;

#[async_trait]
impl Harness for OneLinerHarness {
    fn id(&self) -> HarnessId {
        HarnessId::Mock
    }
    fn display_name(&self) -> &str {
        "OneLiner"
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
        _controls: RunControls,
    ) -> Result<BoxStream<'static, Result<AgentEvent, HarnessError>>, HarnessError> {
        let events: Vec<Result<AgentEvent, HarnessError>> = vec![
            Ok(AgentEvent::SessionStarted {
                harness: HarnessId::Mock,
                model: "mock-1".into(),
                tools: vec![],
                cwd: request.cwd.clone(),
                session_id: "sess-retire".into(),
                assistant_message_id: format!("a-{}", request.prompt.len()),
            }),
            Ok(AgentEvent::TextDelta {
                text: "ran here".into(),
            }),
            Ok(AgentEvent::Done {
                status: DoneStatus::Completed,
                result: None,
                error: None,
                session_id: Some("sess-retire".into()),
            }),
        ];
        Ok(futures::stream::iter(events).boxed())
    }
}

struct World {
    core: EngineCore,
    client: zeron_rpc::RpcClient,
    endpoint: String,
    local: String,
    _server: tokio::task::JoinHandle<()>,
    _dir: tempfile::TempDir,
}

impl World {
    async fn start() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let registry = HarnessRegistry::new();
        registry.register(Arc::new(OneLinerHarness));
        let core = EngineCore::assemble(
            &dir.path().join("data"),
            Arc::new(registry),
            HarnessId::Mock,
            None,
        )
        .unwrap();
        let client = zeron_rpc::memory_client(core.rpc_service());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let endpoint = format!("ws://{}", listener.local_addr().unwrap());
        let service: Arc<dyn zeron_rpc::RpcService> = core.rpc_service();
        let server = tokio::spawn(zeron_rpc::serve_ws_listener(listener, service));
        let local = core.device_id.clone();
        Self {
            core,
            client,
            endpoint,
            local,
            _server: server,
            _dir: dir,
        }
    }

    fn folder(&self, name: &str) -> String {
        let path = self._dir.path().join(name);
        std::fs::create_dir_all(&path).unwrap();
        std::fs::canonicalize(path)
            .unwrap()
            .to_string_lossy()
            .into_owned()
    }

    /// The legacy duplicate, last seen weeks ago.
    fn legacy_device(&self, last_seen_ms: i64) {
        self.core.workspace.upsert_device_row(&Device {
            id: LEGACY.into(),
            name: "MacBook Pro de Guilherme".into(),
            platform: "macos".into(),
            last_seen_at: Some(Utc.timestamp_millis_opt(last_seen_ms).unwrap()),
            created_at: None,
            version: None,
            cursor_sdk_version: None,
            capabilities: Vec::new(),
        });
    }

    async fn mutate(&self, params: Value) -> Result<Value, zeron_rpc::RpcError> {
        self.client.call(methods::MUTATE, params).await
    }

    async fn create_space(&self, id: &str, device: &str, path: &str) {
        self.mutate(json!({"op": "createSpace", "spaceId": id, "deviceId": device, "path": path}))
            .await
            .unwrap();
    }

    async fn run_turn(&self, chat_id: &str, cwd: &str, prompt: &str) {
        let before = self.completed_turns(chat_id);
        self.core
            .doc_host
            .queue_command(
                chat_id,
                SessionCommandPayload::Run {
                    request: RunRequest {
                        mcp: None,
                        prompt: prompt.into(),
                        harness: None,
                        model: None,
                        reasoning: None,
                        model_options: Default::default(),
                        cwd: cwd.into(),
                        sandbox: SandboxLevel::WorkspaceWrite,
                        auto_approve: true,
                        enable_workers_mcp: false,
                        workers_parent_chat_id: None,
                        sessions: None,
                        attachments: Vec::new(),
                        worktree: None,
                        resume: None,
                    },
                    message_id: format!("msg-{chat_id}-{prompt}"),
                },
            )
            .unwrap();
        let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
        while self.completed_turns(chat_id) <= before {
            assert!(
                tokio::time::Instant::now() < deadline,
                "turn in {chat_id} did not run on this device"
            );
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }

    fn completed_turns(&self, chat_id: &str) -> usize {
        self.core
            .doc_host
            .open(chat_id)
            .ok()
            .and_then(|handle| handle.doc().read_entries().ok())
            .unwrap_or_default()
            .iter()
            .filter(|entry| {
                entry.role == MessageRole::Assistant
                    && entry.status == Some(MessageStatus::Complete)
            })
            .count()
    }

    /// A chat that ran one turn here and is then recorded as hosted by the
    /// legacy device inside `space_id` — how the duplicate's chats look.
    async fn legacy_chat(&self, chat_id: &str, space_id: &str, cwd: &str, archived: bool) {
        self.mutate(
            json!({"op": "createChat", "chatId": chat_id, "deviceId": self.local, "cwd": cwd}),
        )
        .await
        .unwrap();
        self.core
            .workspace
            .rename_chat(chat_id, &format!("title {chat_id}"))
            .unwrap();
        self.run_turn(chat_id, cwd, "before").await;
        let mut chat = self.core.workspace.chat(chat_id).unwrap().unwrap();
        chat.space_id = Some(space_id.into());
        chat.device_id = LEGACY.into();
        chat.archived = archived;
        self.core.workspace.import_chat_row(&chat).unwrap();
    }

    /// Everything a refusal must leave untouched.
    fn registry_dump(&self) -> Value {
        json!({
            "devices": self.core.workspace.read_devices().unwrap(),
            "spaces": self.core.workspace.read_spaces().unwrap(),
            "chats": self.core.workspace.read_chats().unwrap(),
        })
    }

    async fn chat_mcp(&self, tool: &str) -> Value {
        let zeron = zeron_mcp::Zeron::new(self.endpoint.clone(), zeron_mcp::Origin::default());
        zeron_mcp::Tools::new(Arc::new(zeron))
            .call(tool, json!({}))
            .await
            .unwrap()
    }

    /// The devices watch once it shows exactly `expected` (it republishes
    /// after the registry write lands, not in the reply to it).
    async fn devices_become(&self, expected: &[String]) -> Vec<String> {
        let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
        loop {
            let ids = self.watched_device_ids().await;
            if ids == expected || tokio::time::Instant::now() >= deadline {
                return ids;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    }

    async fn watched_device_ids(&self) -> Vec<String> {
        let mut stream = self
            .client
            .subscribe_scoped(methods::WATCH_DEVICES, json!({}))
            .await
            .unwrap();
        let devices = tokio::time::timeout(Duration::from_secs(5), stream.recv())
            .await
            .unwrap()
            .unwrap();
        devices
            .as_array()
            .unwrap()
            .iter()
            .map(|device| device["id"].as_str().unwrap().to_owned())
            .collect()
    }
}

const LONG_AGO_MS: i64 = 1_789_128_000_000; // 2026-09-11

/// Scenarios "Retiring the legacy duplicate" and "Chats follow their project".
#[tokio::test(flavor = "multi_thread")]
async fn retiring_the_legacy_duplicate_moves_its_project_and_chats_here() {
    let world = World::start().await;
    world.legacy_device(LONG_AGO_MS);
    let folder = world.folder(".orchestrator");
    world.create_space("space-orch", LEGACY, &folder).await;
    world
        .legacy_chat("chat-live", "space-orch", &folder, false)
        .await;
    world
        .legacy_chat("chat-old", "space-orch", &folder, true)
        .await;
    assert!(
        world
            .watched_device_ids()
            .await
            .contains(&LEGACY.to_owned())
    );

    world
        .mutate(json!({"op": "retireDevice", "deviceId": LEGACY}))
        .await
        .expect("an offline duplicate retires");

    let local_only = vec![world.local.clone()];
    assert_eq!(world.devices_become(&local_only).await, local_only);
    let devices = world.chat_mcp("list_devices").await;
    assert!(
        devices["devices"]
            .as_array()
            .unwrap()
            .iter()
            .all(|device| device["id"] != LEGACY),
        "{devices}"
    );
    let projects = world.chat_mcp("list_projects").await;
    let project = projects["projects"]
        .as_array()
        .unwrap()
        .iter()
        .find(|project| project["id"] == "space-orch")
        .cloned()
        .expect("same project id");
    assert_eq!(project["deviceId"], world.local.as_str());
    assert_eq!(project["path"], folder.as_str());

    for (chat_id, archived) in [("chat-live", false), ("chat-old", true)] {
        let chat = world.core.workspace.chat(chat_id).unwrap().unwrap();
        assert_eq!(chat.device_id, world.local);
        assert_eq!(chat.space_id.as_deref(), Some("space-orch"));
        assert_eq!(chat.archived, archived);
        assert_eq!(chat.title, Some(format!("title {chat_id}")));
        assert_eq!(world.completed_turns(chat_id), 1, "transcript kept");
    }

    world.run_turn("chat-live", &folder, "after").await;
    assert_eq!(world.completed_turns("chat-live"), 2);
    let session = world
        .core
        .workspace
        .read_sessions()
        .unwrap()
        .into_iter()
        .find(|session| session.chat_id == "chat-live")
        .expect("session row");
    assert_eq!(session.device_id, world.local);
    world.core.shutdown().await;
}

/// Scenario "A retired device reconnects".
#[tokio::test(flavor = "multi_thread")]
async fn a_retired_device_that_reconnects_stays_retired() {
    let world = World::start().await;
    world.legacy_device(LONG_AGO_MS);
    let folder = world.folder("project");
    world.create_space("space-1", LEGACY, &folder).await;
    world
        .mutate(json!({"op": "retireDevice", "deviceId": LEGACY}))
        .await
        .unwrap();

    let local_only = vec![world.local.clone()];
    assert_eq!(world.devices_become(&local_only).await, local_only);
    // Its engine comes back and upserts its row with a fresh sighting.
    world.legacy_device(Utc::now().timestamp_millis());
    tokio::time::sleep(Duration::from_millis(300)).await;

    assert_eq!(world.watched_device_ids().await, local_only);
    let space = world.core.workspace.space("space-1").unwrap().unwrap();
    assert_eq!(space.device_id, world.local);
    world.core.shutdown().await;
}

/// Scenario "The local or an online device": the RPC rejects both, unchanged.
#[tokio::test(flavor = "multi_thread")]
async fn the_local_or_an_online_device_cannot_be_retired() {
    let world = World::start().await;
    world.legacy_device(Utc::now().timestamp_millis());
    let folder = world.folder("project");
    world.create_space("space-1", LEGACY, &folder).await;
    let before = world.registry_dump();

    let local = world
        .mutate(json!({"op": "retireDevice", "deviceId": world.local}))
        .await
        .unwrap_err();
    assert!(local.to_string().contains("local device"), "{local}");
    let online = world
        .mutate(json!({"op": "retireDevice", "deviceId": LEGACY}))
        .await
        .unwrap_err();
    assert!(online.to_string().contains("online"), "{online}");
    assert_eq!(world.registry_dump(), before);
    world.core.shutdown().await;
}

/// Scenario "A folder missing locally": refused naming the folder, unchanged.
#[tokio::test(flavor = "multi_thread")]
async fn retirement_is_refused_when_a_project_folder_is_missing_here() {
    let world = World::start().await;
    world.legacy_device(LONG_AGO_MS);
    let present = world.folder("present");
    world.create_space("space-present", LEGACY, &present).await;
    world
        .create_space("space-gone", LEGACY, "/nonexistent/renamed-away")
        .await;
    let before = world.registry_dump();

    let error = world
        .mutate(json!({"op": "retireDevice", "deviceId": LEGACY}))
        .await
        .unwrap_err();
    assert!(
        error.to_string().contains("/nonexistent/renamed-away"),
        "{error}"
    );
    assert_eq!(
        world.registry_dump(),
        before,
        "no project, chat or device changes"
    );
    world.core.shutdown().await;
}

/// Scenario "A folder that already has a local project": refused naming the
/// conflicting projects, unchanged.
#[tokio::test(flavor = "multi_thread")]
async fn retirement_is_refused_when_the_folder_already_has_a_local_project() {
    let world = World::start().await;
    world.legacy_device(LONG_AGO_MS);
    let folder = world.folder("orchestrator");
    world.create_space("space-legacy", LEGACY, &folder).await;
    let local = world.local.clone();
    world.create_space("space-local", &local, &folder).await;
    let before = world.registry_dump();

    let error = world
        .mutate(json!({"op": "retireDevice", "deviceId": LEGACY}))
        .await
        .unwrap_err();
    let message = error.to_string();
    assert!(message.contains("already registered here"), "{message}");
    assert!(message.contains(&folder), "{message}");
    assert_eq!(world.registry_dump(), before);
    world.core.shutdown().await;
}
