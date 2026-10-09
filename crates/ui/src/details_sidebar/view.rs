use std::collections::{BTreeSet, HashMap};

use serde::{Deserialize, Serialize};

use super::context::DetailsContext;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct DetailsSidebarPreferences {
    /// Per-context collapse markers (`:`-prefixed keys, e.g.
    /// `:projects_worked:collapsed`).
    pub expanded: HashMap<String, Vec<String>>,
    pub idle_recaps: HashMap<String, super::idle_recap::IdleRecapEntry>,
    pub idle_recap_enabled: bool,
    pub idle_recap_delay_seconds: u64,
    pub hidden_widgets: BTreeSet<String>,
}

impl Default for DetailsSidebarPreferences {
    fn default() -> Self {
        Self {
            expanded: HashMap::new(),
            idle_recaps: HashMap::new(),
            idle_recap_enabled: true,
            idle_recap_delay_seconds: super::idle_recap::IDLE_RECAP_DEFAULT_SECONDS,
            hidden_widgets: BTreeSet::new(),
        }
    }
}

/// The Details-tab widget cards the user can show/hide from the header gear
/// (`SETTINGS_MINIMALISTIC`), in render order: `(stable id, menu label, icon)`.
/// The id matches the `widget_card` id in [`DetailsSidebar::render_details`]
/// and the key persisted in [`DetailsSidebarPreferences::hidden_widgets`].
pub const TOGGLEABLE_WIDGETS: &[(&str, &str, &str)] = &[
    ("workspace-widget", "Workspace", icons::DETAILS_BOX),
    ("chat-workers-widget", "Workers", icons::DETAILS_WORKERS),
    ("todos-widget", "To-dos", icons::CHECKLIST),
    ("usage-widget", "Usage", icons::DETAILS_GAUGE),
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkedProjectsCacheKey {
    pub context_key: String,
    pub transcript_len: usize,
    /// Total tool/text parts across the transcript.
    ///
    /// Entry count alone is NOT enough: a streaming turn grows the SAME entry,
    /// and `TranscriptFrame::Delta` upserts it by removing and reinserting at
    /// the same id (`zeron_doc::apply_transcript_frame`), so `len()` is
    /// unchanged while a new `ReadFile`/`Exec` part lands. Without this the
    /// card freezes for the whole turn — exactly when it is being watched.
    pub parts_total: usize,
    pub projects_revision: u64,
}

#[derive(Debug, Clone)]
struct WorkedProjectsCache {
    key: WorkedProjectsCacheKey,
    result: Vec<super::worked_projects::WorkedProject>,
}

pub fn projects_snapshot_fingerprint(projects: &[zeron_workers_unpeel::WorkersProject]) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    for p in projects {
        p.id.hash(&mut hasher);
        p.name.hash(&mut hasher);
        p.path.hash(&mut hasher);
        p.is_group.hash(&mut hasher);
    }
    hasher.finish()
}

#[derive(Debug, Clone)]
pub struct DetailsSidebarState {
    context: Option<DetailsContext>,
    preferences: DetailsSidebarPreferences,
    load_generation: u64,
    worked_projects_cache: Option<WorkedProjectsCache>,
}

impl DetailsSidebarState {
    pub fn new(mut preferences: DetailsSidebarPreferences) -> Self {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;
        super::idle_recap::prune_idle_recaps(&mut preferences.idle_recaps, now);
        // Builds that had a Files tree also stored its expanded file paths here.
        for markers in preferences.expanded.values_mut() {
            markers.retain(|marker| marker.starts_with(':'));
        }
        preferences
            .expanded
            .retain(|_, markers| !markers.is_empty());
        Self {
            context: None,
            preferences,
            load_generation: 0,
            worked_projects_cache: None,
        }
    }

    pub fn context(&self) -> Option<&DetailsContext> {
        self.context.as_ref()
    }

    pub fn set_context(&mut self, context: Option<DetailsContext>) -> u64 {
        if self.context.as_ref() != context.as_ref() {
            self.load_generation = self.load_generation.wrapping_add(1);
            self.context = context;
        }
        self.load_generation
    }

    pub fn projects_worked_collapsed(&self) -> bool {
        let Some(context) = &self.context else {
            return false;
        };
        self.preferences
            .expanded
            .get(&context.key)
            .map(|paths| paths.iter().any(|p| p == ":projects_worked:collapsed"))
            .unwrap_or(false)
    }

    pub fn toggle_projects_worked_collapsed(&mut self) {
        let Some(context) = &self.context else {
            return;
        };
        let paths = self
            .preferences
            .expanded
            .entry(context.key.clone())
            .or_default();
        const KEY: &str = ":projects_worked:collapsed";
        if let Some(index) = paths.iter().position(|path| path == KEY) {
            paths.remove(index);
        } else {
            paths.push(KEY.to_string());
            paths.sort();
        }
    }

    pub fn idle_recap_for(&self, context_key: &str) -> Option<&super::idle_recap::IdleRecapEntry> {
        self.preferences.idle_recaps.get(context_key)
    }

    pub fn set_idle_recap(
        &mut self,
        context_key: String,
        entry: super::idle_recap::IdleRecapEntry,
    ) {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;
        self.preferences.idle_recaps.insert(context_key, entry);
        super::idle_recap::prune_idle_recaps(&mut self.preferences.idle_recaps, now);
    }

    pub fn clear_idle_recap(&mut self, context_key: &str) {
        self.preferences.idle_recaps.remove(context_key);
    }

    pub fn widget_hidden(&self, widget_id: &str) -> bool {
        self.preferences.hidden_widgets.contains(widget_id)
    }

    pub fn toggle_widget_hidden(&mut self, widget_id: &str) {
        if !self.preferences.hidden_widgets.remove(widget_id) {
            self.preferences
                .hidden_widgets
                .insert(widget_id.to_string());
        }
    }

    pub fn preferences(&self) -> DetailsSidebarPreferences {
        self.preferences.clone()
    }

    pub fn load_generation(&self) -> u64 {
        self.load_generation
    }

    pub fn accept_load(&self, generation: u64, context_key: &str) -> bool {
        self.load_generation == generation
            && self.context.as_ref().map(|context| context.key.as_str()) == Some(context_key)
    }

    pub fn worked_projects(
        &mut self,
        context: &DetailsContext,
        transcript: &[zeron_doc::SessionMessageEntry],
        projects: &[zeron_workers_unpeel::WorkersProject],
        home_dir: Option<&std::path::Path>,
    ) -> Vec<super::worked_projects::WorkedProject> {
        let key = WorkedProjectsCacheKey {
            context_key: context.key.clone(),
            transcript_len: transcript.len(),
            parts_total: transcript.iter().map(|entry| entry.parts.len()).sum(),
            projects_revision: projects_snapshot_fingerprint(projects),
        };
        if self.worked_projects_cache.as_ref().map(|c| &c.key) != Some(&key) {
            let result = super::worked_projects::worked_projects(
                transcript,
                projects,
                &context.cwd,
                home_dir,
            );
            self.worked_projects_cache = Some(WorkedProjectsCache { key, result });
        }
        self.worked_projects_cache.as_ref().unwrap().result.clone()
    }

    pub fn worked_projects_cache_key(&self) -> Option<&WorkedProjectsCacheKey> {
        self.worked_projects_cache.as_ref().map(|c| &c.key)
    }
}

use gpui::{
    AnyElement, App, AppContext as _, Context, Entity, EventEmitter, Image, IntoElement, ObjectFit,
    Render, SharedString, Subscription, Task, div, img, prelude::*, px,
};
use zeron_proto::{
    AgentAccountsSnapshot, CheckoutFilesRequest, CheckoutOpRequest, CheckoutStatus,
    CheckoutStatusFile, CommitCheckoutRequest, GetCheckoutStatusRequest,
    WatchCheckoutStatusRequest,
    agent::{WorkflowProgressNode, WorkflowTaskStatus},
};
use zeron_rpc::methods;

#[path = "session_card.rs"]
mod session_card;

use crate::{
    composer::{Composer, ComposerInput, ComposerInputEvent},
    details_sidebar::{
        chat_workers::{
            ChatActivityRow, ChatWorkerRow, ChatWorkersSnapshot, SubagentFinishedGroup,
            SubagentPageState, WorkerLaunchAge, WorkerSemantic, activity_tasks_from_entries,
            compact_activity_label, format_token_total, group_subagents, project_chat_workers,
            running_subagent_count, snapshot_is_active, worker_compact_metadata, worker_launch_age,
        },
        context::detect_git_branch,
        source_control,
        subagent_avatars::blobatar_subagent_avatar_path,
        todos::{latest_todos, todo_status_layout, todo_viewport_height_px},
        usage::{
            ProviderUsageRow, ProviderUsageState, UsageTone, provider_usage_rows,
            usage_provider_icon,
        },
        widgets::{
            ActionTooltip, CHAT_WORKERS_ROW_HEIGHT, ChatWorkersTab, ChatWorkersWidgetState,
            TabActivity, chat_workers_viewport_height_px, widget_card, worker_expansion_key,
            workers_tab_presence,
        },
    },
    icons,
    pickers::Pickers,
    popover,
    state::AppState,
    theme::Theme,
    workers::{
        model::WorkersModel,
        presentation::{runtime_icon_path, session_title_truncate},
    },
};

const USAGE_TICK: std::time::Duration = std::time::Duration::from_secs(30);
const USAGE_FETCH_INTERVAL: std::time::Duration = std::time::Duration::from_secs(120);

fn subagent_section_heading(label: &'static str, count: usize, theme: &Theme) -> gpui::Div {
    div()
        .w_full()
        .px(px(9.0))
        .pt(px(8.0))
        .pb(px(4.0))
        .text_size(px(10.0))
        .font_weight(gpui::FontWeight::MEDIUM)
        .text_color(theme.text_muted)
        .child(format!("{label} · {count}"))
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
struct FinishedSubagentsDisclosure {
    expanded: bool,
}

impl FinishedSubagentsDisclosure {
    fn toggle(&mut self) {
        self.expanded = !self.expanded;
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ContextFileAccess {
    Local,
    Remote,
    WaitingForDevice,
}

fn context_file_access(
    context: &DetailsContext,
    local_device_id: Option<&str>,
) -> ContextFileAccess {
    match (context.target_device_id.as_deref(), local_device_id) {
        (None, _) => ContextFileAccess::Local,
        (Some(target), Some(local)) if target == local => ContextFileAccess::Local,
        (Some(_), Some(_)) => ContextFileAccess::Remote,
        (Some(_), None) => ContextFileAccess::WaitingForDevice,
    }
}

fn details_sidebar_background(_theme: &Theme) -> Option<gpui::Hsla> {
    None
}

fn dirs_home() -> Option<std::path::PathBuf> {
    std::env::var_os("HOME").map(std::path::PathBuf::from)
}

#[derive(Debug, Clone)]
enum LoadState<T> {
    Idle,
    Loading,
    Ready(T),
    Error(SharedString),
}

#[derive(Debug, Clone)]
pub enum DetailsSidebarEvent {
    Close,
    PreferencesChanged(DetailsSidebarPreferences),
    OpenFile {
        context_key: String,
        root: std::path::PathBuf,
        relative_path: String,
        remote_target: Option<(zeron_proto::WorkspaceTarget, String)>,
    },
    OpenSubagent {
        chat_id: String,
        doc_id: String,
        title: String,
        frozen: bool,
    },
    OpenWorkerSession {
        chat_id: String,
        session_id: String,
        title: String,
    },
    OpenWorkingTreeDiff {
        path: String,
    },
}

#[derive(Clone, Copy)]
enum SourceControlSection {
    Staged,
    Changes,
}

fn open_subagent_event(chat_id: &str, row: &ChatActivityRow) -> DetailsSidebarEvent {
    DetailsSidebarEvent::OpenSubagent {
        chat_id: chat_id.to_owned(),
        doc_id: row.id.clone(),
        title: row.title.clone(),
        frozen: row.status != WorkflowTaskStatus::Running,
    }
}

fn subagent_row_avatar_path(row_id: &str) -> &'static str {
    blobatar_subagent_avatar_path(row_id)
}

/// Settled-success badge for activity and worker rows: a ringed check, not a
/// bare glyph. The loose checkmark read as punctuation next to the row's
/// avatar, while every other terminal state in the same column already
/// carries a ring (`CLOSE_CIRCLE`) — the ring is what makes "done" land as a
/// status instead of a tick. Muted, not green: a finished row is settled
/// information, not a call to attention.
fn settled_success_badge(theme: &Theme) -> AnyElement {
    div()
        .size(px(15.0))
        .flex_none()
        .rounded_full()
        .border_1()
        .border_color(theme.text_muted)
        .flex()
        .items_center()
        .justify_center()
        .child(
            icons::icon(icons::CHECK)
                .size(px(9.0))
                .text_color(theme.text_muted),
        )
        .into_any_element()
}

fn open_worker_event(chat_id: &str, worker: &ChatWorkerRow) -> DetailsSidebarEvent {
    DetailsSidebarEvent::OpenWorkerSession {
        chat_id: chat_id.to_owned(),
        session_id: worker.session_id.clone(),
        title: worker.title.clone(),
    }
}

fn worker_click_event(
    event: DetailsSidebarEvent,
    _still_available_in_sidebar_snapshot: bool,
) -> DetailsSidebarEvent {
    // The sidebar snapshot is advisory. Shell owns the final lookup against
    // the latest WorkersModel snapshot and refreshes when this identity raced
    // with session removal.
    event
}

pub struct DetailsSidebar {
    app_state: Entity<AppState>,
    workers_model: Entity<WorkersModel>,
    pickers: Entity<Pickers>,
    /// Read-only, for one question: does the chat being shown have an unsent
    /// draft? A draft is what tells the idle recap the user is not idle.
    composer: Entity<Composer>,
    sidebar: DetailsSidebarState,
    chat_workers: ChatWorkersWidgetState,
    subagent_pages_chat: Option<String>,
    subagent_pages: [SubagentPageState; 3],
    finished_subagents: FinishedSubagentsDisclosure,
    usage: LoadState<Vec<ProviderUsageRow>>,
    usage_snapshot: Option<AgentAccountsSnapshot>,
    usage_fetched_at: Option<std::time::Instant>,
    widgets_menu: popover::Popup<()>,
    usage_expanded: std::collections::HashSet<String>,
    material_icons: std::collections::HashMap<SharedString, std::sync::Arc<Image>>,
    resolved_branch: Option<String>,
    /// Memoized `<cwd>/.git` probe: without it the Workspace widget stats the
    /// disk on every render. Re-probed when the context's cwd changes.
    /// ponytail: a `git init` under an unchanged context is not noticed until
    /// the sidebar switches context; add an fs watch if that ever matters.
    has_git_dir: Option<(std::path::PathBuf, bool)>,
    branch_task: Option<Task<()>>,
    usage_task: Option<Task<()>>,
    usage_tick: Option<Task<()>>,
    recap_task: Option<Task<()>>,
    recap_armed_epoch: Option<(String, usize)>,
    failed_epochs: std::collections::HashMap<String, usize>,
    _state_observe: Subscription,
    _workers_observe: Subscription,
    _pickers_observe: Subscription,
    _composer_observe: Subscription,
    checkout_status: Option<CheckoutStatus>,
    checkout_not_git: bool,
    checkout_status_error: Option<SharedString>,
    source_control_watch: Option<Task<()>>,
    source_control_watch_key: Option<String>,
    source_control_fetch: Option<Task<()>>,
    source_control_op: Option<Task<()>>,
    source_control_busy: bool,
    commit_generating: bool,
    commit_revision: u64,
    source_control_collapsed: [bool; 2],
    source_control_op_error: Option<SharedString>,
    commit_input: Entity<ComposerInput>,
    _commit_events: Subscription,
    discard_prompt: Option<source_control::DiscardPrompt>,
    session_diff: Option<session_card::SessionDiffTotals>,
    session_diff_watch: Option<Task<()>>,
    session_diff_key: Option<String>,
    context_sources: Option<session_card::ContextSources>,
    context_sources_task: Option<Task<()>>,
    context_sources_pending: Option<String>,
    context_sources_expanded: bool,
    context_sources_scroll: gpui::UniformListScrollHandle,
    turn_stats_collapsed: bool,
}

impl DetailsSidebar {
    pub fn new(
        app_state: Entity<AppState>,
        workers_model: Entity<WorkersModel>,
        preferences: DetailsSidebarPreferences,
        pickers: Entity<Pickers>,
        composer: Entity<Composer>,
        cx: &mut Context<Self>,
    ) -> Self {
        let commit_input = cx.new(|cx| {
            ComposerInput::new("Message (Enter to commit)", cx).with_text_metrics(12.0, 18.0)
        });
        let commit_events = cx.subscribe(&commit_input, |this, _, event, cx| {
            if matches!(event, ComposerInputEvent::Edited) {
                this.commit_revision = this.commit_revision.wrapping_add(1);
                cx.notify();
            }
            if matches!(event, ComposerInputEvent::Submitted) {
                this.commit_checkout(cx);
            }
        });
        let state_observe = cx.observe(&app_state, |this, state, cx| {
            let engine_connected = state.read(cx).engine().is_some();
            if engine_connected && matches!(this.usage, LoadState::Idle | LoadState::Error(_)) {
                this.load_usage(cx);
            }
            this.sync_idle_recap(cx);
            this.ensure_source_control_watch(cx);
            cx.notify();
        });
        let workers_observe = cx.observe(&workers_model, |_, _, cx| cx.notify());
        // Every keystroke reaches the composer's own notify (`on_input_edited`),
        // so this is where the draft gate learns the user stopped being idle.
        // Re-arming is idempotent for an unchanged epoch, so the repeat is free
        // and no repaint is requested here — `sync_idle_recap` notifies itself
        // when it actually changes something.
        let composer_observe = cx.observe(&composer, |this, _, cx| this.sync_idle_recap(cx));
        let pickers_observe = cx.observe(&pickers, |this, p, cx| {
            if let Some(target) = &p.read(cx).active_repo_target {
                if let Some(branch) = &target.branch {
                    if this.resolved_branch.as_ref() != Some(branch) {
                        this.resolved_branch = Some(branch.clone());
                    }
                }
            }
            cx.notify();
        });
        let mut sidebar = Self {
            app_state,
            workers_model,
            pickers,
            composer,
            sidebar: DetailsSidebarState::new(preferences),
            chat_workers: ChatWorkersWidgetState::default(),
            subagent_pages_chat: None,
            subagent_pages: Default::default(),
            finished_subagents: FinishedSubagentsDisclosure::default(),
            usage: LoadState::Idle,
            usage_snapshot: None,
            usage_fetched_at: None,
            widgets_menu: popover::Popup::default(),
            usage_expanded: std::collections::HashSet::new(),
            material_icons: std::collections::HashMap::new(),
            resolved_branch: None,
            has_git_dir: None,
            checkout_status: None,
            checkout_not_git: false,
            checkout_status_error: None,
            source_control_watch: None,
            source_control_watch_key: None,
            source_control_fetch: None,
            source_control_op: None,
            source_control_busy: false,
            commit_generating: false,
            commit_revision: 0,
            source_control_collapsed: [false; 2],
            source_control_op_error: None,
            commit_input,
            _commit_events: commit_events,
            discard_prompt: None,
            session_diff: None,
            session_diff_watch: None,
            session_diff_key: None,
            context_sources: None,
            context_sources_task: None,
            context_sources_pending: None,
            context_sources_expanded: false,
            context_sources_scroll: gpui::UniformListScrollHandle::new(),
            turn_stats_collapsed: true,
            branch_task: None,
            usage_task: None,
            usage_tick: None,
            recap_task: None,
            recap_armed_epoch: None,
            failed_epochs: std::collections::HashMap::new(),
            _state_observe: state_observe,
            _workers_observe: workers_observe,
            _pickers_observe: pickers_observe,
            _composer_observe: composer_observe,
        };
        sidebar.load_usage(cx);
        sidebar.ensure_usage_tick(cx);
        sidebar
    }

    pub fn set_context(&mut self, context: Option<DetailsContext>, cx: &mut Context<Self>) {
        let before = self.sidebar.load_generation();
        let after = self.sidebar.set_context(context);
        if before != after {
            self.chat_workers
                .sync_context(self.sidebar.context().map(|context| context.key.as_str()));
            self.source_control_watch = None;
            self.source_control_watch_key = None;
            self.source_control_fetch = None;
            self.source_control_op = None;
            self.checkout_status = None;
            self.checkout_not_git = false;
            self.checkout_status_error = None;
            self.source_control_op_error = None;
            self.source_control_busy = false;
            self.commit_generating = false;
            self.commit_revision = self.commit_revision.wrapping_add(1);
            self.source_control_collapsed = [false; 2];
            self.discard_prompt = None;
            self.commit_input
                .update(cx, |input, cx| input.set_text("", cx));
            self.resolved_branch = self
                .sidebar
                .context()
                .and_then(|value| value.branch.clone());
            self.load_branch(cx);
            self.ensure_source_control_watch(cx);
            cx.notify();
            self.sync_idle_recap(cx);
        }
    }

    pub fn preferences(&self) -> DetailsSidebarPreferences {
        self.sidebar.preferences()
    }

    pub fn render_shell_header(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let theme = Theme::of(cx).clone();
        self.render_header(&theme, cx).into_any_element()
    }

    fn emit_preferences(&self, cx: &mut Context<Self>) {
        cx.emit(DetailsSidebarEvent::PreferencesChanged(self.preferences()));
    }

    fn close_widgets_menu(&mut self, cx: &mut Context<Self>) {
        if self.widgets_menu.begin_close() {
            popover::reap_popup(cx, |this: &mut Self| &mut this.widgets_menu);
            cx.notify();
        }
    }

    fn render_widgets_gear(&mut self, theme: &Theme, cx: &mut Context<Self>) -> AnyElement {
        let mounted = self.widgets_menu.get().is_some();
        let closing = self.widgets_menu.closing_since();
        let mut trigger = div()
            .id("details-widgets-menu-toggle")
            .relative()
            .size(px(28.0))
            .rounded(px(6.0))
            .flex()
            .items_center()
            .justify_center()
            .cursor_pointer()
            .hover(|style| style.bg(crate::theme::ink(0.05)))
            .when(mounted, |el| el.bg(crate::theme::ink(0.05)))
            .on_mouse_down(
                gpui::MouseButton::Left,
                cx.listener(|this, _, _, _| this.widgets_menu.note_trigger_press()),
            )
            .on_click(cx.listener(|this, _, _, cx| {
                if !this.widgets_menu.take_press_was_open() {
                    this.widgets_menu.open(());
                    cx.notify();
                }
            }))
            .child(
                icons::icon(icons::SETTINGS_MINIMALISTIC)
                    .size(px(16.0))
                    .text_color(theme.text_muted),
            );
        if mounted {
            let menu = self.render_widgets_menu(theme, cx);
            trigger = trigger.child(popover::anchored_menu_below_end(
                "details-widgets-menu",
                menu,
                closing,
            ));
        }
        trigger.into_any_element()
    }

    fn render_widgets_menu(&mut self, theme: &Theme, cx: &mut Context<Self>) -> AnyElement {
        let rows: Vec<AnyElement> = TOGGLEABLE_WIDGETS
            .iter()
            .enumerate()
            .map(|(ix, entry)| {
                let (widget_id, label, icon_path) = *entry;
                let visible = !self.sidebar.widget_hidden(widget_id);
                popover::menu_row(theme, false, format!("details-widget-row-{ix}"))
                    .id(("details-widget-row", ix))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.sidebar.toggle_widget_hidden(widget_id);
                        this.emit_preferences(cx);
                        cx.notify();
                    }))
                    .child(
                        icons::icon(icon_path)
                            .size(px(15.0))
                            .flex_none()
                            .text_color(theme.text_muted.opacity(0.8)),
                    )
                    .child(div().flex_1().child(SharedString::from(label)))
                    .child(div().w(px(14.0)).flex_none().when(visible, |el| {
                        el.child(
                            icons::icon(icons::CHECK)
                                .size(px(14.0))
                                .text_color(theme.text_muted),
                        )
                    }))
                    .into_any_element()
            })
            .collect();
        popover::popover_card(theme)
            .w(px(200.0))
            .on_mouse_down_out(cx.listener(|this, _, _, cx| this.close_widgets_menu(cx)))
            .flex()
            .flex_col()
            .gap(px(2.0))
            .child(popover::menu_heading(theme, "Widgets"))
            .children(rows)
            .into_any_element()
    }

    fn source_control_cwd(&self) -> Option<(String, Option<String>)> {
        let context = self.sidebar.context()?;
        Some((
            context.cwd.to_string_lossy().into_owned(),
            context.target_device_id.clone(),
        ))
    }

    fn source_control_params(
        &self,
        request: impl serde::Serialize,
    ) -> Option<(String, Option<String>, serde_json::Value)> {
        let (cwd, device) = self.source_control_cwd()?;
        let mut params = serde_json::to_value(request).ok()?;
        if let Some(device) = &device {
            params["targetDeviceId"] = device.clone().into();
        }
        Some((cwd, device, params))
    }

    fn is_not_git_error(error: &zeron_rpc::RpcError) -> bool {
        error.to_string().to_lowercase().contains("not a git")
    }

    fn ensure_source_control_watch(&mut self, cx: &mut Context<Self>) {
        let Some(context) = self.sidebar.context().cloned() else {
            self.source_control_watch = None;
            self.source_control_watch_key = None;
            self.checkout_status = None;
            return;
        };
        let key = context.key.clone();
        if self.source_control_watch_key.as_ref() == Some(&key) {
            return;
        }
        let local_device = self.app_state.read(cx).local_device_id.clone();
        if context_file_access(&context, local_device.as_deref()) == ContextFileAccess::Local
            && !self.git_dir_exists(&context.cwd)
        {
            self.source_control_watch_key = Some(key);
            self.source_control_watch = None;
            self.checkout_status = None;
            self.checkout_not_git = true;
            self.checkout_status_error = None;
            cx.notify();
            return;
        }
        let Some(engine) = self.app_state.read(cx).engine().cloned() else {
            return;
        };
        self.source_control_watch_key = Some(key.clone());
        self.checkout_not_git = false;
        self.refresh_checkout_status(cx);
        let cwd = context.cwd.to_string_lossy().into_owned();
        let device = context.target_device_id.clone();
        self.source_control_watch = Some(cx.spawn(async move |this, cx| {
            loop {
                let request = WatchCheckoutStatusRequest { cwd: cwd.clone() };
                let mut params = serde_json::to_value(&request).unwrap_or(serde_json::json!({}));
                if let Some(device) = &device {
                    params["targetDeviceId"] = device.clone().into();
                }
                match engine
                    .client()
                    .subscribe(methods::WATCH_CHECKOUT_STATUS, params)
                    .await
                {
                    Err(zeron_rpc::RpcError::UnknownMethod(_)) => {
                        let _ = this.update(cx, |this, cx| {
                            this.checkout_status_error =
                                Some("Update the project device to enable Source Control.".into());
                            cx.notify();
                        });
                        return;
                    }
                    Err(error) => {
                        let not_git = Self::is_not_git_error(&error);
                        let message = format!("{error}");
                        let stop = this
                            .update(cx, |this, cx| {
                                if this.sidebar.context().map(|c| c.key.as_str())
                                    != Some(key.as_str())
                                {
                                    return true;
                                }
                                if not_git {
                                    this.checkout_not_git = true;
                                    this.checkout_status = None;
                                    this.checkout_status_error = None;
                                } else {
                                    this.checkout_status_error = Some(message.into());
                                }
                                cx.notify();
                                not_git
                            })
                            .unwrap_or(true);
                        if stop {
                            return;
                        }
                        cx.background_executor()
                            .timer(std::time::Duration::from_secs(2))
                            .await;
                    }
                    Ok(mut stream) => {
                        while let Some(value) = stream.recv().await {
                            let Ok(status) = serde_json::from_value::<CheckoutStatus>(value) else {
                                continue;
                            };
                            let _ = this.update(cx, |this, cx| {
                                if this.sidebar.context().map(|c| c.key.as_str())
                                    != Some(key.as_str())
                                {
                                    return;
                                }
                                this.checkout_not_git = false;
                                this.checkout_status = Some(status);
                                this.checkout_status_error = None;
                                cx.notify();
                            });
                        }
                    }
                }
            }
        }));
    }

    fn refresh_checkout_status(&mut self, cx: &mut Context<Self>) {
        let Some(context) = self.sidebar.context().cloned() else {
            return;
        };
        let Some(engine) = self.app_state.read(cx).engine().cloned() else {
            return;
        };
        let key = context.key.clone();
        let request = GetCheckoutStatusRequest {
            cwd: context.cwd.to_string_lossy().into_owned(),
        };
        let mut params = serde_json::to_value(&request).unwrap_or(serde_json::json!({}));
        if let Some(device) = &context.target_device_id {
            params["targetDeviceId"] = device.clone().into();
        }
        self.source_control_fetch = Some(cx.spawn(async move |this, cx| {
            let result = engine
                .client()
                .call_as::<CheckoutStatus>(methods::GET_CHECKOUT_STATUS, params)
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.sidebar.context().map(|c| c.key.as_str()) != Some(key.as_str()) {
                    return;
                }
                match result {
                    Ok(status) => {
                        this.checkout_not_git = false;
                        this.checkout_status = Some(status);
                        this.checkout_status_error = None;
                    }
                    Err(zeron_rpc::RpcError::UnknownMethod(_)) => {
                        this.checkout_status_error =
                            Some("Update the project device to enable Source Control.".into());
                    }
                    Err(error) if Self::is_not_git_error(&error) => {
                        this.checkout_not_git = true;
                        this.checkout_status = None;
                        this.checkout_status_error = None;
                    }
                    Err(error) => {
                        this.checkout_status_error = Some(format!("{error}").into());
                    }
                }
                cx.notify();
            });
        }));
    }

    fn execute_source_control(
        &mut self,
        method: &'static str,
        params: serde_json::Value,
        clear_commit: bool,
        cx: &mut Context<Self>,
    ) {
        if self.source_control_busy {
            return;
        }
        let Some(engine) = self.app_state.read(cx).engine().cloned() else {
            return;
        };
        let key = self.sidebar.context().map(|context| context.key.clone());
        self.source_control_busy = true;
        self.source_control_op_error = None;
        cx.notify();
        self.source_control_op = Some(cx.spawn(async move |this, cx| {
            let result = engine.client().call(method, params).await;
            let _ = this.update(cx, |this, cx| {
                if this.sidebar.context().map(|context| context.key.clone()) != key {
                    return;
                }
                this.source_control_busy = false;
                match result {
                    Ok(_) => {
                        this.source_control_op_error = None;
                        if clear_commit {
                            this.commit_input
                                .update(cx, |input, cx| input.set_text("", cx));
                        }
                    }
                    Err(error) => {
                        this.source_control_op_error = Some(format!("{error}").into());
                    }
                }
                this.refresh_checkout_status(cx);
                cx.notify();
            });
        }));
    }

    fn stage_paths(&mut self, paths: Vec<String>, cx: &mut Context<Self>) {
        let Some((cwd, _)) = self.source_control_cwd() else {
            return;
        };
        let Some((_, _, params)) = self.source_control_params(CheckoutFilesRequest { cwd, paths })
        else {
            return;
        };
        self.execute_source_control(methods::STAGE_FILES, params, false, cx);
    }

    fn unstage_paths(&mut self, paths: Vec<String>, cx: &mut Context<Self>) {
        let Some((cwd, _)) = self.source_control_cwd() else {
            return;
        };
        let Some((_, _, params)) = self.source_control_params(CheckoutFilesRequest { cwd, paths })
        else {
            return;
        };
        self.execute_source_control(methods::UNSTAGE_FILES, params, false, cx);
    }

    fn discard_paths(&mut self, paths: Vec<String>, cx: &mut Context<Self>) {
        let Some((cwd, _)) = self.source_control_cwd() else {
            return;
        };
        let request = CheckoutFilesRequest { cwd, paths };
        let Some((_, _, params)) = self.source_control_params(request) else {
            return;
        };
        self.execute_source_control(methods::DISCARD_FILES, params, false, cx);
    }

    fn generate_commit_message(&mut self, cx: &mut Context<Self>) {
        if self.source_control_busy
            || !self
                .checkout_status
                .as_ref()
                .is_some_and(|status| status.files.iter().any(source_control::is_staged))
        {
            return;
        }
        let Some(engine) = self.app_state.read(cx).engine().cloned() else {
            return;
        };
        let Some((cwd, _)) = self.source_control_cwd() else {
            return;
        };
        let Some((_, _, params)) =
            self.source_control_params(zeron_proto::GenerateCommitMessageRequest { cwd })
        else {
            return;
        };
        let key = self.sidebar.context().map(|context| context.key.clone());
        let revision = self.commit_revision;
        self.source_control_busy = true;
        self.commit_generating = true;
        self.source_control_op_error = None;
        cx.notify();
        self.source_control_op = Some(cx.spawn(async move |this, cx| {
            let result = engine.client().call(methods::GENERATE_COMMIT_MESSAGE, params).await
                .and_then(|value| serde_json::from_value::<zeron_proto::GeneratedCommitMessage>(value)
                    .map_err(|error| zeron_rpc::RpcError::Failed(error.to_string())));
            let _ = this.update(cx, |this, cx| {
                let current_key = this.sidebar.context().map(|context| context.key.clone());
                if current_key != key { return; }
                this.source_control_busy = false;
                this.commit_generating = false;
                match result {
                    Ok(result) if source_control::can_apply_generated_message(key.as_deref(), current_key.as_deref(), revision, this.commit_revision) => {
                        this.commit_input.update(cx, |input, cx| input.set_text(result.message, cx));
                    }
                    Ok(_) => {
                        this.source_control_op_error = Some("Message changed while generating; your edits were kept. Generate again to replace them.".into());
                    }
                    Err(zeron_rpc::RpcError::UnknownMethod(_)) => {
                        this.source_control_op_error = Some("Update the project device to generate commit messages.".into());
                    }
                    Err(error) => { this.source_control_op_error = Some(error.to_string().into()); }
                }
                cx.notify();
            });
        }));
    }

    fn commit_checkout(&mut self, cx: &mut Context<Self>) {
        if self.source_control_busy {
            return;
        }
        let message = self.commit_input.read(cx).text().to_string();
        if !self
            .checkout_status
            .as_ref()
            .is_some_and(|status| source_control::can_commit(&message, &status.files))
        {
            return;
        }

        let Some((cwd, _)) = self.source_control_cwd() else {
            return;
        };
        let message = self.commit_input.read(cx).text().to_string();
        let request = CommitCheckoutRequest { cwd, message };
        let Some((_, _, params)) = self.source_control_params(request) else {
            return;
        };
        self.execute_source_control(methods::COMMIT_CHECKOUT, params, true, cx);
    }

    fn sync_or_publish_checkout(&mut self, cx: &mut Context<Self>) {
        let Some((cwd, _)) = self.source_control_cwd() else {
            return;
        };
        let publish = self
            .checkout_status
            .as_ref()
            .and_then(|status| status.upstream.as_ref())
            .is_none();
        let request = CheckoutOpRequest { cwd };
        let Some((_, _, params)) = self.source_control_params(request) else {
            return;
        };
        let method = if publish {
            methods::PUSH_CHECKOUT
        } else {
            methods::SYNC_CHECKOUT
        };
        self.execute_source_control(method, params, false, cx);
    }

    fn request_discard(&mut self, files: Vec<CheckoutStatusFile>, cx: &mut Context<Self>) {
        if files.is_empty() {
            return;
        }
        self.discard_prompt = Some(source_control::discard_prompt(&files));
        cx.notify();
    }

    fn confirm_discard_prompt(&mut self, accepted: bool, cx: &mut Context<Self>) {
        let Some(prompt) = self.discard_prompt.take() else {
            return;
        };
        if accepted {
            self.discard_paths(prompt.paths, cx);
        }
        cx.notify();
    }

    pub(crate) fn render_source_control(
        &mut self,
        theme: &Theme,
        cx: &mut Context<Self>,
    ) -> gpui::Div {
        self.ensure_source_control_watch(cx);
        let mut content = div().w_full().min_w_0().flex().flex_col().pb(px(12.0));
        if self.sidebar.context().is_none() {
            return content.child(
                div()
                    .text_size(px(12.0))
                    .text_color(theme.text_muted)
                    .child("No workspace selected."),
            );
        }
        if self.checkout_not_git {
            return content.child(
                div()
                    .text_size(px(12.0))
                    .text_color(theme.text_muted)
                    .child("This folder is not a git repository."),
            );
        }
        if let Some(error) = &self.checkout_status_error {
            content = content.child(
                div()
                    .text_size(px(12.0))
                    .text_color(theme.danger)
                    .child(error.clone()),
            );
        }
        let Some(status) = self.checkout_status.clone() else {
            return content.child(
                div()
                    .text_size(px(12.0))
                    .text_color(theme.text_muted)
                    .child("Loading source control…"),
            );
        };
        if let Some(error) = &self.source_control_op_error {
            content = content.child(
                div()
                    .text_size(px(12.0))
                    .text_color(theme.danger)
                    .child(error.clone()),
            );
        }
        let message = self.commit_input.read(cx).text().to_string();
        let can_commit =
            source_control::can_commit(&message, &status.files) && !self.source_control_busy;
        let sync_label = source_control::sync_button_label(status.upstream.as_deref());
        let busy = self.source_control_busy;
        let branch = if status.branch.is_empty() {
            "Detached".to_string()
        } else {
            status.branch.clone()
        };
        let header = div()
            .flex()
            .items_center()
            .gap(px(6.0))
            .text_size(px(11.0))
            .text_color(theme.text_muted)
            .child(
                icons::icon(icons::GIT_BRANCH)
                    .size(px(13.0))
                    .text_color(theme.text_muted),
            )
            .child(div().min_w_0().truncate().child(branch));
        let can_generate = !busy && status.files.iter().any(source_control::is_staged);
        let mut generate_btn = div()
            .id("details-generate-commit-message")
            .size(px(20.0))
            .flex_none()
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(5.0))
            .bg(theme.text.opacity(0.06))
            .role(gpui::Role::Button)
            .aria_label("Generate commit message")
            .tooltip(|_, cx| {
                cx.new(|_| ActionTooltip {
                    label: "Generate commit message".into(),
                })
                .into()
            })
            .opacity(if can_generate || self.commit_generating {
                1.0
            } else {
                0.4
            });
        if self.commit_generating {
            generate_btn = generate_btn.child(crate::loaders::mini_mono_spinner(
                "commit-message-generation",
                2.0,
                theme.text_muted,
                cx.entity_id(),
                cx,
            ));
        } else {
            generate_btn = generate_btn.child(
                icons::icon(icons::THOUGHT_SPARKLE)
                    .size(px(13.0))
                    .text_color(theme.text_muted),
            );
        }
        if can_generate {
            generate_btn = generate_btn
                .cursor_pointer()
                .hover(|style| style.bg(theme.text.opacity(0.12)))
                .on_click(cx.listener(|this, _, _, cx| this.generate_commit_message(cx)));
        }
        let mut commit_btn = div()
            .id("details-source-control-commit")
            .w_full()
            .h(px(26.0))
            .flex_none()
            .flex()
            .items_center()
            .justify_center()
            .gap(px(6.0))
            .rounded(px(6.0))
            .bg(theme.text)
            .text_color(theme.bg)
            .text_size(px(12.0))
            .font_weight(gpui::FontWeight::SEMIBOLD)
            .opacity(if can_commit { 1.0 } else { 0.4 })
            .role(gpui::Role::Button)
            .aria_label("Commit staged changes")
            .child(
                icons::icon(icons::CHECK)
                    .size(px(13.0))
                    .text_color(theme.bg),
            )
            .child("Commit");
        if can_commit {
            commit_btn = commit_btn
                .cursor_pointer()
                .on_click(cx.listener(|this, _, _, cx| this.commit_checkout(cx)));
        }
        let mut sync_btn = div()
            .id("details-source-control-sync")
            .w_full()
            .h(px(26.0))
            .flex_none()
            .flex()
            .items_center()
            .justify_center()
            .gap(px(6.0))
            .rounded(px(6.0))
            .bg(theme.text.opacity(0.08))
            .text_size(px(12.0))
            .text_color(theme.text)
            .font_weight(gpui::FontWeight::SEMIBOLD)
            .role(gpui::Role::Button)
            .aria_label(sync_label)
            .opacity(if busy { 0.4 } else { 1.0 })
            .child(
                icons::icon(if status.upstream.is_some() {
                    icons::REFRESH
                } else {
                    icons::ARROW_UP
                })
                .size(px(13.0))
                .text_color(theme.text),
            )
            .child(sync_label);
        if status.behind > 0 {
            sync_btn = sync_btn.child(
                div()
                    .text_color(theme.text_muted)
                    .child(format!("↓{}", status.behind)),
            );
        }
        if status.ahead > 0 {
            sync_btn = sync_btn.child(
                div()
                    .text_color(theme.text_muted)
                    .child(format!("↑{}", status.ahead)),
            );
        }
        if !busy {
            sync_btn = sync_btn
                .cursor_pointer()
                .hover(|s| s.bg(theme.text.opacity(0.12)))
                .on_click(cx.listener(|this, _, _, cx| this.sync_or_publish_checkout(cx)));
        }
        content = content.child(
            div()
                .w_full()
                .flex_none()
                .flex()
                .flex_col()
                .gap(px(6.0))
                .p(px(8.0))
                .pb(px(10.0))
                .border_b_1()
                .border_color(theme.border)
                .child(header)
                .child(
                    div()
                        .w_full()
                        .min_w_0()
                        .h(px((message.lines().count().max(1) as f32 * 18.0 + 10.0)
                            .clamp(28.0, 118.0)))
                        .flex_none()
                        .px(px(8.0))
                        .rounded(px(6.0))
                        .bg(theme.text.opacity(0.05))
                        .flex()
                        .items_center()
                        .gap(px(6.0))
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .h_full()
                                .child(self.commit_input.clone()),
                        )
                        .child(generate_btn),
                )
                .child(commit_btn)
                .child(sync_btn),
        );
        let staged = source_control::staged_rows(&status.files);
        let changes = source_control::changes_rows(&status.files);
        let staged_files: Vec<CheckoutStatusFile> = status
            .files
            .iter()
            .filter(|file| source_control::is_staged(file))
            .cloned()
            .collect();
        let change_files: Vec<CheckoutStatusFile> = status
            .files
            .iter()
            .filter(|file| source_control::is_unstaged(file))
            .cloned()
            .collect();
        if !staged.is_empty() {
            content = content.child(self.render_source_control_section(
                theme,
                "Staged Changes",
                &staged,
                &staged_files,
                SourceControlSection::Staged,
                cx,
            ));
        }
        content = content.child(self.render_source_control_section(
            theme,
            "Changes",
            &changes,
            &change_files,
            SourceControlSection::Changes,
            cx,
        ));
        content
    }

    fn render_source_control_section(
        &mut self,
        theme: &Theme,
        title: &'static str,
        rows: &[source_control::SourceControlRow],
        files: &[CheckoutStatusFile],
        section: SourceControlSection,
        cx: &mut Context<Self>,
    ) -> gpui::Div {
        let slot = match section {
            SourceControlSection::Staged => 0,
            SourceControlSection::Changes => 1,
        };
        let tag = if slot == 0 { "staged" } else { "changes" };
        let collapsed = self.source_control_collapsed[slot];
        let busy = self.source_control_busy;
        let paths: Vec<String> = rows.iter().map(|row| row.path.clone()).collect();
        let bulk_files = files.to_vec();
        let mut actions = div().flex().items_center().flex_none();
        if !busy {
            if slot == 1 {
                actions = actions.child(self.toolbar_button_with_tooltip(
                    "source-control-refresh",
                    icons::REFRESH,
                    "Refresh Changes",
                    theme,
                    cx.listener(|this, _, _, cx| {
                        cx.stop_propagation();
                        this.refresh_checkout_status(cx);
                    }),
                ));
            }
            if !rows.is_empty() {
                actions = actions.child(self.toolbar_button_with_tooltip(
                    if slot == 0 {
                        "source-control-discard-staged"
                    } else {
                        "source-control-discard-changes"
                    },
                    icons::RESTART,
                    "Discard Changes…",
                    theme,
                    cx.listener(move |this, _, _, cx| {
                        cx.stop_propagation();
                        this.request_discard(bulk_files.clone(), cx);
                    }),
                ));
                actions = actions.child(self.toolbar_button_with_tooltip(
                    if slot == 0 {
                        "source-control-unstage-all"
                    } else {
                        "source-control-stage-all"
                    },
                    if slot == 0 { icons::MINUS } else { icons::PLUS },
                    if slot == 0 {
                        "Unstage All Changes"
                    } else {
                        "Stage All Changes"
                    },
                    theme,
                    cx.listener(move |this, _, _, cx| {
                        cx.stop_propagation();
                        match section {
                            SourceControlSection::Staged => this.unstage_paths(paths.clone(), cx),
                            SourceControlSection::Changes => this.stage_paths(paths.clone(), cx),
                        }
                    }),
                ));
            }
        }
        let header = div()
            .id(("source-control-section", slot))
            .h(px(27.0))
            .w_full()
            .flex_none()
            .px(px(8.0))
            .flex()
            .items_center()
            .gap(px(6.0))
            .cursor_pointer()
            .on_click(cx.listener(move |this, _, _, cx| {
                this.source_control_collapsed[slot] = !this.source_control_collapsed[slot];
                cx.notify();
            }))
            .child(
                icons::icon(if collapsed {
                    icons::ALT_ARROW_RIGHT
                } else {
                    icons::ALT_ARROW_DOWN
                })
                .size(px(12.0))
                .text_color(theme.text_muted),
            )
            .child(
                div()
                    .text_size(px(10.0))
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .text_color(theme.text_muted)
                    .child(title.to_uppercase()),
            )
            .child(
                div()
                    .min_w(px(18.0))
                    .h(px(18.0))
                    .px(px(5.0))
                    .rounded_full()
                    .flex()
                    .items_center()
                    .justify_center()
                    .bg(gpui::rgb(0x387dcc))
                    .text_size(px(10.0))
                    .text_color(gpui::rgb(0xffffff))
                    .child(rows.len().to_string()),
            )
            .child(div().flex_1())
            .child(actions);
        let mut list = div()
            .w_full()
            .min_w_0()
            .flex_none()
            .flex()
            .flex_col()
            .child(header);
        if collapsed {
            return list;
        }
        if rows.is_empty() {
            return list.child(
                div()
                    .px(px(26.0))
                    .py(px(6.0))
                    .text_size(px(12.0))
                    .text_color(theme.text_muted)
                    .child("No changes"),
            );
        }
        for (index, row) in rows.iter().enumerate() {
            let path = row.path.clone();
            let (directory, name) = path
                .trim_end_matches('/')
                .rsplit_once('/')
                .unwrap_or(("", path.trim_end_matches('/')));
            let tooltip: SharedString = match &row.old_path {
                Some(old) => format!("{old} → {}", row.path).into(),
                None => row.path.clone().into(),
            };
            let glyph = self.material_icon(crate::material_icons::material_icon_path(
                name,
                path.ends_with('/'),
                false,
            ));
            let group: SharedString = format!("source-control-{tag}-{index}").into();
            let status_color = match row.letter {
                'M' => theme.warning,
                'A' => theme.success,
                'D' | '!' => theme.danger,
                'U' | 'R' | 'C' => match theme.appearance {
                    crate::theme::Appearance::Dark => gpui::rgb(0x29c5f6).into(),
                    crate::theme::Appearance::Light => gpui::rgb(0x007eaa).into(),
                },
                _ => theme.text_muted,
            };
            let mut row_actions = div()
                .flex_none()
                .flex()
                .items_center()
                .invisible()
                .group_hover(group.clone(), |s| s.visible());
            if !busy {
                if let Some(file) = files.iter().find(|file| file.path == row.path).cloned() {
                    row_actions = row_actions.child(
                        self.toolbar_button_with_tooltip(
                            "source-control-row-discard",
                            icons::RESTART,
                            "Discard Changes…",
                            theme,
                            cx.listener(move |this, _, _, cx| {
                                cx.stop_propagation();
                                this.request_discard(vec![file.clone()], cx);
                            }),
                        )
                        .size(px(22.0)),
                    );
                }
                let action_path = path.clone();
                row_actions = row_actions.child(
                    self.toolbar_button_with_tooltip(
                        "source-control-row-stage",
                        if slot == 0 { icons::MINUS } else { icons::PLUS },
                        if slot == 0 {
                            "Unstage Changes"
                        } else {
                            "Stage Changes"
                        },
                        theme,
                        cx.listener(move |this, _, _, cx| {
                            cx.stop_propagation();
                            match section {
                                SourceControlSection::Staged => {
                                    this.unstage_paths(vec![action_path.clone()], cx)
                                }
                                SourceControlSection::Changes => {
                                    this.stage_paths(vec![action_path.clone()], cx)
                                }
                            }
                        }),
                    )
                    .size(px(22.0)),
                );
            }
            let open_path = path.clone();
            let row_el = div()
                .id((
                    SharedString::from(format!("source-control-row-{tag}")),
                    index,
                ))
                .group(group)
                .h(px(27.0))
                .flex_none()
                .w_full()
                .min_w_0()
                .pl(px(12.0))
                .pr(px(10.0))
                .flex()
                .items_center()
                .gap(px(7.0))
                .cursor_pointer()
                .hover(|s| s.bg(theme.element_hover))
                .tooltip(move |_, cx| {
                    cx.new(|_| ActionTooltip {
                        label: tooltip.clone(),
                    })
                    .into()
                })
                .on_click(cx.listener(move |_, _, _, cx| {
                    cx.emit(DetailsSidebarEvent::OpenWorkingTreeDiff {
                        path: open_path.clone(),
                    });
                }))
                .child(glyph)
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .overflow_hidden()
                        .flex()
                        .items_center()
                        .gap(px(6.0))
                        .child(
                            div()
                                .min_w_0()
                                .flex_shrink(1.0)
                                .truncate()
                                .text_size(px(11.5))
                                .text_color(theme.text)
                                .child(name.to_string()),
                        )
                        .when(!directory.is_empty(), |el| {
                            el.child(
                                div()
                                    .min_w_0()
                                    .flex_1()
                                    .truncate()
                                    .text_size(px(10.0))
                                    .text_color(theme.text_muted)
                                    .child(directory.to_string()),
                            )
                        }),
                )
                .child(row_actions)
                .child(
                    div()
                        .w(px(12.0))
                        .flex_none()
                        .text_center()
                        .text_size(px(10.5))
                        .font_weight(gpui::FontWeight::SEMIBOLD)
                        .text_color(status_color)
                        .child(row.letter.to_string()),
                );
            list = list.child(row_el);
        }
        list
    }

    pub(crate) fn render_discard_dialog(
        &mut self,
        viewport: gpui::Size<gpui::Pixels>,
        theme: &Theme,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let prompt = self.discard_prompt.as_ref()?;
        let message = prompt.message.clone();
        let card = popover::dialog_card(theme)
            .child(popover::dialog_title(theme, "Discard changes"))
            .child(
                div()
                    .mt(px(12.0))
                    .child(popover::dialog_body(theme, message)),
            )
            .child(
                div()
                    .mt(px(16.0))
                    .flex()
                    .flex_row()
                    .justify_end()
                    .gap(px(8.0))
                    .child(
                        popover::btn_ghost(
                            theme,
                            "Cancel",
                            "details-source-control-discard-cancel",
                        )
                        .id("details-source-control-discard-cancel")
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.confirm_discard_prompt(false, cx);
                        })),
                    )
                    .child(
                        popover::btn_danger(theme, "Discard")
                            .id("details-source-control-discard-confirm")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.confirm_discard_prompt(true, cx);
                            })),
                    ),
            )
            .into_any_element();
        Some(popover::modal(
            "details-source-control-discard-dialog",
            viewport,
            card,
        ))
    }

    /// Cached `<cwd>/.git` probe — see the `has_git_dir` field.
    fn git_dir_exists(&mut self, cwd: &std::path::Path) -> bool {
        if let Some((cached_cwd, exists)) = &self.has_git_dir
            && cached_cwd == cwd
        {
            return *exists;
        }
        let exists = cwd.join(".git").exists();
        self.has_git_dir = Some((cwd.to_path_buf(), exists));
        exists
    }

    fn sync_idle_recap(&mut self, cx: &mut Context<Self>) {
        let Some(context) = self.sidebar.context() else {
            self.recap_task = None;
            self.recap_armed_epoch = None;
            return;
        };

        if context.mode != super::context::DetailsMode::Orchestrator {
            self.recap_task = None;
            self.recap_armed_epoch = None;
            return;
        }

        let Some(chat_id) = context.chat_id.clone() else {
            self.recap_task = None;
            self.recap_armed_epoch = None;
            return;
        };
        let context_key = context.key.clone();

        let (is_working, is_compacting, message_count, composer_shows_chat) = {
            let state = self.app_state.read(cx);
            let is_working = state.indicator_for(&chat_id, chrono::Utc::now())
                == crate::state::Indicator::Working;
            let count = state.transcript.len();
            // Compaction has its own flag: the indicator does not report it.
            (
                is_working,
                state.is_compacting(&chat_id),
                count,
                state.selected_chat.as_deref() == Some(chat_id.as_str()),
            )
        };
        let has_entry = self.sidebar.idle_recap_for(&context_key).is_some();
        let prefs = self.sidebar.preferences();

        let signals = super::idle_recap::IdleRecapSignals {
            enabled: prefs.idle_recap_enabled,
            delay_seconds: prefs.idle_recap_delay_seconds,
            is_streaming: is_working,
            is_compacting,
            message_count,
            failed_epoch: self.failed_epochs.get(&chat_id).copied(),
            composer_shows_chat,
            composer_has_draft: self.composer.read(cx).has_draft(cx),
        };

        let action = super::idle_recap::evaluate_idle_recap_signals(
            &signals,
            self.sidebar.idle_recap_for(&context_key),
        );
        match action {
            super::idle_recap::IdleRecapAction::Clear => {
                self.recap_task = None;
                self.recap_armed_epoch = None;
                if has_entry {
                    self.sidebar.clear_idle_recap(&context_key);
                    self.emit_preferences(cx);
                    cx.notify();
                }
            }
            super::idle_recap::IdleRecapAction::Keep | super::idle_recap::IdleRecapAction::None => {
                self.recap_task = None;
                self.recap_armed_epoch = None;
            }
            super::idle_recap::IdleRecapAction::Arm { delay_ms } => {
                if self.recap_armed_epoch.as_ref() == Some(&(chat_id.clone(), message_count))
                    && self.recap_task.is_some()
                {
                    return;
                }

                self.recap_armed_epoch = Some((chat_id.clone(), message_count));
                let chat_id_clone = chat_id.clone();
                let context_key = context.key.clone();
                let epoch_at_arm = message_count;

                self.recap_task = Some(cx.spawn(async move |this, cx| {
                    cx.background_executor()
                        .timer(std::time::Duration::from_millis(delay_ms))
                        .await;

                    let should_call = this
                        .update(cx, |this, cx| {
                            let state = this.app_state.read(cx);
                            let is_working = state.indicator_for(&chat_id_clone, chrono::Utc::now())
                                == crate::state::Indicator::Working;
                            let dispatch = super::idle_recap::IdleRecapDispatch {
                                is_streaming: is_working,
                                message_count: state.transcript.len(),
                                epoch_at_arm,
                                is_active_chat: this
                                    .sidebar
                                    .context()
                                    .and_then(|c| c.chat_id.as_deref())
                                    == Some(&chat_id_clone),
                            };
                            super::idle_recap::should_dispatch_idle_recap(&dispatch)
                        })
                        .unwrap_or(false);

                    if !should_call {
                        return;
                    }
                    let engine = this
                        .update(cx, |this, cx| {
                            this.app_state.read(cx).engine().cloned()
                        })
                        .ok()
                        .flatten();

                    let Some(engine) = engine else {
                        return;
                    };

                    let result = engine
                        .client()
                        .call_as::<zeron_rpc::GenerateChatRecapReply>(
                            zeron_rpc::methods::GENERATE_CHAT_RECAP,
                            serde_json::to_value(zeron_rpc::GenerateChatRecapParams::new(
                                &chat_id_clone,
                            ))
                            .unwrap_or_default(),
                        )
                        .await;

                    this.update(cx, |this, cx| {
                        match result {
                            Ok(reply) => {
                                if let Some(text) = reply.recap {
                                    let state = this.app_state.read(cx);
                                    let is_working = state
                                        .indicator_for(&chat_id_clone, chrono::Utc::now())
                                        == crate::state::Indicator::Working;
                                    let count_now = state.transcript.len();
                                    if !is_working && count_now == epoch_at_arm {
                                        let now = std::time::SystemTime::now()
                                            .duration_since(std::time::UNIX_EPOCH)
                                            .unwrap_or_default()
                                            .as_millis() as u64;
                                        let entry = super::idle_recap::IdleRecapEntry {
                                            text,
                                            epoch: epoch_at_arm,
                                            generated_at: now,
                                        };
                                        this.sidebar.set_idle_recap(context_key, entry);
                                        this.emit_preferences(cx);
                                        cx.notify();
                                    }
                                } else {
                                    this.failed_epochs.insert(chat_id_clone, epoch_at_arm);
                                }
                            }
                            Err(err) => {
                                tracing::debug!(chat = %chat_id_clone, error = %err, "GenerateChatRecap failed");
                                this.failed_epochs.insert(chat_id_clone, epoch_at_arm);
                            }
                        }
                    })
                    .ok();
                }));
            }
        }
    }

    fn workspace_file_source(
        &self,
        cx: &gpui::App,
    ) -> Option<(
        crate::state::EngineHandle,
        zeron_proto::WorkspaceTarget,
        String,
    )> {
        let context = self.sidebar.context()?;
        let state = self.app_state.read(cx);
        let device = context.target_device_id.clone()?;
        let engine = state.engine()?.clone();
        let target = if let Some(chat_id) = &context.chat_id {
            // Project-less local Chats retain the existing local-folder surface.
            if state
                .chats
                .iter()
                .find(|chat| &chat.id == chat_id)?
                .space_id
                .is_none()
            {
                return None;
            }
            zeron_proto::WorkspaceTarget {
                chat_id: Some(chat_id.clone()),
                space_id: None,
                checkout_path: None,
            }
        } else {
            let space = state.spaces.iter().find(|space| {
                space.device_id == device && std::path::Path::new(&space.path) == context.cwd
            })?;
            zeron_proto::WorkspaceTarget {
                chat_id: None,
                space_id: Some(space.id.clone()),
                checkout_path: None,
            }
        };
        Some((engine, target, device))
    }

    fn ensure_usage_tick(&mut self, cx: &mut Context<Self>) {
        if self.usage_tick.is_some() {
            return;
        }
        self.usage_tick = Some(cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(USAGE_TICK).await;
                let keep_ticking = this.update(cx, |this, cx| {
                    let mut should_notify = false;
                    if let Some(snapshot) = &this.usage_snapshot {
                        this.usage = LoadState::Ready(provider_usage_rows(
                            snapshot,
                            &crate::settings::current(cx).usage_widget_hidden_account_ids,
                            chrono::Utc::now(),
                        ));
                        should_notify = true;
                    }
                    if should_notify || this.worker_launch_age_widget_visible(cx) {
                        cx.notify();
                    }
                    let should_fetch = this
                        .usage_fetched_at
                        .is_none_or(|at| at.elapsed() >= USAGE_FETCH_INTERVAL);
                    if should_fetch && this.usage_task.is_none() {
                        this.load_usage(cx);
                    }
                    true
                });
                if !matches!(keep_ticking, Ok(true)) {
                    break;
                }
            }
        }));
    }

    fn worker_launch_age_widget_visible(&self, cx: &App) -> bool {
        let Some(context) = self.sidebar.context() else {
            return false;
        };
        if context.mode != super::context::DetailsMode::Orchestrator
            || self.sidebar.widget_hidden("chat-workers-widget")
        {
            return false;
        }
        let Some(chat_id) = context.chat_id.as_deref() else {
            return false;
        };
        let now = chrono::Utc::now();
        self.workers_model
            .read(cx)
            .sessions_for_parent_chat(chat_id)
            .is_ok_and(|sessions| {
                sessions.iter().any(|session| {
                    worker_launch_age(session.created_at_unix_ms, now.clone()).is_some()
                })
            })
    }

    fn load_usage(&mut self, cx: &mut Context<Self>) {
        let Some(engine) = self.app_state.read(cx).engine().cloned() else {
            if self.usage_snapshot.is_none() {
                self.usage = LoadState::Error("Engine not connected".into());
            }
            return;
        };
        if self.usage_snapshot.is_none() {
            self.usage = LoadState::Loading;
        }
        self.usage_task = Some(cx.spawn(async move |this, cx| {
            let result = engine
                .client()
                .call(
                    zeron_rpc::methods::LIST_AGENT_ACCOUNTS,
                    serde_json::json!({ "forceUsage": true }),
                )
                .await;
            let _ = this.update(cx, |this, cx| {
                this.usage_task = None;
                match result {
                    Ok(value) => match serde_json::from_value::<AgentAccountsSnapshot>(value) {
                        Ok(snapshot) => {
                            this.usage_fetched_at = Some(std::time::Instant::now());
                            let snapshot = this.usage_snapshot.insert(snapshot);
                            let rows = provider_usage_rows(
                                snapshot,
                                &crate::settings::current(cx).usage_widget_hidden_account_ids,
                                chrono::Utc::now(),
                            );
                            this.usage = LoadState::Ready(rows);
                        }
                        Err(error) => {
                            if this.usage_snapshot.is_none() {
                                this.usage = LoadState::Error(error.to_string().into());
                            }
                        }
                    },
                    Err(error) => {
                        if this.usage_snapshot.is_none() {
                            this.usage = LoadState::Error(error.to_string().into());
                        }
                    }
                };
                cx.notify();
            });
        }));
        cx.notify();
    }

    fn load_branch(&mut self, cx: &mut Context<Self>) {
        let Some(context) = self.sidebar.context().cloned() else {
            return;
        };
        let local_device_id = self.app_state.read(cx).local_device_id.clone();
        if context_file_access(&context, local_device_id.as_deref()) != ContextFileAccess::Local {
            return;
        }
        if context.branch.is_some() {
            return;
        }
        let generation = self.sidebar.load_generation();
        let context_key = context.key.clone();
        self.branch_task = Some(cx.spawn(async move |this, cx| {
            let branch = cx
                .background_executor()
                .spawn(async move { detect_git_branch(&context.cwd) })
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.sidebar.accept_load(generation, &context_key) {
                    this.resolved_branch = branch;
                    cx.notify();
                }
            });
        }));
    }

    fn material_icon(&mut self, path: SharedString) -> AnyElement {
        let image = self
            .material_icons
            .entry(path.clone())
            .or_insert_with(|| {
                icons::material_file_icon_image(path.as_ref())
                    .expect("resolved Material Icon Theme asset is embedded")
            })
            .clone();
        img(image)
            .size(px(15.0))
            .object_fit(ObjectFit::Contain)
            .flex_none()
            .into_any_element()
    }

    fn render_header(&mut self, theme: &Theme, cx: &mut Context<Self>) -> gpui::Div {
        div()
            .h(px(40.0))
            .flex_none()
            .px(px(8.0))
            .flex()
            .items_center()
            .justify_between()
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .child(
                        div()
                            .id("details-sidebar-close")
                            .size(px(28.0))
                            .rounded(px(6.0))
                            .flex()
                            .items_center()
                            .justify_center()
                            .cursor_pointer()
                            .hover(|style| style.bg(crate::theme::ink(0.05)))
                            .on_click(cx.listener(|_, _, _, cx| {
                                cx.emit(DetailsSidebarEvent::Close);
                            }))
                            .child(
                                icons::icon(icons::DETAILS_CHEVRONS_RIGHT)
                                    .size(px(17.0))
                                    .text_color(theme.text),
                            ),
                    )
                    .child(
                        div()
                            .text_size(px(12.5))
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .text_color(theme.text)
                            .child("Details"),
                    ),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(2.0))
                    .child(self.render_widgets_gear(theme, cx)),
            )
    }

    fn toolbar_button(
        &self,
        id: &'static str,
        icon_path: &'static str,
        theme: &Theme,
        listener: impl Fn(&gpui::ClickEvent, &mut gpui::Window, &mut gpui::App) + 'static,
    ) -> gpui::Stateful<gpui::Div> {
        div()
            .id(id)
            .size(px(28.0))
            .rounded(px(6.0))
            .flex()
            .items_center()
            .justify_center()
            .cursor_pointer()
            .hover(|style| style.bg(crate::theme::ink(0.05)))
            .on_click(listener)
            .child(
                icons::icon(icon_path)
                    .size(px(15.0))
                    .text_color(theme.text_muted),
            )
    }

    fn toolbar_button_with_tooltip(
        &self,
        id: &'static str,
        icon_path: &'static str,
        tooltip: &'static str,
        theme: &Theme,
        listener: impl Fn(&gpui::ClickEvent, &mut gpui::Window, &mut gpui::App) + 'static,
    ) -> gpui::Stateful<gpui::Div> {
        let label: SharedString = tooltip.into();
        self.toolbar_button(id, icon_path, theme, listener)
            .aria_label(tooltip)
            .tooltip(move |_, cx| {
                cx.new(|_| ActionTooltip {
                    label: label.clone(),
                })
                .into()
            })
    }

    fn current_chat_workers(
        &self,
        chat_id: &str,
        cx: &App,
    ) -> (ChatWorkersSnapshot, Option<SharedString>) {
        let tasks = activity_tasks_from_entries(&self.app_state.read(cx).transcript);
        match self
            .workers_model
            .read(cx)
            .sessions_for_parent_chat(chat_id)
        {
            Ok(workers) => (
                project_chat_workers(tasks, workers.into_iter().cloned().collect()),
                None,
            ),
            Err(error) => (project_chat_workers(tasks, Vec::new()), Some(error.into())),
        }
    }

    fn render_activity_status(
        &self,
        status: WorkflowTaskStatus,
        key: SharedString,
        theme: &Theme,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        match status {
            WorkflowTaskStatus::Running => {
                crate::loaders::mini_mono_spinner(key, 2.0, theme.accent, cx.entity_id(), cx)
                    .into_any_element()
            }
            WorkflowTaskStatus::Completed => settled_success_badge(theme),
            WorkflowTaskStatus::Failed => icons::icon(icons::CLOSE_CIRCLE)
                .size(px(13.0))
                .text_color(theme.danger)
                .into_any_element(),
            WorkflowTaskStatus::Cancelled => icons::icon(icons::CLOSE_CIRCLE)
                .size(px(13.0))
                .text_color(theme.text_muted)
                .into_any_element(),
        }
    }

    fn render_worker_status(
        &self,
        worker: &ChatWorkerRow,
        theme: &Theme,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        match worker.semantic {
            WorkerSemantic::Starting | WorkerSemantic::Working => {
                crate::loaders::mini_mono_spinner(
                    SharedString::from(format!("chat-worker-status-{}", worker.session_id)),
                    2.0,
                    theme.accent,
                    cx.entity_id(),
                    cx,
                )
                .into_any_element()
            }
            WorkerSemantic::Blocked => icons::icon(icons::INFO_CIRCLE)
                .size(px(13.0))
                .text_color(theme.warning)
                .into_any_element(),
            WorkerSemantic::Terminal if worker.activity == "failed" => {
                icons::icon(icons::CLOSE_CIRCLE)
                    .size(px(13.0))
                    .text_color(theme.danger)
                    .into_any_element()
            }
            WorkerSemantic::Terminal if worker.activity == "cancelled" => {
                icons::icon(icons::CLOSE_CIRCLE)
                    .size(px(13.0))
                    .text_color(theme.text_muted)
                    .into_any_element()
            }
            WorkerSemantic::Terminal => settled_success_badge(theme),
            // Turn finished, session still open for the next prompt: the
            // same settled check as a finished subagent.
            WorkerSemantic::Idle => settled_success_badge(theme),
            WorkerSemantic::Recovery => icons::icon(icons::RESTART)
                .size(px(13.0))
                .text_color(theme.text_muted)
                .into_any_element(),
            WorkerSemantic::Disconnected => icons::icon(icons::WIFI_OFF)
                .size(px(13.0))
                .text_color(theme.text_muted)
                .into_any_element(),
        }
    }

    fn render_workers_tab(
        &mut self,
        tab: ChatWorkersTab,
        label: &'static str,
        count: usize,
        active: ChatWorkersTab,
        theme: &Theme,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .id(SharedString::from(format!("chat-workers-tab-{label}")))
            .h(px(24.0))
            .px(px(7.0))
            .rounded(px(6.0))
            .flex()
            .items_center()
            .gap(px(4.0))
            .cursor_pointer()
            .text_size(px(11.0))
            .font_weight(gpui::FontWeight::MEDIUM)
            .text_color(if active == tab {
                theme.text
            } else {
                theme.text_muted
            })
            .when(active == tab, |pill| pill.bg(crate::theme::ink(0.08)))
            .hover(move |style| {
                if active == tab {
                    style.bg(crate::theme::ink(0.10))
                } else {
                    style.bg(crate::theme::ink(0.05))
                }
            })
            .on_click(cx.listener(move |this, _, _, cx| {
                this.chat_workers.select(tab);
                cx.notify();
            }))
            .child(label)
            .when(count > 0, |pill| {
                pill.child(
                    div()
                        .text_size(px(10.0))
                        .text_color(if active == tab {
                            theme.text_muted.opacity(0.9)
                        } else {
                            theme.text_muted
                        })
                        .child(count.to_string()),
                )
            })
            .into_any_element()
    }

    fn render_workflow_progress(&self, row: &ChatActivityRow, theme: &Theme) -> gpui::Div {
        let mut body = div();
        if let Some(usage) = &row.usage {
            body = body.child(
                div()
                    .ml(px(25.0))
                    .pb(px(4.0))
                    .text_size(px(10.0))
                    .text_color(theme.text_muted.opacity(0.75))
                    .child(usage.clone()),
            );
        }
        let mut current_phase = None;
        for node in &row.progress {
            match node {
                WorkflowProgressNode::Phase { index, title } => {
                    current_phase = Some(*index);
                    body = body.child(
                        div()
                            .ml(px(12.0))
                            .pl(px(9.0))
                            .py(px(3.0))
                            .border_l_1()
                            .border_color(theme.border.opacity(0.55))
                            .text_size(px(10.0))
                            .text_color(theme.text_muted)
                            .child(compact_activity_label(title)),
                    );
                }
                WorkflowProgressNode::Agent {
                    label,
                    phase_index,
                    phase_title,
                    model,
                    state,
                    ..
                } => {
                    if current_phase != Some(*phase_index)
                        && let Some(title) = phase_title
                    {
                        current_phase = Some(*phase_index);
                        body = body.child(
                            div()
                                .ml(px(12.0))
                                .pl(px(9.0))
                                .py(px(3.0))
                                .border_l_1()
                                .border_color(theme.border.opacity(0.55))
                                .text_size(px(10.0))
                                .text_color(theme.text_muted)
                                .child(compact_activity_label(title)),
                        );
                    }
                    let state_color = match state.as_deref() {
                        Some("done" | "completed") => theme.success,
                        Some("error" | "failed") => theme.danger,
                        Some("start" | "running") => theme.accent,
                        _ => theme.text_muted,
                    };
                    let indent = if current_phase == Some(*phase_index) {
                        21.0
                    } else {
                        12.0
                    };
                    body = body.child(
                        div()
                            .ml(px(indent))
                            .h(px(22.0))
                            .pl(px(9.0))
                            .border_l_1()
                            .border_color(theme.border.opacity(0.55))
                            .flex()
                            .items_center()
                            .gap(px(6.0))
                            .child(div().size(px(6.0)).rounded_full().bg(state_color))
                            .child(
                                div()
                                    .min_w_0()
                                    .flex_1()
                                    .truncate()
                                    .text_size(px(11.0))
                                    .text_color(theme.text)
                                    .child(compact_activity_label(label)),
                            )
                            .when_some(model.clone(), |line, model| {
                                line.child(
                                    div()
                                        .text_size(px(10.0))
                                        .text_color(theme.text_muted.opacity(0.75))
                                        .child(model.trim_start_matches("claude-").to_owned()),
                                )
                            }),
                    );
                }
            }
        }
        body
    }

    fn render_workflow_row(
        &mut self,
        row: ChatActivityRow,
        theme: &Theme,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let collapsible = row.usage.is_some() || !row.progress.is_empty();
        let expanded = self
            .chat_workers
            .activity_expanded_with_default(&row.id, false);
        let row_id = row.id.clone();
        let status = self.render_activity_status(
            row.status,
            SharedString::from(format!("workflow-status-{}", row.id)),
            theme,
            cx,
        );
        let header = div()
            .h(px(30.0))
            .px(px(8.0))
            .flex()
            .items_center()
            .gap(px(6.0))
            .when(collapsible, |header| {
                header.child(
                    div()
                        .id(SharedString::from(format!("workflow-expand-{}", row.id)))
                        .size(px(18.0))
                        .rounded(px(4.0))
                        .flex()
                        .items_center()
                        .justify_center()
                        .cursor_pointer()
                        .hover(|style| style.bg(crate::theme::ink(0.05)))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.chat_workers
                                .toggle_activity_with_default(&row_id, false);
                            cx.notify();
                        }))
                        .child(
                            icons::icon(if expanded {
                                icons::ALT_ARROW_DOWN
                            } else {
                                icons::ALT_ARROW_RIGHT
                            })
                            .size(px(11.0))
                            .text_color(theme.text_muted),
                        ),
                )
            })
            .when(!collapsible, |header| header.child(div().w(px(18.0))))
            .child(status)
            .child(
                div()
                    .min_w_0()
                    .flex_1()
                    .truncate()
                    .text_size(px(12.0))
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .text_color(theme.text)
                    .child(row.title.clone()),
            );
        div()
            .border_t_1()
            .border_color(theme.border.opacity(0.45))
            .child(header)
            .when(expanded, |item| {
                item.child(self.render_workflow_progress(&row, theme))
            })
            .into_any_element()
    }

    fn render_subagent_row(
        &mut self,
        row: ChatActivityRow,
        chat_id: String,
        theme: &Theme,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let collapsible = row.usage.is_some() || !row.progress.is_empty();
        let expanded = self
            .chat_workers
            .activity_expanded_with_default(&row.id, false);
        let row_id = row.id.clone();
        let event = open_subagent_event(&chat_id, &row);
        let doc_id = row.id.clone();
        let avatar_path = subagent_row_avatar_path(&row.id);
        let status = div()
            .size(px(15.0))
            .flex_none()
            .flex()
            .items_center()
            .justify_center()
            .child(self.render_activity_status(
                row.status,
                SharedString::from(format!("subagent-status-{}", row.id)),
                theme,
                cx,
            ));
        let transcript = div()
            .id(SharedString::from(format!("subagent-open-{}", row.id)))
            .min_w_0()
            .flex_1()
            .h_full()
            .flex()
            .items_center()
            .gap(px(7.0))
            .cursor_pointer()
            .hover(|style| style.bg(crate::theme::ink(0.05)))
            .on_click(cx.listener(move |this, _, _, cx| {
                let still_available = project_chat_workers(
                    activity_tasks_from_entries(&this.app_state.read(cx).transcript),
                    Vec::new(),
                )
                .subagents
                .iter()
                .any(|row| row.id == doc_id);
                if still_available {
                    cx.emit(event.clone());
                }
            }))
            .child(
                img(avatar_path)
                    .size(px(20.0))
                    .object_fit(ObjectFit::Contain),
            )
            .child(
                div()
                    .min_w_0()
                    .flex_1()
                    .truncate()
                    .text_size(px(12.0))
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .text_color(theme.text)
                    .child(row.title.clone()),
            )
            .child(status);
        let row_selector = SharedString::from(format!("chat-subagent-{}", row.id));
        div()
            .id(row_selector.clone())
            .debug_selector(move || row_selector.to_string())
            .border_t_1()
            .border_color(theme.border.opacity(0.45))
            .child(
                div()
                    .h(px(CHAT_WORKERS_ROW_HEIGHT))
                    .px(px(8.0))
                    .flex()
                    .items_center()
                    .gap(px(3.0))
                    .when(collapsible, |header| {
                        header.child(
                            div()
                                .id(SharedString::from(format!("subagent-expand-{}", row.id)))
                                .size(px(18.0))
                                .rounded(px(4.0))
                                .flex()
                                .items_center()
                                .justify_center()
                                .cursor_pointer()
                                .hover(|style| style.bg(crate::theme::ink(0.05)))
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.chat_workers
                                        .toggle_activity_with_default(&row_id, false);
                                    cx.notify();
                                }))
                                .child(
                                    icons::icon(if expanded {
                                        icons::ALT_ARROW_DOWN
                                    } else {
                                        icons::ALT_ARROW_RIGHT
                                    })
                                    .size(px(11.0))
                                    .text_color(theme.text_muted),
                                ),
                        )
                    })
                    .when(!collapsible, |header| header.child(div().w(px(18.0))))
                    .child(transcript),
            )
            .when(expanded, |item| {
                item.child(self.render_workflow_progress(&row, theme))
            })
            .into_any_element()
    }

    fn render_worker_row(
        &mut self,
        worker: ChatWorkerRow,
        chat_id: String,
        theme: &Theme,
        now: chrono::DateTime<chrono::Utc>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let event = open_worker_event(&chat_id, &worker);
        let session_id = worker.session_id.clone();
        let status = self.render_worker_status(&worker, theme, cx);
        let runtime_icon = runtime_icon_path(worker.provider_id.as_deref(), Some(&worker.command));
        let expansion_key = worker_expansion_key(&worker.session_id);
        let collapsible = worker.total_tokens.is_some() && !worker.model_usage.is_empty();
        let expanded = collapsible
            && self
                .chat_workers
                .activity_expanded_with_default(&expansion_key, false);
        let compact_metadata = worker_compact_metadata(&worker);
        let launch_age = worker_launch_age(worker.created_at_unix_ms, now);
        let model_usage = worker.model_usage.clone();
        let (label, total) = compact_metadata
            .map(|(model, total)| (model, Some(total)))
            .unwrap_or_else(|| (worker.command.clone(), None));
        let mut subtitle = div()
            .min_w_0()
            .flex()
            .items_center()
            .gap(px(6.0))
            .text_size(px(10.0))
            .text_color(theme.text_muted)
            .child(div().min_w_0().flex_1().truncate().child(label));
        if let Some(total) = total {
            subtitle = subtitle.child(div().flex_none().child(total));
        }
        if let Some(WorkerLaunchAge { label, tooltip }) = launch_age {
            let tooltip: SharedString = tooltip.into();
            subtitle = subtitle.child(
                div()
                    .id(SharedString::from(format!("chat-worker-age-{session_id}")))
                    .w(px(32.0))
                    .flex_none()
                    .truncate()
                    .tooltip(move |_, cx| {
                        cx.new(|_| ActionTooltip {
                            label: tooltip.clone(),
                        })
                        .into()
                    })
                    .child(label),
            );
        }
        let open_target = div()
            .min_w_0()
            .flex_1()
            .min_h(px(38.0))
            .py(px(5.0))
            .flex()
            .items_center()
            .gap(px(7.0))
            .child(
                icons::icon(runtime_icon)
                    .size(px(14.0))
                    .text_color(theme.text_muted),
            )
            .child(
                div()
                    .min_w_0()
                    .flex_1()
                    .child(
                        div()
                            .min_w_0()
                            .map(session_title_truncate)
                            .text_size(px(12.0))
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .text_color(theme.text)
                            .child(worker.title),
                    )
                    .child(subtitle),
            )
            .child(status);
        div()
            .id(SharedString::from(format!(
                "chat-worker-{}",
                worker.session_id
            )))
            .border_t_1()
            .border_color(theme.border.opacity(0.45))
            .child(
                div()
                    .id(SharedString::from(format!(
                        "chat-worker-open-{}",
                        worker.session_id
                    )))
                    .px(px(8.0))
                    .flex()
                    .items_center()
                    .gap(px(3.0))
                    .cursor_pointer()
                    .hover(|style| style.bg(crate::theme::ink(0.05)))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        let still_available = this
                            .workers_model
                            .read(cx)
                            .sessions_for_parent_chat(&chat_id)
                            .is_ok_and(|sessions| sessions.iter().any(|row| row.id == session_id));
                        cx.emit(worker_click_event(event.clone(), still_available));
                    }))
                    .when(collapsible, |header| {
                        let expansion_key = expansion_key.clone();
                        header.child(
                            div()
                                .id(SharedString::from(format!(
                                    "worker-telemetry-expand-{}",
                                    worker.session_id
                                )))
                                .size(px(18.0))
                                .flex_none()
                                .rounded(px(4.0))
                                .flex()
                                .items_center()
                                .justify_center()
                                .cursor_pointer()
                                .hover(|style| style.bg(crate::theme::ink(0.05)))
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    cx.stop_propagation();
                                    this.chat_workers
                                        .toggle_activity_with_default(&expansion_key, false);
                                    cx.notify();
                                }))
                                .child(
                                    icons::icon(if expanded {
                                        icons::ALT_ARROW_DOWN
                                    } else {
                                        icons::ALT_ARROW_RIGHT
                                    })
                                    .size(px(11.0))
                                    .text_color(theme.text_muted),
                                ),
                        )
                    })
                    .when(!collapsible, |header| header.child(div().w(px(18.0))))
                    .child(open_target),
            )
            .when(expanded, |container| {
                container.child(
                    div()
                        .pb(px(5.0))
                        .children(model_usage.into_iter().map(|usage| {
                            div()
                                .ml(px(29.0))
                                .h(px(22.0))
                                .pl(px(9.0))
                                .pr(px(8.0))
                                .border_l_1()
                                .border_color(theme.border.opacity(0.55))
                                .flex()
                                .items_center()
                                .gap(px(6.0))
                                .child(div().size(px(6.0)).flex_none().rounded_full().bg(
                                    if usage.active {
                                        theme.accent
                                    } else {
                                        theme.text_muted.opacity(0.65)
                                    },
                                ))
                                .child(
                                    div()
                                        .min_w_0()
                                        .flex_1()
                                        .truncate()
                                        .text_size(px(11.0))
                                        .text_color(theme.text)
                                        .child(usage.model),
                                )
                                .child(
                                    div()
                                        .flex_none()
                                        .text_size(px(10.0))
                                        .text_color(theme.text_muted)
                                        .child(format_token_total(usage.total_tokens)),
                                )
                        })),
                )
            })
            .into_any_element()
    }

    fn render_chat_workers(
        &mut self,
        chat_id: String,
        snapshot: ChatWorkersSnapshot,
        workers_error: Option<SharedString>,
        theme: &Theme,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<gpui::Div> {
        let workflows = snapshot.workflows.len();
        let subagents = snapshot.subagents.len();
        let running_subagents = running_subagent_count(&snapshot.subagents);
        let workers = snapshot.workers.len();
        if self.subagent_pages_chat.as_deref() != Some(chat_id.as_str()) {
            self.subagent_pages_chat = Some(chat_id.clone());
            self.subagent_pages = Default::default();
            self.finished_subagents = FinishedSubagentsDisclosure::default();
        }
        let expansion_ids = snapshot
            .workflows
            .iter()
            .chain(snapshot.subagents.iter())
            .map(|row| row.id.clone())
            .chain(
                snapshot
                    .workers
                    .iter()
                    .filter(|worker| !worker.model_usage.is_empty())
                    .map(|worker| worker_expansion_key(&worker.session_id)),
            )
            .collect::<Vec<_>>();
        self.chat_workers
            .sync_activities(expansion_ids.iter().map(String::as_str));
        // Raw counts, not `workers_tab_presence`: a binding failure lifting the
        // Workers tab from 0 to 1 is an error to surface, not a dispatch to
        // follow. And an errored snapshot goes in as absence, never as zero —
        // `current_chat_workers` empties the list on any client failure, so a
        // count would make the recovery look like a launch.
        let latest_started = [
            snapshot.latest_started_at(ChatWorkersTab::Workflows),
            snapshot.latest_started_at(ChatWorkersTab::Subagents),
            snapshot.latest_started_at(ChatWorkersTab::Workers),
        ];
        // What is running right now, per tab. A worker that goes back to work
        // shows up only here: its `created_at` is old and `updated_at` moves
        // with the host heartbeat, so neither can say it is active again.
        let running = |rows: &[ChatActivityRow]| {
            rows.iter()
                .filter(|row| row.status == WorkflowTaskStatus::Running)
                .map(|row| row.id.clone())
                .collect::<Vec<_>>()
        };
        self.chat_workers.sync_tab_focus([
            Some(TabActivity::new(
                workflows,
                running(&snapshot.workflows),
                latest_started[0],
            )),
            Some(TabActivity::new(
                subagents,
                running(&snapshot.subagents),
                latest_started[1],
            )),
            workers_error.is_none().then(|| {
                TabActivity::new(
                    workers,
                    snapshot
                        .workers
                        .iter()
                        .filter(|worker| worker.semantic.is_active())
                        .map(|worker| worker.session_id.clone())
                        .collect(),
                    latest_started[2],
                )
            }),
        ]);
        let active = self.chat_workers.active_tab_with_recency(
            (workflows, latest_started[0]),
            (subagents, latest_started[1]),
            (
                workers_tab_presence(workers, workers_error.is_some()),
                latest_started[2],
            ),
        );
        // Shimmer de atividade: a strip inteira brilha enquanto ha worker
        // rodando ou workflow/subagente em voo — um sinal por widget, em vez de
        // um spinner por linha.
        let shimmer = snapshot_is_active(&snapshot)
            .then(|| crate::loaders::activity_shimmer(crate::theme::ink(0.07), cx.entity_id(), cx));
        let now = chrono::Utc::now();
        let tabs = div()
            .relative()
            .overflow_hidden()
            .h(px(34.0))
            .px(px(7.0))
            .flex()
            .items_center()
            .gap(px(3.0))
            .border_b_1()
            .border_color(theme.border.opacity(0.55))
            .children(shimmer)
            .child(self.render_workers_tab(
                ChatWorkersTab::Workflows,
                "Workflows",
                workflows,
                active,
                theme,
                cx,
            ))
            .child(self.render_workers_tab(
                ChatWorkersTab::Subagents,
                "Subagents",
                running_subagents,
                active,
                theme,
                cx,
            ))
            .child(self.render_workers_tab(
                ChatWorkersTab::Workers,
                "Workers",
                workers,
                active,
                theme,
                cx,
            ));
        let subagent_groups = group_subagents(snapshot.subagents.clone());
        let body = match active {
            ChatWorkersTab::Workflows if workflows > 0 => div().children(
                snapshot
                    .workflows
                    .into_iter()
                    .map(|row| self.render_workflow_row(row, theme, cx)),
            ),
            ChatWorkersTab::Subagents if subagents > 0 => {
                let mut body = div().w_full().flex().flex_col();
                if !subagent_groups[0].is_empty() {
                    body = body.child(subagent_section_heading(
                        "Running",
                        subagent_groups[0].len(),
                        theme,
                    ));
                    body = body.children(
                        subagent_groups[0]
                            .iter()
                            .cloned()
                            .map(|row| self.render_subagent_row(row, chat_id.clone(), theme, cx)),
                    );
                }
                let finished_count = subagent_groups[1..].iter().map(Vec::len).sum::<usize>();
                if finished_count > 0 {
                    let expanded = self.finished_subagents.expanded;
                    body = body.child(
                        div()
                            .id("chat-subagents-finished-toggle")
                            .debug_selector(|| "chat-subagents-finished-toggle".into())
                            .h(px(30.0))
                            .w_full()
                            .px(px(8.0))
                            .flex()
                            .items_center()
                            .gap(px(6.0))
                            .cursor_pointer()
                            .role(gpui::Role::Button)
                            .aria_label("Finished subagents")
                            .aria_expanded(expanded)
                            .hover(|style| style.bg(crate::theme::ink(0.05)))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.finished_subagents.toggle();
                                cx.notify();
                            }))
                            .child(
                                icons::icon(if expanded {
                                    icons::ALT_ARROW_DOWN
                                } else {
                                    icons::ALT_ARROW_RIGHT
                                })
                                .size(px(12.0))
                                .text_color(theme.text_muted),
                            )
                            .child(
                                div()
                                    .flex_1()
                                    .text_size(px(10.0))
                                    .font_weight(gpui::FontWeight::MEDIUM)
                                    .text_color(theme.text_muted)
                                    .child("Finished"),
                            )
                            .child(
                                div()
                                    .text_size(px(10.0))
                                    .text_color(theme.text_muted)
                                    .child(finished_count.to_string()),
                            ),
                    );
                    if expanded {
                        for (slot, page, label, group) in [
                            (1, 0, "Completed", SubagentFinishedGroup::Completed),
                            (2, 1, "Failed", SubagentFinishedGroup::Failed),
                            (3, 2, "Cancelled", SubagentFinishedGroup::Cancelled),
                        ] {
                            let rows = &subagent_groups[slot];
                            if rows.is_empty() {
                                continue;
                            }
                            body = body.child(subagent_section_heading(label, rows.len(), theme));
                            let shown = self.subagent_pages[page].shown(group, rows.len());
                            body = body.children(rows.iter().take(shown).cloned().map(|row| {
                                self.render_subagent_row(row, chat_id.clone(), theme, cx)
                            }));
                            if self.subagent_pages[page].has_more(group, rows.len()) {
                                let id = SharedString::from(format!(
                                    "chat-subagents-more-{}",
                                    label.to_lowercase()
                                ));
                                let selector = id.clone();
                                let total = rows.len();
                                body = body.child(
                                    div()
                                        .id(id)
                                        .debug_selector(move || selector.to_string())
                                        .h(px(28.0))
                                        .mx(px(8.0))
                                        .my(px(4.0))
                                        .rounded(px(6.0))
                                        .flex()
                                        .items_center()
                                        .justify_center()
                                        .cursor_pointer()
                                        .text_size(px(11.0))
                                        .text_color(theme.text_muted)
                                        .hover(|style| {
                                            style.bg(crate::theme::ink(0.05)).text_color(theme.text)
                                        })
                                        .on_click(cx.listener(move |this, _, _, cx| {
                                            this.subagent_pages[page].page_more(group, total);
                                            cx.notify();
                                        }))
                                        .child("Show more"),
                                );
                            }
                        }
                    }
                }
                body
            }
            ChatWorkersTab::Workers if workers > 0 => {
                div().children(snapshot.workers.into_iter().map(|worker| {
                    self.render_worker_row(worker, chat_id.clone(), theme, now.clone(), cx)
                }))
            }
            ChatWorkersTab::Workflows => Self::render_workers_empty("workflows", theme),
            ChatWorkersTab::Subagents => Self::render_workers_empty("subagents", theme),
            ChatWorkersTab::Workers => workers_error.map_or_else(
                || Self::render_workers_empty("workers", theme),
                |error| Self::render_workers_error(error, theme),
            ),
        };
        widget_card(
            "chat-workers-widget",
            icons::DETAILS_WORKERS,
            "Workers",
            div().child(tabs).child(
                div()
                    .id("chat-workers-body")
                    .debug_selector(|| "chat-workers-body".into())
                    .max_h(px(chat_workers_viewport_height_px()))
                    .overflow_y_scroll()
                    .child(body),
            ),
            theme,
        )
    }

    fn render_workers_empty(label: &'static str, theme: &Theme) -> gpui::Div {
        div()
            .px(px(9.0))
            .py(px(12.0))
            .text_size(px(12.0))
            .text_color(theme.text_muted.opacity(0.75))
            .child(format!("No {label} yet."))
    }

    fn render_workers_error(error: SharedString, theme: &Theme) -> gpui::Div {
        div()
            .px(px(9.0))
            .py(px(12.0))
            .text_size(px(12.0))
            .text_color(theme.warning)
            .child("Workers unavailable")
            .child(
                div()
                    .mt(px(3.0))
                    .text_size(px(10.0))
                    .text_color(theme.text_muted)
                    .child(error),
            )
    }

    fn render_details(&mut self, theme: &Theme, cx: &mut Context<Self>) -> gpui::Div {
        let Some(context) = self.sidebar.context().cloned() else {
            return div()
                .p(px(16.0))
                .text_color(theme.text_muted)
                .child("No workspace selected");
        };
        let hide_workspace = self.sidebar.widget_hidden("workspace-widget");
        let hide_workers = self.sidebar.widget_hidden("chat-workers-widget");
        let hide_todos = self.sidebar.widget_hidden("todos-widget");
        let hide_usage = self.sidebar.widget_hidden("usage-widget");
        let folder = context
            .cwd
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("Workspace")
            .to_string();
        let current_branch = self
            .resolved_branch
            .clone()
            .or_else(|| context.branch.clone());
        let has_git = current_branch.is_some() || self.git_dir_exists(&context.cwd);
        let disabled = !has_git;
        let repo_target = crate::pickers::RepoTarget {
            path: context.cwd.to_string_lossy().to_string(),
            device_id: context.target_device_id.clone(),
            chat_id: context.chat_id.clone(),
            branch: current_branch.clone(),
        };
        let branch_control: AnyElement = self.pickers.update(cx, |p, cx| {
            p.render_workspace_branch_control(repo_target, disabled, cx)
        });

        let _ = folder;
        let mut workspace_body = div();
        let mut workspace_extra = false;

        if context.mode == super::context::DetailsMode::Orchestrator {
            let home = dirs_home();
            let worked = self.sidebar.worked_projects(
                &context,
                &self.app_state.read(cx).transcript,
                self.workers_model.read(cx).projects(),
                home.as_deref(),
            );
            if !worked.is_empty() {
                let collapsed = self.sidebar.projects_worked_collapsed();
                let count = worked.len();
                let header = div()
                    .id("projects-worked-header")
                    .h(px(super::worked_projects::WORKED_PROJECTS_ROW_HEIGHT))
                    .px(px(10.0))
                    .flex()
                    .items_center()
                    .gap(px(7.0))
                    .cursor_pointer()
                    .rounded_md()
                    .hover(|style| style.bg(crate::theme::ink(0.045)))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.sidebar.toggle_projects_worked_collapsed();
                        this.emit_preferences(cx);
                        cx.notify();
                    }))
                    .child(
                        icons::icon(icons::FOLDER)
                            .size(px(14.0))
                            .text_color(theme.text_muted),
                    )
                    .child(
                        div()
                            .text_size(px(12.0))
                            .text_color(theme.text_muted)
                            .child("Projects worked"),
                    )
                    .child(
                        div()
                            .ml_auto()
                            .mr(px(4.0))
                            .text_size(px(11.0))
                            .text_color(theme.text_muted)
                            .child(count.to_string()),
                    )
                    .child(
                        icons::icon(if collapsed {
                            icons::ALT_ARROW_RIGHT
                        } else {
                            icons::ALT_ARROW_DOWN
                        })
                        .size(px(12.0))
                        .text_color(theme.text_muted),
                    );

                let mut worked_section = div()
                    .mt(px(4.0))
                    .pt(px(4.0))
                    .border_t_1()
                    .border_color(theme.border.opacity(0.55))
                    .child(header);

                if !collapsed {
                    let rows = div()
                        .flex()
                        .flex_col()
                        .children(worked.iter().enumerate().map(|(index, project)| {
                            let path = project.path.clone();
                            div()
                                .id(("worked-project", index))
                                .h(px(super::worked_projects::WORKED_PROJECTS_ROW_HEIGHT))
                                .px(px(10.0))
                                .flex()
                                .items_center()
                                .gap(px(7.0))
                                .cursor_pointer()
                                .rounded_md()
                                .hover(|style| style.bg(crate::theme::ink(0.045)))
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.workers_model.update(cx, |model, cx| {
                                        model.reveal_project(path.clone(), cx);
                                    });
                                }))
                                // A Workers worktree reads as a branch, the same
                                // mark the Workers sidebar gives its sessions.
                                .child(if project.branch.is_some() {
                                    icons::icon(icons::WORKER_BRANCH)
                                        .size(px(14.0))
                                        .text_color(theme.success)
                                } else {
                                    icons::icon(icons::FOLDER)
                                        .size(px(14.0))
                                        .text_color(theme.text_muted)
                                })
                                .child(
                                    div()
                                        .flex_1()
                                        .min_w_0()
                                        .truncate()
                                        .text_size(px(12.0))
                                        .text_color(theme.text)
                                        .child(zeron_proto::view::single_line(&project.name)),
                                )
                        }));

                    worked_section = worked_section.child(
                        div()
                            .id("projects-worked-body")
                            .max_h(px(
                                super::worked_projects::worked_projects_viewport_height_px(),
                            ))
                            .overflow_y_scroll()
                            .child(rows),
                    );
                }
                workspace_body = workspace_body.child(worked_section);
                workspace_extra = true;
            }
            if let Some(entry) = self.sidebar.idle_recap_for(&context.key) {
                let generated_at =
                    chrono::DateTime::from_timestamp_millis(entry.generated_at as i64)
                        .unwrap_or_else(chrono::Utc::now);
                let local_now = chrono::Local::now();
                let entry_local = generated_at.with_timezone(&chrono::Local);
                let clock_text = if entry_local.date_naive() == local_now.date_naive() {
                    entry_local.format("%H:%M").to_string()
                } else {
                    entry_local.format("%b %d, %H:%M").to_string()
                };

                let recap_row = div()
                    .id("idle-recap-row")
                    .mt(px(4.0))
                    .pt(px(6.0))
                    .border_t_1()
                    .border_color(theme.border.opacity(0.50))
                    .flex()
                    .items_end()
                    .gap(px(8.0))
                    .px(px(10.0))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_size(px(12.0))
                            .italic()
                            .text_color(theme.text_muted.opacity(0.85))
                            .child(format!("※ recap: {}", entry.text)),
                    )
                    .child(
                        div()
                            .flex_shrink_0()
                            .text_size(px(10.0))
                            .text_color(theme.text_muted.opacity(0.50))
                            .child(clock_text),
                    );
                workspace_body = workspace_body.child(recap_row);
                workspace_extra = true;
            }
        }
        let mut content = div().w_full().flex().flex_col().gap(px(10.0)).p(px(10.0));
        if !hide_workspace {
            let extra = workspace_extra.then_some(workspace_body);
            content = content.child(self.render_session_card(
                &context,
                branch_control,
                has_git,
                extra,
                theme,
                cx,
            ));
        }

        if !hide_workers
            && context.mode == super::context::DetailsMode::Orchestrator
            && let Some(chat_id) = context.chat_id.clone()
        {
            let (snapshot, workers_error) = self.current_chat_workers(&chat_id, cx);
            if !snapshot.workflows.is_empty()
                || !snapshot.subagents.is_empty()
                || !snapshot.workers.is_empty()
                || workers_error.is_some()
            {
                content = content.child(self.render_chat_workers(
                    chat_id,
                    snapshot,
                    workers_error,
                    theme,
                    cx,
                ));
            }
        }

        if !hide_todos
            && context.mode == super::context::DetailsMode::Orchestrator
            && let Some(todos) = latest_todos(&self.app_state.read(cx).transcript)
        {
            let status_layout = todo_status_layout();
            let todo_rows =
                div().children(todos.items.into_iter().enumerate().map(|(index, todo)| {
                    div()
                        .h(px(status_layout.row_height_px))
                        .px(px(status_layout.horizontal_padding_px))
                        .border_t_1()
                        .border_color(theme.border.opacity(0.55))
                        .flex()
                        .items_center()
                        .gap(px(status_layout.gap_px))
                        .child(
                            div()
                                .size(px(status_layout.slot_px))
                                .flex_none()
                                .flex()
                                .items_center()
                                .justify_center()
                                .rounded_full()
                                .border_1()
                                .border_color(if todo.current {
                                    theme.text
                                } else {
                                    theme.border_strong
                                })
                                .when(todo.current, |dot| {
                                    dot.bg(theme.text).child(
                                        icons::icon(icons::ARROW_RIGHT)
                                            .size(px(status_layout.glyph_px))
                                            .text_color(theme.bg),
                                    )
                                })
                                .when(todo.done, |dot| {
                                    dot.bg(crate::theme::ink(0.08)).child(
                                        icons::icon(icons::CHECK)
                                            .size(px(status_layout.glyph_px))
                                            .text_color(theme.text_muted),
                                    )
                                }),
                        )
                        .child(
                            div()
                                .min_w_0()
                                .truncate()
                                .text_size(px(12.0))
                                .text_color(if todo.done {
                                    theme.text_muted
                                } else {
                                    theme.text
                                })
                                .child(format!("{}. {}", index + 1, todo.text)),
                        )
                }));
            let todo_body = div().child(
                div()
                    .id("todos-widget-body")
                    .max_h(px(todo_viewport_height_px(status_layout)))
                    .overflow_y_scroll()
                    .child(todo_rows),
            );
            content = content.child(widget_card(
                "todos-widget",
                icons::CHECKLIST,
                "To-dos",
                todo_body,
                theme,
            ));
        }

        if hide_usage {
            return content;
        }
        let hidden = crate::settings::current(cx).usage_widget_hidden_account_ids;
        let usage_body = match (&self.usage, &self.usage_snapshot) {
            (_, Some(snapshot)) => {
                let rows = provider_usage_rows(snapshot, &hidden, chrono::Utc::now());
                // Reachable only when every detected provider is toggled off —
                // an undetected one still contributes its placeholder.
                if rows.is_empty() {
                    div()
                        .p(px(10.0))
                        .text_size(px(12.0))
                        .text_color(theme.text_muted)
                        .child(SharedString::from(
                            "No accounts in Usage. Toggle them on in Settings → Accounts.",
                        ))
                } else {
                    div().children(
                        rows.into_iter()
                            .map(|row| self.render_usage_row(row, theme, cx)),
                    )
                }
            }
            (LoadState::Loading, None) => div()
                .p(px(10.0))
                .text_size(px(12.0))
                .text_color(theme.text_muted)
                .child("Loading usage…"),
            (LoadState::Error(message), None) => div()
                .p(px(10.0))
                .text_size(px(12.0))
                .text_color(theme.danger)
                .child(message.clone()),
            _ => div(),
        };
        content.child(widget_card(
            "usage-widget",
            icons::DETAILS_GAUGE,
            "Usage",
            usage_body,
            theme,
        ))
    }

    fn render_usage_row(
        &self,
        row: ProviderUsageRow,
        theme: &Theme,
        cx: &mut Context<Self>,
    ) -> gpui::Div {
        let key = row
            .account_id
            .clone()
            .unwrap_or_else(|| row.label.to_string());
        let expandable = row.state == ProviderUsageState::Ready
            && (!row.windows.is_empty() || !row.usage_lines.is_empty());
        let expanded = expandable && self.usage_expanded.contains(&key);
        let (icon_path, claude_tint) = usage_provider_icon(row.harness);
        // A row without quota reports the engine's own reason when there is one:
        // an expired credential or a rejected payload is not "no usage yet".
        let summary: SharedString = match row.state {
            ProviderUsageState::Ready => row
                .weekly_summary
                .clone()
                .unwrap_or_else(|| "—".into())
                .into(),
            ProviderUsageState::NoUsage => row
                .warning
                .clone()
                .unwrap_or_else(|| "No usage yet".into())
                .into(),
            ProviderUsageState::NotSignedIn => row
                .warning
                .clone()
                .unwrap_or_else(|| "Not signed in".into())
                .into(),
        };
        let reset_badge: Option<SharedString> = row
            .weekly_reset_badge
            .clone()
            .map(SharedString::from)
            .filter(|_| row.state == ProviderUsageState::Ready);
        let (tone_text, badge_bg) = match row.weekly_tone {
            UsageTone::Neutral => (theme.text_muted, crate::theme::ink(0.08)),
            UsageTone::Warning => (theme.warning, theme.warning.opacity(0.12)),
            UsageTone::Danger => (theme.danger, theme.danger.opacity(0.12)),
        };
        let windows = row.windows.clone();
        let usage_lines = row.usage_lines.clone();
        div()
            .border_t_1()
            .border_color(theme.border.opacity(0.55))
            .child(
                div()
                    .id(SharedString::from(format!("usage-provider-{key}")))
                    .h(px(34.0))
                    .px(px(10.0))
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .when(expandable, |header| {
                        header
                            .cursor_pointer()
                            .on_click(cx.listener(move |this, _, _, cx| {
                                if !this.usage_expanded.remove(&key) {
                                    this.usage_expanded.insert(key.clone());
                                }
                                cx.notify();
                            }))
                    })
                    .child(
                        icons::icon(icon_path)
                            .size(px(15.0))
                            .text_color(if claude_tint {
                                icons::claude_brand()
                            } else {
                                theme.text_muted
                            }),
                    )
                    .child(
                        div()
                            .min_w_0()
                            .flex()
                            .items_center()
                            .gap(px(6.0))
                            .child(
                                div()
                                    .text_size(px(12.5))
                                    .font_weight(gpui::FontWeight::MEDIUM)
                                    .text_color(theme.text)
                                    .child(row.label),
                            )
                            .when_some(row.account_label.clone(), |el, label| {
                                el.child(
                                    div()
                                        .min_w_0()
                                        .truncate()
                                        .text_size(px(11.0))
                                        .text_color(theme.text_muted)
                                        .child(SharedString::from(label)),
                                )
                            }),
                    )
                    .child(
                        // Badge + percent read as one right-aligned cluster, so
                        // the countdown sits next to the number it qualifies.
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .items_center()
                            .justify_end()
                            .gap(px(6.0))
                            .when_some(reset_badge, |cluster, badge| {
                                cluster.child(
                                    div()
                                        .flex_none()
                                        .rounded(px(4.0))
                                        .px(px(4.0))
                                        .py(px(2.0))
                                        .bg(badge_bg)
                                        .text_size(px(10.0))
                                        .font_weight(gpui::FontWeight::MEDIUM)
                                        .text_color(tone_text)
                                        .child(badge),
                                )
                            })
                            .child(
                                div()
                                    .min_w_0()
                                    .truncate()
                                    .text_size(px(12.0))
                                    .text_color(tone_text)
                                    .child(summary),
                            ),
                    )
                    .when(expandable, |header| {
                        header.child(
                            icons::icon(if expanded {
                                icons::ALT_ARROW_UP
                            } else {
                                icons::ALT_ARROW_DOWN
                            })
                            .size(px(13.0))
                            .text_color(theme.text_muted),
                        )
                    }),
            )
            .when(expanded, |container| {
                container.child(
                    div()
                        .border_t_1()
                        .border_color(theme.border.opacity(0.4))
                        .px(px(10.0))
                        .py(px(9.0))
                        .children(windows.into_iter().map(|window| {
                            let remaining = 1.0 - window.used_fraction;
                            let fill = if window.remaining_percent <= 10 {
                                theme.danger
                            } else if window.remaining_percent <= 25 {
                                theme.warning
                            } else {
                                theme.success
                            };
                            let label = match window.label.as_str() {
                                "Week" => "Weekly".to_string(),
                                "Session" => "5h".to_string(),
                                _ => window.label.clone(),
                            };
                            let pace = window.pace.clone();
                            div()
                                .pb(px(8.0))
                                .text_size(px(11.5))
                                .text_color(theme.text_muted)
                                .child(
                                    div()
                                        .flex()
                                        .items_center()
                                        .child(
                                            div()
                                                .font_weight(gpui::FontWeight::MEDIUM)
                                                .text_color(theme.text)
                                                .child(format!(
                                                    "{label}  {}% left",
                                                    window.remaining_percent
                                                )),
                                        )
                                        .child(div().flex_1())
                                        .when_some(window.reset_text.clone(), |el, reset| {
                                            el.child(reset)
                                        }),
                                )
                                .child(
                                    div()
                                        .mt(px(5.0))
                                        .h(px(6.0))
                                        .w_full()
                                        .rounded_full()
                                        .overflow_hidden()
                                        .relative()
                                        .bg(crate::theme::ink(0.08))
                                        .when(remaining > 0.0, |track| {
                                            track.child(
                                                div()
                                                    .h_full()
                                                    .w(gpui::relative(remaining))
                                                    .rounded_full()
                                                    .bg(fill),
                                            )
                                        })
                                        .when_some(pace.clone(), |track, pace| {
                                            track.child(
                                                div()
                                                    .absolute()
                                                    .top_0()
                                                    .bottom_0()
                                                    .left(gpui::relative(
                                                        pace.expected_remaining_fraction,
                                                    ))
                                                    .w(px(2.0))
                                                    .bg(
                                                        if pace.amount_text.as_deref().is_some_and(
                                                            |text| text.contains("deficit"),
                                                        ) {
                                                            theme.danger
                                                        } else {
                                                            theme.success
                                                        },
                                                    ),
                                            )
                                        }),
                                )
                                .when_some(pace, |el, pace| {
                                    el.child(
                                        div()
                                            .mt(px(5.0))
                                            .flex()
                                            .items_center()
                                            .child(pace.amount_text.unwrap_or_default())
                                            .child(div().flex_1())
                                            .child(pace.eta_text.unwrap_or_default()),
                                    )
                                })
                        }))
                        .when(!usage_lines.is_empty(), |body| {
                            body.child(
                                div()
                                    .border_t_1()
                                    .border_color(theme.border.opacity(0.4))
                                    .pt(px(8.0))
                                    .children(usage_lines.into_iter().map(|line| {
                                        div()
                                            .pb(px(7.0))
                                            .child(
                                                div()
                                                    .flex()
                                                    .items_center()
                                                    .text_size(px(11.5))
                                                    .child(
                                                        div()
                                                            .font_weight(gpui::FontWeight::MEDIUM)
                                                            .text_color(theme.text)
                                                            .child(line.label),
                                                    )
                                                    .child(div().flex_1())
                                                    .child(
                                                        div()
                                                            .text_color(theme.text_muted)
                                                            .child(line.value),
                                                    ),
                                            )
                                            .when_some(line.subtitle, |el, subtitle| {
                                                el.child(
                                                    div()
                                                        .mt(px(2.0))
                                                        .text_size(px(11.0))
                                                        .text_color(theme.text_muted.opacity(0.75))
                                                        .child(subtitle),
                                                )
                                            })
                                    })),
                            )
                        }),
                )
            })
    }

    fn emit_open_file(&self, relative_path: &str, cx: &mut Context<Self>) {
        let Some(context) = self.sidebar.context() else {
            return;
        };
        cx.emit(DetailsSidebarEvent::OpenFile {
            context_key: context.key.clone(),
            root: context.cwd.clone(),
            relative_path: relative_path.to_string(),
            remote_target: if context_file_access(
                context,
                self.app_state.read(cx).local_device_id.as_deref(),
            ) == ContextFileAccess::Remote
            {
                self.workspace_file_source(cx)
                    .map(|(_, target, device)| (target, device))
            } else {
                None
            },
        });
    }
}

impl EventEmitter<DetailsSidebarEvent> for DetailsSidebar {}

/// The Details sidebar's Changes panel mounted in another surface (the Files
/// explorer's Changes tab). It renders the ONE source-control state owned by
/// [`DetailsSidebar`] — commit message, staging, sync, discard prompt — so a
/// commit from either place is the same commit and both stay in step.
pub struct SourceControlPane {
    sidebar: Entity<DetailsSidebar>,
    _observe: gpui::Subscription,
}

impl SourceControlPane {
    pub fn new(sidebar: Entity<DetailsSidebar>, cx: &mut Context<Self>) -> Self {
        let _observe = cx.observe(&sidebar, |_, _, cx| cx.notify());
        Self { sidebar, _observe }
    }

    /// Staged + unstaged paths, for the host tab's badge.
    pub fn change_count(&self, cx: &gpui::App) -> usize {
        self.sidebar
            .read(cx)
            .checkout_status
            .as_ref()
            .map(|status| source_control::change_badge(&status.files))
            .unwrap_or(0)
    }
}

impl Render for SourceControlPane {
    fn render(&mut self, window: &mut gpui::Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = Theme::of(cx).clone();
        let viewport = window.viewport_size();
        let (body, discard_dialog) = self.sidebar.update(cx, |sidebar, cx| {
            (
                sidebar.render_source_control(&theme, cx),
                sidebar.render_discard_dialog(viewport, &theme, cx),
            )
        });
        div()
            .size_full()
            .flex()
            .flex_col()
            .child(
                div()
                    .id("files-changes-scroll")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .child(body),
            )
            .children(discard_dialog)
    }
}

impl Render for DetailsSidebar {
    fn render(&mut self, window: &mut gpui::Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = Theme::of(cx).clone();
        let viewport = window.viewport_size();
        let body = self.render_details(&theme, cx);
        let discard_dialog = self.render_discard_dialog(viewport, &theme, cx);
        div()
            .size_full()
            .flex()
            .flex_col()
            .when_some(details_sidebar_background(&theme), |sidebar, background| {
                sidebar.bg(background)
            })
            .child(self.render_header(&theme, cx))
            .child(
                div()
                    .id("details-sidebar-scroll")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .child(body),
            )
            .children(discard_dialog)
    }
}

#[cfg(test)]
mod tests {
    use std::{collections::HashMap, path::PathBuf};

    use super::{
        ContextFileAccess, DetailsSidebar, DetailsSidebarEvent, DetailsSidebarPreferences,
        DetailsSidebarState, FinishedSubagentsDisclosure, context_file_access,
        details_sidebar_background, open_subagent_event, open_worker_event,
        subagent_row_avatar_path, worker_click_event,
    };
    use crate::details_sidebar::chat_workers::{
        ChatActivityRow, ChatWorkerRow, ChatWorkersSnapshot, WorkerSemantic,
    };
    use crate::details_sidebar::context::{DetailsContext, DetailsMode};
    use crate::theme::Theme;
    use gpui::{
        AppContext, Context, Entity, IntoElement, ParentElement, Render, Styled, Subscription,
        Window, div, px,
    };
    use zeron_proto::agent::WorkflowTaskStatus;

    fn context(key: &str) -> DetailsContext {
        DetailsContext {
            key: key.into(),
            cwd: PathBuf::from(format!("/tmp/{key}")),
            branch: Some("main".into()),
            chat_id: None,
            target_device_id: None,
            mode: DetailsMode::Workers,
        }
    }

    #[test]
    fn finished_subagent_disclosure_defaults_closed_and_toggles() {
        let mut disclosure = FinishedSubagentsDisclosure::default();
        assert!(!disclosure.expanded);

        disclosure.toggle();
        assert!(disclosure.expanded);

        disclosure.toggle();
        assert!(!disclosure.expanded);
    }

    #[gpui::test]
    fn finished_subagents_start_hidden_and_reveal_on_native_click(cx: &mut gpui::TestAppContext) {
        use crate::details_sidebar::widgets::ChatWorkersTab;
        use gpui::Modifiers;

        struct Probe {
            sidebar: Entity<DetailsSidebar>,
            chat_id: String,
            snapshot: ChatWorkersSnapshot,
            _sidebar_observe: Subscription,
        }

        impl Render for Probe {
            fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
                let snapshot = self.snapshot.clone();
                let chat_id = self.chat_id.clone();
                let theme = Theme::of(cx).clone();
                let mut body = None;
                let _ = self.sidebar.update(cx, |sidebar, cx| {
                    sidebar.chat_workers.select(ChatWorkersTab::Subagents);
                    body = Some(sidebar.render_chat_workers(chat_id, snapshot, None, &theme, cx));
                });
                div()
                    .w(px(900.0))
                    .h(px(900.0))
                    .child(body.expect("the Details widget stays mounted"))
            }
        }

        let data = tempfile::tempdir().unwrap();
        cx.update(|cx| {
            gpui_base::init(cx);
            crate::settings::init(crate::settings::UiSettings::default(), data.path(), cx);
            cx.set_global(Theme::dark());
        });
        let state = cx.new(|_| crate::state::AppState::new());
        let workers = cx.new(|cx| crate::workers::model::WorkersModel::detached(state.clone(), cx));
        let composer = cx.new(|cx| crate::composer::Composer::new(state.clone(), cx));
        let pickers = composer.read_with(cx, |composer, _| composer.pickers().clone());
        let sidebar = cx.new(|cx| {
            DetailsSidebar::new(
                state,
                workers,
                DetailsSidebarPreferences::default(),
                pickers,
                composer,
                cx,
            )
        });
        let row = |id: String, status| ChatActivityRow {
            title: id.clone(),
            description: None,
            id,
            status,
            usage: None,
            progress: Vec::new(),
            subagent_type: None,
            started_at_unix_ms: 1,
        };
        let mut subagents = vec![row("running".into(), WorkflowTaskStatus::Running)];
        subagents.extend((0..12).map(|index| {
            row(
                format!("completed-{index:02}"),
                WorkflowTaskStatus::Completed,
            )
        }));
        subagents.push(row("failed".into(), WorkflowTaskStatus::Failed));
        subagents.push(row("cancelled".into(), WorkflowTaskStatus::Cancelled));
        let snapshot = ChatWorkersSnapshot {
            subagents,
            ..Default::default()
        };
        let (probe_view, window) = cx.add_window_view(|_, cx| {
            let probe_sidebar = sidebar.clone();
            let observe = cx.observe(&probe_sidebar, |_, _, cx| cx.notify());
            Probe {
                sidebar: probe_sidebar,
                chat_id: "fixture-chat".into(),
                snapshot,
                _sidebar_observe: observe,
            }
        });

        window.update(|window, cx| window.draw(cx).clear());
        assert!(window.debug_bounds("chat-subagent-running").is_some());
        assert!(
            window
                .debug_bounds("chat-subagents-finished-toggle")
                .is_some()
        );
        assert!(window.debug_bounds("chat-subagent-completed-00").is_none());
        assert!(window.debug_bounds("chat-subagent-failed").is_none());
        assert!(window.debug_bounds("chat-subagent-cancelled").is_none());

        let finished = window
            .debug_bounds("chat-subagents-finished-toggle")
            .expect("finished disclosure is visible");
        window.simulate_click(finished.center(), Modifiers::default());
        window.update(|window, cx| window.draw(cx).clear());
        assert!(window.debug_bounds("chat-subagent-completed-00").is_some());
        assert!(window.debug_bounds("chat-subagent-failed").is_some());
        assert!(window.debug_bounds("chat-subagent-cancelled").is_some());
        assert!(
            window
                .debug_bounds("chat-subagents-more-completed")
                .is_some()
        );

        let body = window
            .debug_bounds("chat-workers-body")
            .expect("workers body remains scrollable");
        window.simulate_event(gpui::ScrollWheelEvent {
            position: body.center(),
            delta: gpui::ScrollDelta::Pixels(gpui::point(px(0.0), px(-1_000.0))),
            ..Default::default()
        });
        window.run_until_parked();
        window.update(|window, cx| window.draw(cx).clear());
        let more = window
            .debug_bounds("chat-subagents-more-completed")
            .expect("the Completed list has another page");
        window.simulate_click(more.center(), Modifiers::default());
        window.update(|window, cx| window.draw(cx).clear());
        let mut completed_shown = None;
        sidebar.read_with(window, |sidebar, _| {
            completed_shown = Some(sidebar.subagent_pages[0].shown(
                crate::details_sidebar::chat_workers::SubagentFinishedGroup::Completed,
                12,
            ));
        });
        assert_eq!(completed_shown, Some(12));

        let _ = probe_view.update(window, |probe, cx| {
            probe.chat_id = "another-chat".into();
            cx.notify();
        });
        window.update(|window, cx| window.draw(cx).clear());
        assert!(window.debug_bounds("chat-subagent-completed-00").is_none());
        assert!(window.debug_bounds("chat-subagent-running").is_some());
        let mut reset_state = None;
        sidebar.read_with(window, |sidebar, _| {
            reset_state = Some((
                sidebar.finished_subagents.expanded,
                sidebar.subagent_pages[0].shown(
                    crate::details_sidebar::chat_workers::SubagentFinishedGroup::Completed,
                    12,
                ),
            ));
        });
        assert_eq!(reset_state, Some((false, 10)));
    }

    #[test]
    fn sidebar_background_is_inherited_from_the_chat_surface() {
        assert_eq!(details_sidebar_background(&Theme::dark()), None);
        assert_eq!(details_sidebar_background(&Theme::light()), None);
    }

    #[test]
    fn subagent_row_uses_seeded_blobatar_avatar() {
        let avatar = subagent_row_avatar_path("subagent-1");

        assert_eq!(avatar, "icons/subagents/blobatar/23.svg");
        assert_ne!(avatar, crate::icons::BOT);
    }

    #[test]
    fn preferences_from_a_files_tree_build_load_and_drop_file_paths() {
        // Written by builds that had the Details Files tree.
        let old = r#"{
            "activeTab": "files",
            "expanded": {"one": ["src", ":projects_worked:collapsed"], "two": ["docs"]},
            "hidden": {"one": true},
            "hiddenWidgets": ["usage-widget"]
        }"#;
        let preferences: DetailsSidebarPreferences = serde_json::from_str(old).unwrap();
        let mut state = DetailsSidebarState::new(preferences);
        let preferences = state.preferences();
        assert_eq!(
            preferences.expanded,
            HashMap::from([(
                "one".to_string(),
                vec![":projects_worked:collapsed".to_string()]
            )])
        );
        assert!(state.widget_hidden("usage-widget"));
        state.set_context(Some(context("one")));
        assert!(state.projects_worked_collapsed());
    }

    #[test]
    fn widget_visibility_toggles_and_persists() {
        let mut state = DetailsSidebarState::new(DetailsSidebarPreferences::default());
        // Default: every toggleable widget is visible.
        for entry in super::TOGGLEABLE_WIDGETS {
            assert!(!state.widget_hidden(entry.0));
        }
        state.toggle_widget_hidden("usage-widget");
        assert!(state.widget_hidden("usage-widget"));
        assert!(!state.widget_hidden("workspace-widget"));
        // Persists across a reload (boot path: preferences() -> stored -> new()).
        let mut reloaded = DetailsSidebarState::new(state.preferences());
        assert!(reloaded.widget_hidden("usage-widget"));
        assert_eq!(
            reloaded.preferences().hidden_widgets,
            std::collections::BTreeSet::from(["usage-widget".to_string()])
        );
        // Toggling again clears it.
        reloaded.toggle_widget_hidden("usage-widget");
        assert!(!reloaded.widget_hidden("usage-widget"));
    }

    #[test]
    fn projects_worked_collapse_toggle_and_round_trip() {
        let mut state = DetailsSidebarState::new(DetailsSidebarPreferences::default());
        state.set_context(Some(context("one")));

        // Default is expanded (not collapsed)
        assert!(!state.projects_worked_collapsed());

        // Toggle to collapsed
        state.toggle_projects_worked_collapsed();
        assert!(state.projects_worked_collapsed());

        // Switch context: other context defaults to expanded
        state.set_context(Some(context("two")));
        assert!(!state.projects_worked_collapsed());

        // Switch back to "one": remains collapsed
        state.set_context(Some(context("one")));
        assert!(state.projects_worked_collapsed());

        // Toggle back to expanded
        state.toggle_projects_worked_collapsed();
        assert!(!state.projects_worked_collapsed());
    }
    #[test]
    fn worked_projects_cache_key_invalidation() {
        use zeron_doc::{MessagePart, MessageRole, SessionMessageEntry};
        use zeron_proto::ToolCall;
        use zeron_workers_unpeel::{WorkersProject, WorkersSessionSort};

        let mut state = DetailsSidebarState::new(DetailsSidebarPreferences::default());
        let ctx_one = context("one");
        let ctx_two = context("two");

        let p1 = WorkersProject {
            id: "p1".to_string(),
            name: "Proj 1".to_string(),
            path: "/tmp/proj1".to_string(),
            folder_id: None,
            parent_project_id: None,
            is_group: false,
            worktree_branch: None,
            git_branch: None,
            archived_session_count: 0,
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
            space_id: None,
        };

        let entry1 = SessionMessageEntry {
            id: "m1".to_string(),
            role: MessageRole::Assistant,
            parts: vec![MessagePart::Tool {
                id: "t1".to_string(),
                call: ToolCall::ReadFile {
                    path: "/tmp/proj1/file.rs".to_string(),
                },
                diff: None,
                output: None,
                output_ref: None,
                output_bytes: None,
                diff_ref: None,
                diff_stats: None,
                file_preview: None,
                subagent_ref: None,
                subagent_status: None,
                subagent_tail: None,
                subagent_end: None,
                execution: None,
                resolved: true,
                is_error: false,
            }],
            created_at: 0,
            device_id: "d1".to_string(),
            status: None,
            duration_ms: None,
            continuation_of: None,
        };

        let transcript = vec![entry1.clone()];
        let projects = vec![p1.clone()];

        // 1. First call computes and caches
        let res1 = state.worked_projects(&ctx_one, &transcript, &projects, None);
        assert_eq!(res1.len(), 1);
        let key1 = state.worked_projects_cache_key().unwrap().clone();
        assert_eq!(key1.context_key, "one");
        assert_eq!(key1.transcript_len, 1);

        // 2. Same inputs reuse cache (key identical)
        let res2 = state.worked_projects(&ctx_one, &transcript, &projects, None);
        assert_eq!(res2.len(), 1);
        assert_eq!(state.worked_projects_cache_key(), Some(&key1));

        // 3. Changed transcript length recomputes with new key
        let mut transcript_longer = transcript.clone();
        transcript_longer.push(entry1.clone());
        let _ = state.worked_projects(&ctx_one, &transcript_longer, &projects, None);
        let key2 = state.worked_projects_cache_key().unwrap().clone();
        assert_ne!(key1, key2);
        assert_eq!(key2.transcript_len, 2);

        // 4. Changed projects list recomputes with new key
        let mut projects_modified = projects.clone();
        projects_modified.push(WorkersProject {
            id: "p2".to_string(),
            name: "Proj 2".to_string(),
            path: "/tmp/proj2".to_string(),
            folder_id: None,
            parent_project_id: None,
            is_group: false,
            worktree_branch: None,
            git_branch: None,
            archived_session_count: 0,
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
            space_id: None,
        });
        let _ = state.worked_projects(&ctx_one, &transcript_longer, &projects_modified, None);
        let key3 = state.worked_projects_cache_key().unwrap().clone();
        assert_ne!(key2, key3);

        // 5. Changed context recomputes with new key
        let _ = state.worked_projects(&ctx_two, &transcript_longer, &projects_modified, None);
        let key4 = state.worked_projects_cache_key().unwrap().clone();
        assert_ne!(key3, key4);
        assert_eq!(key4.context_key, "two");

        // 6. A STREAMING turn grows the same entry: entry count is unchanged
        //    (`TranscriptFrame::Delta` upserts by id), so only `parts_total`
        //    can catch it. Without it the card freezes for the whole turn.
        let mut streaming = vec![entry1.clone()];
        let MessagePart::Tool { .. } = &streaming[0].parts[0] else {
            panic!("fixture must start with a tool part");
        };
        streaming[0].parts.push(MessagePart::Tool {
            id: "t2".to_string(),
            call: ToolCall::ReadFile {
                path: "/tmp/proj2/other.rs".to_string(),
            },
            diff: None,
            output: None,
            output_ref: None,
            output_bytes: None,
            diff_ref: None,
            diff_stats: None,
            file_preview: None,
            subagent_ref: None,
            subagent_status: None,
            subagent_tail: None,
            subagent_end: None,
            execution: None,
            resolved: true,
            is_error: false,
        });
        let before = state.worked_projects(&ctx_one, &[entry1.clone()], &projects_modified, None);
        let key_before = state.worked_projects_cache_key().unwrap().clone();
        let after = state.worked_projects(&ctx_one, &streaming, &projects_modified, None);
        let key_after = state.worked_projects_cache_key().unwrap().clone();
        assert_eq!(key_before.transcript_len, key_after.transcript_len);
        assert_ne!(key_before, key_after);
        assert_eq!(before.len(), 1);
        assert_eq!(after.len(), 2, "the new part's project must appear");
    }

    #[test]
    fn workers_widget_actions_preserve_stable_target_identity() {
        let subagent = ChatActivityRow {
            id: "chat--sub--review".into(),
            title: "Review parser".into(),
            description: None,
            status: WorkflowTaskStatus::Completed,
            usage: None,
            progress: Vec::new(),
            subagent_type: Some("reviewer".into()),
            started_at_unix_ms: 0,
        };
        let worker = ChatWorkerRow {
            session_id: "worker-42".into(),
            project_id: "project-1".into(),
            title: "Fix tests".into(),
            command: "codex".into(),
            provider_id: Some("codex".into()),
            semantic: WorkerSemantic::Working,
            state: "running".into(),
            activity: "working".into(),
            created_at_unix_ms: 0,
            updated_at_unix_ms: 42,
            total_tokens: None,
            model_usage: Vec::new(),
        };

        let DetailsSidebarEvent::OpenSubagent {
            chat_id,
            doc_id,
            title,
            frozen,
        } = open_subagent_event("chat-1", &subagent)
        else {
            panic!("expected subagent action");
        };
        assert_eq!(chat_id, "chat-1");
        assert_eq!(doc_id, "chat--sub--review");
        assert_eq!(title, "Review parser");
        assert!(frozen);

        let DetailsSidebarEvent::OpenWorkerSession {
            chat_id,
            session_id,
            title,
        } = open_worker_event("chat-1", &worker)
        else {
            panic!("expected worker action");
        };
        assert_eq!(chat_id, "chat-1");
        assert_eq!(session_id, "worker-42");
        assert_eq!(title, "Fix tests");
    }

    #[test]
    fn stale_worker_click_still_reaches_shell_for_final_revalidation() {
        let worker = ChatWorkerRow {
            session_id: "worker-42".into(),
            project_id: "project-1".into(),
            title: "Fix tests".into(),
            command: "codex".into(),
            provider_id: Some("codex".into()),
            semantic: WorkerSemantic::Disconnected,
            state: "disconnected".into(),
            activity: "disconnected".into(),
            created_at_unix_ms: 0,
            updated_at_unix_ms: 42,
            total_tokens: None,
            model_usage: Vec::new(),
        };

        let DetailsSidebarEvent::OpenWorkerSession {
            chat_id,
            session_id,
            title,
        } = worker_click_event(open_worker_event("chat-1", &worker), false)
        else {
            panic!("expected worker action");
        };
        assert_eq!(chat_id, "chat-1");
        assert_eq!(session_id, "worker-42");
        assert_eq!(title, "Fix tests");
    }

    #[test]
    fn stale_load_cannot_replace_current_context() {
        let mut state = DetailsSidebarState::new(DetailsSidebarPreferences::default());
        let first = state.set_context(Some(context("one")));
        let second = state.set_context(Some(context("two")));
        assert!(first < second);
        assert!(!state.accept_load(first, "one"));
        assert!(state.accept_load(second, "two"));
    }

    #[test]
    fn remote_contexts_never_fall_back_to_the_local_filesystem() {
        let mut remote = context("remote");
        remote.target_device_id = Some("device-b".into());

        assert_eq!(
            context_file_access(&remote, Some("device-a")),
            ContextFileAccess::Remote
        );
        assert_eq!(
            context_file_access(&remote, None),
            ContextFileAccess::WaitingForDevice
        );
        assert_eq!(
            context_file_access(&remote, Some("device-b")),
            ContextFileAccess::Local
        );
    }
}
