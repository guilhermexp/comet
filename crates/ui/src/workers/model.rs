use std::collections::HashSet;
use std::time::Duration;

use gpui::{Context, Task};
use zeron_workers_unpeel::{
    LocalWorkersClient, SessionAction, SessionOrganizationPatch, WorkersBootstrap, WorkersProject,
    WorkersSession,
};

pub fn reconcile_selection(current: Option<&str>, sessions: &[WorkersSession]) -> Option<String> {
    if let Some(current) = current
        && sessions.iter().any(|session| session.id == current)
    {
        return Some(current.to_owned());
    }
    sessions
        .iter()
        .find(|session| session.is_live())
        .or_else(|| sessions.first())
        .map(|session| session.id.clone())
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

pub struct WorkersModel {
    client: LocalWorkersClient,
    pub snapshot: Option<WorkersBootstrap>,
    pub selected_project_id: Option<String>,
    pub selected_session_id: Option<String>,
    pub expanded_project_ids: HashSet<String>,
    pub loading: bool,
    pub error: Option<String>,
    initialized_expansion: bool,
    refresh_generation: u64,
    refresh_task: Option<Task<()>>,
    action_task: Option<Task<()>>,
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
            initialized_expansion: false,
            refresh_generation: 0,
            refresh_task: None,
            action_task: None,
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
        self.selected_project_id = Some(session.project_id.clone());
        self.selected_session_id = Some(session_id);
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
        self.selected_project_id = Some(project_id);
        self.selected_session_id = None;
        cx.notify();
    }

    pub fn toggle_project(&mut self, project_id: &str, cx: &mut Context<Self>) {
        toggle_expanded(&mut self.expanded_project_ids, project_id);
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
                    Ok(snapshot) => model.apply_snapshot(snapshot),
                    Err(error) => model.error = Some(error.to_string()),
                }
                cx.notify();
            })
            .ok();
        }));
    }

    pub fn launch(&mut self, project_id: String, command: String, cx: &mut Context<Self>) {
        self.run_action(
            move |client| client.create_session(&project_id, &command),
            |model, session_id| model.selected_session_id = Some(session_id),
            cx,
        );
    }

    pub fn stop(&mut self, session_id: String, cx: &mut Context<Self>) {
        self.run_unit_action(
            move |client| client.session_action(&session_id, SessionAction::Stop),
            cx,
        );
    }

    pub fn restart(&mut self, session_id: String, cx: &mut Context<Self>) {
        self.run_unit_action(
            move |client| client.session_action(&session_id, SessionAction::Restart),
            cx,
        );
    }

    pub fn remove(&mut self, session_id: String, cx: &mut Context<Self>) {
        self.run_unit_action(
            move |client| client.session_action(&session_id, SessionAction::Remove),
            cx,
        );
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

    fn apply_snapshot(&mut self, snapshot: WorkersBootstrap) {
        self.selected_session_id =
            reconcile_selection(self.selected_session_id.as_deref(), &snapshot.sessions);
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
            .or_else(|| snapshot.projects.first().map(|project| project.id.clone()));
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
                        model.refresh(cx);
                    }
                    Err(error) => model.error = Some(error.to_string()),
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

    use super::{reconcile_selection, sessions_for_project, toggle_expanded};

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
            worktree_branch: None,
            created_at_unix_ms: 1,
            updated_at_unix_ms: 1,
            capabilities: WorkersSessionCapabilities::default(),
        }
    }

    #[test]
    fn selection_stays_stable_then_falls_back_to_the_first_live_session() {
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
            Some("live")
        );
        assert_eq!(
            reconcile_selection(None, &sessions).as_deref(),
            Some("live")
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
}
