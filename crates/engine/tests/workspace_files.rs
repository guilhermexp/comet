//! Read-only Files contracts exercised over the same RPC transport as the UI.
use serde_json::json;
use std::{sync::Arc, time::Duration};
use zeron_engine::{EngineCore, HarnessRegistry};

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn files_rpc_pages_reads_searches_and_watches_the_owned_folder() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("project");
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(root.join("src/report.md"), "# Relatório ação\n").unwrap();
    for i in 0..503 {
        std::fs::write(root.join(format!("item-{i:04}.txt")), "item").unwrap();
    }
    let core = EngineCore::assemble(
        &temp.path().join("data"),
        Arc::new(HarnessRegistry::new()),
        zeron_proto::HarnessId::Mock,
        None,
    )
    .unwrap();
    core.workspace
        .create_space("files", &core.device_id, root.to_str().unwrap(), None, true)
        .unwrap();
    let client = zeron_rpc::memory_client(core.rpc_service());
    let first = client
        .call("ListWorkspaceDirectory", json!({"spaceId":"files"}))
        .await
        .expect("first directory page");
    assert_eq!(first["entries"].as_array().unwrap().len(), 500);
    assert_eq!(first["entries"][0]["path"], "src");
    let next = client
        .call(
            "ListWorkspaceDirectory",
            json!({"spaceId":"files","cursor":first["nextCursor"]}),
        )
        .await
        .unwrap();
    assert_eq!(next["entries"].as_array().unwrap().len(), 4);
    assert!(next["nextCursor"].is_null());
    let nested = client
        .call(
            "ListWorkspaceDirectory",
            json!({"spaceId":"files","directory":"src"}),
        )
        .await
        .unwrap();
    assert_eq!(nested["entries"].as_array().unwrap().len(), 1);
    let read = client
        .call(
            "ReadWorkspaceFile",
            json!({"spaceId":"files","path":"src/report.md"}),
        )
        .await
        .unwrap();
    assert_eq!(read["text"], "# Relatório ação\n");
    let search = client
        .call(
            "SearchWorkspaceFiles",
            json!({"spaceId":"files","query":"report"}),
        )
        .await
        .unwrap();
    assert_eq!(search[0]["path"], "src/report.md");
    for path in ["../secret.md", "/etc/passwd", "src/../../secret.md"] {
        assert!(
            client
                .call("ReadWorkspaceFile", json!({"spaceId":"files","path":path}))
                .await
                .is_err()
        );
    }
    let mut watch = client
        .subscribe("WatchWorkspaceFiles", json!({"spaceId":"files"}))
        .await
        .unwrap();
    let baseline = tokio::time::timeout(Duration::from_secs(3), watch.recv())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(baseline["resyncRequired"], true);
    std::fs::write(root.join("src/report.md"), "updated\n").unwrap();
    let update = tokio::time::timeout(Duration::from_secs(3), watch.recv())
        .await
        .unwrap()
        .unwrap();
    assert!(update["sequence"].as_u64().unwrap() > baseline["sequence"].as_u64().unwrap());
    assert!(
        update["changes"]
            .as_array()
            .unwrap()
            .iter()
            .any(|c| c["path"] == "src/report.md")
    );
    drop(watch);
    core.shutdown().await;
}

#[tokio::test]
async fn files_rpc_rejects_foreign_workspace_and_unregistered_checkout() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("project");
    std::fs::create_dir(&root).unwrap();
    let core = EngineCore::assemble(
        &temp.path().join("data"),
        Arc::new(HarnessRegistry::new()),
        zeron_proto::HarnessId::Mock,
        None,
    )
    .unwrap();
    core.workspace
        .create_space(
            "foreign",
            "other-device",
            root.to_str().unwrap(),
            None,
            true,
        )
        .unwrap();
    core.workspace
        .create_space("local", &core.device_id, root.to_str().unwrap(), None, true)
        .unwrap();
    let client = zeron_rpc::memory_client(core.rpc_service());
    let foreign = client
        .call("ListWorkspaceDirectory", json!({"spaceId":"foreign"}))
        .await
        .unwrap_err();
    assert!(foreign.to_string().contains("another device"), "{foreign}");
    let unrelated = client
        .call(
            "ListWorkspaceDirectory",
            json!({"spaceId":"local","checkoutPath":temp.path()}),
        )
        .await
        .unwrap_err();
    assert!(
        unrelated.to_string().contains("workspace checkout"),
        "{unrelated}"
    );
    core.shutdown().await;
}
