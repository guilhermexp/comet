//! MCP servers an embedding app adds beside `unpeel` in every Worker launch.
//!
//! Unpeel registers none, so a standalone build launches exactly as before.
//! Comet registers its servers (`comet-computer-use`) at process start, before
//! the session host runs. Adapters with an MCP config seam (claude-code,
//! cursor-agent, cline, kimi) write each server next to `unpeel`; the pi
//! family has no MCP client and loads one generated bridge extension per
//! server instead. Every server runs the current executable with its args.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use serde_json::{json, Value};

#[derive(Debug)]
pub struct ExtraMcpServer {
    pub name: &'static str,
    pub args: &'static [&'static str],
}

struct Registry {
    servers: &'static [ExtraMcpServer],
    /// Pi extension source that bridges one stdio MCP server. It reads its
    /// server as JSON from `process.env.ZERON_PI_MCP`, which the generated
    /// file replaces with the literal config.
    pi_bridge: Option<&'static str>,
}

static REGISTRY: OnceLock<Registry> = OnceLock::new();

/// Register once per process; later calls are ignored.
pub fn register(servers: &'static [ExtraMcpServer], pi_bridge: Option<&'static str>) {
    let _ = REGISTRY.set(Registry { servers, pi_bridge });
}

pub fn servers() -> &'static [ExtraMcpServer] {
    REGISTRY.get().map_or(&[], |registry| registry.servers)
}

/// `(name, {command, args})` per server. `stdio_type` adds
/// `"type": "stdio"` for clients whose config carries it.
pub fn config_entries(
    servers: &[ExtraMcpServer],
    executable: &Path,
    stdio_type: bool,
) -> Vec<(&'static str, Value)> {
    servers
        .iter()
        .map(|server| {
            let mut entry = json!({
                "command": executable.to_string_lossy(),
                "args": server.args,
            });
            if stdio_type {
                entry["type"] = json!("stdio");
            }
            (server.name, entry)
        })
        .collect()
}

/// Same as [`config_entries`] for the registered servers and the current
/// executable.
pub fn registered_entries(stdio_type: bool) -> Result<Vec<(&'static str, Value)>, String> {
    if servers().is_empty() {
        return Ok(Vec::new());
    }
    let executable = crate::session_host::resolve_current_executable()?;
    Ok(config_entries(servers(), &executable, stdio_type))
}

pub fn pi_bridge_path(name: &str) -> PathBuf {
    crate::app_paths::unpeel_home()
        .join("hooks")
        .join(format!("mcp-bridge-{name}.js"))
}

/// The bridge source with this server's config baked in.
pub fn render_pi_bridge(bridge: &str, name: &str, executable: &Path, args: &[&str]) -> String {
    let config = json!({
        "name": name,
        "command": executable.to_string_lossy(),
        "args": args,
        "env": {},
    })
    .to_string();
    let literal = serde_json::to_string(&config).expect("string serialization cannot fail");
    bridge.replace("process.env.ZERON_PI_MCP", &literal)
}

/// Write one bridge extension per registered server. Returns their paths, in
/// registration order; empty when nothing is registered.
pub fn install_pi_bridges() -> Result<Vec<PathBuf>, String> {
    let Some(bridge) = REGISTRY.get().and_then(|registry| registry.pi_bridge) else {
        return Ok(Vec::new());
    };
    if servers().is_empty() {
        return Ok(Vec::new());
    }
    let executable = crate::session_host::resolve_current_executable()?;
    servers()
        .iter()
        .map(|server| {
            let path = pi_bridge_path(server.name);
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent).map_err(|error| {
                    format!(
                        "Failed to create MCP bridge dir {}: {error}",
                        parent.display()
                    )
                })?;
            }
            crate::hook_assets::write_file_atomic(
                &path,
                &render_pi_bridge(bridge, server.name, &executable, server.args),
                "Pi-family MCP bridge extension",
            )?;
            Ok(path)
        })
        .collect()
}

/// Bridge paths a pi-family launch should load. Only servers with a bridge
/// source registered get one.
pub fn pi_bridge_paths() -> Vec<PathBuf> {
    if REGISTRY
        .get()
        .and_then(|registry| registry.pi_bridge)
        .is_none()
    {
        return Vec::new();
    }
    servers()
        .iter()
        .map(|server| pi_bridge_path(server.name))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const SERVERS: &[ExtraMcpServer] = &[ExtraMcpServer {
        name: "comet-computer-use",
        args: &["__computer_use_mcp__", "--worker"],
    }];

    #[test]
    fn entries_run_the_executable_with_each_servers_args() {
        let entries = config_entries(SERVERS, Path::new("/opt/zeron"), true);
        assert_eq!(entries.len(), 1);
        let (name, entry) = &entries[0];
        assert_eq!(*name, "comet-computer-use");
        assert_eq!(entry["command"], "/opt/zeron");
        assert_eq!(entry["args"], json!(["__computer_use_mcp__", "--worker"]));
        assert_eq!(entry["type"], "stdio");
        let plain = config_entries(SERVERS, Path::new("/opt/zeron"), false);
        assert!(plain[0].1.get("type").is_none());
    }

    #[test]
    fn bridge_gets_the_config_as_a_literal() {
        let source = "const config = JSON.parse(process.env.ZERON_PI_MCP);";
        let rendered = render_pi_bridge(
            source,
            "comet-computer-use",
            Path::new("/opt/zeron"),
            &["__computer_use_mcp__", "--worker"],
        );
        assert!(!rendered.contains("process.env.ZERON_PI_MCP"));
        let literal = rendered
            .strip_prefix("const config = JSON.parse(")
            .and_then(|rest| rest.strip_suffix(");"))
            .unwrap();
        let config: Value =
            serde_json::from_str(&serde_json::from_str::<String>(literal).unwrap()).unwrap();
        assert_eq!(config["name"], "comet-computer-use");
        assert_eq!(config["args"], json!(["__computer_use_mcp__", "--worker"]));
    }

    #[test]
    fn nothing_is_registered_by_default() {
        assert!(servers().is_empty());
        assert!(pi_bridge_paths().is_empty());
    }
}
