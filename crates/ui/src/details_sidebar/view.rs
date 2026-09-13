use std::collections::{BTreeSet, HashMap, HashSet};

use serde::{Deserialize, Serialize};

use super::context::{DetailsContext, DetailsTab};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct DetailsSidebarPreferences {
    pub active_tab: DetailsTab,
    pub expanded: HashMap<String, Vec<String>>,
    pub hidden: HashMap<String, bool>,
    pub idle_recaps: HashMap<String, super::idle_recap::IdleRecapEntry>,
    pub idle_recap_enabled: bool,
    pub idle_recap_delay_seconds: u64,
    pub hidden_widgets: BTreeSet<String>,
}

impl Default for DetailsSidebarPreferences {
    fn default() -> Self {
        Self {
            active_tab: DetailsTab::Details,
            expanded: HashMap::new(),
            hidden: HashMap::new(),
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

    pub fn tab(&self) -> DetailsTab {
        self.preferences.active_tab
    }

    pub fn set_tab(&mut self, tab: DetailsTab) {
        self.preferences.active_tab = tab;
    }

    pub fn expanded_paths(&self) -> HashSet<String> {
        let Some(context) = &self.context else {
            return HashSet::new();
        };
        self.preferences
            .expanded
            .get(&context.key)
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .filter(|p| !p.starts_with(':'))
            .collect()
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

    pub fn toggle_expanded(&mut self, relative_path: &str) {
        let Some(context) = &self.context else {
            return;
        };
        let paths = self
            .preferences
            .expanded
            .entry(context.key.clone())
            .or_default();
        if let Some(index) = paths.iter().position(|path| path == relative_path) {
            paths.remove(index);
        } else {
            paths.push(relative_path.to_string());
            paths.sort();
        }
    }

    pub fn show_hidden(&self) -> bool {
        self.context
            .as_ref()
            .and_then(|context| self.preferences.hidden.get(&context.key))
            .copied()
            .unwrap_or(false)
    }

    pub fn toggle_hidden(&mut self) {
        let Some(context) = &self.context else {
            return;
        };
        let current = self
            .preferences
            .hidden
            .get(&context.key)
            .copied()
            .unwrap_or(false);
        self.preferences
            .hidden
            .insert(context.key.clone(), !current);
        self.load_generation = self.load_generation.wrapping_add(1);
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

    pub fn accept_file_load(&self, generation: u64, context_key: &str) -> bool {
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
    AnyElement, App, AppContext as _, ClipboardItem, Context, Entity, EventEmitter, FocusHandle,
    Focusable, Image, IntoElement, KeyDownEvent, MouseButton, MouseDownEvent, ObjectFit, Render,
    SharedString, Subscription, Task, div, img, prelude::*, px,
};
use zeron_proto::{
    AgentAccountsSnapshot, CheckoutFilesRequest, CheckoutOpRequest, CheckoutStatus,
    CheckoutStatusFile, CommitCheckoutRequest, GetCheckoutStatusRequest,
    WatchCheckoutStatusRequest,
    agent::{WorkflowProgressNode, WorkflowTaskStatus},
};
use zeron_rpc::methods;

use crate::{
    composer::{Composer, ComposerInput, ComposerInputEvent},
    details_sidebar::{
        chat_workers::{
            ChatActivityRow, ChatWorkerRow, ChatWorkersSnapshot, WorkerSemantic,
            activity_tasks_from_entries, compact_activity_label, format_token_total,
            project_chat_workers, snapshot_is_active, worker_compact_metadata,
        },
        context::detect_git_branch,
        file_actions::{
            DeletePrompt, FileClipboard, FileClipboardMode, FileMutation, InlineCreateKind,
            InlineEditState, create_parent_path, path_is_within, retarget_path,
        },
        file_menu::{
            FileActionTooltip, FileCheckoutAccess, FileClipboardPresence, FileMenuItem,
            FileMenuKind, file_menu_items,
        },
        file_tree::{
            FileActionError, FileNode, VisibleFileRow, copy_entry, create_entry, delete_entry,
            flatten_visible_rows, inline_create_insert_index, is_denied_relative, move_entry,
            rename_entry, scan_checkout,
        },
        files_view::{file_glyph, material_icon_path},
        recency::{FileRecency, RECENCY_TICK, RecencyLevel},
        source_control,
        subagent_avatars::blobatar_subagent_avatar_path,
        todos::{latest_todos, todo_status_layout, todo_viewport_height_px},
        usage::{
            ProviderUsageRow, ProviderUsageState, UsageTone, provider_usage_rows,
            usage_provider_icon,
        },
        widgets::{
            CHAT_WORKERS_ROW_HEIGHT, ChatWorkersTab, ChatWorkersWidgetState,
            chat_workers_viewport_height_px, property_row, property_row_custom, widget_card,
            worker_expansion_key, workers_tab_presence,
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

const FILE_SCAN_LIMIT: usize = 5_000;
const RENDERED_FILE_ROW_LIMIT: usize = 800;
/// Debounce between a filesystem event and the silent rescan it triggers. The
/// rescan keeps the current tree on screen: flipping to `Loading` on every
/// save flashed the pane empty.
const RECENCY_REFRESH_DEBOUNCE: std::time::Duration = std::time::Duration::from_millis(500);
const USAGE_TICK: std::time::Duration = std::time::Duration::from_secs(30);
const USAGE_FETCH_INTERVAL: std::time::Duration = std::time::Duration::from_secs(120);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ContextFileAccess {
    Local,
    Remote,
    WaitingForDevice,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct FileMenuTarget {
    path: String,
    is_dir: bool,
    is_root: bool,
    position: gpui::Point<gpui::Pixels>,
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
    CloseFile {
        context_key: String,
        relative_path: String,
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
/// status instead of a tick.
fn settled_success_badge(theme: &Theme) -> AnyElement {
    div()
        .size(px(15.0))
        .flex_none()
        .rounded_full()
        .border_1()
        .border_color(theme.success)
        .flex()
        .items_center()
        .justify_center()
        .child(
            icons::icon(icons::CHECK)
                .size(px(9.0))
                .text_color(theme.success),
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
    files: LoadState<Vec<FileNode>>,
    directory_cache: super::file_tree::DirectoryCache,
    workspace_watch: Option<Task<()>>,
    workspace_watch_key: Option<String>,
    usage: LoadState<Vec<ProviderUsageRow>>,
    usage_snapshot: Option<AgentAccountsSnapshot>,
    usage_fetched_at: Option<std::time::Instant>,
    search: Entity<ComposerInput>,
    search_visible: bool,
    widgets_menu: popover::Popup<()>,
    file_menu: popover::Popup<FileMenuTarget>,
    files_focus: FocusHandle,
    file_clipboard: Option<FileClipboard>,
    inline_edit: Option<InlineEditState>,
    inline_input: Option<Entity<ComposerInput>>,
    inline_events: Option<Subscription>,
    delete_prompt: Option<DeletePrompt>,
    file_mutation_task: Option<Task<()>>,
    file_mutation_error: Option<SharedString>,
    active_file: Option<String>,
    usage_expanded: std::collections::HashSet<String>,
    material_icons: std::collections::HashMap<SharedString, std::sync::Arc<Image>>,
    resolved_branch: Option<String>,
    /// Memoized `<cwd>/.git` probe: without it the Workspace widget stats the
    /// disk on every render. Re-probed when the context's cwd changes.
    /// ponytail: a `git init` under an unchanged context is not noticed until
    /// the sidebar switches context; add an fs watch if that ever matters.
    has_git_dir: Option<(std::path::PathBuf, bool)>,
    file_task: Option<Task<()>>,
    branch_task: Option<Task<()>>,
    recency: FileRecency,
    recency_root: Option<std::path::PathBuf>,
    recency_watch: Option<Task<()>>,
    recency_tick: Option<Task<()>>,
    recency_ticking: bool,
    recency_refresh: Option<Task<()>>,
    usage_task: Option<Task<()>>,
    usage_tick: Option<Task<()>>,
    recap_task: Option<Task<()>>,
    recap_armed_epoch: Option<(String, usize)>,
    failed_epochs: std::collections::HashMap<String, usize>,
    _state_observe: Subscription,
    _workers_observe: Subscription,
    _pickers_observe: Subscription,
    _composer_observe: Subscription,
    _search_events: Subscription,
    checkout_status: Option<CheckoutStatus>,
    checkout_not_git: bool,
    checkout_status_error: Option<SharedString>,
    source_control_watch: Option<Task<()>>,
    source_control_watch_key: Option<String>,
    source_control_fetch: Option<Task<()>>,
    source_control_op: Option<Task<()>>,
    source_control_busy: bool,
    source_control_op_error: Option<SharedString>,
    commit_input: Entity<ComposerInput>,
    _commit_events: Subscription,
    discard_prompt: Option<source_control::DiscardPrompt>,
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
        let search = cx.new(|cx| ComposerInput::with_context("Search files…", "PaletteSearch", cx));
        let search_events = cx.subscribe(&search, |this, _, event, cx| {
            if matches!(event, ComposerInputEvent::Edited) {
                this.reload_files(cx);
            }
        });
        let commit_input = cx.new(|cx| ComposerInput::new("Commit message", cx).with_single_line());
        let commit_events = cx.subscribe(&commit_input, |this, _, event, cx| {
            if matches!(event, ComposerInputEvent::Edited) {
                cx.notify();
            }
            if matches!(event, ComposerInputEvent::Submitted) {
                this.commit_checkout(cx);
            }
        });
        let state_observe = cx.observe(&app_state, |this, state, cx| {
            let (engine_connected, local_device_id) = {
                let state = state.read(cx);
                (state.engine().is_some(), state.local_device_id.clone())
            };
            if engine_connected && matches!(this.usage, LoadState::Idle | LoadState::Error(_)) {
                this.load_usage(cx);
            }
            let local_files_became_available = matches!(this.files, LoadState::Error(_))
                && this.sidebar.context().is_some_and(|context| {
                    context_file_access(context, local_device_id.as_deref())
                        == ContextFileAccess::Local
                });
            if local_files_became_available {
                this.reload_files(cx);
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
                        this.reload_files(cx);
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
            files: LoadState::Idle,
            directory_cache: super::file_tree::DirectoryCache::default(),
            workspace_watch: None,
            workspace_watch_key: None,
            usage: LoadState::Idle,
            usage_snapshot: None,
            usage_fetched_at: None,
            search,
            search_visible: false,
            widgets_menu: popover::Popup::default(),
            file_menu: popover::Popup::default(),
            files_focus: cx.focus_handle(),
            file_clipboard: None,
            inline_edit: None,
            inline_input: None,
            inline_events: None,
            delete_prompt: None,
            file_mutation_task: None,
            file_mutation_error: None,
            active_file: None,
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
            source_control_op_error: None,
            commit_input,
            _commit_events: commit_events,
            discard_prompt: None,
            file_task: None,
            branch_task: None,
            usage_task: None,
            usage_tick: None,
            recency: FileRecency::default(),
            recency_root: None,
            recency_watch: None,
            recency_tick: None,
            recency_ticking: false,
            recency_refresh: None,
            recap_task: None,
            recap_armed_epoch: None,
            failed_epochs: std::collections::HashMap::new(),
            _state_observe: state_observe,
            _workers_observe: workers_observe,
            _pickers_observe: pickers_observe,
            _composer_observe: composer_observe,
            _search_events: search_events,
        };
        sidebar.load_usage(cx);
        sidebar.ensure_usage_tick(cx);
        sidebar
    }

    pub fn set_active_file(&mut self, relative_path: Option<String>, cx: &mut Context<Self>) {
        if self.active_file != relative_path {
            self.active_file = relative_path;
            cx.notify();
        }
    }

    pub fn set_context(&mut self, context: Option<DetailsContext>, cx: &mut Context<Self>) {
        let before = self.sidebar.load_generation();
        let after = self.sidebar.set_context(context);
        if before != after {
            self.stop_recency_watch();
            self.directory_cache = super::file_tree::DirectoryCache::default();
            self.workspace_watch = None;
            self.workspace_watch_key = None;
            self.file_task = None;
            self.files = LoadState::Idle;
            self.chat_workers
                .sync_context(self.sidebar.context().map(|context| context.key.as_str()));
            self.active_file = None;
            self.file_clipboard = None;
            self.delete_prompt = None;
            self.inline_edit = None;
            self.inline_input = None;
            self.file_mutation_error = None;
            self.source_control_watch = None;
            self.source_control_watch_key = None;
            self.source_control_fetch = None;
            self.source_control_op = None;
            self.checkout_status = None;
            self.checkout_not_git = false;
            self.checkout_status_error = None;
            self.source_control_op_error = None;
            self.source_control_busy = false;
            self.discard_prompt = None;
            self.commit_input
                .update(cx, |input, cx| input.set_text("", cx));
            self.resolved_branch = self
                .sidebar
                .context()
                .and_then(|value| value.branch.clone());
            self.reload_files(cx);
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

    fn set_tab(&mut self, tab: DetailsTab, cx: &mut Context<Self>) {
        if self.sidebar.tab() == tab {
            return;
        }
        self.sidebar.set_tab(tab);
        self.emit_preferences(cx);
        self.ensure_source_control_watch(cx);
        cx.notify();
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

    fn commit_checkout(&mut self, cx: &mut Context<Self>) {
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

    fn render_source_control(&mut self, theme: &Theme, cx: &mut Context<Self>) -> gpui::Div {
        self.ensure_source_control_watch(cx);
        let mut content = div()
            .size_full()
            .flex()
            .flex_col()
            .p(px(12.0))
            .gap(px(12.0));
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
        let mut header = div().flex().items_center().gap(px(8.0)).child(
            div()
                .min_w_0()
                .flex_1()
                .truncate()
                .text_size(px(13.0))
                .font_weight(gpui::FontWeight::MEDIUM)
                .child(branch),
        );
        if status.ahead > 0 {
            header = header.child(
                div()
                    .text_size(px(11.0))
                    .text_color(theme.text_muted)
                    .child(format!("↑{}", status.ahead)),
            );
        }
        if status.behind > 0 {
            header = header.child(
                div()
                    .text_size(px(11.0))
                    .text_color(theme.text_muted)
                    .child(format!("↓{}", status.behind)),
            );
        }
        content = content.child(header);
        content = content.child(
            div()
                .h(px(36.0))
                .flex_none()
                .px(px(8.0))
                .rounded(px(8.0))
                .border_1()
                .border_color(theme.border)
                .flex()
                .items_center()
                .child(self.commit_input.clone()),
        );
        let mut commit_btn = popover::btn_primary(theme, "Commit")
            .id("details-source-control-commit")
            .opacity(if can_commit { 1.0 } else { 0.4 });
        if can_commit {
            commit_btn =
                commit_btn.on_click(cx.listener(|this, _, _, cx| this.commit_checkout(cx)));
        }
        let mut sync_btn = popover::btn_ghost(theme, sync_label, "details-source-control-sync")
            .id("details-source-control-sync")
            .opacity(if busy { 0.4 } else { 1.0 });
        if !busy {
            sync_btn =
                sync_btn.on_click(cx.listener(|this, _, _, cx| this.sync_or_publish_checkout(cx)));
        }
        content = content.child(
            div()
                .flex()
                .items_center()
                .justify_end()
                .gap(px(8.0))
                .child(sync_btn)
                .child(commit_btn),
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
        content = content.child(self.render_source_control_section(
            theme,
            "Staged Changes",
            &staged,
            &staged_files,
            SourceControlSection::Staged,
            cx,
        ));
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
        let bulk_paths: Vec<String> = rows.iter().map(|row| row.path.clone()).collect();
        let bulk_files = files.to_vec();
        let mut header = div().flex().items_center().justify_between().child(
            div()
                .text_size(px(11.0))
                .font_weight(gpui::FontWeight::MEDIUM)
                .text_color(theme.text_muted)
                .child(format!("{title} ({})", rows.len())),
        );
        if !rows.is_empty() && !self.source_control_busy {
            let mut actions = div().flex().items_center().gap(px(8.0));
            match section {
                SourceControlSection::Staged => {
                    let paths = bulk_paths.clone();
                    actions = actions.child(
                        div()
                            .id("details-source-control-unstage-all")
                            .text_size(px(11.0))
                            .text_color(theme.text_muted)
                            .cursor_pointer()
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.unstage_paths(paths.clone(), cx)
                            }))
                            .child("Unstage All"),
                    );
                }
                SourceControlSection::Changes => {
                    let paths = bulk_paths.clone();
                    actions = actions.child(
                        div()
                            .id("details-source-control-stage-all")
                            .text_size(px(11.0))
                            .text_color(theme.text_muted)
                            .cursor_pointer()
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.stage_paths(paths.clone(), cx)
                            }))
                            .child("Stage All"),
                    );
                }
            }
            let discard_id = if matches!(section, SourceControlSection::Staged) {
                "details-source-control-discard-all-staged"
            } else {
                "details-source-control-discard-all-changes"
            };
            actions = actions.child(
                div()
                    .id(discard_id)
                    .text_size(px(11.0))
                    .text_color(theme.danger)
                    .cursor_pointer()
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.request_discard(bulk_files.clone(), cx)
                    }))
                    .child("Discard All"),
            );
            header = header.child(actions);
        }
        let mut list = div().flex().flex_col().gap(px(2.0)).child(header);
        if rows.is_empty() {
            return list.child(
                div()
                    .text_size(px(12.0))
                    .text_color(theme.text_muted)
                    .child("No files"),
            );
        }
        for (index, row) in rows.iter().enumerate() {
            let path = row.path.clone();
            let open_path = row.path.clone();
            let letter = row.letter.to_string();
            let label = match &row.old_path {
                Some(old) => format!("{old} → {}", row.path),
                None => row.path.clone(),
            };
            let file = files.iter().find(|file| file.path == row.path).cloned();
            let section_tag = match section {
                SourceControlSection::Staged => "staged",
                SourceControlSection::Changes => "changes",
            };
            let mut row_el = div()
                .id((
                    SharedString::from(format!("details-source-control-row-{section_tag}")),
                    index,
                ))
                .h(px(24.0))
                .flex()
                .items_center()
                .gap(px(8.0))
                .child(
                    div()
                        .w(px(12.0))
                        .flex_none()
                        .font_family(theme.font_mono.clone())
                        .text_size(px(11.0))
                        .text_color(theme.text_muted)
                        .child(letter),
                )
                .child(
                    div()
                        .id(("details-source-control-file", index))
                        .flex_1()
                        .min_w_0()
                        .truncate()
                        .text_size(px(12.0))
                        .cursor_pointer()
                        .on_click(cx.listener(move |_, _, _, cx| {
                            cx.emit(DetailsSidebarEvent::OpenWorkingTreeDiff {
                                path: open_path.clone(),
                            });
                        }))
                        .child(label),
                );
            if !self.source_control_busy {
                match section {
                    SourceControlSection::Staged => {
                        let unstage_path = path.clone();
                        row_el = row_el.child(
                            div()
                                .id(("details-source-control-unstage", index))
                                .text_size(px(11.0))
                                .text_color(theme.text_muted)
                                .cursor_pointer()
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.unstage_paths(vec![unstage_path.clone()], cx)
                                }))
                                .child("Unstage"),
                        );
                    }
                    SourceControlSection::Changes => {
                        let stage_path = path.clone();
                        row_el = row_el.child(
                            div()
                                .id(("details-source-control-stage", index))
                                .text_size(px(11.0))
                                .text_color(theme.text_muted)
                                .cursor_pointer()
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.stage_paths(vec![stage_path.clone()], cx)
                                }))
                                .child("Stage"),
                        );
                    }
                }
                if let Some(file) = file {
                    row_el = row_el.child(
                        div()
                            .id(("details-source-control-discard", index))
                            .text_size(px(11.0))
                            .text_color(theme.danger)
                            .cursor_pointer()
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.request_discard(vec![file.clone()], cx)
                            }))
                            .child("Discard"),
                    );
                }
            }
            list = list.child(row_el);
        }
        list
    }

    fn render_discard_dialog(
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

    fn reload_files(&mut self, cx: &mut Context<Self>) {
        self.load_files(false, cx);
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

    /// Rescan without flipping to `Loading`, so a watcher-driven refresh keeps
    /// the current rows (and their recency tint) visible while it runs.
    fn refresh_files(&mut self, cx: &mut Context<Self>) {
        self.load_files(true, cx);
    }

    fn load_files(&mut self, silent: bool, cx: &mut Context<Self>) {
        if self.workspace_file_source(cx).is_some() {
            self.load_workspace_files(silent, None, None, cx);
            return;
        }
        let Some(context) = self.sidebar.context().cloned() else {
            self.files = LoadState::Idle;
            return;
        };
        let generation = self.sidebar.load_generation();
        let context_key = context.key.clone();
        let local_device_id = self.app_state.read(cx).local_device_id.clone();
        match context_file_access(&context, local_device_id.as_deref()) {
            ContextFileAccess::Local => {}
            ContextFileAccess::Remote => {
                self.files = LoadState::Error(
                    "Files are unavailable for projects hosted on another device.".into(),
                );
                self.file_task = None;
                self.stop_recency_watch();
                cx.notify();
                return;
            }
            ContextFileAccess::WaitingForDevice => {
                self.files = LoadState::Error("Waiting for the project device connection…".into());
                self.file_task = None;
                self.stop_recency_watch();
                cx.notify();
                return;
            }
        }
        self.ensure_recency_watch(context.cwd.clone(), cx);
        let show_hidden = self.sidebar.show_hidden();
        let query = self.search.read(cx).text().to_string();
        if !silent {
            self.files = LoadState::Loading;
        }
        self.file_task = Some(cx.spawn(async move |this, cx| {
            let result =
                cx.background_executor()
                    .spawn(async move {
                        scan_checkout(&context.cwd, show_hidden, &query, FILE_SCAN_LIMIT)
                    })
                    .await;
            let _ = this.update(cx, |this, cx| {
                if !this.sidebar.accept_file_load(generation, &context_key) {
                    return;
                }
                this.files = match result {
                    Ok(files) => LoadState::Ready(files),
                    Err(error) => LoadState::Error(format!("{error:?}").into()),
                };
                cx.notify();
            });
        }));
        cx.notify();
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

    fn load_workspace_files(
        &mut self,
        silent: bool,
        only: Option<Vec<String>>,
        append: Option<String>,
        cx: &mut Context<Self>,
    ) {
        let Some((engine, target, device)) = self.workspace_file_source(cx) else {
            return;
        };
        let Some(context) = self.sidebar.context().cloned() else {
            return;
        };
        self.ensure_workspace_watch(
            context.key.clone(),
            engine.clone(),
            target.clone(),
            device.clone(),
            cx,
        );
        if self.recency_root.is_some() {
            self.stop_recency_watch();
        }
        let generation = self.sidebar.load_generation();
        let context_key = context.key;
        let show_hidden = self.sidebar.show_hidden();
        let query = self.search.read(cx).text().trim().to_string();
        let mut cache = self.directory_cache.clone();
        let mut directories = only.unwrap_or_else(|| {
            let mut dirs = cache.loaded_directories();
            dirs.push(String::new());
            dirs
        });
        if !cache.contains("") {
            directories.push(String::new());
        }
        // A rapid second expansion can cancel the previous fetch; include all
        // still-missing expanded directories so neither click gets lost.
        directories.extend(
            self.sidebar
                .expanded_paths()
                .into_iter()
                .filter(|path| !cache.contains(path)),
        );
        directories.sort();
        directories.dedup();
        if !silent && !matches!(self.files, LoadState::Ready(_)) {
            self.files = LoadState::Loading;
        }
        self.file_task = Some(cx.spawn(async move |this, cx| {
            let result: Result<Vec<FileNode>, zeron_rpc::RpcError> = async {
                if !query.is_empty() {
                    let request = zeron_proto::SearchWorkspaceFilesRequest {
                        target: target.clone(),
                        query: query.clone(),
                        include_ignored: true,
                        limit: Some(200),
                    };
                    let mut params = serde_json::to_value(request).unwrap();
                    params["targetDeviceId"] = device.clone().into();
                    let matches: Vec<zeron_proto::WorkspaceFileSearchMatch> = engine
                        .client()
                        .call_as(zeron_rpc::methods::SEARCH_WORKSPACE_FILES, params)
                        .await?;
                    return Ok(super::file_tree::search_result_nodes(&matches, show_hidden));
                }
                for directory in directories {
                    if !directory.is_empty() && !cache.can_expand(&directory) {
                        continue;
                    }
                    let is_append = append.as_deref() == Some(directory.as_str());
                    let retained = cache.loaded_count(&directory).max(500);
                    let mut cursor = if is_append {
                        cache.cursor(&directory).map(str::to_owned)
                    } else {
                        None
                    };
                    let mut page = zeron_proto::WorkspaceDirectoryPage {
                        directory: directory.clone(),
                        entries: Vec::new(),
                        next_cursor: None,
                        truncated: false,
                    };
                    loop {
                        let request = zeron_proto::ListWorkspaceDirectoryRequest {
                            target: target.clone(),
                            directory: directory.clone(),
                            include_ignored: true,
                            cursor,
                        };
                        let mut params = serde_json::to_value(request).unwrap();
                        params["targetDeviceId"] = device.clone().into();
                        let next: zeron_proto::WorkspaceDirectoryPage = engine
                            .client()
                            .call_as(zeron_rpc::methods::LIST_WORKSPACE_DIRECTORY, params)
                            .await?;
                        if next.directory != directory {
                            return Err(zeron_rpc::RpcError::Failed(
                                "Files returned a different directory".into(),
                            ));
                        }
                        page.entries.extend(next.entries);
                        page.next_cursor = next.next_cursor;
                        page.truncated = next.truncated;
                        if is_append || page.entries.len() >= retained || page.next_cursor.is_none()
                        {
                            break;
                        }
                        cursor = page.next_cursor.clone();
                    }
                    cache.apply(page, is_append);
                }
                Ok(cache.nodes(show_hidden))
            }
            .await;
            let _ = this.update(cx, |this, cx| {
                if !this.sidebar.accept_file_load(generation, &context_key)
                    || this.search.read(cx).text().trim() != query
                {
                    return;
                }
                match result {
                    Ok(nodes) => {
                        let changed = this.directory_cache != cache
                            || !matches!(&this.files, LoadState::Ready(old) if old == &nodes);
                        this.directory_cache = cache;
                        this.files = LoadState::Ready(nodes);
                        if changed {
                            cx.notify();
                        }
                    }
                    Err(error) => {
                        let message: SharedString = match error {
                            zeron_rpc::RpcError::UnknownMethod(_) => {
                                "Update the project device to enable Files browsing.".into()
                            }
                            other => format!("Files: {other}").into(),
                        };
                        let changed =
                            !matches!(&this.files, LoadState::Error(old) if old == &message);
                        this.files = LoadState::Error(message);
                        if changed {
                            cx.notify();
                        }
                    }
                }
            });
        }));
        if !silent {
            cx.notify();
        }
    }

    fn ensure_workspace_watch(
        &mut self,
        key: String,
        engine: crate::state::EngineHandle,
        target: zeron_proto::WorkspaceTarget,
        device: String,
        cx: &mut Context<Self>,
    ) {
        if self.workspace_watch_key.as_ref() == Some(&key) {
            return;
        }
        self.workspace_watch_key = Some(key.clone());
        self.workspace_watch = Some(cx.spawn(async move |this, cx| {
            loop {
                let mut params = serde_json::to_value(zeron_proto::WatchWorkspaceFilesRequest {
                    target: target.clone(),
                })
                .unwrap();
                params["targetDeviceId"] = device.clone().into();
                let mut last_sequence = None;
                let subscription = engine
                    .client()
                    .subscribe(zeron_rpc::methods::WATCH_WORKSPACE_FILES, params)
                    .await;
                if matches!(&subscription, Err(zeron_rpc::RpcError::UnknownMethod(_))) {
                    return;
                }
                if let Ok(mut stream) = subscription {
                    while let Some(value) = stream.recv().await {
                        let Ok(batch) =
                            serde_json::from_value::<zeron_proto::WorkspaceFileChanges>(value)
                        else {
                            continue;
                        };
                        let initial = last_sequence.is_none();
                        let gap = last_sequence.is_some_and(|last| batch.sequence != last + 1);
                        last_sequence = Some(batch.sequence);
                        if this
                            .update(cx, |this, cx| {
                                if this.sidebar.context().map(|c| c.key.as_str())
                                    != Some(key.as_str())
                                {
                                    return;
                                }
                                if initial || gap || batch.resync_required {
                                    this.load_workspace_files(true, None, None, cx);
                                    return;
                                }
                                let mut parents = std::collections::BTreeSet::new();
                                let now = std::time::Instant::now();
                                for change in batch.changes {
                                    for path in std::iter::once(change.path).chain(change.old_path)
                                    {
                                        if super::file_tree::is_denied_relative(
                                            std::path::Path::new(&path),
                                        ) {
                                            continue;
                                        }
                                        this.recency.mark(path.clone(), now);
                                        if let Some(parent) = std::path::Path::new(&path)
                                            .parent()
                                            .and_then(|p| p.to_str())
                                        {
                                            if this.directory_cache.contains(parent) {
                                                parents.insert(parent.to_string());
                                            }
                                        }
                                    }
                                }
                                if !parents.is_empty() {
                                    this.ensure_recency_tick(cx);
                                    cx.notify();
                                    this.load_workspace_files(
                                        true,
                                        Some(parents.into_iter().collect()),
                                        None,
                                        cx,
                                    );
                                }
                            })
                            .is_err()
                        {
                            return;
                        }
                    }
                }
                if this
                    .update(cx, |this, cx| {
                        this.load_workspace_files(true, None, None, cx)
                    })
                    .is_err()
                {
                    return;
                }
                cx.background_executor()
                    .timer(std::time::Duration::from_secs(2))
                    .await;
            }
        }));
    }

    fn stop_recency_watch(&mut self) {
        self.recency_root = None;
        self.recency_watch = None;
        self.recency_refresh = None;
        self.recency.clear();
    }

    /// Watches the pane's root for changes so rows can advertise recency. The
    /// tree carries no timestamps, so the mark is the instant the event
    /// arrived — never the file's mtime.
    fn ensure_recency_watch(&mut self, root: std::path::PathBuf, cx: &mut Context<Self>) {
        if self.recency_root.as_deref() == Some(root.as_path()) {
            return;
        }
        self.recency_root = Some(root.clone());
        self.recency.clear();
        self.recency_refresh = None;
        self.recency_watch = Some(cx.spawn(async move |this, cx| {
            let (events_tx, mut events_rx) =
                futures::channel::mpsc::unbounded::<Vec<std::path::PathBuf>>();
            let watch_root = root.clone();
            // fsevents reports canonical paths (/private/var over /var), so the
            // marks are stripped against the canonical root or nothing matches.
            let canonical_root = cx
                .background_executor()
                .spawn(async move { watch_root.canonicalize().unwrap_or(watch_root) })
                .await;
            let watch_root = canonical_root.clone();
            let watcher = cx
                .background_executor()
                .spawn(async move {
                    let mut watcher =
                        notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
                            let Ok(event) = event else {
                                return;
                            };
                            if !matches!(
                                event.kind,
                                notify::EventKind::Create(_)
                                    | notify::EventKind::Modify(_)
                                    | notify::EventKind::Remove(_)
                            ) {
                                return;
                            }
                            let _ = events_tx.unbounded_send(event.paths);
                        })
                        .ok()?;
                    notify::Watcher::watch(
                        &mut watcher,
                        &watch_root,
                        notify::RecursiveMode::Recursive,
                    )
                    .ok()?;
                    Some(watcher)
                })
                .await;
            // The watcher lives exactly as long as this task: dropping the
            // task (context switch, remote project) stops the watch.
            let Some(_watcher) = watcher else {
                return;
            };
            while let Some(paths) = futures::StreamExt::next(&mut events_rx).await {
                let marks: Vec<String> = paths
                    .iter()
                    .filter_map(|path| {
                        let relative = path.strip_prefix(&canonical_root).ok()?;
                        if relative.as_os_str().is_empty() || is_denied_relative(relative) {
                            return None;
                        }
                        Some(
                            relative
                                .components()
                                .filter_map(|component| match component {
                                    std::path::Component::Normal(value) => value.to_str(),
                                    _ => None,
                                })
                                .collect::<Vec<_>>()
                                .join("/"),
                        )
                    })
                    .filter(|relative| !relative.is_empty())
                    .collect();
                if marks.is_empty() {
                    continue;
                }
                let alive = this
                    .update(cx, |this, cx| {
                        let now = std::time::Instant::now();
                        for relative in marks {
                            this.recency.mark(relative, now);
                        }
                        this.ensure_recency_tick(cx);
                        this.schedule_recency_refresh(cx);
                        cx.notify();
                    })
                    .is_ok();
                if !alive {
                    break;
                }
            }
        }));
    }

    /// Re-evaluates tiers and prunes expired marks while any mark is alive.
    fn ensure_recency_tick(&mut self, cx: &mut Context<Self>) {
        if self.recency_ticking {
            return;
        }
        self.recency_ticking = true;
        self.recency_tick = Some(cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(RECENCY_TICK).await;
                let keep_ticking = this.update(cx, |this, cx| {
                    let pruned = this.recency.prune(std::time::Instant::now());
                    let remaining = !this.recency.is_empty();
                    // Notify on the emptying tick too, otherwise the last
                    // faded row keeps a stale tint until an unrelated render.
                    if pruned || remaining {
                        cx.notify();
                    }
                    remaining
                });
                if !matches!(keep_ticking, Ok(true)) {
                    break;
                }
            }
            let _ = this.update(cx, |this, _| {
                this.recency_ticking = false;
            });
        }));
    }

    /// Coalesces a burst of filesystem events into one silent rescan, so a new
    /// file appears as a row and not only as a mark.
    fn schedule_recency_refresh(&mut self, cx: &mut Context<Self>) {
        self.recency_refresh = Some(cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(RECENCY_REFRESH_DEBOUNCE)
                .await;
            let _ = this.update(cx, |this, cx| {
                this.refresh_files(cx);
            });
        }));
    }

    fn ensure_usage_tick(&mut self, cx: &mut Context<Self>) {
        if self.usage_tick.is_some() {
            return;
        }
        self.usage_tick = Some(cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(USAGE_TICK).await;
                let keep_ticking = this.update(cx, |this, cx| {
                    if let Some(snapshot) = &this.usage_snapshot {
                        this.usage = LoadState::Ready(provider_usage_rows(
                            snapshot,
                            &crate::settings::current(cx).usage_widget_hidden_account_ids,
                            chrono::Utc::now(),
                        ));
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
                if this.sidebar.accept_file_load(generation, &context_key) {
                    this.resolved_branch = branch;
                    cx.notify();
                }
            });
        }));
    }

    fn all_folder_paths(nodes: &[FileNode], output: &mut Vec<String>) {
        for node in nodes {
            if node.is_dir {
                output.push(node.relative_path.clone());
                Self::all_folder_paths(&node.children, output);
            }
        }
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
        let tab = self.sidebar.tab();
        let pill = |label: &'static str, active: bool| {
            div()
                .h(px(28.0))
                .px(px(14.0))
                .rounded(px(7.0))
                .flex()
                .items_center()
                .justify_center()
                .cursor_pointer()
                .bg(if active {
                    // Translucent active plate (was opaque theme.bg): reads
                    // active over the glass without a solid black slab.
                    theme.bg.opacity(0.6)
                } else {
                    gpui::transparent_black()
                })
                .text_size(px(12.5))
                .font_weight(gpui::FontWeight::MEDIUM)
                .text_color(if active { theme.text } else { theme.text_muted })
                .child(label)
        };
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
                            .p(px(2.0))
                            .rounded(px(9.0))
                            .bg(crate::theme::ink(0.03))
                            .flex()
                            .items_center()
                            .child(
                                pill("Details", tab == DetailsTab::Details)
                                    .id("details-tab")
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.set_tab(DetailsTab::Details, cx)
                                    })),
                            )
                            .child(
                                pill("Files", tab == DetailsTab::Files)
                                    .id("files-tab")
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.set_tab(DetailsTab::Files, cx)
                                    })),
                            )
                            .child({
                                let badge = self
                                    .checkout_status
                                    .as_ref()
                                    .map(|status| source_control::change_badge(&status.files))
                                    .unwrap_or(0);
                                let mut tab_pill =
                                    pill("Source Control", tab == DetailsTab::SourceControl)
                                        .id("source-control-tab")
                                        .gap(px(6.0))
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            this.set_tab(DetailsTab::SourceControl, cx)
                                        }));
                                if badge > 0 {
                                    tab_pill = tab_pill.child(
                                        div()
                                            .px(px(6.0))
                                            .rounded(px(8.0))
                                            .bg(crate::theme::ink(0.08))
                                            .text_size(px(10.0))
                                            .text_color(theme.text_muted)
                                            .child(format!("{badge}")),
                                    );
                                }
                                tab_pill
                            }),
                    ),
            )
            .when(tab == DetailsTab::Files, |header| {
                header.child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(2.0))
                        .child(self.toolbar_button(
                            "details-hidden-toggle",
                            if self.sidebar.show_hidden() {
                                icons::DETAILS_EYE
                            } else {
                                icons::DETAILS_EYE_OFF
                            },
                            theme,
                            cx.listener(|this, _, _, cx| {
                                this.sidebar.toggle_hidden();
                                this.emit_preferences(cx);
                                this.reload_files(cx);
                            }),
                        ))
                        .child(self.toolbar_button_with_tooltip(
                            "details-new-file",
                            icons::DOCUMENT_ADD,
                            "New File",
                            theme,
                            cx.listener(|this, _, window, cx| {
                                this.start_inline_create(InlineCreateKind::File, window, cx);
                            }),
                        ))
                        .child(self.toolbar_button_with_tooltip(
                            "details-new-folder",
                            icons::FOLDER_WITH_FILES,
                            "New Folder",
                            theme,
                            cx.listener(|this, _, window, cx| {
                                this.start_inline_create(InlineCreateKind::Directory, window, cx);
                            }),
                        ))
                        .child(self.toolbar_button(
                            "details-search-toggle",
                            icons::MAGNIFER,
                            theme,
                            cx.listener(|this, _, window, cx| {
                                this.search_visible = !this.search_visible;
                                if this.search_visible {
                                    window.focus(&this.search.read(cx).focus_handle(cx), cx);
                                }
                                cx.notify();
                            }),
                        ))
                        .child(self.toolbar_button(
                            "details-expand-all",
                            icons::FOLD_VERTICAL,
                            theme,
                            cx.listener(|this, _, _, cx| {
                                let folders = match &this.files {
                                    LoadState::Ready(files) => {
                                        let mut folders = Vec::new();
                                        Self::all_folder_paths(files, &mut folders);
                                        folders
                                    }
                                    _ => Vec::new(),
                                };
                                let all_expanded = folders
                                    .iter()
                                    .all(|path| this.sidebar.expanded_paths().contains(path));
                                for path in folders {
                                    if this.sidebar.expanded_paths().contains(&path) == all_expanded
                                    {
                                        this.sidebar.toggle_expanded(&path);
                                    }
                                }
                                this.emit_preferences(cx);
                                cx.notify();
                            }),
                        )),
                )
            })
            .when(tab == DetailsTab::Details, |header| {
                header.child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(2.0))
                        .child(self.render_widgets_gear(theme, cx)),
                )
            })
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
                cx.new(|_| FileActionTooltip {
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
            WorkerSemantic::Idle => div()
                .size(px(7.0))
                .rounded_full()
                .bg(theme.text_muted.opacity(0.65))
                .into_any_element(),
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
        div()
            .id(SharedString::from(format!("chat-subagent-{}", row.id)))
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
        let model_usage = worker.model_usage.clone();
        let subtitle = if let Some((model, total)) = compact_metadata {
            div()
                .min_w_0()
                .flex()
                .items_center()
                .gap(px(6.0))
                .text_size(px(10.0))
                .text_color(theme.text_muted)
                .child(div().min_w_0().flex_1().truncate().child(model))
                .child(div().flex_none().child(total))
        } else {
            div()
                .truncate()
                .text_size(px(10.0))
                .text_color(theme.text_muted)
                .child(worker.command.clone())
        };
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
        let workers = snapshot.workers.len();
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
        self.chat_workers.sync_dispatch_with_recency(
            Some(workflows),
            Some(subagents),
            workers_error.is_none().then_some(workers),
            latest_started,
        );
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
                subagents,
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
        let body = match active {
            ChatWorkersTab::Workflows if workflows > 0 => div().children(
                snapshot
                    .workflows
                    .into_iter()
                    .map(|row| self.render_workflow_row(row, theme, cx)),
            ),
            ChatWorkersTab::Subagents if subagents > 0 => div().children(
                snapshot
                    .subagents
                    .into_iter()
                    .map(|row| self.render_subagent_row(row, chat_id.clone(), theme, cx)),
            ),
            ChatWorkersTab::Workers if workers > 0 => div().children(
                snapshot
                    .workers
                    .into_iter()
                    .map(|worker| self.render_worker_row(worker, chat_id.clone(), theme, cx)),
            ),
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

        let mut workspace_body = div()
            .child(property_row_custom(
                icons::GIT_BRANCH,
                "Branch",
                branch_control,
                theme,
            ))
            .child(property_row(icons::FOLDER, "Path", folder, theme));

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
                                .child(
                                    icons::icon(icons::FOLDER)
                                        .size(px(14.0))
                                        .text_color(theme.text_muted),
                                )
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
            }
        }
        let mut content = div().w_full().flex().flex_col().gap(px(10.0)).p(px(10.0));
        if !hide_workspace {
            content = content.child(widget_card(
                "workspace-widget",
                icons::DETAILS_BOX,
                "Workspace",
                workspace_body,
                theme,
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
        let summary: SharedString = match row.state {
            ProviderUsageState::Ready => row
                .weekly_summary
                .clone()
                .unwrap_or_else(|| "—".into())
                .into(),
            ProviderUsageState::NoUsage => "No usage yet".into(),
            ProviderUsageState::NotSignedIn => "Not signed in".into(),
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
                                    .flex_none()
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

    fn close_file_menu(&mut self, cx: &mut Context<Self>) {
        if self.file_menu.begin_close() {
            popover::reap_popup(cx, |this: &mut Self| &mut this.file_menu);
        }
    }

    fn open_file_menu(&mut self, target: FileMenuTarget, cx: &mut Context<Self>) {
        self.file_menu.open(target);
        cx.notify();
    }

    fn files_access(&self, cx: &App) -> FileCheckoutAccess {
        let Some(context) = self.sidebar.context() else {
            return FileCheckoutAccess::Local;
        };
        match context_file_access(context, self.app_state.read(cx).local_device_id.as_deref()) {
            ContextFileAccess::Local => FileCheckoutAccess::Local,
            ContextFileAccess::Remote | ContextFileAccess::WaitingForDevice => {
                FileCheckoutAccess::Remote
            }
        }
    }

    fn selected_file_path(&self) -> Option<(String, bool)> {
        let relative = self.active_file.clone()?;
        let is_dir = match &self.files {
            LoadState::Ready(files) => file_node_is_dir(files, &relative),
            _ => false,
        };
        Some((relative, is_dir))
    }

    fn sibling_names(&self, parent: &str) -> Vec<String> {
        let LoadState::Ready(files) = &self.files else {
            return Vec::new();
        };
        sibling_entry_names(files, parent)
    }

    fn start_inline_create(
        &mut self,
        kind: InlineCreateKind,
        window: &mut gpui::Window,
        cx: &mut Context<Self>,
    ) {
        let (selected, is_dir) = self
            .selected_file_path()
            .map(|(path, is_dir)| (Some(path), is_dir))
            .unwrap_or((None, false));
        if let Some(target) = self.file_menu.as_open() {
            let parent = create_parent_path(
                if target.is_root {
                    None
                } else {
                    Some(target.path.as_str())
                },
                target.is_dir,
            );
            self.close_file_menu(cx);
            self.begin_inline_edit(InlineEditState::create(parent, kind), window, cx);
            return;
        }
        let parent = create_parent_path(selected.as_deref(), is_dir);
        if !parent.is_empty() {
            if !self.sidebar.expanded_paths().contains(&parent) {
                self.sidebar.toggle_expanded(&parent);
                self.emit_preferences(cx);
            }
            if self.workspace_file_source(cx).is_some() {
                self.load_workspace_files(true, Some(vec![parent.clone()]), None, cx);
            }
        }
        self.begin_inline_edit(InlineEditState::create(parent, kind), window, cx);
    }

    fn start_inline_rename(
        &mut self,
        path: String,
        window: &mut gpui::Window,
        cx: &mut Context<Self>,
    ) {
        if path.is_empty() {
            return;
        }
        let original = std::path::Path::new(&path)
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or(path.as_str())
            .to_string();
        self.begin_inline_edit(InlineEditState::rename(path, original), window, cx);
    }

    fn begin_inline_edit(
        &mut self,
        state: InlineEditState,
        window: &mut gpui::Window,
        cx: &mut Context<Self>,
    ) {
        let placeholder = match &state.edit {
            super::file_actions::InlineFileEdit::Create { kind, .. } => match kind {
                InlineCreateKind::File => "File name",
                InlineCreateKind::Directory => "Folder name",
            },
            super::file_actions::InlineFileEdit::Rename { .. } => "Name",
        };
        let draft = state.draft.clone();
        let input = cx.new(|cx| {
            let mut input = ComposerInput::new(placeholder, cx).with_single_line();
            input.set_text(draft, cx);
            input
        });
        self.inline_events = Some(cx.subscribe(&input, |this, _, event, cx| {
            if matches!(event, ComposerInputEvent::Submitted) {
                this.confirm_inline_edit(cx);
            }
        }));
        let focus = input.read(cx).focus_handle(cx);
        self.inline_input = Some(input);
        self.inline_edit = Some(state);
        self.delete_prompt = None;
        window.focus(&focus, cx);
        cx.notify();
    }

    fn cancel_inline_edit(&mut self, cx: &mut Context<Self>) {
        self.inline_edit = None;
        self.inline_input = None;
        self.inline_events = None;
        cx.notify();
    }

    fn confirm_inline_edit(&mut self, cx: &mut Context<Self>) {
        let Some(mut state) = self.inline_edit.take() else {
            return;
        };
        if let Some(input) = &self.inline_input {
            state.draft = input.read(cx).text().to_string();
        }
        let parent = match &state.edit {
            super::file_actions::InlineFileEdit::Create { parent, .. } => parent.clone(),
            super::file_actions::InlineFileEdit::Rename { path, .. } => {
                super::file_actions::parent_directory(path)
            }
        };
        let siblings = self.sibling_names(&parent);
        let sibling_refs: Vec<&str> = siblings.iter().map(String::as_str).collect();
        match state.confirm(&sibling_refs) {
            Ok(None) => {
                self.inline_input = None;
                self.inline_events = None;
                cx.notify();
            }
            Ok(Some(mutation)) => {
                self.inline_input = None;
                self.inline_events = None;
                self.execute_file_mutation(mutation, cx);
            }
            Err(error) => {
                state.error = Some(error);
                self.inline_edit = Some(state);
                cx.notify();
            }
        }
    }

    fn request_delete(&mut self, path: String, is_dir: bool, cx: &mut Context<Self>) {
        if path.is_empty() {
            return;
        }
        let name = std::path::Path::new(&path)
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or(path.as_str())
            .to_string();
        self.delete_prompt = Some(DeletePrompt { path, name, is_dir });
        self.close_file_menu(cx);
        cx.notify();
    }

    fn execute_file_mutation(&mut self, mutation: FileMutation, cx: &mut Context<Self>) {
        self.file_mutation_error = None;
        if self.workspace_file_source(cx).is_some() {
            self.execute_rpc_file_mutation(mutation, cx);
        } else {
            self.execute_local_file_mutation(mutation, cx);
        }
    }

    fn execute_local_file_mutation(&mut self, mutation: FileMutation, cx: &mut Context<Self>) {
        let Some(context) = self.sidebar.context().cloned() else {
            return;
        };
        let root = context.cwd.clone();
        let result = match &mutation {
            FileMutation::CreateFile { parent, name } => create_entry(&root, parent, name, false),
            FileMutation::CreateDir { parent, name } => create_entry(&root, parent, name, true),
            FileMutation::Rename { path, new_name } => rename_entry(&root, path, new_name),
            FileMutation::Delete { path } => delete_entry(&root, path).map(|()| path.clone()),
            FileMutation::Move {
                source,
                destination_directory,
            } => move_entry(&root, source, destination_directory),
            FileMutation::Copy {
                source,
                destination_directory,
            } => copy_entry(&root, source, destination_directory),
        };
        match result {
            Ok(path) => self.after_file_mutation(&mutation, path, cx),
            Err(error) => {
                self.file_mutation_error = Some(file_action_error_message(&error));
                cx.notify();
            }
        }
    }

    fn execute_rpc_file_mutation(&mut self, mutation: FileMutation, cx: &mut Context<Self>) {
        let Some((engine, target, device)) = self.workspace_file_source(cx) else {
            return;
        };
        self.file_mutation_task = Some(cx.spawn(async move |this, cx| {
            let result = rpc_file_mutation(&engine, target, device, &mutation).await;
            let _ = this.update(cx, |this, cx| match result {
                Ok(path) => this.after_file_mutation(&mutation, path, cx),
                Err(zeron_rpc::RpcError::UnknownMethod(_)) => {
                    this.file_mutation_error =
                        Some("Update the project device to enable Files browsing.".into());
                    cx.notify();
                }
                Err(error) => {
                    this.file_mutation_error = Some(format!("{error}").into());
                    cx.notify();
                }
            });
        }));
    }

    fn after_file_mutation(
        &mut self,
        mutation: &FileMutation,
        path: String,
        cx: &mut Context<Self>,
    ) {
        match mutation {
            FileMutation::Delete { path: deleted } => {
                if let Some(active) = self.active_file.clone() {
                    if path_is_within(deleted, &active) {
                        self.active_file = None;
                        if active != *deleted {
                            self.emit_close_file(&active, cx);
                        }
                    }
                }
                self.emit_close_file(deleted, cx);
                if self
                    .file_clipboard
                    .as_ref()
                    .is_some_and(|clip| path_is_within(deleted, &clip.relative_path))
                {
                    self.file_clipboard = None;
                }
                if self
                    .delete_prompt
                    .as_ref()
                    .is_some_and(|prompt| path_is_within(deleted, &prompt.path))
                {
                    self.delete_prompt = None;
                }
                if self
                    .inline_edit
                    .as_ref()
                    .is_some_and(|edit| match &edit.edit {
                        super::file_actions::InlineFileEdit::Rename { path, .. } => {
                            path_is_within(deleted, path)
                        }
                        super::file_actions::InlineFileEdit::Create { parent, .. } => {
                            path_is_within(deleted, parent)
                        }
                    })
                {
                    self.inline_edit = None;
                    self.inline_input = None;
                }
            }
            FileMutation::Rename { path: old, .. } | FileMutation::Move { source: old, .. } => {
                if let Some(active) = self.active_file.clone() {
                    if let Some(next) = retarget_path(old, &path, &active) {
                        if next != active {
                            self.emit_close_file(&active, cx);
                            self.emit_open_file(&next, cx);
                        }
                        self.active_file = Some(next);
                    }
                }
                let cut_consumed = matches!(mutation, FileMutation::Move { source, .. } if self
                    .file_clipboard
                    .as_ref()
                    .is_some_and(|clip| clip.mode == FileClipboardMode::Cut && clip.relative_path == *source));
                if cut_consumed {
                    self.file_clipboard = None;
                } else if let Some(clip) = self.file_clipboard.as_mut() {
                    if let Some(next) = retarget_path(old, &path, &clip.relative_path) {
                        clip.relative_path = next;
                    }
                }
            }
            _ => {}
        }
        let parents = mutation.refresh_directories(&path);
        for parent in &parents {
            if !parent.is_empty() && !self.sidebar.expanded_paths().contains(parent) {
                self.sidebar.toggle_expanded(parent);
            }
        }
        if matches!(mutation, FileMutation::CreateDir { .. })
            && !self.sidebar.expanded_paths().contains(&path)
        {
            self.sidebar.toggle_expanded(&path);
        }
        self.emit_preferences(cx);
        if self.workspace_file_source(cx).is_some() {
            self.load_workspace_files(true, Some(parents), None, cx);
        } else {
            self.refresh_files(cx);
        }
        if matches!(mutation, FileMutation::CreateFile { .. }) {
            self.active_file = Some(path.clone());
            self.emit_open_file(&path, cx);
        }
        cx.notify();
    }

    fn emit_close_file(&self, relative_path: &str, cx: &mut Context<Self>) {
        if let Some(context) = self.sidebar.context() {
            cx.emit(DetailsSidebarEvent::CloseFile {
                context_key: context.key.clone(),
                relative_path: relative_path.to_string(),
            });
        }
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

    fn apply_file_menu_item(
        &mut self,
        item: FileMenuItem,
        window: &mut gpui::Window,
        cx: &mut Context<Self>,
    ) {
        let Some(target) = self.file_menu.as_open().cloned() else {
            return;
        };
        match item {
            FileMenuItem::NewFile => self.start_inline_create(InlineCreateKind::File, window, cx),
            FileMenuItem::NewFolder => {
                self.start_inline_create(InlineCreateKind::Directory, window, cx)
            }
            FileMenuItem::Cut => {
                if !target.is_root {
                    self.file_clipboard = Some(FileClipboard {
                        relative_path: target.path.clone(),
                        mode: FileClipboardMode::Cut,
                    });
                }
                self.close_file_menu(cx);
                cx.notify();
            }
            FileMenuItem::Copy => {
                if !target.is_root {
                    self.file_clipboard = Some(FileClipboard {
                        relative_path: target.path.clone(),
                        mode: FileClipboardMode::Copy,
                    });
                }
                self.close_file_menu(cx);
                cx.notify();
            }
            FileMenuItem::Paste => {
                let destination = create_parent_path(
                    if target.is_root {
                        None
                    } else {
                        Some(target.path.as_str())
                    },
                    target.is_dir,
                );
                self.close_file_menu(cx);
                if let Some(mutation) = paste_mutation(self.file_clipboard.as_ref(), destination) {
                    self.execute_file_mutation(mutation, cx);
                }
            }
            FileMenuItem::Duplicate => {
                if !target.is_root {
                    let parent = super::file_actions::parent_directory(&target.path);
                    self.execute_file_mutation(
                        FileMutation::Copy {
                            source: target.path.clone(),
                            destination_directory: parent,
                        },
                        cx,
                    );
                }
                self.close_file_menu(cx);
            }
            FileMenuItem::CopyPath => {
                self.copy_absolute_path(&target.path, cx);
                self.close_file_menu(cx);
            }
            FileMenuItem::CopyRelativePath => {
                cx.write_to_clipboard(ClipboardItem::new_string(target.path.clone()));
                self.close_file_menu(cx);
            }
            FileMenuItem::Rename => {
                self.close_file_menu(cx);
                self.start_inline_rename(target.path, window, cx);
            }
            FileMenuItem::Delete => self.request_delete(target.path, target.is_dir, cx),
            FileMenuItem::OpenInTerminal => {
                self.open_in_terminal(&target.path, target.is_dir, cx);
                self.close_file_menu(cx);
            }
            FileMenuItem::RevealInFinder => {
                self.reveal_in_finder(&target.path, cx);
                self.close_file_menu(cx);
            }
        }
    }

    fn copy_absolute_path(&self, relative: &str, cx: &mut Context<Self>) {
        let Some(context) = self.sidebar.context() else {
            return;
        };
        let absolute = if relative.is_empty() {
            context.cwd.clone()
        } else {
            context.cwd.join(relative)
        };
        cx.write_to_clipboard(ClipboardItem::new_string(
            absolute.to_string_lossy().to_string(),
        ));
    }

    fn reveal_in_finder(&self, relative: &str, cx: &mut Context<Self>) {
        let Some(context) = self.sidebar.context() else {
            return;
        };
        let path = if relative.is_empty() {
            context.cwd.clone()
        } else {
            context.cwd.join(relative)
        };
        cx.background_executor()
            .spawn(async move {
                let _ = std::process::Command::new("open")
                    .arg("-R")
                    .arg(path)
                    .status();
            })
            .detach();
    }

    fn open_in_terminal(&self, relative: &str, is_dir: bool, cx: &mut Context<Self>) {
        let Some(context) = self.sidebar.context() else {
            return;
        };
        let folder = if relative.is_empty() || is_dir {
            if relative.is_empty() {
                context.cwd.clone()
            } else {
                context.cwd.join(relative)
            }
        } else {
            context
                .cwd
                .join(relative)
                .parent()
                .map(std::path::Path::to_path_buf)
                .unwrap_or_else(|| context.cwd.clone())
        };
        if let Some((engine, _, device)) = self.workspace_file_source(cx) {
            let mut params = serde_json::json!({
                "cwd": folder.to_string_lossy(),
                "cols": 80,
                "rows": 24,
            });
            params["targetDeviceId"] = device.into();
            cx.spawn(async move |_, _| {
                let _ = engine
                    .client()
                    .call(zeron_rpc::methods::OPEN_TERMINAL, params)
                    .await;
            })
            .detach();
            return;
        }
        cx.background_executor()
            .spawn(async move {
                let _ = std::process::Command::new("open")
                    .arg("-a")
                    .arg("Terminal")
                    .arg(folder)
                    .status();
            })
            .detach();
    }

    fn on_files_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut gpui::Window,
        cx: &mut Context<Self>,
    ) {
        if self.inline_edit.is_some() {
            if event.keystroke.key == "escape" {
                cx.stop_propagation();
                self.cancel_inline_edit(cx);
            }
            return;
        }
        let key = event.keystroke.key.as_str();
        let platform = event.keystroke.modifiers.platform;
        if key == "escape" {
            cx.stop_propagation();
            self.file_clipboard = None;
            self.close_file_menu(cx);
            if self.delete_prompt.take().is_some() {
                cx.notify();
            }
            return;
        }
        if self.delete_prompt.is_some() {
            return;
        }
        let (path, is_dir) = self
            .selected_file_path()
            .unwrap_or_else(|| (String::new(), true));
        if platform && key == "c" && !path.is_empty() {
            cx.stop_propagation();
            self.file_clipboard = Some(FileClipboard {
                relative_path: path,
                mode: FileClipboardMode::Copy,
            });
            cx.notify();
        } else if platform && key == "x" && !path.is_empty() {
            cx.stop_propagation();
            self.file_clipboard = Some(FileClipboard {
                relative_path: path,
                mode: FileClipboardMode::Cut,
            });
            cx.notify();
        } else if platform && key == "v" {
            cx.stop_propagation();
            let destination = create_parent_path(
                if path.is_empty() {
                    None
                } else {
                    Some(path.as_str())
                },
                is_dir || path.is_empty(),
            );
            if let Some(mutation) = paste_mutation(self.file_clipboard.as_ref(), destination) {
                self.execute_file_mutation(mutation, cx);
            }
        } else if key == "f2" && !path.is_empty() {
            cx.stop_propagation();
            self.start_inline_rename(path, window, cx);
        } else if (key == "delete" || key == "backspace") && !path.is_empty() {
            cx.stop_propagation();
            self.request_delete(path, is_dir, cx);
        }
    }

    fn render_file_context_menu(
        &mut self,
        theme: &Theme,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let target = self.file_menu.get()?.clone();
        let closing = self.file_menu.closing_since();
        let kind = if target.is_root {
            FileMenuKind::Root
        } else {
            FileMenuKind::Entry
        };
        let clipboard = if self.file_clipboard.is_some() {
            FileClipboardPresence::Occupied
        } else {
            FileClipboardPresence::Empty
        };
        let access = self.files_access(cx);
        let entries = file_menu_items(kind, access, clipboard);
        let mut children: Vec<AnyElement> = Vec::new();
        for (index, entry) in entries.into_iter().enumerate() {
            if matches!(
                entry.item,
                FileMenuItem::Cut
                    | FileMenuItem::CopyPath
                    | FileMenuItem::Rename
                    | FileMenuItem::OpenInTerminal
            ) {
                children.push(popover::menu_separator().into_any_element());
            }
            let item = entry.item;
            let enabled = entry.enabled;
            let label = item.label();
            let shortcut = item.shortcut();
            let mut row = popover::menu_row(
                theme,
                false,
                SharedString::from(format!("details-file-menu-{index}")),
            )
            .id(("details-file-menu-row", index))
            .opacity(if enabled { 1.0 } else { 0.4 });
            if enabled {
                row = row.on_click(cx.listener(move |this, _, window, cx| {
                    this.apply_file_menu_item(item, window, cx);
                }));
            }
            row = row.child(div().flex_1().min_w_0().text_size(px(13.0)).child(label));
            if let Some(shortcut) = shortcut {
                row = row.child(
                    div()
                        .text_size(px(11.0))
                        .text_color(theme.text_muted)
                        .child(shortcut),
                );
            }
            children.push(row.into_any_element());
        }
        let menu = popover::popover_card(theme)
            .id("details-file-context-menu-card")
            .w(px(240.0))
            .on_mouse_down_out(cx.listener(|this, _, _, cx| this.close_file_menu(cx)))
            .flex()
            .flex_col()
            .gap(px(2.0))
            .children(children)
            .into_any_element();
        Some(popover::menu_at(
            "details-file-context-menu",
            target.position,
            menu,
            closing,
        ))
    }

    fn render_delete_dialog(
        &mut self,
        viewport: gpui::Size<gpui::Pixels>,
        theme: &Theme,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let prompt = self.delete_prompt.as_ref()?;
        let name = prompt.name.clone();
        let kind = if prompt.is_dir { "folder" } else { "file" };
        let card = popover::dialog_card(theme)
            .child(popover::dialog_title(theme, "Delete permanently"))
            .child(div().mt(px(12.0)).child(popover::dialog_body(
                theme,
                format!(
                    "Delete {kind} “{name}”? This cannot be undone and does not use the Trash."
                ),
            )))
            .child(
                div()
                    .mt(px(16.0))
                    .flex()
                    .flex_row()
                    .justify_end()
                    .gap(px(8.0))
                    .child(
                        popover::btn_ghost(theme, "Cancel", "details-file-delete-cancel")
                            .id("details-file-delete-cancel")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.delete_prompt = None;
                                cx.notify();
                            })),
                    )
                    .child(
                        popover::btn_danger(theme, "Delete")
                            .id("details-file-delete-confirm")
                            .on_click(cx.listener(|this, _, _, cx| {
                                if let Some(prompt) = this.delete_prompt.take() {
                                    this.execute_file_mutation(
                                        FileMutation::Delete { path: prompt.path },
                                        cx,
                                    );
                                }
                            })),
                    ),
            )
            .into_any_element();
        Some(popover::modal("details-file-delete-dialog", viewport, card))
    }

    fn render_files(&mut self, theme: &Theme, cx: &mut Context<Self>) -> gpui::Div {
        let mut content = div().size_full().flex().flex_col();
        if let Some(error) = &self.file_mutation_error {
            content = content.child(
                div()
                    .px(px(12.0))
                    .py(px(6.0))
                    .text_size(px(12.0))
                    .text_color(theme.danger)
                    .child(error.clone()),
            );
        }
        if self.search_visible {
            content = content.child(
                div()
                    .h(px(38.0))
                    .flex_none()
                    .mx(px(8.0))
                    .mb(px(4.0))
                    .px(px(8.0))
                    .rounded(px(8.0))
                    .border_1()
                    .border_color(theme.border)
                    .flex()
                    .items_center()
                    .child(self.search.clone()),
            );
        }
        match &self.files {
            LoadState::Loading => content.child(
                div()
                    .p(px(12.0))
                    .text_size(px(12.0))
                    .text_color(theme.text_muted)
                    .child("Loading files…"),
            ),
            LoadState::Error(message) => content.child(
                div()
                    .p(px(12.0))
                    .text_size(px(12.0))
                    .text_color(theme.danger)
                    .child(message.clone()),
            ),
            LoadState::Ready(files) => {
                let mut rows = flatten_visible_rows(files, &self.sidebar.expanded_paths());
                if let Some(edit) = self.inline_edit.as_ref() {
                    if let super::file_actions::InlineFileEdit::Create { parent, kind } = &edit.edit
                    {
                        let is_dir = matches!(kind, InlineCreateKind::Directory);
                        let index = inline_create_insert_index(&rows, parent, is_dir);
                        let depth = if parent.is_empty() {
                            0
                        } else {
                            rows.iter()
                                .find(|row| row.node.relative_path == *parent)
                                .map(|row| row.depth + 1)
                                .unwrap_or(0)
                        };
                        rows.insert(
                            index.min(rows.len()),
                            VisibleFileRow {
                                node: FileNode {
                                    name: String::new(),
                                    relative_path: if parent.is_empty() {
                                        "__inline__".into()
                                    } else {
                                        format!("{parent}/__inline__")
                                    },
                                    is_dir,
                                    children: Vec::new(),
                                },
                                depth,
                                has_next_sibling: false,
                                ancestor_continuations: Vec::new(),
                            },
                        );
                    }
                }
                let truncated = rows.len() > RENDERED_FILE_ROW_LIMIT;
                let root_name_string = self
                    .sidebar
                    .context()
                    .and_then(|context| context.cwd.file_name())
                    .and_then(|name| name.to_str())
                    .unwrap_or("Workspace")
                    .to_string();
                let root_name: SharedString = root_name_string.clone().into();
                let root_icon =
                    self.material_icon(material_icon_path(&root_name_string, true, true));
                content.child(
                    div()
                        .id("details-files-pane")
                        .track_focus(&self.files_focus)
                        .key_context("DetailsFiles")
                        .role(gpui::Role::Group)
                        .aria_label("Files")
                        .on_key_down(cx.listener(|this, event, window, cx| {
                            this.on_files_key(event, window, cx);
                        }))
                        .on_mouse_down(
                            MouseButton::Right,
                            cx.listener(|this, event: &MouseDownEvent, window, cx| {
                                window.focus(&this.files_focus, cx);
                                this.open_file_menu(
                                    FileMenuTarget {
                                        path: String::new(),
                                        is_dir: true,
                                        is_root: true,
                                        position: event.position,
                                    },
                                    cx,
                                );
                            }),
                        )
                        .m(px(10.0))
                        .flex_1()
                        .min_h_0()
                        .rounded(px(10.0))
                        .border_1()
                        .border_color(theme.border.opacity(0.5))
                        .overflow_hidden()
                        .flex()
                        .flex_col()
                        .child(
                            div()
                                .h(px(38.0))
                                .flex_none()
                                .px(px(10.0))
                                .flex()
                                .items_center()
                                .gap(px(8.0))
                                .bg(crate::theme::ink(0.012))
                                .child(
                                    icons::icon(icons::DETAILS_BOX)
                                        .size(px(15.0))
                                        .text_color(theme.text_muted),
                                )
                                .child(
                                    div()
                                        .text_size(px(13.0))
                                        .font_weight(gpui::FontWeight::MEDIUM)
                                        .text_color(theme.text)
                                        .child("Files"),
                                ),
                        )
                        .child(
                            div()
                                .id("details-files-root")
                                .h(px(28.0))
                                .flex_none()
                                .px(px(10.0))
                                .flex()
                                .items_center()
                                .gap(px(8.0))
                                .cursor_pointer()
                                .on_mouse_down(
                                    MouseButton::Right,
                                    cx.listener(|this, event: &MouseDownEvent, window, cx| {
                                        cx.stop_propagation();
                                        window.focus(&this.files_focus, cx);
                                        this.open_file_menu(
                                            FileMenuTarget {
                                                path: String::new(),
                                                is_dir: true,
                                                is_root: true,
                                                position: event.position,
                                            },
                                            cx,
                                        );
                                    }),
                                )
                                .child(root_icon)
                                .child(
                                    div()
                                        .text_size(px(12.0))
                                        .font_weight(gpui::FontWeight::MEDIUM)
                                        .text_color(theme.text)
                                        .child(root_name),
                                ),
                        )
                        .child(
                            div()
                                .id("details-files-scroll")
                                .flex_1()
                                .min_h_0()
                                .overflow_y_scroll()
                                .px(px(8.0))
                                .children(
                                    rows.into_iter()
                                        .take(RENDERED_FILE_ROW_LIMIT)
                                        .enumerate()
                                        .map(|(index, row)| {
                                            self.render_file_row(index, row, theme, cx)
                                        }),
                                )
                                .children(
                                    self.directory_cache
                                        .loaded_directories()
                                        .into_iter()
                                        .filter(|directory| {
                                            (directory.is_empty()
                                                || self
                                                    .sidebar
                                                    .expanded_paths()
                                                    .contains(directory))
                                                && self.directory_cache.cursor(directory).is_some()
                                        })
                                        .map(|directory| {
                                            let label = if directory.is_empty() {
                                                "Load more files".to_string()
                                            } else {
                                                format!("Load more in {directory}")
                                            };
                                            div()
                                                .id(SharedString::from(format!(
                                                    "files-more-{directory}"
                                                )))
                                                .px(px(8.0))
                                                .py(px(6.0))
                                                .text_size(px(11.0))
                                                .text_color(theme.text_muted)
                                                .cursor_pointer()
                                                .on_click(cx.listener(move |this, _, _, cx| {
                                                    this.load_workspace_files(
                                                        true,
                                                        Some(vec![directory.clone()]),
                                                        Some(directory.clone()),
                                                        cx,
                                                    );
                                                }))
                                                .child(label)
                                        }),
                                )
                                .when(truncated, |list| {
                                    list.child(
                                        div()
                                            .px(px(8.0))
                                            .py(px(6.0))
                                            .text_size(px(11.0))
                                            .text_color(theme.text_muted)
                                            .child("Refine search to show more files"),
                                    )
                                }),
                        ),
                )
            }
            LoadState::Idle => content,
        }
    }

    fn render_file_row(
        &mut self,
        index: usize,
        row: super::file_tree::VisibleFileRow,
        theme: &Theme,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<gpui::Div> {
        let relative = row.node.relative_path.clone();
        let is_dir = row.node.is_dir;
        let expanded = self.sidebar.expanded_paths().contains(&relative);
        let active = self.active_file.as_deref() == Some(relative.as_str());
        let material_icon = self.material_icon(file_glyph(&row.node, expanded));
        // Tier is read at render against a wall clock; the 5s tick re-renders
        // so a row decays without an animation replaying on every rescan.
        let recency_color =
            match self
                .recency
                .row_level(&relative, is_dir, std::time::Instant::now())
            {
                Some(RecencyLevel::Fresh) => theme.success,
                Some(RecencyLevel::Recent) => theme.warning,
                Some(RecencyLevel::Fading) => theme.warning_muted,
                None => theme.text,
            };
        let inline_create = relative == "__inline__" || relative.ends_with("/__inline__");
        let inline_rename = self.inline_edit.as_ref().is_some_and(|edit| {
            matches!(
                &edit.edit,
                super::file_actions::InlineFileEdit::Rename { path, .. } if path == &relative
            )
        });
        let inline_error = self
            .inline_edit
            .as_ref()
            .and_then(|edit| edit.error.clone())
            .filter(|_| inline_create || inline_rename);
        let menu_path = relative.clone();
        let mut row_el = div()
            .id(("details-file-row", index))
            .h(px(if inline_create || inline_rename {
                30.0
            } else {
                26.0
            }))
            .pl(px(26.0 + row.depth as f32 * 18.0))
            .pr(px(8.0))
            .rounded(px(6.0))
            .flex()
            .items_center()
            .gap(px(7.0))
            .cursor_pointer()
            .bg(if active {
                crate::theme::ink(0.075)
            } else {
                gpui::transparent_black()
            })
            .hover(|style| style.bg(crate::theme::ink(0.045)));
        if !inline_create {
            let click_path = relative.clone();
            row_el = row_el.on_click(cx.listener(move |this, _, window, cx| {
                window.focus(&this.files_focus, cx);
                if is_dir {
                    this.sidebar.toggle_expanded(&click_path);
                    if this.sidebar.expanded_paths().contains(&click_path)
                        && this.workspace_file_source(cx).is_some()
                    {
                        this.load_workspace_files(true, Some(vec![click_path.clone()]), None, cx);
                    }
                    this.emit_preferences(cx);
                } else {
                    this.active_file = Some(click_path.clone());
                    if let Some(context) = this.sidebar.context() {
                        cx.emit(DetailsSidebarEvent::OpenFile {
                            context_key: context.key.clone(),
                            root: context.cwd.clone(),
                            relative_path: click_path.clone(),
                            remote_target: if context_file_access(
                                context,
                                this.app_state.read(cx).local_device_id.as_deref(),
                            ) == ContextFileAccess::Remote
                            {
                                this.workspace_file_source(cx)
                                    .map(|(_, target, device)| (target, device))
                            } else {
                                None
                            },
                        });
                    }
                }
                cx.notify();
            }));
            row_el = row_el.on_mouse_down(
                MouseButton::Right,
                cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                    cx.stop_propagation();
                    window.focus(&this.files_focus, cx);
                    this.active_file = Some(menu_path.clone());
                    this.open_file_menu(
                        FileMenuTarget {
                            path: menu_path.clone(),
                            is_dir,
                            is_root: false,
                            position: event.position,
                        },
                        cx,
                    );
                }),
            );
        }
        row_el = row_el.child(material_icon);
        if inline_create || inline_rename {
            if let Some(input) = self.inline_input.clone() {
                row_el = row_el.child(div().flex_1().min_w_0().h(px(24.0)).child(input));
            }
            if let Some(error) = inline_error {
                row_el = row_el.child(
                    div()
                        .flex_none()
                        .text_size(px(10.0))
                        .text_color(theme.danger)
                        .child(error),
                );
            }
        } else {
            row_el = row_el.child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .text_size(px(12.0))
                    .text_color(recency_color)
                    .child(row.node.name),
            );
        }
        row_el
    }
}

impl EventEmitter<DetailsSidebarEvent> for DetailsSidebar {}

impl Render for DetailsSidebar {
    fn render(&mut self, window: &mut gpui::Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = Theme::of(cx).clone();
        let viewport = window.viewport_size();
        let body: AnyElement = match self.sidebar.tab() {
            DetailsTab::Details => self.render_details(&theme, cx).into_any_element(),
            DetailsTab::Files => self.render_files(&theme, cx).into_any_element(),
            DetailsTab::SourceControl => self.render_source_control(&theme, cx).into_any_element(),
        };
        let file_menu = self.render_file_context_menu(&theme, cx);
        let delete_dialog = self.render_delete_dialog(viewport, &theme, cx);
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
            .children(file_menu)
            .children(delete_dialog)
            .children(discard_dialog)
    }
}

fn file_action_error_message(error: &FileActionError) -> SharedString {
    match error {
        FileActionError::OutsideCheckout => "Path is outside the workspace.".into(),
        FileActionError::InvalidName => "Invalid name.".into(),
        FileActionError::MissingEntry => "That file no longer exists.".into(),
        FileActionError::TargetExists => "an entry with that name already exists".into(),
        FileActionError::MoveIntoDescendant => "cannot paste a folder into itself".into(),
        FileActionError::Unsupported => "path is a symlink".into(),
        FileActionError::Io(message) => message.clone().into(),
    }
}

fn file_node_is_dir(nodes: &[FileNode], relative: &str) -> bool {
    fn walk(nodes: &[FileNode], relative: &str) -> Option<bool> {
        for node in nodes {
            if node.relative_path == relative {
                return Some(node.is_dir);
            }
            if let Some(found) = walk(&node.children, relative) {
                return Some(found);
            }
        }
        None
    }
    walk(nodes, relative).unwrap_or(false)
}

fn sibling_entry_names(nodes: &[FileNode], parent: &str) -> Vec<String> {
    if parent.is_empty() {
        return nodes.iter().map(|node| node.name.clone()).collect();
    }
    fn find<'a>(nodes: &'a [FileNode], parent: &str) -> Option<&'a FileNode> {
        for node in nodes {
            if node.relative_path == parent {
                return Some(node);
            }
            if let Some(found) = find(&node.children, parent) {
                return Some(found);
            }
        }
        None
    }
    find(nodes, parent)
        .map(|node| {
            node.children
                .iter()
                .map(|child| child.name.clone())
                .collect()
        })
        .unwrap_or_default()
}

fn paste_mutation(clipboard: Option<&FileClipboard>, destination: String) -> Option<FileMutation> {
    let clip = clipboard?;
    Some(match clip.mode {
        FileClipboardMode::Cut => FileMutation::Move {
            source: clip.relative_path.clone(),
            destination_directory: destination,
        },
        FileClipboardMode::Copy => FileMutation::Copy {
            source: clip.relative_path.clone(),
            destination_directory: destination,
        },
    })
}

async fn rpc_file_mutation(
    engine: &crate::state::EngineHandle,
    target: zeron_proto::WorkspaceTarget,
    device: String,
    mutation: &FileMutation,
) -> Result<String, zeron_rpc::RpcError> {
    let (method, mut params) = match mutation {
        FileMutation::CreateFile { parent, name } => (
            zeron_rpc::methods::CREATE_WORKSPACE_ENTRY,
            serde_json::to_value(zeron_proto::CreateWorkspaceEntryRequest {
                target: target.clone(),
                parent_path: parent.clone(),
                name: name.clone(),
                kind: zeron_proto::WorkspaceEntryKind::File,
            })
            .unwrap(),
        ),
        FileMutation::CreateDir { parent, name } => (
            zeron_rpc::methods::CREATE_WORKSPACE_ENTRY,
            serde_json::to_value(zeron_proto::CreateWorkspaceEntryRequest {
                target: target.clone(),
                parent_path: parent.clone(),
                name: name.clone(),
                kind: zeron_proto::WorkspaceEntryKind::Directory,
            })
            .unwrap(),
        ),
        FileMutation::Rename { path, new_name } => (
            zeron_rpc::methods::RENAME_WORKSPACE_ENTRY,
            serde_json::to_value(zeron_proto::RenameWorkspaceEntryRequest {
                target: target.clone(),
                path: path.clone(),
                new_name: new_name.clone(),
            })
            .unwrap(),
        ),
        FileMutation::Delete { path } => (
            zeron_rpc::methods::DELETE_WORKSPACE_ENTRY,
            serde_json::to_value(zeron_proto::DeleteWorkspaceEntryRequest {
                target: target.clone(),
                path: path.clone(),
            })
            .unwrap(),
        ),
        FileMutation::Move {
            source,
            destination_directory,
        } => (
            zeron_rpc::methods::MOVE_WORKSPACE_ENTRY,
            serde_json::to_value(zeron_proto::MoveWorkspaceEntryRequest {
                target: target.clone(),
                source_path: source.clone(),
                destination_directory: destination_directory.clone(),
            })
            .unwrap(),
        ),
        FileMutation::Copy {
            source,
            destination_directory,
        } => (
            zeron_rpc::methods::COPY_WORKSPACE_ENTRY,
            serde_json::to_value(zeron_proto::CopyWorkspaceEntryRequest {
                target: target.clone(),
                source_path: source.clone(),
                destination_directory: destination_directory.clone(),
            })
            .unwrap(),
        ),
    };
    params["targetDeviceId"] = device.into();
    let reply: zeron_proto::WorkspaceEntryMutation =
        engine.client().call_as(method, params).await?;
    Ok(reply.path)
}

#[cfg(test)]
mod tests {
    use std::{collections::HashMap, path::PathBuf};

    use super::{
        ContextFileAccess, DetailsSidebarEvent, DetailsSidebarPreferences, DetailsSidebarState,
        FileNode, context_file_access, details_sidebar_background, file_node_is_dir,
        open_subagent_event, open_worker_event, sibling_entry_names, subagent_row_avatar_path,
        worker_click_event,
    };
    use crate::details_sidebar::chat_workers::{ChatActivityRow, ChatWorkerRow, WorkerSemantic};
    use crate::details_sidebar::context::{DetailsContext, DetailsMode, DetailsTab};
    use crate::theme::Theme;
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
    fn source_control_tab_persists_like_files() {
        let mut state = DetailsSidebarState::new(DetailsSidebarPreferences::default());
        state.set_tab(DetailsTab::SourceControl);
        assert_eq!(state.preferences().active_tab, DetailsTab::SourceControl);
    }

    #[test]
    fn tab_and_per_context_tree_preferences_round_trip() {
        let mut state = DetailsSidebarState::new(DetailsSidebarPreferences::default());
        state.set_tab(DetailsTab::Files);
        state.set_context(Some(context("one")));
        state.toggle_expanded("src");
        state.toggle_hidden();
        state.set_context(Some(context("two")));
        assert!(state.expanded_paths().is_empty());
        assert!(!state.show_hidden());
        state.set_context(Some(context("one")));
        assert!(state.expanded_paths().contains("src"));
        assert!(state.show_hidden());
        let preferences = state.preferences();
        assert_eq!(preferences.active_tab, DetailsTab::Files);
        assert_eq!(preferences.expanded.get("one").unwrap(), &["src"]);
        assert_eq!(preferences.hidden, HashMap::from([("one".into(), true)]));
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

        // Expanded paths for file tree must NOT contain the collapse key
        assert!(
            !state
                .expanded_paths()
                .contains(":projects_worked:collapsed")
        );

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
    fn open_file_event_carries_context_root_and_relative_path() {
        let event = DetailsSidebarEvent::OpenFile {
            context_key: "project".into(),
            root: PathBuf::from("/tmp/project"),
            relative_path: "README.md".into(),
            remote_target: None,
        };
        let DetailsSidebarEvent::OpenFile {
            context_key,
            root,
            relative_path,
            remote_target,
        } = event
        else {
            panic!("expected open file event");
        };
        assert_eq!(context_key, "project");
        assert_eq!(root, PathBuf::from("/tmp/project"));
        assert_eq!(relative_path, "README.md");
        assert!(remote_target.is_none());
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
    fn stale_file_load_cannot_replace_current_context() {
        let mut state = DetailsSidebarState::new(DetailsSidebarPreferences::default());
        let first = state.set_context(Some(context("one")));
        let second = state.set_context(Some(context("two")));
        assert!(first < second);
        assert!(!state.accept_file_load(first, "one"));
        assert!(state.accept_file_load(second, "two"));
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

    #[test]
    fn sibling_lookup_walks_nested_directories() {
        let tree = vec![FileNode {
            name: "src".into(),
            relative_path: "src".into(),
            is_dir: true,
            children: vec![
                FileNode {
                    name: "a.rs".into(),
                    relative_path: "src/a.rs".into(),
                    is_dir: false,
                    children: Vec::new(),
                },
                FileNode {
                    name: "b.rs".into(),
                    relative_path: "src/b.rs".into(),
                    is_dir: false,
                    children: Vec::new(),
                },
            ],
        }];
        assert!(file_node_is_dir(&tree, "src"));
        assert!(!file_node_is_dir(&tree, "src/a.rs"));
        assert_eq!(sibling_entry_names(&tree, "src"), vec!["a.rs", "b.rs"]);
        assert_eq!(sibling_entry_names(&tree, ""), vec!["src"]);
    }
}
