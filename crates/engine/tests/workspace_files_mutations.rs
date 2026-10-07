//! Workspace file mutations exercised over the same RPC transport as the UI.
use serde_json::json;
use std::sync::Arc;
use zeron_engine::{EngineCore, HarnessRegistry};

fn assemble(data: &std::path::Path) -> EngineCore {
    EngineCore::assemble(
        data,
        Arc::new(HarnessRegistry::new()),
        zeron_proto::HarnessId::Mock,
        None,
    )
    .unwrap()
}

fn create_owned_space(core: &EngineCore, root: &std::path::Path) {
    core.workspace
        .create_space("files", &core.device_id, root.to_str().unwrap(), None, true)
        .unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn creates_empty_file_inside_selected_folder() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("project");
    std::fs::create_dir_all(root.join("notes")).unwrap();
    let core = assemble(&temp.path().join("data"));
    create_owned_space(&core, &root);
    let client = zeron_rpc::memory_client(core.rpc_service());

    let created = client
        .call(
            "CreateWorkspaceEntry",
            json!({
                "spaceId": "files",
                "parentPath": "notes",
                "name": "notes.md",
                "kind": "file",
            }),
        )
        .await
        .expect("create file");
    assert_eq!(created["path"], "notes/notes.md");
    assert_eq!(created["isDirectory"], false);
    let path = root.join("notes/notes.md");
    assert!(path.is_file());
    assert_eq!(std::fs::read(&path).unwrap(), b"");
    core.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn nested_create_name_makes_intermediate_directories() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("project");
    std::fs::create_dir(&root).unwrap();
    let core = assemble(&temp.path().join("data"));
    create_owned_space(&core, &root);
    let client = zeron_rpc::memory_client(core.rpc_service());

    let created = client
        .call(
            "CreateWorkspaceEntry",
            json!({
                "spaceId": "files",
                "parentPath": "",
                "name": "docs/adr/0001.md",
                "kind": "file",
            }),
        )
        .await
        .expect("nested create");
    assert_eq!(created["path"], "docs/adr/0001.md");
    assert!(root.join("docs").is_dir());
    assert!(root.join("docs/adr").is_dir());
    assert_eq!(std::fs::read(root.join("docs/adr/0001.md")).unwrap(), b"");
    core.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn path_escape_is_refused_and_writes_nothing_outside() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("project");
    std::fs::create_dir(&root).unwrap();
    let outside = temp.path().join("secret.txt");
    std::fs::write(&outside, "secret").unwrap();
    let core = assemble(&temp.path().join("data"));
    create_owned_space(&core, &root);
    let client = zeron_rpc::memory_client(core.rpc_service());

    for path in ["../secret.txt", "/etc/passwd", "src/../../secret.txt"] {
        let err = client
            .call(
                "CreateWorkspaceEntry",
                json!({
                    "spaceId": "files",
                    "parentPath": "",
                    "name": path,
                    "kind": "file",
                }),
            )
            .await
            .expect_err("escaped create");
        assert!(
            err.to_string().contains("path") || err.to_string().contains("invalid"),
            "{path}: {err}"
        );
    }
    assert_eq!(std::fs::read(&outside).unwrap(), b"secret");
    assert!(std::fs::read_dir(&root).unwrap().next().is_none());
    core.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn foreign_device_is_refused() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("project");
    std::fs::create_dir(&root).unwrap();
    let core = assemble(&temp.path().join("data"));
    core.workspace
        .create_space(
            "foreign",
            "other-device",
            root.to_str().unwrap(),
            None,
            true,
        )
        .unwrap();
    let client = zeron_rpc::memory_client(core.rpc_service());
    let err = client
        .call(
            "CreateWorkspaceEntry",
            json!({
                "spaceId": "foreign",
                "parentPath": "",
                "name": "x.txt",
                "kind": "file",
            }),
        )
        .await
        .expect_err("foreign create");
    assert!(err.to_string().contains("another device"), "{err}");
    assert!(!root.join("x.txt").exists());
    core.shutdown().await;
}
