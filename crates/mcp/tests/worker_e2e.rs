//! The worker tools against a real engine, over a real WebSocket, driving a
//! real PTY.
//!
//! The unit tests in `tools.rs` pin the bounded/resumable logic against a
//! scripted stub. This pins the other half: that the RPC shapes, the base64 on
//! both directions, and the `Exit` that ends a subscription are what the engine
//! actually serves.
//!
//! Terminal echo is why the output markers are computed rather than literal: the
//! PTY echoes the command line back, so a worker that prints `b` after being
//! asked to `echo b` produces `b` twice and "exactly once" would prove nothing.
//! `echo b$((1+1))` types `b$((1+1))` and prints `b2` — the same trick
//! `crates/engine/tests/m5_repos_diffs_terminals.rs` uses.

use std::sync::Arc;
use std::time::Duration;

use comet_engine::{EngineCore, HarnessRegistry};
use comet_mcp::{RpcEngineClient, ToolError, WorkerConfig, WorkerTools};
use comet_proto::HarnessId;

const CHAT: &str = "chat-worker-e2e";

/// A real engine with its RPC surface on a loopback WebSocket, plus a chat
/// whose cwd is a scratch directory — `OpenTerminal` derives the worker's cwd
/// from the chat row.
async fn engine_with_chat() -> (tempfile::TempDir, EngineCore, WorkerTools<RpcEngineClient>) {
    let tmp = tempfile::tempdir().expect("tempdir");
    let cwd = tmp.path().join("checkout");
    std::fs::create_dir_all(&cwd).expect("checkout dir");

    let core = EngineCore::assemble(
        &tmp.path().join("data"),
        Arc::new(HarnessRegistry::new()),
        HarnessId::Mock,
        None,
    )
    .expect("engine core assembles");
    core.workspace
        .create_space(
            "space-worker-e2e",
            &core.device_id,
            &cwd.to_string_lossy(),
            None,
            true,
        )
        .expect("create space");
    core.workspace
        .create_chat(
            CHAT,
            "space-worker-e2e",
            None,
            Some(cwd.to_string_lossy().into_owned()),
        )
        .expect("create chat");

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind ipc");
    let port = listener.local_addr().expect("addr").port();
    tokio::spawn(comet_rpc::serve_ws_listener(listener, core.rpc_service()));

    let rpc = comet_rpc::connect_ws(&format!("ws://127.0.0.1:{port}"))
        .await
        .expect("dial the engine");
    let tools = WorkerTools::new(
        RpcEngineClient::new(Arc::new(rpc)),
        WorkerConfig::new(CHAT, 0),
    );
    (tmp, core, tools)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_worker_runs_to_a_nonzero_exit_code() {
    let (_tmp, core, tools) = engine_with_chat().await;
    let worker = tools
        .spawn("sh -c 'echo hi; exit 3'", None, None)
        .await
        .expect("spawn");

    let waited = tools
        .wait(&worker.worker_id, None, Some(30_000))
        .await
        .expect("wait");
    assert!(!waited.running, "the worker exited: {waited:?}");
    // The login shell exits with the status of its last command.
    assert_eq!(waited.exit_code, Some(3), "output was {:?}", waited.output);
    assert!(
        waited.output.contains("hi"),
        "output was {:?}",
        waited.output
    );
    core.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_killed_worker_is_gone_rather_than_finished() {
    let (_tmp, core, tools) = engine_with_chat().await;
    let worker = tools.spawn("sleep 30", None, None).await.expect("spawn");

    // Still running when the wait's deadline arrives — the bound is what makes
    // "check on it without committing the turn" real.
    let waited = tools
        .wait(&worker.worker_id, None, Some(400))
        .await
        .expect("wait");
    assert!(waited.running, "sleep 30 has not exited: {waited:?}");
    assert_eq!(waited.exit_code, None);

    assert!(tools.kill(&worker.worker_id).await.expect("kill").ok);

    // `CloseTerminal` drops the PTY *and* its replay buffer together, so a
    // killed worker has no exit status left to report — the honest answer is
    // that it is gone, and that is what distinguishes it from one that finished.
    assert_eq!(
        tools.wait(&worker.worker_id, None, Some(400)).await,
        Err(ToolError::NotFound(worker.worker_id.clone()))
    );
    core.shutdown().await;
}

/// Read forward from `cursor` until `needle` shows up, accumulating everything
/// seen. Returns the accumulated output and the cursor to resume from — the
/// same `nextSeq` chaining an agent would do, so the continuity assertions
/// below are about the cursor and not about how long a call happened to take.
async fn read_until(
    tools: &WorkerTools<RpcEngineClient>,
    worker_id: &str,
    mut cursor: u64,
    needle: &str,
) -> (String, u64) {
    let deadline = std::time::Instant::now() + Duration::from_secs(30);
    let mut seen = String::new();
    loop {
        let read = tools
            .read(worker_id, Some(cursor), Some(200))
            .await
            .expect("read");
        cursor = read.next_seq;
        seen.push_str(&read.output);
        if seen.contains(needle) {
            return (seen, cursor);
        }
        assert!(
            std::time::Instant::now() < deadline,
            "never saw {needle:?}; got {seen:?}"
        );
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_resumed_wait_yields_the_rest_exactly_once() {
    let (tmp, core, tools) = engine_with_chat().await;
    // The worker blocks on a gate file this test creates, so "b2 has not
    // printed yet" is caused by the test rather than raced against a sleep.
    let gate = tmp.path().join("gate");
    let worker = tools
        .spawn(
            &format!(
                "echo a$((0+1)); while [ ! -f {} ]; do sleep 0.05; done; echo b$((1+1))",
                gate.display()
            ),
            None,
            None,
        )
        .await
        .expect("spawn");

    let (before, cursor) = read_until(&tools, &worker.worker_id, 0, "a1").await;
    assert!(
        !before.contains("b2"),
        "the gate is still closed, so b2 cannot have printed: {before:?}"
    );
    assert!(cursor > 0, "the cursor advanced over what was delivered");

    std::fs::write(&gate, b"go").expect("open the gate");

    let resumed = tools
        .wait(&worker.worker_id, Some(cursor), Some(30_000))
        .await
        .expect("resumed wait");
    assert!(!resumed.running, "the worker exited: {resumed:?}");
    assert_eq!(resumed.exit_code, Some(0));
    // No hole where b2 was...
    assert!(
        resumed.output.contains("b2"),
        "resumed output was {:?}",
        resumed.output
    );
    // ...and no duplicate of what the cursor already covered.
    assert!(
        !resumed.output.contains("a1"),
        "a1 was already delivered before {cursor}: {:?}",
        resumed.output
    );
    core.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_malformed_command_exits_instead_of_hanging() {
    let (_tmp, core, tools) = engine_with_chat().await;

    // Real syntax errors. Pasted into a shell line these would be rejected by
    // the INTERACTIVE shell, which then sits at its prompt forever holding one
    // of the device's 32 terminal slots. Quoted whole into `sh -c` they are the
    // inner shell's problem: it exits nonzero and the PTY dies with it.
    for command in ["echo a &&", "echo 'unterminated", "if true; then"] {
        let worker = tools.spawn(command, None, None).await.expect("spawn");
        let waited = tools
            .wait(&worker.worker_id, None, Some(30_000))
            .await
            .unwrap_or_else(|err| panic!("wait for {command:?}: {err}"));
        assert!(
            !waited.running,
            "{command:?} must terminate, not hang: {waited:?}"
        );
        assert!(
            matches!(waited.exit_code, Some(code) if code != 0),
            "{command:?} must report a failure: {waited:?}"
        );
    }

    // Valid but awkward glue — a bare `&`, a trailing backslash. These are the
    // shapes that used to be concatenated into `…; exit $?` and turn the whole
    // written line into a syntax error; now they just run.
    for command in ["echo a &", "echo a \\", "echo bg$((3+4)) & wait"] {
        let worker = tools.spawn(command, None, None).await.expect("spawn");
        let waited = tools
            .wait(&worker.worker_id, None, Some(30_000))
            .await
            .unwrap_or_else(|err| panic!("wait for {command:?}: {err}"));
        assert!(
            !waited.running,
            "{command:?} must terminate, not hang: {waited:?}"
        );
        assert_eq!(
            waited.exit_code,
            Some(0),
            "{command:?} is valid: {:?}",
            waited.output
        );
    }
    core.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_worker_runs_in_the_cwd_it_was_given() {
    let (tmp, core, tools) = engine_with_chat().await;
    // A space AND a quote: an unquoted `cd` would silently run in the wrong
    // directory and the worker would still look like it succeeded, and a
    // naive quoting scheme would break the line outright.
    let worktree = tmp.path().join("Work 'Tree'");
    std::fs::create_dir_all(&worktree).expect("worktree dir");
    std::fs::write(worktree.join("marker.txt"), b"m4rk3r\n").expect("marker");

    let worker = tools
        .spawn("cat marker.txt", Some(&worktree.to_string_lossy()), None)
        .await
        .expect("spawn");
    let waited = tools
        .wait(&worker.worker_id, None, Some(30_000))
        .await
        .expect("wait");
    assert!(!waited.running, "the worker exited: {waited:?}");
    assert_eq!(waited.exit_code, Some(0), "output was {:?}", waited.output);
    assert!(
        waited.output.contains("m4rk3r"),
        "output was {:?}",
        waited.output
    );
    core.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn reading_a_worker_this_server_never_spawned_is_a_clean_not_found() {
    let (_tmp, core, tools) = engine_with_chat().await;
    assert_eq!(
        tools.read("term-nope", None, Some(100)).await,
        Err(ToolError::NotFound("term-nope".into()))
    );
    // And a real engine terminal still needs to have come from `spawn`: the
    // tools mint no identity of their own.
    tokio::time::timeout(Duration::from_secs(5), core.shutdown())
        .await
        .expect("shutdown");
}
