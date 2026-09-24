//! The Comet-owned Workers controller MCP server, resolved once and rendered
//! in each runtime's own config dialect. One resolver, three renderers.
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
) -> Option<WorkersMcpServer> {
    if !enabled || disabled_by_environment || !executable.is_absolute() {
        return None;
    }
    let mut env = vec![("COMET_WORKERS_CONTROLLER".to_owned(), "1".to_owned())];
    if let Some(id) = parent_chat_id.filter(|value| !value.trim().is_empty()) {
        env.push(("COMET_WORKERS_PARENT_CHAT_ID".to_owned(), id.to_owned()));
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
            ("COMET_SESSIONS_PARENT_CHAT_ID".into(), grant.parent_chat_id.clone()),
            ("COMET_SESSIONS_ENDPOINT".into(), grant.endpoint.clone()),
            ("COMET_SESSIONS_ENGINE_ID".into(), grant.engine_id.clone()),
        ],
        timeout_secs: SESSIONS_TIMEOUT_SECS,
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
    servers_for(&executable, request, disabled)
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
    ) {
        servers.push(server);
    }
    if let Some(grant) = &request.sessions
        && let Some(server) = resolve_sessions_for(executable, grant)
    {
        servers.push(server);
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
        ];
        overrides.extend(self.env.iter().map(|(name, value)| {
            format!("mcp_servers.{}.env.{name}={}", self.name, quote(value))
        }));
        overrides
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn server() -> WorkersMcpServer {
        resolve_for(Path::new("/opt/zeron"), true, false, Some("chat-1")).expect("enabled")
    }

    #[test]
    fn disabled_or_relative_executable_yields_none() {
        assert!(resolve_for(Path::new("/opt/zeron"), false, false, None).is_none());
        assert!(resolve_for(Path::new("/opt/zeron"), true, true, None).is_none());
        assert!(resolve_for(Path::new("zeron"), true, false, None).is_none());
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

    fn grant() -> zeron_proto::SessionsGrant {
        zeron_proto::SessionsGrant {
            parent_chat_id: "parent".into(),
            endpoint: "ws://127.0.0.1:9".into(),
            engine_id: "engine-1".into(),
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
        }
    }

    #[test]
    fn both_grants_render_workers_and_sessions() {
        let servers = servers_for(Path::new("/opt/zeron"), &request(true, true), false);
        assert_eq!(servers.len(), 2);
        let claude: serde_json::Value =
            serde_json::from_str(&claude_config_json(&servers).unwrap()).unwrap();
        assert!(claude["mcpServers"].get("comet-workers").is_some());
        assert_eq!(
            claude["mcpServers"]["comet-sessions"]["env"]["COMET_SESSIONS_ENGINE_ID"],
            "engine-1"
        );
        let codex: Vec<_> = servers.iter().flat_map(|server| server.codex_overrides()).collect();
        assert!(codex.iter().any(|line| line.contains("mcp_servers.comet-sessions.")));
        assert!(codex.iter().any(|line| line.contains("mcp_servers.comet-workers.tool_timeout_sec=")));
        let acp: Vec<_> = servers.iter().map(|server| server.acp_value()).collect();
        assert_eq!(acp.len(), 2);
    }

    #[test]
    fn workers_only_omits_sessions() {
        let servers = servers_for(Path::new("/opt/zeron"), &request(true, false), false);
        assert_eq!(servers.len(), 1);
        assert_eq!(servers[0].name, "comet-workers");
        let config = claude_config_json(&servers).unwrap();
        assert!(!config.contains("comet-sessions"));
    }

    #[test]
    fn no_grant_yields_no_server() {
        assert!(servers_for(Path::new("/opt/zeron"), &request(false, false), false).is_empty());
        assert!(claude_config_json(&[]).is_none());
    }

}
