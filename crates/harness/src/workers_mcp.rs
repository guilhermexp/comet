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
}

/// Resolve the controller from the process environment: the descriptor is
/// dropped entirely when Workers are off, when `ZERON_DISABLE_WORKERS_MCP=1`,
/// or when the executable is not an absolute path (a child spawned by name
/// resolves against its own PATH, not ours).
pub(crate) fn resolve(enabled: bool, parent_chat_id: Option<&str>) -> Option<WorkersMcpServer> {
    let disabled = std::env::var("ZERON_DISABLE_WORKERS_MCP")
        .ok()
        .is_some_and(|value| value == "1");
    let executable = std::env::var_os("ZERON_WORKERS_MCP_BIN")
        .map(PathBuf::from)
        .or_else(|| std::env::current_exe().ok())?;
    resolve_for(&executable, enabled, disabled, parent_chat_id)
}

/// The pure half of [`resolve`]: every input explicit, nothing read from the
/// environment.
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
    })
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
                "mcp_servers.{NAME}.command={}",
                quote(&self.command.to_string_lossy())
            ),
            format!("mcp_servers.{NAME}.args={}", json!(self.args)),
            format!(
                "mcp_servers.{NAME}.tool_timeout_sec={}",
                crate::WORKERS_CLIENT_DEADLINE_SECONDS
            ),
        ];
        overrides.extend(
            self.env
                .iter()
                .map(|(name, value)| format!("mcp_servers.{NAME}.env.{name}={}", quote(value))),
        );
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
}
