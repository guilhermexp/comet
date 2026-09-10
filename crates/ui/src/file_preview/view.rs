use std::{
    cell::RefCell,
    collections::HashMap,
    path::{Path, PathBuf},
    rc::Rc,
    sync::Arc,
};

use comet_syntax::HighlightedDocument;
use gpui::{
    AnyElement, ClipboardItem, Context, EventEmitter, Image, InteractiveElement, IntoElement,
    ListHorizontalSizingBehavior, ListState, ObjectFit, Render, SharedString, StyledText, Task,
    UniformListScrollHandle, Window, div, font, img, list, prelude::*, px, uniform_list,
};

use crate::{
    details_sidebar::files_view::material_icon_path,
    file_preview::{
        loader::{LoadedPreview, PreviewLoadError, load_preview_with_typography},
        model::{PreviewDisplayMode, PreviewTabs},
    },
    icons,
    markdown::render as markdown_render,
    theme::Theme,
};

#[derive(Clone)]
enum PreviewLoadState {
    Idle,
    Loading,
    Ready(LoadedPreview),
    Error(SharedString),
}

/// Construction of the native host (WKWebView) is decided from load state,
/// never from `Render`. Paint only attaches an already-built view.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NativeDocumentAction {
    Hide,
    Keep,
    OpenHtml,
    OpenPdf,
    OpenVideo,
}

fn native_document_action(
    loaded: &PreviewLoadState,
    absolute: &Path,
    existing: Option<&Path>,
) -> NativeDocumentAction {
    let open = match loaded {
        PreviewLoadState::Ready(LoadedPreview::Html(_)) => NativeDocumentAction::OpenHtml,
        PreviewLoadState::Ready(LoadedPreview::Pdf) => NativeDocumentAction::OpenPdf,
        PreviewLoadState::Ready(LoadedPreview::Video) => NativeDocumentAction::OpenVideo,
        _ => return NativeDocumentAction::Hide,
    };
    if existing == Some(absolute) {
        NativeDocumentAction::Keep
    } else {
        open
    }
}

fn preview_absolute_path(root: &Path, relative_path: &str) -> PathBuf {
    let relative = Path::new(relative_path);
    if relative.is_absolute() {
        relative.to_path_buf()
    } else {
        root.join(relative)
    }
}

#[derive(Debug, Clone)]
pub enum FilePreviewEvent {
    ActiveChanged {
        context_key: String,
        relative_path: Option<String>,
    },
    CloseRequested {
        context_key: String,
        relative_path: String,
    },
    DisplayModeChanged(PreviewDisplayMode),
}

#[derive(Clone)]
struct RemoteFileSource {
    engine: crate::state::EngineHandle,
    target: zeron_proto::WorkspaceTarget,
    device: String,
}

pub struct FilePreview {
    tabs: PreviewTabs,
    roots: HashMap<String, PathBuf>,
    remote_sources: HashMap<String, RemoteFileSource>,
    active_context: Option<String>,
    loaded: PreviewLoadState,
    generation: u64,
    display_mode: PreviewDisplayMode,
    load_task: Option<Task<()>>,
    scroll_handles: HashMap<(String, String), UniformListScrollHandle>,
    markdown_lists: HashMap<(String, String), (ListState, u32)>,
    markdown_cache: Rc<RefCell<markdown_render::RenderCache>>,
    #[cfg(target_os = "macos")]
    native_document: Option<(
        PathBuf,
        Rc<RefCell<super::native_document::NativeDocumentView>>,
    )>,
}

impl FilePreview {
    pub fn new() -> Self {
        Self {
            tabs: PreviewTabs::default(),
            roots: HashMap::new(),
            remote_sources: HashMap::new(),
            active_context: None,
            loaded: PreviewLoadState::Idle,
            generation: 0,
            display_mode: PreviewDisplayMode::SidePeek,
            load_task: None,
            scroll_handles: HashMap::new(),
            markdown_lists: HashMap::new(),
            markdown_cache: Rc::default(),
            #[cfg(target_os = "macos")]
            native_document: None,
        }
    }

    pub fn set_remote_source(
        &mut self,
        context_key: String,
        engine: crate::state::EngineHandle,
        target: zeron_proto::WorkspaceTarget,
        device: String,
    ) {
        self.remote_sources.insert(
            context_key,
            RemoteFileSource {
                engine,
                target,
                device,
            },
        );
    }

    pub fn active_path(&self, context_key: &str) -> Option<&str> {
        self.tabs.active_path(context_key)
    }

    pub fn is_open(&self, context_key: &str) -> bool {
        self.active_path(context_key).is_some()
    }

    pub fn display_mode(&self) -> PreviewDisplayMode {
        self.display_mode
    }

    fn toggle_display_mode(&mut self, cx: &mut Context<Self>) {
        self.display_mode = self.display_mode.toggled();
        cx.emit(FilePreviewEvent::DisplayModeChanged(self.display_mode));
        cx.notify();
    }

    pub fn set_context(&mut self, context_key: Option<String>, cx: &mut Context<Self>) {
        if self.active_context == context_key {
            return;
        }
        self.active_context = context_key;
        self.load_active(cx);
    }

    pub fn open(
        &mut self,
        context_key: String,
        root: PathBuf,
        relative_path: String,
        cx: &mut Context<Self>,
    ) {
        self.activate_surface(context_key.clone(), root, relative_path.clone(), cx);
        cx.emit(FilePreviewEvent::ActiveChanged {
            context_key,
            relative_path: Some(relative_path),
        });
    }

    pub fn activate_surface(
        &mut self,
        context_key: String,
        root: PathBuf,
        relative_path: String,
        cx: &mut Context<Self>,
    ) {
        let already_active = self.active_context.as_deref() == Some(context_key.as_str())
            && self.tabs.active_path(&context_key) == Some(relative_path.as_str())
            && self.roots.get(&context_key) == Some(&root);
        self.roots.insert(context_key.clone(), root);
        self.tabs.open(&context_key, &relative_path);
        self.active_context = Some(context_key);
        if !already_active {
            self.load_active(cx);
        }
    }
    pub fn close_path(&mut self, context_key: &str, relative_path: &str, cx: &mut Context<Self>) {
        let is_active_context = self.active_context.as_deref() == Some(context_key);
        let was_active_tab =
            is_active_context && self.tabs.active_path(context_key) == Some(relative_path);
        self.tabs.close(context_key, relative_path);
        if self.tabs.paths(context_key).is_empty() {
            self.remote_sources.remove(context_key);
        }
        self.markdown_lists
            .remove(&(context_key.to_owned(), relative_path.to_owned()));
        self.scroll_handles
            .remove(&(context_key.to_string(), relative_path.to_string()));
        if is_active_context {
            if was_active_tab {
                self.load_active(cx);
            }
            cx.emit(FilePreviewEvent::ActiveChanged {
                context_key: context_key.to_string(),
                relative_path: self.tabs.active_path(context_key).map(str::to_owned),
            });
        }
    }

    pub fn close_all(&mut self, cx: &mut Context<Self>) {
        let Some(context_key) = self.active_context.clone() else {
            return;
        };
        self.remote_sources.remove(&context_key);
        let paths = self.tabs.paths(&context_key).to_vec();
        for path in paths {
            self.tabs.close(&context_key, &path);
            self.markdown_lists
                .remove(&(context_key.clone(), path.clone()));
            self.scroll_handles.remove(&(context_key.clone(), path));
        }
        self.load_active(cx);
        cx.emit(FilePreviewEvent::ActiveChanged {
            context_key,
            relative_path: None,
        });
    }

    fn load_active(&mut self, cx: &mut Context<Self>) {
        self.clear_native_document();
        self.markdown_cache.borrow_mut().clear();
        self.generation = self.generation.wrapping_add(1);
        let generation = self.generation;
        let Some(context_key) = self.active_context.clone() else {
            self.loaded = PreviewLoadState::Idle;
            cx.notify();
            return;
        };
        let Some(relative_path) = self.tabs.active_path(&context_key).map(str::to_owned) else {
            self.loaded = PreviewLoadState::Idle;
            cx.notify();
            return;
        };
        let Some(root) = self.roots.get(&context_key).cloned() else {
            self.loaded = PreviewLoadState::Error("Project folder is unavailable.".into());
            cx.notify();
            return;
        };
        self.loaded = PreviewLoadState::Loading;
        let font_mono = Theme::of(cx).font_mono.clone();
        let font_size = px(12.5);
        let text_system = cx.text_system().clone();
        let remote = self.remote_sources.get(&context_key).cloned();
        let viewport_key = (context_key, relative_path.clone());
        self.load_task = Some(cx.spawn(async move |this, cx| {
            let result = if let Some(remote) = remote {
                let request = zeron_proto::ReadWorkspaceFileRequest {
                    target: remote.target,
                    path: relative_path.clone(),
                };
                let mut params = serde_json::to_value(request).unwrap();
                params["targetDeviceId"] = remote.device.into();
                match remote
                    .engine
                    .client()
                    .call_as::<zeron_proto::WorkspaceFileText>(
                        zeron_rpc::methods::READ_WORKSPACE_FILE,
                        params,
                    )
                    .await
                {
                    Ok(file) => match file.text {
                        Some(source) => {
                            cx.background_executor()
                                .spawn(async move {
                                    super::loader::load_text_preview(
                                        Path::new(&relative_path),
                                        source,
                                        font_mono,
                                        font_size,
                                        Some(text_system),
                                    )
                                })
                                .await
                        }
                        None => Err(PreviewLoadError::Remote(
                            "This remote file is not available as a text preview.".into(),
                        )),
                    },
                    Err(zeron_rpc::RpcError::UnknownMethod(_)) => Err(PreviewLoadError::Remote(
                        "Update the project device to enable file previews.".into(),
                    )),
                    Err(error) => Err(PreviewLoadError::Remote(format!("Remote file: {error}"))),
                }
            } else {
                cx.background_executor()
                    .spawn(async move {
                        load_preview_with_typography(
                            &root,
                            std::path::Path::new(&relative_path),
                            font_mono,
                            font_size,
                            Some(text_system),
                        )
                    })
                    .await
            };
            let _ = this.update(cx, |this, cx| {
                if this.generation != generation {
                    return;
                }
                this.loaded = match result {
                    Ok(preview) => PreviewLoadState::Ready(preview),
                    Err(error) => PreviewLoadState::Error(load_error_message(&error).into()),
                };
                if let PreviewLoadState::Ready(LoadedPreview::Markdown(tree)) = &this.loaded {
                    let (state, _) = this.markdown_lists.entry(viewport_key).or_insert_with(|| {
                        (
                            ListState::new(0, gpui::ListAlignment::Top, px(200.0)),
                            crate::theme::theme_generation(),
                        )
                    });
                    let mut position = state.logical_scroll_top();
                    position.item_ix = position.item_ix.min(tree.len().saturating_sub(1));
                    state.reset(tree.len());
                    state.scroll_to(position);
                }
                this.ensure_native_document();
                cx.notify();
            });
        }));
        cx.notify();
    }

    fn render_header(&mut self, theme: &Theme, cx: &mut Context<Self>) -> AnyElement {
        let Some(context_key) = self.active_context.as_deref() else {
            return gpui::Empty.into_any_element();
        };
        let Some(relative_path) = self.tabs.active_path(context_key).map(str::to_owned) else {
            return gpui::Empty.into_any_element();
        };
        let root = self.roots.get(context_key).cloned().unwrap_or_default();
        let absolute = root.join(&relative_path);
        let name = file_name(&relative_path).to_string();
        let icon_path = material_icon_path(&name, false, false);
        let image = icons::material_file_icon_image(icon_path.as_ref())
            .expect("material file icon is embedded");
        let close_path = relative_path.clone();
        let close_context = context_key.to_string();
        let remote = self.remote_sources.contains_key(context_key);
        let reveal = absolute.clone();
        let copy = absolute;
        div()
            .h(px(36.0))
            .flex_none()
            .px(px(Theme::SPACE_MD))
            .border_b_1()
            .border_color(crate::theme::hairline(0.06))
            .flex()
            .items_center()
            .justify_between()
            .child(
                div()
                    .min_w_0()
                    .flex()
                    .items_center()
                    .gap(px(9.0))
                    .child(
                        div()
                            .id("file-preview-close")
                            .size(px(26.0))
                            .rounded(px(6.0))
                            .flex()
                            .items_center()
                            .justify_center()
                            .cursor_pointer()
                            .hover(|style| style.bg(crate::theme::ink(0.05)))
                            .on_click(cx.listener(move |_, _, _, cx| {
                                cx.emit(FilePreviewEvent::CloseRequested {
                                    context_key: close_context.clone(),
                                    relative_path: close_path.clone(),
                                });
                            }))
                            .child(
                                icons::icon(icons::DETAILS_CHEVRONS_RIGHT)
                                    .size(px(16.0))
                                    .text_color(theme.text_muted),
                            ),
                    )
                    .child(img(image).size(px(14.0)).object_fit(ObjectFit::Contain))
                    .child(
                        div()
                            .min_w_0()
                            .truncate()
                            .text_size(px(12.0))
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .text_color(theme.text)
                            .child(name),
                    ),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(4.0))
                    .child(
                        div()
                            .id("file-preview-expand")
                            .size(px(28.0))
                            .rounded(px(6.0))
                            .flex()
                            .items_center()
                            .justify_center()
                            .cursor_pointer()
                            .hover(|style| style.bg(crate::theme::ink(0.05)))
                            .on_click(cx.listener(|this, _, _, cx| this.toggle_display_mode(cx)))
                            .child(
                                icons::icon(icons::EXPAND_ARROWS)
                                    .size(px(15.0))
                                    .text_color(theme.text_muted),
                            ),
                    )
                    .when(!remote, |row| {
                        row.child(
                            div()
                                .id("file-preview-reveal")
                                .h(px(28.0))
                                .px(px(10.0))
                                .rounded(px(6.0))
                                .flex()
                                .items_center()
                                .gap(px(6.0))
                                .cursor_pointer()
                                .hover(|style| style.bg(crate::theme::ink(0.05)))
                                .text_size(px(12.0))
                                .text_color(theme.text_muted)
                                .on_click(move |_, _, cx| {
                                    let path = reveal.clone();
                                    cx.background_executor()
                                        .spawn(async move {
                                            let _ = std::process::Command::new("open")
                                                .arg(path)
                                                .status();
                                        })
                                        .detach();
                                })
                                .child("Open in")
                                .child(
                                    icons::icon(icons::WORKER_OPEN_CODE)
                                        .size(px(14.0))
                                        .text_color(theme.text_muted),
                                ),
                        )
                    })
                    .child(
                        div()
                            .id("file-preview-copy-path")
                            .size(px(28.0))
                            .rounded(px(6.0))
                            .flex()
                            .items_center()
                            .justify_center()
                            .cursor_pointer()
                            .hover(|style| style.bg(crate::theme::ink(0.05)))
                            .on_click(move |_, _, cx| {
                                cx.write_to_clipboard(ClipboardItem::new_string(
                                    copy.to_string_lossy().to_string(),
                                ));
                            })
                            .child(
                                icons::icon(icons::COPY)
                                    .size(px(15.0))
                                    .text_color(theme.text_muted),
                            ),
                    ),
            )
            .into_any_element()
    }

    fn render_content(&mut self, window: &mut Window, theme: &Theme) -> AnyElement {
        let Some(context_key) = self.active_context.as_deref() else {
            return gpui::Empty.into_any_element();
        };
        let path = self
            .tabs
            .active_path(context_key)
            .unwrap_or_default()
            .to_string();
        let loaded = self.loaded.clone();
        match loaded {
            PreviewLoadState::Idle => gpui::Empty.into_any_element(),
            PreviewLoadState::Loading => centered_message("Loading file…", theme),
            PreviewLoadState::Error(message) => centered_message(message, theme),
            PreviewLoadState::Ready(LoadedPreview::Unsupported) => {
                centered_message("Cannot view this file", theme)
            }
            PreviewLoadState::Ready(LoadedPreview::Markdown(tree)) => {
                if tree.is_empty() {
                    return centered_message("Empty file", theme);
                }
                let (state, generation) = self
                    .markdown_lists
                    .get_mut(&(context_key.to_owned(), path.clone()))
                    .expect("Markdown viewport is prepared when loading completes");
                let current_generation = crate::theme::theme_generation();
                if *generation != current_generation {
                    state.remeasure();
                    *generation = current_generation;
                }
                let mut options = markdown_render::RenderOptions::settled(
                    format!("file-preview:{context_key}:{path}").into(),
                );
                options.cache = Some(self.markdown_cache.clone());
                let theme = theme.clone();
                list(state.clone(), move |ix, window, _cx| {
                    div()
                        .px(px(28.0))
                        .pt(px(if ix == 0 {
                            24.0
                        } else {
                            markdown_render::MD_BLOCK_GAP
                        }))
                        .when(ix + 1 == tree.len(), |row| row.pb(px(24.0)))
                        .child(markdown_render::render_block(
                            &tree.blocks[ix].block,
                            ix,
                            ix,
                            &options,
                            &theme,
                            window,
                            None,
                        ))
                        .into_any_element()
                })
                .size_full()
                .into_any_element()
            }
            PreviewLoadState::Ready(LoadedPreview::Code {
                lines,
                highlights,
                widest_line_ix,
            }) => {
                let scroll_handle = self
                    .scroll_handles
                    .entry((context_key.to_string(), path.clone()))
                    .or_default()
                    .clone();
                render_code(
                    context_key,
                    &path,
                    lines,
                    highlights,
                    widest_line_ix,
                    &scroll_handle,
                    theme,
                )
            }
            PreviewLoadState::Ready(LoadedPreview::Html(_))
            | PreviewLoadState::Ready(LoadedPreview::Pdf)
            | PreviewLoadState::Ready(LoadedPreview::Video) => {
                self.render_native_document(window, theme)
            }
            PreviewLoadState::Ready(LoadedPreview::Image(image)) => render_image(image, theme),
            PreviewLoadState::Ready(LoadedPreview::Table(rows)) => render_data(&path, rows, theme),
        }
    }

    fn clear_native_document(&mut self) {
        #[cfg(target_os = "macos")]
        if let Some((_, view)) = self.native_document.take() {
            view.borrow_mut().hide();
        }
    }

    fn active_absolute_path(&self) -> Option<PathBuf> {
        let context_key = self.active_context.as_deref()?;
        let relative = self.tabs.active_path(context_key)?;
        let root = self.roots.get(context_key)?;
        Some(preview_absolute_path(root, relative))
    }

    fn ensure_native_document(&mut self) {
        #[cfg(target_os = "macos")]
        {
            let Some(absolute) = self.active_absolute_path() else {
                self.clear_native_document();
                return;
            };
            let action = {
                let existing = self
                    .native_document
                    .as_ref()
                    .map(|(path, _)| path.as_path());
                native_document_action(&self.loaded, &absolute, existing)
            };
            match action {
                NativeDocumentAction::Hide => self.clear_native_document(),
                NativeDocumentAction::Keep => {}
                NativeDocumentAction::OpenHtml => {
                    let document = match &self.loaded {
                        PreviewLoadState::Ready(LoadedPreview::Html(document)) => document.clone(),
                        _ => {
                            self.clear_native_document();
                            return;
                        }
                    };
                    self.clear_native_document();
                    if let Some(view) =
                        super::native_document::NativeDocumentView::open_html(document.as_ref())
                    {
                        self.native_document = Some((absolute, Rc::new(RefCell::new(view))));
                    }
                }
                NativeDocumentAction::OpenPdf => {
                    self.clear_native_document();
                    if let Some(view) =
                        super::native_document::NativeDocumentView::open_pdf(&absolute)
                    {
                        self.native_document = Some((absolute, Rc::new(RefCell::new(view))));
                    }
                }
                NativeDocumentAction::OpenVideo => {
                    self.clear_native_document();
                    if let Some(view) =
                        super::native_document::NativeDocumentView::open_video(&absolute)
                    {
                        self.native_document = Some((absolute, Rc::new(RefCell::new(view))));
                    }
                }
            }
        }
    }

    fn render_native_document(&mut self, window: &Window, theme: &Theme) -> AnyElement {
        #[cfg(target_os = "macos")]
        {
            let Some((_, view)) = self.native_document.as_ref() else {
                return centered_message("The native preview could not be opened.", theme);
            };
            let view = view.clone();
            let viewport_height = f32::from(window.viewport_size().height) as f64;
            return gpui::canvas(
                move |bounds, _, _| {
                    view.borrow_mut().attach_and_layout(
                        f32::from(bounds.origin.x) as f64,
                        f32::from(bounds.origin.y) as f64,
                        f32::from(bounds.size.width) as f64,
                        f32::from(bounds.size.height) as f64,
                        viewport_height,
                    );
                },
                |_, _, _, _| {},
            )
            .size_full()
            .into_any_element();
        }
        #[cfg(not(target_os = "macos"))]
        {
            let _ = window;
            centered_message("Open this file in its native app to preview it.", theme)
        }
    }
}

impl Default for FilePreview {
    fn default() -> Self {
        Self::new()
    }
}

impl EventEmitter<FilePreviewEvent> for FilePreview {}

impl Render for FilePreview {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = Theme::of(cx).clone();
        div()
            .size_full()
            .flex()
            .flex_col()
            // The shell owns the themed utility-pane surface, as for Changes.
            .child(self.render_header(&theme, cx))
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .child(self.render_content(window, &theme)),
            )
    }
}

fn file_name(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

fn load_error_message(error: &PreviewLoadError) -> &str {
    match error {
        PreviewLoadError::OutsideCheckout => "This file is outside the project.",
        PreviewLoadError::Missing => "This file no longer exists.",
        PreviewLoadError::TooLarge => "This file is too large to preview safely.",
        PreviewLoadError::InvalidUtf8 => "This text file is not valid UTF-8.",
        PreviewLoadError::Remote(message) => message,
        PreviewLoadError::Io(_) => "The file could not be read.",
    }
}

fn centered_message(message: impl Into<SharedString>, theme: &Theme) -> AnyElement {
    div()
        .id("file-preview-message")
        .size_full()
        .flex()
        .items_center()
        .justify_center()
        .text_size(px(13.0))
        .text_color(theme.text_muted)
        .child(message.into())
        .into_any_element()
}

fn render_code(
    context_key: &str,
    path: &str,
    lines: Arc<[SharedString]>,
    highlights: Option<Arc<HighlightedDocument>>,
    widest_line_ix: Option<usize>,
    scroll_handle: &UniformListScrollHandle,
    theme: &Theme,
) -> AnyElement {
    let mono = font(theme.font_mono.clone());
    let sampled = minimap_sample_indices(lines.len(), 240);
    let minimap_lines = lines.clone();
    let minimap_highlights = highlights.clone();
    let minimap = div()
        .w(px(72.0))
        .h_full()
        .flex_none()
        .relative()
        .overflow_hidden()
        .bg(crate::theme::ink(0.018))
        .border_l_1()
        .border_color(theme.border)
        .py(px(8.0))
        .flex()
        .flex_col()
        .gap(px(1.0))
        .children(sampled.into_iter().map(move |index| {
            let line = &minimap_lines[index];
            let width = (line.trim().chars().count() as f32 * 0.7).clamp(3.0, 62.0);
            let color = minimap_highlights
                .as_deref()
                .and_then(|document| document.lines.get(index))
                .and_then(|spans| spans.first())
                .map(|span| theme.syntax.color(span.kind).opacity(0.55))
                .unwrap_or_else(|| theme.text_faint.opacity(0.45));
            div()
                .ml(px(4.0))
                .w(px(width))
                .h(px(1.0))
                .flex_none()
                .bg(color)
        }))
        .child(
            div()
                .absolute()
                .top(px(7.0))
                .left_0()
                .right_0()
                .h(px(48.0))
                .border_1()
                .border_color(theme.text_faint.opacity(0.18))
                .bg(crate::theme::ink(0.025)),
        );
    let code_lines = lines.clone();
    let code_highlights = highlights;
    let code_theme = theme.clone();
    let mono_font = mono.clone();
    let line_count = code_lines.len();

    let scroll_id = SharedString::from(format!("file-preview-code-scroll:{context_key}:{path}"));
    let code = div()
        .id(SharedString::from(format!(
            "file-preview-code-wrapper:{context_key}:{path}"
        )))
        .flex_1()
        .min_w_0()
        .h_full()
        .py(px(10.0))
        .child(
            uniform_list(scroll_id, line_count, move |range, _window, _cx| {
                range
                    .map(|index| {
                        let line = code_lines[index].clone();
                        let runs = markdown_render::runs_for_syntax_line_with_plain(
                            line.as_ref(),
                            code_highlights
                                .as_deref()
                                .and_then(|document| document.lines.get(index))
                                .map(Vec::as_slice)
                                .unwrap_or_default(),
                            &mono_font,
                            code_theme.text.opacity(0.92),
                            &code_theme,
                        );
                        div()
                            .h(px(20.0))
                            .min_w_full()
                            .flex()
                            .items_center()
                            .font_family(code_theme.font_mono.clone())
                            .text_size(px(12.5))
                            .child(
                                div()
                                    .w(px(64.0))
                                    .flex_none()
                                    .pr(px(12.0))
                                    .flex()
                                    .justify_end()
                                    .text_color(code_theme.text_faint)
                                    .child((index + 1).to_string()),
                            )
                            .child(
                                div()
                                    .min_w_0()
                                    .whitespace_nowrap()
                                    .child(StyledText::new(line).with_runs(runs)),
                            )
                    })
                    .collect::<Vec<_>>()
            })
            .with_horizontal_sizing_behavior(ListHorizontalSizingBehavior::Unconstrained)
            .with_width_from_item(widest_line_ix)
            .track_scroll(scroll_handle)
            .size_full(),
        );
    div()
        .size_full()
        .flex()
        .flex_row()
        .child(code)
        .child(minimap)
        .into_any_element()
}

fn minimap_sample_indices(line_count: usize, limit: usize) -> Vec<usize> {
    if line_count <= limit {
        return (0..line_count).collect();
    }
    (0..limit)
        .map(|sample| sample.saturating_mul(line_count) / limit)
        .collect()
}

fn render_data(path: &str, rows: Arc<[Vec<SharedString>]>, theme: &Theme) -> AnyElement {
    div()
        .id(SharedString::from(format!(
            "file-preview-data-scroll:{path}"
        )))
        .size_full()
        .overflow_scroll()
        .p(px(16.0))
        .children(
            (0..rows.len().min(2_000))
                .enumerate()
                .map(move |(row_index, source_index)| {
                    let row = &rows[source_index];
                    div()
                        .h(px(28.0))
                        .flex()
                        .items_center()
                        .border_b_1()
                        .border_color(theme.border)
                        .bg(if row_index == 0 {
                            crate::theme::ink(0.035)
                        } else {
                            gpui::transparent_black()
                        })
                        .children(row.iter().take(100).cloned().map(|cell| {
                            div()
                                .w(px(180.0))
                                .flex_none()
                                .px(px(9.0))
                                .truncate()
                                .text_size(px(12.0))
                                .text_color(theme.text)
                                .child(cell)
                        }))
                }),
        )
        .into_any_element()
}

fn render_image(image: Arc<Image>, _theme: &Theme) -> AnyElement {
    div()
        .size_full()
        .p(px(24.0))
        .flex()
        .items_center()
        .justify_center()
        .child(
            img(image)
                .max_w_full()
                .max_h_full()
                .object_fit(ObjectFit::Contain),
        )
        .into_any_element()
}

#[cfg(test)]
mod tests {
    #[test]
    fn code_minimap_is_bounded_for_large_files() {
        assert_eq!(
            super::minimap_sample_indices(10, 240),
            (0..10).collect::<Vec<_>>()
        );
        let sampled = super::minimap_sample_indices(1_000, 240);
        assert_eq!(sampled.len(), 240);
        assert_eq!(sampled.first(), Some(&0));
        assert!(sampled.last().is_some_and(|last| *last < 1_000));
    }

    #[test]
    fn native_webview_is_planned_from_load_state_not_from_paint() {
        use super::{NativeDocumentAction, PreviewLoadState, native_document_action};
        use crate::file_preview::loader::LoadedPreview;
        use std::path::Path;
        use std::sync::Arc;

        let html = PreviewLoadState::Ready(LoadedPreview::Html(Arc::from("<p>hi</p>")));
        let pdf = PreviewLoadState::Ready(LoadedPreview::Pdf);
        let video = PreviewLoadState::Ready(LoadedPreview::Video);
        let path = Path::new("/tmp/a.html");
        let other = Path::new("/tmp/b.html");

        assert_eq!(
            native_document_action(&html, path, None),
            NativeDocumentAction::OpenHtml
        );
        assert_eq!(
            native_document_action(&html, path, Some(path)),
            NativeDocumentAction::Keep
        );
        assert_eq!(
            native_document_action(&html, path, Some(other)),
            NativeDocumentAction::OpenHtml
        );
        assert_eq!(
            native_document_action(&pdf, Path::new("/tmp/a.pdf"), None),
            NativeDocumentAction::OpenPdf
        );
        assert_eq!(
            native_document_action(&video, Path::new("/tmp/a.mp4"), None),
            NativeDocumentAction::OpenVideo
        );
        assert_eq!(
            native_document_action(&PreviewLoadState::Loading, path, Some(path)),
            NativeDocumentAction::Hide
        );
        assert_eq!(
            native_document_action(&PreviewLoadState::Idle, path, None),
            NativeDocumentAction::Hide
        );
    }

    #[test]
    fn preview_absolute_path_keeps_out_of_checkout_keys() {
        use super::preview_absolute_path;
        use std::path::{Path, PathBuf};

        assert_eq!(
            preview_absolute_path(Path::new("/repo"), "docs/a.html"),
            PathBuf::from("/repo/docs/a.html")
        );
        assert_eq!(
            preview_absolute_path(Path::new("/repo"), "/tmp/out.html"),
            PathBuf::from("/tmp/out.html")
        );
    }
}
