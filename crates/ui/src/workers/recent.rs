use std::collections::{HashMap, HashSet};

use chrono::{DateTime, Local, TimeZone};
use zeron_workers_unpeel::{
    WorkersActivityLogKind, WorkersBootstrap, WorkersProject, WorkersSession,
};

use super::activity_menu::project_activity_menu;
use super::model::WorkersSessionTarget;
use super::presentation::{relative_age, runtime_icon_path};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecentActivityRow {
    pub id: String,
    pub target: Option<WorkersSessionTarget>,
    pub title: String,
    pub project: String,
    pub event: String,
    pub runtime_icon: &'static str,
    pub spinner_tint: Option<u32>,
    pub working: bool,
    pub unread: bool,
    pub available: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecentActivitySection {
    pub label: String,
    pub rows: Vec<RecentActivityRow>,
}

pub fn recent_activity_sections(
    snapshot: &WorkersBootstrap,
    now_unix_ms: u64,
) -> Vec<RecentActivitySection> {
    let projects = snapshot
        .projects
        .iter()
        .map(|project| (project.id.as_str(), project))
        .collect::<HashMap<_, _>>();
    let sessions = snapshot
        .sessions
        .iter()
        .map(|session| (session.id.as_str(), session))
        .collect::<HashMap<_, _>>();
    let menu = project_activity_menu(snapshot);
    let active_ids = menu
        .jobs
        .iter()
        .map(|row| row.session_id.clone())
        .collect::<HashSet<_>>();
    let mut sections = Vec::new();

    if !menu.jobs.is_empty() {
        sections.push(RecentActivitySection {
            label: "Active".to_owned(),
            rows: menu
                .jobs
                .into_iter()
                .map(|row| RecentActivityRow {
                    id: format!("active:{}", row.session_id),
                    target: Some(WorkersSessionTarget::new(row.project_id, row.session_id)),
                    title: card_title(&row.title),
                    project: row.project,
                    event: row.status.to_owned(),
                    runtime_icon: row.runtime_icon,
                    spinner_tint: row.spinner_tint,
                    working: true,
                    unread: false,
                    available: true,
                })
                .collect(),
        });
    }

    let mut current_day: Option<chrono::NaiveDate> = None;
    for entry in snapshot.activity_log.iter().rev() {
        if active_ids.contains(&entry.session_id) {
            continue;
        }
        let at = local_datetime(entry.at_unix_ms);
        let day = at.date_naive();
        if current_day != Some(day) {
            sections.push(RecentActivitySection {
                label: day_label(at, local_datetime(now_unix_ms)),
                rows: Vec::new(),
            });
            current_day = Some(day);
        }
        let live = sessions.get(entry.session_id.as_str()).copied();
        let title = live
            .map(|session| card_title(&session.title))
            .unwrap_or_else(|| card_title(&entry.title));
        let project = live
            .and_then(|session| projects.get(session.project_id.as_str()).copied())
            .map(|project| project_name(project, &projects))
            .unwrap_or_else(|| entry.project_name.clone());
        let command = live
            .map(|session| session.command.as_str())
            .unwrap_or(entry.command.as_str());
        let runtime_id = live.and_then(session_runtime_id);
        let age = relative_age(entry.at_unix_ms, now_unix_ms);
        let when = if age == "now" {
            "just now".to_owned()
        } else {
            format!("{age} ago")
        };
        let event = match entry.kind {
            WorkersActivityLogKind::Started => format!("Started {when}"),
            WorkersActivityLogKind::NeedsInput => format!("Needed input {when}"),
            WorkersActivityLogKind::Finished => format!("Finished {when}"),
            WorkersActivityLogKind::Exited => format!("Exited {when}"),
        };
        sections
            .last_mut()
            .expect("day section exists")
            .rows
            .push(RecentActivityRow {
                id: entry.id.clone(),
                target: live
                    .map(|session| WorkersSessionTarget::new(&session.project_id, &session.id)),
                title,
                project,
                event,
                runtime_icon: runtime_icon_path(runtime_id, Some(command)),
                spinner_tint: None,
                working: false,
                unread: live.is_some_and(|session| session.unread),
                available: live.is_some(),
            });
    }

    sections
}

fn card_title(raw: &str) -> String {
    let title = raw.trim();
    if title.is_empty() {
        "Untitled session".to_owned()
    } else {
        title.to_owned()
    }
}

fn session_runtime_id(session: &WorkersSession) -> Option<&str> {
    session
        .active_runtime_id
        .as_deref()
        .or(session.provider_id.as_deref())
}

fn project_name(project: &WorkersProject, projects: &HashMap<&str, &WorkersProject>) -> String {
    if project.worktree_branch.is_none()
        && let Some(parent) = project
            .parent_project_id
            .as_deref()
            .and_then(|parent_id| projects.get(parent_id))
    {
        return format!("{} › {}", parent.name, project.name);
    }
    project.name.clone()
}

fn local_datetime(unix_ms: u64) -> DateTime<Local> {
    Local
        .timestamp_millis_opt(i64::try_from(unix_ms).unwrap_or(i64::MAX))
        .single()
        .unwrap_or_else(Local::now)
}

fn day_label(day: DateTime<Local>, now: DateTime<Local>) -> String {
    let date = day.date_naive();
    let today = now.date_naive();
    if date == today {
        "Today".to_owned()
    } else if date == today.pred_opt().unwrap_or(today) {
        "Yesterday".to_owned()
    } else {
        day.format("%b %-d").to_string()
    }
}

#[cfg(test)]
mod tests {
    use zeron_workers_unpeel::{
        WorkersActivityLogEntry, WorkersActivityLogKind, WorkersBootstrap, WorkersProject,
        WorkersProtocol, WorkersSession, WorkersSessionCapabilities,
    };

    use super::{WorkersSessionTarget, recent_activity_sections};

    fn session(id: &str, activity: &str) -> WorkersSession {
        WorkersSession {
            id: id.to_owned(),
            project_id: "project".to_owned(),
            title: format!("Live {id}"),
            command: "claude".to_owned(),
            state: "running".to_owned(),
            activity: activity.to_owned(),
            unread: false,
            pinned: false,
            archived: false,
            provider_id: Some("claude".to_owned()),
            active_runtime_id: Some("claude".to_owned()),
            runtime_launch_pending: false,
            runtime_generation: 1,
            notify_when_done: false,
            terminal_background_hex: None,
            worktree_branch: None,
            created_at_unix_ms: 1,
            updated_at_unix_ms: 2,
            idle_since_unix_ms: None,
            total_tokens: None,
            model_usage: Vec::new(),
            capabilities: WorkersSessionCapabilities::default(),
        }
    }

    #[test]
    fn active_sessions_are_first_and_removed_from_the_feed() {
        let snapshot = WorkersBootstrap {
            mac_name: "Mac".to_owned(),
            protocol: WorkersProtocol {
                major_version: 1,
                minor_version: 0,
                capabilities: Vec::new(),
            },
            projects: vec![WorkersProject {
                id: "project".to_owned(),
                name: "Project".to_owned(),
                path: "/tmp/project".to_owned(),
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
            sessions: vec![session("busy", "working"), session("idle", "idle")],
            activity_log: vec![
                WorkersActivityLogEntry {
                    id: "old-busy".to_owned(),
                    session_id: "busy".to_owned(),
                    kind: WorkersActivityLogKind::Started,
                    at_unix_ms: 1_700_000_000_000,
                    title: "Busy".to_owned(),
                    command: "claude".to_owned(),
                    project_id: "project".to_owned(),
                    project_name: "Project".to_owned(),
                },
                WorkersActivityLogEntry {
                    id: "idle-finished".to_owned(),
                    session_id: "idle".to_owned(),
                    kind: WorkersActivityLogKind::Finished,
                    at_unix_ms: 1_700_000_010_000,
                    title: "Old title".to_owned(),
                    command: "claude".to_owned(),
                    project_id: "project".to_owned(),
                    project_name: "Project".to_owned(),
                },
            ],
        };

        let sections = recent_activity_sections(&snapshot, 1_700_000_020_000);
        assert_eq!(sections[0].label, "Active");
        assert_eq!(
            sections[0].rows[0].target,
            Some(WorkersSessionTarget::new("project", "busy"))
        );
        assert_eq!(
            sections
                .iter()
                .map(|section| section.rows.len())
                .sum::<usize>(),
            2
        );
        assert_eq!(sections[1].rows[0].title, "Live idle");
        assert_eq!(sections[1].rows[0].event, "Finished 10s ago");
    }

    #[test]
    fn recent_rows_preserve_the_exact_project_and_session_identity() {
        let mut left = session("left-session", "working");
        left.project_id = "project-left".to_owned();
        left.title = "Same title".to_owned();
        let mut right = session("right-session", "working");
        right.project_id = "project-right".to_owned();
        right.title = "Same title".to_owned();
        let project = |id: &str| WorkersProject {
            id: id.to_owned(),
            name: id.to_owned(),
            path: format!("/tmp/{id}"),
            folder_id: None,
            parent_project_id: None,
            is_group: false,
            worktree_branch: None,
            git_branch: None,
            archived_session_count: 0,
            folder_color_id: None,
            session_sort: Default::default(),
        };
        let snapshot = WorkersBootstrap {
            mac_name: "Mac".to_owned(),
            protocol: WorkersProtocol {
                major_version: 1,
                minor_version: 0,
                capabilities: Vec::new(),
            },
            projects: vec![project("project-left"), project("project-right")],
            presets: Vec::new(),
            sessions: vec![left, right],
            activity_log: Vec::new(),
        };

        let sections = recent_activity_sections(&snapshot, 10);
        let targets = sections[0]
            .rows
            .iter()
            .map(|row| row.target.clone())
            .collect::<Vec<_>>();
        assert_eq!(
            targets,
            vec![
                Some(crate::workers::model::WorkersSessionTarget::new(
                    "project-left",
                    "left-session",
                )),
                Some(crate::workers::model::WorkersSessionTarget::new(
                    "project-right",
                    "right-session",
                )),
            ]
        );
    }
}
