use std::time::{SystemTime, UNIX_EPOCH};

use gpui::{
    AnyElement, Context, Entity, IntoElement, PathPromptOptions, Render, SharedString,
    Subscription, Task, Window, div, prelude::*, px,
};
use zeron_workers_unpeel::{WorkersLaunchRequest, WorkersPreset, WorkersProject, WorkersSession};

use crate::composer::{ComposerInput, ComposerInputEvent};
use crate::icons::{self, icon};
use crate::popover;
use crate::theme::Theme;

use super::model::{WorkersModel, WorkersRoute, WorkersSettingsTab};
use super::presentation::{SessionIndicator, session_indicator, spinner_frame};
use super::settings::WorkersSettingsView;
use super::terminal::WorkersTerminal;

fn project_depth(project: &WorkersProject, projects: &[WorkersProject]) -> usize {
    let mut depth = 0;
    let mut parent = project.parent_project_id.as_deref();
    while let Some(parent_id) = parent {
        let Some(project) = projects.iter().find(|project| project.id == parent_id) else {
            break;
        };
        depth += 1;
        if depth >= 8 {
            break;
        }
        parent = project.parent_project_id.as_deref();
    }
    depth
}

fn project_visible(
    project: &WorkersProject,
    projects: &[WorkersProject],
    expanded: &std::collections::HashSet<String>,
) -> bool {
    let mut parent = project.parent_project_id.as_deref();
    let mut depth = 0;
    while let Some(parent_id) = parent {
        if !expanded.contains(parent_id) {
            return false;
        }
        let Some(project) = projects.iter().find(|project| project.id == parent_id) else {
            break;
        };
        parent = project.parent_project_id.as_deref();
        depth += 1;
        if depth >= 8 {
            break;
        }
    }
    true
}

pub struct WorkersSidebar {
    model: Entity<WorkersModel>,
    picker_task: Option<Task<()>>,
    _spinner_task: Task<()>,
    _model_observation: Subscription,
}

impl WorkersSidebar {
    pub fn new(model: Entity<WorkersModel>, cx: &mut Context<Self>) -> Self {
        let model_observation = cx.observe(&model, |_, _, cx| cx.notify());
        let spinner_task = cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(std::time::Duration::from_millis(120))
                    .await;
                if this.update(cx, |_, cx| cx.notify()).is_err() {
                    break;
                }
            }
        });
        Self {
            model,
            picker_task: None,
            _spinner_task: spinner_task,
            _model_observation: model_observation,
        }
    }

    fn open_project_picker(&mut self, cx: &mut Context<Self>) {
        let rx = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some("Add Project".into()),
        });
        self.picker_task = Some(cx.spawn(async move |this, cx| {
            if let Ok(Ok(Some(paths))) = rx.await
                && let Some(path) = paths.into_iter().next()
            {
                this.update(cx, |sidebar, cx| {
                    sidebar
                        .model
                        .update(cx, |model, cx| model.add_project(path, cx));
                })
                .ok();
            }
        }));
    }

    fn render_settings_nav(
        &self,
        selected: WorkersSettingsTab,
        theme: &Theme,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let rows = WorkersSettingsTab::ALL
            .into_iter()
            .enumerate()
            .map(|(index, tab)| {
                div()
                    .id(("workers-settings-tab", index))
                    .h(px(34.0))
                    .mx(px(8.0))
                    .px(px(12.0))
                    .flex()
                    .items_center()
                    .rounded(px(9.0))
                    .cursor_pointer()
                    .text_size(px(12.5))
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .text_color(if tab == selected {
                        theme.text
                    } else {
                        theme.text_muted
                    })
                    .when(tab == selected, |el| el.bg(crate::theme::ink(0.15)))
                    .hover(|el| el.bg(crate::theme::ink(0.09)))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.model
                            .update(cx, |model, cx| model.set_settings_tab(tab, cx));
                    }))
                    .child(tab.label())
            });
        div()
            .size_full()
            .flex()
            .flex_col()
            .child(
                div()
                    .id("workers-settings-back")
                    .h(px(42.0))
                    .mx(px(8.0))
                    .px(px(10.0))
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .rounded(px(9.0))
                    .cursor_pointer()
                    .text_size(px(12.5))
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .text_color(theme.text_muted)
                    .hover(|el| el.bg(crate::theme::ink(0.08)))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.model.update(cx, |model, cx| model.close_settings(cx));
                    }))
                    .child(
                        icon(icons::ALT_ARROW_LEFT)
                            .size(px(13.0))
                            .text_color(theme.text_faint),
                    )
                    .child("Back"),
            )
            .child(
                div()
                    .mt(px(8.0))
                    .flex()
                    .flex_col()
                    .gap(px(4.0))
                    .children(rows),
            )
            .into_any_element()
    }

    fn render_project(
        &self,
        project: WorkersProject,
        sessions: Vec<WorkersSession>,
        presets: Vec<WorkersPreset>,
        expanded: bool,
        selected_session_id: Option<&str>,
        depth: usize,
        index: usize,
        theme: &Theme,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let project_id = project.id.clone();
        let select_project_id = project.id.clone();
        let toggle_project_id = project.id.clone();
        let project_name: SharedString = project.name.clone().into();
        let is_group = project.is_group;
        let chevron = if expanded {
            icons::ALT_ARROW_DOWN
        } else {
            icons::ALT_ARROW_RIGHT
        };

        let rows = sessions
            .into_iter()
            .enumerate()
            .map(|(session_index, session)| {
                self.render_session(
                    session,
                    selected_session_id,
                    depth,
                    index * 10_000 + session_index,
                    theme,
                    cx,
                )
            })
            .collect::<Vec<_>>();

        let terminal_project_id = project.id.clone();
        let terminal_worktree_path = project
            .worktree_branch
            .as_ref()
            .map(|_| project.path.clone());
        let terminal_worktree_branch = project.worktree_branch.clone();
        let quick_launch = presets
            .into_iter()
            .filter(|preset| preset.enabled && preset.quick_launch)
            .take(3)
            .enumerate()
            .map(|(quick_index, preset)| {
                let project_id = project.id.clone();
                let preset_id = preset.id.clone();
                let worktree_path = project
                    .worktree_branch
                    .as_ref()
                    .map(|_| project.path.clone());
                let worktree_branch = project.worktree_branch.clone();
                div()
                    .id(("workers-project-quick", index * 10 + quick_index))
                    .size(px(22.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(7.0))
                    .cursor_pointer()
                    .hover(|el| el.bg(crate::theme::ink(0.10)))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        cx.stop_propagation();
                        this.model.update(cx, |model, cx| {
                            model.launch(
                                WorkersLaunchRequest::preset(project_id.clone(), preset_id.clone())
                                    .with_optional_worktree(
                                        worktree_path.clone(),
                                        worktree_branch.clone(),
                                    ),
                                cx,
                            )
                        });
                    }))
                    .child(
                        icon(icons::TERMINAL)
                            .size(px(12.0))
                            .text_color(theme.text_muted),
                    )
                    .into_any_element()
            })
            .collect::<Vec<_>>();

        div()
            .w_full()
            .flex()
            .flex_col()
            .child(
                div()
                    .id(("workers-project-row", index))
                    .h(px(30.0))
                    .ml(px(8.0 + depth as f32 * 12.0))
                    .mr(px(8.0))
                    .px(px(7.0))
                    .flex()
                    .items_center()
                    .gap(px(6.0))
                    .rounded(px(9.0))
                    .cursor_pointer()
                    .hover(|el| el.bg(crate::theme::ink(0.07)))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.model.update(cx, |model, cx| {
                            if !is_group {
                                model.select_project(select_project_id.clone(), cx);
                            }
                            model.toggle_project(&toggle_project_id, cx);
                        });
                    }))
                    .child(
                        icon(icons::FOLDER)
                            .size(px(14.0))
                            .text_color(theme.text_muted),
                    )
                    .child(
                        div()
                            .min_w_0()
                            .flex_1()
                            .truncate()
                            .text_size(px(12.5))
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .text_color(theme.text)
                            .child(project_name),
                    )
                    .when(!is_group, |el| {
                        el.child(
                            div()
                                .id(("workers-project-terminal", index))
                                .size(px(22.0))
                                .flex()
                                .items_center()
                                .justify_center()
                                .rounded(px(7.0))
                                .cursor_pointer()
                                .hover(|el| el.bg(crate::theme::ink(0.10)))
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    cx.stop_propagation();
                                    this.model.update(cx, |model, cx| {
                                        model.launch(
                                            WorkersLaunchRequest::terminal(
                                                terminal_project_id.clone(),
                                            )
                                            .with_optional_worktree(
                                                terminal_worktree_path.clone(),
                                                terminal_worktree_branch.clone(),
                                            ),
                                            cx,
                                        )
                                    });
                                }))
                                .child(
                                    icon(icons::TERMINAL)
                                        .size(px(12.0))
                                        .text_color(theme.text_muted),
                                ),
                        )
                    })
                    .children(quick_launch)
                    .when(!is_group, |el| {
                        el.child(
                            div()
                                .id(("workers-project-add", index))
                                .size(px(22.0))
                                .flex()
                                .items_center()
                                .justify_center()
                                .rounded(px(7.0))
                                .hover(|el| el.bg(crate::theme::ink(0.08)))
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    cx.stop_propagation();
                                    this.model.update(cx, |model, cx| {
                                        model.select_project(project_id.clone(), cx)
                                    });
                                }))
                                .child(
                                    icon(icons::PLUS)
                                        .size(px(13.0))
                                        .text_color(theme.text_muted),
                                ),
                        )
                    })
                    .when(is_group, |el| {
                        el.child(icon(chevron).size(px(11.0)).text_color(theme.text_faint))
                    }),
            )
            .when(expanded, |el| {
                if rows.is_empty() && !is_group {
                    el.child(
                        div()
                            .h(px(28.0))
                            .pl(px(38.0 + depth as f32 * 12.0))
                            .flex()
                            .items_center()
                            .text_size(px(11.0))
                            .text_color(theme.text_faint)
                            .child("No sessions yet."),
                    )
                } else {
                    el.children(rows)
                }
            })
            .into_any_element()
    }

    fn render_session(
        &self,
        session: WorkersSession,
        selected_session_id: Option<&str>,
        depth: usize,
        index: usize,
        theme: &Theme,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let selected = selected_session_id == Some(session.id.as_str());
        let session_id = session.id.clone();
        let title: SharedString = if session.title.trim().is_empty() {
            "Untitled session".into()
        } else {
            session.title.clone().into()
        };
        let indicator = session_indicator(
            &session.state,
            &session.activity,
            session.unread,
            session.runtime_launch_pending,
        );
        let indicator_color = match indicator {
            SessionIndicator::Busy | SessionIndicator::Restarting => theme.accent,
            SessionIndicator::Attention => theme.warning,
            SessionIndicator::Unread => theme.accent,
            SessionIndicator::Idle => gpui::transparent_black(),
            SessionIndicator::Exited => theme.text_faint.opacity(0.45),
        };
        let now_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;
        let runtime_label = session
            .active_runtime_id
            .as_deref()
            .or(session.provider_id.as_deref())
            .unwrap_or("terminal")
            .chars()
            .next()
            .unwrap_or('›')
            .to_ascii_uppercase()
            .to_string();

        div()
            .id(("workers-session-row", index))
            .h(px(28.0))
            .mx(px(8.0))
            .pl(px(29.0 + depth as f32 * 12.0))
            .pr(px(8.0))
            .flex()
            .items_center()
            .gap(px(7.0))
            .rounded(px(9.0))
            .cursor_pointer()
            .when(selected, |el| el.bg(crate::theme::ink(0.16)))
            .hover(|el| el.bg(crate::theme::ink(0.10)))
            .on_click(cx.listener(move |this, _, _, cx| {
                this.model
                    .update(cx, |model, cx| model.select_session(session_id.clone(), cx));
            }))
            .child(match indicator {
                SessionIndicator::Busy | SessionIndicator::Restarting => div()
                    .w(px(12.0))
                    .text_size(px(12.0))
                    .text_color(indicator_color)
                    .child(spinner_frame(now_ms)),
                SessionIndicator::Attention | SessionIndicator::Unread => div()
                    .w(px(12.0))
                    .flex()
                    .justify_center()
                    .child(div().size(px(7.0)).rounded_full().bg(indicator_color)),
                SessionIndicator::Idle | SessionIndicator::Exited => {
                    div().w(px(12.0)).flex().justify_center()
                }
            })
            .child(
                div()
                    .min_w_0()
                    .flex_1()
                    .truncate()
                    .text_size(px(12.0))
                    .text_color(if selected {
                        theme.text
                    } else if matches!(indicator, SessionIndicator::Exited) {
                        theme.text_muted.opacity(0.82)
                    } else {
                        theme.text_muted
                    })
                    .child(title),
            )
            .when(session.pinned, |el| {
                el.child(
                    icon(icons::STAR_BOLD)
                        .size(px(10.0))
                        .text_color(theme.warning),
                )
            })
            .child(
                div()
                    .text_size(px(9.5))
                    .text_color(theme.text_faint)
                    .child(format!("⌘{}", (index % 9) + 1)),
            )
            .child(
                div()
                    .w(px(13.0))
                    .text_size(px(9.5))
                    .font_weight(gpui::FontWeight::BOLD)
                    .text_color(theme.text_muted)
                    .child(runtime_label),
            )
            .into_any_element()
    }
}

impl Render for WorkersSidebar {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = Theme::of(cx).clone();
        let route = self.model.read(cx).route;
        if let WorkersRoute::Settings(tab) = route {
            return self.render_settings_nav(tab, &theme, cx);
        }
        let (
            loading,
            error,
            selected_session_id,
            expanded,
            projects,
            all_sessions,
            archive_project,
            presets,
            has_attention,
        ) = {
            let model = self.model.read(cx);
            (
                model.loading,
                model.error.clone(),
                model.selected_session_id.clone(),
                model.expanded_project_ids.clone(),
                model.projects().to_vec(),
                model.sessions().to_vec(),
                model
                    .selected_project()
                    .filter(|project| project.archived_session_count > 0)
                    .cloned(),
                model.presets().to_vec(),
                model.has_attention(),
            )
        };

        let rows = projects
            .iter()
            .filter(|project| project_visible(project, &projects, &expanded))
            .cloned()
            .enumerate()
            .map(|(index, project)| {
                let sessions = all_sessions
                    .iter()
                    .filter(|session| session.project_id == project.id && !session.archived)
                    .cloned()
                    .collect();
                self.render_project(
                    project.clone(),
                    sessions,
                    presets.clone(),
                    expanded.contains(&project.id),
                    selected_session_id.as_deref(),
                    project_depth(&project, &projects),
                    index,
                    &theme,
                    cx,
                )
            })
            .collect::<Vec<_>>();

        div()
            .size_full()
            .flex()
            .flex_col()
            .child(
                div()
                    .h(px(32.0))
                    .px(px(14.0))
                    .flex()
                    .items_center()
                    .child(
                        div()
                            .flex_1()
                            .text_size(px(10.0))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_color(theme.text_faint)
                            .child("Workers"),
                    )
                    .child(
                        div()
                            .relative()
                            .size(px(24.0))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(7.0))
                            .child(
                                icon(icons::BELL)
                                    .size(px(13.0))
                                    .text_color(theme.text_muted),
                            )
                            .when(has_attention, |el| {
                                el.child(
                                    div()
                                        .absolute()
                                        .top(px(3.0))
                                        .right(px(3.0))
                                        .size(px(6.0))
                                        .rounded_full()
                                        .bg(theme.accent),
                                )
                            }),
                    )
                    .child(
                        div()
                            .id("workers-refresh")
                            .size(px(24.0))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(7.0))
                            .cursor_pointer()
                            .hover(|el| el.bg(crate::theme::ink(0.08)))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.model.update(cx, |model, cx| model.refresh(cx));
                            }))
                            .child(
                                icon(icons::REFRESH)
                                    .size(px(13.0))
                                    .text_color(theme.text_muted),
                            ),
                    ),
            )
            .child(
                div()
                    .id("workers-project-list")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .when(loading, |el| {
                        el.child(
                            div()
                                .px(px(16.0))
                                .py(px(12.0))
                                .text_size(px(11.0))
                                .text_color(theme.text_faint)
                                .child("Loading local workers…"),
                        )
                    })
                    .when_some(error, |el, error| {
                        el.child(
                            div()
                                .mx(px(10.0))
                                .p(px(10.0))
                                .rounded(px(9.0))
                                .bg(theme.danger.opacity(0.08))
                                .text_size(px(11.0))
                                .text_color(theme.danger_muted)
                                .child(format!("Could not load workers. {error}")),
                        )
                    })
                    .when(!loading && projects.is_empty(), |el| {
                        el.child(
                            div()
                                .size_full()
                                .pb(px(72.0))
                                .flex()
                                .flex_col()
                                .items_center()
                                .justify_end()
                                .gap(px(12.0))
                                .child(
                                    icon(icons::FOLDER)
                                        .size(px(36.0))
                                        .text_color(theme.text_muted),
                                )
                                .child(
                                    div()
                                        .id("workers-add-project-empty")
                                        .h(px(34.0))
                                        .px(px(15.0))
                                        .flex()
                                        .items_center()
                                        .gap(px(8.0))
                                        .rounded(px(8.0))
                                        .bg(theme.text)
                                        .text_size(px(12.5))
                                        .font_weight(gpui::FontWeight::SEMIBOLD)
                                        .text_color(theme.bg)
                                        .cursor_pointer()
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            this.open_project_picker(cx)
                                        }))
                                        .child(
                                            icon(icons::FOLDER).size(px(14.0)).text_color(theme.bg),
                                        )
                                        .child("Add Project"),
                                ),
                        )
                    })
                    .children(rows),
            )
            .child(
                div()
                    .h(px(34.0))
                    .px(px(14.0))
                    .flex()
                    .items_center()
                    .gap(px(4.0))
                    .text_size(px(10.0))
                    .text_color(theme.text_faint)
                    .child(
                        div()
                            .id("workers-open-settings")
                            .size(px(24.0))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(7.0))
                            .cursor_pointer()
                            .hover(|el| el.bg(crate::theme::ink(0.08)))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.model.update(cx, |model, cx| {
                                    model.open_settings(WorkersSettingsTab::Presets, cx)
                                });
                            }))
                            .child(
                                icon(icons::SETTINGS_MINIMALISTIC)
                                    .size(px(13.0))
                                    .text_color(theme.text_muted),
                            ),
                    )
                    .child(
                        div()
                            .id("workers-add-project-footer")
                            .size(px(24.0))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(7.0))
                            .cursor_pointer()
                            .hover(|el| el.bg(crate::theme::ink(0.08)))
                            .on_click(cx.listener(|this, _, _, cx| this.open_project_picker(cx)))
                            .child(
                                icon(icons::PLUS)
                                    .size(px(13.0))
                                    .text_color(theme.text_muted),
                            ),
                    )
                    .child(div().flex_1())
                    .child(div().size(px(6.0)).rounded_full().bg(theme.success))
                    .when_some(archive_project, |el, project| {
                        let project_id = project.id.clone();
                        el.child(
                            div()
                                .id("workers-open-archive")
                                .h(px(24.0))
                                .px(px(7.0))
                                .flex()
                                .items_center()
                                .gap(px(5.0))
                                .rounded(px(7.0))
                                .cursor_pointer()
                                .hover(|el| el.bg(crate::theme::ink(0.08)))
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.model.update(cx, |model, cx| {
                                        model.open_archive(project_id.clone(), cx)
                                    });
                                }))
                                .child(
                                    icon(icons::ARCHIVE_MINIMALISTIC)
                                        .size(px(11.0))
                                        .text_color(theme.text_muted),
                                )
                                .child(project.archived_session_count.to_string()),
                        )
                    }),
            )
            .into_any_element()
    }
}

struct RenameDialog {
    session_id: String,
    input: Entity<ComposerInput>,
    _events: Subscription,
}

#[derive(Clone)]
struct RemoveConfirmation {
    session_id: String,
    title: String,
    archived: bool,
}

pub struct WorkersContent {
    model: Entity<WorkersModel>,
    terminal: Entity<WorkersTerminal>,
    settings: Entity<WorkersSettingsView>,
    rename: Option<RenameDialog>,
    remove_confirmation: Option<RemoveConfirmation>,
    _model_observation: Subscription,
}

impl WorkersContent {
    pub fn new(model: Entity<WorkersModel>, cx: &mut Context<Self>) -> Self {
        let terminal = cx.new(WorkersTerminal::new);
        let settings = cx.new(|cx| WorkersSettingsView::new(model.clone(), cx));
        let model_observation = cx.observe(&model, {
            let terminal = terminal.clone();
            move |_, model, cx| {
                let session_id = model.read(cx).selected_session_id.clone();
                terminal.update(cx, |terminal, cx| terminal.set_session(session_id, cx));
                cx.notify();
            }
        });
        Self {
            model,
            terminal,
            settings,
            rename: None,
            remove_confirmation: None,
            _model_observation: model_observation,
        }
    }

    fn open_rename(&mut self, session: WorkersSession, cx: &mut Context<Self>) {
        let input = cx.new(|cx| ComposerInput::new("Worker title", cx));
        input.update(cx, |input, cx| input.set_text(session.title, cx));
        let events = cx.subscribe(&input, |this: &mut Self, _, event, cx| {
            if matches!(event, ComposerInputEvent::Submitted) {
                this.submit_rename(cx);
            }
        });
        self.rename = Some(RenameDialog {
            session_id: session.id,
            input,
            _events: events,
        });
        cx.notify();
    }

    fn submit_rename(&mut self, cx: &mut Context<Self>) {
        let Some(dialog) = self.rename.take() else {
            return;
        };
        let title = dialog.input.read(cx).text().trim().to_owned();
        if title.is_empty() {
            cx.notify();
            return;
        }
        self.model
            .update(cx, |model, cx| model.rename(dialog.session_id, title, cx));
        cx.notify();
    }

    fn request_remove(&mut self, session: WorkersSession, archived: bool, cx: &mut Context<Self>) {
        self.remove_confirmation = Some(RemoveConfirmation {
            session_id: session.id,
            title: if session.title.trim().is_empty() {
                "Untitled session".to_owned()
            } else {
                session.title
            },
            archived,
        });
        cx.notify();
    }

    fn confirm_remove(&mut self, cx: &mut Context<Self>) {
        let Some(confirmation) = self.remove_confirmation.take() else {
            return;
        };
        self.model.update(cx, |model, cx| {
            if confirmation.archived {
                model.remove_archived(confirmation.session_id, cx);
            } else {
                model.remove(confirmation.session_id, cx);
            }
        });
        cx.notify();
    }

    fn render_rename_dialog(
        &mut self,
        viewport: gpui::Size<gpui::Pixels>,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let theme = Theme::of(cx).clone();
        let input = self.rename.as_ref()?.input.clone();
        let card = popover::dialog_card(&theme)
            .child(popover::dialog_title(&theme, "Rename worker"))
            .child(
                div()
                    .mt(px(12.0))
                    .child(popover::dialog_field(input.into_any_element())),
            )
            .child(
                div()
                    .mt(px(16.0))
                    .flex()
                    .justify_end()
                    .gap(px(8.0))
                    .child(
                        popover::btn_ghost(&theme, "Cancel", "workers-rename-cancel")
                            .id("workers-rename-cancel")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.rename = None;
                                cx.notify();
                            })),
                    )
                    .child(
                        popover::btn_primary(&theme, "Rename")
                            .id("workers-rename-save")
                            .on_click(cx.listener(|this, _, _, cx| this.submit_rename(cx))),
                    ),
            )
            .into_any_element();
        Some(popover::modal("workers-rename-dialog", viewport, card))
    }

    fn render_remove_dialog(
        &mut self,
        viewport: gpui::Size<gpui::Pixels>,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let theme = Theme::of(cx).clone();
        let confirmation = self.remove_confirmation.as_ref()?;
        let card = popover::dialog_card(&theme)
            .child(popover::dialog_title(&theme, "Remove worker permanently?"))
            .child(
                div()
                    .mt(px(10.0))
                    .text_size(px(11.0))
                    .text_color(theme.text_muted)
                    .child(format!(
                        "{} and its local session data will be removed. This cannot be undone.",
                        confirmation.title
                    )),
            )
            .child(
                div()
                    .mt(px(16.0))
                    .flex()
                    .justify_end()
                    .gap(px(8.0))
                    .child(
                        popover::btn_ghost(&theme, "Cancel", "workers-remove-cancel")
                            .id("workers-remove-cancel")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.remove_confirmation = None;
                                cx.notify();
                            })),
                    )
                    .child(
                        popover::btn_danger(&theme, "Remove")
                            .id("workers-remove-confirm")
                            .on_click(cx.listener(|this, _, _, cx| this.confirm_remove(cx))),
                    ),
            )
            .into_any_element();
        Some(popover::modal("workers-remove-dialog", viewport, card))
    }

    fn render_launcher(
        &self,
        project: WorkersProject,
        presets: Vec<WorkersPreset>,
        busy: bool,
        theme: &Theme,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let project_name: SharedString = project.name.clone().into();
        let project_path: SharedString = project.path.clone().into();
        let shell_project_id = project.id.clone();
        let shell_worktree_path = project
            .worktree_branch
            .as_ref()
            .map(|_| project.path.clone());
        let shell_worktree_branch = project.worktree_branch.clone();
        let preset_rows = presets
            .into_iter()
            .filter(|preset| preset.enabled)
            .enumerate()
            .map(|(index, preset)| {
                let project_id = project.id.clone();
                let preset_id = preset.id.clone();
                let worktree_path = project
                    .worktree_branch
                    .as_ref()
                    .map(|_| project.path.clone());
                let worktree_branch = project.worktree_branch.clone();
                div()
                    .id(("workers-preset", index))
                    .h(px(46.0))
                    .w_full()
                    .px(px(11.0))
                    .flex()
                    .items_center()
                    .gap(px(9.0))
                    .rounded(px(10.0))
                    .border_1()
                    .border_color(theme.border.opacity(0.75))
                    .cursor_pointer()
                    .hover(|el| el.bg(crate::theme::ink(0.07)))
                    .when(busy, |el| el.opacity(0.45))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.model.update(cx, |model, cx| {
                            model.launch(
                                WorkersLaunchRequest::preset(project_id.clone(), preset_id.clone())
                                    .with_optional_worktree(
                                        worktree_path.clone(),
                                        worktree_branch.clone(),
                                    ),
                                cx,
                            )
                        });
                    }))
                    .child(
                        icon(icons::TERMINAL)
                            .size(px(15.0))
                            .text_color(theme.text_muted),
                    )
                    .child(
                        div()
                            .min_w_0()
                            .flex_1()
                            .truncate()
                            .text_size(px(12.0))
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .text_color(theme.text_muted)
                            .child(preset.command),
                    )
                    .child(
                        icon(icons::ALT_ARROW_RIGHT)
                            .size(px(12.0))
                            .text_color(theme.text_faint),
                    )
                    .into_any_element()
            })
            .collect::<Vec<_>>();

        div()
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .child(
                div()
                    .w(px(460.0))
                    .max_w_full()
                    .px(px(24.0))
                    .flex()
                    .flex_col()
                    .gap(px(7.0))
                    .child(
                        div()
                            .mb(px(2.0))
                            .flex()
                            .justify_center()
                            .text_size(px(12.0))
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .text_color(theme.text_muted)
                            .child("New session"),
                    )
                    .child(
                        div()
                            .flex()
                            .justify_center()
                            .text_size(px(17.0))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_color(theme.text)
                            .child(project_name),
                    )
                    .child(
                        div()
                            .mb(px(16.0))
                            .flex()
                            .justify_center()
                            .truncate()
                            .text_size(px(11.0))
                            .text_color(theme.text_faint)
                            .child(project_path),
                    )
                    .child(
                        div()
                            .id("workers-launch-shell")
                            .h(px(46.0))
                            .w_full()
                            .px(px(12.0))
                            .flex()
                            .items_center()
                            .gap(px(9.0))
                            .rounded(px(10.0))
                            .border_1()
                            .border_color(theme.border.opacity(0.75))
                            .text_size(px(12.0))
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .text_color(theme.text_muted)
                            .cursor_pointer()
                            .when(busy, |el| el.opacity(0.45))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.model.update(cx, |model, cx| {
                                    model.launch(
                                        WorkersLaunchRequest::terminal(shell_project_id.clone())
                                            .with_optional_worktree(
                                                shell_worktree_path.clone(),
                                                shell_worktree_branch.clone(),
                                            ),
                                        cx,
                                    )
                                });
                            }))
                            .child(
                                icon(icons::TERMINAL)
                                    .size(px(15.0))
                                    .text_color(theme.text_muted),
                            )
                            .child(if busy { "Starting…" } else { "Terminal" })
                            .child(div().flex_1())
                            .child(
                                icon(icons::ALT_ARROW_RIGHT)
                                    .size(px(12.0))
                                    .text_color(theme.text_faint),
                            ),
                    )
                    .children(preset_rows)
                    .child(
                        div()
                            .id("workers-manage-presets")
                            .mt(px(14.0))
                            .h(px(32.0))
                            .flex()
                            .items_center()
                            .justify_center()
                            .gap(px(7.0))
                            .rounded(px(8.0))
                            .cursor_pointer()
                            .text_size(px(11.0))
                            .text_color(theme.text_muted)
                            .hover(|el| el.bg(crate::theme::ink(0.07)))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.model.update(cx, |model, cx| {
                                    model.open_settings(WorkersSettingsTab::Presets, cx)
                                });
                            }))
                            .child(
                                icon(icons::SETTINGS_MINIMALISTIC)
                                    .size(px(12.0))
                                    .text_color(theme.text_faint),
                            )
                            .child("Manage presets…"),
                    ),
            )
            .into_any_element()
    }

    fn render_session(
        &self,
        session: WorkersSession,
        project: Option<WorkersProject>,
        busy: bool,
        theme: &Theme,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let title = if session.title.trim().is_empty() {
            "Untitled session".to_owned()
        } else {
            session.title.clone()
        };
        let subtitle = project
            .map(|project| project.name)
            .unwrap_or_else(|| "Local worker".to_owned());
        let stop_id = session.id.clone();
        let restart_id = session.id.clone();
        let pin_id = session.id.clone();
        let archive_id = session.id.clone();
        let rename_session = session.clone();
        let remove_session = session.clone();
        let live = session.is_live();

        div()
            .size_full()
            .flex()
            .flex_col()
            .child(
                div()
                    .h(px(44.0))
                    .px(px(14.0))
                    .flex()
                    .items_center()
                    .gap(px(10.0))
                    .border_b_1()
                    .border_color(theme.border.opacity(0.6))
                    .child(div().size(px(7.0)).rounded_full().bg(if live {
                        theme.success
                    } else {
                        theme.text_faint
                    }))
                    .child(
                        div()
                            .min_w_0()
                            .flex_1()
                            .flex()
                            .flex_col()
                            .child(
                                div()
                                    .truncate()
                                    .text_size(px(12.0))
                                    .font_weight(gpui::FontWeight::MEDIUM)
                                    .text_color(theme.text)
                                    .child(title),
                            )
                            .child(
                                div()
                                    .truncate()
                                    .text_size(px(9.5))
                                    .text_color(theme.text_faint)
                                    .child(subtitle),
                            ),
                    )
                    .child(
                        div()
                            .id("workers-rename")
                            .h(px(28.0))
                            .px(px(8.0))
                            .flex()
                            .items_center()
                            .gap(px(5.0))
                            .rounded(px(8.0))
                            .cursor_pointer()
                            .text_size(px(10.0))
                            .text_color(theme.text_muted)
                            .hover(|el| el.bg(crate::theme::ink(0.08)))
                            .when(busy, |el| el.opacity(0.4))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.open_rename(rename_session.clone(), cx);
                            }))
                            .child(icon(icons::PEN).size(px(12.0)).text_color(theme.text_muted))
                            .child("Rename"),
                    )
                    .child(self.action_button(
                        "workers-pin",
                        icons::STAR,
                        if session.pinned { "Unpin" } else { "Pin" },
                        busy,
                        theme,
                        cx,
                        move |model, cx| model.pin(pin_id.clone(), !session.pinned, cx),
                    ))
                    .child(self.action_button(
                        "workers-archive",
                        icons::ARCHIVE_MINIMALISTIC,
                        "Archive",
                        busy,
                        theme,
                        cx,
                        move |model, cx| model.archive(archive_id.clone(), true, cx),
                    ))
                    .when(live, |el| {
                        el.child(self.action_button(
                            "workers-stop",
                            icons::STOP,
                            "Stop",
                            busy,
                            theme,
                            cx,
                            move |model, cx| model.stop(stop_id.clone(), cx),
                        ))
                    })
                    .when(!live, |el| {
                        el.child(
                            div()
                                .flex()
                                .items_center()
                                .child(self.action_button(
                                    "workers-restart",
                                    icons::RESTART,
                                    "Restart",
                                    busy,
                                    theme,
                                    cx,
                                    move |model, cx| model.restart(restart_id.clone(), cx),
                                ))
                                .child(
                                    div()
                                        .id("workers-remove")
                                        .h(px(28.0))
                                        .px(px(8.0))
                                        .flex()
                                        .items_center()
                                        .gap(px(5.0))
                                        .rounded(px(8.0))
                                        .cursor_pointer()
                                        .text_size(px(10.0))
                                        .text_color(theme.danger_muted)
                                        .hover(|el| el.bg(theme.danger.opacity(0.08)))
                                        .when(busy, |el| el.opacity(0.4))
                                        .on_click(cx.listener(move |this, _, _, cx| {
                                            this.request_remove(remove_session.clone(), false, cx);
                                        }))
                                        .child(
                                            icon(icons::TRASH_BIN_MINIMALISTIC)
                                                .size(px(12.0))
                                                .text_color(theme.danger_muted),
                                        )
                                        .child("Remove"),
                                ),
                        )
                    }),
            )
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .bg(crate::terminal::view::terminal_panel_bg(theme))
                    .when(live, |el| el.child(self.terminal.clone()))
                    .when(!live, |el| {
                        el.flex()
                            .items_center()
                            .justify_center()
                            .text_size(px(11.0))
                            .text_color(theme.text_faint)
                            .child("This worker has stopped. Restart it to continue.")
                    }),
            )
            .into_any_element()
    }

    fn render_archive(
        &self,
        project: Option<WorkersProject>,
        sessions: Vec<WorkersSession>,
        loading: bool,
        error: Option<String>,
        busy: bool,
        theme: &Theme,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let project_name = project
            .map(|project| project.name)
            .unwrap_or_else(|| "Project".to_owned());
        let rows = sessions
            .into_iter()
            .enumerate()
            .map(|(index, session)| {
                let restore_session = session.clone();
                let resume_session = session.clone();
                let remove_session = session.clone();
                let title = if session.title.trim().is_empty() {
                    "Untitled session".to_owned()
                } else {
                    session.title.clone()
                };
                let resume_label = if session.capabilities.resume_agent {
                    "Resume"
                } else {
                    "Restart"
                };
                div()
                    .id(("workers-archive-row", index))
                    .min_h(px(52.0))
                    .px(px(12.0))
                    .flex()
                    .items_center()
                    .gap(px(10.0))
                    .rounded(px(10.0))
                    .border_1()
                    .border_color(theme.border.opacity(0.7))
                    .child(
                        icon(icons::ARCHIVE_MINIMALISTIC)
                            .size(px(14.0))
                            .text_color(theme.text_faint),
                    )
                    .child(
                        div()
                            .min_w_0()
                            .flex_1()
                            .flex()
                            .flex_col()
                            .child(
                                div()
                                    .truncate()
                                    .text_size(px(12.0))
                                    .font_weight(gpui::FontWeight::MEDIUM)
                                    .text_color(theme.text)
                                    .child(title),
                            )
                            .child(
                                div()
                                    .truncate()
                                    .text_size(px(9.5))
                                    .text_color(theme.text_faint)
                                    .child(session.command.clone()),
                            ),
                    )
                    .child(
                        div()
                            .id(("workers-archive-restore", index))
                            .h(px(27.0))
                            .px(px(8.0))
                            .flex()
                            .items_center()
                            .rounded(px(8.0))
                            .cursor_pointer()
                            .text_size(px(10.0))
                            .text_color(theme.text_muted)
                            .hover(|el| el.bg(crate::theme::ink(0.08)))
                            .when(busy, |el| el.opacity(0.4))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.model.update(cx, |model, cx| {
                                    model.restore(restore_session.clone(), false, cx)
                                });
                            }))
                            .child("Restore"),
                    )
                    .child(
                        div()
                            .id(("workers-archive-resume", index))
                            .h(px(27.0))
                            .px(px(8.0))
                            .flex()
                            .items_center()
                            .rounded(px(8.0))
                            .cursor_pointer()
                            .text_size(px(10.0))
                            .text_color(theme.text_muted)
                            .hover(|el| el.bg(crate::theme::ink(0.08)))
                            .when(busy, |el| el.opacity(0.4))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.model.update(cx, |model, cx| {
                                    model.restore(resume_session.clone(), true, cx)
                                });
                            }))
                            .child(resume_label),
                    )
                    .child(
                        div()
                            .id(("workers-archive-remove", index))
                            .size(px(27.0))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(8.0))
                            .cursor_pointer()
                            .hover(|el| el.bg(theme.danger.opacity(0.08)))
                            .when(busy, |el| el.opacity(0.4))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.request_remove(remove_session.clone(), true, cx);
                            }))
                            .child(
                                icon(icons::TRASH_BIN_MINIMALISTIC)
                                    .size(px(12.0))
                                    .text_color(theme.danger_muted),
                            ),
                    )
                    .into_any_element()
            })
            .collect::<Vec<_>>();

        div()
            .size_full()
            .flex()
            .flex_col()
            .child(
                div()
                    .h(px(48.0))
                    .px(px(16.0))
                    .flex()
                    .items_center()
                    .gap(px(9.0))
                    .border_b_1()
                    .border_color(theme.border.opacity(0.6))
                    .child(
                        div()
                            .id("workers-archive-back")
                            .size(px(28.0))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(8.0))
                            .cursor_pointer()
                            .hover(|el| el.bg(crate::theme::ink(0.08)))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.model.update(cx, |model, cx| model.close_archive(cx));
                            }))
                            .child(
                                icon(icons::ALT_ARROW_LEFT)
                                    .size(px(13.0))
                                    .text_color(theme.text_muted),
                            ),
                    )
                    .child(
                        div()
                            .flex_1()
                            .flex()
                            .flex_col()
                            .child(
                                div()
                                    .text_size(px(13.0))
                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                    .text_color(theme.text)
                                    .child("Archived workers"),
                            )
                            .child(
                                div()
                                    .text_size(px(9.5))
                                    .text_color(theme.text_faint)
                                    .child(project_name),
                            ),
                    ),
            )
            .child(
                div()
                    .id("workers-archive-list")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .p(px(16.0))
                    .flex()
                    .flex_col()
                    .gap(px(8.0))
                    .when_some(error, |el, error| {
                        el.child(
                            div()
                                .mb(px(4.0))
                                .p(px(10.0))
                                .rounded(px(9.0))
                                .bg(theme.danger.opacity(0.08))
                                .text_size(px(11.0))
                                .text_color(theme.danger_muted)
                                .child(format!("Could not load archive. {error}")),
                        )
                    })
                    .when(loading, |el| {
                        el.child(
                            div()
                                .text_size(px(11.0))
                                .text_color(theme.text_faint)
                                .child("Loading archived workers…"),
                        )
                    })
                    .when(!loading && rows.is_empty(), |el| {
                        el.child(
                            div()
                                .py(px(28.0))
                                .flex()
                                .justify_center()
                                .text_size(px(11.0))
                                .text_color(theme.text_faint)
                                .child("No archived workers in this project."),
                        )
                    })
                    .children(rows),
            )
            .into_any_element()
    }

    fn action_button(
        &self,
        id: &'static str,
        icon_path: &'static str,
        label: &'static str,
        busy: bool,
        theme: &Theme,
        cx: &mut Context<Self>,
        action: impl Fn(&mut WorkersModel, &mut Context<WorkersModel>) + 'static,
    ) -> AnyElement {
        div()
            .id(id)
            .h(px(28.0))
            .px(px(8.0))
            .flex()
            .items_center()
            .gap(px(5.0))
            .rounded(px(8.0))
            .cursor_pointer()
            .text_size(px(10.0))
            .text_color(theme.text_muted)
            .hover(|el| el.bg(crate::theme::ink(0.08)))
            .when(busy, |el| el.opacity(0.4))
            .on_click(cx.listener(move |this, _, _, cx| {
                this.model.update(cx, |model, cx| action(model, cx));
            }))
            .child(icon(icon_path).size(px(12.0)).text_color(theme.text_muted))
            .child(label)
            .into_any_element()
    }
}

impl Render for WorkersContent {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = Theme::of(cx).clone();
        let (
            loading,
            error,
            has_snapshot,
            busy,
            selected_session,
            selected_project,
            presets,
            archive_project_id,
            archived_sessions,
            archive_loading,
            archive_error,
            route,
        ) = {
            let model = self.model.read(cx);
            (
                model.loading,
                model.error.clone(),
                model.snapshot.is_some(),
                model.action_in_flight(),
                model.selected_session().cloned(),
                model.selected_project().cloned(),
                model.presets().to_vec(),
                model.archive_project_id.clone(),
                model.archived_sessions.clone(),
                model.archive_loading,
                model.archive_error.clone(),
                model.route,
            )
        };

        let content = if matches!(route, WorkersRoute::Settings(_)) {
            self.settings.clone().into_any_element()
        } else if archive_project_id.is_some() {
            self.render_archive(
                selected_project.clone(),
                archived_sessions,
                archive_loading,
                archive_error,
                busy,
                &theme,
                cx,
            )
        } else if loading && !has_snapshot {
            div()
                .size_full()
                .flex()
                .items_center()
                .justify_center()
                .text_size(px(12.0))
                .text_color(theme.text_faint)
                .child("Loading local worker runtime…")
                .into_any_element()
        } else if !has_snapshot && error.is_some() {
            div()
                .size_full()
                .flex()
                .items_center()
                .justify_center()
                .px(px(32.0))
                .text_size(px(12.0))
                .text_color(theme.danger_muted)
                .child(format!(
                    "Workers are unavailable. {}",
                    error.clone().unwrap_or_default()
                ))
                .into_any_element()
        } else if let Some(session) = selected_session {
            self.render_session(session, selected_project, busy, &theme, cx)
        } else if let Some(project) = selected_project {
            self.render_launcher(project, presets, busy, &theme, cx)
        } else {
            div()
                .size_full()
                .flex()
                .items_center()
                .justify_center()
                .child(
                    div()
                        .w(px(420.0))
                        .max_w_full()
                        .px(px(24.0))
                        .flex()
                        .flex_col()
                        .items_center()
                        .gap(px(10.0))
                        .child(
                            icon(icons::FOLDER)
                                .size(px(56.0))
                                .text_color(theme.text_muted),
                        )
                        .child(
                            div()
                                .mt(px(6.0))
                                .text_size(px(14.0))
                                .font_weight(gpui::FontWeight::MEDIUM)
                                .text_color(theme.text_muted)
                                .child("No session selected"),
                        )
                        .child(
                            div()
                                .text_size(px(11.0))
                                .text_color(theme.text_faint)
                                .child("Pick a session in the sidebar, or hit + on a project"),
                        )
                        .child(
                            div()
                                .mt(px(8.0))
                                .text_size(px(10.0))
                                .text_color(theme.text_faint.opacity(0.72))
                                .child(format!("v{}", env!("CARGO_PKG_VERSION"))),
                        ),
                )
                .into_any_element()
        };

        let rename_dialog = self.render_rename_dialog(window.viewport_size(), cx);
        let remove_dialog = self.render_remove_dialog(window.viewport_size(), cx);

        div()
            .size_full()
            .pt(px(Theme::TITLEBAR_HEIGHT))
            .flex()
            .flex_col()
            .when(
                has_snapshot && error.is_some() && archive_project_id.is_none(),
                |el| {
                    el.child(
                        div()
                            .mx(px(12.0))
                            .mt(px(8.0))
                            .p(px(8.0))
                            .rounded(px(8.0))
                            .bg(theme.danger.opacity(0.08))
                            .text_size(px(10.0))
                            .text_color(theme.danger_muted)
                            .child(format!(
                                "Workers refresh failed; showing the last good state. {}",
                                error.clone().unwrap_or_default()
                            )),
                    )
                },
            )
            .child(content)
            .when_some(rename_dialog, |el, dialog| el.child(dialog))
            .when_some(remove_dialog, |el, dialog| el.child(dialog))
            .into_any_element()
    }
}
