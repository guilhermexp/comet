//! Hardening of Source Control: authorized cwd, porcelain paths, literal
//! pathspecs, atomic discard. Temporary checkouts only — never a user repo.
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
    String::from_utf8_lossy(&output.stdout).to_string()
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

struct Harness {
    _temp: tempfile::TempDir,
    repo: PathBuf,
    core: EngineCore,
    client: zeron_rpc::RpcClient,
}

impl Harness {
    async fn owned() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let repo = temp.path().join("repo");
        init_repo(&repo).await;
        let core = assemble(&temp.path().join("data"));
        own_checkout(&core, &repo);
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

async fn status_of(client: &zeron_rpc::RpcClient, cwd: &Path) -> Value {
    client
        .call(
            methods::GET_CHECKOUT_STATUS,
            json!({ "cwd": cwd.to_str().unwrap() }),
        )
        .await
        .expect("GetCheckoutStatus")
}

fn head_sha(repo: &Path) -> String {
    let output = std::process::Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(repo)
        .output()
        .expect("rev-parse");
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn mutant_rpc_refuses_cwd_outside_any_chat_or_space() {
    let temp = tempfile::tempdir().unwrap();
    let owned = temp.path().join("owned");
    let foreign = temp.path().join("foreign");
    init_repo(&owned).await;
    init_repo(&foreign).await;
    std::fs::write(foreign.join(".env"), "SECRET=1\n").unwrap();
    std::fs::write(foreign.join("tracked.txt"), "staged-for-commit\n").unwrap();
    git(&foreign, &["add", "tracked.txt"]).await;
    let before = head_sha(&foreign);

    let core = assemble(&temp.path().join("data"));
    own_checkout(&core, &owned);
    let client = zeron_rpc::memory_client(core.rpc_service());
    let cwd = foreign.to_str().unwrap();

    let commit_err = client
        .call(
            methods::COMMIT_CHECKOUT,
            json!({ "cwd": cwd, "message": "pwned" }),
        )
        .await
        .expect_err("foreign commit must be refused");
    let discard_err = client
        .call(
            methods::DISCARD_FILES,
            json!({ "cwd": cwd, "paths": [".env"] }),
        )
        .await
        .expect_err("foreign discard must be refused");

    let commit_msg = commit_err.to_string().to_lowercase();
    let discard_msg = discard_err.to_string().to_lowercase();
    assert!(
        commit_msg.contains("known checkout") || commit_msg.contains("bad params"),
        "{commit_err}"
    );
    assert!(
        discard_msg.contains("known checkout") || discard_msg.contains("bad params"),
        "{discard_err}"
    );
    assert_eq!(head_sha(&foreign), before, "foreign HEAD must not move");
    assert_eq!(
        std::fs::read_to_string(foreign.join(".env")).unwrap(),
        "SECRET=1\n"
    );
    core.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn porcelain_preserves_leading_space_in_path() {
    let h = Harness::owned().await;
    std::fs::write(h.repo.join(" nota.txt"), "leading space\n").unwrap();
    let status = status_of(&h.client, &h.repo).await;
    let files = status["files"].as_array().expect("files");
    assert!(
        files.iter().any(|file| file["path"] == " nota.txt"),
        "expected exact path with leading space, got {}",
        status["files"]
    );
    assert!(
        !files.iter().any(|file| file["path"] == "nota.txt"),
        "trim_start must not eat the filename: {}",
        status["files"]
    );
    h.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn rename_reports_new_path_and_old_path() {
    let h = Harness::owned().await;
    git(&h.repo, &["mv", "tracked.txt", "renamed.txt"]).await;
    let status = status_of(&h.client, &h.repo).await;
    let files = status["files"].as_array().expect("files");
    let renamed = files
        .iter()
        .find(|file| file["index"] == "renamed" || file["worktree"] == "renamed")
        .unwrap_or_else(|| panic!("missing rename in {}", status["files"]));
    assert_eq!(renamed["path"], "renamed.txt", "{renamed}");
    assert_eq!(renamed["oldPath"], "tracked.txt", "{renamed}");
    h.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn literal_pathspec_does_not_glob_star_rs() {
    let h = Harness::owned().await;
    std::fs::write(h.repo.join("foo.rs"), "foo\n").unwrap();
    std::fs::write(h.repo.join("bar.rs"), "bar\n").unwrap();
    std::fs::write(h.repo.join("*.rs"), "literal glob name\n").unwrap();
    git(&h.repo, &["add", "foo.rs", "bar.rs", "--", "*.rs"]).await;
    git(&h.repo, &["commit", "-m", "add rs files"]).await;
    std::fs::write(h.repo.join("foo.rs"), "foo-changed\n").unwrap();
    std::fs::write(h.repo.join("bar.rs"), "bar-changed\n").unwrap();
    std::fs::write(h.repo.join("*.rs"), "literal-changed\n").unwrap();

    h.client
        .call(
            methods::STAGE_FILES,
            json!({ "cwd": h.repo.to_str().unwrap(), "paths": ["*.rs"] }),
        )
        .await
        .expect("stage literal *.rs");

    let staged = git_stdout(&h.repo, &["diff", "--cached", "--name-only", "-z"]).await;
    let staged_names: Vec<&str> = staged.split('\0').filter(|name| !name.is_empty()).collect();
    assert_eq!(staged_names, ["*.rs"], "staged names: {staged_names:?}");
    h.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn failed_batch_discard_does_not_delete_untracked_first() {
    let h = Harness::owned().await;
    let scratch = h.repo.join("scratch.txt");
    std::fs::write(&scratch, "keep-me\n").unwrap();
    std::fs::write(h.repo.join("tracked.txt"), "dirty\n").unwrap();

    let err = h
        .client
        .call(
            methods::DISCARD_FILES,
            json!({
                "cwd": h.repo.to_str().unwrap(),
                "paths": ["scratch.txt", "missing-tracked.txt"]
            }),
        )
        .await
        .expect_err("restore of missing path must fail the batch");
    assert!(
        scratch.exists(),
        "untracked file must survive a failed batch discard: {err}"
    );
    assert_eq!(std::fs::read_to_string(&scratch).unwrap(), "keep-me\n");
    h.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn discard_matches_untracked_directory_reported_with_trailing_slash() {
    let h = Harness::owned().await;
    let dir = h.repo.join("udir");
    std::fs::create_dir(&dir).unwrap();
    std::fs::write(dir.join("inside.txt"), "x\n").unwrap();

    h.client
        .call(
            methods::DISCARD_FILES,
            json!({ "cwd": h.repo.to_str().unwrap(), "paths": ["udir"] }),
        )
        .await
        .expect("discard untracked directory");

    assert!(!dir.exists(), "untracked directory must be removed");
    h.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn diff_captures_survive_stale_index_lock() {
    let h = Harness::owned().await;
    std::fs::write(h.repo.join("tracked.txt"), "dirty\n").unwrap();
    std::fs::write(h.repo.join(".git/index.lock"), "stale\n").unwrap();
    zeron_engine::capture_diff(&h.core.repos, &h.repo)
        .await
        .expect("diff capture must not fail on a stale index.lock");
    h.shutdown().await;
}

/// A Workers state whose only checkout is `checkout`, linked to `space_id`.
fn write_worker_checkout(state: &Path, checkout: &Path, space_id: Option<&str>) {
    let mut entry = json!({
        "projectID": "comet-worker",
        "path": std::fs::canonicalize(checkout).unwrap(),
        "kind": "linked",
    });
    if let Some(space_id) = space_id {
        entry["spaceID"] = json!(space_id);
    }
    std::fs::write(
        state,
        json!({
            "projects": [{"id": "comet-worker", "path": checkout}],
            "comet_project_identity": {"version": 1, "checkouts": [entry]},
        })
        .to_string(),
    )
    .unwrap();
}

/// Engine whose Source Control reads the given Workers state, plus a local
/// project (Space) at `project_folder`, which is not the Worker checkout.
async fn worker_engine(
    data: &Path,
    state: &Path,
    project_folder: &Path,
) -> (EngineCore, zeron_rpc::RpcClient, String) {
    let core = assemble(data);
    let service = Arc::try_unwrap(core.rpc_service())
        .ok()
        .unwrap()
        .with_worker_checkouts(state.to_path_buf());
    let client = zeron_rpc::memory_client(Arc::new(service));
    let device = client.call(methods::LOCAL_DEVICE, json!({})).await.unwrap()["deviceId"]
        .as_str()
        .unwrap()
        .to_owned();
    std::fs::create_dir_all(project_folder).unwrap();
    let space_id = "space-worker-project".to_owned();
    client
        .call(
            methods::MUTATE,
            json!({"op": "createSpace", "spaceId": space_id, "deviceId": device,
                   "path": project_folder, "gitDetected": false}),
        )
        .await
        .unwrap();
    (core, client, space_id)
}

/// Scenario "Source Control in a Worker worktree": a linked worktree checkout
/// of a local project is authorized through its project link.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn source_control_authorizes_a_worker_worktree_of_a_local_project() {
    let temp = tempfile::tempdir().unwrap();
    let repo = temp.path().join("worker");
    init_repo(&repo).await;
    let worktree = temp.path().join("worker-feature");
    git(
        &repo,
        &[
            "worktree",
            "add",
            "-b",
            "feature",
            worktree.to_str().unwrap(),
        ],
    )
    .await;
    std::fs::write(worktree.join("tracked.txt"), "worker change\n").unwrap();
    let state = temp.path().join("workers.json");
    let (core, client, space_id) = worker_engine(
        &temp.path().join("data"),
        &state,
        &temp.path().join("project"),
    )
    .await;
    let params = json!({"cwd": worktree, "paths": ["tracked.txt"]});

    // Listed in the Workers state but linked to no project of this device.
    write_worker_checkout(&state, &worktree, None);
    assert!(
        client
            .call(methods::STAGE_FILES, params.clone())
            .await
            .is_err()
    );
    write_worker_checkout(&state, &worktree, Some("space-of-another-registry"));
    assert!(
        client
            .call(methods::STAGE_FILES, params.clone())
            .await
            .is_err()
    );

    write_worker_checkout(&state, &worktree, Some(&space_id));
    let status = status_of(&client, &worktree).await;
    assert!(
        status["files"]
            .as_array()
            .unwrap()
            .iter()
            .any(|file| file["path"] == "tracked.txt")
    );
    client
        .call(methods::STAGE_FILES, params.clone())
        .await
        .unwrap();
    assert_eq!(
        git_stdout(&worktree, &["diff", "--cached", "--name-only"])
            .await
            .trim(),
        "tracked.txt"
    );
    core.shutdown().await;
}

/// Scenario "Source Control in an unlinked folder": a Git folder that belongs
/// to no local project is rejected as unknown on this device.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn source_control_rejects_a_folder_with_no_project() {
    let temp = tempfile::tempdir().unwrap();
    let repo = temp.path().join("stray");
    init_repo(&repo).await;
    std::fs::write(repo.join("tracked.txt"), "stray change\n").unwrap();
    let state = temp.path().join("workers.json");
    let (core, client, _) = worker_engine(
        &temp.path().join("data"),
        &state,
        &temp.path().join("project"),
    )
    .await;
    let error = client
        .call(
            methods::STAGE_FILES,
            json!({"cwd": repo, "paths": ["tracked.txt"]}),
        )
        .await
        .unwrap_err();
    assert!(
        error.to_string().contains("not a known checkout"),
        "{error}"
    );
    core.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn commit_message_requires_authorized_checkout_and_staged_changes() {
    let h = Harness::owned().await;
    let foreign = tempfile::tempdir().unwrap();
    let error = h
        .client
        .call(
            methods::GENERATE_COMMIT_MESSAGE,
            json!({"cwd": foreign.path().to_str().unwrap()}),
        )
        .await
        .unwrap_err();
    assert!(
        error.to_string().contains("not a known checkout"),
        "{error}"
    );
    let error = h
        .client
        .call(
            methods::GENERATE_COMMIT_MESSAGE,
            json!({"cwd": h.repo.to_str().unwrap()}),
        )
        .await
        .unwrap_err();
    assert!(error.to_string().contains("Stage changes"), "{error}");
    h.shutdown().await;
}
