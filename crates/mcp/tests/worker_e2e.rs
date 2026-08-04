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
    let worker = tools
        .spawn("sh -c 'sleep 30'", None, None)
        .await
        .expect("spawn");

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

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_resumed_wait_yields_the_rest_exactly_once() {
    let (_tmp, core, tools) = engine_with_chat().await;
    let worker = tools
        .spawn(
            "sh -c 'echo a$((0+1)); sleep 1; echo b$((1+1))'",
            None,
            None,
        )
        .await
        .expect("spawn");

    // Short enough to land between the two prints.
    let first = tools
        .wait(&worker.worker_id, None, Some(400))
        .await
        .expect("first wait");
    assert!(first.running, "the sleep is still ahead: {first:?}");
    assert!(
        first.output.contains("a1"),
        "first output was {:?}",
        first.output
    );
    assert!(
        !first.output.contains("b2"),
        "b2 cannot have printed yet: {:?}",
        first.output
    );

    let resumed = tools
        .wait(&worker.worker_id, Some(first.next_seq), Some(30_000))
        .await
        .expect("resumed wait");
    assert!(!resumed.running, "the worker exited: {resumed:?}");
    assert_eq!(resumed.exit_code, Some(0));
    // No hole where b2 was, and no duplicate of a1.
    assert!(
        resumed.output.contains("b2"),
        "resumed output was {:?}",
        resumed.output
    );
    assert!(
        !resumed.output.contains("a1"),
        "a1 was already delivered: {:?}",
        resumed.output
    );
    core.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_worker_runs_in_the_cwd_it_was_given() {
    let (tmp, core, tools) = engine_with_chat().await;
    // A path with a space: an unquoted `cd` would silently run in the wrong
    // directory, and the worker would still look like it succeeded.
    let worktree = tmp.path().join("Work Tree");
    std::fs::create_dir_all(&worktree).expect("worktree dir");
    std::fs::write(worktree.join("marker.txt"), b"m4rk3r\n").expect("marker");

    let worker = tools
        .spawn(
            "sh -c 'cat marker.txt; exit 0'",
            Some(&worktree.to_string_lossy()),
            None,
        )
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
