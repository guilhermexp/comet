use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use futures::{StreamExt as _, stream::BoxStream};
use serde_json::{Value, json};
use zeron_harness::omp::normalize::{AgentEndDisposition, OmpNormalizer};
use zeron_harness::omp::process::{OmpLaunch, OmpProcess};
use zeron_harness::omp::protocol::{
    MAX_INBOUND_BYTES, MAX_OUTBOUND_BYTES, parse_frame, sanitize_diagnostic,
};
use zeron_harness::omp::workers_bridge::{WorkersBridge, WorkersBridgeOptions};
use zeron_harness::omp::{discover_commands_with_launch, discover_models_with_launch};
use zeron_harness::{
    CancellationToken, Harness, HarnessError, OmpHarness, RunControls, SteerMessage,
};
use zeron_proto::{
    AgentEvent, DoneStatus, HarnessId, ReasoningLevel, RunRequest, SandboxLevel, ToolCall,
    ToolDiff, UserInputAnswer,
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

fn controls_with_pending_answer() -> (
    RunControls,
    tokio::sync::mpsc::Sender<SteerMessage>,
    CancellationToken,
) {
    let (steer_tx, steer_rx) = tokio::sync::mpsc::channel(8);
    let interrupt = CancellationToken::new();
    let held = Arc::new(Mutex::new(Vec::new()));
    let controls = RunControls {
        request_input: Box::new(move |_questions| {
            let (tx, rx) = tokio::sync::oneshot::channel();
            held.lock().unwrap().push(tx);
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
    let json_secret = sanitize_diagnostic(r#"provider error: {"apiKey":"secret-value"}"#);
    assert!(!json_secret.contains("secret-value"), "{json_secret}");
    assert_eq!(
        sanitize_diagnostic("request failed with sk-proj_0123456789abcdef"),
        "request failed with [redacted]"
    );
    let pem = sanitize_diagnostic(
        "before -----BEGIN RSA PRIVATE KEY-----\nsecret-body\n-----END RSA PRIVATE KEY----- after",
    );
    assert_eq!(pem, "before [redacted private key] after");
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
async fn process_rejects_oversized_frame_before_waiting_for_newline() {
    let mut launch = fake_launch("oversized-no-newline");
    launch.request_timeout = Duration::from_secs(5);
    let process = OmpProcess::start(launch).await.unwrap();
    let result = tokio::time::timeout(
        Duration::from_secs(2),
        process.request(json!({ "type": "get_state" })),
    )
    .await
    .expect("reader must reject once the byte limit is crossed, without waiting for newline");
    let error = result.unwrap_err();
    assert!(error.to_string().contains("frame exceeded"), "{error}");
    process.shutdown().await.unwrap();
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
    assert_eq!(
        models
            .iter()
            .map(|model| model.id.as_str())
            .collect::<Vec<_>>(),
        vec![
            "openai-codex/gpt-5.6-sol",
            "anthropic/shared",
            "openai-codex/shared",
        ]
    );
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
    assert_eq!(
        normalizer.push(json!({
            "type": "tool_execution_end",
            "toolCallId": "tool-2",
            "toolName": "edit",
            "result": {
                "path": "src/main.rs",
                "oldText": "old",
                "newText": "new"
            },
            "isError": false
        })),
        vec![AgentEvent::ToolResult {
            id: "tool-2".into(),
            is_error: false,
            output: Some(r#"{"newText":"new","oldText":"old","path":"src/main.rs"}"#.into()),
            diff: Some(ToolDiff {
                path: "src/main.rs".into(),
                old_text: Some("old".into()),
                new_text: "new".into(),
            }),
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
fn normalizer_settles_aborted_subagent_as_interrupted() {
    let mut normalizer = OmpNormalizer::new("/repo", "openai-codex/gpt-5.6-sol");
    normalizer.push(json!({
        "type": "subagent_lifecycle",
        "payload": {
            "id": "child-1",
            "parentToolCallId": "task-1",
            "status": "running",
            "agent": "explore",
            "sessionFile": "/tmp/child.jsonl"
        }
    }));
    assert_eq!(
        normalizer.classify_agent_end(&json!({ "type": "agent_end", "messages": [] })),
        AgentEndDisposition::Continue
    );

    let settled = normalizer.push(json!({
        "type": "subagent_lifecycle",
        "payload": { "id": "child-1", "status": "aborted" }
    }));
    assert!(matches!(
        settled.as_slice(),
        [AgentEvent::Subagent { event, .. }]
            if matches!(event.as_ref(), AgentEvent::Done { status: DoneStatus::Interrupted, .. })
    ));
    assert_eq!(normalizer.active_subagents(), 0);
    assert_eq!(
        normalizer.classify_agent_end(&json!({ "type": "agent_end", "messages": [] })),
        AgentEndDisposition::Complete
    );
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
    assert_eq!(
        normalizer.classify_agent_end(&json!({
            "type": "agent_end",
            "messages": [{
                "role": "assistant",
                "stopReason": "error",
                "errorMessage": "provider rejected sk-proj_0123456789abcdef"
            }]
        })),
        AgentEndDisposition::Error("provider rejected [redacted]".into())
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
    assert_eq!(enabled.definition()["loadMode"], "essential");
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
async fn workers_bridge_rejects_duplicate_and_excess_pending_calls() {
    let bridge = WorkersBridge::start(WorkersBridgeOptions {
        enabled: true,
        executable: fake_workers_controller_path(),
        parent_chat_id: Some("chat-1".into()),
    })
    .await
    .unwrap()
    .unwrap();

    let first = bridge
        .begin_call("duplicate", "workers", json!({ "action": "hang" }))
        .unwrap();
    let duplicate = bridge
        .begin_call("duplicate", "workers", json!({ "action": "hang" }))
        .unwrap_err();
    assert_eq!(duplicate["isError"], true);
    assert!(
        tokio::time::timeout(Duration::from_secs(1), first)
            .await
            .unwrap()
            .is_err(),
        "duplicate id must invalidate the original call"
    );

    let mut pending = Vec::new();
    for index in 0..64 {
        pending.push(
            bridge
                .begin_call(
                    &format!("pending-{index}"),
                    "workers",
                    json!({ "action": "hang" }),
                )
                .unwrap(),
        );
    }
    let overflow = bridge
        .begin_call("pending-overflow", "workers", json!({ "action": "hang" }))
        .unwrap_err();
    assert_eq!(overflow["isError"], true);
    bridge.shutdown().await.unwrap();
    drop(pending);
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
async fn run_rejects_state_without_resume_identity() {
    let harness = fake_harness("missing-session");
    let (controls, _steer, _interrupt) = controls_with_answer("Yes");
    let error = match harness.run(request("hello"), controls).await {
        Ok(_) => panic!("OMP state without a session identity must be rejected"),
        Err(error) => error,
    };
    assert!(error.to_string().contains("session identity"), "{error}");
}

#[tokio::test]
async fn run_rejects_image_that_cannot_fit_the_outbound_rpc_frame() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("large.png");
    tokio::fs::write(&path, vec![0_u8; MAX_OUTBOUND_BYTES])
        .await
        .unwrap();
    let harness = fake_harness("normal");
    let (controls, _steer, _interrupt) = controls_with_answer("Yes");
    let mut run_request = request("with image");
    run_request.attachments = vec![path.to_string_lossy().into_owned()];
    let error = match harness.run(run_request, controls).await {
        Ok(_) => panic!("oversized OMP image must be rejected"),
        Err(error) => error,
    };
    assert!(error.to_string().contains("RPC frame budget"), "{error}");
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

#[tokio::test]
async fn remote_ui_cancel_does_not_block_following_events() {
    let harness = fake_harness("interactive-cancel");
    let (controls, _steer, _interrupt) = controls_with_pending_answer();
    let mut stream = harness.run(request("ask"), controls).await.unwrap();
    let events = tokio::time::timeout(Duration::from_secs(2), collect_until_done(&mut stream))
        .await
        .expect("remote UI cancellation must unblock the OMP event pump");
    assert!(events.iter().any(|event| matches!(
        event,
        AgentEvent::TextDelta { text } if text == "after cancel"
    )));
    assert!(events.iter().any(|event| matches!(
        event,
        AgentEvent::InputResolved { request_id } if request_id == "question-pending"
    )));
}

#[tokio::test]
async fn interactive_timeout_cancels_the_host_question_and_resumes_omp() {
    let harness = fake_harness("interactive-timeout");
    let (controls, _steer, _interrupt) = controls_with_pending_answer();
    let mut stream = harness.run(request("ask"), controls).await.unwrap();
    let events = tokio::time::timeout(Duration::from_secs(2), collect_until_done(&mut stream))
        .await
        .expect("OMP interactive timeout must resolve without a host answer");
    assert!(events.iter().any(|event| matches!(
        event,
        AgentEvent::TextDelta { text } if text == "after timeout"
    )));
    assert!(events.iter().any(|event| matches!(
        event,
        AgentEvent::InputResolved { request_id } if request_id == "question-timeout"
    )));
}

#[tokio::test]
async fn cancelled_workers_call_does_not_block_the_omp_event_pump() {
    let harness = fake_harness("workers-cancel");
    let (controls, _steer, _interrupt) = controls_with_answer("Yes");
    let mut run_request = request("workers");
    run_request.enable_workers_mcp = true;
    let mut stream = harness.run(run_request, controls).await.unwrap();
    let events = tokio::time::timeout(Duration::from_secs(2), collect_until_done(&mut stream))
        .await
        .expect("cancelled Workers call must not starve OMP events");
    assert!(events.iter().any(|event| matches!(
        event,
        AgentEvent::TextDelta { text } if text == "after workers cancel"
    )));
}

#[tokio::test]
async fn oversized_workers_result_returns_a_bounded_error_to_omp() {
    let harness = fake_harness("workers-oversized");
    let (controls, _steer, _interrupt) = controls_with_answer("Yes");
    let mut run_request = request("workers");
    run_request.enable_workers_mcp = true;
    let mut stream = harness.run(run_request, controls).await.unwrap();
    let events = tokio::time::timeout(Duration::from_secs(3), collect_until_done(&mut stream))
        .await
        .expect("oversized Workers output must return a bounded host-tool error");
    assert!(events.iter().any(|event| matches!(
        event,
        AgentEvent::TextDelta { text } if text == "after oversized workers result"
    )));
}

#[tokio::test]
async fn dropping_the_event_stream_terminates_the_omp_process() {
    let temp = tempfile::tempdir().unwrap();
    let pid_file = temp.path().join("omp.pid");
    let mut env = fake_env("wait");
    env.insert(
        "FAKE_OMP_PID_FILE".into(),
        pid_file.to_string_lossy().into_owned(),
    );
    let harness = OmpHarness::new()
        .with_executable(fixture_path())
        .with_env(env)
        .with_timeouts(Duration::from_secs(1), Duration::from_secs(1));
    let (controls, _steer, _interrupt) = controls_with_answer("Yes");
    let stream = harness.run(request("wait"), controls).await.unwrap();
    let pid: u32 = tokio::fs::read_to_string(&pid_file)
        .await
        .unwrap()
        .trim()
        .parse()
        .unwrap();
    drop(stream);

    let deadline = tokio::time::Instant::now() + Duration::from_secs(3);
    while tokio::time::Instant::now() < deadline {
        let alive = std::process::Command::new("kill")
            .args(["-0", &pid.to_string()])
            .status()
            .is_ok_and(|status| status.success());
        if !alive {
            return;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("OMP process {pid} survived after its event stream was dropped");
}

#[tokio::test]
#[ignore = "uses the installed authenticated OMP CLI and performs two minimal model turns"]
async fn real_omp_rpc_turn_and_resume_smoke() {
    let harness = OmpHarness::new();
    let models = harness.models().await.unwrap();
    let model = models
        .first()
        .expect("OMP must advertise one model")
        .id
        .clone();

    let (controls, _steer, _interrupt) = controls_with_answer("Yes");
    let mut first = request("Reply with exactly: OMP RPC OK");
    first.model = Some(model.clone());
    first.reasoning = Some(ReasoningLevel::High);
    let mut stream = harness.run(first, controls).await.unwrap();
    let first_events =
        tokio::time::timeout(Duration::from_secs(120), collect_until_done(&mut stream))
            .await
            .expect("OMP first turn timed out");
    let first_text: String = first_events
        .iter()
        .filter_map(|event| match event {
            AgentEvent::TextDelta { text } => Some(text.as_str()),
            _ => None,
        })
        .collect();
    assert!(
        first_text.contains("OMP RPC OK"),
        "first text: {first_text}"
    );
    let session_id = first_events
        .iter()
        .find_map(|event| match event {
            AgentEvent::Done {
                status: DoneStatus::Completed,
                session_id: Some(session_id),
                ..
            } => Some(session_id.clone()),
            _ => None,
        })
        .expect("OMP first turn must complete with a resume id");

    let (controls, _steer, _interrupt) = controls_with_answer("Yes");
    let mut follow_up = request("Reply with exactly: OMP RESUME OK");
    follow_up.model = Some(model);
    follow_up.reasoning = Some(ReasoningLevel::High);
    follow_up.resume = Some(session_id);
    let mut stream = harness.run(follow_up, controls).await.unwrap();
    let follow_events =
        tokio::time::timeout(Duration::from_secs(120), collect_until_done(&mut stream))
            .await
            .expect("OMP resumed turn timed out");
    let follow_text: String = follow_events
        .iter()
        .filter_map(|event| match event {
            AgentEvent::TextDelta { text } => Some(text.as_str()),
            _ => None,
        })
        .collect();
    assert!(
        follow_text.contains("OMP RESUME OK"),
        "follow-up text: {follow_text}"
    );
}
