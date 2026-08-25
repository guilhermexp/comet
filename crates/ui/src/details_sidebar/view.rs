use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

use super::context::{DetailsContext, DetailsTab};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct DetailsSidebarPreferences {
    pub active_tab: DetailsTab,
    pub expanded: HashMap<String, Vec<String>>,
    pub hidden: HashMap<String, bool>,
}

impl Default for DetailsSidebarPreferences {
    fn default() -> Self {
        Self {
            active_tab: DetailsTab::Details,
            expanded: HashMap::new(),
            hidden: HashMap::new(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct DetailsSidebarState {
    context: Option<DetailsContext>,
    preferences: DetailsSidebarPreferences,
    load_generation: u64,
}

impl DetailsSidebarState {
    pub fn new(preferences: DetailsSidebarPreferences) -> Self {
        Self {
            context: None,
            preferences,
            load_generation: 0,
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
            .collect()
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
}

use gpui::{
    AnyElement, App, AppContext as _, ClipboardItem, Context, Entity, EventEmitter, Focusable,
    Image, IntoElement, ObjectFit, Render, SharedString, Subscription, Task, div, img, prelude::*,
    px,
};
use zeron_proto::{
    AgentAccountsSnapshot,
    agent::{WorkflowProgressNode, WorkflowTaskStatus},
};

use crate::{
    composer::{ComposerInput, ComposerInputEvent},
    details_sidebar::{
        chat_workers::{
            ChatActivityRow, ChatWorkerRow, ChatWorkersSnapshot, WorkerSemantic,
            activity_tasks_from_entries, compact_activity_label, project_chat_workers,
        },
        context::detect_git_branch,
        file_tree::{FileNode, flatten_visible_rows, is_denied_relative, scan_checkout},
        files_view::{file_glyph, material_icon_path},
        recency::{FileRecency, RECENCY_TICK, RecencyLevel},
        subagent_avatars::blobatar_subagent_avatar_path,
        todos::{latest_todos, todo_status_layout, todo_viewport_height_px},
        usage::{ProviderUsageRow, ProviderUsageState, provider_usage_rows, usage_provider_icon},
        widgets::{
            CHAT_WORKERS_ROW_HEIGHT, ChatWorkersTab, ChatWorkersWidgetState,
            chat_workers_viewport_height_px, property_row, widget_card, workers_tab_presence,
        },
    },
    icons,
    state::AppState,
    theme::Theme,
    workers::{model::WorkersModel, presentation::runtime_icon_path},
};

const FILE_SCAN_LIMIT: usize = 5_000;
const RENDERED_FILE_ROW_LIMIT: usize = 800;
/// Debounce between a filesystem event and the silent rescan it triggers. The
/// rescan keeps the current tree on screen: flipping to `Loading` on every
/// save flashed the pane empty.
const RECENCY_REFRESH_DEBOUNCE: std::time::Duration = std::time::Duration::from_millis(500);

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
    sidebar: DetailsSidebarState,
    chat_workers: ChatWorkersWidgetState,
    files: LoadState<Vec<FileNode>>,
    usage: LoadState<Vec<ProviderUsageRow>>,
    search: Entity<ComposerInput>,
    search_visible: bool,
    active_file: Option<String>,
    usage_expanded: std::collections::HashSet<String>,
    material_icons: std::collections::HashMap<SharedString, std::sync::Arc<Image>>,
    resolved_branch: Option<String>,
    file_task: Option<Task<()>>,
    branch_task: Option<Task<()>>,
    recency: FileRecency,
    recency_root: Option<std::path::PathBuf>,
    recency_watch: Option<Task<()>>,
    recency_tick: Option<Task<()>>,
    recency_ticking: bool,
    recency_refresh: Option<Task<()>>,
    usage_task: Option<Task<()>>,
    _state_observe: Subscription,
    _workers_observe: Subscription,
    _search_events: Subscription,
}

impl DetailsSidebar {
    pub fn new(
        app_state: Entity<AppState>,
        workers_model: Entity<WorkersModel>,
        preferences: DetailsSidebarPreferences,
        cx: &mut Context<Self>,
    ) -> Self {
        let search = cx.new(|cx| ComposerInput::with_context("Search files…", "PaletteSearch", cx));
        let search_events = cx.subscribe(&search, |this, _, event, cx| {
            if matches!(event, ComposerInputEvent::Edited) {
                this.reload_files(cx);
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
            cx.notify();
        });
        let workers_observe = cx.observe(&workers_model, |_, _, cx| cx.notify());
        let mut sidebar = Self {
            app_state,
            workers_model,
            sidebar: DetailsSidebarState::new(preferences),
            chat_workers: ChatWorkersWidgetState::default(),
            files: LoadState::Idle,
            usage: LoadState::Idle,
            search,
            search_visible: false,
            active_file: None,
            usage_expanded: std::collections::HashSet::new(),
            material_icons: std::collections::HashMap::new(),
            resolved_branch: None,
            file_task: None,
            branch_task: None,
            usage_task: None,
            recency: FileRecency::default(),
            recency_root: None,
            recency_watch: None,
            recency_tick: None,
            recency_ticking: false,
            recency_refresh: None,
            _state_observe: state_observe,
            _workers_observe: workers_observe,
            _search_events: search_events,
        };
        sidebar.load_usage(cx);
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
            self.chat_workers
                .sync_context(self.sidebar.context().map(|context| context.key.as_str()));
            self.active_file = None;
            self.resolved_branch = self
                .sidebar
                .context()
                .and_then(|value| value.branch.clone());
            self.reload_files(cx);
            self.load_branch(cx);
            cx.notify();
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

    fn set_tab(&mut self, tab: DetailsTab, cx: &mut Context<Self>) {
        if self.sidebar.tab() == tab {
            return;
        }
        self.sidebar.set_tab(tab);
        self.emit_preferences(cx);
        cx.notify();
    }

    fn reload_files(&mut self, cx: &mut Context<Self>) {
        self.load_files(false, cx);
    }

    /// Rescan without flipping to `Loading`, so a watcher-driven refresh keeps
    /// the current rows (and their recency tint) visible while it runs.
    fn refresh_files(&mut self, cx: &mut Context<Self>) {
        self.load_files(true, cx);
    }

    fn load_files(&mut self, silent: bool, cx: &mut Context<Self>) {
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

    fn load_usage(&mut self, cx: &mut Context<Self>) {
        let Some(engine) = self.app_state.read(cx).engine().cloned() else {
            self.usage = LoadState::Error("Engine not connected".into());
            return;
        };
        self.usage = LoadState::Loading;
        self.usage_task = Some(cx.spawn(async move |this, cx| {
            let result = engine
                .client()
                .call(
                    zeron_rpc::methods::LIST_AGENT_ACCOUNTS,
                    serde_json::json!({ "forceUsage": true }),
                )
                .await;
            let _ = this.update(cx, |this, cx| {
                this.usage = match result {
                    Ok(value) => match serde_json::from_value::<AgentAccountsSnapshot>(value) {
                        Ok(snapshot) => {
                            LoadState::Ready(provider_usage_rows(&snapshot, chrono::Utc::now()))
                        }
                        Err(error) => LoadState::Error(error.to_string().into()),
                    },
                    Err(error) => LoadState::Error(error.to_string().into()),
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
                    theme.bg
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
                            .bg(crate::theme::ink(0.045))
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
                            ),
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
            .hover(|style| style.bg(crate::theme::ink(0.05)))
            .on_click(cx.listener(move |this, _, _, cx| {
                this.chat_workers.select(tab);
                cx.notify();
            }))
            .child(label)
            .when(count > 0, |pill| {
                pill.child(
                    div()
                        .text_size(px(10.0))
                        .text_color(theme.text_muted)
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
        &self,
        worker: ChatWorkerRow,
        chat_id: String,
        theme: &Theme,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let event = open_worker_event(&chat_id, &worker);
        let session_id = worker.session_id.clone();
        let status = self.render_worker_status(&worker, theme, cx);
        let runtime_icon = runtime_icon_path(worker.provider_id.as_deref(), Some(&worker.command));
        div()
            .id(SharedString::from(format!(
                "chat-worker-{}",
                worker.session_id
            )))
            .min_h(px(38.0))
            .px(px(8.0))
            .py(px(5.0))
            .border_t_1()
            .border_color(theme.border.opacity(0.45))
            .flex()
            .items_center()
            .gap(px(7.0))
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
                            .truncate()
                            .text_size(px(12.0))
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .text_color(theme.text)
                            .child(worker.title),
                    )
                    .child(
                        div()
                            .truncate()
                            .text_size(px(10.0))
                            .text_color(theme.text_muted)
                            .child(worker.command),
                    ),
            )
            .child(status)
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
        self.chat_workers.sync_activities(
            snapshot
                .workflows
                .iter()
                .chain(snapshot.subagents.iter())
                .map(|row| row.id.as_str()),
        );
        let active = self.chat_workers.active_tab(
            workflows,
            subagents,
            workers_tab_presence(workers, workers_error.is_some()),
        );
        let tabs = div()
            .h(px(34.0))
            .px(px(7.0))
            .flex()
            .items_center()
            .gap(px(3.0))
            .border_b_1()
            .border_color(theme.border.opacity(0.55))
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
        let folder = context
            .cwd
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("Workspace")
            .to_string();
        let workspace_body = div()
            .child(property_row(
                icons::GIT_BRANCH,
                "Branch",
                self.resolved_branch.clone().unwrap_or_else(|| "—".into()),
                theme,
            ))
            .child(property_row(icons::FOLDER, "Path", folder, theme));
        let mut content = div()
            .w_full()
            .flex()
            .flex_col()
            .gap(px(10.0))
            .p(px(10.0))
            .child(widget_card(
                "workspace-widget",
                icons::DETAILS_BOX,
                "Workspace",
                workspace_body,
                theme,
            ));

        if context.mode == super::context::DetailsMode::Orchestrator
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

        if context.mode == super::context::DetailsMode::Orchestrator
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

        let usage_body = match &self.usage {
            LoadState::Ready(rows) => div().children(
                rows.clone()
                    .into_iter()
                    .map(|row| self.render_usage_row(row, theme, cx)),
            ),
            LoadState::Loading => div()
                .p(px(10.0))
                .text_size(px(12.0))
                .text_color(theme.text_muted)
                .child("Loading usage…"),
            LoadState::Error(message) => div()
                .p(px(10.0))
                .text_size(px(12.0))
                .text_color(theme.danger)
                .child(message.clone()),
            LoadState::Idle => div(),
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
        let key = row.label.to_string();
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
        let reset_soon = reset_badge.is_some();
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
                            .text_size(px(12.5))
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .text_color(theme.text)
                            .child(row.label),
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
                                        .bg(theme.warning.opacity(0.12))
                                        .text_size(px(10.0))
                                        .font_weight(gpui::FontWeight::MEDIUM)
                                        .text_color(theme.warning)
                                        .child(badge),
                                )
                            })
                            .child(
                                div()
                                    .flex_none()
                                    .text_size(px(12.0))
                                    .text_color(if reset_soon {
                                        theme.warning
                                    } else {
                                        theme.text_muted
                                    })
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
                            let label = if window.label.to_lowercase().contains("week") {
                                "Weekly".to_string()
                            } else if window.label.to_lowercase().contains("session") {
                                "5h".to_string()
                            } else {
                                window.label.clone()
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

    fn render_files(&mut self, theme: &Theme, cx: &mut Context<Self>) -> gpui::Div {
        let mut content = div().size_full().flex().flex_col();
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
                let rows = flatten_visible_rows(files, &self.sidebar.expanded_paths());
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
                        .m(px(10.0))
                        .flex_1()
                        .min_h_0()
                        .rounded(px(10.0))
                        .border_1()
                        .border_color(theme.border)
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
                                .bg(crate::theme::ink(0.025))
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
                                .h(px(28.0))
                                .flex_none()
                                .px(px(10.0))
                                .flex()
                                .items_center()
                                .gap(px(8.0))
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
        let absolute = self
            .sidebar
            .context()
            .map(|context| context.cwd.join(&relative));
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
        div()
            .id(("details-file-row", index))
            .h(px(26.0))
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
            .hover(|style| style.bg(crate::theme::ink(0.045)))
            .on_click(cx.listener(move |this, _, _, cx| {
                if is_dir {
                    this.sidebar.toggle_expanded(&relative);
                    this.emit_preferences(cx);
                } else {
                    this.active_file = Some(relative.clone());
                    if let Some(context) = this.sidebar.context() {
                        cx.emit(DetailsSidebarEvent::OpenFile {
                            context_key: context.key.clone(),
                            root: context.cwd.clone(),
                            relative_path: relative.clone(),
                        });
                    }
                }
                cx.notify();
            }))
            .child(material_icon)
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .text_size(px(12.0))
                    .text_color(recency_color)
                    .child(row.node.name),
            )
            .when_some(active.then_some(absolute).flatten(), |row, absolute| {
                let copy_path = absolute.clone();
                let reveal_path = absolute;
                row.child(
                    div()
                        .id(("details-file-copy", index))
                        .size(px(22.0))
                        .flex_none()
                        .rounded(px(5.0))
                        .flex()
                        .items_center()
                        .justify_center()
                        .on_click(move |_, _, cx| {
                            cx.stop_propagation();
                            cx.write_to_clipboard(ClipboardItem::new_string(
                                copy_path.to_string_lossy().to_string(),
                            ));
                        })
                        .child(
                            icons::icon(icons::COPY)
                                .size(px(13.0))
                                .text_color(theme.text_muted),
                        ),
                )
                .child(
                    div()
                        .id(("details-file-reveal", index))
                        .size(px(22.0))
                        .flex_none()
                        .rounded(px(5.0))
                        .flex()
                        .items_center()
                        .justify_center()
                        .on_click(move |_, _, cx| {
                            cx.stop_propagation();
                            let path = reveal_path.clone();
                            cx.background_executor()
                                .spawn(async move {
                                    let _ = std::process::Command::new("open")
                                        .arg("-R")
                                        .arg(path)
                                        .status();
                                })
                                .detach();
                        })
                        .child(
                            icons::icon(icons::FOLDER)
                                .size(px(13.0))
                                .text_color(theme.text_muted),
                        ),
                )
            })
    }
}

impl EventEmitter<DetailsSidebarEvent> for DetailsSidebar {}

impl Render for DetailsSidebar {
    fn render(&mut self, _window: &mut gpui::Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = Theme::of(cx).clone();
        let body: AnyElement = match self.sidebar.tab() {
            DetailsTab::Details => self.render_details(&theme, cx).into_any_element(),
            DetailsTab::Files => self.render_files(&theme, cx).into_any_element(),
        };
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
    }
}

#[cfg(test)]
mod tests {
    use std::{collections::HashMap, path::PathBuf};

    use super::{
        ContextFileAccess, DetailsSidebarEvent, DetailsSidebarPreferences, DetailsSidebarState,
        context_file_access, details_sidebar_background, open_subagent_event, open_worker_event,
        subagent_row_avatar_path, worker_click_event,
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
    fn open_file_event_carries_context_root_and_relative_path() {
        let event = DetailsSidebarEvent::OpenFile {
            context_key: "project".into(),
            root: PathBuf::from("/tmp/project"),
            relative_path: "README.md".into(),
        };
        let DetailsSidebarEvent::OpenFile {
            context_key,
            root,
            relative_path,
        } = event
        else {
            panic!("expected open file event");
        };
        assert_eq!(context_key, "project");
        assert_eq!(root, PathBuf::from("/tmp/project"));
        assert_eq!(relative_path, "README.md");
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
            updated_at_unix_ms: 42,
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
            updated_at_unix_ms: 42,
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
}
