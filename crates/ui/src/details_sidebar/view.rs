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
        if self.context.as_ref().map(|value| &value.key) != context.as_ref().map(|value| &value.key)
        {
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
    AnyElement, AppContext as _, ClipboardItem, Context, Entity, EventEmitter, Focusable, Image,
    IntoElement, ObjectFit, Render, SharedString, Subscription, Task, div, img, prelude::*, px,
};
use zeron_proto::{AgentAccountsSnapshot, HarnessId};

use crate::{
    composer::{ComposerInput, ComposerInputEvent},
    details_sidebar::{
        context::detect_git_branch,
        file_tree::{FileNode, flatten_visible_rows, scan_checkout},
        files_view::{file_glyph, material_icon_path},
        todos::latest_todos,
        usage::{ProviderUsageRow, ProviderUsageState, provider_usage_rows},
        widgets::{property_row, widget_card},
    },
    icons,
    state::AppState,
    theme::Theme,
};

const FILE_SCAN_LIMIT: usize = 5_000;
const RENDERED_FILE_ROW_LIMIT: usize = 800;

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
}

pub struct DetailsSidebar {
    app_state: Entity<AppState>,
    sidebar: DetailsSidebarState,
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
    usage_task: Option<Task<()>>,
    _state_observe: Subscription,
    _search_events: Subscription,
}

impl DetailsSidebar {
    pub fn new(
        app_state: Entity<AppState>,
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
            if state.read(cx).engine().is_some()
                && matches!(this.usage, LoadState::Idle | LoadState::Error(_))
            {
                this.load_usage(cx);
            }
            cx.notify();
        });
        let mut sidebar = Self {
            app_state,
            sidebar: DetailsSidebarState::new(preferences),
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
            _state_observe: state_observe,
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
        let Some(context) = self.sidebar.context().cloned() else {
            self.files = LoadState::Idle;
            return;
        };
        let generation = self.sidebar.load_generation();
        let context_key = context.key.clone();
        let show_hidden = self.sidebar.show_hidden();
        let query = self.search.read(cx).text().to_string();
        self.files = LoadState::Loading;
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
            && let Some(todos) = latest_todos(&self.app_state.read(cx).transcript)
        {
            let todo_body =
                div().children(todos.items.into_iter().enumerate().map(|(index, todo)| {
                    div()
                        .h(px(32.0))
                        .px(px(10.0))
                        .border_t_1()
                        .border_color(theme.border.opacity(0.55))
                        .flex()
                        .items_center()
                        .gap(px(8.0))
                        .child(
                            div()
                                .size(px(15.0))
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
                                            .size(px(9.0))
                                            .text_color(theme.bg),
                                    )
                                })
                                .when(todo.done, |dot| {
                                    dot.bg(crate::theme::ink(0.08)).child(
                                        icons::icon(icons::CHECK)
                                            .size(px(9.0))
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
        let expanded = self.usage_expanded.contains(&key);
        let icon_path = if row.harness == HarnessId::ClaudeCode {
            icons::CLAUDE_MARK
        } else {
            icons::OPENAI_MARK
        };
        let summary: SharedString = match row.state {
            ProviderUsageState::Ready => row
                .weekly_summary
                .clone()
                .unwrap_or_else(|| "—".into())
                .into(),
            ProviderUsageState::NoUsage => "No usage yet".into(),
            ProviderUsageState::NotSignedIn => "Not signed in".into(),
        };
        let windows = row.windows.clone();
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
                    .cursor_pointer()
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if !this.usage_expanded.remove(&key) {
                            this.usage_expanded.insert(key.clone());
                        }
                        cx.notify();
                    }))
                    .child(icons::icon(icon_path).size(px(15.0)).text_color(
                        if row.harness == HarnessId::ClaudeCode {
                            icons::claude_brand()
                        } else {
                            theme.text_muted
                        },
                    ))
                    .child(
                        div()
                            .text_size(px(12.5))
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .text_color(theme.text)
                            .child(row.label),
                    )
                    .child(
                        div()
                            .flex_1()
                            .text_right()
                            .text_size(px(12.0))
                            .text_color(theme.text_muted)
                            .child(summary),
                    )
                    .child(
                        icons::icon(icons::ALT_ARROW_DOWN)
                            .size(px(13.0))
                            .text_color(theme.text_muted),
                    ),
            )
            .when(expanded, |container| {
                container.child(div().children(windows.into_iter().map(|window| {
                    div()
                        .h(px(30.0))
                        .px(px(10.0))
                        .border_t_1()
                        .border_color(theme.border.opacity(0.4))
                        .flex()
                        .items_center()
                        .text_size(px(11.5))
                        .text_color(theme.text_muted)
                        .child(window.label)
                        .child(
                            div()
                                .flex_1()
                                .text_right()
                                .child(format!("{}%", window.remaining_percent)),
                        )
                })))
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
                    .text_color(theme.text)
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
        DetailsSidebarEvent, DetailsSidebarPreferences, DetailsSidebarState,
        details_sidebar_background,
    };
    use crate::details_sidebar::context::{DetailsContext, DetailsMode, DetailsTab};
    use crate::theme::Theme;

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
    fn stale_file_load_cannot_replace_current_context() {
        let mut state = DetailsSidebarState::new(DetailsSidebarPreferences::default());
        let first = state.set_context(Some(context("one")));
        let second = state.set_context(Some(context("two")));
        assert!(first < second);
        assert!(!state.accept_file_load(first, "one"));
        assert!(state.accept_file_load(second, "two"));
    }
}
