use zeron_workers_unpeel::{WorkersProject, WorkersSession};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkersSessionMenuItem {
    Rename,
    Pin,
    Unpin,
    MoveTo,
    ClearAttention,
    ResumeAgent,
    Resume,
    Fork,
    AppendSystemContext,
    NotifyWhenDone,
    CopyTranscript20,
    CopyTranscript50,
    CopyTranscriptAll,
    StopAndArchive,
    Archive,
    Restore,
    RestoreAndResume,
    Remove,
}

pub fn session_menu_items(
    session: &WorkersSession,
    move_targets: &[WorkersProject],
) -> Vec<WorkersSessionMenuItem> {
    let live = session.is_live();
    let starting = session.activity == "starting" || session.runtime_launch_pending;
    let mut items = vec![
        WorkersSessionMenuItem::Rename,
        if session.pinned {
            WorkersSessionMenuItem::Unpin
        } else {
            WorkersSessionMenuItem::Pin
        },
    ];

    if move_targets
        .iter()
        .any(|project| project.id != session.project_id)
    {
        items.push(WorkersSessionMenuItem::MoveTo);
    }
    if session.activity == "blocked" {
        items.push(WorkersSessionMenuItem::ClearAttention);
    }
    if !starting && !session.archived {
        if live && session.capabilities.resume_agent {
            items.push(WorkersSessionMenuItem::ResumeAgent);
        } else if !live && session.capabilities.restart {
            items.push(WorkersSessionMenuItem::Resume);
        }
        if session.capabilities.fork {
            items.push(WorkersSessionMenuItem::Fork);
        }
        if session.capabilities.append_system_context {
            items.push(WorkersSessionMenuItem::AppendSystemContext);
        }
    }
    if session.capabilities.notify_when_done {
        items.push(WorkersSessionMenuItem::NotifyWhenDone);
    }

    items.extend([
        WorkersSessionMenuItem::CopyTranscript20,
        WorkersSessionMenuItem::CopyTranscript50,
        WorkersSessionMenuItem::CopyTranscriptAll,
    ]);

    if session.archived {
        items.push(if session.capabilities.restart {
            WorkersSessionMenuItem::RestoreAndResume
        } else {
            WorkersSessionMenuItem::Restore
        });
    } else if session.capabilities.archive {
        items.push(if live {
            WorkersSessionMenuItem::StopAndArchive
        } else {
            WorkersSessionMenuItem::Archive
        });
    }
    items.push(WorkersSessionMenuItem::Remove);
    items
}

#[cfg(test)]
mod tests {
    use super::{WorkersSessionMenuItem as Item, session_menu_items};
    use zeron_workers_unpeel::{WorkersProject, WorkersSession, WorkersSessionCapabilities};

    fn project(id: &str) -> WorkersProject {
        WorkersProject {
            id: id.into(),
            name: id.into(),
            path: format!("/tmp/{id}"),
            folder_id: None,
            parent_project_id: None,
            is_group: false,
            worktree_branch: None,
            git_branch: None,
            archived_session_count: 0,
        }
    }

    fn session() -> WorkersSession {
        WorkersSession {
            id: "session-1".into(),
            project_id: "project-1".into(),
            title: "Worker".into(),
            command: "codex".into(),
            state: "running".into(),
            activity: "working".into(),
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
            capabilities: WorkersSessionCapabilities {
                restart: true,
                resume_agent: true,
                fork: true,
                archive: true,
                append_system_context: true,
                notify_when_done: true,
            },
        }
    }

    #[test]
    fn live_worker_menu_matches_unpeel_capability_order() {
        assert_eq!(
            session_menu_items(&session(), &[project("project-1"), project("project-2")]),
            vec![
                Item::Rename,
                Item::Pin,
                Item::MoveTo,
                Item::ResumeAgent,
                Item::Fork,
                Item::AppendSystemContext,
                Item::NotifyWhenDone,
                Item::CopyTranscript20,
                Item::CopyTranscript50,
                Item::CopyTranscriptAll,
                Item::StopAndArchive,
                Item::Remove,
            ]
        );
    }

    #[test]
    fn archived_worker_has_one_restore_verb_and_no_standalone_resume() {
        let mut archived = session();
        archived.state = "exited".into();
        archived.activity = "idle".into();
        archived.archived = true;
        assert_eq!(
            session_menu_items(&archived, &[]),
            vec![
                Item::Rename,
                Item::Pin,
                Item::NotifyWhenDone,
                Item::CopyTranscript20,
                Item::CopyTranscript50,
                Item::CopyTranscriptAll,
                Item::RestoreAndResume,
                Item::Remove,
            ]
        );
    }
}
