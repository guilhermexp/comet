use zeron_workers_unpeel::{SessionAction, WorkersSession};

/// Keep the archive deterministic: pinned sessions first, then most recently
/// touched. The upstream endpoint remains the source of truth for membership.
pub fn archived_sessions_for_project(mut sessions: Vec<WorkersSession>) -> Vec<WorkersSession> {
    sessions.sort_by(|left, right| {
        right
            .pinned
            .cmp(&left.pinned)
            .then_with(|| right.updated_at_unix_ms.cmp(&left.updated_at_unix_ms))
            .then_with(|| left.id.cmp(&right.id))
    });
    sessions
}

pub fn restore_action(session: &WorkersSession) -> SessionAction {
    if session.capabilities.resume_agent {
        SessionAction::ResumeAgent
    } else {
        SessionAction::Restart
    }
}

#[cfg(test)]
mod tests {
    use zeron_workers_unpeel::{SessionAction, WorkersSession, WorkersSessionCapabilities};

    use super::{archived_sessions_for_project, restore_action};

    fn archived(id: &str, pinned: bool, updated_at_unix_ms: u64) -> WorkersSession {
        WorkersSession {
            id: id.to_owned(),
            project_id: "project".to_owned(),
            title: id.to_owned(),
            command: "zsh".to_owned(),
            state: "exited".to_owned(),
            activity: "idle".to_owned(),
            unread: false,
            pinned,
            archived: true,
            provider_id: None,
            active_runtime_id: None,
            runtime_launch_pending: false,
            notify_when_done: false,
            terminal_background_hex: None,
            worktree_branch: None,
            created_at_unix_ms: 1,
            updated_at_unix_ms,
            capabilities: WorkersSessionCapabilities::default(),
        }
    }

    #[test]
    fn archive_groups_pinned_then_recent_sessions() {
        let sessions = archived_sessions_for_project(vec![
            archived("old", false, 1),
            archived("pinned", true, 2),
            archived("recent", false, 3),
        ]);
        let ids = sessions
            .iter()
            .map(|session| session.id.as_str())
            .collect::<Vec<_>>();
        assert_eq!(ids, ["pinned", "recent", "old"]);
    }

    #[test]
    fn restore_prefers_agent_resume_when_upstream_supports_it() {
        let mut session = archived("worker", false, 1);
        assert_eq!(restore_action(&session), SessionAction::Restart);
        session.capabilities.resume_agent = true;
        assert_eq!(restore_action(&session), SessionAction::ResumeAgent);
    }
}
