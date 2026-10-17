//! Finds the `cua_repl` that ChatGPT.app installs as a Codex plugin.

use std::cmp::Ordering;
use std::path::{Path, PathBuf};

use serde_json::Value;

/// How to start `cua_repl`, taken from one plugin version's `.mcp.json`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeCommand {
    pub version: String,
    pub command: String,
    pub args: Vec<String>,
    pub env: Vec<(String, String)>,
}

/// `$COMET_COMPUTER_USE_PLUGIN_DIR`, else
/// `{$CODEX_HOME or ~/.codex}/plugins/cache/openai-bundled/unified-computer-use`.
pub fn plugin_dir() -> Option<PathBuf> {
    if let Some(dir) =
        std::env::var_os("COMET_COMPUTER_USE_PLUGIN_DIR").filter(|dir| !dir.is_empty())
    {
        return Some(PathBuf::from(dir));
    }
    let codex_home = std::env::var_os("CODEX_HOME")
        .filter(|dir| !dir.is_empty())
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME")
                .filter(|home| !home.is_empty())
                .map(|home| PathBuf::from(home).join(".codex"))
        })?;
    Some(
        codex_home
            .join("plugins")
            .join("cache")
            .join("openai-bundled")
            .join("unified-computer-use"),
    )
}

/// The newest version, by numeric parts, whose `.mcp.json` declares a usable
/// `mcpServers.cua_repl`.
pub fn resolve(plugin_dir: &Path) -> Result<RuntimeCommand, String> {
    let entries = std::fs::read_dir(plugin_dir).map_err(|error| {
        format!(
            "Computer Use runtime missing: cannot read {} ({error}). Install or open ChatGPT.app so it installs the unified-computer-use plugin.",
            plugin_dir.display()
        )
    })?;
    let mut versions: Vec<String> = entries
        .filter_map(Result::ok)
        .filter(|entry| entry.path().is_dir())
        .filter_map(|entry| entry.file_name().into_string().ok())
        .collect();
    versions.sort_by(|a, b| compare_versions(b, a));
    for version in &versions {
        if let Some(command) = read_manifest(&plugin_dir.join(version).join(".mcp.json"), version) {
            return Ok(command);
        }
    }
    Err(format!(
        "no .mcp.json with mcpServers.cua_repl under {}/<version>/ (versions: {})",
        plugin_dir.display(),
        if versions.is_empty() {
            "none".to_owned()
        } else {
            versions.join(", ")
        }
    ))
}

fn read_manifest(path: &Path, version: &str) -> Option<RuntimeCommand> {
    let manifest: Value = serde_json::from_slice(&std::fs::read(path).ok()?).ok()?;
    let server = manifest.pointer("/mcpServers/cua_repl")?;
    let command = server.get("command")?.as_str()?.to_owned();
    let args = server
        .get("args")?
        .as_array()?
        .iter()
        .filter_map(Value::as_str)
        .map(str::to_owned)
        .collect();
    let mut env: Vec<(String, String)> = server
        .get("env")
        .and_then(Value::as_object)
        .into_iter()
        .flatten()
        .filter(|(name, _)| name.as_str() != SURFACES_ENV)
        .filter_map(|(name, value)| Some((name.clone(), value.as_str()?.to_owned())))
        .collect();
    // Only the native computer surface: the browser surfaces need a Codex
    // turn token this host never has.
    env.push((SURFACES_ENV.to_owned(), "computer".to_owned()));
    Some(RuntimeCommand {
        version: version.to_owned(),
        command,
        args,
        env,
    })
}

const SURFACES_ENV: &str = "CUA_REPL_ENABLED_SURFACES";

/// Numeric part by part; a name with a non-numeric part loses to any
/// all-numeric one.
pub fn compare_versions(a: &str, b: &str) -> Ordering {
    let numeric = |version: &str| -> Option<Vec<u64>> {
        version.split('.').map(|part| part.parse().ok()).collect()
    };
    match (numeric(a), numeric(b)) {
        (Some(a), Some(b)) => a.cmp(&b),
        (Some(_), None) => Ordering::Greater,
        (None, Some(_)) => Ordering::Less,
        (None, None) => a.cmp(b),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn version(dir: &Path, name: &str, manifest: &str) {
        let path = dir.join(name);
        std::fs::create_dir_all(&path).unwrap();
        std::fs::write(path.join(".mcp.json"), manifest).unwrap();
    }

    fn manifest(command: &str) -> String {
        serde_json::json!({
            "mcpServers": { "cua_repl": {
                "command": command,
                "args": ["repl.mjs"],
                "env": { "A": "1", "CUA_REPL_ENABLED_SURFACES": "browser,computer" }
            }}
        })
        .to_string()
    }

    #[test]
    fn numeric_order_picks_the_newest_version() {
        let dir = tempfile::tempdir().unwrap();
        version(dir.path(), "26.915.31029", &manifest("/old/node"));
        version(dir.path(), "26.1007.71326", &manifest("/new/node"));
        let runtime = resolve(dir.path()).unwrap();
        assert_eq!(runtime.version, "26.1007.71326");
        assert_eq!(runtime.command, "/new/node");
        assert_eq!(runtime.args, ["repl.mjs"]);
        assert!(runtime.env.contains(&("A".into(), "1".into())));
        assert!(
            runtime
                .env
                .contains(&("CUA_REPL_ENABLED_SURFACES".into(), "computer".into()))
        );
        assert_eq!(
            runtime
                .env
                .iter()
                .filter(|(name, _)| name == "CUA_REPL_ENABLED_SURFACES")
                .count(),
            1
        );
    }

    #[test]
    fn version_without_cua_repl_is_skipped() {
        let dir = tempfile::tempdir().unwrap();
        version(dir.path(), "26.2000.1", r#"{"mcpServers":{"other":{}}}"#);
        version(dir.path(), "26.1000.1", &manifest("/usable/node"));
        assert_eq!(resolve(dir.path()).unwrap().version, "26.1000.1");
    }

    #[test]
    fn missing_dir_or_no_valid_version_is_an_error() {
        let dir = tempfile::tempdir().unwrap();
        assert!(resolve(&dir.path().join("absent")).is_err());
        version(dir.path(), "26.1.1", r#"{"mcpServers":{}}"#);
        let error = resolve(dir.path()).unwrap_err();
        assert!(error.contains("26.1.1"), "{error}");
    }

    #[test]
    fn non_numeric_names_lose_to_numeric_ones() {
        assert_eq!(compare_versions("26.1007.1", "26.915.1"), Ordering::Greater);
        assert_eq!(compare_versions("latest", "1.0"), Ordering::Less);
        assert_eq!(compare_versions("1.0", "beta"), Ordering::Greater);
    }
}
