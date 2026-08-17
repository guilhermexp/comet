use std::ffi::OsString;
use std::fs;
use std::sync::Mutex;

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
        notify_when_done: false,
        terminal_background_hex: None,
        worktree_branch: None,
        created_at_unix_ms: 1,
        updated_at_unix_ms: 2,
        capabilities: WorkersSessionCapabilities::default(),
    }
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
        WorkersSessionCommand::Restore,
        WorkersSessionCommand::RestoreAndResume,
        WorkersSessionCommand::Remove,
    ];
    assert_eq!(commands.len(), 12);
}
