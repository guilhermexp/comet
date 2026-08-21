use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;

use futures::{StreamExt as _, stream::BoxStream};
use serde_json::{Value, json};
use zeron_harness::omp::normalize::{AgentEndDisposition, OmpNormalizer};
use zeron_harness::omp::process::{OmpLaunch, OmpProcess};
use zeron_harness::omp::protocol::{MAX_INBOUND_BYTES, parse_frame, sanitize_diagnostic};
use zeron_harness::omp::workers_bridge::{WorkersBridge, WorkersBridgeOptions};
use zeron_harness::omp::{discover_commands_with_launch, discover_models_with_launch};
use zeron_harness::{
    CancellationToken, Harness, HarnessError, OmpHarness, RunControls, SteerMessage,
};
use zeron_proto::{
    AgentEvent, DoneStatus, HarnessId, ReasoningLevel, RunRequest, SandboxLevel, ToolCall,
    UserInputAnswer,
};

fn fixture_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/fake-omp-rpc.sh")
}

fn fake_workers_controller_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/fake-workers-controller-mcp.sh")
}

fn fake_env(scenario: &str) -> HashMap<String, String> {
    HashMap::from([("FAKE_OMP_SCENARIO".to_owned(), scenario.to_owned())])
}

async fn start_fake(scenario: &str) -> (OmpProcess, tokio::sync::mpsc::Receiver<Value>) {
    let process = OmpProcess::start(fake_launch(scenario)).await.unwrap();
    let events = process.take_events().unwrap();
    (process, events)
}

fn fake_launch(scenario: &str) -> OmpLaunch {
    OmpLaunch {
        executable: fixture_path(),
        cwd: std::env::current_dir().unwrap(),
        ephemeral: true,
        env: Some(fake_env(scenario)),
        handshake_timeout: Duration::from_secs(1),
        request_timeout: Duration::from_secs(1),
    }
}

fn fake_harness(scenario: &str) -> OmpHarness {
    OmpHarness::new()
        .with_executable(fixture_path())
        .with_env(fake_env(scenario))
        .with_workers_mcp_executable(fake_workers_controller_path())
        .with_timeouts(Duration::from_secs(1), Duration::from_secs(1))
}

fn request(prompt: &str) -> RunRequest {
    RunRequest {
        prompt: prompt.to_owned(),
        harness: Some(HarnessId::Omp),
        model: None,
        reasoning: None,
        model_options: serde_json::Map::new(),
        cwd: std::env::current_dir()
            .unwrap()
            .to_string_lossy()
            .into_owned(),
        sandbox: SandboxLevel::WorkspaceWrite,
        auto_approve: false,
        enable_workers_mcp: false,
        workers_parent_chat_id: None,
        resume: None,
        attachments: Vec::new(),
        worktree: None,
    }
}

fn controls_with_answer(
    label: &'static str,
) -> (
    RunControls,
    tokio::sync::mpsc::Sender<SteerMessage>,
    CancellationToken,
) {
    let (steer_tx, steer_rx) = tokio::sync::mpsc::channel(8);
    let interrupt = CancellationToken::new();
    let controls = RunControls {
        request_input: Box::new(move |questions| {
            let (tx, rx) = tokio::sync::oneshot::channel();
            let answers = questions
                .into_iter()
                .map(|question| UserInputAnswer {
                    question_id: question.id,
                    labels: vec![label.to_owned()],
                })
                .collect();
            let _ = tx.send(answers);
            rx
        }),
        steering: steer_rx,
        interrupt: interrupt.clone(),
    };
    (controls, steer_tx, interrupt)
}

async fn collect_until_done(
    stream: &mut BoxStream<'static, Result<AgentEvent, HarnessError>>,
) -> Vec<AgentEvent> {
    let mut events = Vec::new();
    while let Some(event) = stream.next().await {
        let event = event.unwrap();
        let done = matches!(event, AgentEvent::Done { .. });
        events.push(event);
        if done {
            break;
        }
    }
    events
}

#[test]
fn protocol_bounds_and_redacts_frames() {
    let ready = parse_frame(r#"{"type":"ready"}"#).unwrap();
    assert_eq!(ready["type"], "ready");
    assert!(parse_frame("not-json").is_err());
    assert!(parse_frame(&"x".repeat(MAX_INBOUND_BYTES + 1)).is_err());
    assert_eq!(
        sanitize_diagnostic("Authorization: Bearer token-secret-123"),
        "Authorization=[redacted]"
    );
}

#[tokio::test]
async fn process_correlates_out_of_order_responses() {
    let (process, mut events) = start_fake("out-of-order").await;
    let first = process.request(json!({ "type": "get_state" }));
    let second = process.request(json!({ "type": "get_available_models" }));
    let (state, models) = tokio::join!(first, second);
    assert_eq!(state.unwrap()["sessionId"], "s-1");
    assert_eq!(models.unwrap()["models"][0]["id"], "gpt-5.6-sol");
    assert_eq!(
        events.recv().await.unwrap()["type"],
        "available_commands_update"
    );
    process.shutdown().await.unwrap();
}

#[tokio::test]
async fn process_rejects_exit_before_ready() {
    let error = OmpProcess::start(fake_launch("early-exit"))
        .await
        .unwrap_err();
    assert!(error.to_string().contains("before ready"));
}

#[tokio::test]
async fn catalog_preserves_provider_identity_and_current_model() {
    let models = discover_models_with_launch(fake_launch("catalog"))
        .await
        .unwrap();
    assert_eq!(models[0].id, "openai-codex/gpt-5.6-sol");
    assert_eq!(models[0].label, "openai-codex/GPT-5.6 Sol");
    assert!(models[0].reasoning_levels.contains(&ReasoningLevel::Max));
    assert!(models.iter().any(|model| model.id == "anthropic/shared"));
    assert!(models.iter().any(|model| model.id == "openai-codex/shared"));
}

#[tokio::test]
async fn commands_are_discovered_from_the_rpc_runtime() {
    let commands = discover_commands_with_launch(fake_launch("catalog"))
        .await
        .unwrap();
    assert_eq!(commands[0].name, "model");
    assert_eq!(commands[0].input_hint.as_deref(), Some("provider/model"));
}

#[test]
fn normalizer_maps_text_reasoning_and_tools() {
    let mut normalizer = OmpNormalizer::new("/repo", "openai-codex/gpt-5.6-sol");
    assert_eq!(
        normalizer.push(json!({
            "type": "message_update",
            "assistantMessageEvent": { "type": "text_delta", "delta": "hello" }
        })),
        vec![AgentEvent::TextDelta {
            text: "hello".into()
        }]
    );
    assert_eq!(
        normalizer.push(json!({
            "type": "message_update",
            "assistantMessageEvent": { "type": "thinking_delta", "delta": "checking" }
        })),
        vec![AgentEvent::ReasoningDelta {
            text: "checking".into()
        }]
    );
    assert_eq!(
        normalizer.push(json!({
            "type": "tool_execution_start",
            "toolCallId": "tool-1",
            "toolName": "bash",
            "args": { "command": "cargo test" }
        })),
        vec![AgentEvent::ToolCall {
            id: "tool-1".into(),
            call: ToolCall::Exec {
                command: "cargo test".into()
            }
        }]
    );
    assert_eq!(
        normalizer.push(json!({
            "type": "tool_execution_end",
            "toolCallId": "tool-1",
            "toolName": "bash",
            "result": { "content": [{ "type": "text", "text": "ok" }] },
            "isError": false
        })),
        vec![AgentEvent::ToolResult {
            id: "tool-1".into(),
            is_error: false,
            output: Some("ok".into()),
            diff: None
        }]
    );
}

#[test]
fn normalizer_attributes_subagent_lifecycle() {
    let mut normalizer = OmpNormalizer::new("/repo", "openai-codex/gpt-5.6-sol");
    let started = normalizer.push(json!({
        "type": "subagent_lifecycle",
        "payload": {
            "id": "child-1",
            "parentToolCallId": "task-1",
            "status": "running",
            "agent": "explore",
            "sessionFile": "/tmp/child.jsonl"
        }
    }));
    assert!(matches!(
        started.as_slice(),
        [AgentEvent::Subagent { parent_tool_use_id, event }]
            if parent_tool_use_id == "task-1"
                && matches!(event.as_ref(), AgentEvent::SessionStarted { session_id, .. } if session_id == "/tmp/child.jsonl")
    ));
    assert_eq!(normalizer.active_subagents(), 1);

    let finished = normalizer.push(json!({
        "type": "subagent_lifecycle",
        "payload": {
            "id": "child-1",
            "parentToolCallId": "task-1",
            "status": "completed"
        }
    }));
    assert!(matches!(
        finished.as_slice(),
        [AgentEvent::Subagent { event, .. }]
            if matches!(event.as_ref(), AgentEvent::Done { status: DoneStatus::Completed, .. })
    ));
    assert_eq!(normalizer.active_subagents(), 0);
}

#[test]
fn normalizer_classifies_only_terminal_agent_ends() {
    let mut normalizer = OmpNormalizer::new("/repo", "openai-codex/gpt-5.6-sol");
    assert_eq!(
        normalizer.classify_agent_end(&json!({ "type": "agent_end", "isTerminal": false })),
        AgentEndDisposition::Continue
    );
    assert_eq!(
        normalizer.classify_agent_end(&json!({
            "type": "agent_end",
            "messages": [{ "role": "assistant", "stopReason": "error", "errorMessage": "provider failed" }]
        })),
        AgentEndDisposition::Error("provider failed".into())
    );
    assert_eq!(
        normalizer.classify_agent_end(&json!({ "type": "agent_end", "messages": [] })),
        AgentEndDisposition::Complete
    );
}

#[tokio::test]
async fn workers_host_tool_is_registered_only_when_enabled() {
    let disabled = WorkersBridge::start(WorkersBridgeOptions {
        enabled: false,
        executable: fake_workers_controller_path(),
        parent_chat_id: Some("chat-1".into()),
    })
    .await
    .unwrap();
    assert!(disabled.is_none());

    let enabled = WorkersBridge::start(WorkersBridgeOptions {
        enabled: true,
        executable: fake_workers_controller_path(),
        parent_chat_id: Some("chat-1".into()),
    })
    .await
    .unwrap()
    .unwrap();
    assert_eq!(enabled.definition()["name"], "workers");
    let result = enabled
        .handle_call("omp-call-1", "workers", json!({ "action": "help" }))
        .await;
    assert_eq!(result["type"], "host_tool_result");
    assert_eq!(result["id"], "omp-call-1");
    assert_eq!(result["isError"], false);
    assert_eq!(result["result"]["content"][0]["text"], "worker help");
    enabled.shutdown().await.unwrap();
}

#[tokio::test]
async fn run_streams_resumes_steers_answers_and_completes_once() {
    let harness = fake_harness("full-run");
    let (controls, steer, _interrupt) = controls_with_answer("Yes");
    let mut request = request("hello");
    request.model = Some("openai-codex/gpt-5.6-sol".into());
    request.reasoning = Some(ReasoningLevel::High);
    request.resume = Some("/tmp/omp-session.jsonl".into());
    request.enable_workers_mcp = true;
    request.workers_parent_chat_id = Some("chat-1".into());

    let mut stream = harness.run(request, controls).await.unwrap();
    steer
        .send(SteerMessage {
            prompt: "next".into(),
            message_id: Some("m2".into()),
        })
        .await
        .unwrap();
    let events = collect_until_done(&mut stream).await;

    assert_eq!(
        events
            .iter()
            .filter(|event| matches!(event, AgentEvent::Done { .. }))
            .count(),
        1
    );
    assert!(events.iter().any(|event| matches!(
        event,
        AgentEvent::SessionStarted {
            harness: HarnessId::Omp,
            ..
        }
    )));
    assert!(events.iter().any(|event| matches!(
        event,
        AgentEvent::TextDelta { text } if text == " after steer"
    )));
    assert!(
        events
            .iter()
            .any(|event| matches!(event, AgentEvent::Steered { .. }))
    );
    assert!(events.iter().any(|event| matches!(
        event,
        AgentEvent::ToolCall { call: ToolCall::Exec { command }, .. } if command == "cargo test"
    )));
}

#[tokio::test]
async fn run_reports_provider_error_honestly() {
    let harness = fake_harness("provider-error");
    let (controls, _steer, _interrupt) = controls_with_answer("Yes");
    let mut request = request("fail");
    request.model = Some("openai-codex/gpt-5.6-sol".into());
    request.reasoning = Some(ReasoningLevel::High);
    let mut stream = harness.run(request, controls).await.unwrap();
    let events = collect_until_done(&mut stream).await;
    assert!(events.iter().any(|event| matches!(
        event,
        AgentEvent::Done { status: DoneStatus::Errored, error: Some(error), .. }
            if error == "provider failed"
    )));
}

#[tokio::test]
async fn run_interrupts_a_waiting_rpc_turn() {
    let harness = fake_harness("wait");
    let (controls, _steer, interrupt) = controls_with_answer("Yes");
    let mut request = request("wait");
    request.model = Some("openai-codex/gpt-5.6-sol".into());
    request.reasoning = Some(ReasoningLevel::High);
    let mut stream = harness.run(request, controls).await.unwrap();
    interrupt.cancel();
    let events = collect_until_done(&mut stream).await;
    assert!(events.iter().any(|event| matches!(
        event,
        AgentEvent::Done {
            status: DoneStatus::Interrupted,
            ..
        }
    )));
}
