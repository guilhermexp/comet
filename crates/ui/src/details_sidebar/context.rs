use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use zeron_proto::{Chat, Space};
use zeron_workers_unpeel::{WorkersProject, WorkersSession};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DetailsMode {
    Orchestrator,
    Workers,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DetailsTab {
    #[default]
    Details,
    Files,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DetailsContext {
    pub key: String,
    pub cwd: PathBuf,
    pub branch: Option<String>,
    pub chat_id: Option<String>,
    pub target_device_id: Option<String>,
    pub mode: DetailsMode,
}

pub fn context_for_orchestrator(
    chat: Option<&Chat>,
    space: Option<&Space>,
) -> Option<DetailsContext> {
    if let Some(chat) = chat {
        let cwd = chat
            .cwd
            .as_deref()
            .or_else(|| space.map(|space| space.path.as_str()))?;
        return Some(DetailsContext {
            key: format!("orchestrator-chat:{}", chat.id),
            cwd: PathBuf::from(cwd),
            branch: chat.branch.clone(),
            chat_id: Some(chat.id.clone()),
            target_device_id: Some(chat.device_id.clone()),
            mode: DetailsMode::Orchestrator,
        });
    }

    space.map(|space| DetailsContext {
        key: format!("orchestrator-space:{}", space.id),
        cwd: PathBuf::from(&space.path),
        branch: None,
        chat_id: None,
        target_device_id: Some(space.device_id.clone()),
        mode: DetailsMode::Orchestrator,
    })
}

pub fn context_for_worker(
    project: Option<&WorkersProject>,
    session: Option<&WorkersSession>,
) -> Option<DetailsContext> {
    let project = project?;
    let (key, branch) = if let Some(session) = session {
        (
            format!("workers-session:{}", session.id),
            session
                .worktree_branch
                .clone()
                .or_else(|| project.worktree_branch.clone())
                .or_else(|| project.git_branch.clone()),
        )
    } else {
        (
            format!("workers-project:{}", project.id),
            project
                .worktree_branch
                .clone()
                .or_else(|| project.git_branch.clone()),
        )
    };

    Some(DetailsContext {
        key,
        cwd: PathBuf::from(&project.path),
        branch,
        chat_id: None,
        target_device_id: None,
        mode: DetailsMode::Workers,
    })
}

pub fn detect_git_branch(root: &std::path::Path) -> Option<String> {
    let symbolic = std::process::Command::new("git")
        .args(["symbolic-ref", "--short", "HEAD"])
        .current_dir(root)
        .output()
        .ok();
    let output = match symbolic {
        Some(output) if output.status.success() => output,
        _ => std::process::Command::new("git")
            .args(["rev-parse", "--abbrev-ref", "HEAD"])
            .current_dir(root)
            .output()
            .ok()?,
    };
    if !output.status.success() {
        return None;
    }
    let branch = String::from_utf8(output.stdout).ok()?;
    let branch = branch.trim();
    (!branch.is_empty() && branch != "HEAD").then(|| branch.to_string())
}

#[cfg(test)]
mod tests {
    use chrono::Utc;
    use zeron_proto::{Chat, Space};
    use zeron_workers_unpeel::{
        WorkersProject, WorkersSession, WorkersSessionCapabilities, WorkersSessionSort,
    };

    use super::{DetailsMode, context_for_orchestrator, context_for_worker};

    fn space() -> Space {
        Space {
            id: "space-1".into(),
            device_id: "device-1".into(),
            path: "/tmp/project".into(),
            name: None,
            git_detected: true,
            git_checked_at: None,
            checkout_id: Some("checkout-1".into()),
            created_at: Utc::now(),
        }
    }

    fn chat() -> Chat {
        Chat {
            id: "chat-1".into(),
            device_id: "device-1".into(),
            title: None,
            archived: false,
            cwd: Some("/tmp/project/worktree".into()),
            branch: Some("feature/details".into()),
            checkout_id: Some("checkout-1".into()),
            config: None,
            last_message_preview: None,
            last_message_at: None,
            created_at: Utc::now(),
            harness_session_id: None,
            harness_session_cwd: None,
            space_id: Some("space-1".into()),
            last_seen_at: None,
            room_gen: None,
        }
    }

    fn project() -> WorkersProject {
        WorkersProject {
            id: "project-1".into(),
            name: "project".into(),
            path: "/tmp/workers-project".into(),
            folder_id: None,
            parent_project_id: None,
            is_group: false,
            worktree_branch: None,
            git_branch: Some("main".into()),
            archived_session_count: 0,
            folder_color_id: None,
            session_sort: WorkersSessionSort::Custom,
        }
    }

    fn session() -> WorkersSession {
        WorkersSession {
            id: "session-1".into(),
            project_id: "project-1".into(),
            title: "worker".into(),
            command: "codex".into(),
            state: "running".into(),
            activity: "idle".into(),
            unread: false,
            pinned: false,
            archived: false,
            provider_id: Some("codex".into()),
            active_runtime_id: None,
            runtime_launch_pending: false,
            runtime_generation: 1,
            notify_when_done: false,
            terminal_background_hex: None,
            worktree_branch: Some("worker/fix".into()),
            created_at_unix_ms: 1,
            updated_at_unix_ms: 1,
            capabilities: WorkersSessionCapabilities::default(),
        }
    }

    #[test]
    fn selected_chat_is_the_orchestrator_context() {
        let context = context_for_orchestrator(Some(&chat()), Some(&space())).unwrap();
        assert_eq!(context.key, "orchestrator-chat:chat-1");
        assert_eq!(context.cwd.to_string_lossy(), "/tmp/project/worktree");
        assert_eq!(context.branch.as_deref(), Some("feature/details"));
        assert_eq!(context.chat_id.as_deref(), Some("chat-1"));
        assert_eq!(context.mode, DetailsMode::Orchestrator);
    }

    #[test]
    fn new_chat_uses_the_selected_project() {
        let context = context_for_orchestrator(None, Some(&space())).unwrap();
        assert_eq!(context.key, "orchestrator-space:space-1");
        assert_eq!(context.cwd.to_string_lossy(), "/tmp/project");
        assert_eq!(context.chat_id, None);
    }

    #[test]
    fn worker_session_scopes_state_without_changing_the_project_root() {
        let context = context_for_worker(Some(&project()), Some(&session())).unwrap();
        assert_eq!(context.key, "workers-session:session-1");
        assert_eq!(context.cwd.to_string_lossy(), "/tmp/workers-project");
        assert_eq!(context.branch.as_deref(), Some("worker/fix"));
        assert_eq!(context.mode, DetailsMode::Workers);
    }

    #[test]
    fn worker_project_without_session_still_has_details() {
        let context = context_for_worker(Some(&project()), None).unwrap();
        assert_eq!(context.key, "workers-project:project-1");
        assert_eq!(context.branch.as_deref(), Some("main"));
    }

    #[test]
    fn missing_selection_has_no_details_context() {
        assert!(context_for_orchestrator(None, None).is_none());
        assert!(context_for_worker(None, None).is_none());
    }

    #[test]
    fn git_branch_is_detected_for_new_chat_projects() {
        let root = tempfile::tempdir().unwrap();
        std::process::Command::new("git")
            .args(["init", "-q", "-b", "details-test"])
            .current_dir(root.path())
            .status()
            .unwrap();
        assert_eq!(
            super::detect_git_branch(root.path()).as_deref(),
            Some("details-test")
        );
    }
}
