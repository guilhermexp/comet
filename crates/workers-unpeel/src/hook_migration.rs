use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// Assets the pinned upstream still resolves under the legacy hook root.
///
/// `third_party/unpeel/runtimes/_shared/pi-family/adapter/setup.rs` both writes
/// and reads the pi-family lifecycle extension at `<unpeel_home>/hooks`, while
/// every Comet-managed hook script lives under `app_hooks_root()`. Deleting the
/// legacy root wholesale therefore strips a live launch dependency: pi-family
/// runtimes (`omp`) are started with `--extension <that path>`, and a missing
/// file makes the runtime load no extension at all, so the session never emits
/// Start/Stop/PermissionRequest and its activity stays pinned at `idle` while
/// the PTY streams. Vendored code is read-only here, so the migration keeps
/// what upstream still owns instead of patching the path.
const UPSTREAM_OWNED_LEGACY_ASSETS: &[&str] = &["pi-family-lifecycle-extension.js"];

pub(crate) fn ensure_managed_hook_migration() -> Result<(), String> {
    static INSTALL_RESULT: OnceLock<Result<(), String>> = OnceLock::new();
    INSTALL_RESULT
        .get_or_init(install_comet_managed_hooks)
        .clone()?;

    let legacy_root = unpeel_core::app_paths::unpeel_home().join("hooks");
    let has_live_sessions = unpeel_core::session_host::list_manifests()
        .into_iter()
        .any(|manifest| manifest.state == unpeel_core::session_host::HostedSessionState::Running);
    remove_legacy_hook_root_at(
        &legacy_root,
        &managed_provider_config_paths(),
        has_live_sessions,
    )?;
    Ok(())
}

pub(crate) fn is_comet_application_process() -> bool {
    std::env::current_exe()
        .ok()
        .and_then(|path| path.file_stem().map(|name| name.to_owned()))
        .is_some_and(|name| name == "zeron")
}

/// Install every runtime's Comet-managed hooks.
///
/// One runtime's failure must not skip the runtimes behind it in the catalog:
/// the loop is the only installer, so an early `?` used to leave every later
/// runtime hookless while the user saw the ones ahead of it working.
fn install_comet_managed_hooks() -> Result<(), String> {
    std::fs::create_dir_all(unpeel_core::app_paths::app_hooks_root())
        .map_err(|error| format!("Failed to create Comet hook root: {error}"))?;
    let mut installed = HashSet::new();
    let mut failures = Vec::new();
    for runtime in
        unpeel_core::runtime_catalog::builtin_runtime_catalog().current_platform_descriptors()
    {
        let Some(alias) = runtime.detection.command_aliases.first() else {
            continue;
        };
        if !unpeel_core::integrations::has_runtime_support_installer(alias)
            || !installed.insert(runtime.id.clone())
        {
            continue;
        }
        if let Err(error) = unpeel_core::integrations::install_runtime_support(alias) {
            failures.push(format!(
                "Failed to install Comet hooks for runtime {} ({alias}): {error}",
                runtime.id
            ));
        }
    }
    if failures.is_empty() {
        return Ok(());
    }
    Err(failures.join("; "))
}

/// Restore the upstream-owned launch assets when any is missing.
///
/// The pinned upstream resolves the pi-family lifecycle extension under the
/// legacy hook root at spawn time (`--extension <unpeel_home>/hooks/...`): a
/// missing file launches the runtime with no extension at all, so the session
/// never emits Start/Stop and stays visually idle. Installation is otherwise
/// lazy and per-process, which lets the first launch of a fresh process race
/// the install. Cheap when healthy: one metadata probe per asset.
pub(crate) fn ensure_upstream_owned_launch_assets() -> Result<(), String> {
    let legacy_root = unpeel_core::app_paths::unpeel_home().join("hooks");
    if UPSTREAM_OWNED_LEGACY_ASSETS
        .iter()
        .all(|asset| legacy_root.join(asset).is_file())
    {
        return Ok(());
    }
    install_comet_managed_hooks()
}

fn managed_provider_config_paths() -> Vec<PathBuf> {
    let Some(home) = dirs::home_dir() else {
        return Vec::new();
    };
    [
        ".claude/settings.json",
        ".codex/hooks.json",
        ".codex/config.toml",
        ".kimi/config.toml",
        ".kimi-code/config.toml",
        ".gemini/settings.json",
        ".grok/config.toml",
        ".kiro/settings.json",
        ".copilot/config.json",
    ]
    .into_iter()
    .map(|relative| home.join(relative))
    .collect()
}

pub fn remove_legacy_hook_root_at(
    legacy_root: &Path,
    config_paths: &[PathBuf],
    has_live_sessions: bool,
) -> Result<bool, String> {
    let legacy = legacy_root.to_string_lossy();
    let mut references = Vec::new();
    for path in config_paths {
        let raw = match std::fs::read_to_string(path) {
            Ok(raw) => raw,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => {
                return Err(format!(
                    "Failed to verify managed hook config {}: {error}",
                    path.display()
                ));
            }
        };
        if config_has_stale_managed_hook(&raw, legacy.as_ref()) {
            references.push(path.display().to_string());
        }
    }
    if !references.is_empty() {
        return Err(format!(
            "Provider configs still reference a stale managed hook: {}",
            references.join(", ")
        ));
    }
    if !legacy_root.exists() {
        return Ok(false);
    }
    if has_live_sessions {
        return Ok(false);
    }
    prune_legacy_hook_root(legacy_root)
}

/// Delete every migrated asset in the legacy root, keeping the entries the
/// pinned upstream still resolves there. The root itself is removed only once
/// nothing upstream-owned is left, so a repeat migration is a no-op.
fn prune_legacy_hook_root(legacy_root: &Path) -> Result<bool, String> {
    let read_error = |error: std::io::Error| {
        format!(
            "Failed to read verified legacy hook root {}: {error}",
            legacy_root.display()
        )
    };
    let mut removed = false;
    let mut retained = false;
    for entry in std::fs::read_dir(legacy_root).map_err(read_error)? {
        let entry = entry.map_err(read_error)?;
        let path = entry.path();
        if entry
            .file_name()
            .to_str()
            .is_some_and(|name| UPSTREAM_OWNED_LEGACY_ASSETS.contains(&name))
        {
            retained = true;
            continue;
        }
        let outcome = if entry.file_type().is_ok_and(|kind| kind.is_dir()) {
            std::fs::remove_dir_all(&path)
        } else {
            std::fs::remove_file(&path)
        };
        outcome.map_err(|error| {
            format!(
                "Failed to delete verified legacy hook asset {}: {error}",
                path.display()
            )
        })?;
        removed = true;
    }
    if retained {
        return Ok(removed);
    }
    std::fs::remove_dir_all(legacy_root).map_err(|error| {
        format!(
            "Failed to delete verified legacy hook root {}: {error}",
            legacy_root.display()
        )
    })?;
    Ok(true)
}

/// A provider config is stale when it still points at a managed hook under the
/// legacy root or under a throwaway root (demo/test homes).
///
/// The throwaway-root probe is per line, because a config command is one line
/// in every provider format we write. Whole-file matching made any unrelated
/// tool's temp-path hook (an orchestrator wrapper in `~/.codex/hooks.json`)
/// look like our own stale asset and blocked the migration for good.
fn config_has_stale_managed_hook(raw: &str, legacy_root: &str) -> bool {
    if raw.contains(legacy_root) || raw.contains("/.unpeel/hooks/") {
        return true;
    }
    raw.lines().any(|line| {
        let temporary_root = line.contains("/tmp/")
            || line.contains("/private/tmp/")
            || line.contains("/var/folders/");
        temporary_root
            && [
                "claude-hooks.sh",
                "codex-notify-hook.sh",
                "notify-hook.sh",
                "kimi-hook.sh",
                "gemini-hook.sh",
                "grok-hook.sh",
                "cursor-hook.sh",
                "kiro-hook.sh",
                "copilot-hook.sh",
                "cline-hook.sh",
            ]
            .iter()
            .any(|name| line.contains(name))
    })
}
