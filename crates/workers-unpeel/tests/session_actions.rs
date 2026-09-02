use std::ffi::OsString;
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixListener;
use std::sync::Arc;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use tempfile::TempDir;
use zeron_workers_unpeel::{
    LocalWorkersClient, WorkersError, WorkersSession, WorkersSessionCapabilities,
    WorkersSessionCommand, WorkersTranscriptRange,
};

static ENV_LOCK: Mutex<()> = Mutex::new(());

struct UnpeelHomeGuard(Option<OsString>);

impl UnpeelHomeGuard {
    fn set(path: &std::path::Path) -> Self {
        let previous = std::env::var_os("UNPEEL_HOME");
        // SAFETY: every environment mutation in this test binary holds ENV_LOCK.
        unsafe { std::env::set_var("UNPEEL_HOME", path) };
        Self(previous)
    }
}

impl Drop for UnpeelHomeGuard {
    fn drop(&mut self) {
        // SAFETY: the caller still holds ENV_LOCK when this guard is dropped.
        unsafe {
            match self.0.take() {
                Some(previous) => std::env::set_var("UNPEEL_HOME", previous),
                None => std::env::remove_var("UNPEEL_HOME"),
            }
        }
    }
}

fn isolated_home() -> Result<(TempDir, UnpeelHomeGuard), Box<dyn std::error::Error>> {
    let home = TempDir::new()?;
    fs::write(
        home.path().join("app-state.json"),
        serde_json::to_vec(&serde_json::json!({
            "projects": [],
            "presets": [],
            "active_tabs": {},
            "pinned_sessions": {}
        }))?,
    )?;
    let guard = UnpeelHomeGuard::set(home.path());
    Ok((home, guard))
}

fn session() -> WorkersSession {
    WorkersSession {
        id: "session-1".into(),
        project_id: "project-1".into(),
        title: "Worker".into(),
        command: "codex".into(),
        state: "exited".into(),
        activity: "idle".into(),
        unread: false,
        pinned: false,
        archived: false,
        provider_id: Some("com.openai.codex".into()),
        active_runtime_id: Some("com.openai.codex".into()),
        runtime_launch_pending: false,
        runtime_generation: 1,
        notify_when_done: false,
        terminal_background_hex: None,
        worktree_branch: None,
        created_at_unix_ms: 1,
        updated_at_unix_ms: 2,
        idle_since_unix_ms: None,
        idle_confirmed_by_hook: false,
        resumable_conversation: false,
        hibernation_activity_token: None,
        total_tokens: None,
        model_usage: Vec::new(),
        capabilities: WorkersSessionCapabilities::default(),
    }
}

#[test]
fn activity_after_confirmation_prevents_hibernation_stop_and_archive()
-> Result<(), Box<dyn std::error::Error>> {
    let _lock = ENV_LOCK.lock().expect("UNPEEL_HOME test lock");
    let (home, _guard) = isolated_home()?;
    write_manifest("session-1", "codex", true);
    let manifest_path = unpeel_core::session_host::manifest_path("session-1");
    let mut manifest: serde_json::Value = serde_json::from_slice(&fs::read(&manifest_path)?)?;
    manifest["state"] = serde_json::json!("running");
    manifest["runtime_launch_generation"] = serde_json::json!(1);
    manifest["runtime_launched_at"] = serde_json::json!(1);
    manifest["screen_changed_at"] = serde_json::json!(1);
    fs::write(&manifest_path, serde_json::to_vec(&manifest)?)?;

    let client = LocalWorkersClient::new();
    let expected = client
        .capture_hibernation_activity_token("session-1")
        .expect("running Worker activity token");
    let commands = Arc::new(Mutex::new(Vec::new()));
    let observed = Arc::clone(&commands);
    let socket = unpeel_core::session_host::socket_path("session-1");
    let listener = UnixListener::bind(&socket)?;
    listener.set_nonblocking(true)?;
    let server = std::thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(2);
        while Instant::now() < deadline {
            match listener.accept() {
                Ok((mut stream, _)) => {
                    let mut line = String::new();
                    BufReader::new(stream.try_clone().expect("clone fake host stream"))
                        .read_line(&mut line)
                        .expect("read fake host command");
                    let command: serde_json::Value =
                        serde_json::from_str(line.trim()).expect("host command json");
                    observed
                        .lock()
                        .expect("command capture")
                        .push(command["type"].as_str().unwrap_or_default().to_owned());
                    stream
                        .write_all(b"{\"ok\":true,\"error\":null}\n")
                        .expect("answer fake host command");
                }
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    std::thread::sleep(Duration::from_millis(10));
                }
                Err(error) => panic!("fake host accept failed: {error}"),
            }
        }
    });

    client.write("session-1", "new work\r")?;
    let mut worker = session();
    worker.state = "running".into();
    let error = client
        .session_command(
            &worker,
            WorkersSessionCommand::Hibernate {
                expected_activity_token: expected,
            },
        )
        .expect_err("new activity must invalidate automatic hibernation");
    assert!(matches!(error, WorkersError::State(message) if message.contains("activity changed")));
    assert!(
        !home
            .path()
            .join("app-sessions/session-1/archived.json")
            .exists()
    );

    server.join().expect("fake host thread");
    assert_eq!(
        commands.lock().expect("command capture").as_slice(),
        ["write"]
    );
    Ok(())
}

#[test]
fn transcript_ranges_keep_unpeels_exact_entry_values() {
    assert_eq!(WorkersTranscriptRange::Last20.entries(), 20);
    assert_eq!(WorkersTranscriptRange::Last50.entries(), 50);
    assert_eq!(WorkersTranscriptRange::WholeConversation.entries(), 0);
}

#[test]
fn blank_system_context_is_rejected_before_session_lookup() -> Result<(), Box<dyn std::error::Error>>
{
    let _lock = ENV_LOCK.lock().expect("UNPEEL_HOME test lock");
    let (_home, _guard) = isolated_home()?;
    let error = LocalWorkersClient::new()
        .session_command(
            &session(),
            WorkersSessionCommand::AppendSystemContext { text: "  ".into() },
        )
        .expect_err("blank context must be rejected");
    assert!(matches!(error, WorkersError::State(message) if message.contains("must not be blank")));
    Ok(())
}

fn write_manifest(session_id: &str, command: &str, has_been_written_to: bool) {
    let path = unpeel_core::session_host::manifest_path(session_id);
    fs::create_dir_all(path.parent().expect("session dir")).expect("session dir");
    fs::write(
        &path,
        serde_json::to_vec(&serde_json::json!({
            "session": {
                "id": session_id,
                "project_id": "project-1",
                "label": "Worker",
                "command": command,
            },
            "cwd": "/tmp",
            "state": "exited",
            "pid": null,
            "exit_code": 0,
            "has_been_written_to": has_been_written_to,
        }))
        .expect("manifest json"),
    )
    .expect("write manifest");
}

#[test]
fn pi_family_workers_expose_resume_and_restart_clean_before_first_input() {
    let _lock = ENV_LOCK.lock().expect("UNPEEL_HOME test lock");
    let (home, _guard) = isolated_home().expect("isolated home");

    for command in ["omp --yolo", "prime-agent --model x"] {
        assert!(
            unpeel_core::resume::can_resume(command),
            "restart must be offered for {command}"
        );
        assert!(
            unpeel_core::resume::can_resume_agent(command, None),
            "resume agent must be offered for {command}"
        );
    }

    let pinned = home.path().join("pi-sessions").join("omp-1");
    let command = format!("omp --session-dir '{}'", pinned.display());
    write_manifest("omp-1", &command, false);
    assert_eq!(
        unpeel_core::session_ops::relaunch_command(
            "omp-1",
            unpeel_core::session_ops::RelaunchMode::Restart { force_fresh: false }
        )
        .expect("relaunch command"),
        command,
        "a Worker that never received input must relaunch clean"
    );

    write_manifest("omp-2", &command, true);
    assert_eq!(
        unpeel_core::session_ops::relaunch_command(
            "omp-2",
            unpeel_core::session_ops::RelaunchMode::Restart { force_fresh: false }
        )
        .expect("relaunch command"),
        format!("{command} --continue"),
        "a written Worker must resume its pinned session directory"
    );
}

#[test]
fn session_command_contract_contains_every_local_unpeel_verb() {
    let commands = [
        WorkersSessionCommand::Stop,
        WorkersSessionCommand::RestartSession,
        WorkersSessionCommand::RestartAgent,
        WorkersSessionCommand::ResumeAgent,
        WorkersSessionCommand::Fork,
        WorkersSessionCommand::ClearAttention,
        WorkersSessionCommand::AppendSystemContext {
            text: "context".into(),
        },
        WorkersSessionCommand::SetNotifyWhenDone { enabled: true },
        WorkersSessionCommand::Archive,
        WorkersSessionCommand::Hibernate {
            expected_activity_token: "activity-token".into(),
        },
        WorkersSessionCommand::Restore,
        WorkersSessionCommand::RestoreAndResume,
        WorkersSessionCommand::Remove,
    ];
    assert_eq!(commands.len(), 13);
}
