use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use tempfile::TempDir;
use zeron_workers_unpeel::{
    controller_mcp_is_booting_screen, controller_mcp_is_briefing_screen_ready,
    controller_mcp_startup_prompt_response, controller_mcp_tracks_task_episode,
};

static ENV_LOCK: Mutex<()> = Mutex::new(());

const LITERAL_TASK: &str = "review \"quotes\"\n--flag && true; echo $HOME\n@not-a-file\nunicodé ✓";

struct IsolatedHome {
    _dir: TempDir,
    previous_home: Option<std::ffi::OsString>,
    previous_hooks: Option<std::ffi::OsString>,
}

impl IsolatedHome {
    fn new() -> Result<Self, Box<dyn std::error::Error>> {
        let dir = TempDir::new()?;
        let previous_home = std::env::var_os("UNPEEL_HOME");
        let previous_hooks = std::env::var_os("COMET_WORKERS_HOOKS_DIR");
        unsafe {
            std::env::set_var("UNPEEL_HOME", dir.path());
            std::env::set_var("COMET_WORKERS_HOOKS_DIR", dir.path().join("hooks"));
        }
        Ok(Self {
            _dir: dir,
            previous_home,
            previous_hooks,
        })
    }

    fn path(&self) -> &Path {
        self._dir.path()
    }
}

impl Drop for IsolatedHome {
    fn drop(&mut self) {
        unsafe {
            match &self.previous_home {
                Some(value) => std::env::set_var("UNPEEL_HOME", value),
                None => std::env::remove_var("UNPEEL_HOME"),
            }
            match &self.previous_hooks {
                Some(value) => std::env::set_var("COMET_WORKERS_HOOKS_DIR", value),
                None => std::env::remove_var("COMET_WORKERS_HOOKS_DIR"),
            }
        }
    }
}

fn write_fake_omp(bin_dir: &Path, probe: &Path) -> Result<PathBuf, Box<dyn std::error::Error>> {
    fs::create_dir_all(bin_dir)?;
    let omp = bin_dir.join("omp");
    fs::write(
        &omp,
        format!(
            r#"#!/bin/sh
PROBE='{}'
native=0
: > "$PROBE/argv"
while [ "$#" -gt 0 ]; do
  printf '%s\n' "$1" >> "$PROBE/argv"
  case "$1" in
    @*)
      cat "${{1#@}}" > "$PROBE/task"
      native=$((native + 1))
      ;;
  esac
  shift
done
printf '%s\n' "$native" > "$PROBE/native_executions"
printf 'Connecting to MCP servers: graft…\nMCP error: graft failed\n❯\n'
exit 0
"#,
            probe.display()
        ),
    )?;
    fs::set_permissions(&omp, fs::Permissions::from_mode(0o755))?;
    Ok(omp)
}

fn run_spawn_command(
    command: &str,
    bin_dir: &Path,
    cwd: &Path,
) -> Result<std::process::Output, Box<dyn std::error::Error>> {
    Ok(Command::new("sh")
        .arg("-c")
        .arg(command)
        .current_dir(cwd)
        .env(
            "PATH",
            format!(
                "{}:{}",
                bin_dir.display(),
                std::env::var("PATH").unwrap_or_default()
            ),
        )
        .output()?)
}

fn native_executions(probe: &Path) -> u64 {
    fs::read_to_string(probe.join("native_executions"))
        .ok()
        .and_then(|value| value.trim().parse().ok())
        .unwrap_or(0)
}

#[test]
fn omp_native_startup_executes_the_literal_task_once_despite_mcp_warning()
-> Result<(), Box<dyn std::error::Error>> {
    let _lock = ENV_LOCK.lock().expect("UNPEEL_HOME test lock");
    let home = IsolatedHome::new()?;
    let probe = home.path().join("probe");
    fs::create_dir_all(&probe)?;
    let bin = home.path().join("bin");
    write_fake_omp(&bin, &probe)?;

    let session_id = "omp-native-1";
    unpeel_core::omp_native_initial::stage_native_initial_prompt("omp", session_id, LITERAL_TASK)?;

    let startup =
        unpeel_core::integrations::startup_command("omp", "omp --yolo", false, false, false);
    assert!(
        !startup.contains("--auto-approve"),
        "native delivery must not add approval bypass: {startup}"
    );
    let spawn =
        unpeel_core::omp_native_initial::apply_native_initial_prompt("omp", &startup, session_id);
    assert!(
        !spawn.contains(LITERAL_TASK),
        "brief must stay out of command metadata: {spawn}"
    );

    let warning_screen = "Connecting to MCP servers: graft…\nMCP error: graft failed\n❯";
    assert!(controller_mcp_is_booting_screen(warning_screen));

    let output = run_spawn_command(&spawn, &bin, home.path())?;
    assert!(
        output.status.success(),
        "fake omp failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        native_executions(&probe),
        1,
        "OMP must receive the initial task through native startup exactly once; viewport MCP warning must not block it. stdout:\n{}",
        String::from_utf8_lossy(&output.stdout)
    );
    assert_eq!(fs::read_to_string(probe.join("task"))?, LITERAL_TASK);
    Ok(())
}

#[test]
fn restart_does_not_replay_the_native_initial_task() -> Result<(), Box<dyn std::error::Error>> {
    let _lock = ENV_LOCK.lock().expect("UNPEEL_HOME test lock");
    let home = IsolatedHome::new()?;
    let probe = home.path().join("probe");
    fs::create_dir_all(&probe)?;
    let bin = home.path().join("bin");
    write_fake_omp(&bin, &probe)?;
    let session_id = "omp-native-restart";
    unpeel_core::omp_native_initial::stage_native_initial_prompt("omp", session_id, LITERAL_TASK)?;

    let first = unpeel_core::omp_native_initial::apply_native_initial_prompt(
        "omp",
        "omp --yolo",
        session_id,
    );
    run_spawn_command(&first, &bin, home.path())?;
    assert_eq!(native_executions(&probe), 1);

    fs::write(probe.join("native_executions"), "0\n")?;
    let _ = fs::remove_file(probe.join("task"));
    let second = unpeel_core::omp_native_initial::apply_native_initial_prompt(
        "omp",
        "omp --yolo --continue",
        session_id,
    );
    assert!(
        !second.contains('@'),
        "resume argv must not carry the one-shot file: {second}"
    );
    run_spawn_command(&second, &bin, home.path())?;
    assert_eq!(native_executions(&probe), 0);
    assert!(!probe.join("task").exists());
    Ok(())
}

#[test]
fn missing_native_delivery_is_not_reported_as_submitted() {
    let _lock = ENV_LOCK.lock().expect("UNPEEL_HOME test lock");
    let _home = IsolatedHome::new().expect("isolated home");
    let session_id = "omp-native-missing";
    let spawn =
        unpeel_core::omp_native_initial::apply_native_initial_prompt("omp", "omp", session_id);
    assert_eq!(spawn, "omp");
    assert!(!unpeel_core::omp_native_initial::native_initial_prompt_attached(session_id));
}

#[test]
fn parent_task_episode_cutoff_survives_immediate_completion() {
    let registered_at = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64;
    assert!(controller_mcp_tracks_task_episode(
        Some("parent-chat"),
        true
    ));
    assert!(!controller_mcp_tracks_task_episode(None, true));
    assert!(registered_at > 0);
}

#[test]
fn interactive_runtimes_keep_shell_boot_and_menu_protections() {
    assert!(!controller_mcp_is_briefing_screen_ready(
        "claude",
        "$ echo hi\n$",
        5_000
    ));
    assert!(!controller_mcp_is_briefing_screen_ready(
        "codex",
        "Starting MCP servers (2/6): graft\n› Ask Codex to do anything",
        5_000
    ));
    assert!(!controller_mcp_is_briefing_screen_ready(
        "claude",
        "Choose setup:\n1. Continue\n2. Exit\nPress enter",
        1_000
    ));
    assert_eq!(controller_mcp_startup_prompt_response("login:"), None);
    let spawn = unpeel_core::omp_native_initial::apply_native_initial_prompt(
        "claude",
        "claude --permission-mode plan",
        "claude-1",
    );
    assert_eq!(spawn, "claude --permission-mode plan");
}
