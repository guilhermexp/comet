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
async fn rename_preserves_content_and_refuses_collision() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("project");
    std::fs::create_dir(&root).unwrap();
    std::fs::write(root.join("a.txt"), "hello").unwrap();
    std::fs::write(root.join("b.txt"), "other").unwrap();
    let core = assemble(&temp.path().join("data"));
    create_owned_space(&core, &root);
    let client = zeron_rpc::memory_client(core.rpc_service());

    let renamed = client
        .call(
            "RenameWorkspaceEntry",
            json!({
                "spaceId": "files",
                "path": "a.txt",
                "newName": "renamed.txt",
            }),
        )
        .await
        .expect("rename");
    assert_eq!(renamed["path"], "renamed.txt");
    assert!(!root.join("a.txt").exists());
    assert_eq!(std::fs::read(root.join("renamed.txt")).unwrap(), b"hello");

    let collision = client
        .call(
            "RenameWorkspaceEntry",
            json!({
                "spaceId": "files",
                "path": "renamed.txt",
                "newName": "b.txt",
            }),
        )
        .await
        .expect_err("rename collision");
    assert!(collision.to_string().contains("exist"), "{collision}");
    assert_eq!(std::fs::read(root.join("renamed.txt")).unwrap(), b"hello");
    assert_eq!(std::fs::read(root.join("b.txt")).unwrap(), b"other");
    core.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn delete_removes_directory_recursively() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("project");
    std::fs::create_dir_all(root.join("folder/child")).unwrap();
    std::fs::write(root.join("folder/child/file.txt"), "gone").unwrap();
    let core = assemble(&temp.path().join("data"));
    create_owned_space(&core, &root);
    let client = zeron_rpc::memory_client(core.rpc_service());

    let deleted = client
        .call(
            "DeleteWorkspaceEntry",
            json!({
                "spaceId": "files",
                "path": "folder",
            }),
        )
        .await
        .expect("delete");
    assert_eq!(deleted["path"], "folder");
    assert_eq!(deleted["isDirectory"], true);
    assert!(!root.join("folder").exists());
    core.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn move_relocates_file_between_folders() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("project");
    std::fs::create_dir_all(root.join("src/util")).unwrap();
    std::fs::write(root.join("src/a.rs"), "fn a() {}").unwrap();
    let core = assemble(&temp.path().join("data"));
    create_owned_space(&core, &root);
    let client = zeron_rpc::memory_client(core.rpc_service());

    let moved = client
        .call(
            "MoveWorkspaceEntry",
            json!({
                "spaceId": "files",
                "sourcePath": "src/a.rs",
                "destinationDirectory": "src/util",
            }),
        )
        .await
        .expect("move");
    assert_eq!(moved["path"], "src/util/a.rs");
    assert!(!root.join("src/a.rs").exists());
    assert_eq!(
        std::fs::read(root.join("src/util/a.rs")).unwrap(),
        b"fn a() {}"
    );
    core.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn copy_collision_gets_unique_name() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("project");
    std::fs::create_dir(&root).unwrap();
    std::fs::write(root.join("a.txt"), "original").unwrap();
    let core = assemble(&temp.path().join("data"));
    create_owned_space(&core, &root);
    let client = zeron_rpc::memory_client(core.rpc_service());

    let copied = client
        .call(
            "CopyWorkspaceEntry",
            json!({
                "spaceId": "files",
                "sourcePath": "a.txt",
                "destinationDirectory": "",
            }),
        )
        .await
        .expect("copy");
    assert_eq!(copied["path"], "a copy.txt");
    assert_eq!(std::fs::read(root.join("a.txt")).unwrap(), b"original");
    assert_eq!(std::fs::read(root.join("a copy.txt")).unwrap(), b"original");
    core.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn paste_folder_into_itself_is_refused() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("project");
    std::fs::create_dir_all(root.join("docs/adr")).unwrap();
    std::fs::write(root.join("docs/adr/note.md"), "keep").unwrap();
    let core = assemble(&temp.path().join("data"));
    create_owned_space(&core, &root);
    let client = zeron_rpc::memory_client(core.rpc_service());

    for method in ["MoveWorkspaceEntry", "CopyWorkspaceEntry"] {
        let err = client
            .call(
                method,
                json!({
                    "spaceId": "files",
                    "sourcePath": "docs",
                    "destinationDirectory": "docs/adr",
                }),
            )
            .await
            .expect_err("descendant paste");
        assert!(
            err.to_string().to_ascii_lowercase().contains("itself")
                || err.to_string().to_ascii_lowercase().contains("descendant"),
            "{method}: {err}"
        );
    }
    assert!(root.join("docs/adr/note.md").is_file());
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
        let rename = client
            .call(
                "RenameWorkspaceEntry",
                json!({
                    "spaceId": "files",
                    "path": path,
                    "newName": "x.txt",
                }),
            )
            .await
            .expect_err("escaped rename");
        assert!(
            rename.to_string().contains("path") || rename.to_string().contains("invalid"),
            "{rename}"
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
