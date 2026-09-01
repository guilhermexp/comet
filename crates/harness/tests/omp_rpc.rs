use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use futures::{StreamExt as _, stream::BoxStream};
use serde_json::{Value, json};
use zeron_harness::omp::normalize::{AgentEndDisposition, OmpNormalizer};
use zeron_harness::omp::process::{OmpLaunch, OmpProcess};
use zeron_harness::omp::protocol::{
    ChunkAssembler, MAX_INBOUND_BYTES, MAX_OUTBOUND_BYTES, live_context_command,
    live_mute_command, live_start_command, live_stop_command, parse_frame, parse_live_event,
    sanitize_diagnostic,
};
use zeron_harness::omp::workers_bridge::{WorkersBridge, WorkersBridgeOptions};
use zeron_harness::omp::{discover_commands_with_launch, discover_models_with_launch};
use zeron_harness::{
    CancellationToken, Harness, HarnessError, LiveVoiceContextKind, LiveVoiceEvent, OmpHarness,
    RunControls, SteerMessage,
};
use zeron_proto::{
    AgentEvent, DoneStatus, HarnessId, LiveVoicePhase, LiveVoiceRole, ReasoningLevel, RunRequest,
    SandboxLevel, ToolCall, ToolDiff, UserInputAnswer,
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
        system_prompt_append: None,
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
        chat_id: String::new(),
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
        chat_id: String::new(),
    };
    (controls, steer_tx, interrupt)
}

/// Answers with NO labels — exactly what the question panel's Skip submits.
fn controls_declining() -> (
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
                    labels: Vec::new(),
                })
                .collect();
            let _ = tx.send(answers);
            rx
        }),
        steering: steer_rx,
        interrupt: interrupt.clone(),
        chat_id: String::new(),
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
async fn omp_live_protocol_retains_additive_capability() {
    let supported = OmpProcess::start(fake_launch("live-protocol")).await.unwrap();
    assert!(supported.capabilities().live_voice);
    supported.shutdown().await.unwrap();

    let unsupported = OmpProcess::start(fake_launch("no-live-capability"))
        .await
        .unwrap();
    assert!(!unsupported.capabilities().live_voice);
    unsupported.shutdown().await.unwrap();
}

#[test]
fn omp_live_protocol_parses_and_validates_transient_events() {
    assert_eq!(
        parse_live_event(&json!({"type":"live_phase","phase":"working"})).unwrap(),
        Some(LiveVoiceEvent::Phase(LiveVoicePhase::Working))
    );
    assert_eq!(
        parse_live_event(&json!({"type":"live_levels","input":-0.5,"output":1.5})).unwrap(),
        Some(LiveVoiceEvent::Levels {
            input: 0.0,
            output: 1.0,
        })
    );
    assert_eq!(
        parse_live_event(&json!({
            "type":"live_transcript",
            "role":"assistant",
            "turn":2,
            "text":"Done",
            "final":true
        }))
        .unwrap(),
        Some(LiveVoiceEvent::Transcript(
            zeron_proto::LiveVoiceTranscript {
                role: LiveVoiceRole::Assistant,
                turn: 2,
                text: "Done".into(),
                final_text: true,
            }
        ))
    );
    assert_eq!(
        parse_live_event(&json!({
            "type":"live_delegation_created",
            "delegationId":"del-1",
            "request":"Inspect auth"
        }))
        .unwrap(),
        Some(LiveVoiceEvent::Delegation {
            delegation_id: "del-1".into(),
            request: "Inspect auth".into(),
        })
    );
    let ended = parse_live_event(&json!({
        "type":"live_ended",
        "error":"Authorization: Bearer token-secret-123"
    }))
    .unwrap();
    assert!(matches!(
        ended,
        Some(LiveVoiceEvent::Ended { error: Some(error) })
            if error == "Authorization=[redacted]"
    ));
    assert_eq!(
        parse_live_event(&json!({"type":"future_additive_event","value":1})).unwrap(),
        None
    );

    for malformed in [
        json!({"type":"live_levels","input":"bad","output":0.5}),
        json!({"type":"live_transcript","role":"system","turn":1,"text":"bad","final":true}),
        json!({"type":"live_delegation_created","delegationId":"","request":"bad"}),
        json!({"type":"live_phase","phase":"future"}),
    ] {
        assert!(parse_live_event(&malformed).is_err(), "{malformed}");
    }
}

#[test]
fn omp_live_protocol_encodes_exact_commands() {
    assert_eq!(
        live_start_command(),
        json!({"type":"live_start","delegationMode":"host"})
    );
    assert_eq!(
        live_mute_command(true),
        json!({"type":"live_set_muted","muted":true})
    );
    assert_eq!(
        live_context_command("del-1", LiveVoiceContextKind::Final, "Fixed").unwrap(),
        json!({
            "type":"live_append_context",
            "delegationId":"del-1",
            "kind":"final",
            "text":"Fixed"
        })
    );
    assert_eq!(live_stop_command(), json!({"type":"live_stop"}));
    assert!(live_context_command("", LiveVoiceContextKind::Progress, "work").is_err());
    assert!(live_context_command("del-1", LiveVoiceContextKind::Progress, " ").is_err());
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

#[test]
fn a_broken_chunk_sequence_fails_instead_of_reassembling_garbage() {
    let chunk = |index: u64, data: &str| {
        json!({
            "type": "rpc_chunk",
            "chunkId": "rpc-1",
            "index": index,
            "count": 2,
            "byteLength": 4,
            "data": base64_of(data),
        })
    };

    let mut assembler = ChunkAssembler::default();
    assert!(assembler.push(chunk(0, "ab")).unwrap().is_none());
    // Um frame comum no meio da sequencia = pedaco perdido.
    let error = assembler
        .push(json!({ "type": "notice" }))
        .expect_err("a lost chunk must not pass as a whole frame");
    assert!(error.to_string().contains("interrupted"), "{error}");

    let mut assembler = ChunkAssembler::default();
    assert!(assembler.push(chunk(0, "ab")).unwrap().is_none());
    let error = assembler
        .push(chunk(0, "ab"))
        .expect_err("a repeated index must not pass");
    assert!(error.to_string().contains("mismatch"), "{error}");

    // Uma sequencia que nao comeca no zero nao tem inicio para remontar.
    let mut assembler = ChunkAssembler::default();
    let error = assembler.push(chunk(1, "cd")).expect_err("index 1 first");
    assert!(error.to_string().contains("index 0"), "{error}");

    // O caminho feliz continua fechando: dois pedacos, um frame.
    let mut assembler = ChunkAssembler::default();
    let frame = json!({ "type": "notice", "message": "hi" }).to_string();
    let (head, tail) = frame.split_at(frame.len() / 2);
    let split = |index: u64, part: &str| {
        json!({
            "type": "rpc_chunk",
            "chunkId": "rpc-2",
            "index": index,
            "count": 2,
            "byteLength": frame.len(),
            "data": base64_of(part),
        })
    };
    assert!(assembler.push(split(0, head)).unwrap().is_none());
    assert_eq!(
        assembler.push(split(1, tail)).unwrap().unwrap()["type"],
        "notice"
    );
}

fn base64_of(value: &str) -> String {
    use base64::Engine as _;
    base64::engine::general_purpose::STANDARD.encode(value)
}

#[tokio::test]
async fn a_catalog_split_across_chunks_is_reassembled() {
    // O catalogo real do OMP mede ~1,2 MiB em 550 linhas — acima do 1 MiB por
    // linha do filho. Sem remontar `rpc_chunk` a resposta chega degradada
    // ("RPC response exceeded the transport limit") e o picker fica sem lista.
    let models = discover_models_with_launch(fake_launch("chunked-models"))
        .await
        .unwrap();
    assert_eq!(
        models
            .iter()
            .map(|model| model.id.as_str())
            .collect::<Vec<_>>(),
        vec!["openai-codex/gpt-5.6-sol", "anthropic/shared"],
    );
}

#[tokio::test]
async fn a_child_that_refuses_the_chunked_protocol_still_starts() {
    // Uma negociacao recusada custa os frames grandes, nao a sessao.
    let models = discover_models_with_launch(fake_launch("refuses-chunked-frames"))
        .await
        .unwrap();
    assert!(models.iter().any(|model| model.id == "anthropic/shared"));
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
            "result": {
                "content": [{ "type": "text", "text": "ok" }],
                "details": { "exitCode": 0, "durationMs": 250 }
            },
            "isError": false
        })),
        vec![AgentEvent::ToolResult {
            id: "tool-1".into(),
            is_error: false,
            output: Some("ok".into()),
            diff: None,
            execution: Some(zeron_proto::ToolExecutionMeta {
                exit_code: Some(0),
                duration_ms: Some(250),
            }),
        }]
    );
    let edited = normalizer.push(json!({
        "type": "tool_execution_end",
        "toolCallId": "tool-2",
        "toolName": "edit",
        "result": {
            "path": "src/main.rs",
            "oldText": "old",
            "newText": "new"
        },
        "isError": false
    }));
    let [
        AgentEvent::ToolResult {
            id,
            is_error,
            output,
            diff,
            execution,
        },
    ] = &edited[..]
    else {
        panic!("one tool result expected, got {edited:?}");
    };
    assert_eq!(id, "tool-2");
    assert!(!is_error);
    // `output` is the raw result object re-serialized, so its key ORDER follows
    // whichever map serde_json was built with: sorted with the default BTreeMap,
    // insertion order once anything in the graph turns on `preserve_order` (the
    // workspace does, so the shipped app runs the second one). Compare the
    // parsed value — the contract is the payload, never the byte order.
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(output.as_deref().expect("output"))
            .expect("output is json"),
        json!({ "path": "src/main.rs", "oldText": "old", "newText": "new" })
    );
    assert_eq!(
        diff.as_ref().expect("diff"),
        &ToolDiff {
            path: "src/main.rs".into(),
            old_text: Some("old".into()),
            new_text: "new".into(),
        }
    );
    assert!(execution.is_none());
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
    // Fan-out routing: the spawn opens a synthetic chip, then its tagged
    // session, both keyed by the compound id — never the shared tool-call id.
    assert!(matches!(
        started.as_slice(),
        [AgentEvent::ToolCall { id, .. }, AgentEvent::Subagent { parent_tool_use_id, event }]
            if id == "task-1--child-1"
                && parent_tool_use_id == "task-1--child-1"
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
        [AgentEvent::ToolResult { id, .. }, AgentEvent::Subagent { parent_tool_use_id, event }]
            if id == "task-1--child-1"
                && parent_tool_use_id == "task-1--child-1"
                && matches!(event.as_ref(), AgentEvent::Done { status: DoneStatus::Completed, .. })
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
        [AgentEvent::ToolResult { .. }, AgentEvent::Subagent { parent_tool_use_id, event }]
            if parent_tool_use_id == "task-1--child-1"
                && matches!(event.as_ref(), AgentEvent::Done { status: DoneStatus::Interrupted, .. })
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
async fn declining_a_question_reaches_omp_as_a_cancelled_response() {
    // The Skip button submits an answer with no labels. That has to arrive as
    // `cancelled: true, timedOut: false` — a distinct thing from a timeout and
    // from `confirmed: false`, which would be a real "no". The fixture exits
    // non-zero if any of those three details is wrong, so a silent drift shows
    // up as a failed turn instead of a question the user cannot escape.
    let harness = fake_harness("decline-question");
    let (controls, _steer, _interrupt) = controls_declining();

    let mut stream = harness.run(request("hello"), controls).await.unwrap();
    let events = collect_until_done(&mut stream).await;

    assert!(
        events.iter().any(|event| matches!(
            event,
            AgentEvent::Done {
                status: DoneStatus::Completed,
                ..
            }
        )),
        "declining must let the turn finish: {events:?}"
    );
}

#[tokio::test]
async fn orchestrator_cwd_receives_the_omp_system_prompt() {
    let root = tempfile::tempdir().unwrap();
    let orchestrator = root.path().join(".orchestrator");
    std::fs::create_dir(&orchestrator).unwrap();
    let harness =
        fake_harness("require-system-prompt").with_orchestrator_workspace(orchestrator.clone());
    let (controls, _steer, _interrupt) = controls_with_answer("Yes");
    let mut request = request("hello");
    request.cwd = orchestrator.to_string_lossy().into_owned();
    assert!(!request.enable_workers_mcp);

    let events = harness
        .run(request, controls)
        .await
        .unwrap()
        .collect::<Vec<_>>()
        .await;

    assert!(events.iter().all(Result::is_ok));
    assert!(events.iter().any(|event| {
        matches!(
            event,
            Ok(AgentEvent::Done {
                status: DoneStatus::Completed,
                ..
            })
        )
    }));
}

#[tokio::test]
async fn every_omp_launch_scopes_skills_to_the_project() {
    // The fixture exits non-zero unless `--config` names a readable overlay
    // that turns off EVERY user-level skill root. Without it a chat inherits
    // the personal skills mirrored under `~` (176 here) on top of the repo's
    // own — one workspace's skills leaked into every other project.
    let harness = fake_harness("require-skill-scope");
    let (controls, _steer, _interrupt) = controls_with_answer("Yes");

    let events = harness
        .run(request("hello"), controls)
        .await
        .unwrap()
        .collect::<Vec<_>>()
        .await;

    assert!(events.iter().all(Result::is_ok));
    assert!(events.iter().any(|event| {
        matches!(
            event,
            Ok(AgentEvent::Done {
                status: DoneStatus::Completed,
                ..
            })
        )
    }));
}

#[tokio::test]
async fn non_orchestrator_cwd_does_not_receive_the_omp_system_prompt() {
    let root = tempfile::tempdir().unwrap();
    let orchestrator = root.path().join(".orchestrator");
    let project = root.path().join("project");
    std::fs::create_dir(&orchestrator).unwrap();
    std::fs::create_dir(&project).unwrap();
    let harness = fake_harness("reject-system-prompt").with_orchestrator_workspace(orchestrator);
    let (controls, _steer, _interrupt) = controls_with_answer("Yes");
    let mut request = request("hello");
    request.cwd = project.to_string_lossy().into_owned();

    let events = harness
        .run(request, controls)
        .await
        .unwrap()
        .collect::<Vec<_>>()
        .await;

    assert!(events.iter().all(Result::is_ok));
    assert!(events.iter().any(|event| {
        matches!(
            event,
            Ok(AgentEvent::Done {
                status: DoneStatus::Completed,
                ..
            })
        )
    }));
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
    assert!(events.iter().any(|event| matches!(
        event,
        AgentEvent::Usage {
            context_usage: Some(zeron_proto::ContextUsage {
                tokens: 392_000,
                context_window: 828_000,
            }),
            ..
        }
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
async fn a_dying_child_reports_its_own_stderr_not_a_bare_transport_error() {
    let harness = fake_harness("stderr-crash");
    let (controls, _steer, _interrupt) = controls_with_answer("Yes");
    let error = match harness.run(request("hello"), controls).await {
        Ok(_) => panic!("a child that exits before ready must fail the run"),
        Err(error) => error,
    };
    let error = error.to_string();
    // The transport frame stays, but the child's own reason rides with it —
    // "OMP RPC exited before ready" alone sent the user to the debug log.
    assert!(error.contains("exited before ready"), "{error}");
    assert!(error.contains("no credentials for anthropic"), "{error}");
}

#[tokio::test]
async fn an_oversized_image_degrades_to_its_path_instead_of_failing_the_run() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("large.png");
    tokio::fs::write(&path, vec![0_u8; MAX_OUTBOUND_BYTES])
        .await
        .unwrap();
    let harness = fake_harness("normal");
    let (controls, _steer, _interrupt) = controls_with_answer("Yes");
    let mut run_request = request("with image");
    run_request.attachments = vec![path.to_string_lossy().into_owned()];
    // The prompt text already lists the local path and OMP reads files itself,
    // so an attachment that cannot ride the frame is not a failed send: the
    // turn is ACCEPTED without inline images (three screenshots used to make
    // `run` return `Err` before the first token).
    assert!(
        harness.run(run_request, controls).await.is_ok(),
        "an oversized attachment must degrade to its path, not reject the turn"
    );
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
#[ignore = "uses the installed OMP CLI: proves the real catalog survives the 1 MiB frame limit"]
async fn real_omp_catalog_arrives_whole() {
    // Medido em 2026-08-28: 550 linhas, 1,2 MiB — o frame nao cabe, entao esta
    // e a unica prova de que a v2 do protocolo esta mesmo negociada. Com v1 o
    // filho devolve "RPC response exceeded the transport limit" e a lista some.
    let models = OmpHarness::new().models().await.unwrap();
    assert!(
        models.len() > 100,
        "a truncated catalog means the chunked protocol is off: {} rows",
        models.len()
    );
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

#[test]
fn workers_bridge_timeout_strictly_exceeds_tool_blocking_ceiling() {
    let tools_list_response = zeron_workers_unpeel::controller_mcp_handle_request(json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/list",
        "params": {}
    }))
    .expect("controller MCP tools/list response");

    let tools = tools_list_response
        .get("result")
        .and_then(|r| r.get("tools"))
        .and_then(Value::as_array)
        .expect("tools array in response");

    let workers_tool = tools
        .iter()
        .find(|t| t.get("name").and_then(Value::as_str) == Some("workers"))
        .expect("workers tool in schema");

    let max_timeout_seconds = workers_tool
        .pointer("/inputSchema/properties/timeout_seconds/maximum")
        .and_then(Value::as_u64)
        .expect("timeout_seconds maximum property in workers tool inputSchema");

    let transport_timeout_seconds = zeron_harness::omp::workers_bridge::TOOL_CALL_TIMEOUT.as_secs();

    // The transport timeout must strictly exceed the maximum tool blocking duration,
    // with at least a 60s margin for IPC round-trip and process scheduling.
    assert!(
        transport_timeout_seconds > max_timeout_seconds,
        "transport timeout ({transport_timeout_seconds}s) must strictly exceed maximum tool blocking duration ({max_timeout_seconds}s)"
    );
    let margin = transport_timeout_seconds - max_timeout_seconds;
    assert!(
        margin >= 60,
        "transport timeout ({transport_timeout_seconds}s) must have at least 60s margin over tool blocking ceiling ({max_timeout_seconds}s), got {margin}s"
    );
}
