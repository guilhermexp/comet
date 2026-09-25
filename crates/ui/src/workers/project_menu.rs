use zeron_workers_unpeel::{
    CheckoutAvailability, CheckoutKind, CheckoutOwnership, WorkersProject, WorkersSession,
    WorkersSessionSort,
};

/// Stable presentation-only parent for checkouts whose Git evidence is not
/// sufficient to associate them with a repository. It is never written into
/// the host project records or used as a launch target.
pub const ASSOCIATION_PENDING_PROJECT_ID: &str = "comet-association-pending";

pub fn is_presentation_container(project: &WorkersProject) -> bool {
    project.is_group
        && (project.id == ASSOCIATION_PENDING_PROJECT_ID
            || (project.repository_id.is_some() && project.path.trim().is_empty()))
}

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
    ArchiveCheckout,
    RestoreCheckout,
    RemoveWorktree,
    RemoveWorktreeWithoutHooks,
    RemoveGroup,
    RemoveProject,
}

pub fn checkout_is_available(project: &WorkersProject) -> bool {
    !project.is_group
        && !project.checkout_archived
        && project.checkout_kind != Some(CheckoutKind::Unresolved)
        && project
            .checkout_availability
            .is_none_or(|availability| availability == CheckoutAvailability::Available)
        && !project.path.trim().is_empty()
}

pub fn checkout_is_missing(project: &WorkersProject) -> bool {
    project.checkout_kind == Some(CheckoutKind::Unresolved)
        || matches!(
            project.checkout_availability,
            Some(CheckoutAvailability::Missing | CheckoutAvailability::ProbeFailed)
        )
}

pub fn checkout_is_linked(project: &WorkersProject) -> bool {
    !project.is_group
        && (project.parent_project_id.is_some()
            || project.worktree_branch.is_some()
            || project.checkout_kind == Some(CheckoutKind::Linked))
}

pub fn checkout_can_be_removed(project: &WorkersProject) -> bool {
    checkout_is_linked(project)
        && !project.checkout_archived
        && project
            .checkout_availability
            .is_none_or(|availability| availability == CheckoutAvailability::Available)
        && match project.checkout_ownership {
            Some(CheckoutOwnership::AppManaged) => project.owns_worktree_checkout(),
            Some(CheckoutOwnership::External | CheckoutOwnership::Unknown) => false,
            None => project.owns_worktree_checkout(),
        }
}

pub fn project_menu_items(
    project: &WorkersProject,
    sessions: &[WorkersSession],
) -> Vec<WorkersProjectMenuItem> {
    if is_presentation_container(project) {
        return Vec::new();
    }
    let is_child = project.parent_project_id.is_some();
    // A checkout is a non-group project row. The branch field is intentionally
    // not used as the discriminator: adopted worktrees and a main checkout
    // can have no persisted creation branch, while an external worktree still
    // needs the non-destructive archive action.
    let is_checkout = !project.is_group;
    let is_available = checkout_is_available(project);
    let is_managed_worktree = checkout_can_be_removed(project);
    let mut items = Vec::new();
    if is_child {
        items.push(WorkersProjectMenuItem::Rename);
    }
    if is_available {
        items.push(WorkersProjectMenuItem::NewSession);
    }
    if !is_child {
        items.push(WorkersProjectMenuItem::FolderColor);
    }
    items.push(match project.session_sort {
        WorkersSessionSort::Custom => WorkersProjectMenuItem::SortRecentlyUpdated,
        WorkersSessionSort::RecentlyUpdated => WorkersProjectMenuItem::SortCustom,
    });
    // Same gate the "In a new worktree" section of the launcher uses: a
    // worktree branches from a ROOT project, and `worktree_branch` alone misses
    // an adopted worktree, which is a child with no branch in the registry.
    if is_available && !is_child && !project.is_group {
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
    if is_available {
        items.extend([
            WorkersProjectMenuItem::RevealInFinder,
            WorkersProjectMenuItem::OpenInEditor,
        ]);
    }
    if project.is_group {
        items.push(WorkersProjectMenuItem::RemoveGroup);
    } else if project.checkout_archived && !checkout_is_missing(project) {
        items.push(WorkersProjectMenuItem::RestoreCheckout);
    } else if is_checkout {
        // Archive is always safe and reversible. A managed linked worktree
        // gets a second, explicit physical-removal action; the two verbs must
        // never share a dispatch path because archive retains sessions/files.
        items.push(WorkersProjectMenuItem::ArchiveCheckout);
        if is_managed_worktree {
            items.extend([
                WorkersProjectMenuItem::RemoveWorktree,
                WorkersProjectMenuItem::RemoveWorktreeWithoutHooks,
            ]);
        }
    } else {
        items.push(WorkersProjectMenuItem::RemoveProject);
    }
    items
}

#[cfg(test)]
mod tests {
    use super::{WorkersProjectMenuItem as Item, project_menu_items};
    use zeron_workers_unpeel::{
        CheckoutAvailability, CheckoutKind, CheckoutOwnership, WorkersProject, WorkersSession,
        WorkersSessionCapabilities, WorkersSessionSort,
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
            repository_id: None,
            repository_name: None,
            repository_path: None,
            checkout_kind: None,
            checkout_ownership: None,
            checkout_availability: None,
            checkout_archived: false,
            checkout_detached: false,
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
            idle_since_unix_ms: None,
            idle_confirmed_by_hook: false,
            resumable_conversation: false,
            total_tokens: None,
            model_usage: Vec::new(),
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
                Item::ArchiveCheckout,
            ]
        );
    }

    #[test]
    fn external_worktree_menu_archives_without_physical_deletion() {
        assert_eq!(
            project_menu_items(&project(Some("root"), Some("feature/sidebar")), &[]),
            vec![
                Item::Rename,
                Item::NewSession,
                Item::SortRecentlyUpdated,
                Item::Archived,
                Item::RevealInFinder,
                Item::OpenInEditor,
                Item::ArchiveCheckout,
            ]
        );
    }

    #[test]
    fn managed_worktree_exposes_hook_override_as_a_separate_removal_action() {
        let mut managed = project(Some("root"), Some("feature/sidebar"));
        managed.checkout_kind = Some(CheckoutKind::Linked);
        managed.checkout_ownership = Some(CheckoutOwnership::AppManaged);
        managed.checkout_availability = Some(CheckoutAvailability::Available);

        let items = project_menu_items(&managed, &[]);
        assert!(items.contains(&Item::RemoveWorktree));
        assert!(items.contains(&Item::RemoveWorktreeWithoutHooks));

        let mut external = managed;
        external.checkout_ownership = Some(CheckoutOwnership::External);
        let external_items = project_menu_items(&external, &[]);
        assert!(!external_items.contains(&Item::RemoveWorktree));
        assert!(!external_items.contains(&Item::RemoveWorktreeWithoutHooks));
    }

    /// An adopted worktree on a detached HEAD: a child with a parent projected
    /// from disk and no registry branch, but not an organization. Routing it to
    /// `RemoveGroup` would die on "project is not a group".
    #[test]
    fn adopted_worktree_without_a_branch_removes_as_a_project() {
        let mut adopted = project(Some("root"), None);
        adopted.is_group = false;
        let items = project_menu_items(&adopted, &[]);
        assert_eq!(items.last(), Some(&Item::ArchiveCheckout));
        // And no worktree branches off a worktree.
        assert!(!items.contains(&Item::NewWorktree));
    }

    #[test]
    fn unavailable_checkout_keeps_history_without_launch_or_filesystem_actions() {
        let mut missing = project(Some("root"), Some("feature/sidebar"));
        missing.checkout_availability = Some(CheckoutAvailability::Missing);
        let items = project_menu_items(&missing, &[]);
        assert!(!items.contains(&Item::NewSession));
        assert!(!items.contains(&Item::RevealInFinder));
        assert!(!items.contains(&Item::OpenInEditor));
        assert!(items.contains(&Item::ArchiveCheckout));
    }

    #[test]
    fn unresolved_checkout_is_pending_and_cannot_launch() {
        let mut pending = project(None, None);
        pending.checkout_kind = Some(CheckoutKind::Unresolved);
        let items = project_menu_items(&pending, &[]);
        assert!(!items.contains(&Item::NewSession));
        assert!(!items.contains(&Item::RevealInFinder));
        assert!(!items.contains(&Item::OpenInEditor));
        assert!(super::checkout_is_missing(&pending));
    }

    #[test]
    fn archived_checkout_offers_restore_instead_of_archive_or_delete() {
        let mut archived = project(Some("root"), Some("feature/sidebar"));
        archived.checkout_archived = true;
        assert_eq!(
            project_menu_items(&archived, &[]).last(),
            Some(&Item::RestoreCheckout)
        );
    }

    #[test]
    fn archived_missing_checkout_does_not_offer_unavailable_restore() {
        let mut archived = project(Some("root"), Some("feature/sidebar"));
        archived.checkout_archived = true;
        archived.checkout_availability = Some(CheckoutAvailability::Missing);
        assert!(!project_menu_items(&archived, &[]).contains(&Item::RestoreCheckout));
    }

    #[test]
    fn synthetic_repository_container_has_no_project_actions() {
        let mut container = project(None, None);
        container.id = "repo-1".into();
        container.is_group = true;
        container.path.clear();
        container.repository_id = Some("repo-1".into());
        assert!(project_menu_items(&container, &[]).is_empty());
    }
}
