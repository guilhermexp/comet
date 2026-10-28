//! The Workers controller dials the engine's IPC with its bearer; nothing it
//! launches (Worker session, detached host, shell) may inherit that bearer.
//! Its own process: the test rewrites the environment.

use std::process::Command;

const TOKEN_ENV: &str = "ZERON_IPC_TOKEN";
const TOKEN_FILE_ENV: &str = "ZERON_IPC_TOKEN_FILE";
/// Pi bridge descriptors: the JSON carries the bearer file path too.
const PI_BRIDGE_ENVS: [&str; 2] = ["ZERON_PI_MCP", "ZERON_PI_MCP_1"];

/// The names of the variables a child process actually received.
fn child_env_names(mut command: Command) -> Vec<String> {
    let output = command.output().expect("child runs");
    assert!(output.status.success());
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| line.split_once('=').map(|(name, _)| name.to_owned()))
        .collect()
}

fn leaked(names: &[String]) -> Vec<&String> {
    names
        .iter()
        .filter(|name| {
            *name == TOKEN_ENV || *name == TOKEN_FILE_ENV || PI_BRIDGE_ENVS.contains(&name.as_str())
        })
        .collect()
}

fn env_command() -> Command {
    Command::new("/usr/bin/env")
}

#[test]
fn worker_processes_never_inherit_the_engine_ipc_bearer() {
    // SAFETY: the only test in this binary; nothing else reads the env.
    unsafe {
        std::env::set_var(TOKEN_ENV, "secret-token");
        std::env::set_var(TOKEN_FILE_ENV, "/data/ipc-token");
        for name in PI_BRIDGE_ENVS {
            std::env::set_var(
                name,
                r#"{"name":"comet-workers","env":{"ZERON_IPC_TOKEN_FILE":"/data/ipc-token"}}"#,
            );
        }
    }

    // A Worker launched by any embedder process (the app included) is
    // spawned through unpeel-core's launcher, which strips the bearer.
    let mut launcher = env_command();
    unpeel_core::session_host::strip_embedder_ipc_credentials(&mut launcher);
    let names = child_env_names(launcher);
    assert_eq!(
        leaked(&names),
        Vec::<&String>::new(),
        "launcher inherited the bearer"
    );

    // The controller takes the bearer out of its own environment at startup,
    // so every descendant (in-process Worker launches included) lacks it.
    zeron_workers_unpeel::controller_mcp_take_ipc_credentials();
    assert!(std::env::var_os(TOKEN_ENV).is_none());
    assert!(std::env::var_os(TOKEN_FILE_ENV).is_none());
    for name in PI_BRIDGE_ENVS {
        assert!(std::env::var_os(name).is_none(), "{name}");
    }
    let names = child_env_names(env_command());
    assert_eq!(
        leaked(&names),
        Vec::<&String>::new(),
        "worker inherited the bearer"
    );

    // The controller itself still dials the engine with the bearer it took.
    let runtime = tokio::runtime::Runtime::new().unwrap();
    runtime.block_on(async {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("ws://{}", listener.local_addr().unwrap());
        tokio::spawn(zeron_rpc::serve_ws_listener(
            listener,
            std::sync::Arc::new(NoMethods),
            "secret-token".to_owned(),
        ));
        assert!(zeron_workers_unpeel::controller_mcp_engine_reachable(&url).await);
    });
}

struct NoMethods;

#[async_trait::async_trait]
impl zeron_rpc::RpcService for NoMethods {
    async fn handle(
        &self,
        method: &str,
        _params: serde_json::Value,
    ) -> Result<zeron_rpc::RpcReply, zeron_rpc::RpcError> {
        Err(zeron_rpc::RpcError::UnknownMethod(method.into()))
    }
}
