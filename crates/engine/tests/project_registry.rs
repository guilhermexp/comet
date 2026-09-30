//! One project registry: the chat MCP `list_projects` and the Workers
//! controller MCP `list_projects` read the same engine and must agree.
//! Every repository and Workers home is a temporary directory created here.
use serde_json::{Value, json};
use std::collections::BTreeSet;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use zeron_engine::{EngineCore, HarnessRegistry};
use zeron_rpc::methods;

/// The controller reads its Workers home and engine endpoint from the
/// process environment; every test that touches either holds this lock.
static ENV_LOCK: Mutex<()> = Mutex::new(());

struct EnvGuard(Vec<(&'static str, Option<OsString>)>);

impl EnvGuard {
    fn set(values: Vec<(&'static str, OsString)>) -> Self {
        let previous = values
            .iter()
            .map(|(key, _)| (*key, std::env::var_os(key)))
            .collect();
        for (key, value) in values {
            // SAFETY: every mutation in this binary holds ENV_LOCK.
            unsafe { std::env::set_var(key, value) };
        }
        Self(previous)
    }
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        for (key, previous) in self.0.drain(..) {
            // SAFETY: dropped while the owning test still holds ENV_LOCK.
            unsafe {
                match previous {
                    Some(value) => std::env::set_var(key, value),
                    None => std::env::remove_var(key),
                }
            }
        }
    }
}

fn assemble(data: &Path) -> EngineCore {
    EngineCore::assemble(
        data,
        Arc::new(HarnessRegistry::new()),
        zeron_proto::HarnessId::Mock,
        None,
    )
    .unwrap()
}

fn git(cwd: &Path, args: &[&str]) {
    let output = std::process::Command::new("git")
        .args(args)
        .current_dir(cwd)
        .env("GIT_AUTHOR_NAME", "test")
        .env("GIT_AUTHOR_EMAIL", "test@test")
        .env("GIT_COMMITTER_NAME", "test")
        .env("GIT_COMMITTER_EMAIL", "test@test")
        .output()
        .expect("git spawns");
    assert!(
        output.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn init_repo(dir: &Path) -> PathBuf {
    std::fs::create_dir_all(dir).unwrap();
    git(dir, &["init", "-b", "main"]);
    git(dir, &["config", "commit.gpgsign", "false"]);
    std::fs::write(dir.join("README.md"), "fixture\n").unwrap();
    git(dir, &["add", "README.md"]);
    git(dir, &["commit", "-m", "initial"]);
    std::fs::canonicalize(dir).unwrap()
}

/// A running engine reachable the way `zeron mcp` and `__workers_mcp__`
/// reach the real one: a loopback WebSocket.
struct TestEngine {
    core: EngineCore,
    endpoint: String,
    _server: tokio::task::JoinHandle<()>,
}

impl TestEngine {
    async fn start(data: &Path) -> Self {
        let core = assemble(data);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let endpoint = format!("ws://{}", listener.local_addr().unwrap());
        let service: Arc<dyn zeron_rpc::RpcService> = core.rpc_service();
        let server = tokio::spawn(zeron_rpc::serve_ws_listener(listener, service));
        Self {
            core,
            endpoint,
            _server: server,
        }
    }

    fn client(&self) -> zeron_rpc::RpcClient {
        zeron_rpc::memory_client(self.core.rpc_service())
    }

    async fn local_device(&self) -> String {
        let reply = self
            .client()
            .call(methods::LOCAL_DEVICE, json!({}))
            .await
            .unwrap();
        reply["deviceId"].as_str().unwrap().to_owned()
    }

    async fn create_space(&self, device_id: &str, path: &Path) -> String {
        let space_id = uuid::Uuid::new_v4().to_string();
        self.client()
            .call(
                methods::MUTATE,
                json!({
                    "op": "createSpace",
                    "spaceId": space_id,
                    "deviceId": device_id,
                    "path": path.to_string_lossy(),
                    "gitDetected": true,
                }),
            )
            .await
            .unwrap();
        space_id
    }

    async fn chat_mcp(&self, tool: &str, args: Value) -> Value {
        let zeron = zeron_mcp::Zeron::new(self.endpoint.clone(), zeron_mcp::Origin::default());
        zeron_mcp::Tools::new(Arc::new(zeron))
            .call(tool, args)
            .await
            .unwrap_or_else(|error| panic!("chat MCP {tool}: {error}"))
    }
}

/// Call the Workers controller MCP `workers` tool the way a harness does,
/// off the async runtime (the controller is a blocking stdio server).
async fn workers(arguments: Value) -> Value {
    let response = tokio::task::spawn_blocking(move || {
        zeron_workers_unpeel::controller_mcp_handle_request(json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "tools/call",
            "params": { "name": "workers", "arguments": arguments }
        }))
        .expect("tools/call responds")
    })
    .await
    .unwrap();
    response["result"].clone()
}

fn ids(projects: &Value) -> BTreeSet<String> {
    projects
        .as_array()
        .expect("projects array")
        .iter()
        .map(|project| project["id"].as_str().expect("project id").to_owned())
        .collect()
}

fn workers_home(root: &Path, projects: Value) -> PathBuf {
    let home = root.join("unpeel");
    std::fs::create_dir_all(&home).unwrap();
    std::fs::write(
        home.join("app-state.json"),
        serde_json::to_vec(&json!({
            "projects": projects,
            "presets": [],
            "active_tabs": {},
            "pinned_sessions": {}
        }))
        .unwrap(),
    )
    .unwrap();
    home
}

fn controller_env(home: &Path, endpoint: &str) -> EnvGuard {
    EnvGuard::set(vec![
        ("UNPEEL_HOME", home.into()),
        ("ZERON_WORKTREES_DIR", home.join("worktrees").into()),
        (
            "ZERON_WORKTREE_OWNERSHIP_FILE",
            home.join("worktree-ownership.json").into(),
        ),
        ("COMET_WORKERS_HOOKS_DIR", home.join("hooks").into()),
        ("COMET_WORKERS_ENGINE_ENDPOINT", endpoint.into()),
    ])
}

/// Scenario "Every project listing agrees" (chat MCP ↔ Workers controller):
/// a folder that is both a Space and a Workers registration is one project
/// with one id in both MCP listings.
#[tokio::test(flavor = "multi_thread")]
async fn chat_mcp_and_workers_controller_list_the_same_project_ids() {
    let _lock = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let root = tempfile::tempdir().unwrap();
    let repo = init_repo(&root.path().join("repo"));
    let engine = TestEngine::start(&root.path().join("data")).await;
    let device = engine.local_device().await;
    engine.create_space(&device, &repo).await;
    let home = workers_home(
        root.path(),
        json!([{
            "id": "comet-registered-checkout",
            "name": "repo",
            "path": repo.to_string_lossy(),
            "sort_order": 0,
            "is_folder": false
        }]),
    );
    let _env = controller_env(&home, &engine.endpoint);

    let chat = engine.chat_mcp("list_projects", json!({})).await;
    let controller = workers(json!({ "action": "list_projects" })).await;
    assert_eq!(controller["isError"], false, "{controller}");

    assert_eq!(
        ids(&controller["structuredContent"]["projects"]),
        ids(&chat["projects"]),
        "the Workers controller must list exactly the chat MCP's project ids"
    );
}
