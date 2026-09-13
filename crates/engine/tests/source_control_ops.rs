//! Source-control mutations over the in-memory RPC transport, against a
//! temporary git checkout created by the test — never a user repository.
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use zeron_engine::{EngineCore, HarnessRegistry};
use zeron_rpc::methods;

fn assemble(data: &Path) -> EngineCore {
    EngineCore::assemble(
        data,
        Arc::new(HarnessRegistry::new()),
        zeron_proto::HarnessId::Mock,
        None,
    )
    .unwrap()
}

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
    assert!(
        output.status.success(),
        "git {:?} failed: {}",
        args,
        String::from_utf8_lossy(&output.stderr)
    );
}

async fn git_stdout(cwd: &Path, args: &[&str]) -> String {
    let output = tokio::process::Command::new("git")
        .args(args)
        .current_dir(cwd)
        .output()
        .await
        .expect("git spawns");
    assert!(
        output.status.success(),
        "git {:?} failed: {}",
        args,
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

async fn init_repo(dir: &Path) {
    std::fs::create_dir_all(dir).unwrap();
    git(dir, &["init", "-b", "main"]).await;
    git(dir, &["config", "user.email", "test@test"]).await;
    git(dir, &["config", "user.name", "test"]).await;
    git(dir, &["config", "commit.gpgsign", "false"]).await;
    std::fs::write(dir.join("tracked.txt"), "base\n").unwrap();
    git(dir, &["add", "tracked.txt"]).await;
    git(dir, &["commit", "-m", "initial"]).await;
}

async fn add_origin(repo: &Path, bare: &Path) {
    git(bare, &["init", "--bare", "-b", "main"]).await;
    git(repo, &["remote", "add", "origin", bare.to_str().unwrap()]).await;
    git(repo, &["push", "-u", "origin", "main"]).await;
}

fn file_named<'a>(status: &'a Value, name: &str) -> &'a Value {
    status["files"]
        .as_array()
        .unwrap()
        .iter()
        .find(|file| file["path"] == name)
        .unwrap_or_else(|| panic!("missing {name} in {}", status["files"]))
}

async fn status_of(client: &zeron_rpc::RpcClient, cwd: &Path) -> Value {
    client
        .call(
            methods::GET_CHECKOUT_STATUS,
            json!({ "cwd": cwd.to_str().unwrap() }),
        )
        .await
        .expect("GetCheckoutStatus")
}

fn staged(file: &Value) -> bool {
    file["index"] != "unmodified" && file["index"] != "untracked"
}

fn unstaged(file: &Value) -> bool {
    file["worktree"] != "unmodified"
}

struct Harness {
    _temp: tempfile::TempDir,
    repo: PathBuf,
    core: EngineCore,
    client: zeron_rpc::RpcClient,
}

impl Harness {
    async fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let repo = temp.path().join("repo");
        init_repo(&repo).await;
        let core = assemble(&temp.path().join("data"));
        core.workspace
            .create_chat(
                "sc-owned",
                None,
                Some(&core.device_id),
                None,
                Some(repo.to_string_lossy().into_owned()),
            )
            .unwrap();
        let client = zeron_rpc::memory_client(core.rpc_service());
        Self {
            _temp: temp,
            repo,
            core,
            client,
        }
    }

    async fn shutdown(self) {
        self.core.shutdown().await;
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn status_separates_staged_unstaged_and_marks_untracked() {
    let h = Harness::new().await;
    std::fs::write(h.repo.join("tracked.txt"), "staged\n").unwrap();
    git(&h.repo, &["add", "tracked.txt"]).await;
    std::fs::write(h.repo.join("tracked.txt"), "staged\nand unstaged\n").unwrap();
    std::fs::write(h.repo.join("brand-new.txt"), "untracked\n").unwrap();
    std::fs::write(h.repo.join("added.txt"), "added\n").unwrap();
    git(&h.repo, &["add", "added.txt"]).await;

    let status = status_of(&h.client, &h.repo).await;
    let tracked = file_named(&status, "tracked.txt");
    assert_eq!(tracked["index"], "modified");
    assert_eq!(tracked["worktree"], "modified");
    let untracked = file_named(&status, "brand-new.txt");
    assert_eq!(untracked["index"], "untracked");
    assert_eq!(untracked["worktree"], "untracked");
    let added = file_named(&status, "added.txt");
    assert_eq!(added["index"], "added");
    assert_eq!(added["worktree"], "unmodified");
    h.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn stage_moves_file_from_changes_to_staged() {
    let h = Harness::new().await;
    std::fs::write(h.repo.join("tracked.txt"), "changed\n").unwrap();
    let before_status = status_of(&h.client, &h.repo).await;
    let before = file_named(&before_status, "tracked.txt");
    assert!(!staged(before));
    assert!(unstaged(before));

    h.client
        .call(
            methods::STAGE_FILES,
            json!({ "cwd": h.repo.to_str().unwrap(), "paths": ["tracked.txt"] }),
        )
        .await
        .expect("stage");

    let after_status = status_of(&h.client, &h.repo).await;
    let after = file_named(&after_status, "tracked.txt");
    assert!(staged(after), "{after}");
    assert!(!unstaged(after), "{after}");
    h.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn unstage_preserves_worktree_content() {
    let h = Harness::new().await;
    std::fs::write(h.repo.join("tracked.txt"), "keep-me\n").unwrap();
    git(&h.repo, &["add", "tracked.txt"]).await;

    h.client
        .call(
            methods::UNSTAGE_FILES,
            json!({ "cwd": h.repo.to_str().unwrap(), "paths": ["tracked.txt"] }),
        )
        .await
        .expect("unstage");

    let after_status = status_of(&h.client, &h.repo).await;
    let after = file_named(&after_status, "tracked.txt");
    assert!(!staged(after), "{after}");
    assert!(unstaged(after), "{after}");
    assert_eq!(
        std::fs::read_to_string(h.repo.join("tracked.txt")).unwrap(),
        "keep-me\n"
    );
    h.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn discard_tracked_preserves_staged_and_restores_worktree() {
    let h = Harness::new().await;
    std::fs::write(h.repo.join("tracked.txt"), "index\n").unwrap();
    git(&h.repo, &["add", "tracked.txt"]).await;
    std::fs::write(h.repo.join("tracked.txt"), "index\nworktree\n").unwrap();

    h.client
        .call(
            methods::DISCARD_FILES,
            json!({ "cwd": h.repo.to_str().unwrap(), "paths": ["tracked.txt"] }),
        )
        .await
        .expect("discard");

    let after_status = status_of(&h.client, &h.repo).await;
    let after = file_named(&after_status, "tracked.txt");
    assert_eq!(after["index"], "modified");
    assert_eq!(after["worktree"], "unmodified");
    assert_eq!(
        std::fs::read_to_string(h.repo.join("tracked.txt")).unwrap(),
        "index\n"
    );
    h.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn discard_untracked_deletes_the_file() {
    let h = Harness::new().await;
    let path = h.repo.join("scratch.txt");
    std::fs::write(&path, "gone\n").unwrap();

    h.client
        .call(
            methods::DISCARD_FILES,
            json!({ "cwd": h.repo.to_str().unwrap(), "paths": ["scratch.txt"] }),
        )
        .await
        .expect("discard untracked");

    assert!(!path.exists());
    h.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn path_with_dotdot_is_refused() {
    let h = Harness::new().await;
    let before = git_stdout(&h.repo, &["status", "--porcelain=v1"]).await;
    let err = h
        .client
        .call(
            methods::STAGE_FILES,
            json!({ "cwd": h.repo.to_str().unwrap(), "paths": ["../outside.txt"] }),
        )
        .await
        .expect_err("dotdot must be refused");
    assert!(
        err.to_string().to_lowercase().contains("path")
            || err.to_string().to_lowercase().contains("param"),
        "{err}"
    );
    let after = git_stdout(&h.repo, &["status", "--porcelain=v1"]).await;
    assert_eq!(before, after);
    h.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn commit_consumes_index_and_increases_ahead() {
    let h = Harness::new().await;
    let origin = h._temp.path().join("origin.git");
    std::fs::create_dir_all(&origin).unwrap();
    add_origin(&h.repo, &origin).await;

    std::fs::write(h.repo.join("tracked.txt"), "committed\n").unwrap();
    git(&h.repo, &["add", "tracked.txt"]).await;
    let before = status_of(&h.client, &h.repo).await;
    assert_eq!(before["ahead"], 0);

    h.client
        .call(
            methods::COMMIT_CHECKOUT,
            json!({
                "cwd": h.repo.to_str().unwrap(),
                "message": "consume the index",
            }),
        )
        .await
        .expect("commit");

    let after = status_of(&h.client, &h.repo).await;
    assert!(
        after["files"]
            .as_array()
            .unwrap()
            .iter()
            .all(|file| !staged(file)),
        "{}",
        after["files"]
    );
    assert_eq!(after["ahead"], 1);
    let log = git_stdout(&h.repo, &["log", "-1", "--pretty=%s"]).await;
    assert_eq!(log, "consume the index");
    h.shutdown().await;
}

fn own_checkout(core: &EngineCore, repo: &Path) {
    core.workspace
        .create_chat(
            "sc-owned",
            None,
            Some(&core.device_id),
            None,
            Some(repo.to_string_lossy().into_owned()),
        )
        .unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn empty_commit_message_is_refused() {
    let h = Harness::new().await;
    std::fs::write(h.repo.join("tracked.txt"), "x\n").unwrap();
    git(&h.repo, &["add", "tracked.txt"]).await;
    let before = git_stdout(&h.repo, &["rev-parse", "HEAD"]).await;
    let err = h
        .client
        .call(
            methods::COMMIT_CHECKOUT,
            json!({ "cwd": h.repo.to_str().unwrap(), "message": "   " }),
        )
        .await
        .expect_err("blank message");
    assert!(err.to_string().to_lowercase().contains("message"), "{err}");
    assert_eq!(git_stdout(&h.repo, &["rev-parse", "HEAD"]).await, before);
    h.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn empty_index_commit_is_refused() {
    let h = Harness::new().await;
    std::fs::write(h.repo.join("tracked.txt"), "only worktree\n").unwrap();
    let before = git_stdout(&h.repo, &["rev-parse", "HEAD"]).await;
    let err = h
        .client
        .call(
            methods::COMMIT_CHECKOUT,
            json!({
                "cwd": h.repo.to_str().unwrap(),
                "message": "should not land",
            }),
        )
        .await
        .expect_err("empty index");
    assert!(
        err.to_string().to_lowercase().contains("staged")
            || err.to_string().to_lowercase().contains("nothing"),
        "{err}"
    );
    assert_eq!(git_stdout(&h.repo, &["rev-parse", "HEAD"]).await, before);
    assert_eq!(
        std::fs::read_to_string(h.repo.join("tracked.txt")).unwrap(),
        "only worktree\n"
    );
    h.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn sync_fast_forward_against_local_remote() {
    let temp = tempfile::tempdir().unwrap();
    let origin = temp.path().join("origin.git");
    let a = temp.path().join("a");
    let b = temp.path().join("b");
    std::fs::create_dir_all(&origin).unwrap();
    init_repo(&a).await;
    add_origin(&a, &origin).await;
    git(
        temp.path(),
        &["clone", origin.to_str().unwrap(), b.to_str().unwrap()],
    )
    .await;
    git(&b, &["config", "user.email", "test@test"]).await;
    git(&b, &["config", "user.name", "test"]).await;
    git(&b, &["config", "commit.gpgsign", "false"]).await;

    std::fs::write(b.join("from-b.txt"), "remote\n").unwrap();
    git(&b, &["add", "from-b.txt"]).await;
    git(&b, &["commit", "-m", "from b"]).await;
    git(&b, &["push"]).await;

    let core = assemble(&temp.path().join("data"));
    own_checkout(&core, &a);
    let client = zeron_rpc::memory_client(core.rpc_service());
    client
        .call(
            methods::SYNC_CHECKOUT,
            json!({ "cwd": a.to_str().unwrap() }),
        )
        .await
        .expect("sync");
    assert!(a.join("from-b.txt").exists());
    core.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn diverged_sync_aborts_without_merge() {
    let temp = tempfile::tempdir().unwrap();
    let origin = temp.path().join("origin.git");
    let a = temp.path().join("a");
    let b = temp.path().join("b");
    std::fs::create_dir_all(&origin).unwrap();
    init_repo(&a).await;
    add_origin(&a, &origin).await;
    git(
        temp.path(),
        &["clone", origin.to_str().unwrap(), b.to_str().unwrap()],
    )
    .await;
    git(&b, &["config", "user.email", "test@test"]).await;
    git(&b, &["config", "user.name", "test"]).await;
    git(&b, &["config", "commit.gpgsign", "false"]).await;

    std::fs::write(a.join("from-a.txt"), "a\n").unwrap();
    git(&a, &["add", "from-a.txt"]).await;
    git(&a, &["commit", "-m", "from a"]).await;

    std::fs::write(b.join("from-b.txt"), "b\n").unwrap();
    git(&b, &["add", "from-b.txt"]).await;
    git(&b, &["commit", "-m", "from b"]).await;
    git(&b, &["push"]).await;

    let before = git_stdout(&a, &["rev-parse", "HEAD"]).await;
    let core = assemble(&temp.path().join("data"));
    own_checkout(&core, &a);
    let client = zeron_rpc::memory_client(core.rpc_service());
    client
        .call(
            methods::SYNC_CHECKOUT,
            json!({ "cwd": a.to_str().unwrap() }),
        )
        .await
        .expect_err("diverged sync must abort");
    assert_eq!(git_stdout(&a, &["rev-parse", "HEAD"]).await, before);
    assert!(!a.join("from-b.txt").exists());
    core.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn publish_sets_upstream_on_new_branch() {
    let temp = tempfile::tempdir().unwrap();
    let origin = temp.path().join("origin.git");
    let repo = temp.path().join("repo");
    std::fs::create_dir_all(&origin).unwrap();
    init_repo(&repo).await;
    git(&origin, &["init", "--bare", "-b", "main"]).await;
    git(
        &repo,
        &["remote", "add", "origin", origin.to_str().unwrap()],
    )
    .await;
    git(&repo, &["checkout", "-b", "feature"]).await;

    let core = assemble(&temp.path().join("data"));
    own_checkout(&core, &repo);
    let client = zeron_rpc::memory_client(core.rpc_service());
    client
        .call(
            methods::SYNC_CHECKOUT,
            json!({ "cwd": repo.to_str().unwrap() }),
        )
        .await
        .expect("publish");

    let upstream = git_stdout(
        &repo,
        &[
            "rev-parse",
            "--abbrev-ref",
            "--symbolic-full-name",
            "@{upstream}",
        ],
    )
    .await;
    assert_eq!(upstream, "origin/feature");
    core.shutdown().await;
}
