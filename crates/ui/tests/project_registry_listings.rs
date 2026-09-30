//! One registry, three listings: the chat MCP `list_projects`, the Workers
//! controller MCP `list_projects` and the Settings → Projects row builder
//! read the same engine and must name the same projects. Runs in its own
//! process: it points `UNPEEL_HOME` at a temporary Workers home.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde_json::{Value, json};
use zeron_engine::{EngineCore, HarnessRegistry};
use zeron_proto::{Chat, Device, Space};
use zeron_rpc::methods;
use zeron_ui::settings::project_catalog::{project_entries, rename_project_params};
use zeron_workers_unpeel::LocalWorkersClient;
use zeron_workers_unpeel::project_ledger;

fn git(cwd: &Path, args: &[&str]) {
    let status = std::process::Command::new("git")
        .args(args)
        .current_dir(cwd)
        .env("GIT_AUTHOR_NAME", "t")
        .env("GIT_AUTHOR_EMAIL", "t@t")
        .env("GIT_COMMITTER_NAME", "t")
        .env("GIT_COMMITTER_EMAIL", "t@t")
        .status()
        .unwrap();
    assert!(status.success(), "git {args:?}");
}

fn repo(path: &Path) -> PathBuf {
    std::fs::create_dir_all(path).unwrap();
    git(path, &["init", "-q", "-b", "main"]);
    git(path, &["config", "commit.gpgsign", "false"]);
    std::fs::write(path.join("README.md"), "fixture\n").unwrap();
    git(path, &["add", "README.md"]);
    git(path, &["commit", "-q", "-m", "fixture"]);
    std::fs::canonicalize(path).unwrap()
}

async fn snapshot<T: serde::de::DeserializeOwned>(
    client: &zeron_rpc::RpcClient,
    method: &str,
) -> T {
    let mut stream = client.subscribe_scoped(method, json!({})).await.unwrap();
    let item = tokio::time::timeout(std::time::Duration::from_secs(5), stream.recv())
        .await
        .unwrap()
        .unwrap();
    serde_json::from_value(item).unwrap()
}

async fn workers(arguments: Value) -> Value {
    let response = tokio::task::spawn_blocking(move || {
        zeron_workers_unpeel::controller_mcp_handle_request(json!({
            "jsonrpc": "2.0", "id": 1, "method": "tools/call",
            "params": { "name": "workers", "arguments": arguments }
        }))
        .unwrap()
    })
    .await
    .unwrap();
    assert_eq!(response["result"]["isError"], false, "{response}");
    response["result"]["structuredContent"].clone()
}

/// `id -> (name, path, device id)` of a listing.
type Listing = BTreeMap<String, (String, String, String)>;

fn listing(projects: &Value, name: &str, path: &str, device: &str) -> Listing {
    projects
        .as_array()
        .unwrap()
        .iter()
        .map(|project| {
            (
                project["id"].as_str().unwrap().to_owned(),
                (
                    project[name].as_str().unwrap().to_owned(),
                    project[path].as_str().unwrap().to_owned(),
                    project[device].as_str().unwrap().to_owned(),
                ),
            )
        })
        .collect()
}

/// Scenarios "Every project listing agrees", "The list matches the chat MCP"
/// and "Renaming a remote project".
#[tokio::test(flavor = "multi_thread")]
async fn chat_mcp_workers_controller_and_settings_list_the_same_projects() {
    let root = tempfile::tempdir().unwrap();
    let home = root.path().join("unpeel");
    std::fs::create_dir_all(&home).unwrap();
    std::fs::write(
        home.join("app-state.json"),
        json!({"projects": [], "presets": [], "active_tabs": {}, "pinned_sessions": {}})
            .to_string(),
    )
    .unwrap();
    // SAFETY: this test binary holds one test; nothing else reads the env.
    unsafe {
        std::env::set_var("UNPEEL_HOME", &home);
        std::env::set_var("ZERON_WORKTREES_DIR", home.join("worktrees"));
        std::env::set_var("ZERON_WORKTREE_OWNERSHIP_FILE", home.join("ownership.json"));
        std::env::set_var("COMET_WORKERS_HOOKS_DIR", home.join("hooks"));
    }

    let core = EngineCore::assemble(
        &root.path().join("data"),
        Arc::new(HarnessRegistry::new()),
        zeron_proto::HarnessId::Mock,
        None,
    )
    .unwrap();
    let client = zeron_rpc::memory_client(core.rpc_service());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = format!("ws://{}", listener.local_addr().unwrap());
    let service: Arc<dyn zeron_rpc::RpcService> = core.rpc_service();
    let _server = tokio::spawn(zeron_rpc::serve_ws_listener(listener, service));
    // SAFETY: as above.
    unsafe { std::env::set_var("COMET_WORKERS_ENGINE_ENDPOINT", &endpoint) };
    let local = core.device_id.clone();

    // A project of another device, and local ones: one added through
    // Workers (with a linked worktree) and one created by the Orchestrator.
    core.workspace.upsert_device_row(&Device {
        id: "dev-mini".into(),
        name: "Mac mini".into(),
        platform: "macos".into(),
        last_seen_at: None,
        created_at: None,
        version: None,
        cursor_sdk_version: None,
        capabilities: Vec::new(),
    });
    client
        .call(
            methods::MUTATE,
            json!({"op": "createSpace", "spaceId": "space-craft",
            "deviceId": "dev-mini", "path": "/Users/mini/craft-agents-oss"}),
        )
        .await
        .unwrap();
    let orchestrator = repo(&root.path().join("orchestrator"));
    client
        .call(
            methods::MUTATE,
            json!({"op": "createSpace", "spaceId": "space-orch",
            "deviceId": local, "path": orchestrator}),
        )
        .await
        .unwrap();
    let jk = repo(&root.path().join("JK Distribuição"));
    let sec = root.path().join("sec-cron");
    git(
        &jk,
        &[
            "worktree",
            "add",
            "-q",
            "-b",
            "sec/cron",
            sec.to_str().unwrap(),
        ],
    );
    workers(json!({"action": "add_project", "path": jk})).await;
    workers(json!({"action": "add_project", "path": sec})).await;
    // Workers adds the Orchestrator's folder: same project, not a new one.
    let reused = workers(json!({"action": "add_project", "path": orchestrator})).await;
    assert_eq!(reused["project_id"], "space-orch");

    let chat = zeron_mcp::Tools::new(Arc::new(zeron_mcp::Zeron::new(
        endpoint.clone(),
        zeron_mcp::Origin::default(),
    )));
    let chat_projects = chat.call("list_projects", json!({})).await.unwrap();
    let chat_listing = listing(&chat_projects["projects"], "name", "path", "deviceId");
    let controller = workers(json!({"action": "list_projects"})).await;
    let controller_listing = listing(&controller["projects"], "name", "path", "device_id");

    let spaces: Vec<Space> = snapshot(&client, methods::WATCH_SPACES).await;
    let devices: Vec<Device> = snapshot(&client, methods::WATCH_DEVICES).await;
    let chats: Vec<Chat> = snapshot(&client, methods::WATCH_CHATS).await;
    let workers_client = LocalWorkersClient::new();
    let rows = tokio::task::spawn_blocking(move || {
        let rows = workers_client.projects_with_ledger().unwrap();
        let identity = workers_client.project_identity_registry().unwrap();
        project_ledger::decorate_with_identity(rows, &identity)
    })
    .await
    .unwrap();
    let (entries, pending) = project_entries(&spaces, &devices, Some(&local), &chats, &rows);
    let settings_listing: Listing = entries
        .iter()
        .map(|entry| {
            (
                entry.space.id.clone(),
                (
                    entry.space.name.clone(),
                    entry.space.path.clone(),
                    entry.space.device_id.clone(),
                ),
            )
        })
        .collect();

    assert_eq!(chat_listing.len(), 3, "{chat_projects}");
    assert_eq!(controller_listing, chat_listing);
    assert_eq!(settings_listing, chat_listing);
    assert!(
        pending.is_empty(),
        "no Workers registration without a project"
    );
    let jk_entry = entries
        .iter()
        .find(|entry| entry.space.path == jk.to_string_lossy())
        .unwrap();
    assert_eq!(jk_entry.group.checkouts.len(), 2, "principal and worktree");

    // Renaming the remote project from Settings reaches the chat MCP.
    client
        .call(
            methods::MUTATE,
            rename_project_params("space-craft", "Craft (mini)"),
        )
        .await
        .unwrap();
    let renamed = chat.call("list_projects", json!({})).await.unwrap();
    let craft = renamed["projects"]
        .as_array()
        .unwrap()
        .iter()
        .find(|project| project["id"] == "space-craft")
        .cloned()
        .unwrap();
    assert_eq!(craft["name"], "Craft (mini)");
    core.shutdown().await;
}
