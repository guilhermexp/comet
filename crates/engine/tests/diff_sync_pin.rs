//! A checkout nothing chats about must still be watchable.
//!
//! Diff entries are built from this device's chat rows, so a Workers project
//! (which has no chat row at all) was never captured and its right-pane diff
//! sat on "Preparing diff…" forever. `pin_checkout` is the subscription-side
//! way in.

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use zeron_engine::{EngineCore, HarnessRegistry};

async fn git(cwd: &Path, args: &[&str]) {
    let output = tokio::process::Command::new("git")
        .args(args)
        .current_dir(cwd)
        .env("GIT_AUTHOR_NAME", "test")
        .env("GIT_AUTHOR_EMAIL", "test@test")
        .env("GIT_COMMITTER_NAME", "test")
        .env("GIT_COMMITTER_EMAIL", "test@test")
        .output()
        .await
        .expect("git spawns");
    assert!(output.status.success(), "git {args:?} failed");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_chatless_checkout_is_published_while_pinned() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let repo_dir = tmp.path().join("repo");
    std::fs::create_dir_all(&repo_dir).expect("repo dir");
    git(&repo_dir, &["init", "-b", "main"]).await;
    std::fs::write(repo_dir.join("a.txt"), "one\n").expect("write");
    git(&repo_dir, &["add", "."]).await;
    git(&repo_dir, &["commit", "-m", "initial"]).await;
    std::fs::write(repo_dir.join("a.txt"), "one\ntwo\n").expect("dirty");

    let data = tmp.path().join("data");
    std::fs::create_dir_all(&data).expect("data dir");
    let core = EngineCore::assemble(
        &data,
        Arc::new(HarnessRegistry::new()),
        zeron_proto::HarnessId::Mock,
        None,
    )
    .expect("engine assembles");

    // No space, no chat: reconcile has nothing to group, so nothing publishes.
    core.diff_sync.reconcile_now().await;
    assert!(
        core.diff_sync.watch_diffs().borrow().is_empty(),
        "a chatless checkout is invisible to reconcile"
    );

    let pin = core
        .diff_sync
        .pin_checkout(&repo_dir)
        .await
        .expect("repo is a checkout");

    let deadline = tokio::time::Instant::now() + Duration::from_secs(120);
    let diff = loop {
        if let Some(diff) = core.diff_sync.watch_diffs().borrow().first().cloned() {
            break diff;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "pinned checkout published before timeout"
        );
        tokio::time::sleep(Duration::from_millis(100)).await;
    };
    assert!(
        diff.patch.contains("a.txt"),
        "the dirty file is in the patch"
    );

    // A reconcile pass while pinned must not tear the entry down.
    core.diff_sync.reconcile_now().await;
    assert!(!core.diff_sync.watch_diffs().borrow().is_empty());
    drop(pin);
}
