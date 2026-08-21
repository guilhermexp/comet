use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;

use serde_json::{Value, json};
use zeron_harness::omp::process::{OmpLaunch, OmpProcess};
use zeron_harness::omp::protocol::{MAX_INBOUND_BYTES, parse_frame, sanitize_diagnostic};

fn fixture_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/fake-omp-rpc.sh")
}

fn fake_env(scenario: &str) -> HashMap<String, String> {
    HashMap::from([("FAKE_OMP_SCENARIO".to_owned(), scenario.to_owned())])
}

async fn start_fake(scenario: &str) -> (OmpProcess, tokio::sync::mpsc::Receiver<Value>) {
    let process = OmpProcess::start(OmpLaunch {
        executable: fixture_path(),
        cwd: std::env::current_dir().unwrap(),
        ephemeral: true,
        env: Some(fake_env(scenario)),
        handshake_timeout: Duration::from_secs(1),
        request_timeout: Duration::from_secs(1),
    })
    .await
    .unwrap();
    let events = process.take_events().unwrap();
    (process, events)
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
    let error = OmpProcess::start(OmpLaunch {
        executable: fixture_path(),
        cwd: std::env::current_dir().unwrap(),
        ephemeral: true,
        env: Some(fake_env("early-exit")),
        handshake_timeout: Duration::from_secs(1),
        request_timeout: Duration::from_secs(1),
    })
    .await
    .unwrap_err();
    assert!(error.to_string().contains("before ready"));
}
