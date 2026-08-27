use std::path::PathBuf;

fn home_dir() -> PathBuf {
    dirs::home_dir().unwrap_or_else(|| PathBuf::from("."))
}

/// The Unpeel state dir: `~/.unpeel`, or the directory named by `UNPEEL_HOME`
/// when that env var is set and non-empty. The native app sets it for blank
/// dev instances and spawns hosts with the env inherited, so app + host agree
/// on one isolated state dir.
pub fn unpeel_home() -> PathBuf {
    if let Some(override_dir) = std::env::var_os("UNPEEL_HOME") {
        let trimmed = override_dir.to_string_lossy().trim().to_string();
        if !trimmed.is_empty() {
            return PathBuf::from(trimmed);
        }
    }
    home_dir().join(".unpeel")
}

/// Comet-owned runtime hook assets. Worker session state remains compatible
/// with the vendored runtime, but provider hooks do not depend on an Unpeel
/// application directory.
pub fn app_hooks_root() -> PathBuf {
    if let Some(override_dir) = std::env::var_os("COMET_WORKERS_HOOKS_DIR") {
        let trimmed = override_dir.to_string_lossy().trim().to_string();
        if !trimmed.is_empty() {
            return PathBuf::from(trimmed);
        }
    }
    home_dir().join(".zeron").join("workers").join("hooks")
}

/// Ensure `~/.unpeel` exists and is private to the current user (mode `0700`).
/// Session artifacts underneath (`output.bin`, `manifest.json`, `launch.json`)
/// can contain echoed secrets and the first typed prompt; a `0700` parent makes
/// them unreadable by other local users even though the files themselves are
/// created under the default umask. Idempotent and cheap; safe to call at every
/// host startup.
pub fn ensure_unpeel_home() -> std::io::Result<PathBuf> {
    let home = unpeel_home();
    std::fs::create_dir_all(&home)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Ok(metadata) = std::fs::metadata(&home) {
            let mut perms = metadata.permissions();
            if perms.mode() & 0o777 != 0o700 {
                perms.set_mode(0o700);
                let _ = std::fs::set_permissions(&home, perms);
            }
        }
    }
    Ok(home)
}

pub fn app_state_path() -> PathBuf {
    unpeel_home().join("app-state.json")
}

pub fn activity_state_path() -> PathBuf {
    unpeel_home().join("activity-state.json")
}

pub fn app_sessions_root() -> PathBuf {
    unpeel_home().join("app-sessions")
}

pub fn worktrees_root() -> PathBuf {
    unpeel_home().join("worktrees")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn managed_hooks_use_a_comet_owned_root() {
        let root = app_hooks_root();
        assert!(root.ends_with(".zeron/workers/hooks"));
        assert!(!root.to_string_lossy().contains("/.unpeel/hooks"));
    }
}
