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

const LITERAL_TASK: &str = "--flag && true; echo $HOME; touch shell-injected\nreview \"quotes\"\n@not-a-file\nunicodé ✓\n\n";
const SHELL_INJECTION_FILE: &str = "shell-injected";

struct NativeRuntime {
    id: &'static str,
    bin: &'static str,
    command: &'static str,
    flag: &'static str,
    file_argument: bool,
}

const NATIVE_RUNTIMES: &[NativeRuntime] = &[
    NativeRuntime {
        id: "omp",
        bin: "omp",
        command: "omp --yolo",
        flag: "--yolo",
        file_argument: true,
    },
    NativeRuntime {
        id: "pi",
        bin: "pi",
        command: "pi --yolo",
        flag: "--yolo",
        file_argument: true,
    },
    NativeRuntime {
        id: "claude",
        bin: "claude",
        command: "claude --permission-mode plan",
        flag: "--permission-mode",
        file_argument: false,
    },
    NativeRuntime {
        id: "codex",
        bin: "codex",
        command: "codex --full-auto",
        flag: "--full-auto",
        file_argument: false,
    },
];

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

fn write_fake_cli(
    bin_dir: &Path,
    probe: &Path,
    bin: &str,
) -> Result<PathBuf, Box<dyn std::error::Error>> {
    fs::create_dir_all(bin_dir)?;
    let path = bin_dir.join(bin);
    fs::write(
        &path,
        format!(
            r#"#!/bin/sh
PROBE='{}'
native=0
after_terminator=0
: > "$PROBE/argv"
: > "$PROBE/flags"
while [ "$#" -gt 0 ]; do
  printf '%s\n' "$1" >> "$PROBE/argv"
  if [ "$after_terminator" -eq 1 ]; then
    printf '%s' "$1" > "$PROBE/task"
    native=$((native + 1))
    break
  fi
  case "$1" in
    --)
      after_terminator=1
      ;;
    @*)
      cat "${{1#@}}" > "$PROBE/task"
      native=$((native + 1))
      ;;
    --yolo|--full-auto|--continue|--no-alt-screen)
      printf '%s\n' "$1" >> "$PROBE/flags"
      ;;
    --permission-mode|--model|--extension|--mcp-config|-c|--sandbox|--cd)
      printf '%s\n' "$1" >> "$PROBE/flags"
      shift
      if [ "$#" -gt 0 ]; then
        printf '%s\n' "$1" >> "$PROBE/argv"
        printf '%s\n' "$1" >> "$PROBE/flags"
      fi
      ;;
    --flag)
      printf 'consumed\n' > "$PROBE/leading_option_as_cli_flag"
      ;;
    -*)
      printf '%s\n' "$1" > "$PROBE/unknown_option"
      ;;
    *)
      printf '%s' "$1" > "$PROBE/task"
      native=$((native + 1))
      break
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
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755))?;
    Ok(path)
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
fn configured_runtimes_receive_the_literal_task_through_native_startup()
-> Result<(), Box<dyn std::error::Error>> {
    let _lock = env_lock();
    let home = IsolatedHome::new()?;
    let bin = home.path().join("bin");
    let warning_screen = "Connecting to MCP servers: graft…\nMCP error: graft failed\n❯";
    assert!(controller_mcp_is_booting_screen(warning_screen));

    for runtime in NATIVE_RUNTIMES {
        let probe = home.path().join(format!("probe-{}", runtime.id));
        fs::create_dir_all(&probe)?;
        write_fake_cli(&bin, &probe, runtime.bin)?;
        let session_id = format!("{}-native-1", runtime.id);
        let _ = unpeel_core::native_initial::stage_native_initial_prompt(
            runtime.id,
            &session_id,
            LITERAL_TASK,
        );

        let startup = unpeel_core::integrations::startup_command(
            runtime.id,
            runtime.command,
            false,
            false,
            false,
        );
        assert!(
            !startup.contains("--auto-approve") && !startup.contains("--dangerously-bypass"),
            "native delivery must not add approval bypass: {startup}"
        );
        let spawn = unpeel_core::native_initial::prepare_native_initial_argv(
            runtime.id,
            runtime.command,
            &session_id,
        )?;
        if runtime.file_argument {
            assert!(
                !spawn.contains(LITERAL_TASK),
                "file-argument delivery must keep the task out of command metadata: {spawn}"
            );
        }

        let output =
            unpeel_core::native_initial::submit_with_native_reservation(&session_id, || {
                run_spawn_command(&spawn, &bin, home.path())
            })?;
        assert!(
            output.status.success(),
            "fake {} failed: {}",
            runtime.id,
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(
            native_executions(&probe),
            1,
            "{} must receive the initial task through native startup exactly once; viewport waiting must not be required. stdout:\n{}",
            runtime.id,
            String::from_utf8_lossy(&output.stdout)
        );
        assert_eq!(
            fs::read_to_string(probe.join("task"))?,
            LITERAL_TASK,
            "{} must receive the literal task, including leading options and trailing newlines",
            runtime.id
        );
        let flags = fs::read_to_string(probe.join("flags")).unwrap_or_default();
        assert!(
            flags.lines().any(|line| line == runtime.flag),
            "{} must keep the original preset flag {}",
            runtime.id,
            runtime.flag
        );
        assert!(
            !probe.join("leading_option_as_cli_flag").exists(),
            "{} must not consume a leading task option as a CLI flag",
            runtime.id
        );
        assert!(
            !home.path().join(SHELL_INJECTION_FILE).exists(),
            "{} must not let the shell evaluate task metacharacters",
            runtime.id
        );
        assert!(
            unpeel_core::native_initial::native_initial_prompt_attached(&session_id),
            "successful Host submit must persist the native receipt for {}",
            runtime.id
        );
    }
    Ok(())
}

#[test]
fn restart_does_not_replay_the_native_initial_task() -> Result<(), Box<dyn std::error::Error>> {
    let _lock = env_lock();
    let home = IsolatedHome::new()?;
    let bin = home.path().join("bin");

    for runtime in NATIVE_RUNTIMES {
        let probe = home.path().join(format!("probe-restart-{}", runtime.id));
        fs::create_dir_all(&probe)?;
        write_fake_cli(&bin, &probe, runtime.bin)?;
        let session_id = format!("{}-native-restart", runtime.id);
        let _ = unpeel_core::native_initial::stage_native_initial_prompt(
            runtime.id,
            &session_id,
            LITERAL_TASK,
        );

        let first = unpeel_core::native_initial::prepare_native_initial_argv(
            runtime.id,
            runtime.command,
            &session_id,
        )?;
        unpeel_core::native_initial::submit_with_native_reservation(&session_id, || {
            run_spawn_command(&first, &bin, home.path())
        })?;
        assert_eq!(
            native_executions(&probe),
            1,
            "{} restart must not resubmit the native task",
            runtime.id
        );

        fs::write(probe.join("native_executions"), "0\n")?;
        let _ = fs::remove_file(probe.join("task"));
        let resume = format!("{} --continue", runtime.command);
        let second = unpeel_core::native_initial::prepare_native_initial_argv(
            runtime.id,
            &resume,
            &session_id,
        )?;
        assert!(
            !second.contains(LITERAL_TASK) && !second.contains('@'),
            "resume command must not carry the initial task"
        );
        run_spawn_command(&second, &bin, home.path())?;
        assert_eq!(native_executions(&probe), 0);
        assert!(!probe.join("task").exists());
    }
    Ok(())
}

#[test]
fn missing_native_delivery_is_not_reported_as_submitted() {
    let _lock = env_lock();
    let _home = IsolatedHome::new().expect("isolated home");
    let session_id = "omp-native-missing";
    let spawn = unpeel_core::native_initial::prepare_native_initial_argv("omp", "omp", session_id)
        .expect("prepare");
    assert_eq!(spawn, "omp");
    assert!(!unpeel_core::native_initial::native_initial_prompt_attached(session_id));
}

#[test]
fn failed_spawn_does_not_report_briefing_submitted() -> Result<(), Box<dyn std::error::Error>> {
    let _lock = env_lock();
    let home = IsolatedHome::new()?;

    for runtime in NATIVE_RUNTIMES {
        let session_id = format!("{}-native-spawn-fail", runtime.id);
        let _ = unpeel_core::native_initial::stage_native_initial_prompt(
            runtime.id,
            &session_id,
            LITERAL_TASK,
        );
        let argv = unpeel_core::native_initial::prepare_native_initial_argv(
            runtime.id,
            runtime.command,
            &session_id,
        )?;
        let spawned =
            unpeel_core::native_initial::submit_with_native_reservation(&session_id, || {
                Command::new("/bin/sh")
                    .arg("-c")
                    .arg(&argv)
                    .current_dir(home.path().join("missing-cwd"))
                    .spawn()
                    .map(|_| ())
                    .map_err(|error| error.to_string())
            });
        assert!(
            spawned.is_err(),
            "prepared command must fail at the Host submission seam: {spawned:?}"
        );
        assert!(
            !unpeel_core::native_initial::native_initial_prompt_attached(&session_id),
            "failed spawn must not report briefing_submitted for {}",
            runtime.id
        );
        assert!(
            unpeel_core::native_initial::native_initial_prompt_pending(&session_id),
            "pending must survive a failed spawn so recovery can retry without replay-after-ACK for {}",
            runtime.id
        );
    }
    Ok(())
}

#[test]
fn missing_body_does_not_ack_a_surviving_pending() -> Result<(), Box<dyn std::error::Error>> {
    let _lock = env_lock();
    let _home = IsolatedHome::new()?;
    for runtime in NATIVE_RUNTIMES {
        let session_id = format!("{}-native-missing-body", runtime.id);
        unpeel_core::native_initial::stage_native_initial_prompt(
            runtime.id,
            &session_id,
            LITERAL_TASK,
        )?;
        fs::remove_file(unpeel_core::native_initial::native_initial_message_path(
            &session_id,
        ))?;
        let spawn = unpeel_core::native_initial::prepare_native_initial_argv(
            runtime.id,
            runtime.command,
            &session_id,
        )?;
        assert_eq!(spawn, runtime.command);
        unpeel_core::native_initial::submit_with_native_reservation(&session_id, || Ok(()))?;
        assert!(
            !unpeel_core::native_initial::native_initial_prompt_attached(&session_id),
            "body ausente must not turn pending into attached for {}",
            runtime.id
        );
    }
    Ok(())
}

#[test]
fn ack_failure_after_submit_keeps_result_and_does_not_replay()
-> Result<(), Box<dyn std::error::Error>> {
    let _lock = env_lock();
    let _home = IsolatedHome::new()?;
    for runtime in NATIVE_RUNTIMES {
        let session_id = format!("{}-native-ack-fail", runtime.id);
        unpeel_core::native_initial::stage_native_initial_prompt(
            runtime.id,
            &session_id,
            LITERAL_TASK,
        )?;
        let first = unpeel_core::native_initial::prepare_native_initial_argv(
            runtime.id,
            runtime.command,
            &session_id,
        )?;
        let attached = unpeel_core::session_host::session_dir(&session_id)
            .join(unpeel_core::native_initial::NATIVE_INITIAL_ATTACHED_FILE);
        fs::create_dir(&attached)?;
        let result =
            unpeel_core::native_initial::submit_with_native_reservation(&session_id, || {
                Ok("child")
            });
        assert_eq!(
            result.as_deref(),
            Ok("child"),
            "ACK failure after successful submit must keep the Host result for {}: {result:?}",
            runtime.id
        );
        assert!(
            !unpeel_core::native_initial::native_initial_prompt_pending(&session_id),
            "successful submit must consume the reservation even when the receipt cannot be persisted for {}",
            runtime.id
        );
        let resume = format!("{} --continue", runtime.command);
        let second = unpeel_core::native_initial::prepare_native_initial_argv(
            runtime.id,
            &resume,
            &session_id,
        )?;
        assert!(
            !second.contains(LITERAL_TASK) && !second.contains('@') && first != second,
            "ACK failure must not leave a replayable pending file for {}: {second}",
            runtime.id
        );
    }
    Ok(())
}

#[test]
fn paste_only_and_raw_keep_host_pty_contracts() {
    for runtime in NATIVE_RUNTIMES {
        assert!(
            !native_initial_startup_enabled(runtime.id, HostCreateSubmitMode::PasteOnly),
            "PasteOnly must keep Host PTY contracts; native startup is PasteAndSubmit only"
        );
        assert!(
            !native_initial_startup_enabled(runtime.id, HostCreateSubmitMode::Raw),
            "Raw must keep Host PTY contracts; native startup is PasteAndSubmit only"
        );
        assert!(
            native_initial_startup_enabled(runtime.id, HostCreateSubmitMode::PasteAndSubmit),
            "{} PasteAndSubmit must use native startup",
            runtime.id
        );
    }
    assert!(!native_initial_startup_enabled(
        "prime-agent",
        HostCreateSubmitMode::PasteAndSubmit
    ));
}

#[test]
fn mcp_native_decision_matches_host_command_resolution() {
    let presets = [
        catalog_preset("omp", "omp --yolo", true, None, Some("omp")),
        catalog_preset("omp", "prime-agent", true, Some("proj"), Some("omp")),
        catalog_preset("stale", "prime-agent", true, None, Some("omp")),
        catalog_preset(
            "claude",
            "claude --permission-mode plan",
            true,
            None,
            Some("stale"),
        ),
        catalog_preset("pi", "pi --yolo", true, None, Some("pi")),
        catalog_preset("codex", "codex --full-auto", true, None, Some("codex")),
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
    assert!(controller_mcp_native_initial_from_presets(
        "other", "claude", &presets
    ));
    assert!(controller_mcp_native_initial_from_presets(
        "other", "pi", &presets
    ));
    assert!(controller_mcp_native_initial_from_presets(
        "other", "codex", &presets
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
    let body = unpeel_core::native_initial::native_initial_message_path(session_id);
    fs::write(&body, "stale")?;
    fs::set_permissions(&body, fs::Permissions::from_mode(0o644))?;

    unpeel_core::native_initial::stage_native_initial_prompt("omp", session_id, "secret")?;
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
    let spawn = unpeel_core::native_initial::prepare_native_initial_argv(
        "prime-agent",
        "prime-agent --model x",
        "prime-1",
    )
    .expect("prepare");
    assert_eq!(spawn, "prime-agent --model x");
}

#[test]
fn codex_wrapper_install_updates_an_existing_managed_asset_without_tracing_the_prompt()
-> Result<(), Box<dyn std::error::Error>> {
    let _lock = env_lock();
    let home = IsolatedHome::new()?;
    let wrapper_dir = unpeel_core::hook_assets::wrapper_bin_dir();
    fs::create_dir_all(&wrapper_dir)?;
    let wrapper_path = wrapper_dir.join("codex");
    fs::write(
        &wrapper_path,
        r#"#!/bin/bash
TRACE_FILE="${UNPEEL_HOOK_TRACE_FILE:-$HOME/.zeron/workers/hooks/trace.log}"
mkdir -p "$(dirname "$TRACE_FILE")" >/dev/null 2>&1 || true
printf '%s argv=%s\n' "$(date '+%Y-%m-%d %H:%M:%S')" "$*" >> "$TRACE_FILE" 2>/dev/null || true
REAL_BIN="${UNPEEL_REAL_CODEX_BIN:-}"
exec "$REAL_BIN" "$@"
"#,
    )?;
    fs::set_permissions(&wrapper_path, fs::Permissions::from_mode(0o755))?;

    unpeel_core::hook_assets::install_codex_wrapper()?;

    let real_bin = home.path().join("bin-wrapper").join("codex-real");
    fs::create_dir_all(real_bin.parent().expect("parent"))?;
    fs::write(
        &real_bin,
        r#"#!/bin/sh
printf '%s\n' 'wrapper-child-executed'
"#,
    )?;
    fs::set_permissions(&real_bin, fs::Permissions::from_mode(0o755))?;
    let trace = home.path().join("wrapper.trace");
    let secret = "positional-secret-must-not-reach-trace";
    let output = Command::new(&wrapper_path)
        .arg("--full-auto")
        .arg("--")
        .arg(secret)
        .env("UNPEEL_REAL_CODEX_BIN", &real_bin)
        .env("UNPEEL_HOOK_TRACE_FILE", &trace)
        .env("UNPEEL_SESSION_ID", "codex-wrapper-privacy")
        .output()?;
    assert!(
        output.status.success(),
        "installed wrapper failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(output.stdout, b"wrapper-child-executed\n");
    let trace_text = fs::read_to_string(&trace).unwrap_or_default();
    assert!(
        !trace_text.contains(secret),
        "Codex wrapper must not persist the positional prompt in diagnostic traces"
    );
    Ok(())
}
