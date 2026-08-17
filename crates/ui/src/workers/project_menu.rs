use zeron_workers_unpeel::{WorkersProject, WorkersSession, WorkersSessionSort};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkersProjectMenuItem {
    Rename,
    NewSession,
    FolderColor,
    SortCustom,
    SortRecentlyUpdated,
    NewWorktree,
    NewGroup,
    StopAll,
    Archived,
    RevealInFinder,
    OpenInEditor,
    RemoveWorktree,
    RemoveGroup,
    RemoveProject,
}

pub fn project_menu_items(
    project: &WorkersProject,
    sessions: &[WorkersSession],
) -> Vec<WorkersProjectMenuItem> {
    let is_child = project.parent_project_id.is_some();
    let is_worktree = project.worktree_branch.is_some();
    let mut items = Vec::new();
    if is_child {
        items.push(WorkersProjectMenuItem::Rename);
    }
    items.push(WorkersProjectMenuItem::NewSession);
    if !is_child {
        items.push(WorkersProjectMenuItem::FolderColor);
    }
    items.push(match project.session_sort {
        WorkersSessionSort::Custom => WorkersProjectMenuItem::SortRecentlyUpdated,
        WorkersSessionSort::RecentlyUpdated => WorkersProjectMenuItem::SortCustom,
    });
    if !project.is_group && !is_worktree {
        items.push(WorkersProjectMenuItem::NewWorktree);
    }
    if !is_child {
        items.push(WorkersProjectMenuItem::NewGroup);
    }
    if sessions.iter().any(WorkersSession::is_live) {
        items.push(WorkersProjectMenuItem::StopAll);
    }
    if project.archived_session_count > 0 {
        items.push(WorkersProjectMenuItem::Archived);
    }
    items.extend([
        WorkersProjectMenuItem::RevealInFinder,
        WorkersProjectMenuItem::OpenInEditor,
        if is_worktree {
            WorkersProjectMenuItem::RemoveWorktree
        } else if is_child {
            WorkersProjectMenuItem::RemoveGroup
        } else {
            WorkersProjectMenuItem::RemoveProject
        },
    ]);
    items
}

#[cfg(test)]
mod tests {
    use super::{WorkersProjectMenuItem as Item, project_menu_items};
    use zeron_workers_unpeel::{
        WorkersProject, WorkersSession, WorkersSessionCapabilities, WorkersSessionSort,
    };

    fn project(parent: Option<&str>, branch: Option<&str>) -> WorkersProject {
        WorkersProject {
            id: "project".into(),
            name: "Project".into(),
            path: "/tmp/project".into(),
            folder_id: None,
            parent_project_id: parent.map(str::to_owned),
            is_group: parent.is_some() && branch.is_none(),
            worktree_branch: branch.map(str::to_owned),
            git_branch: Some("main".into()),
            archived_session_count: 2,
            folder_color_id: None,
            session_sort: WorkersSessionSort::Custom,
        }
    }

    fn live_session() -> WorkersSession {
        WorkersSession {
            id: "session".into(),
            project_id: "project".into(),
            title: "Worker".into(),
            command: "claude".into(),
            state: "running".into(),
            activity: "working".into(),
            unread: false,
            pinned: false,
            archived: false,
            provider_id: None,
            active_runtime_id: None,
            runtime_launch_pending: false,
            runtime_generation: 1,
            notify_when_done: false,
            terminal_background_hex: None,
            worktree_branch: None,
            created_at_unix_ms: 0,
            updated_at_unix_ms: 0,
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
    fn main_project_menu_matches_unpeels_verb_order() {
        assert_eq!(
            project_menu_items(&project(None, None), &[live_session()]),
            vec![
                Item::NewSession,
                Item::FolderColor,
                Item::SortRecentlyUpdated,
                Item::NewWorktree,
                Item::NewGroup,
                Item::StopAll,
                Item::Archived,
                Item::RevealInFinder,
                Item::OpenInEditor,
                Item::RemoveProject,
            ]
        );
    }

    #[test]
    fn worktree_menu_renames_and_removes_the_child_only() {
        assert_eq!(
            project_menu_items(&project(Some("root"), Some("feature/sidebar")), &[]),
            vec![
                Item::Rename,
                Item::NewSession,
                Item::SortRecentlyUpdated,
                Item::Archived,
                Item::RevealInFinder,
                Item::OpenInEditor,
                Item::RemoveWorktree,
            ]
        );
    }
}
