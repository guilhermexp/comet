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

fn rooted_context_key(kind: &str, id: &str, device_id: Option<&str>, root: &str) -> String {
    format!(
        "{kind}:{id}:device:{}:root:{root}",
        device_id.unwrap_or("local")
    )
}

pub fn worker_context_key(
    project: &WorkersProject,
    session: Option<&WorkersSession>,
) -> String {
    let (kind, id) = session
        .map(|session| ("workers-session", session.id.as_str()))
        .unwrap_or(("workers-project", project.id.as_str()));
    rooted_context_key(kind, id, None, &project.path)
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
            key: rooted_context_key(
                "orchestrator-chat",
                &chat.id,
                Some(&chat.device_id),
                cwd,
            ),
            cwd: PathBuf::from(cwd),
            branch: chat.branch.clone(),
            chat_id: Some(chat.id.clone()),
            target_device_id: Some(chat.device_id.clone()),
            mode: DetailsMode::Orchestrator,
        });
    }

    space.map(|space| DetailsContext {
        key: rooted_context_key(
            "orchestrator-space",
            &space.id,
            Some(&space.device_id),
            &space.path,
        ),
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
    let branch = if let Some(session) = session {
        session
            .worktree_branch
            .clone()
            .or_else(|| project.worktree_branch.clone())
            .or_else(|| project.git_branch.clone())
    } else {
        project
            .worktree_branch
            .clone()
            .or_else(|| project.git_branch.clone())
    };

    Some(DetailsContext {
        key: worker_context_key(project, session),
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
        assert_eq!(
            context.key,
            "orchestrator-chat:chat-1:device:device-1:root:/tmp/project/worktree"
        );
        assert_eq!(context.cwd.to_string_lossy(), "/tmp/project/worktree");
        assert_eq!(context.branch.as_deref(), Some("feature/details"));
        assert_eq!(context.chat_id.as_deref(), Some("chat-1"));
        assert_eq!(context.mode, DetailsMode::Orchestrator);
    }

    #[test]
    fn new_chat_uses_the_selected_project() {
        let context = context_for_orchestrator(None, Some(&space())).unwrap();
        assert_eq!(
            context.key,
            "orchestrator-space:space-1:device:device-1:root:/tmp/project"
        );
        assert_eq!(context.cwd.to_string_lossy(), "/tmp/project");
        assert_eq!(context.chat_id, None);
    }

    #[test]
    fn worker_session_scopes_state_without_changing_the_project_root() {
        let context = context_for_worker(Some(&project()), Some(&session())).unwrap();
        assert_eq!(
            context.key,
            "workers-session:session-1:device:local:root:/tmp/workers-project"
        );
        assert_eq!(context.cwd.to_string_lossy(), "/tmp/workers-project");
        assert_eq!(context.branch.as_deref(), Some("worker/fix"));
        assert_eq!(context.mode, DetailsMode::Workers);
    }

    #[test]
    fn worker_project_without_session_still_has_details() {
        let context = context_for_worker(Some(&project()), None).unwrap();
        assert_eq!(
            context.key,
            "workers-project:project-1:device:local:root:/tmp/workers-project"
        );
        assert_eq!(context.branch.as_deref(), Some("main"));
    }

    #[test]
    fn moving_a_worker_session_to_another_root_changes_its_context_identity() {
        let first = context_for_worker(Some(&project()), Some(&session())).unwrap();
        let mut moved = project();
        moved.path = "/tmp/another-checkout".into();
        let second = context_for_worker(Some(&moved), Some(&session())).unwrap();

        assert_ne!(first.key, second.key);
        assert_ne!(first.cwd, second.cwd);
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
