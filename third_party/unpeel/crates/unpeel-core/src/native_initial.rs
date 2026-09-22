//! One-shot native startup input. The stored Session command never carries the
//! task; adapters declare file-argument or positional delivery, and this module
//! stages a private file and attaches it only on the first spawn argv.

use crate::integrations::shared;
use crate::session_host;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

pub const NATIVE_INITIAL_MESSAGE_FILE: &str = "native-initial-message";
pub const NATIVE_INITIAL_PENDING_FILE: &str = "native-initial-message.pending";
pub const NATIVE_INITIAL_CLAIMED_FILE: &str = "native-initial-message.claimed";
pub const NATIVE_INITIAL_ATTACHED_FILE: &str = "native-initial-message.attached";

pub fn uses_native_initial_delivery(runtime_or_command: &str) -> bool {
    crate::integrations::native_initial_input(runtime_or_command).is_some()
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

pub fn native_initial_prompt_claimed(session_id: &str) -> bool {
    session_host::session_dir(session_id)
        .join(NATIVE_INITIAL_CLAIMED_FILE)
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

pub fn resolve_enabled_preset_command<'a, I>(
    project_id: &str,
    preset_id: &str,
    presets: I,
) -> Option<&'a str>
where
    I: IntoIterator<Item = CatalogPreset<'a>>,
{
    let mut global = None;
    for preset in presets {
        if !preset.enabled || preset.id != preset_id {
            continue;
        }
        match preset.project_id {
            Some(id) if id == project_id => return Some(preset.command),
            None if global.is_none() => global = Some(preset.command),
            _ => {}
        }
    }
    global
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
    let _ = fs::remove_file(directory.join(NATIVE_INITIAL_CLAIMED_FILE));
    let _ = fs::remove_file(directory.join(NATIVE_INITIAL_ATTACHED_FILE));
    Ok(body)
}

/// Compute the first-spawn argv. Must not ACK: `.attached` is written only
/// after the real spawn/PTY submit succeeds. Native arguments are added only
/// when the pending reservation still has a body to attach.
pub fn prepare_native_initial_argv(
    runtime_or_command: &str,
    command: &str,
    session_id: &str,
) -> Result<String, String> {
    let Some(input) = crate::integrations::native_initial_input(runtime_or_command)
        .or_else(|| crate::integrations::native_initial_input(command))
    else {
        return Ok(command.to_string());
    };
    let directory = session_host::session_dir(session_id);
    let pending = directory.join(NATIVE_INITIAL_PENDING_FILE);
    let body = directory.join(NATIVE_INITIAL_MESSAGE_FILE);
    if !pending.is_file() || !body.is_file() {
        return Ok(command.to_string());
    }
    let attachment = match input {
        crate::integrations::NativeInitialInput::FileArgument => {
            shared::shell_quote(&format!("@{}", body.display()))
        }
        crate::integrations::NativeInitialInput::PositionalPrompt => {
            let text = fs::read_to_string(&body).map_err(|error| {
                format!(
                    "Failed to read native initial prompt {}: {error}",
                    body.display()
                )
            })?;
            format!("-- {}", shared::shell_quote(&text))
        }
    };
    Ok(format!("{} {}", command.trim(), attachment))
}

/// Claim a non-replayable reservation, run the Host's irreversible spawn/PTY
/// submit, then persist the receipt. Missing body never claims. Pre-submit
/// failure restores pending. Successful submit never restores pending; an ACK
/// failure keeps the Host result and leaves the reservation consumed.
pub fn submit_with_native_reservation<T>(
    session_id: &str,
    submit: impl FnOnce() -> Result<T, String>,
) -> Result<T, String> {
    let reservation = claim_native_initial(session_id)?;
    match submit() {
        Err(error) => {
            if let Some(reservation) = reservation {
                if let Err(restore_error) = reservation.restore_pending() {
                    return Err(format!(
                        "{error}; additionally failed to restore native initial reservation: {restore_error}"
                    ));
                }
            }
            Err(error)
        }
        Ok(value) => {
            if let Some(reservation) = reservation {
                if let Err(error) = reservation.consume() {
                    log::warn!(
                        "session {session_id} native initial was submitted but the confirmation receipt could not be persisted: {error}"
                    );
                }
            }
            Ok(value)
        }
    }
}

struct NativeInitialReservation {
    session_id: String,
}

fn claim_native_initial(session_id: &str) -> Result<Option<NativeInitialReservation>, String> {
    let directory = session_host::session_dir(session_id);
    let pending = directory.join(NATIVE_INITIAL_PENDING_FILE);
    let claimed = directory.join(NATIVE_INITIAL_CLAIMED_FILE);
    let body = directory.join(NATIVE_INITIAL_MESSAGE_FILE);
    if claimed.is_file() {
        return Ok(None);
    }
    if !pending.is_file() {
        return Ok(None);
    }
    if !body.is_file() {
        return Ok(None);
    }
    fs::rename(&pending, &claimed)
        .map_err(|error| format!("Failed to claim native initial reservation: {error}"))?;
    enforce_private_mode(&claimed, 0o600)?;
    Ok(Some(NativeInitialReservation {
        session_id: session_id.to_owned(),
    }))
}

impl NativeInitialReservation {
    fn restore_pending(self) -> Result<(), String> {
        let directory = session_host::session_dir(&self.session_id);
        let pending = directory.join(NATIVE_INITIAL_PENDING_FILE);
        let claimed = directory.join(NATIVE_INITIAL_CLAIMED_FILE);
        if pending.is_file() {
            let _ = fs::remove_file(&claimed);
            return Ok(());
        }
        if !claimed.is_file() {
            return Ok(());
        }
        fs::rename(&claimed, &pending).map_err(|error| {
            format!("Failed to restore native initial reservation to pending: {error}")
        })?;
        enforce_private_mode(&pending, 0o600)?;
        Ok(())
    }

    fn consume(self) -> Result<(), String> {
        let directory = session_host::session_dir(&self.session_id);
        let claimed = directory.join(NATIVE_INITIAL_CLAIMED_FILE);
        let attached = directory.join(NATIVE_INITIAL_ATTACHED_FILE);
        let pending = directory.join(NATIVE_INITIAL_PENDING_FILE);
        if pending.is_file() {
            fs::remove_file(&pending).map_err(|error| {
                format!("native initial pending survived submit and would replay: {error}")
            })?;
        }
        if attached.is_file() {
            let _ = fs::remove_file(&claimed);
            enforce_private_mode(&attached, 0o600)?;
            return Ok(());
        }
        if !claimed.is_file() {
            return Ok(());
        }
        fs::rename(&claimed, &attached)
            .map_err(|error| format!("Failed to ack native initial prompt: {error}"))?;
        enforce_private_mode(&attached, 0o600)?;
        Ok(())
    }
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
        let first = prepare_native_initial_argv("omp", "omp --yolo", "s1").expect("prepare");
        assert!(first.contains(&format!("'@{}'", path.display())), "{first}");
        assert!(!first.contains(briefing), "{first}");
        assert!(!first.contains("--auto-approve"), "{first}");
        assert_eq!(std::fs::read_to_string(&path).expect("body"), briefing);
        assert!(!native_initial_prompt_attached("s1"));
        submit_with_native_reservation("s1", || Ok(())).expect("submit");
        assert!(native_initial_prompt_attached("s1"));

        let second =
            prepare_native_initial_argv("omp", "omp --yolo --continue", "s1").expect("prepare");
        assert_eq!(second, "omp --yolo --continue");
        restore_home(previous);
    }

    #[test]
    fn unsupported_runtimes_do_not_consume_native_prompt_files() {
        let _lock = ENV_LOCK.lock().expect("lock");
        let (_dir, previous) = isolated_home();
        stage_native_initial_prompt("omp", "s2", "secret").expect("stage");
        let command =
            prepare_native_initial_argv("prime-agent", "prime-agent", "s2").expect("prepare");
        assert_eq!(command, "prime-agent");
        restore_home(previous);
    }

    #[test]
    fn positional_runtime_attaches_quoted_body_once() {
        let _lock = ENV_LOCK.lock().expect("lock");
        let (_dir, previous) = isolated_home();
        let briefing = "--flag && true; echo $HOME\nunicodé ✓\n\n";
        stage_native_initial_prompt("claude", "s3", briefing).expect("stage");
        let first = prepare_native_initial_argv("claude", "claude --permission-mode plan", "s3")
            .expect("prepare");
        assert!(first.contains("-- "), "{first}");
        assert!(first.contains(&shared::shell_quote(briefing)), "{first}");
        assert!(!first.contains("--auto-approve"), "{first}");
        assert!(!native_initial_prompt_attached("s3"));
        submit_with_native_reservation("s3", || Ok(())).expect("submit");
        assert!(native_initial_prompt_attached("s3"));
        let second =
            prepare_native_initial_argv("claude", "claude --permission-mode plan --continue", "s3")
                .expect("prepare");
        assert_eq!(second, "claude --permission-mode plan --continue");
        restore_home(previous);
    }
}
