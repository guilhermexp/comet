//! One-shot OMP startup input. The stored Session command never carries the
//! task; a private file is consumed through OMP's `@file` argument only on the
//! first spawn argv.

use crate::integrations::shared;
use crate::session_host;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

pub const NATIVE_INITIAL_MESSAGE_FILE: &str = "native-initial-message";
pub const NATIVE_INITIAL_PENDING_FILE: &str = "native-initial-message.pending";
pub const NATIVE_INITIAL_ATTACHED_FILE: &str = "native-initial-message.attached";

pub fn uses_native_initial_delivery(runtime_or_command: &str) -> bool {
    matches!(
        crate::integrations::command_head(runtime_or_command),
        "omp" | "omp-cli"
    )
}

pub fn native_initial_message_path(session_id: &str) -> PathBuf {
    session_host::session_dir(session_id).join(NATIVE_INITIAL_MESSAGE_FILE)
}

pub fn native_initial_prompt_attached(session_id: &str) -> bool {
    session_host::session_dir(session_id)
        .join(NATIVE_INITIAL_ATTACHED_FILE)
        .is_file()
}

pub fn native_initial_prompt_pending(session_id: &str) -> bool {
    session_host::session_dir(session_id)
        .join(NATIVE_INITIAL_PENDING_FILE)
        .is_file()
}

/// Host-equivalent catalog row. MCP must resolve the same enabled
/// project-scoped-then-global command the Host will spawn — never `cli_id`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CatalogPreset<'a> {
    pub id: &'a str,
    pub command: &'a str,
    pub enabled: bool,
    pub project_id: Option<&'a str>,
}

pub fn resolve_enabled_preset_command<'a>(
    project_id: &str,
    preset_id: &str,
    presets: &[CatalogPreset<'a>],
) -> Option<&'a str> {
    presets
        .iter()
        .find(|preset| {
            preset.enabled && preset.id == preset_id && preset.project_id == Some(project_id)
        })
        .or_else(|| {
            presets.iter().find(|preset| {
                preset.enabled && preset.id == preset_id && preset.project_id.is_none()
            })
        })
        .map(|preset| preset.command)
}

pub fn native_initial_is_authorized(command: &str, submit_and_run: bool) -> bool {
    submit_and_run && uses_native_initial_delivery(command)
}

pub fn stage_native_initial_prompt(
    runtime_or_command: &str,
    session_id: &str,
    text: &str,
) -> Result<PathBuf, String> {
    if !uses_native_initial_delivery(runtime_or_command) {
        return Err(format!(
            "runtime '{}' has no native initial-input path",
            crate::integrations::command_head(runtime_or_command)
        ));
    }
    if text.is_empty() {
        return Err("native initial prompt is empty".into());
    }
    let directory = session_host::session_dir(session_id);
    fs::create_dir_all(&directory)
        .map_err(|error| format!("Failed to create session dir for native prompt: {error}"))?;
    enforce_private_mode(&directory, 0o700)?;
    let body = directory.join(NATIVE_INITIAL_MESSAGE_FILE);
    write_private_file(&body, text.as_bytes())?;
    write_private_file(&directory.join(NATIVE_INITIAL_PENDING_FILE), b"")?;
    let _ = fs::remove_file(directory.join(NATIVE_INITIAL_ATTACHED_FILE));
    Ok(body)
}

pub fn apply_native_initial_prompt(
    runtime_or_command: &str,
    command: &str,
    session_id: &str,
) -> String {
    prepare_native_initial_argv(runtime_or_command, command, session_id)
        .unwrap_or_else(|_| command.to_string())
}

/// Compute the first-spawn argv. Must not ACK: `.attached` is written only
/// after the real spawn/PTY submit succeeds.
pub fn prepare_native_initial_argv(
    runtime_or_command: &str,
    command: &str,
    session_id: &str,
) -> Result<String, String> {
    if !(uses_native_initial_delivery(runtime_or_command) || uses_native_initial_delivery(command))
    {
        return Ok(command.to_string());
    }
    let directory = session_host::session_dir(session_id);
    let pending = directory.join(NATIVE_INITIAL_PENDING_FILE);
    let body = directory.join(NATIVE_INITIAL_MESSAGE_FILE);
    if !pending.is_file() || !body.is_file() {
        return Ok(command.to_string());
    }
    let token = format!("@{}", body.display());
    Ok(format!(
        "{} {}",
        command.trim(),
        shared::shell_quote(&token)
    ))
}

/// Commit the one-shot after the host actually spawned or submitted.
/// Rename pending → attached so a failed ACK cannot both claim success and
/// leave a replayable pending file.
pub fn ack_native_initial_prompt(session_id: &str) -> Result<(), String> {
    let directory = session_host::session_dir(session_id);
    let pending = directory.join(NATIVE_INITIAL_PENDING_FILE);
    let attached = directory.join(NATIVE_INITIAL_ATTACHED_FILE);
    if attached.is_file() {
        if pending.is_file() {
            fs::remove_file(&pending).map_err(|error| {
                format!("native initial pending survived ack and would replay: {error}")
            })?;
        }
        enforce_private_mode(&attached, 0o600)?;
        return Ok(());
    }
    if !pending.is_file() {
        return Ok(());
    }
    fs::rename(&pending, &attached)
        .map_err(|error| format!("Failed to ack native initial prompt: {error}"))?;
    enforce_private_mode(&attached, 0o600)?;
    Ok(())
}

fn write_private_file(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let mut options = fs::OpenOptions::new();
    options.create(true).write(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options
        .open(path)
        .map_err(|error| format!("Failed to write {}: {error}", path.display()))?;
    file.write_all(bytes)
        .map_err(|error| format!("Failed to write {}: {error}", path.display()))?;
    drop(file);
    enforce_private_mode(path, 0o600)?;
    Ok(())
}

fn enforce_private_mode(path: &Path, mode: u32) -> Result<(), String> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(mode)).map_err(|error| {
            format!("Failed to set mode {mode:o} on {}: {error}", path.display())
        })?;
    }
    let _ = mode;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    fn isolated_home() -> (tempfile::TempDir, Option<std::ffi::OsString>) {
        let dir = tempfile::TempDir::new().expect("tempdir");
        let previous = std::env::var_os("UNPEEL_HOME");
        unsafe { std::env::set_var("UNPEEL_HOME", dir.path()) };
        (dir, previous)
    }

    fn restore_home(previous: Option<std::ffi::OsString>) {
        unsafe {
            match previous {
                Some(value) => std::env::set_var("UNPEEL_HOME", value),
                None => std::env::remove_var("UNPEEL_HOME"),
            }
        }
    }

    #[test]
    fn omp_spawn_argv_consumes_the_private_file_once() {
        let _lock = ENV_LOCK.lock().expect("lock");
        let (_dir, previous) = isolated_home();
        let briefing = "hello \"world\"\n--flag\n@other";
        let path = stage_native_initial_prompt("omp", "s1", briefing).expect("stage");
        let first = apply_native_initial_prompt("omp", "omp --yolo", "s1");
        assert!(first.contains(&format!("'@{}'", path.display())), "{first}");
        assert!(!first.contains(briefing), "{first}");
        assert!(!first.contains("--auto-approve"), "{first}");
        assert_eq!(std::fs::read_to_string(&path).expect("body"), briefing);
        assert!(!native_initial_prompt_attached("s1"));
        ack_native_initial_prompt("s1").expect("ack");
        assert!(native_initial_prompt_attached("s1"));

        let second = apply_native_initial_prompt("omp", "omp --yolo --continue", "s1");
        assert_eq!(second, "omp --yolo --continue");
        restore_home(previous);
    }

    #[test]
    fn other_runtimes_do_not_consume_omp_prompt_files() {
        let _lock = ENV_LOCK.lock().expect("lock");
        let (_dir, previous) = isolated_home();
        stage_native_initial_prompt("omp", "s2", "secret").expect("stage");
        let command = apply_native_initial_prompt("claude", "claude", "s2");
        assert_eq!(command, "claude");
        restore_home(previous);
    }
}
