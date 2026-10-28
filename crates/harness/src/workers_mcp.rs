//! Comet MCP servers (`comet-workers`, `comet-sessions`, `zeron`,
//! `comet-computer-use`), resolved together and rendered in each runtime's
//! own config dialect.
use std::path::{Path, PathBuf};

use serde_json::{Value, json};

pub(crate) const WORKERS_MCP_ARG: &str = "__workers_mcp__";
const NAME: &str = "comet-workers";

pub(crate) struct WorkersMcpServer {
    pub name: &'static str,
    pub command: PathBuf,
    pub args: Vec<String>,
    pub env: Vec<(String, String)>,
    pub timeout_secs: u64,
}

/// Environment entry carrying the engine endpoint to the controller MCP, which
/// lists and creates projects (Spaces) through it. Mirrors
/// `zeron_workers_unpeel::space_registry::ENGINE_ENDPOINT_ENV`.
pub(crate) const ENGINE_ENDPOINT_ENV: &str = "COMET_WORKERS_ENGINE_ENDPOINT";

/// The IPC endpoint this process's engine actually bound, recorded by the
/// engine once IPC is served. One engine per process owns the Workers state.
static ENGINE_ENDPOINT: std::sync::Mutex<Option<String>> = std::sync::Mutex::new(None);

/// Record the engine endpoint every controller MCP spawned from now on gets.
pub fn note_workers_engine_endpoint(endpoint: &str) {
    let endpoint = endpoint.trim();
    if endpoint.is_empty() {
        return;
    }
    *ENGINE_ENDPOINT
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(endpoint.to_owned());
}

pub(crate) fn engine_endpoint() -> Option<String> {
    ENGINE_ENDPOINT
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .clone()
}

/// The engine's IPC bearer file (`{data_dir}/ipc-token`), recorded once IPC is
/// served. Every Comet server that dials the engine gets its PATH as
/// `ZERON_IPC_TOKEN_FILE` (never the token itself: some runtimes put this env
/// on a command line any local user can read).
static IPC_TOKEN_FILE: std::sync::Mutex<Option<PathBuf>> = std::sync::Mutex::new(None);

/// Record the IPC bearer file every Comet MCP server spawned from now on gets.
pub fn note_workers_ipc_token_file(path: &Path) {
    *IPC_TOKEN_FILE
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(path.to_path_buf());
}

pub(crate) fn ipc_token_file() -> Option<PathBuf> {
    IPC_TOKEN_FILE
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .clone()
}

/// Mirrors `zeron_rpc::ipc_auth::TOKEN_FILE_ENV` (the harness doesn't link the
/// RPC crate).
const TOKEN_FILE_ENV: &str = "ZERON_IPC_TOKEN_FILE";

/// Mirrors `zeron_proto::McpServer::SIDE_CHATS_ONLY_ENV`.
const SIDE_CHATS_ONLY_ENV: &str = "ZERON_SIDE_CHATS_ONLY";

/// Resolve the controller from the process environment: the descriptor is
/// dropped entirely when Workers are off, when `ZERON_DISABLE_WORKERS_MCP=1`,
/// or when the executable is not an absolute path (a child spawned by name
/// resolves against its own PATH, not ours).
/// Every input explicit, nothing read from the environment.
pub(crate) fn resolve_for(
    executable: &Path,
    enabled: bool,
    disabled_by_environment: bool,
    parent_chat_id: Option<&str>,
    engine_endpoint: Option<&str>,
) -> Option<WorkersMcpServer> {
    if !enabled || disabled_by_environment || !executable.is_absolute() {
        return None;
    }
    let mut env = vec![("COMET_WORKERS_CONTROLLER".to_owned(), "1".to_owned())];
    if let Some(id) = parent_chat_id.filter(|value| !value.trim().is_empty()) {
        env.push(("COMET_WORKERS_PARENT_CHAT_ID".to_owned(), id.to_owned()));
    }
    if let Some(endpoint) = engine_endpoint.filter(|value| !value.trim().is_empty()) {
        env.push((ENGINE_ENDPOINT_ENV.to_owned(), endpoint.to_owned()));
    }
    Some(WorkersMcpServer {
        name: NAME,
        command: executable.to_path_buf(),
        args: vec![WORKERS_MCP_ARG.to_owned()],
        env,
        timeout_secs: crate::WORKERS_CLIENT_DEADLINE_SECONDS,
    })
}

pub(crate) const SESSIONS_MCP_ARG: &str = "__sessions_mcp__";
const SESSIONS_NAME: &str = "comet-sessions";
const SESSIONS_TIMEOUT_SECS: u64 = 120;

pub(crate) fn resolve_sessions_for(
    executable: &Path,
    grant: &zeron_proto::SessionsGrant,
) -> Option<WorkersMcpServer> {
    if !executable.is_absolute()
        || grant.parent_chat_id.trim().is_empty()
        || grant.endpoint.trim().is_empty()
        || grant.engine_id.trim().is_empty()
    {
        return None;
    }
    Some(WorkersMcpServer {
        name: SESSIONS_NAME,
        command: executable.to_path_buf(),
        args: vec![SESSIONS_MCP_ARG.to_owned()],
        env: vec![
            ("COMET_SESSIONS_CONTROLLER".into(), "1".into()),
            (
                "COMET_SESSIONS_PARENT_CHAT_ID".into(),
                grant.parent_chat_id.clone(),
            ),
            ("COMET_SESSIONS_ENDPOINT".into(), grant.endpoint.clone()),
            ("COMET_SESSIONS_ENGINE_ID".into(), grant.engine_id.clone()),
        ],
        timeout_secs: SESSIONS_TIMEOUT_SECS,
    })
}

const ZERON_NAME: &str = "zeron";
/// `wait_for_turn` blocks up to an hour; the client must outlast it.
const ZERON_TIMEOUT_SECS: u64 = 3660;

/// Zeron's own chat MCP (`zeron mcp`: list/read/create/send/wait on chats,
/// batch `create_chats`/`send_messages`). Upstream injects it into every run;
/// the fork rides the same root-orchestrator grant as `comet-sessions`, so
/// subagents, Workers and child chats never gain chat control. Chats it
/// creates link back to `grant.parent_chat_id` (`ZERON_CHAT_ID`).
pub(crate) fn resolve_zeron_for(
    executable: &Path,
    grant: &zeron_proto::SessionsGrant,
) -> Option<WorkersMcpServer> {
    let port = grant
        .endpoint
        .trim()
        .trim_end_matches('/')
        .rsplit_once(':')
        .and_then(|(_, port)| port.parse::<u16>().ok())
        .filter(|port| *port != 0)?;
    if !executable.is_absolute()
        || grant.parent_chat_id.trim().is_empty()
        || grant.engine_id.trim().is_empty()
    {
        return None;
    }
    Some(WorkersMcpServer {
        name: ZERON_NAME,
        command: executable.to_path_buf(),
        args: vec!["mcp".to_owned()],
        env: vec![
            ("ZERON_IPC_PORT".into(), port.to_string()),
            ("ZERON_CHAT_ID".into(), grant.parent_chat_id.clone()),
            ("ZERON_DEVICE_ID".into(), grant.engine_id.clone()),
        ]
        .into_iter()
        // The host turned the provider's native subagents off: this server
        // tells the agent to delegate through side chats instead (b8ed9d9c2,
        // re-wired through the fork's root grant instead of `request.mcp`).
        .chain(
            grant
                .side_chats_only
                .then(|| (SIDE_CHATS_ONLY_ENV.to_owned(), "1".to_owned())),
        )
        .collect(),
        timeout_secs: ZERON_TIMEOUT_SECS,
    })
}

pub(crate) const COMPUTER_USE_MCP_ARG: &str = "__computer_use_mcp__";
pub(crate) const COMPUTER_USE_NAME: &str = "comet-computer-use";
/// Above the tool's 300 s `timeout_ms` ceiling plus the server's margin.
pub(crate) const COMPUTER_USE_TIMEOUT_SECS: u64 = 330;

/// Comet's Computer Use proxy for an agent chat. Mounted whenever the run is
/// an agent chat (`enable_workers_mcp`), independent of the Workers kill
/// switch: the Settings switch is enforced inside the server, which reads
/// this host's `ui-settings.json`. Codex never mounts it (native Computer
/// Use); see `codex_workers_mcp_overrides`.
pub(crate) fn resolve_computer_use_for(
    executable: &Path,
    chat_id: Option<&str>,
    data_dir: Option<&str>,
) -> Option<WorkersMcpServer> {
    if !executable.is_absolute() {
        return None;
    }
    let mut env = Vec::new();
    if let Some(dir) = data_dir.filter(|value| !value.trim().is_empty()) {
        env.push(("ZERON_DATA_DIR".to_owned(), dir.to_owned()));
    }
    if let Some(id) = chat_id.filter(|value| !value.trim().is_empty()) {
        env.push(("COMET_COMPUTER_USE_SESSION".to_owned(), id.to_owned()));
    }
    Some(WorkersMcpServer {
        name: COMPUTER_USE_NAME,
        command: executable.to_path_buf(),
        args: vec![COMPUTER_USE_MCP_ARG.to_owned()],
        env,
        timeout_secs: COMPUTER_USE_TIMEOUT_SECS,
    })
}

pub(crate) fn servers_for_request(request: &zeron_proto::RunRequest) -> Vec<WorkersMcpServer> {
    let disabled = std::env::var("ZERON_DISABLE_WORKERS_MCP")
        .ok()
        .is_some_and(|value| value == "1");
    let Some(executable) = std::env::var_os("ZERON_WORKERS_MCP_BIN")
        .map(PathBuf::from)
        .or_else(|| std::env::current_exe().ok())
    else {
        return Vec::new();
    };
    let mut servers = servers_for(&executable, request, disabled);
    with_ipc_token_file(&mut servers, ipc_token_file().as_deref());
    servers
}

/// The servers that dial the engine's IPC present its bearer, read from the
/// file the engine published.
pub(crate) fn with_ipc_token_file(servers: &mut [WorkersMcpServer], token_file: Option<&Path>) {
    let Some(path) = token_file.and_then(Path::to_str) else {
        return;
    };
    for server in servers
        .iter_mut()
        .filter(|server| [NAME, SESSIONS_NAME, ZERON_NAME].contains(&server.name))
    {
        server
            .env
            .push((TOKEN_FILE_ENV.to_owned(), path.to_owned()));
    }
}

pub(crate) fn servers_for(
    executable: &Path,
    request: &zeron_proto::RunRequest,
    workers_disabled: bool,
) -> Vec<WorkersMcpServer> {
    let mut servers = Vec::new();
    if let Some(server) = resolve_for(
        executable,
        request.enable_workers_mcp,
        workers_disabled,
        request.workers_parent_chat_id.as_deref(),
        engine_endpoint().as_deref(),
    ) {
        servers.push(server);
    }
    if let Some(grant) = &request.sessions {
        servers.extend(resolve_sessions_for(executable, grant));
        servers.extend(resolve_zeron_for(executable, grant));
    }
    if request.enable_workers_mcp {
        servers.extend(resolve_computer_use_for(
            executable,
            request.workers_parent_chat_id.as_deref(),
            std::env::var("ZERON_DATA_DIR").ok().as_deref(),
        ));
    }
    servers
}

pub(crate) fn claude_config_json(servers: &[WorkersMcpServer]) -> Option<String> {
    if servers.is_empty() {
        return None;
    }
    let mut mcp_servers = serde_json::Map::new();
    for server in servers {
        let env: serde_json::Map<String, Value> = server
            .env
            .iter()
            .map(|(name, value)| (name.clone(), Value::String(value.clone())))
            .collect();
        mcp_servers.insert(
            server.name.to_owned(),
            json!({
                "command": server.command.to_string_lossy(),
                "args": server.args,
                "env": env,
            }),
        );
    }
    Some(json!({ "mcpServers": mcp_servers }).to_string())
}

impl WorkersMcpServer {
    /// Generic MCP server shape, for harnesses that take `zeron_proto::McpServer`.
    pub(crate) fn proto(&self) -> zeron_proto::McpServer {
        zeron_proto::McpServer {
            name: self.name.to_owned(),
            command: self.command.to_string_lossy().into_owned(),
            args: self.args.clone(),
            env: self.env.iter().cloned().collect(),
        }
    }

    /// ACP `mcpServers` entry: env as a list of `{name, value}` rows.
    pub(crate) fn acp_value(&self) -> Value {
        json!({
            "type": "stdio",
            "name": self.name,
            "command": self.command.to_string_lossy(),
            "args": self.args,
            "env": self
                .env
                .iter()
                .map(|(name, value)| json!({ "name": name, "value": value }))
                .collect::<Vec<_>>(),
        })
    }

    /// Claude Code `--mcp-config` payload: env as an object.
    #[cfg(test)]
    pub(crate) fn claude_config_json(&self) -> String {
        let env: serde_json::Map<String, Value> = self
            .env
            .iter()
            .map(|(name, value)| (name.clone(), Value::String(value.clone())))
            .collect();
        json!({
            "mcpServers": {
                self.name: {
                    "command": self.command.to_string_lossy(),
                    "args": self.args,
                    "env": env,
                }
            }
        })
        .to_string()
    }

    /// Codex `-c` overrides. The Workers wait is orchestrator-sized (up to
    /// hours); Codex's MCP client must not expire it first.
    pub(crate) fn codex_overrides(&self) -> Vec<String> {
        let quote =
            |value: &str| serde_json::to_string(value).expect("string serialization cannot fail");
        let mut overrides = vec![
            format!(
                "mcp_servers.{}.command={}",
                self.name,
                quote(&self.command.to_string_lossy())
            ),
            format!("mcp_servers.{}.args={}", self.name, json!(self.args)),
            format!(
                "mcp_servers.{}.tool_timeout_sec={}",
                self.name, self.timeout_secs
            ),
            // The fork runs Codex with `approvalPolicy: "never"`, which forbids
            // approval prompts; explicitly approve our servers' tools so Codex
            // does not reject calls that need approval (#985).
            format!(
                "mcp_servers.{}.default_tools_approval_mode={}",
                self.name,
                quote("approve")
            ),
        ];
        overrides.extend(
            self.env.iter().map(|(name, value)| {
                format!("mcp_servers.{}.env.{name}={}", self.name, quote(value))
            }),
        );
        overrides
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn server() -> WorkersMcpServer {
        resolve_for(Path::new("/opt/zeron"), true, false, Some("chat-1"), None).expect("enabled")
    }

    #[test]
    fn controller_env_carries_the_engine_endpoint_when_known() {
        let with = resolve_for(
            Path::new("/opt/zeron"),
            true,
            false,
            Some("chat-1"),
            Some("ws://127.0.0.1:27654"),
        )
        .expect("enabled");
        assert!(with.env.contains(&(
            ENGINE_ENDPOINT_ENV.to_owned(),
            "ws://127.0.0.1:27654".to_owned()
        )));
        let without =
            resolve_for(Path::new("/opt/zeron"), true, false, None, None).expect("enabled");
        assert!(
            without
                .env
                .iter()
                .all(|(name, _)| name != ENGINE_ENDPOINT_ENV)
        );
    }

    #[test]
    fn disabled_or_relative_executable_yields_none() {
        assert!(resolve_for(Path::new("/opt/zeron"), false, false, None, None).is_none());
        assert!(resolve_for(Path::new("/opt/zeron"), true, true, None, None).is_none());
        assert!(resolve_for(Path::new("zeron"), true, false, None, None).is_none());
    }

    #[test]
    fn acp_value_matches_previous_shape() {
        let v = server().acp_value();
        assert_eq!(v["type"], "stdio");
        assert_eq!(v["name"], "comet-workers");
        assert_eq!(v["command"], "/opt/zeron");
        assert_eq!(v["args"], serde_json::json!([WORKERS_MCP_ARG]));
        assert_eq!(v["args"][0], WORKERS_MCP_ARG);
        assert_eq!(v["env"][0]["name"], "COMET_WORKERS_CONTROLLER");
        assert_eq!(v["env"][0]["value"], "1");
        assert_eq!(v["env"][1]["name"], "COMET_WORKERS_PARENT_CHAT_ID");
        assert_eq!(v["env"][1]["value"], "chat-1");
    }

    #[test]
    fn claude_config_nests_env_as_object() {
        let parsed: serde_json::Value =
            serde_json::from_str(&server().claude_config_json()).unwrap();
        assert_eq!(
            parsed["mcpServers"]["comet-workers"]["env"]["COMET_WORKERS_PARENT_CHAT_ID"],
            "chat-1"
        );
    }

    #[test]
    fn codex_overrides_carry_deadline_and_env() {
        let overrides = server().codex_overrides();
        assert!(overrides.iter().any(|o| o
            == &format!(
                "mcp_servers.comet-workers.tool_timeout_sec={}",
                crate::WORKERS_CLIENT_DEADLINE_SECONDS
            )));
        assert!(
            overrides
                .iter()
                .any(|o| o
                    == "mcp_servers.comet-workers.env.COMET_WORKERS_PARENT_CHAT_ID=\"chat-1\"")
        );
    }

    #[test]
    fn codex_overrides_approve_our_tools_under_never_policy() {
        assert!(
            server()
                .codex_overrides()
                .iter()
                .any(|o| o == "mcp_servers.comet-workers.default_tools_approval_mode=\"approve\"")
        );
    }

    #[test]
    fn engine_dialing_servers_carry_the_ipc_token_file() {
        let mut servers = servers_for(Path::new("/opt/zeron"), &request(true, true), false);
        with_ipc_token_file(&mut servers, Some(Path::new("/data/ipc-token")));
        for server in &servers {
            let carries = server
                .env
                .iter()
                .any(|(k, v)| k == TOKEN_FILE_ENV && v == "/data/ipc-token");
            assert_eq!(
                carries,
                server.name != COMPUTER_USE_NAME,
                "{} token file",
                server.name
            );
        }
    }

    #[test]
    fn side_chats_only_rides_the_root_grant() {
        let env_of = |grant: &zeron_proto::SessionsGrant| {
            resolve_zeron_for(Path::new("/opt/zeron"), grant)
                .expect("zeron server")
                .env
        };
        let off = grant();
        assert!(!env_of(&off).iter().any(|(k, _)| k == SIDE_CHATS_ONLY_ENV));
        let on = zeron_proto::SessionsGrant {
            side_chats_only: true,
            ..grant()
        };
        assert!(
            env_of(&on)
                .iter()
                .any(|(k, v)| k == SIDE_CHATS_ONLY_ENV && v == "1")
        );
    }

    fn grant() -> zeron_proto::SessionsGrant {
        zeron_proto::SessionsGrant {
            parent_chat_id: "parent".into(),
            endpoint: "ws://127.0.0.1:9".into(),
            engine_id: "engine-1".into(),
            side_chats_only: false,
        }
    }

    fn request(workers: bool, sessions: bool) -> zeron_proto::RunRequest {
        zeron_proto::RunRequest {
            prompt: "p".into(),
            harness: None,
            model: None,
            reasoning: None,
            model_options: Default::default(),
            cwd: ".".into(),
            sandbox: zeron_proto::SandboxLevel::WorkspaceWrite,
            auto_approve: false,
            enable_workers_mcp: workers,
            workers_parent_chat_id: workers.then(|| "parent".into()),
            sessions: sessions.then(grant),
            attachments: Vec::new(),
            resume: None,
            worktree: None,
            mcp: None,
            env: Default::default(),
        }
    }

    #[test]
    fn both_grants_render_workers_and_sessions() {
        let servers = servers_for(Path::new("/opt/zeron"), &request(true, true), false);
        assert_eq!(servers.len(), 4);
        let claude: serde_json::Value =
            serde_json::from_str(&claude_config_json(&servers).unwrap()).unwrap();
        assert!(claude["mcpServers"].get("comet-workers").is_some());
        assert_eq!(
            claude["mcpServers"]["comet-sessions"]["env"]["COMET_SESSIONS_ENGINE_ID"],
            "engine-1"
        );
        let codex: Vec<_> = servers
            .iter()
            .flat_map(|server| server.codex_overrides())
            .collect();
        assert!(
            codex
                .iter()
                .any(|line| line.contains("mcp_servers.comet-sessions."))
        );
        assert!(
            codex
                .iter()
                .any(|line| line.contains("mcp_servers.comet-workers.tool_timeout_sec="))
        );
        let acp: Vec<_> = servers.iter().map(|server| server.acp_value()).collect();
        assert_eq!(acp.len(), 4);
    }

    #[test]
    fn root_grant_also_carries_the_zeron_chat_mcp() {
        let servers = servers_for(Path::new("/opt/zeron"), &request(true, true), false);
        let claude: serde_json::Value =
            serde_json::from_str(&claude_config_json(&servers).unwrap()).unwrap();
        let zeron = &claude["mcpServers"]["zeron"];
        assert_eq!(zeron["args"], serde_json::json!(["mcp"]));
        assert_eq!(zeron["env"]["ZERON_IPC_PORT"], "9");
        assert_eq!(zeron["env"]["ZERON_CHAT_ID"], "parent");
        assert_eq!(zeron["env"]["ZERON_DEVICE_ID"], "engine-1");
        // No port, no server: the MCP would dial the default engine.
        let mut unbound = grant();
        unbound.endpoint = "ws://127.0.0.1".into();
        assert!(resolve_zeron_for(Path::new("/opt/zeron"), &unbound).is_none());
    }

    #[test]
    fn voice_root_grant_mounts_only_chat_tools() {
        let request = request(false, true);
        assert!(request.mcp.is_none());
        let servers = servers_for(Path::new("/opt/zeron"), &request, false);
        let names: Vec<_> = servers.iter().map(|server| server.name).collect();
        assert_eq!(names, ["comet-sessions", "zeron"]);

        let codex: Vec<_> = servers
            .iter()
            .flat_map(|server| server.codex_overrides())
            .collect();
        assert!(
            codex
                .iter()
                .any(|line| line.contains("mcp_servers.comet-sessions."))
        );
        assert!(codex.iter().any(|line| line.contains("mcp_servers.zeron.")));
        assert!(!codex.iter().any(|line| line.contains("comet-workers")));
    }

    #[test]
    fn workers_only_omits_sessions() {
        let servers = servers_for(Path::new("/opt/zeron"), &request(true, false), false);
        let names: Vec<_> = servers.iter().map(|server| server.name).collect();
        assert_eq!(names, ["comet-workers", "comet-computer-use"]);
        let config = claude_config_json(&servers).unwrap();
        assert!(!config.contains("comet-sessions"));
        assert!(
            !config.contains("\"zeron\""),
            "no chat MCP without the root grant"
        );
    }

    #[test]
    fn agent_chat_mounts_computer_use_even_with_workers_disabled() {
        let servers = servers_for(Path::new("/opt/zeron"), &request(true, false), true);
        let names: Vec<_> = servers.iter().map(|server| server.name).collect();
        assert_eq!(names, ["comet-computer-use"]);
        let claude: serde_json::Value =
            serde_json::from_str(&claude_config_json(&servers).unwrap()).unwrap();
        let server = &claude["mcpServers"]["comet-computer-use"];
        assert_eq!(server["args"], serde_json::json!([COMPUTER_USE_MCP_ARG]));
        assert_eq!(server["env"]["COMET_COMPUTER_USE_SESSION"], "parent");
        let acp = servers[0].acp_value();
        assert_eq!(acp["name"], "comet-computer-use");
    }

    #[test]
    fn computer_use_env_carries_data_dir_and_chat() {
        let server =
            resolve_computer_use_for(Path::new("/opt/zeron"), Some("chat-9"), Some("/data/zeron"))
                .unwrap();
        assert!(
            server
                .env
                .contains(&("ZERON_DATA_DIR".into(), "/data/zeron".into()))
        );
        assert!(
            server
                .env
                .contains(&("COMET_COMPUTER_USE_SESSION".into(), "chat-9".into()))
        );
        assert!(resolve_computer_use_for(Path::new("zeron"), None, None).is_none());
    }

    #[test]
    fn no_grant_yields_no_server() {
        assert!(servers_for(Path::new("/opt/zeron"), &request(false, false), false).is_empty());
        assert!(claude_config_json(&[]).is_none());
    }
}
