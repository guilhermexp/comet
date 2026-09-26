//! Retargeting a local Chat away from a checkout currently used by Workers.

#![cfg(unix)]

use std::ffi::OsString;
use std::fs;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde_json::json;
use zeron_engine::{EngineCore, HarnessRegistry};
use zeron_proto::HarnessId;
use zeron_rpc::methods;

static ENV_LOCK: Mutex<()> = Mutex::new(());

struct UnpeelHomeGuard(Option<OsString>);

impl UnpeelHomeGuard {
    fn set(path: &Path) -> Self {
        let previous = std::env::var_os("UNPEEL_HOME");
        // SAFETY: this test is the sole environment-mutating test in its binary.
        unsafe { std::env::set_var("UNPEEL_HOME", path) };
        Self(previous)
    }
}

impl Drop for UnpeelHomeGuard {
    fn drop(&mut self) {
        // SAFETY: the caller holds ENV_LOCK until after this guard is dropped.
        unsafe {
            match self.0.take() {
                Some(previous) => std::env::set_var("UNPEEL_HOME", previous),
                None => std::env::remove_var("UNPEEL_HOME"),
            }
        }
    }
}

struct ChildGuard(Child);

impl Drop for ChildGuard {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn git(cwd: &Path, args: &[&str]) {
    let result = Command::new("git")
        .args(args)
        .current_dir(cwd)
        .output()
        .expect("run git");
    assert!(
        result.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&result.stderr)
    );
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

fn write_worker_manifest(home: &Path, worktree: &Path, state: &str, pid: u32) {
    let session_id = "worker-retarget-test";
    let session_dir = home.join("app-sessions").join(session_id);
    fs::create_dir_all(&session_dir).expect("session directory");
    let timestamp = now_ms();
    fs::write(
        session_dir.join("manifest.json"),
        serde_json::to_vec(&json!({
            "session": {
                "id": session_id,
                "project_id": "worker-project",
                "label": "Worker on feature branch",
                "command": "sleep 60",
                "created_at": timestamp,
                "worktree_path": worktree,
                "worktree_branch": "feature/guard-retarget"
            },
            "cwd": worktree,
            "state": state,
            "pid": pid,
            "heartbeat_at": timestamp,
            "updated_at": timestamp,
            "exit_code": null
        }))
        .expect("serialize worker manifest"),
    )
    .expect("write worker manifest");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn local_retarget_is_blocked_by_live_worker_and_allowed_after_it_stops() {
    let _env_lock = ENV_LOCK.lock().expect("UNPEEL_HOME test lock");
    // macOS Unix-domain socket paths have a short SUN_LEN limit. A short path
    // under /tmp keeps the synthetic Worker socket within that limit.
    let temp = tempfile::Builder::new()
        .prefix("wt-")
        .tempdir_in("/tmp")
        .expect("tempdir");
    let unpeel_home = temp.path().join("unpeel");
    fs::create_dir_all(&unpeel_home).expect("unpeel home");
    let _home_guard = UnpeelHomeGuard::set(&unpeel_home);

    let repo = temp.path().join("repo");
    let worktree = temp.path().join("repo-feature");
    fs::create_dir_all(&repo).expect("repo directory");
    git(&repo, &["init", "-b", "main"]);
    git(&repo, &["config", "user.email", "test@example.invalid"]);
    git(&repo, &["config", "user.name", "Retarget test"]);
    fs::write(repo.join("README.md"), "fixture\n").expect("repo file");
    git(&repo, &["add", "README.md"]);
    git(&repo, &["commit", "-m", "fixture"]);
    git(
        &repo,
        &[
            "worktree",
            "add",
            "-b",
            "feature/guard-retarget",
            worktree.to_str().expect("worktree path utf8"),
            "HEAD",
        ],
    );

    let mut child = ChildGuard(
        Command::new("sleep")
            .arg("60")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("sleep worker fixture"),
    );
    fs::write(
        unpeel_home.join("app-state.json"),
        serde_json::to_vec(&json!({
            "projects": [{
                "id": "worker-project",
                "name": "Worker project",
                "path": worktree,
                "sort_order": 0,
                "is_folder": false
            }],
            "presets": [],
            "active_tabs": {},
            "pinned_sessions": {}
        }))
        .expect("serialize Workers state"),
    )
    .expect("write Workers state");
    write_worker_manifest(&unpeel_home, &worktree, "running", child.0.id());
    let _worker_socket = std::os::unix::net::UnixListener::bind(
        unpeel_home
            .join("app-sessions")
            .join("worker-retarget-test")
            .join("session.sock"),
    )
    .expect("bind live Worker socket");

    let workers = zeron_workers_unpeel::LocalWorkersClient::new();
    let running = workers.bootstrap().expect("Worker bootstrap");
    assert!(
        running
            .sessions
            .iter()
            .any(|session| session.id == "worker-retarget-test" && session.is_live()),
        "fixture must be visible as a live Worker: {:?}",
        running.sessions
    );

    let core = EngineCore::assemble(
        &temp.path().join("engine-data"),
        std::sync::Arc::new(HarnessRegistry::new()),
        HarnessId::Mock,
        None,
    )
    .expect("assemble engine");
    let local_chat = "local-chat";
    core.workspace
        .create_chat(
            local_chat,
            None,
            Some(&core.device_id),
            None,
            Some(repo.to_string_lossy().into_owned()),
        )
        .expect("create local Chat");
    let remote_chat = "remote-chat";
    core.workspace
        .create_chat(
            remote_chat,
            None,
            Some("another-device"),
            None,
            Some(repo.to_string_lossy().into_owned()),
        )
        .expect("create remote Chat");

    let client = zeron_rpc::memory_client(core.rpc_service());
    let params = |chat_id: &str| {
        json!({
            "op": "setChatCwd",
            "chatId": chat_id,
            "cwd": worktree
        })
    };

    let error = client
        .call(methods::MUTATE, params(local_chat))
        .await
        .expect_err("a live Worker blocks local retarget");
    assert!(error.to_string().contains("Worker is working"), "{error}");
    assert_eq!(
        core.workspace
            .chat(local_chat)
            .expect("read local Chat")
            .expect("local Chat exists")
            .cwd
            .as_deref(),
        Some(repo.to_str().expect("repo path utf8")),
        "the rejected Retarget must not update the Chat cwd"
    );

    // This Mutate is applied by the local viewer, while the Chat itself is
    // hosted remotely. It keeps the prior behavior and does not claim to know
    // what Workers are doing on the host device.
    client
        .call(methods::MUTATE, params(remote_chat))
        .await
        .expect("remote Chat retarget retains existing behavior");
    assert_eq!(
        core.workspace
            .chat(remote_chat)
            .expect("read remote Chat")
            .expect("remote Chat exists")
            .cwd
            .as_deref(),
        Some(worktree.to_str().expect("worktree path utf8"))
    );

    child.0.kill().expect("stop Worker process fixture");
    child.0.wait().expect("reap Worker process fixture");
    write_worker_manifest(&unpeel_home, &worktree, "exited", child.0.id());
    tokio::time::sleep(Duration::from_millis(550)).await;
    let stopped = workers.bootstrap().expect("stopped Worker bootstrap");
    assert!(
        stopped
            .sessions
            .iter()
            .any(|session| session.id == "worker-retarget-test" && !session.is_live()),
        "the Worker must be visible as stopped: {:?}",
        stopped.sessions
    );
    client
        .call(methods::MUTATE, params(local_chat))
        .await
        .expect("a stopped Worker allows local retarget");
    assert_eq!(
        core.workspace
            .chat(local_chat)
            .expect("read local Chat after stop")
            .expect("local Chat exists")
            .cwd
            .as_deref(),
        Some(worktree.to_str().expect("worktree path utf8"))
    );

    drop(client);
    core.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn terminal_inside_linked_checkout_reserves_it_only_against_removal() {
    let _env_lock = ENV_LOCK.lock().expect("UNPEEL_HOME test lock");
    let temp = tempfile::Builder::new()
        .prefix("wt-")
        .tempdir_in("/tmp")
        .expect("tempdir");
    let unpeel_home = temp.path().join("unpeel");
    fs::create_dir_all(&unpeel_home).expect("unpeel home");
    let _home_guard = UnpeelHomeGuard::set(&unpeel_home);

    let repo = temp.path().join("repo");
    let worktree = temp.path().join("repo-terminal");
    fs::create_dir_all(&repo).expect("repo directory");
    git(&repo, &["init", "-b", "main"]);
    git(&repo, &["config", "user.email", "test@example.invalid"]);
    git(&repo, &["config", "user.name", "Terminal test"]);
    fs::write(repo.join("README.md"), "fixture\n").expect("repo file");
    git(&repo, &["add", "README.md"]);
    git(&repo, &["commit", "-m", "fixture"]);
    git(
        &repo,
        &[
            "worktree",
            "add",
            "-b",
            "feature/terminal-in-use",
            worktree.to_str().expect("worktree path utf8"),
            "HEAD",
        ],
    );
    let nested = worktree.join("nested");
    fs::create_dir_all(&nested).expect("nested directory");

    let workers = zeron_workers_unpeel::LocalWorkersClient::new();
    assert!(!workers.checkout_is_busy(&worktree).expect("idle probe"));

    let terminals = zeron_engine::Terminals::new();
    let session = terminals
        .open_with_shell(
            nested.to_str().expect("nested path utf8"),
            80,
            24,
            Some("/bin/sh"),
        )
        .expect("open terminal in checkout");
    // checkout-activity.json is the persisted cross-process reservation
    // record that physical removal consults.
    let activity = unpeel_home.join("checkout-activity.json");
    let canonical = worktree.canonicalize().expect("canonical worktree");
    let terminal_reserved = || {
        fs::read(&activity)
            .ok()
            .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok())
            .and_then(|state| state["entries"].as_object().cloned())
            .is_some_and(|entries| {
                entries.values().any(|entry| {
                    entry["kind"] == "terminal" && entry["path"].as_str() == canonical.to_str()
                })
            })
    };
    assert!(
        terminal_reserved(),
        "a terminal inside the checkout reserves it before its shell starts"
    );
    assert!(
        !workers.checkout_is_busy(&worktree).expect("retarget probe"),
        "an open terminal must not block moving a Chat into the checkout"
    );

    terminals.close(&session.id).expect("close terminal");
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    while terminal_reserved() {
        assert!(
            std::time::Instant::now() < deadline,
            "closing the terminal must release the checkout"
        );
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    let session = terminals
        .open_with_shell(
            nested.to_str().expect("nested path utf8"),
            80,
            24,
            Some("/bin/sh"),
        )
        .expect("reopen terminal in checkout");
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    while !terminal_reserved() {
        assert!(
            std::time::Instant::now() < deadline,
            "the reopened terminal must reserve the checkout"
        );
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    terminals.shutdown();
    assert!(
        !terminal_reserved(),
        "an orderly shutdown releases terminal reservations before returning"
    );
    drop(session);
}
