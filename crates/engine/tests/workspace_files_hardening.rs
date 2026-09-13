//! Hardening of workspace file mutations: no silent overwrite, no cleanup of
//! a destination the copy did not create, depth cap, and Windows name rules.
use serde_json::json;
use std::sync::Arc;
use zeron_engine::{EngineCore, HarnessRegistry};
use zeron_proto::{WorkspaceNameError, validate_workspace_component};

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
        .unwrap()
}

async fn call(
    client: &zeron_rpc::RpcClient,
    method: &str,
    params: serde_json::Value,
) -> Result<serde_json::Value, zeron_rpc::RpcError> {
    client.call(method, params).await
}

fn nest_directories(root: &std::path::Path, levels: usize) {
    let mut path = root.to_path_buf();
    std::fs::create_dir_all(&path).unwrap();
    for _ in 1..levels {
        path.push("d");
        std::fs::create_dir(&path).unwrap();
    }
    std::fs::write(path.join("leaf.txt"), "bottom").unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn copy_create_dir_collision_does_not_delete_existing_directory() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("project");
    std::fs::create_dir(&root).unwrap();
    let nfd = "cafe\u{0301}";
    let nfc = "caf\u{00e9}";
    std::fs::create_dir(root.join(nfd)).unwrap();
    std::fs::write(root.join(nfd).join("keep.txt"), "precious").unwrap();
    std::fs::create_dir_all(root.join("from").join(nfc)).unwrap();
    std::fs::write(root.join("from").join(nfc).join("file.txt"), "payload").unwrap();

    let core = assemble(&temp.path().join("data"));
    create_owned_space(&core, &root);
    let client = zeron_rpc::memory_client(core.rpc_service());

    let result = call(
        &client,
        "CopyWorkspaceEntry",
        json!({
            "spaceId": "files",
            "sourcePath": format!("from/{nfc}"),
            "destinationDirectory": "",
        }),
    )
    .await;

    assert_eq!(
        std::fs::read(root.join(nfd).join("keep.txt")).unwrap(),
        b"precious",
        "existing destination must survive a colliding create_dir",
    );
    let probe = temp.path().join("probe");
    std::fs::create_dir(&probe).unwrap();
    std::fs::create_dir(probe.join(nfd)).unwrap();
    let names_collide = std::fs::create_dir(probe.join(nfc)).is_err();
    if names_collide {
        let error = result.expect_err("colliding copy must not succeed");
        assert!(
            error.to_string().contains("exist"),
            "collision must be reported, got {error}"
        );
    }
    core.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn rename_refuses_existing_destination_instead_of_overwriting() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("project");
    std::fs::create_dir(&root).unwrap();
    std::fs::write(root.join("Ä.txt"), "original").unwrap();
    std::fs::write(root.join("other.txt"), "moved").unwrap();
    let core = assemble(&temp.path().join("data"));
    create_owned_space(&core, &root);
    let client = zeron_rpc::memory_client(core.rpc_service());

    let error = call(
        &client,
        "RenameWorkspaceEntry",
        json!({
            "spaceId": "files",
            "path": "other.txt",
            "newName": "ä.txt",
        }),
    )
    .await
    .expect_err("unicode-case collision must be refused");
    assert!(
        error.to_string().contains("exist"),
        "collision message, got {error}"
    );
    assert_eq!(std::fs::read(root.join("Ä.txt")).unwrap(), b"original");
    assert_eq!(std::fs::read(root.join("other.txt")).unwrap(), b"moved");
    core.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn move_refuses_existing_destination_instead_of_overwriting() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("project");
    std::fs::create_dir_all(root.join("dest")).unwrap();
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(root.join("dest/Ä.txt"), "original").unwrap();
    std::fs::write(root.join("src/ä.txt"), "moved").unwrap();
    let core = assemble(&temp.path().join("data"));
    create_owned_space(&core, &root);
    let client = zeron_rpc::memory_client(core.rpc_service());

    let error = call(
        &client,
        "MoveWorkspaceEntry",
        json!({
            "spaceId": "files",
            "sourcePath": "src/ä.txt",
            "destinationDirectory": "dest",
        }),
    )
    .await
    .expect_err("unicode-case move collision must be refused");
    assert!(
        error.to_string().contains("exist"),
        "collision message, got {error}"
    );
    assert_eq!(std::fs::read(root.join("dest/Ä.txt")).unwrap(), b"original");
    assert_eq!(std::fs::read(root.join("src/ä.txt")).unwrap(), b"moved");
    core.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn copy_refuses_tree_deeper_than_component_limit() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("project");
    nest_directories(&root.join("deep"), 257);
    let core = assemble(&temp.path().join("data"));
    create_owned_space(&core, &root);
    let client = zeron_rpc::memory_client(core.rpc_service());

    let error = call(
        &client,
        "CopyWorkspaceEntry",
        json!({
            "spaceId": "files",
            "sourcePath": "deep",
            "destinationDirectory": "",
        }),
    )
    .await
    .expect_err("over-deep copy must be refused");
    let message = error.to_string();
    assert!(
        message.contains("deep"),
        "depth error must mention depth, got {message}"
    );
    assert!(
        !root.join("deep copy").exists(),
        "failed copy must not leave a destination tree"
    );
    assert!(root.join("deep").is_dir());
    core.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn delete_refuses_tree_deeper_than_component_limit() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("project");
    nest_directories(&root.join("deep"), 257);
    let core = assemble(&temp.path().join("data"));
    create_owned_space(&core, &root);
    let client = zeron_rpc::memory_client(core.rpc_service());

    let error = call(
        &client,
        "DeleteWorkspaceEntry",
        json!({
            "spaceId": "files",
            "path": "deep",
        }),
    )
    .await
    .expect_err("over-deep delete must be refused");
    let message = error.to_string();
    assert!(
        message.contains("deep"),
        "depth error must mention depth, got {message}"
    );
    assert!(
        root.join("deep").is_dir(),
        "refused delete must leave the tree in place"
    );
    core.shutdown().await;
}

#[test]
fn validate_workspace_component_rejects_windows_reserved_and_trailing_junk() {
    for name in ["foo.", "foo ", "CON"] {
        assert_eq!(
            validate_workspace_component(name),
            Err(WorkspaceNameError::InvalidComponent),
            "{name:?}"
        );
    }
    assert_eq!(validate_workspace_component("notes.md"), Ok(()));
}
