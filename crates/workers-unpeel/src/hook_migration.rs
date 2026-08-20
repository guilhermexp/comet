use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

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

fn install_comet_managed_hooks() -> Result<(), String> {
    std::fs::create_dir_all(unpeel_core::app_paths::app_hooks_root())
        .map_err(|error| format!("Failed to create Comet hook root: {error}"))?;
    let mut installed = HashSet::new();
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
        unpeel_core::integrations::install_runtime_support(alias).map_err(|error| {
            format!(
                "Failed to install Comet hooks for runtime {} ({alias}): {error}",
                runtime.id
            )
        })?;
    }
    Ok(())
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
    std::fs::remove_dir_all(legacy_root).map_err(|error| {
        format!(
            "Failed to delete verified legacy hook root {}: {error}",
            legacy_root.display()
        )
    })?;
    Ok(true)
}

fn config_has_stale_managed_hook(raw: &str, legacy_root: &str) -> bool {
    if raw.contains(legacy_root) || raw.contains("/.unpeel/hooks/") {
        return true;
    }
    let temporary_root =
        raw.contains("/tmp/") || raw.contains("/private/tmp/") || raw.contains("/var/folders/");
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
        .any(|name| raw.contains(name))
}
