use std::{cell::RefCell, collections::HashMap, path::PathBuf, rc::Rc, sync::Arc};

use comet_syntax::HighlightedDocument;
use gpui::{
    AnyElement, ClipboardItem, Context, EventEmitter, Image, IntoElement, ObjectFit, Render,
    SharedString, StyledText, Task, Window, div, font, img, prelude::*, px,
};

use crate::{
    details_sidebar::files_view::material_icon_path,
    file_preview::{
        loader::{LoadedPreview, PreviewLoadError, load_preview},
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

/// What the native host is being asked to paint. HTML arrives as a sanitized
/// string; PDF and video are loaded straight off disk by WebKit.
#[derive(Clone, Copy)]
enum NativeSource<'a> {
    Html(&'a str),
    Pdf,
    Video,
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

pub struct FilePreview {
    tabs: PreviewTabs,
    roots: HashMap<String, PathBuf>,
    active_context: Option<String>,
    loaded: PreviewLoadState,
    generation: u64,
    display_mode: PreviewDisplayMode,
    load_task: Option<Task<()>>,
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
            active_context: None,
            loaded: PreviewLoadState::Idle,
            generation: 0,
            display_mode: PreviewDisplayMode::SidePeek,
            load_task: None,
            #[cfg(target_os = "macos")]
            native_document: None,
        }
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
        self.tabs.close(context_key, relative_path);
        if self.active_context.as_deref() == Some(context_key) {
            self.load_active(cx);
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
        let paths = self.tabs.paths(&context_key).to_vec();
        for path in paths {
            self.tabs.close(&context_key, &path);
        }
        self.load_active(cx);
        cx.emit(FilePreviewEvent::ActiveChanged {
            context_key,
            relative_path: None,
        });
    }

    fn load_active(&mut self, cx: &mut Context<Self>) {
        self.clear_native_document();
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
        self.load_task = Some(cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { load_preview(&root, std::path::Path::new(&relative_path)) })
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.generation != generation {
                    return;
                }
                this.loaded = match result {
                    Ok(preview) => PreviewLoadState::Ready(preview),
                    Err(error) => PreviewLoadState::Error(load_error_message(&error).into()),
                };
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
        let reveal = absolute.clone();
        let copy = absolute;
        div()
            .h(px(44.0))
            .flex_none()
            .px(px(14.0))
            .border_b_1()
            .border_color(theme.border)
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
                    .child(img(image).size(px(16.0)).object_fit(ObjectFit::Contain))
                    .child(
                        div()
                            .min_w_0()
                            .truncate()
                            .text_size(px(14.0))
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
                    .child(
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
                                        let _ =
                                            std::process::Command::new("open").arg(path).status();
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
            // Scroll offset is element state keyed by id: a shared id handed
            // the next file the previous file's offset, so a short document
            // opened parked past its own end (user report: "opens in the
            // middle, shows nothing"). One id per path, one offset per file.
            PreviewLoadState::Ready(LoadedPreview::Markdown(tree)) => div()
                .id(SharedString::from(format!(
                    "file-preview-markdown-scroll:{path}"
                )))
                .size_full()
                .overflow_y_scroll()
                .px(px(28.0))
                .py(px(24.0))
                .child(markdown_render::render_tree(
                    tree.as_ref(),
                    &markdown_render::RenderOptions::settled(format!("file-preview:{path}").into()),
                    theme,
                    window,
                    &|_| None,
                ))
                .into_any_element(),
            PreviewLoadState::Ready(LoadedPreview::Code { lines, highlights }) => {
                render_code(&path, lines, highlights, theme)
            }
            PreviewLoadState::Ready(LoadedPreview::Html(document)) => {
                self.render_native_document(window, theme, NativeSource::Html(document.as_ref()))
            }
            PreviewLoadState::Ready(LoadedPreview::Image(image)) => render_image(image, theme),
            PreviewLoadState::Ready(LoadedPreview::Pdf) => {
                self.render_native_document(window, theme, NativeSource::Pdf)
            }
            PreviewLoadState::Ready(LoadedPreview::Video) => {
                self.render_native_document(window, theme, NativeSource::Video)
            }
            PreviewLoadState::Ready(LoadedPreview::Table(rows)) => render_data(&path, rows, theme),
        }
    }

    fn clear_native_document(&mut self) {
        #[cfg(target_os = "macos")]
        if let Some((_, view)) = self.native_document.take() {
            view.borrow_mut().hide();
        }
    }

    fn render_native_document(
        &mut self,
        window: &Window,
        theme: &Theme,
        source: NativeSource<'_>,
    ) -> AnyElement {
        #[cfg(target_os = "macos")]
        {
            let Some(context_key) = self.active_context.as_deref() else {
                return gpui::Empty.into_any_element();
            };
            let Some(relative_path) = self.tabs.active_path(context_key) else {
                return gpui::Empty.into_any_element();
            };
            let Some(root) = self.roots.get(context_key).cloned() else {
                return centered_message("Project folder is unavailable.", theme);
            };
            let absolute = root.join(relative_path);
            let needs_new = self
                .native_document
                .as_ref()
                .is_none_or(|(path, _)| path != &absolute);
            if needs_new {
                self.clear_native_document();
                let view = match source {
                    NativeSource::Html(document) => {
                        super::native_document::NativeDocumentView::open_html(document)
                    }
                    NativeSource::Pdf => {
                        super::native_document::NativeDocumentView::open_pdf(&absolute)
                    }
                    NativeSource::Video => {
                        super::native_document::NativeDocumentView::open_video(&absolute)
                    }
                };
                let Some(view) = view else {
                    return centered_message("The native preview could not be opened.", theme);
                };
                self.native_document = Some((absolute, Rc::new(RefCell::new(view))));
            }
            let view = self.native_document.as_ref().unwrap().1.clone();
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
        centered_message("Open this file in its native app to preview it.", theme)
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
            .bg(theme.bg)
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

fn load_error_message(error: &PreviewLoadError) -> &'static str {
    match error {
        PreviewLoadError::OutsideCheckout => "This file is outside the project.",
        PreviewLoadError::Missing => "This file no longer exists.",
        PreviewLoadError::TooLarge => "This file is too large to preview safely.",
        PreviewLoadError::InvalidUtf8 => "This text file is not valid UTF-8.",
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
    path: &str,
    lines: Arc<[SharedString]>,
    highlights: Option<Arc<HighlightedDocument>>,
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
    let code = div()
        .id(SharedString::from(format!(
            "file-preview-code-scroll:{path}"
        )))
        .flex_1()
        .min_w_0()
        .h_full()
        .overflow_scroll()
        .py(px(10.0))
        .children((0..code_lines.len()).map(move |index| {
            let line = code_lines[index].clone();
            let runs = markdown_render::runs_for_syntax_line_with_plain(
                line.as_ref(),
                code_highlights
                    .as_deref()
                    .and_then(|document| document.lines.get(index))
                    .map(Vec::as_slice)
                    .unwrap_or_default(),
                &mono,
                theme.text.opacity(0.92),
                theme,
            );
            div()
                .h(px(20.0))
                .min_w_full()
                .flex()
                .items_center()
                .font_family(theme.font_mono.clone())
                .text_size(px(12.5))
                .child(
                    div()
                        .w(px(54.0))
                        .flex_none()
                        .pr(px(14.0))
                        .flex()
                        .justify_end()
                        .text_color(theme.text_faint)
                        .child((index + 1).to_string()),
                )
                .child(
                    div()
                        .min_w_0()
                        .whitespace_nowrap()
                        .child(StyledText::new(line).with_runs(runs)),
                )
        }));
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
}
