use std::collections::{HashMap, HashSet, VecDeque};
use std::path::PathBuf;
use std::time::Duration;

use gpui::{Context, Task};
use zeron_workers_unpeel::{
    LocalWorkersClient, PresetPatch, SessionAction, SessionOrganizationPatch, WorkersBootstrap,
    WorkersCreateGroupRequest, WorkersCreateWorktreeRequest, WorkersLaunchRequest,
    WorkersNotificationSettings, WorkersPreset, WorkersProject, WorkersProjectOrganizationPatch,
    WorkersSession, WorkersSessionSort, WorkersSettingsSnapshot, WorkersTranscriptSettings,
};

use super::archive::{archived_sessions_for_project, restore_action};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkersSettingsTab {
    Presets,
    Transcripts,
    Notifications,
}

impl WorkersSettingsTab {
    pub const ALL: [Self; 3] = [Self::Presets, Self::Transcripts, Self::Notifications];

    pub fn label(self) -> &'static str {
        match self {
            Self::Presets => "Presets",
            Self::Transcripts => "Transcripts",
            Self::Notifications => "Notifications",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkersRoute {
    Workspace,
    Settings(WorkersSettingsTab),
    Recent,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkersRevealTarget {
    Workspace,
    Session(String),
    Recent,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkersReveal {
    pub generation: u64,
    pub target: WorkersRevealTarget,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkerNotification {
    Attention,
    Done,
}

pub fn notification_transition(
    previous: Option<(&str, bool)>,
    activity: &str,
    unread: bool,
) -> Option<WorkerNotification> {
    let was_attention = previous.is_some_and(|(activity, _)| activity == "blocked");
    let was_done = previous.is_some_and(|(activity, unread)| activity == "done" && unread);
    if activity == "blocked" && !was_attention {
        Some(WorkerNotification::Attention)
    } else if activity == "done" && unread && !was_done {
        Some(WorkerNotification::Done)
    } else {
        None
    }
}

pub fn reconcile_selection(current: Option<&str>, sessions: &[WorkersSession]) -> Option<String> {
    current
        .filter(|current| sessions.iter().any(|session| session.id == *current))
        .map(str::to_owned)
}

pub fn reconcile_selection_with_pending(
    current: Option<&str>,
    pending_session_id: Option<&str>,
    sessions: &[WorkersSession],
) -> Option<String> {
    if let Some(pending_session_id) = pending_session_id
        && current == Some(pending_session_id)
        && !sessions
            .iter()
            .any(|session| session.id == pending_session_id)
    {
        return Some(pending_session_id.to_owned());
    }
    reconcile_selection(current, sessions)
}

pub fn sessions_for_project<'a>(
    sessions: &'a [WorkersSession],
    project_id: &str,
) -> Vec<&'a WorkersSession> {
    sessions
        .iter()
        .filter(|session| session.project_id == project_id && !session.archived)
        .collect()
}

pub fn toggle_expanded(expanded: &mut HashSet<String>, project_id: &str) {
    if !expanded.remove(project_id) {
        expanded.insert(project_id.to_owned());
    }
}

#[derive(Debug, Clone)]
struct PendingReplacement {
    source_id: String,
    project_id: String,
    command: String,
    provider_id: Option<String>,
    worktree_branch: Option<String>,
    source_created_at_unix_ms: u64,
    baseline_ids: HashSet<String>,
    remaining_refreshes: u8,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct PendingRemove {
    session_id: String,
    archived: bool,
}

fn dispatch_or_queue_remove(
    action_running: bool,
    queued: &mut Option<PendingRemove>,
    request: PendingRemove,
) -> Option<PendingRemove> {
    if action_running {
        *queued = Some(request);
        None
    } else {
        Some(request)
    }
}

#[derive(Debug, Clone)]
struct PendingLaunchSelection {
    session_id: String,
    remaining_refreshes: u8,
}

fn replacement_selection(
    pending: &PendingReplacement,
    sessions: &[WorkersSession],
) -> Option<Option<String>> {
    let candidates = sessions
        .iter()
        .filter(|session| {
            session.project_id == pending.project_id
                && !pending.baseline_ids.contains(&session.id)
                && session.is_live()
                && session.command == pending.command
                && session.provider_id == pending.provider_id
                && session.worktree_branch == pending.worktree_branch
                && session.created_at_unix_ms >= pending.source_created_at_unix_ms
        })
        .map(|session| session.id.clone())
        .collect::<Vec<_>>();
    match candidates.as_slice() {
        [only] => Some(Some(only.clone())),
        [] => None,
        _ => Some(None),
    }
}

pub struct WorkersModel {
    client: LocalWorkersClient,
    pub snapshot: Option<WorkersBootstrap>,
    pub selected_project_id: Option<String>,
    pub selected_session_id: Option<String>,
    pub expanded_project_ids: HashSet<String>,
    pub loading: bool,
    pub error: Option<String>,
    pub archive_project_id: Option<String>,
    pub archived_sessions: Vec<WorkersSession>,
    pub archive_loading: bool,
    pub archive_error: Option<String>,
    pub route: WorkersRoute,
    reveal: WorkersReveal,
    pub settings: Option<WorkersSettingsSnapshot>,
    pub settings_loading: bool,
    pub settings_error: Option<String>,
    pub confirming_remove_session_id: Option<String>,
    pub confirming_remove_project: Option<WorkersProject>,
    confirming_remove_archived: bool,
    initialized_expansion: bool,
    refresh_generation: u64,
    refresh_task: Option<Task<()>>,
    action_task: Option<Task<()>>,
    pending_remove: Option<PendingRemove>,
    launch_queue: VecDeque<WorkersLaunchRequest>,
    launch_task: Option<Task<()>>,
    pending_launch_selection: Option<PendingLaunchSelection>,
    archive_generation: u64,
    archive_task: Option<Task<()>>,
    settings_generation: u64,
    settings_task: Option<Task<()>>,
    notification_state: HashMap<String, (String, bool)>,
    pending_replacement: Option<PendingReplacement>,
    _poll_task: Task<()>,
}

impl WorkersModel {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let poll_task = cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(Duration::from_secs(1)).await;
                if this.update(cx, |model, cx| model.refresh(cx)).is_err() {
                    break;
                }
            }
        });
        let mut model = Self {
            client: LocalWorkersClient::new(),
            snapshot: None,
            selected_project_id: None,
            selected_session_id: None,
            expanded_project_ids: HashSet::new(),
            loading: true,
            error: None,
            archive_project_id: None,
            archived_sessions: Vec::new(),
            archive_loading: false,
            archive_error: None,
            route: WorkersRoute::Workspace,
            reveal: WorkersReveal {
                generation: 0,
                target: WorkersRevealTarget::Workspace,
            },
            settings: None,
            settings_loading: false,
            settings_error: None,
            confirming_remove_session_id: None,
            confirming_remove_project: None,
            confirming_remove_archived: false,
            initialized_expansion: false,
            refresh_generation: 0,
            refresh_task: None,
            action_task: None,
            pending_remove: None,
            launch_queue: VecDeque::new(),
            launch_task: None,
            pending_launch_selection: None,
            archive_generation: 0,
            archive_task: None,
            settings_generation: 0,
            settings_task: None,
            notification_state: HashMap::new(),
            pending_replacement: None,
            _poll_task: poll_task,
        };
        model.refresh(cx);
        model
    }

    pub fn projects(&self) -> &[WorkersProject] {
        self.snapshot
            .as_ref()
            .map(|snapshot| snapshot.projects.as_slice())
            .unwrap_or_default()
    }

    pub fn sessions(&self) -> &[WorkersSession] {
        self.snapshot
            .as_ref()
            .map(|snapshot| snapshot.sessions.as_slice())
            .unwrap_or_default()
    }

    pub fn presets(&self) -> &[WorkersPreset] {
        self.snapshot
            .as_ref()
            .map(|snapshot| snapshot.presets.as_slice())
            .unwrap_or_default()
    }

    pub fn action_in_flight(&self) -> bool {
        self.action_task.is_some() || self.launch_task.is_some() || !self.launch_queue.is_empty()
    }

    pub fn has_attention(&self) -> bool {
        self.sessions()
            .iter()
            .any(|session| session.unread || (session.is_live() && session.activity == "blocked"))
    }

    pub fn reveal(&self) -> &WorkersReveal {
        &self.reveal
    }

    pub fn request_session_reveal(&mut self, session_id: String, cx: &mut Context<Self>) {
        self.route = WorkersRoute::Workspace;
        self.reveal.generation = self.reveal.generation.wrapping_add(1);
        if self
            .sessions()
            .iter()
            .any(|session| session.id == session_id)
        {
            self.select_session(session_id.clone(), cx);
            self.reveal.target = WorkersRevealTarget::Session(session_id);
        } else {
            self.selected_project_id = None;
            self.selected_session_id = None;
            self.reveal.target = WorkersRevealTarget::Workspace;
            cx.notify();
        }
    }

    pub fn request_recent_reveal(&mut self, cx: &mut Context<Self>) {
        self.route = WorkersRoute::Recent;
        self.reveal.generation = self.reveal.generation.wrapping_add(1);
        self.reveal.target = WorkersRevealTarget::Recent;
        cx.notify();
    }

    pub fn close_recent(&mut self, cx: &mut Context<Self>) {
        self.route = WorkersRoute::Workspace;
        cx.notify();
    }

    pub fn open_settings(&mut self, tab: WorkersSettingsTab, cx: &mut Context<Self>) {
        self.route = WorkersRoute::Settings(tab);
        self.refresh_settings(cx);
        cx.notify();
    }

    pub fn close_settings(&mut self, cx: &mut Context<Self>) {
        self.route = WorkersRoute::Workspace;
        cx.notify();
    }

    pub fn set_settings_tab(&mut self, tab: WorkersSettingsTab, cx: &mut Context<Self>) {
        self.route = WorkersRoute::Settings(tab);
        if self.settings.is_none() {
            self.refresh_settings(cx);
        }
        cx.notify();
    }

    pub fn refresh_settings(&mut self, cx: &mut Context<Self>) {
        if self.settings_task.is_some() {
            return;
        }
        self.settings_generation = self.settings_generation.wrapping_add(1);
        let generation = self.settings_generation;
        let client = self.client.clone();
        self.settings_loading = self.settings.is_none();
        self.settings_task = Some(cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { client.settings() })
                .await;
            this.update(cx, |model, cx| {
                if generation != model.settings_generation {
                    return;
                }
                model.settings_task = None;
                model.settings_loading = false;
                match result {
                    Ok(settings) => {
                        model.settings = Some(settings);
                        model.settings_error = None;
                    }
                    Err(error) => model.settings_error = Some(error.to_string()),
                }
                cx.notify();
            })
            .ok();
        }));
    }

    pub fn selected_session(&self) -> Option<&WorkersSession> {
        let selected = self.selected_session_id.as_deref()?;
        self.sessions()
            .iter()
            .find(|session| session.id == selected)
    }

    pub fn selected_project(&self) -> Option<&WorkersProject> {
        let selected = self.selected_project_id.as_deref().or_else(|| {
            self.selected_session()
                .map(|session| session.project_id.as_str())
        })?;
        self.projects()
            .iter()
            .find(|project| project.id == selected)
    }

    pub fn sessions_for_project(&self, project_id: &str) -> Vec<&WorkersSession> {
        sessions_for_project(self.sessions(), project_id)
    }

    pub fn select_session(&mut self, session_id: String, cx: &mut Context<Self>) {
        let Some(session) = self
            .sessions()
            .iter()
            .find(|session| session.id == session_id)
        else {
            return;
        };
        let project_id = session.project_id.clone();
        let was_unread = session.unread;
        self.pending_replacement = None;
        self.pending_launch_selection = None;
        self.reset_archive_view();
        self.route = WorkersRoute::Workspace;
        self.selected_project_id = Some(project_id);
        self.selected_session_id = Some(session_id);
        if was_unread {
            let selected = self.selected_session_id.clone().unwrap_or_default();
            self.run_unit_action(move |client| client.mark_read(&selected), cx);
        }
        cx.notify();
    }

    pub fn select_project(&mut self, project_id: String, cx: &mut Context<Self>) {
        if !self
            .projects()
            .iter()
            .any(|project| project.id == project_id)
        {
            return;
        }
        self.pending_replacement = None;
        self.pending_launch_selection = None;
        self.reset_archive_view();
        self.selected_project_id = Some(project_id);
        self.selected_session_id = None;
        cx.notify();
    }

    pub fn toggle_project(&mut self, project_id: &str, cx: &mut Context<Self>) {
        toggle_expanded(&mut self.expanded_project_ids, project_id);
        cx.notify();
    }

    pub fn collapse_all_projects(&mut self, cx: &mut Context<Self>) {
        if self.expanded_project_ids.is_empty() {
            return;
        }
        self.expanded_project_ids.clear();
        cx.notify();
    }

    pub fn refresh(&mut self, cx: &mut Context<Self>) {
        if self.refresh_task.is_some() {
            return;
        }
        self.refresh_generation = self.refresh_generation.wrapping_add(1);
        let generation = self.refresh_generation;
        let client = self.client.clone();
        self.loading = self.snapshot.is_none();
        self.refresh_task = Some(cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { client.bootstrap() })
                .await;
            this.update(cx, |model, cx| {
                if generation != model.refresh_generation {
                    return;
                }
                model.refresh_task = None;
                model.loading = false;
                match result {
                    Ok(snapshot) => {
                        let app_focused = cx.active_window().is_some();
                        model.apply_snapshot(snapshot, app_focused);
                    }
                    Err(error) => model.error = Some(error.to_string()),
                }
                cx.notify();
            })
            .ok();
        }));
    }

    pub fn launch(&mut self, request: WorkersLaunchRequest, cx: &mut Context<Self>) {
        self.launch_queue.push_back(request);
        self.start_next_launch(cx);
        cx.notify();
    }

    fn start_next_launch(&mut self, cx: &mut Context<Self>) {
        if self.launch_task.is_some() {
            return;
        }
        let Some(request) = self.launch_queue.pop_front() else {
            return;
        };
        let project_id = request.project_id.clone();
        let client = self.client.clone();
        self.launch_task = Some(cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { client.launch_session(&request) })
                .await;
            this.update(cx, |model, cx| {
                model.launch_task = None;
                match result {
                    Ok(session_id) => {
                        model.reset_archive_view();
                        model.route = WorkersRoute::Workspace;
                        model.selected_project_id = Some(project_id.clone());
                        model.selected_session_id = Some(session_id.clone());
                        model.expanded_project_ids.insert(project_id.clone());
                        model.pending_launch_selection = Some(PendingLaunchSelection {
                            session_id,
                            remaining_refreshes: 12,
                        });
                        model.refresh(cx);
                    }
                    Err(error) => model.error = Some(error.to_string()),
                }
                model.start_next_launch(cx);
                cx.notify();
            })
            .ok();
        }));
    }

    pub fn add_project(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        self.run_action(
            move |client| client.add_project(&path),
            |model, project_id| {
                model.route = WorkersRoute::Workspace;
                model.selected_project_id = Some(project_id);
                model.selected_session_id = None;
            },
            cx,
        );
    }

    pub fn create_group(
        &mut self,
        parent_project_id: String,
        name: String,
        cx: &mut Context<Self>,
    ) {
        self.run_action(
            move |client| {
                client.create_group(WorkersCreateGroupRequest {
                    parent_project_id,
                    name,
                })
            },
            |model, project_id| {
                model.expanded_project_ids.insert(project_id.clone());
                model.selected_project_id = Some(project_id);
                model.selected_session_id = None;
            },
            cx,
        );
    }

    pub fn create_worktree(
        &mut self,
        project_id: String,
        branch: String,
        name: Option<String>,
        cx: &mut Context<Self>,
    ) {
        let parent_id = project_id.clone();
        self.run_action(
            move |client| {
                client.create_worktree(WorkersCreateWorktreeRequest {
                    project_id,
                    branch,
                    name,
                    base_ref: None,
                })
            },
            move |model, worktree| {
                model.expanded_project_ids.insert(parent_id);
                model.selected_project_id = Some(worktree.project_id);
                model.selected_session_id = None;
            },
            cx,
        );
    }

    pub fn create_worktree_and_launch(
        &mut self,
        project_id: String,
        task_name: String,
        branch: String,
        preset_id: Option<String>,
        cx: &mut Context<Self>,
    ) {
        let parent_id = project_id.clone();
        self.run_action(
            move |client| {
                let launch = match preset_id {
                    Some(preset_id) => WorkersLaunchRequest::preset(project_id.clone(), preset_id),
                    None => WorkersLaunchRequest::terminal(project_id.clone()),
                };
                client.create_worktree_and_launch(
                    WorkersCreateWorktreeRequest {
                        project_id,
                        branch,
                        name: Some(task_name),
                        base_ref: None,
                    },
                    launch,
                )
            },
            move |model, result| {
                model.expanded_project_ids.insert(parent_id);
                model.expanded_project_ids.insert(result.project_id.clone());
                model.selected_project_id = Some(result.project_id);
                model.selected_session_id = Some(result.session_id);
            },
            cx,
        );
    }

    pub fn rename_project(&mut self, project_id: String, name: String, cx: &mut Context<Self>) {
        self.run_unit_action(
            move |client| {
                client.set_project_organization(
                    &project_id,
                    WorkersProjectOrganizationPatch {
                        display_name: Some(name),
                        ..Default::default()
                    },
                )
            },
            cx,
        );
    }

    pub fn set_project_color(
        &mut self,
        project_id: String,
        color_id: Option<String>,
        cx: &mut Context<Self>,
    ) {
        self.run_unit_action(
            move |client| {
                client.set_project_organization(
                    &project_id,
                    WorkersProjectOrganizationPatch {
                        folder_color_id: Some(color_id),
                        ..Default::default()
                    },
                )
            },
            cx,
        );
    }

    pub fn set_project_session_sort(
        &mut self,
        project_id: String,
        session_sort: WorkersSessionSort,
        cx: &mut Context<Self>,
    ) {
        self.run_unit_action(
            move |client| {
                client.set_project_organization(
                    &project_id,
                    WorkersProjectOrganizationPatch {
                        session_sort: Some(session_sort),
                        ..Default::default()
                    },
                )
            },
            cx,
        );
    }

    pub fn stop_all(&mut self, sessions: Vec<WorkersSession>, cx: &mut Context<Self>) {
        self.run_unit_action(
            move |client| {
                for session in sessions.into_iter().filter(WorkersSession::is_live) {
                    client.session_action(&session.id, SessionAction::Stop)?;
                }
                Ok(())
            },
            cx,
        );
    }

    pub fn remove_project(&mut self, project: WorkersProject, cx: &mut Context<Self>) {
        let selected_id = project.id.clone();
        self.run_action(
            move |client| {
                if project.worktree_branch.is_some() {
                    client.remove_worktree(&project.id, false)
                } else if project.parent_project_id.is_some() {
                    client.remove_group(&project.id)
                } else {
                    client.remove_project(&project.id)
                }
            },
            move |model, ()| {
                model.expanded_project_ids.remove(&selected_id);
                if model.selected_project_id.as_deref() == Some(&selected_id) {
                    model.selected_project_id = None;
                    model.selected_session_id = None;
                }
            },
            cx,
        );
    }

    pub fn request_remove_project(&mut self, project: WorkersProject, cx: &mut Context<Self>) {
        self.confirming_remove_project = Some(project);
        cx.notify();
    }

    pub fn cancel_remove_project(&mut self, cx: &mut Context<Self>) {
        self.confirming_remove_project = None;
        cx.notify();
    }

    pub fn confirm_remove_project(&mut self, cx: &mut Context<Self>) {
        let Some(project) = self.confirming_remove_project.take() else {
            return;
        };
        self.remove_project(project, cx);
    }

    pub fn reveal_project(&mut self, path: String, cx: &mut Context<Self>) {
        self.run_unit_action(move |client| client.reveal_project(&path), cx);
    }

    pub fn open_project_in_editor(&mut self, path: String, cx: &mut Context<Self>) {
        self.run_unit_action(move |client| client.open_project_in_editor(&path), cx);
    }

    pub fn add_preset(&mut self, label: String, command: String, cx: &mut Context<Self>) {
        self.run_settings_action(move |client| client.add_preset(&label, &command), cx);
    }

    pub fn update_preset(&mut self, id: String, patch: PresetPatch, cx: &mut Context<Self>) {
        self.run_settings_action(move |client| client.update_preset(&id, patch), cx);
    }

    pub fn delete_preset(&mut self, id: String, cx: &mut Context<Self>) {
        self.run_settings_action(move |client| client.delete_preset(&id), cx);
    }

    pub fn move_preset(&mut self, id: String, index: usize, cx: &mut Context<Self>) {
        self.run_settings_action(move |client| client.move_preset(&id, index), cx);
    }

    pub fn set_transcript_settings(
        &mut self,
        settings: WorkersTranscriptSettings,
        cx: &mut Context<Self>,
    ) {
        self.run_settings_action(move |client| client.set_transcript_settings(settings), cx);
    }

    pub fn set_notification_settings(
        &mut self,
        settings: WorkersNotificationSettings,
        cx: &mut Context<Self>,
    ) {
        self.run_settings_action(move |client| client.set_notification_settings(settings), cx);
    }

    pub fn test_notification(&self) {
        crate::notify::post(
            "Workers notification",
            "Notifications from active CLI workers are enabled.",
        );
        crate::sound::play(crate::sound::Sound::Done);
    }

    pub fn stop(&mut self, session_id: String, cx: &mut Context<Self>) {
        self.run_unit_action(
            move |client| client.session_action(&session_id, SessionAction::Stop),
            cx,
        );
    }

    pub fn restart(&mut self, session_id: String, cx: &mut Context<Self>) {
        self.prepare_replacement(&session_id);
        self.run_unit_action(
            move |client| client.session_action(&session_id, SessionAction::Restart),
            cx,
        );
    }

    pub fn resume_agent(&mut self, session_id: String, cx: &mut Context<Self>) {
        self.run_unit_action(
            move |client| client.session_action(&session_id, SessionAction::ResumeAgent),
            cx,
        );
    }

    pub fn fork(&mut self, session: WorkersSession, cx: &mut Context<Self>) {
        let project_id = session.project_id.clone();
        self.run_action(
            move |client| client.fork_session(&session),
            move |model, session_id| {
                model.route = WorkersRoute::Workspace;
                model.selected_project_id = Some(project_id.clone());
                model.selected_session_id = Some(session_id.clone());
                model.expanded_project_ids.insert(project_id);
                model.pending_launch_selection = Some(PendingLaunchSelection {
                    session_id,
                    remaining_refreshes: 12,
                });
            },
            cx,
        );
    }

    pub fn clear_attention(&mut self, session_id: String, cx: &mut Context<Self>) {
        self.run_unit_action(move |client| client.clear_attention(&session_id), cx);
    }

    pub fn move_session(
        &mut self,
        session_id: String,
        project_id: Option<String>,
        cx: &mut Context<Self>,
    ) {
        self.run_unit_action(
            move |client| client.move_session(&session_id, project_id.as_deref()),
            cx,
        );
    }

    pub fn append_system_context(
        &mut self,
        session_id: String,
        context: String,
        cx: &mut Context<Self>,
    ) {
        self.run_unit_action(
            move |client| client.append_system_context(&session_id, Some(&context)),
            cx,
        );
    }

    pub fn set_notify_when_done(
        &mut self,
        session_id: String,
        enabled: bool,
        cx: &mut Context<Self>,
    ) {
        self.organize(
            session_id,
            SessionOrganizationPatch {
                notify_when_done: Some(enabled),
                ..Default::default()
            },
            cx,
        );
    }

    pub fn remove(&mut self, session_id: String, cx: &mut Context<Self>) {
        self.run_unit_action(
            move |client| client.session_action(&session_id, SessionAction::Remove),
            cx,
        );
    }

    pub fn request_remove(&mut self, session_id: String, archived: bool, cx: &mut Context<Self>) {
        self.confirming_remove_session_id = Some(session_id);
        self.confirming_remove_archived = archived;
        cx.notify();
    }

    pub fn cancel_remove(&mut self, cx: &mut Context<Self>) {
        self.confirming_remove_session_id = None;
        self.confirming_remove_archived = false;
        cx.notify();
    }

    pub fn confirm_remove(&mut self, cx: &mut Context<Self>) {
        let Some(session_id) = self.confirming_remove_session_id.take() else {
            return;
        };
        let archived = std::mem::take(&mut self.confirming_remove_archived);
        let request = PendingRemove {
            session_id,
            archived,
        };
        if let Some(request) = dispatch_or_queue_remove(
            self.action_task.is_some(),
            &mut self.pending_remove,
            request,
        ) {
            self.dispatch_remove(request, cx);
        }
        cx.notify();
    }

    pub fn pin(&mut self, session_id: String, pinned: bool, cx: &mut Context<Self>) {
        self.organize(
            session_id,
            SessionOrganizationPatch {
                pinned: Some(pinned),
                ..Default::default()
            },
            cx,
        );
    }

    pub fn archive(&mut self, session_id: String, archived: bool, cx: &mut Context<Self>) {
        self.organize(
            session_id,
            SessionOrganizationPatch {
                archived: Some(archived),
                ..Default::default()
            },
            cx,
        );
    }

    pub fn stop_and_archive(&mut self, session_id: String, live: bool, cx: &mut Context<Self>) {
        self.run_unit_action(
            move |client| {
                if live {
                    client.session_action(&session_id, SessionAction::Stop)?;
                }
                client.set_session_organization(
                    &session_id,
                    SessionOrganizationPatch {
                        archived: Some(true),
                        ..Default::default()
                    },
                )
            },
            cx,
        );
    }

    pub fn rename(&mut self, session_id: String, title: String, cx: &mut Context<Self>) {
        self.organize(
            session_id,
            SessionOrganizationPatch {
                title: Some(title),
                ..Default::default()
            },
            cx,
        );
    }

    pub fn open_archive(&mut self, project_id: String, cx: &mut Context<Self>) {
        if self.archive_project_id.as_deref() != Some(project_id.as_str()) {
            self.archived_sessions.clear();
        }
        self.archive_project_id = Some(project_id.clone());
        self.archive_loading = true;
        self.archive_error = None;
        self.archive_generation = self.archive_generation.wrapping_add(1);
        let generation = self.archive_generation;
        let client = self.client.clone();
        self.archive_task = Some(cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { client.archived_sessions(&project_id) })
                .await;
            this.update(cx, |model, cx| {
                if generation != model.archive_generation {
                    return;
                }
                model.archive_task = None;
                model.archive_loading = false;
                match result {
                    Ok(sessions) => {
                        model.archived_sessions = archived_sessions_for_project(sessions);
                        model.archive_error = None;
                    }
                    Err(error) => model.archive_error = Some(error.to_string()),
                }
                cx.notify();
            })
            .ok();
        }));
        cx.notify();
    }

    pub fn close_archive(&mut self, cx: &mut Context<Self>) {
        self.reset_archive_view();
        cx.notify();
    }

    fn reset_archive_view(&mut self) {
        self.archive_generation = self.archive_generation.wrapping_add(1);
        self.archive_task = None;
        self.archive_project_id = None;
        self.archive_loading = false;
        self.archive_error = None;
    }

    pub fn restore(&mut self, session: WorkersSession, resume: bool, cx: &mut Context<Self>) {
        if resume {
            self.prepare_replacement_for(&session);
        }
        let session_id = session.id.clone();
        let completed_id = session.id.clone();
        self.run_action(
            move |client| {
                client.set_session_organization(
                    &session_id,
                    SessionOrganizationPatch {
                        archived: Some(false),
                        ..Default::default()
                    },
                )?;
                if resume {
                    if let Err(error) = client.session_action(&session_id, restore_action(&session))
                    {
                        let _ = client.set_session_organization(
                            &session_id,
                            SessionOrganizationPatch {
                                archived: Some(true),
                                ..Default::default()
                            },
                        );
                        return Err(error);
                    }
                }
                Ok(())
            },
            move |model, ()| {
                model
                    .archived_sessions
                    .retain(|session| session.id != completed_id);
            },
            cx,
        );
    }

    pub fn remove_archived(&mut self, session_id: String, cx: &mut Context<Self>) {
        let completed_id = session_id.clone();
        self.run_action(
            move |client| client.session_action(&session_id, SessionAction::Remove),
            move |model, ()| {
                model
                    .archived_sessions
                    .retain(|session| session.id != completed_id);
            },
            cx,
        );
    }

    fn organize(
        &mut self,
        session_id: String,
        patch: SessionOrganizationPatch,
        cx: &mut Context<Self>,
    ) {
        self.run_unit_action(
            move |client| client.set_session_organization(&session_id, patch),
            cx,
        );
    }

    fn prepare_replacement(&mut self, source_id: &str) {
        let source = self
            .sessions()
            .iter()
            .find(|session| session.id == source_id)
            .cloned();
        if let Some(source) = source {
            self.prepare_replacement_for(&source);
        }
    }

    fn dispatch_remove(&mut self, request: PendingRemove, cx: &mut Context<Self>) {
        if request.archived {
            self.remove_archived(request.session_id, cx);
        } else {
            self.remove(request.session_id, cx);
        }
    }

    fn start_pending_remove(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(request) = self.pending_remove.take() else {
            return false;
        };
        self.dispatch_remove(request, cx);
        true
    }

    fn prepare_replacement_for(&mut self, source: &WorkersSession) {
        self.pending_replacement = Some(PendingReplacement {
            source_id: source.id.clone(),
            project_id: source.project_id.clone(),
            command: source.command.clone(),
            provider_id: source.provider_id.clone(),
            worktree_branch: source.worktree_branch.clone(),
            source_created_at_unix_ms: source.created_at_unix_ms,
            baseline_ids: self
                .sessions()
                .iter()
                .map(|session| session.id.clone())
                .collect(),
            remaining_refreshes: 8,
        });
    }

    fn apply_snapshot(&mut self, snapshot: WorkersBootstrap, app_focused: bool) {
        let notification_settings = self
            .settings
            .as_ref()
            .map(|settings| settings.notifications.clone())
            .unwrap_or_default();
        for session in &snapshot.sessions {
            let previous = self
                .notification_state
                .get(&session.id)
                .map(|(activity, unread)| (activity.as_str(), *unread));
            if let Some(notification) =
                notification_transition(previous, &session.activity, session.unread)
            {
                let (title, body, sound) = match notification {
                    WorkerNotification::Attention => (
                        "Worker needs attention",
                        session.title.as_str(),
                        crate::sound::Sound::Request,
                    ),
                    WorkerNotification::Done => (
                        "Worker finished",
                        session.title.as_str(),
                        crate::sound::Sound::Done,
                    ),
                };
                let allowed = !matches!(notification, WorkerNotification::Attention)
                    || notification_settings.menu_attention_detection;
                if allowed
                    && notification_settings.desktop_notifications
                    && (!notification_settings.background_only || !app_focused)
                {
                    crate::notify::post(title, body);
                }
                if allowed && notification_settings.sound_enabled {
                    crate::sound::play(sound);
                }
            }
        }
        self.notification_state = snapshot
            .sessions
            .iter()
            .map(|session| {
                (
                    session.id.clone(),
                    (session.activity.clone(), session.unread),
                )
            })
            .collect();
        if let Some(mut pending) = self.pending_replacement.take() {
            if snapshot
                .sessions
                .iter()
                .any(|session| session.id == pending.source_id)
                && pending.remaining_refreshes > 0
            {
                pending.remaining_refreshes -= 1;
                self.selected_session_id = Some(pending.source_id.clone());
                self.pending_replacement = Some(pending);
            } else {
                match replacement_selection(&pending, &snapshot.sessions) {
                    Some(selection) => self.selected_session_id = selection,
                    None if pending.remaining_refreshes > 0 => {
                        pending.remaining_refreshes -= 1;
                        self.selected_session_id = None;
                        self.pending_replacement = Some(pending);
                    }
                    None => self.selected_session_id = None,
                }
            }
        } else if let Some(mut pending) = self.pending_launch_selection.take() {
            let visible = snapshot
                .sessions
                .iter()
                .any(|session| session.id == pending.session_id);
            self.selected_session_id = reconcile_selection_with_pending(
                self.selected_session_id.as_deref(),
                Some(pending.session_id.as_str()),
                &snapshot.sessions,
            );
            if !visible && pending.remaining_refreshes > 0 {
                pending.remaining_refreshes -= 1;
                self.pending_launch_selection = Some(pending);
            }
        } else {
            self.selected_session_id =
                reconcile_selection(self.selected_session_id.as_deref(), &snapshot.sessions);
        }
        self.selected_project_id = self
            .selected_session_id
            .as_deref()
            .and_then(|selected| {
                snapshot
                    .sessions
                    .iter()
                    .find(|session| session.id == selected)
            })
            .map(|session| session.project_id.clone())
            .or_else(|| {
                self.selected_project_id
                    .as_ref()
                    .filter(|selected| snapshot.projects.iter().any(|p| &p.id == *selected))
                    .cloned()
            })
            .or_else(|| {
                snapshot
                    .projects
                    .iter()
                    .find(|project| !project.is_group)
                    .map(|project| project.id.clone())
            });
        if !self.initialized_expansion {
            self.expanded_project_ids
                .extend(snapshot.projects.iter().map(|project| project.id.clone()));
            self.initialized_expansion = true;
        } else {
            self.expanded_project_ids
                .retain(|id| snapshot.projects.iter().any(|project| &project.id == id));
        }
        self.error = None;
        self.snapshot = Some(snapshot);
    }

    fn run_unit_action(
        &mut self,
        operation: impl FnOnce(LocalWorkersClient) -> Result<(), zeron_workers_unpeel::WorkersError>
        + Send
        + 'static,
        cx: &mut Context<Self>,
    ) {
        self.run_action(operation, |_, ()| {}, cx);
    }

    fn run_action<T: Send + 'static>(
        &mut self,
        operation: impl FnOnce(LocalWorkersClient) -> Result<T, zeron_workers_unpeel::WorkersError>
        + Send
        + 'static,
        apply: impl FnOnce(&mut Self, T) + Send + 'static,
        cx: &mut Context<Self>,
    ) {
        if self.action_task.is_some() {
            return;
        }
        let client = self.client.clone();
        self.action_task = Some(cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { operation(client) })
                .await;
            this.update(cx, |model, cx| {
                model.action_task = None;
                match result {
                    Ok(value) => {
                        apply(model, value);
                    }
                    Err(error) => {
                        model.pending_replacement = None;
                        model.error = Some(error.to_string());
                    }
                }
                if !model.start_pending_remove(cx) {
                    model.refresh(cx);
                }
                cx.notify();
            })
            .ok();
        }));
    }

    fn run_settings_action<T: Send + 'static>(
        &mut self,
        operation: impl FnOnce(LocalWorkersClient) -> Result<T, zeron_workers_unpeel::WorkersError>
        + Send
        + 'static,
        cx: &mut Context<Self>,
    ) {
        if self.action_task.is_some() {
            return;
        }
        let client = self.client.clone();
        self.action_task = Some(cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { operation(client) })
                .await;
            this.update(cx, |model, cx| {
                model.action_task = None;
                match result {
                    Ok(_) if !model.start_pending_remove(cx) => model.refresh_settings(cx),
                    Ok(_) => {}
                    Err(error) => model.settings_error = Some(error.to_string()),
                }
                if model.action_task.is_none() {
                    model.start_pending_remove(cx);
                }
                cx.notify();
            })
            .ok();
        }));
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use zeron_workers_unpeel::{WorkersSession, WorkersSessionCapabilities};

    use super::{
        PendingRemove, PendingReplacement, WorkerNotification, dispatch_or_queue_remove,
        notification_transition, reconcile_selection, reconcile_selection_with_pending,
        replacement_selection, sessions_for_project, toggle_expanded,
    };

    #[test]
    fn remove_is_queued_instead_of_dropped_while_another_action_finishes() {
        let request = PendingRemove {
            session_id: "session-1".into(),
            archived: false,
        };
        let mut queued = None;

        let immediate = dispatch_or_queue_remove(true, &mut queued, request.clone());

        assert_eq!(immediate, None);
        assert_eq!(queued, Some(request));
    }

    fn session(id: &str, project_id: &str, live: bool) -> WorkersSession {
        WorkersSession {
            id: id.to_owned(),
            project_id: project_id.to_owned(),
            title: id.to_owned(),
            command: "zsh".to_owned(),
            state: if live { "running" } else { "exited" }.to_owned(),
            activity: if live { "working" } else { "idle" }.to_owned(),
            unread: false,
            pinned: false,
            archived: false,
            provider_id: None,
            active_runtime_id: None,
            runtime_launch_pending: false,
            notify_when_done: false,
            terminal_background_hex: None,
            worktree_branch: None,
            created_at_unix_ms: 1,
            updated_at_unix_ms: 1,
            capabilities: WorkersSessionCapabilities::default(),
        }
    }

    #[test]
    fn selection_stays_stable_and_clears_when_the_session_disappears() {
        let sessions = vec![
            session("exited", "project", false),
            session("live", "project", true),
        ];
        assert_eq!(
            reconcile_selection(Some("exited"), &sessions).as_deref(),
            Some("exited")
        );
        assert_eq!(
            reconcile_selection(Some("missing"), &sessions).as_deref(),
            None
        );
        assert_eq!(reconcile_selection(None, &sessions), None);
    }

    #[test]
    fn project_launcher_selection_never_falls_back_to_a_session_from_another_project() {
        let sessions = vec![session("old", "project-a", true)];

        assert_eq!(reconcile_selection(None, &sessions), None);
    }

    #[test]
    fn pending_launch_selection_survives_until_the_new_session_reaches_the_snapshot() {
        let existing = vec![session("old", "project", true)];
        assert_eq!(
            reconcile_selection_with_pending(Some("new"), Some("new"), &existing).as_deref(),
            Some("new")
        );

        let visible = vec![
            session("old", "project", true),
            session("new", "project", true),
        ];
        assert_eq!(
            reconcile_selection_with_pending(Some("new"), Some("new"), &visible).as_deref(),
            Some("new")
        );
    }

    #[test]
    fn notification_edges_are_authoritative_and_deduplicated() {
        assert_eq!(
            notification_transition(Some(("working", false)), "blocked", false),
            Some(WorkerNotification::Attention)
        );
        assert_eq!(
            notification_transition(Some(("blocked", false)), "blocked", false),
            None
        );
        assert_eq!(
            notification_transition(Some(("working", false)), "done", true),
            Some(WorkerNotification::Done)
        );
        assert_eq!(
            notification_transition(Some(("done", true)), "done", true),
            None
        );
    }

    #[test]
    fn project_grouping_preserves_upstream_session_order() {
        let sessions = vec![
            session("a", "one", true),
            session("b", "two", true),
            session("c", "one", false),
        ];
        let ids: Vec<_> = sessions_for_project(&sessions, "one")
            .into_iter()
            .map(|session| session.id.as_str())
            .collect();
        assert_eq!(ids, ["a", "c"]);
    }

    #[test]
    fn expanded_projects_toggle_without_touching_other_rows() {
        let mut expanded = HashSet::from(["one".to_owned(), "two".to_owned()]);
        toggle_expanded(&mut expanded, "one");
        assert_eq!(expanded, HashSet::from(["two".to_owned()]));
        toggle_expanded(&mut expanded, "three");
        assert_eq!(
            expanded,
            HashSet::from(["two".to_owned(), "three".to_owned()])
        );
    }

    #[test]
    fn replacement_adopts_only_one_new_live_session_in_the_same_project() {
        let pending = PendingReplacement {
            source_id: "old".to_owned(),
            project_id: "one".to_owned(),
            command: "zsh".to_owned(),
            provider_id: None,
            worktree_branch: None,
            source_created_at_unix_ms: 1,
            baseline_ids: HashSet::from(["other".to_owned()]),
            remaining_refreshes: 8,
        };
        assert_eq!(
            replacement_selection(
                &pending,
                &[session("other", "two", true), session("new", "one", true)]
            ),
            Some(Some("new".to_owned()))
        );
        assert_eq!(
            replacement_selection(
                &pending,
                &[session("new-a", "one", true), session("new-b", "one", true)]
            ),
            Some(None)
        );
        let mut decoy = session("decoy", "one", true);
        decoy.command = "claude".to_owned();
        assert_eq!(replacement_selection(&pending, &[decoy]), None);
    }
}
