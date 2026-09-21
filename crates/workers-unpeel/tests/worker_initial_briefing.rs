use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Mutex;

use tempfile::TempDir;
use unpeel_core::controller_api::{HostCreateSubmitMode, native_initial_startup_enabled};
use zeron_workers_unpeel::{
    WorkerCompletionEvidence, WorkerParentNotificationKind, WorkersPresetSetting, WorkersSession,
    WorkersSessionCapabilities, begin_worker_parent_task_at, controller_mcp_is_booting_screen,
    controller_mcp_is_briefing_screen_ready, controller_mcp_native_initial_from_presets,
    controller_mcp_startup_prompt_response, pending_worker_parent_notifications_with_evidence_at,
    register_worker_parent_at,
};

static ENV_LOCK: Mutex<()> = Mutex::new(());

fn env_lock() -> std::sync::MutexGuard<'static, ()> {
    ENV_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

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
) -> Result<std::process::Output, String> {
    Command::new("sh")
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
        .output()
        .map_err(|error| error.to_string())
}

fn native_executions(probe: &Path) -> u64 {
    fs::read_to_string(probe.join("native_executions"))
        .ok()
        .and_then(|value| value.trim().parse().ok())
        .unwrap_or(0)
}

fn unix_mode(path: &Path) -> u32 {
    fs::metadata(path).expect("metadata").permissions().mode() & 0o777
}

fn catalog_preset(
    id: &str,
    command: &str,
    enabled: bool,
    project_id: Option<&str>,
    cli_id: Option<&str>,
) -> WorkersPresetSetting {
    WorkersPresetSetting {
        id: id.into(),
        label: id.into(),
        command: command.into(),
        project_id: project_id.map(str::to_owned),
        cli_id: cli_id.map(str::to_owned),
        enabled,
        quick_launch: false,
        is_default: false,
        installed: true,
        supports_quick_launch: true,
        risky: false,
        icon: String::new(),
        tint_color_hex: None,
    }
}

fn parent_session(id: &str, generation: u64, activity: &str, state: &str) -> WorkersSession {
    WorkersSession {
        id: id.into(),
        project_id: "project-1".into(),
        title: "Review parser".into(),
        command: "omp".into(),
        state: state.into(),
        activity: activity.into(),
        unread: activity == "done",
        pinned: false,
        archived: false,
        provider_id: Some("omp".into()),
        active_runtime_id: Some("omp".into()),
        runtime_launch_pending: false,
        runtime_generation: generation,
        notify_when_done: false,
        terminal_background_hex: None,
        worktree_branch: None,
        created_at_unix_ms: 1_000,
        updated_at_unix_ms: 1_001,
        idle_since_unix_ms: None,
        idle_confirmed_by_hook: false,
        resumable_conversation: false,
        total_tokens: None,
        model_usage: Vec::new(),
        capabilities: WorkersSessionCapabilities::default(),
    }
}

fn write_stop_hook(root: &Path, session_id: &str, occurred_at_unix_ms: u64) {
    let dir = root.join(session_id);
    fs::create_dir_all(&dir).unwrap();
    fs::write(
        dir.join("comet-hook-events.jsonl"),
        format!(
            "{}\n",
            serde_json::json!({
                "sequence": 1,
                "hook_event_name": "Stop",
                "runtime_generation": 1,
                "occurred_at_unix_ms": occurred_at_unix_ms,
                "source_modified_unix_ns": 1u64
            })
        ),
    )
    .unwrap();
}

#[test]
fn omp_native_startup_executes_the_literal_task_once_despite_mcp_warning()
-> Result<(), Box<dyn std::error::Error>> {
    let _lock = env_lock();
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
        unpeel_core::omp_native_initial::prepare_native_initial_argv("omp", &startup, session_id)?;
    assert!(
        !spawn.contains(LITERAL_TASK),
        "brief must stay out of command metadata: {spawn}"
    );

    let warning_screen = "Connecting to MCP servers: graft…\nMCP error: graft failed\n❯";
    assert!(controller_mcp_is_booting_screen(warning_screen));

    let output =
        unpeel_core::omp_native_initial::submit_with_native_reservation(session_id, || {
            run_spawn_command(&spawn, &bin, home.path())
        })?;
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
    assert!(
        unpeel_core::omp_native_initial::native_initial_prompt_attached(session_id),
        "successful Host submit must persist the native receipt"
    );
    Ok(())
}

#[test]
fn restart_does_not_replay_the_native_initial_task() -> Result<(), Box<dyn std::error::Error>> {
    let _lock = env_lock();
    let home = IsolatedHome::new()?;
    let probe = home.path().join("probe");
    fs::create_dir_all(&probe)?;
    let bin = home.path().join("bin");
    write_fake_omp(&bin, &probe)?;
    let session_id = "omp-native-restart";
    unpeel_core::omp_native_initial::stage_native_initial_prompt("omp", session_id, LITERAL_TASK)?;

    let first = unpeel_core::omp_native_initial::prepare_native_initial_argv(
        "omp",
        "omp --yolo",
        session_id,
    )?;
    unpeel_core::omp_native_initial::submit_with_native_reservation(session_id, || {
        run_spawn_command(&first, &bin, home.path())
    })?;
    assert_eq!(native_executions(&probe), 1);

    fs::write(probe.join("native_executions"), "0\n")?;
    let _ = fs::remove_file(probe.join("task"));
    let second = unpeel_core::omp_native_initial::prepare_native_initial_argv(
        "omp",
        "omp --yolo --continue",
        session_id,
    )?;
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
    let _lock = env_lock();
    let _home = IsolatedHome::new().expect("isolated home");
    let session_id = "omp-native-missing";
    let spawn =
        unpeel_core::omp_native_initial::prepare_native_initial_argv("omp", "omp", session_id)
            .expect("prepare");
    assert_eq!(spawn, "omp");
    assert!(!unpeel_core::omp_native_initial::native_initial_prompt_attached(session_id));
}

#[test]
fn failed_spawn_does_not_report_briefing_submitted() -> Result<(), Box<dyn std::error::Error>> {
    let _lock = env_lock();
    let _home = IsolatedHome::new()?;
    let session_id = "omp-native-spawn-fail";
    unpeel_core::omp_native_initial::stage_native_initial_prompt("omp", session_id, LITERAL_TASK)?;
    let argv = unpeel_core::omp_native_initial::prepare_native_initial_argv(
        "omp",
        "omp --yolo",
        session_id,
    )?;
    assert!(
        argv.contains('@'),
        "prepare must still attach @file in argv: {argv}"
    );
    let spawned =
        unpeel_core::omp_native_initial::submit_with_native_reservation(session_id, || {
            Command::new("/bin/sh")
                .arg("-c")
                .arg(&argv)
                .current_dir(_home.path().join("missing-cwd"))
                .spawn()
                .map(|_| ())
                .map_err(|error| error.to_string())
        });
    assert!(
        spawned.is_err(),
        "prepared command must fail at the Host submission seam: {spawned:?}"
    );
    assert!(
        !unpeel_core::omp_native_initial::native_initial_prompt_attached(session_id),
        "failed spawn must not report briefing_submitted"
    );
    assert!(
        unpeel_core::omp_native_initial::native_initial_prompt_pending(session_id),
        "pending must survive a failed spawn so recovery can retry without replay-after-ACK"
    );
    Ok(())
}

#[test]
fn missing_body_does_not_ack_a_surviving_pending() -> Result<(), Box<dyn std::error::Error>> {
    let _lock = env_lock();
    let _home = IsolatedHome::new()?;
    let session_id = "omp-native-missing-body";
    unpeel_core::omp_native_initial::stage_native_initial_prompt("omp", session_id, LITERAL_TASK)?;
    fs::remove_file(unpeel_core::omp_native_initial::native_initial_message_path(session_id))?;
    let spawn = unpeel_core::omp_native_initial::prepare_native_initial_argv(
        "omp",
        "omp --yolo",
        session_id,
    )?;
    assert_eq!(spawn, "omp --yolo");
    unpeel_core::omp_native_initial::submit_with_native_reservation(session_id, || Ok(()))?;
    assert!(
        !unpeel_core::omp_native_initial::native_initial_prompt_attached(session_id),
        "body ausente must not turn pending into attached"
    );
    Ok(())
}

#[test]
fn ack_failure_after_submit_keeps_result_and_does_not_replay()
-> Result<(), Box<dyn std::error::Error>> {
    let _lock = env_lock();
    let _home = IsolatedHome::new()?;
    let session_id = "omp-native-ack-fail";
    unpeel_core::omp_native_initial::stage_native_initial_prompt("omp", session_id, LITERAL_TASK)?;
    let first = unpeel_core::omp_native_initial::prepare_native_initial_argv(
        "omp",
        "omp --yolo",
        session_id,
    )?;
    assert!(first.contains('@'), "{first}");
    let attached = unpeel_core::session_host::session_dir(session_id)
        .join(unpeel_core::omp_native_initial::NATIVE_INITIAL_ATTACHED_FILE);
    fs::create_dir(&attached)?;
    let result =
        unpeel_core::omp_native_initial::submit_with_native_reservation(session_id, || Ok("child"));
    assert_eq!(
        result.as_deref(),
        Ok("child"),
        "ACK failure after successful submit must keep the Host result: {result:?}"
    );
    assert!(
        !unpeel_core::omp_native_initial::native_initial_prompt_pending(session_id),
        "successful submit must consume the reservation even when the receipt cannot be persisted"
    );
    let second = unpeel_core::omp_native_initial::prepare_native_initial_argv(
        "omp",
        "omp --yolo --continue",
        session_id,
    )?;
    assert!(
        !second.contains('@'),
        "ACK failure must not leave a replayable pending file: {second}"
    );
    Ok(())
}

#[test]
fn paste_only_and_raw_keep_host_pty_contracts() {
    assert!(
        !native_initial_startup_enabled("omp", HostCreateSubmitMode::PasteOnly),
        "PasteOnly must keep Host PTY contracts; native startup is PasteAndSubmit only"
    );
    assert!(
        !native_initial_startup_enabled("omp", HostCreateSubmitMode::Raw),
        "Raw must keep Host PTY contracts; native startup is PasteAndSubmit only"
    );
    assert!(native_initial_startup_enabled(
        "omp",
        HostCreateSubmitMode::PasteAndSubmit
    ));
    assert!(!native_initial_startup_enabled(
        "claude",
        HostCreateSubmitMode::PasteAndSubmit
    ));
}

#[test]
fn mcp_native_decision_matches_host_command_resolution() {
    let presets = [
        catalog_preset("omp", "omp --yolo", true, None, Some("omp")),
        catalog_preset(
            "omp",
            "claude --permission-mode plan",
            true,
            Some("proj"),
            Some("omp"),
        ),
        catalog_preset("stale", "claude", true, None, Some("omp")),
    ];
    assert!(
        !controller_mcp_native_initial_from_presets("proj", "omp", &presets),
        "project-scoped command wins; stale cli_id must not make MCP wait for an ACK Host never writes"
    );
    assert!(
        !controller_mcp_native_initial_from_presets("other", "stale", &presets),
        "cli_id must not override the command Host will spawn"
    );
    assert!(controller_mcp_native_initial_from_presets(
        "other", "omp", &presets
    ));
}

#[test]
fn parent_task_episode_cutoff_survives_immediate_completion() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("app-state.json");
    fs::write(
        &path,
        serde_json::to_vec(&serde_json::json!({
            "projects": [],
            "presets": [],
            "active_tabs": {},
            "pinned_sessions": {}
        }))
        .unwrap(),
    )
    .unwrap();
    let sessions_root = dir.path().join("sessions");
    let cutoff = 500u64;
    write_stop_hook(&sessions_root, "worker-fast", 800);
    register_worker_parent_at(&path, "worker-fast", "parent-chat", cutoff).unwrap();
    begin_worker_parent_task_at(&path, "worker-fast", cutoff).unwrap();

    let pending = pending_worker_parent_notifications_with_evidence_at(
        &path,
        &[parent_session("worker-fast", 1, "done", "running")],
        &sessions_root,
        |_| WorkerCompletionEvidence::quiescent(),
    )
    .unwrap();
    assert_eq!(
        pending.len(),
        1,
        "completion before register/activate must still notify exactly once: {pending:?}"
    );
    assert_eq!(pending[0].kind, WorkerParentNotificationKind::Completed);
    assert_eq!(pending[0].parent_chat_id, "parent-chat");
    assert_eq!(pending[0].worker_session_id, "worker-fast");

    let again = pending_worker_parent_notifications_with_evidence_at(
        &path,
        &[parent_session("worker-fast", 1, "done", "running")],
        &sessions_root,
        |_| WorkerCompletionEvidence::quiescent(),
    )
    .unwrap();
    assert_eq!(again.len(), 1, "unacked completion must not duplicate");
}

#[test]
fn private_native_prompt_files_keep_forced_mode_on_existing_paths()
-> Result<(), Box<dyn std::error::Error>> {
    let _lock = env_lock();
    let _home = IsolatedHome::new()?;
    let session_id = "omp-native-mode";
    let directory = unpeel_core::session_host::session_dir(session_id);
    fs::create_dir_all(&directory)?;
    fs::set_permissions(&directory, fs::Permissions::from_mode(0o755))?;
    let body = unpeel_core::omp_native_initial::native_initial_message_path(session_id);
    fs::write(&body, "stale")?;
    fs::set_permissions(&body, fs::Permissions::from_mode(0o644))?;

    unpeel_core::omp_native_initial::stage_native_initial_prompt("omp", session_id, "secret")?;
    assert_eq!(unix_mode(&directory), 0o700);
    assert_eq!(unix_mode(&body), 0o600);
    Ok(())
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
    let spawn = unpeel_core::omp_native_initial::prepare_native_initial_argv(
        "claude",
        "claude --permission-mode plan",
        "claude-1",
    )
    .expect("prepare");
    assert_eq!(spawn, "claude --permission-mode plan");
}
