use std::collections::{HashMap, HashSet};

use zeron_workers_unpeel::{WorkersBootstrap, WorkersSession};

use super::presentation::{
    SessionIndicator, runtime_icon_path, runtime_spinner_tint, session_indicator,
};

pub const POPOVER_WIDTH: f64 = 332.0;
pub const CONTENT_WIDTH: f64 = 320.0;
pub const OUTER_PADDING: f64 = 12.0;
pub const EMPTY_BODY_HEIGHT: f64 = 34.0;
pub const ROW_HEIGHT: f64 = 42.0;
pub const DIVIDER_HEIGHT: f64 = 9.0;
pub const FOOTER_HEIGHT: f64 = 28.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum WorkersMenuBarMode {
    Working {
        blocked: bool,
    },
    Blocked,
    Unread,
    #[default]
    Idle,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkersActivityRowKind {
    Working,
    Blocked,
    Unread,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkersActivityRow {
    pub session_id: String,
    pub title: String,
    pub project: String,
    pub status: &'static str,
    pub command: String,
    pub runtime_icon: &'static str,
    pub spinner_tint: Option<u32>,
    pub kind: WorkersActivityRowKind,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct WorkersActivityMenu {
    pub mode: WorkersMenuBarMode,
    pub blockers: Vec<WorkersActivityRow>,
    pub jobs: Vec<WorkersActivityRow>,
    pub finished: Vec<WorkersActivityRow>,
}

impl WorkersActivityMenu {
    pub fn is_empty(&self) -> bool {
        self.blockers.is_empty() && self.jobs.is_empty() && self.finished.is_empty()
    }

    pub fn section_count(&self) -> usize {
        [&self.blockers, &self.jobs, &self.finished]
            .into_iter()
            .filter(|rows| !rows.is_empty())
            .count()
    }

    pub fn row_count(&self) -> usize {
        self.blockers.len() + self.jobs.len() + self.finished.len()
    }
}

pub fn project_activity_menu(snapshot: &WorkersBootstrap) -> WorkersActivityMenu {
    let projects = snapshot
        .projects
        .iter()
        .map(|project| (project.id.as_str(), project))
        .collect::<HashMap<_, _>>();
    let mut seen = HashSet::new();

    let blockers = snapshot
        .sessions
        .iter()
        .filter(|session| {
            session_indicator(
                &session.state,
                &session.activity,
                session.unread,
                session.runtime_launch_pending,
            ) == SessionIndicator::Attention
        })
        .filter(|session| seen.insert(session.id.clone()))
        .map(|session| activity_row(session, WorkersActivityRowKind::Blocked, &projects))
        .collect::<Vec<_>>();

    let mut jobs = snapshot
        .sessions
        .iter()
        .filter(|session| {
            matches!(
                session_indicator(
                    &session.state,
                    &session.activity,
                    session.unread,
                    session.runtime_launch_pending,
                ),
                SessionIndicator::Busy | SessionIndicator::Restarting
            )
        })
        .filter(|session| seen.insert(session.id.clone()))
        .collect::<Vec<_>>();
    jobs.sort_by(|left, right| {
        right
            .updated_at_unix_ms
            .cmp(&left.updated_at_unix_ms)
            .then_with(|| left.id.cmp(&right.id))
    });
    let jobs = jobs
        .into_iter()
        .map(|session| activity_row(session, WorkersActivityRowKind::Working, &projects))
        .collect::<Vec<_>>();

    let finished = snapshot
        .sessions
        .iter()
        .filter(|session| session.unread)
        .filter(|session| seen.insert(session.id.clone()))
        .map(|session| activity_row(session, WorkersActivityRowKind::Unread, &projects))
        .collect::<Vec<_>>();

    let mode = if !jobs.is_empty() {
        WorkersMenuBarMode::Working {
            blocked: !blockers.is_empty(),
        }
    } else if !blockers.is_empty() {
        WorkersMenuBarMode::Blocked
    } else if !finished.is_empty() {
        WorkersMenuBarMode::Unread
    } else {
        WorkersMenuBarMode::Idle
    };

    WorkersActivityMenu {
        mode,
        blockers,
        jobs,
        finished,
    }
}

pub fn menu_popover_size(menu: &WorkersActivityMenu) -> (f64, f64) {
    let body = if menu.is_empty() {
        EMPTY_BODY_HEIGHT
    } else {
        menu.row_count() as f64 * ROW_HEIGHT
            + menu.section_count().saturating_sub(1) as f64 * DIVIDER_HEIGHT
    };
    (POPOVER_WIDTH, OUTER_PADDING + body + FOOTER_HEIGHT)
}

fn activity_row<'a>(
    session: &WorkersSession,
    kind: WorkersActivityRowKind,
    projects: &HashMap<&'a str, &'a zeron_workers_unpeel::WorkersProject>,
) -> WorkersActivityRow {
    let status = match kind {
        WorkersActivityRowKind::Blocked => "Blocked",
        WorkersActivityRowKind::Unread if session.state != "running" => "Exited",
        WorkersActivityRowKind::Unread => "Done",
        WorkersActivityRowKind::Working if session.runtime_launch_pending => {
            if session.is_live() {
                "Restarting"
            } else {
                "Resuming"
            }
        }
        WorkersActivityRowKind::Working if session.activity == "starting" => "Starting",
        WorkersActivityRowKind::Working => "Working",
    };
    let project = projects
        .get(session.project_id.as_str())
        .map(|project| {
            if project.worktree_branch.is_none() {
                project
                    .parent_project_id
                    .as_deref()
                    .and_then(|parent_id| projects.get(parent_id))
                    .map(|parent| format!("{} › {}", parent.name, project.name))
                    .unwrap_or_else(|| project.name.clone())
            } else {
                project.name.clone()
            }
        })
        .unwrap_or_else(|| "Unknown project".to_owned());
    let runtime_id = session
        .active_runtime_id
        .as_deref()
        .or(session.provider_id.as_deref());
    WorkersActivityRow {
        session_id: session.id.clone(),
        title: session.title.clone(),
        project,
        status,
        command: session.command.clone(),
        runtime_icon: runtime_icon_path(runtime_id, Some(&session.command)),
        spinner_tint: runtime_spinner_tint(runtime_id, Some(&session.command)),
        kind,
    }
}

#[cfg(test)]
mod tests {
    use zeron_workers_unpeel::{
        WorkersBootstrap, WorkersProject, WorkersProtocol, WorkersSession,
        WorkersSessionCapabilities,
    };

    use super::{
        WorkersActivityMenu, WorkersActivityRowKind, WorkersMenuBarMode, menu_popover_size,
        project_activity_menu,
    };

    fn session(id: &str, activity: &str, unread: bool) -> WorkersSession {
        WorkersSession {
            id: id.to_owned(),
            project_id: "project-a".to_owned(),
            title: id.to_owned(),
            command: "claude".to_owned(),
            state: "running".to_owned(),
            activity: activity.to_owned(),
            unread,
            pinned: false,
            archived: false,
            provider_id: Some("claude".to_owned()),
            active_runtime_id: Some("claude".to_owned()),
            runtime_launch_pending: false,
            notify_when_done: false,
            terminal_background_hex: None,
            worktree_branch: None,
            created_at_unix_ms: 1,
            updated_at_unix_ms: 1,
            capabilities: WorkersSessionCapabilities::default(),
        }
    }

    fn menu(sessions: Vec<WorkersSession>) -> WorkersActivityMenu {
        project_activity_menu(&WorkersBootstrap {
            mac_name: "Mac".to_owned(),
            protocol: WorkersProtocol {
                major_version: 1,
                minor_version: 0,
                capabilities: Vec::new(),
            },
            projects: vec![WorkersProject {
                id: "project-a".to_owned(),
                name: "Project A".to_owned(),
                path: "/tmp/project-a".to_owned(),
                folder_id: None,
                parent_project_id: None,
                is_group: false,
                worktree_branch: None,
                git_branch: None,
                archived_session_count: 0,
                folder_color_id: None,
                session_sort: Default::default(),
            }],
            presets: Vec::new(),
            sessions,
            activity_log: Vec::new(),
        })
    }

    #[test]
    fn status_mode_matches_unpeel_precedence() {
        assert_eq!(menu(Vec::new()).mode, WorkersMenuBarMode::Idle);
        assert_eq!(
            menu(vec![session("done", "done", true)]).mode,
            WorkersMenuBarMode::Unread
        );
        assert_eq!(
            menu(vec![session("blocked", "blocked", false)]).mode,
            WorkersMenuBarMode::Blocked
        );
        assert_eq!(
            menu(vec![
                session("working", "working", false),
                session("blocked", "blocked", false),
            ])
            .mode,
            WorkersMenuBarMode::Working { blocked: true }
        );
    }

    #[test]
    fn blockers_jobs_and_finished_are_unique_and_ordered() {
        let mut working = session("working-b", "working", false);
        working.updated_at_unix_ms = 30;
        let projection = menu(vec![
            session("blocked-a", "blocked", true),
            working,
            session("finished-a", "done", true),
        ]);
        assert_eq!(
            projection
                .blockers
                .iter()
                .map(|row| row.session_id.as_str())
                .collect::<Vec<_>>(),
            ["blocked-a"]
        );
        assert_eq!(projection.blockers[0].kind, WorkersActivityRowKind::Blocked);
        assert_eq!(projection.jobs[0].session_id, "working-b");
        assert_eq!(projection.finished[0].session_id, "finished-a");
    }

    #[test]
    fn explicit_popover_height_matches_unpeel_rows_and_dividers() {
        assert_eq!(
            menu_popover_size(&WorkersActivityMenu::default()),
            (332.0, 74.0)
        );
        let populated = menu(vec![
            session("a", "blocked", false),
            session("b", "working", false),
            session("c", "done", true),
        ]);
        assert_eq!(menu_popover_size(&populated), (332.0, 184.0));
    }
}
