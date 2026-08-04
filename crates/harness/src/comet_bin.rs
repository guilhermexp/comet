//! Locating our own binary and the engine's IPC port.
//!
//! Two callers need this and neither should guess: `comet-tui` spawns
//! `comet headless` with it, and [`crate::worker_tools`] points the agent's MCP
//! client at `comet mcp-server`.

use std::path::PathBuf;

use anyhow::bail;

/// The engine's IPC port when nothing says otherwise.
pub const DEFAULT_IPC_PORT: u16 = 27654;

/// Find the `comet` binary. Checked in order of "most likely to be the one the
/// user means": an explicit override, the binary sitting next to this one (a
/// cargo target dir or an installed `app/current/`), PATH, then the installer's
/// well-known location.
pub fn resolve_comet_bin() -> anyhow::Result<PathBuf> {
    let exe_name = if cfg!(windows) { "comet.exe" } else { "comet" };

    if let Some(explicit) = std::env::var_os("COMET_BIN").map(PathBuf::from) {
        if explicit.is_file() {
            return Ok(explicit);
        }
        bail!("COMET_BIN={} is not a file", explicit.display());
    }

    if let Ok(self_exe) = std::env::current_exe()
        && let Some(dir) = self_exe.parent()
    {
        let sibling = dir.join(exe_name);
        if sibling.is_file() {
            return Ok(sibling);
        }
    }

    if let Some(path) = std::env::var_os("PATH") {
        for dir in std::env::split_paths(&path) {
            let candidate = dir.join(exe_name);
            if candidate.is_file() {
                return Ok(candidate);
            }
        }
    }

    if let Some(home) = std::env::var_os("HOME") {
        let installed = PathBuf::from(home)
            .join(".comet-native/app/current")
            .join(exe_name);
        if installed.is_file() {
            return Ok(installed);
        }
    }

    bail!("`{exe_name}` not found next to this binary, on PATH, or under ~/.comet-native/app")
}

/// The port the engine serves IPC on. Same resolution every other launch path
/// uses (`apps/comet/src/main.rs`, `comet_ui::UiConfig`), so the harness and the
/// engine that runs it always agree without a new channel between them.
pub fn ipc_port() -> u16 {
    std::env::var("COMET_IPC_PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(DEFAULT_IPC_PORT)
}
