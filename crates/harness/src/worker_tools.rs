//! Handing the comet MCP server to the agent.
//!
//! `comet mcp-server` exposes the worker tools (spawn/read/wait/kill) over
//! stdio; the agent only learns they exist if the command that launches it
//! carries the server's configuration. Claude takes it inline through
//! `--mcp-config`, Codex through `-c mcp_servers.comet.*`.
//!
//! Two rules the builders exist to enforce:
//!
//! - **Everything is JSON-encoded, never formatted.** The binary path comes off
//!   the filesystem and the chat id out of the doc; one quote, backslash or
//!   space in either turns a hand-built template into malformed JSON that the
//!   CLI rejects — and the failure surfaces as "no tools", which is the hardest
//!   kind to diagnose.
//! - **`--strict-mcp-config` is never passed.** Strict mode would drop the
//!   user's own MCP servers from every comet-spawned agent. The comet server is
//!   additive.
//!
//! A binary we cannot resolve degrades to "no worker tools", never to a failed
//! run: the builders emit nothing and the agent starts without them.

use std::path::PathBuf;

use serde_json::{Value, json};

use crate::comet_bin::{ipc_port, resolve_comet_bin};

/// The depth a harness-launched server runs at: the session itself, with the
/// full budget of nested workers still ahead of it.
pub const SESSION_DEPTH: usize = 0;

/// Where a comet MCP server can be reached from a spawned agent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerTarget {
    /// The comet binary the agent's MCP client launches.
    pub bin: PathBuf,
    /// The engine's IPC port, which that server dials back into.
    pub port: u16,
}

/// Resolve the server for this machine, or `None` when there is no comet binary
/// to point at.
pub fn server_target() -> Option<ServerTarget> {
    match resolve_comet_bin() {
        Ok(bin) => Some(ServerTarget {
            bin,
            port: ipc_port(),
        }),
        Err(err) => {
            tracing::debug!(error = %err, "worker tools unavailable: no comet binary");
            None
        }
    }
}

/// `["mcp-server", "--chat", <id>, "--port", <port>, "--depth", <n>]`.
fn server_args(target: &ServerTarget, chat_id: &str, depth: usize) -> Vec<String> {
    vec![
        "mcp-server".into(),
        "--chat".into(),
        chat_id.into(),
        "--port".into(),
        target.port.to_string(),
        "--depth".into(),
        depth.to_string(),
    ]
}

/// The single inline argument for Claude's `--mcp-config`.
pub fn claude_mcp_config(
    target: Option<&ServerTarget>,
    chat_id: &str,
    depth: usize,
) -> Option<String> {
    let target = target?;
    Some(
        json!({
            "mcpServers": {
                "comet": {
                    "command": target.bin.to_string_lossy(),
                    "args": server_args(target, chat_id, depth),
                }
            }
        })
        .to_string(),
    )
}

/// Codex's flag pairs: `-c mcp_servers.comet.command=…` and
/// `-c mcp_servers.comet.args=[…]`. Both values are JSON-encoded, which is also
/// valid TOML for a basic string and an array of them.
pub fn codex_mcp_flags(target: Option<&ServerTarget>, chat_id: &str, depth: usize) -> Vec<String> {
    let Some(target) = target else {
        return Vec::new();
    };
    let command = Value::String(target.bin.to_string_lossy().into_owned()).to_string();
    let args = json!(server_args(target, chat_id, depth)).to_string();
    vec![
        "-c".into(),
        format!("mcp_servers.comet.command={command}"),
        "-c".into(),
        format!("mcp_servers.comet.args={args}"),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A path a `format!` template would mangle: a space and a quote.
    pub(crate) const NASTY_BIN: &str = "/Users/First \"Last\"/bin/comet";

    pub(crate) fn nasty_target() -> ServerTarget {
        ServerTarget {
            bin: PathBuf::from(NASTY_BIN),
            port: 30001,
        }
    }

    #[test]
    fn claude_config_survives_a_quoted_binary_path() {
        let config = claude_mcp_config(Some(&nasty_target()), "chat-1", 0)
            .expect("a resolved binary yields a config");
        // Parsed back, not string-compared: a hand-built template would pass a
        // string assertion while the real thing is malformed.
        let parsed: Value = serde_json::from_str(&config).expect("the flag is valid JSON");
        let server = &parsed["mcpServers"]["comet"];
        assert_eq!(server["command"], json!(NASTY_BIN));
        assert_eq!(
            server["args"],
            json!([
                "mcp-server",
                "--chat",
                "chat-1",
                "--port",
                "30001",
                "--depth",
                "0"
            ])
        );
    }

    #[test]
    fn codex_flags_survive_a_quoted_binary_path() {
        let target = nasty_target();
        let flags = codex_mcp_flags(Some(&target), "chat 'x'", 1);
        assert_eq!(flags[0], "-c");
        assert_eq!(flags[2], "-c");
        let command = flags[1]
            .strip_prefix("mcp_servers.comet.command=")
            .expect("command override");
        assert_eq!(
            serde_json::from_str::<String>(command).expect("valid JSON string"),
            NASTY_BIN
        );
        let args = flags[3]
            .strip_prefix("mcp_servers.comet.args=")
            .expect("args override");
        assert_eq!(
            serde_json::from_str::<Vec<String>>(args).expect("valid JSON array"),
            server_args(&target, "chat 'x'", 1)
        );
    }

    #[test]
    fn an_unresolvable_binary_emits_nothing() {
        // No worker tools is a degraded session; a failed run is not.
        assert_eq!(claude_mcp_config(None, "chat-1", 0), None);
        assert!(codex_mcp_flags(None, "chat-1", 0).is_empty());
    }
}
