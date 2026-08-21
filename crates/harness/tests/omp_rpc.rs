use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;

use serde_json::{Value, json};
use zeron_harness::omp::normalize::{AgentEndDisposition, OmpNormalizer};
use zeron_harness::omp::process::{OmpLaunch, OmpProcess};
use zeron_harness::omp::protocol::{MAX_INBOUND_BYTES, parse_frame, sanitize_diagnostic};
use zeron_harness::omp::{discover_commands_with_launch, discover_models_with_launch};
use zeron_proto::{AgentEvent, DoneStatus, ReasoningLevel, ToolCall};

fn fixture_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/fake-omp-rpc.sh")
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
