//! Upstream #514 workspace entry mutations and absolute-path reads, over the
//! real in-memory transport.

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use zeron_engine::{EngineCore, HarnessRegistry};
use zeron_proto::{WorkspaceDirectoryPage, WorkspaceFileChanges, WorkspaceFileText};
use zeron_rpc::methods;

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
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

async fn init_repo(path: &Path) {
    std::fs::create_dir_all(path.join("src/nested")).expect("repo tree");
    git(path, &["init", "-b", "main"]).await;
    std::fs::write(path.join("README.md"), "hello\n").expect("readme");
    std::fs::write(path.join("src/lib.rs"), "pub fn answer() -> u8 { 42 }\n").expect("source");
    std::fs::write(path.join("src/nested/mod.rs"), "pub mod child;\n").expect("nested source");
    git(path, &["add", "."]).await;
    git(path, &["commit", "-m", "initial"]).await;
}

fn assemble(data_dir: &Path, device_id: &str) -> EngineCore {
    std::fs::create_dir_all(data_dir).expect("data dir");
    std::fs::write(data_dir.join("device-id"), device_id).expect("device id");
    EngineCore::assemble(
        data_dir,
        Arc::new(HarnessRegistry::new()),
        zeron_proto::HarnessId::Mock,
        None,
    )
    .expect("engine assembles")
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn workspace_mutations_validate_revision_and_publish_semantic_events() {
    let temp = tempfile::tempdir().unwrap();
    let repo = temp.path().join("repo");
    init_repo(&repo).await;
    let core = assemble(&temp.path().join("data"), "device-mutations");
    core.workspace
        .create_space(
            "space",
            &core.device_id,
            &repo.to_string_lossy(),
            None,
            true,
        )
        .unwrap();
    core.workspace
        .create_chat("chat", Some("space"), None, None, None)
        .unwrap();
    let client = zeron_rpc::memory_client(core.rpc_service());
    let page: WorkspaceDirectoryPage = serde_json::from_value(
        client
            .call(
                methods::LIST_WORKSPACE_DIRECTORY,
                serde_json::json!({"chatId":"chat"}),
            )
            .await
            .unwrap(),
    )
    .unwrap();
    assert!(page.mutation_capabilities.unwrap().delete_entry);
    let revision = page
        .entries
        .iter()
        .find(|e| e.path == "README.md")
        .unwrap()
        .mutation_revision
        .clone()
        .unwrap();
    let checkout = page.checkout_id.unwrap();
    let mut watch = client
        .subscribe(
            methods::WATCH_WORKSPACE_FILES,
            serde_json::json!({"chatId":"chat"}),
        )
        .await
        .unwrap();
    watch.recv().await.unwrap();
    let mut request = serde_json::json!({"chatId":"chat", "operationId":"rename", "expectedCheckoutId":checkout, "sourcePath":"README.md", "destinationPath":"src/read me.md", "expectedSourceRevision":revision, "expectedKind":"file"});
    request["expectedCheckoutId"] = "wrong".into();
    let rejected = client
        .call(methods::MOVE_WORKSPACE_ENTRY, request.clone())
        .await
        .unwrap();
    assert_eq!(rejected["reason"], "workspaceChanged");
    request["expectedCheckoutId"] = checkout.clone().into();
    let moved = client
        .call(methods::MOVE_WORKSPACE_ENTRY, request)
        .await
        .unwrap();
    assert_eq!(moved["status"], "applied");
    assert!(!repo.join("README.md").exists());
    assert_eq!(
        std::fs::read_to_string(repo.join("src/read me.md")).unwrap(),
        "hello\n"
    );
    tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            let frame: WorkspaceFileChanges =
                serde_json::from_value(watch.recv().await.unwrap()).unwrap();
            if frame.changes.iter().any(|c| {
                c.operation_id.as_deref() == Some("rename")
                    && c.old_path.as_deref() == Some("README.md")
            }) {
                break;
            }
        }
    })
    .await
    .unwrap();
    let deleted = client.call(methods::DELETE_WORKSPACE_ENTRY, serde_json::json!({"chatId":"chat", "operationId":"delete", "expectedCheckoutId":checkout, "path":"src/read me.md", "expectedSourceRevision":moved["entry"]["mutationRevision"], "expectedKind":"file", "recursive":false})).await.unwrap();
    assert_eq!(deleted["status"], "applied");
    assert!(!repo.join("src/read me.md").exists());
}

// Absolute reads take POSIX paths — the only shape a UI sends.
#[cfg(unix)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn absolute_paths_read_inside_normally_and_outside_read_only() {
    let temp = tempfile::tempdir().expect("tempdir");
    let repo = temp.path().join("repo");
    init_repo(&repo).await;
    let outside = temp.path().join("outside");
    std::fs::create_dir_all(&outside).expect("outside dir");
    std::fs::write(outside.join("report.md"), "outside\n").expect("outside file");
    std::fs::write(outside.join("pixel.png"), b"\x89PNG\r\n\x1a\nfake").expect("png");
    let core = assemble(&temp.path().join("data"), "device-absolute");
    core.workspace
        .create_space(
            "space-absolute",
            &core.device_id,
            &repo.to_string_lossy(),
            None,
            true,
        )
        .expect("space");
    core.workspace
        .create_chat("chat-absolute", Some("space-absolute"), None, None, None)
        .expect("chat");
    let client = zeron_rpc::memory_client(core.rpc_service());

    // An absolute path inside the chat root reads like its relative
    // equivalent — checkout-bound and editable.
    let inside_abs = std::fs::canonicalize(repo.join("README.md"))
        .unwrap()
        .to_string_lossy()
        .into_owned();
    let read: WorkspaceFileText = serde_json::from_value(
        client
            .call(
                methods::READ_WORKSPACE_FILE,
                serde_json::json!({ "chatId": "chat-absolute", "path": inside_abs }),
            )
            .await
            .expect("inside absolute read"),
    )
    .expect("typed inside read");
    assert_eq!(read.text.as_deref(), Some("hello\n"));
    assert_eq!(read.path, "README.md");
    assert!(read.read_only_reason.is_none());
    assert!(!read.checkout_id.is_empty());
    let written = client
        .call(
            methods::WRITE_WORKSPACE_FILE,
            serde_json::json!({
                "chatId": "chat-absolute",
                "path": "README.md",
                "text": "inside write\n",
                "expectedCheckoutId": read.checkout_id,
                "expectedContentHash": read.content_hash,
                "encoding": "utf8",
                "lineEnding": "lf",
            }),
        )
        .await
        .expect("inside write");
    assert_eq!(written["status"], "written");

    // The same file outside every root is a read-only host file with no
    // checkout identity.
    let outside_abs = std::fs::canonicalize(outside.join("report.md"))
        .unwrap()
        .to_string_lossy()
        .into_owned();
    let read: WorkspaceFileText = serde_json::from_value(
        client
            .call(
                methods::READ_WORKSPACE_FILE,
                serde_json::json!({ "chatId": "chat-absolute", "path": outside_abs }),
            )
            .await
            .expect("outside read"),
    )
    .expect("typed outside read");
    assert_eq!(read.text.as_deref(), Some("outside\n"));
    assert_eq!(
        read.read_only_reason,
        Some(zeron_proto::WorkspaceReadOnlyReason::OutsideWorkspace)
    );
    assert!(read.checkout_id.is_empty());

    // Missing outside files report NotFound; directories report
    // not-a-regular-file; images read like workspace images.
    let missing = outside.join("missing.md").to_string_lossy().into_owned();
    let error = client
        .call(
            methods::READ_WORKSPACE_FILE,
            serde_json::json!({ "chatId": "chat-absolute", "path": missing }),
        )
        .await
        .expect_err("missing outside read");
    assert!(error.to_string().contains("not found"), "{error}");
    let directory: WorkspaceFileText = serde_json::from_value(
        client
            .call(
                methods::READ_WORKSPACE_FILE,
                serde_json::json!({
                    "chatId": "chat-absolute",
                    "path": outside.to_string_lossy(),
                }),
            )
            .await
            .expect("directory read"),
    )
    .expect("typed directory read");
    assert_eq!(
        directory.read_only_reason,
        Some(zeron_proto::WorkspaceReadOnlyReason::NotRegularFile)
    );
    let png_abs = std::fs::canonicalize(outside.join("pixel.png"))
        .unwrap()
        .to_string_lossy()
        .into_owned();
    let image = client
        .call(
            methods::READ_WORKSPACE_IMAGE,
            serde_json::json!({
                "chatId": "chat-absolute",
                "path": png_abs,
                "expectedCheckoutId": "",
                "offset": 0,
            }),
        )
        .await
        .expect("outside image read");
    assert_eq!(image["mimeType"], "image/png");
    assert_eq!(image["done"], true);

    // Writes and space targets keep rejecting absolute paths.
    assert!(
        client
            .call(
                methods::WRITE_WORKSPACE_FILE,
                serde_json::json!({
                    "chatId": "chat-absolute",
                    "path": outside_abs,
                    "text": "nope\n",
                    "expectedCheckoutId": "anything",
                    "expectedContentHash": "anything",
                    "encoding": "utf8",
                    "lineEnding": "lf",
                }),
            )
            .await
            .is_err()
    );
    assert!(
        client
            .call(
                methods::READ_WORKSPACE_FILE,
                serde_json::json!({ "spaceId": "space-absolute", "path": outside_abs }),
            )
            .await
            .is_err()
    );
    core.shutdown().await;
}
