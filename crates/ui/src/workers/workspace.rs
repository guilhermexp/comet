use std::time::{SystemTime, UNIX_EPOCH};

use gpui::{
    AnyElement, ClipboardItem, Context, Entity, IntoElement, MouseButton, MouseDownEvent,
    PathPromptOptions, Pixels, Point, Render, SharedString, Subscription, Task, Window, div,
    prelude::*, px,
};
use zeron_workers_unpeel::{
    WorkersLaunchRequest, WorkersPreset, WorkersProject, WorkersSession, WorkersSessionSort,
};

use crate::composer::{ComposerInput, ComposerInputEvent};
use crate::icons::{self, icon};
use crate::popover;
use crate::theme::Theme;

use super::archive::archive_restore_presentation;
use super::model::{WorkersModel, WorkersRoute, WorkersSessionTarget, WorkersSettingsTab};
use super::new_session_menu::WorkersNewSessionMenuItem as NewSessionMenuItem;
#[cfg(target_os = "macos")]
use super::new_session_menu::native as native_new_session_menu;
use super::presentation::{
    HOSTED_SIDEBAR_TOP_PADDING, PROJECT_ROW_BASE_LEADING, SESSION_ROW_BASE_LEADING,
    SIDEBAR_BOTTOM_PADDING, SIDEBAR_LABEL_SIZE, SIDEBAR_LIST_SPACING, SIDEBAR_NESTING_STEP,
    SIDEBAR_ROW_GAP, SIDEBAR_ROW_HEIGHT, SIDEBAR_ROW_RADIUS, SIDEBAR_SIDE_PADDING,
    SessionIndicator, relative_age, runtime_icon_path, runtime_spinner_tint, session_indicator,
    spinner_frame,
};
use super::project_menu::{WorkersProjectMenuItem as ProjectMenuItem, project_menu_items};
use super::recent::recent_activity_sections;
#[cfg(target_os = "macos")]
use super::session_menu::native::{self as native_session_menu, Selection as NativeMenuSelection};
use super::session_menu::{WorkersSessionMenuItem as SessionMenuItem, session_menu_items};
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

fn worktree_branch_slug(task_name: &str) -> String {
    let slug = task_name
        .trim()
        .to_lowercase()
        .chars()
        .fold((String::new(), false), |(mut output, separator), ch| {
            if ch.is_ascii_alphanumeric() {
                if separator && !output.is_empty() {
                    output.push('-');
                }
                output.push(ch);
                (output, false)
            } else {
                (output, true)
            }
        })
        .0;
    if slug.is_empty() {
        "session".to_owned()
    } else {
        slug
    }
}

fn project_folder_tint(color_id: Option<&str>, dark: bool) -> Option<gpui::Hsla> {
    let (light, dark_color) = match color_id? {
        "sky" => (0x2095C9, 0x7DD3FC),
        "blue" => (0x4F73E6, 0x7EA6FF),
        "violet" => (0x7B5BDA, 0xB79CFF),
        "rose" => (0xD75F8F, 0xF79AC0),
        "amber" => (0xB87511, 0xF8C86A),
        "moss" => (0x5F9A3D, 0x9DD67A),
        "teal" => (0x159B91, 0x64DCCB),
        "graphite" => (0x687083, 0xB8BCC8),
        _ => return None,
    };
    Some(gpui::Hsla::from(gpui::rgb(if dark {
        dark_color
    } else {
        light
    })))
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
    content: Entity<WorkersContent>,
    picker_task: Option<Task<()>>,
    _spinner_task: Task<()>,
    _model_observation: Subscription,
}

impl WorkersSidebar {
    pub fn new(
        model: Entity<WorkersModel>,
        content: Entity<WorkersContent>,
        cx: &mut Context<Self>,
    ) -> Self {
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
            content,
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

    fn open_new_session_menu(
        &mut self,
        project: WorkersProject,
        presets: Vec<WorkersPreset>,
        cx: &mut Context<Self>,
    ) {
        #[cfg(target_os = "macos")]
        {
            let allow_worktree = project.parent_project_id.is_none() && !project.is_group;
            let selection = native_new_session_menu::show_async(&presets, allow_worktree);
            cx.spawn(async move |this, cx| {
                let Ok(Some(selection)) = selection.await else {
                    return;
                };
                this.update(cx, |this, cx| match selection {
                    NewSessionMenuItem::Terminal => {
                        this.model.update(cx, |model, cx| {
                            model.launch(
                                WorkersLaunchRequest::terminal(project.id.clone())
                                    .with_optional_worktree(
                                        project
                                            .worktree_branch
                                            .as_ref()
                                            .map(|_| project.path.clone()),
                                        project.worktree_branch.clone(),
                                    ),
                                cx,
                            )
                        });
                    }
                    NewSessionMenuItem::Preset(preset_id) => {
                        this.model.update(cx, |model, cx| {
                            model.launch(
                                WorkersLaunchRequest::preset(project.id.clone(), preset_id)
                                    .with_optional_worktree(
                                        project
                                            .worktree_branch
                                            .as_ref()
                                            .map(|_| project.path.clone()),
                                        project.worktree_branch.clone(),
                                    ),
                                cx,
                            )
                        });
                    }
                    NewSessionMenuItem::WorktreeTerminal => {
                        this.content.update(cx, |content, cx| {
                            content.open_project_dialog(
                                project.clone(),
                                ProjectDialogKind::NewWorktreeSession(None),
                                cx,
                            )
                        });
                    }
                    NewSessionMenuItem::WorktreePreset(preset_id) => {
                        this.content.update(cx, |content, cx| {
                            content.open_project_dialog(
                                project.clone(),
                                ProjectDialogKind::NewWorktreeSession(Some(preset_id)),
                                cx,
                            )
                        });
                    }
                    NewSessionMenuItem::ManagePresets => {
                        this.model.update(cx, |model, cx| {
                            model.open_settings(WorkersSettingsTab::Presets, cx)
                        });
                    }
                })
                .ok();
            })
            .detach();
            return;
        }

        #[cfg(not(target_os = "macos"))]
        self.model
            .update(cx, |model, cx| model.open_launcher(project.id, cx));
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
        if self
            .model
            .read(cx)
            .confirming_remove_project
            .as_ref()
            .is_some_and(|candidate| candidate.id == project.id)
        {
            let label = if project.worktree_branch.is_some() {
                "Remove worktree?"
            } else if project.parent_project_id.is_some() {
                "Remove group?"
            } else if sessions.is_empty() {
                "Remove project?"
            } else {
                "Remove project and sessions?"
            };
            return div()
                .id(("workers-project-remove-confirm", index))
                .min_h(px(30.0))
                .px(px(9.0))
                .flex()
                .items_center()
                .gap(px(7.0))
                .rounded(px(9.0))
                .bg(crate::theme::ink(0.10))
                .text_size(px(SIDEBAR_LABEL_SIZE))
                .font_weight(gpui::FontWeight::MEDIUM)
                .text_color(theme.text)
                .child(label)
                .child(div().flex_1().min_w(px(4.0)))
                .child(
                    div()
                        .id(("workers-project-remove-cancel", index))
                        .h(px(22.0))
                        .px(px(8.0))
                        .flex()
                        .items_center()
                        .rounded(px(6.0))
                        .cursor_pointer()
                        .text_size(px(11.0))
                        .text_color(theme.text_muted)
                        .bg(crate::theme::ink(0.06))
                        .hover(|el| el.bg(crate::theme::ink(0.10)))
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.model
                                .update(cx, |model, cx| model.cancel_remove_project(cx));
                        }))
                        .child("Cancel"),
                )
                .child(
                    div()
                        .id(("workers-project-remove-confirm-button", index))
                        .h(px(22.0))
                        .px(px(8.0))
                        .flex()
                        .items_center()
                        .rounded(px(6.0))
                        .cursor_pointer()
                        .text_size(px(11.0))
                        .text_color(theme.danger)
                        .bg(theme.danger.opacity(0.15))
                        .hover(|el| el.bg(theme.danger.opacity(0.25)))
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.model
                                .update(cx, |model, cx| model.confirm_remove_project(cx));
                        }))
                        .child("Remove"),
                )
                .into_any_element();
        }
        let new_session_project = project.clone();
        let new_session_presets = presets.clone();
        let select_project_id = project.id.clone();
        let toggle_project_id = project.id.clone();
        let project_name: SharedString = project.name.clone().into();
        let is_group = project.is_group;
        let is_child_folder = project.parent_project_id.is_some();
        let folder_tint = project_folder_tint(
            project.folder_color_id.as_deref(),
            theme.appearance.is_dark(),
        )
        .unwrap_or(theme.text_muted);

        let menu_sessions = sessions.clone();
        let menu_project = project.clone();
        let menu_content = self.content.clone();
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
        let mut quick_cli_ids = std::collections::HashSet::new();
        let quick_launch = presets
            .iter()
            .filter(|preset| preset.enabled && preset.quick_launch)
            .filter(|preset| {
                quick_cli_ids.insert(
                    preset
                        .cli_id
                        .clone()
                        .unwrap_or_else(|| preset.command.clone()),
                )
            })
            .cloned()
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .enumerate()
            .map(|(quick_index, preset)| {
                let quick_menu_sessions = menu_sessions.clone();
                let quick_menu_project = menu_project.clone();
                let quick_menu_content = menu_content.clone();
                let icon_path =
                    runtime_icon_path(preset.cli_id.as_deref(), Some(preset.command.as_str()));
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
                    .rounded(px(8.0))
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
                    .on_mouse_down(
                        MouseButton::Right,
                        cx.listener(move |_, event: &MouseDownEvent, _, cx| {
                            quick_menu_content.update(cx, |content, cx| {
                                content.open_project_menu(
                                    quick_menu_project.clone(),
                                    quick_menu_sessions.clone(),
                                    event.position,
                                    cx,
                                )
                            });
                        }),
                    )
                    .child(icon(icon_path).size(px(14.0)).text_color(theme.text_muted))
                    .into_any_element()
            })
            .collect::<Vec<_>>();

        let project_group: SharedString = format!("workers-project-{index}").into();
        div()
            .w_full()
            .flex()
            .flex_col()
            .child(
                div()
                    .id(("workers-project-row", index))
                    .group(project_group.clone())
                    .relative()
                    .min_h(px(SIDEBAR_ROW_HEIGHT))
                    .pt(px(2.0))
                    .pb(px(2.0))
                    .pl(px(if is_child_folder {
                        10.0 + depth.saturating_sub(1) as f32 * SIDEBAR_NESTING_STEP
                    } else {
                        PROJECT_ROW_BASE_LEADING + depth as f32 * SIDEBAR_NESTING_STEP
                    }))
                    .pr(px(7.0))
                    .flex()
                    .items_center()
                    .gap(px(SIDEBAR_ROW_GAP))
                    .rounded(px(SIDEBAR_ROW_RADIUS))
                    .cursor_pointer()
                    .hover(|el| el.bg(crate::theme::ink(0.10)))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.model.update(cx, |model, cx| {
                            if !is_group {
                                model.select_project(select_project_id.clone(), cx);
                            }
                            model.toggle_project(&toggle_project_id, cx);
                        });
                    }))
                    .child(if is_child_folder {
                        icon(if expanded {
                            icons::ALT_ARROW_DOWN
                        } else {
                            icons::ALT_ARROW_RIGHT
                        })
                        .size(px(11.0))
                        .text_color(theme.text_muted)
                    } else {
                        icon(if expanded {
                            icons::WORKER_FOLDER_OPEN
                        } else {
                            icons::WORKER_FOLDER_CLOSED
                        })
                        .size(px(16.0))
                        .text_color(folder_tint)
                    })
                    .child(
                        div()
                            .min_w_0()
                            .flex_1()
                            .truncate()
                            .text_size(px(SIDEBAR_LABEL_SIZE))
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .text_color(if is_child_folder {
                                theme.text
                            } else {
                                theme.text.opacity(0.60)
                            })
                            .child(project_name),
                    )
                    .when_some(project.worktree_branch.clone(), |el, branch| {
                        el.child(
                            div()
                                .max_w(px(110.0))
                                .flex()
                                .items_center()
                                .gap(px(3.0))
                                .opacity(0.55)
                                .child(
                                    icon(icons::WORKER_BRANCH)
                                        .size(px(12.0))
                                        .text_color(theme.text_muted),
                                )
                                .when(branch != project.name, |el| {
                                    el.child(
                                        div()
                                            .truncate()
                                            .text_size(px(10.0))
                                            .text_color(theme.text_muted)
                                            .child(branch),
                                    )
                                }),
                        )
                    })
                    .when(!is_group, |el| {
                        el.child(
                            div()
                                .absolute()
                                .right(px(4.0))
                                .top(px(2.0))
                                .h(px(24.0))
                                .flex()
                                .items_center()
                                .gap(px(1.0))
                                .invisible()
                                .group_hover(project_group.clone(), |style| style.visible())
                                .child(
                                    div()
                                        .id(("workers-project-terminal", index))
                                        .size(px(22.0))
                                        .flex()
                                        .items_center()
                                        .justify_center()
                                        .rounded(px(8.0))
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
                                                .size(px(14.0))
                                                .text_color(theme.text_muted),
                                        ),
                                )
                                .children(quick_launch)
                                .child(
                                    div()
                                        .id(("workers-project-add", index))
                                        .size(px(22.0))
                                        .flex()
                                        .items_center()
                                        .justify_center()
                                        .rounded(px(8.0))
                                        .hover(|el| el.bg(crate::theme::ink(0.08)))
                                        .on_click(cx.listener(move |this, _, _, cx| {
                                            cx.stop_propagation();
                                            this.open_new_session_menu(
                                                new_session_project.clone(),
                                                new_session_presets.clone(),
                                                cx,
                                            );
                                        }))
                                        .child(
                                            icon(icons::WORKER_PLUS)
                                                .size(px(16.0))
                                                .text_color(theme.text_muted),
                                        ),
                                ),
                        )
                    }),
            )
            .when(expanded, |el| {
                if rows.is_empty() && !is_group {
                    el.child(
                        div()
                            .pt(px(2.0))
                            .pl(px(28.0 + depth as f32 * SIDEBAR_NESTING_STEP))
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
        if self.model.read(cx).confirming_remove_session_id.as_deref() == Some(session.id.as_str())
        {
            let live = session.is_live();
            return div()
                .id(("workers-session-remove-confirm", index))
                .min_h(px(28.0))
                .pt(px(2.0))
                .pb(px(2.0))
                .pl(px(
                    SESSION_ROW_BASE_LEADING + depth as f32 * SIDEBAR_NESTING_STEP
                ))
                .pr(px(5.0))
                .flex()
                .items_center()
                .gap(px(7.0))
                .rounded(px(9.0))
                .bg(crate::theme::ink(0.10))
                .text_size(px(SIDEBAR_LABEL_SIZE))
                .font_weight(gpui::FontWeight::MEDIUM)
                .text_color(theme.text)
                .child(if live {
                    "Remove session?"
                } else {
                    "Remove from list?"
                })
                .child(div().flex_1().min_w(px(4.0)))
                .child(
                    div()
                        .id(("workers-session-remove-cancel", index))
                        .h(px(20.0))
                        .px(px(8.0))
                        .flex()
                        .items_center()
                        .rounded(px(6.0))
                        .cursor_pointer()
                        .text_size(px(11.0))
                        .font_weight(gpui::FontWeight::MEDIUM)
                        .text_color(theme.text_muted)
                        .bg(crate::theme::ink(0.06))
                        .hover(|el| el.bg(crate::theme::ink(0.10)).text_color(theme.text))
                        .on_click(cx.listener(|this, _, _, cx| {
                            cx.stop_propagation();
                            this.model.update(cx, |model, cx| model.cancel_remove(cx));
                        }))
                        .child("Cancel"),
                )
                .child(
                    div()
                        .id(("workers-session-remove-confirm-button", index))
                        .h(px(20.0))
                        .px(px(8.0))
                        .flex()
                        .items_center()
                        .rounded(px(6.0))
                        .cursor_pointer()
                        .text_size(px(11.0))
                        .font_weight(gpui::FontWeight::MEDIUM)
                        .text_color(theme.danger)
                        .bg(theme.danger.opacity(0.15))
                        .hover(|el| el.bg(theme.danger.opacity(0.25)))
                        .on_click(cx.listener(|this, _, _, cx| {
                            cx.stop_propagation();
                            this.model.update(cx, |model, cx| model.confirm_remove(cx));
                        }))
                        .child("Remove"),
                )
                .into_any_element();
        }
        let selected = selected_session_id == Some(session.id.as_str());
        let menu_session = session.clone();
        let menu_content = self.content.clone();
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
        let runtime_id = session
            .active_runtime_id
            .as_deref()
            .or(session.provider_id.as_deref());
        let indicator_color = match indicator {
            SessionIndicator::Busy => runtime_spinner_tint(runtime_id, Some(&session.command))
                .map(|hex| gpui::Hsla::from(gpui::rgb(hex)))
                .unwrap_or_else(|| {
                    gpui::Hsla::from(gpui::rgb(if session.command.trim().is_empty() {
                        if theme.appearance.is_dark() {
                            0xD6D9E1
                        } else {
                            0x4A4F5A
                        }
                    } else if theme.appearance.is_dark() {
                        0xB9BDC9
                    } else {
                        0x4A4D55
                    }))
                }),
            SessionIndicator::Restarting => theme.text_muted,
            SessionIndicator::Attention => theme.warning,
            SessionIndicator::Unread => theme.accent,
            SessionIndicator::Idle => gpui::transparent_black(),
            SessionIndicator::Exited => theme.text_faint.opacity(0.45),
        };
        let now_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;
        let runtime_icon = runtime_icon_path(runtime_id, Some(session.command.as_str()));
        let age = relative_age(session.updated_at_unix_ms, now_ms);
        let session_target = WorkersSessionTarget::new(&session.project_id, &session.id);
        let menu_session_target = session_target.clone();

        div()
            .id(("workers-session-row", index))
            .min_h(px(SIDEBAR_ROW_HEIGHT))
            .pt(px(2.0))
            .pb(px(2.0))
            .pl(px(
                SESSION_ROW_BASE_LEADING + depth as f32 * SIDEBAR_NESTING_STEP
            ))
            .pr(px(9.0))
            .flex()
            .items_center()
            .gap(px(SIDEBAR_ROW_GAP))
            .rounded(px(SIDEBAR_ROW_RADIUS))
            .cursor_pointer()
            .when(selected, |el| el.bg(crate::theme::ink(0.16)))
            .hover(|el| el.bg(crate::theme::ink(0.10)))
            .on_click(cx.listener(move |this, _, _, cx| {
                this.model.update(cx, |model, cx| {
                    model.select_session_target(session_target.clone(), cx)
                });
            }))
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(move |this, event: &MouseDownEvent, _, cx| {
                    this.model.update(cx, |model, cx| {
                        model.select_session_target(menu_session_target.clone(), cx)
                    });
                    menu_content.update(cx, |content, cx| {
                        content.open_session_menu(menu_session.clone(), event.position, cx)
                    });
                }),
            )
            .child(match indicator {
                SessionIndicator::Busy | SessionIndicator::Restarting => div()
                    .w(px(16.0))
                    .h(px(16.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .font_family("SF Mono")
                    .font_weight(gpui::FontWeight::BOLD)
                    .text_size(px(14.7))
                    .line_height(px(16.0))
                    .text_color(indicator_color)
                    .child(spinner_frame(now_ms)),
                SessionIndicator::Attention => div()
                    .w(px(16.0))
                    .h(px(16.0))
                    .relative()
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(
                        div()
                            .absolute()
                            .size(px(14.0))
                            .rounded_full()
                            .bg(indicator_color.opacity(0.20)),
                    )
                    .child(div().size(px(6.0)).rounded_full().bg(indicator_color)),
                SessionIndicator::Unread => div()
                    .w(px(16.0))
                    .h(px(16.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(div().size(px(7.0)).rounded_full().bg(indicator_color)),
                SessionIndicator::Idle | SessionIndicator::Exited => {
                    div().w(px(16.0)).flex().justify_center()
                }
            })
            .child(
                div()
                    .min_w_0()
                    .flex_1()
                    .truncate()
                    .text_size(px(SIDEBAR_LABEL_SIZE))
                    .text_color(if selected {
                        theme.text
                    } else if matches!(indicator, SessionIndicator::Exited) {
                        theme.text_muted.opacity(0.82)
                    } else {
                        theme.text_muted
                    })
                    .child(title),
            )
            .child(
                div()
                    .w(px(24.0))
                    .flex()
                    .justify_end()
                    .text_size(px(9.0))
                    .text_color(theme.text_muted.opacity(0.70))
                    .child(age),
            )
            .when(session.pinned, |el| {
                el.child(
                    icon(icons::WORKER_PIN)
                        .size(px(13.0))
                        .text_color(theme.text.opacity(0.88)),
                )
            })
            .child(
                icon(runtime_icon)
                    .size(px(12.0))
                    .ml(px(3.0))
                    .text_color(theme.text_muted),
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
        let (loading, error, selected_session_id, expanded, projects, all_sessions, presets) = {
            let model = self.model.read(cx);
            (
                model.loading,
                model.error.clone(),
                model.selected_session_id.clone(),
                model.expanded_project_ids.clone(),
                model.projects().to_vec(),
                model.sessions().to_vec(),
                model.presets().to_vec(),
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
                    .id("workers-project-list")
                    .flex_1()
                    .min_h_0()
                    .px(px(SIDEBAR_SIDE_PADDING))
                    .pt(px(HOSTED_SIDEBAR_TOP_PADDING))
                    .pb(px(SIDEBAR_BOTTOM_PADDING))
                    .flex()
                    .flex_col()
                    .gap(px(SIDEBAR_LIST_SPACING))
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
                                .flex()
                                .flex_col()
                                .items_center()
                                .justify_center()
                                .gap(px(14.0))
                                .child(
                                    icon(icons::WORKER_FOLDER_CLOSED)
                                        .size(px(40.0))
                                        .text_color(theme.text),
                                )
                                .child(
                                    div()
                                        .id("workers-add-project-empty")
                                        .h(px(32.0))
                                        .px(px(15.0))
                                        .flex()
                                        .items_center()
                                        .gap(px(8.0))
                                        .rounded(px(8.0))
                                        .bg(theme.text)
                                        .text_size(px(13.0))
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
                    .pt(px(0.0))
                    .pl(px(7.5))
                    .pr(px(7.5))
                    .pb(px(7.5))
                    .flex()
                    .items_center()
                    .gap(px(2.0))
                    .text_size(px(10.0))
                    .text_color(theme.text_faint)
                    .child(
                        div()
                            .id("workers-open-settings")
                            .size(px(22.0))
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
                                icon(icons::WORKER_SETTINGS)
                                    .size(px(14.0))
                                    .text_color(theme.text_muted),
                            ),
                    )
                    .child(
                        div()
                            .id("workers-add-project-footer")
                            .size(px(22.0))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(7.0))
                            .cursor_pointer()
                            .hover(|el| el.bg(crate::theme::ink(0.08)))
                            .on_click(cx.listener(|this, _, _, cx| this.open_project_picker(cx)))
                            .child(
                                icon(icons::WORKER_ADD_PROJECT_PLUS)
                                    .size(px(14.0))
                                    .text_color(theme.text_muted),
                            ),
                    )
                    .child(div().flex_1())
                    .child(
                        div()
                            .id("workers-collapse-all")
                            .size(px(22.0))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(7.0))
                            .cursor_pointer()
                            .opacity(if expanded.is_empty() { 0.4 } else { 1.0 })
                            .hover(|el| el.bg(crate::theme::ink(0.10)))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.model
                                    .update(cx, |model, cx| model.collapse_all_projects(cx));
                            }))
                            .child(
                                icon(icons::WORKER_COLLAPSE_ALL)
                                    .size(px(14.0))
                                    .text_color(theme.text_muted),
                            ),
                    ),
            )
            .into_any_element()
    }
}

struct RenameDialog {
    session_id: String,
    input: Entity<ComposerInput>,
    _events: Subscription,
}

struct AppendContextDialog {
    session_id: String,
    input: Entity<ComposerInput>,
    _events: Subscription,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum ProjectDialogKind {
    Rename,
    NewGroup,
    NewWorktree,
    NewWorktreeSession(Option<String>),
}

struct ProjectDialog {
    project_id: String,
    kind: ProjectDialogKind,
    input: Entity<ComposerInput>,
    _events: Subscription,
}

pub struct WorkersContent {
    model: Entity<WorkersModel>,
    terminal: Entity<WorkersTerminal>,
    settings: Entity<WorkersSettingsView>,
    rename: Option<RenameDialog>,
    append_context: Option<AppendContextDialog>,
    session_menu: popover::Popup<(WorkersSession, Point<Pixels>)>,
    project_menu: popover::Popup<(WorkersProject, Vec<WorkersSession>, Point<Pixels>)>,
    project_dialog: Option<ProjectDialog>,
    transcript_task: Option<Task<()>>,
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
            append_context: None,
            session_menu: popover::Popup::default(),
            project_menu: popover::Popup::default(),
            project_dialog: None,
            transcript_task: None,
            _model_observation: model_observation,
        }
    }

    pub fn open_session_menu(
        &mut self,
        session: WorkersSession,
        position: Point<Pixels>,
        cx: &mut Context<Self>,
    ) {
        #[cfg(target_os = "macos")]
        {
            let move_targets = self
                .model
                .read(cx)
                .projects()
                .iter()
                .filter(|project| project.id != session.project_id)
                .cloned()
                .collect::<Vec<_>>();
            let items = session_menu_items(&session, &move_targets);
            let selection = native_session_menu::show_async(&session, &move_targets, &items);
            cx.spawn(async move |this, cx| {
                let Ok(Some(selection)) = selection.await else {
                    return;
                };
                this.update(cx, |this, cx| match selection {
                    NativeMenuSelection::Item(item) => {
                        this.perform_session_menu_action(session, item, cx)
                    }
                    NativeMenuSelection::MoveTo(project_id) => {
                        this.model.update(cx, |model, cx| {
                            model.move_session(session.id, Some(project_id), cx)
                        });
                    }
                })
                .ok();
            })
            .detach();
            let _ = position;
            return;
        }

        #[cfg(not(target_os = "macos"))]
        {
            self.session_menu.open((session, position));
            cx.notify();
        }
    }

    pub fn open_project_menu(
        &mut self,
        project: WorkersProject,
        sessions: Vec<WorkersSession>,
        position: Point<Pixels>,
        cx: &mut Context<Self>,
    ) {
        self.project_menu.open((project, sessions, position));
        cx.notify();
    }

    fn close_session_menu(&mut self, cx: &mut Context<Self>) {
        if self.session_menu.begin_close() {
            popover::reap_popup(cx, |this| &mut this.session_menu);
        }
        cx.notify();
    }

    fn close_project_menu(&mut self, cx: &mut Context<Self>) {
        if self.project_menu.begin_close() {
            popover::reap_popup(cx, |this| &mut this.project_menu);
        }
        cx.notify();
    }

    fn copy_transcript(
        &mut self,
        session_id: String,
        entries: Option<usize>,
        cx: &mut Context<Self>,
    ) {
        let client = zeron_workers_unpeel::LocalWorkersClient::new();
        self.transcript_task = Some(cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { client.transcript_markdown(&session_id, entries) })
                .await;
            this.update(cx, |content, cx| {
                content.transcript_task = None;
                match result {
                    Ok(markdown) => cx.write_to_clipboard(ClipboardItem::new_string(markdown)),
                    Err(error) => content.model.update(cx, |model, cx| {
                        model.error = Some(format!("Could not copy worker transcript. {error}"));
                        cx.notify();
                    }),
                }
                cx.notify();
            })
            .ok();
        }));
    }

    fn perform_session_menu_action(
        &mut self,
        session: WorkersSession,
        item: SessionMenuItem,
        cx: &mut Context<Self>,
    ) {
        match item {
            SessionMenuItem::Rename => self.open_rename(session, cx),
            SessionMenuItem::Pin | SessionMenuItem::Unpin => {
                let pinned = item == SessionMenuItem::Pin;
                self.model
                    .update(cx, |model, cx| model.pin(session.id, pinned, cx));
            }
            SessionMenuItem::MoveTo | SessionMenuItem::CopyTranscript => {}
            SessionMenuItem::ClearAttention => {
                self.model
                    .update(cx, |model, cx| model.clear_attention(session.id, cx));
            }
            SessionMenuItem::ResumeAgent => {
                self.model
                    .update(cx, |model, cx| model.resume_agent(session.id, cx));
            }
            SessionMenuItem::Resume => {
                self.model
                    .update(cx, |model, cx| model.restart(session.id, cx));
            }
            SessionMenuItem::Fork => {
                self.model.update(cx, |model, cx| model.fork(session, cx));
            }
            SessionMenuItem::AppendSystemContext => self.open_append_context(session, cx),
            SessionMenuItem::NotifyWhenDone => {
                self.model.update(cx, |model, cx| {
                    model.set_notify_when_done(session.id, !session.notify_when_done, cx)
                });
            }
            SessionMenuItem::CopySessionId => cx.write_to_clipboard(ClipboardItem::new_string(
                format!("Zeron Session ID: {}", session.id),
            )),
            SessionMenuItem::CopyTranscript20 => self.copy_transcript(session.id, Some(20), cx),
            SessionMenuItem::CopyTranscript50 => self.copy_transcript(session.id, Some(50), cx),
            SessionMenuItem::CopyTranscriptAll => self.copy_transcript(session.id, Some(0), cx),
            SessionMenuItem::StopAndArchive | SessionMenuItem::Archive => {
                let is_live = session.is_live();
                self.model.update(cx, |model, cx| {
                    model.stop_and_archive(session.id, is_live, cx)
                });
            }
            SessionMenuItem::RestoreAndResume => {
                self.model
                    .update(cx, |model, cx| model.restore(session, true, cx));
            }
            SessionMenuItem::Restore => {
                self.model
                    .update(cx, |model, cx| model.restore(session, false, cx));
            }
            SessionMenuItem::Remove => self
                .model
                .update(cx, |model, cx| model.request_remove(session.id, false, cx)),
        }
    }

    fn render_project_menu(&mut self, theme: &Theme, cx: &mut Context<Self>) -> Option<AnyElement> {
        let (project, sessions, position) = self.project_menu.get()?.clone();
        let closing = self.project_menu.closing_since();
        let presets = self.model.read(cx).presets().to_vec();
        let mut rows = Vec::new();
        for (index, item) in project_menu_items(&project, &sessions)
            .into_iter()
            .enumerate()
        {
            if matches!(
                item,
                ProjectMenuItem::RevealInFinder | ProjectMenuItem::RemoveProject
            ) {
                rows.push(popover::menu_separator().into_any_element());
            }
            let label = match item {
                ProjectMenuItem::Rename => "Rename",
                ProjectMenuItem::NewSession => "New session",
                ProjectMenuItem::FolderColor => "Folder color",
                ProjectMenuItem::SortCustom => "Sort sessions · Custom order",
                ProjectMenuItem::SortRecentlyUpdated => "Sort sessions · Recently updated",
                ProjectMenuItem::NewWorktree => "New worktree…",
                ProjectMenuItem::NewGroup => "New group…",
                ProjectMenuItem::StopAll => "Stop all",
                ProjectMenuItem::Archived => "Archived",
                ProjectMenuItem::RevealInFinder => "Reveal in Finder",
                ProjectMenuItem::OpenInEditor => "Open in editor",
                ProjectMenuItem::RemoveWorktree => "Remove worktree",
                ProjectMenuItem::RemoveGroup => "Remove group",
                ProjectMenuItem::RemoveProject => "Remove project",
            };
            let icon_path = match item {
                ProjectMenuItem::Rename => icons::PEN,
                ProjectMenuItem::NewSession => icons::TERMINAL,
                ProjectMenuItem::FolderColor => icons::FOLDER,
                ProjectMenuItem::SortCustom | ProjectMenuItem::SortRecentlyUpdated => icons::CHECK,
                ProjectMenuItem::NewWorktree => icons::GIT_BRANCH,
                ProjectMenuItem::NewGroup => icons::WORKER_FOLDER_OPEN,
                ProjectMenuItem::StopAll => icons::STOP,
                ProjectMenuItem::Archived => icons::ARCHIVE_MINIMALISTIC,
                ProjectMenuItem::RevealInFinder | ProjectMenuItem::OpenInEditor => icons::FOLDER,
                ProjectMenuItem::RemoveWorktree
                | ProjectMenuItem::RemoveGroup
                | ProjectMenuItem::RemoveProject => icons::TRASH_BIN_MINIMALISTIC,
            };
            let destructive = matches!(
                item,
                ProjectMenuItem::RemoveWorktree
                    | ProjectMenuItem::RemoveGroup
                    | ProjectMenuItem::RemoveProject
            );
            let menu_project = project.clone();
            let menu_sessions = sessions.clone();
            let mut row = popover::menu_row(theme, false, format!("workers-project-menu-{index}"))
                .id(("workers-project-menu-row", index))
                .child(icon(icon_path).size(px(15.0)).text_color(if destructive {
                    theme.danger
                } else {
                    theme.text_muted
                }))
                .child(SharedString::from(label));
            if destructive {
                row = row.text_color(theme.danger);
            }
            rows.push(
                row.on_click(cx.listener(move |this, _, _, cx| {
                    if matches!(
                        item,
                        ProjectMenuItem::NewSession | ProjectMenuItem::FolderColor
                    ) {
                        return;
                    }
                    this.close_project_menu(cx);
                    match item {
                        ProjectMenuItem::Rename => this.open_project_dialog(
                            menu_project.clone(),
                            ProjectDialogKind::Rename,
                            cx,
                        ),
                        ProjectMenuItem::SortCustom => this.model.update(cx, |model, cx| {
                            model.set_project_session_sort(
                                menu_project.id.clone(),
                                WorkersSessionSort::Custom,
                                cx,
                            )
                        }),
                        ProjectMenuItem::SortRecentlyUpdated => {
                            this.model.update(cx, |model, cx| {
                                model.set_project_session_sort(
                                    menu_project.id.clone(),
                                    WorkersSessionSort::RecentlyUpdated,
                                    cx,
                                )
                            })
                        }
                        ProjectMenuItem::NewWorktree => this.open_project_dialog(
                            menu_project.clone(),
                            ProjectDialogKind::NewWorktree,
                            cx,
                        ),
                        ProjectMenuItem::NewGroup => this.open_project_dialog(
                            menu_project.clone(),
                            ProjectDialogKind::NewGroup,
                            cx,
                        ),
                        ProjectMenuItem::StopAll => this
                            .model
                            .update(cx, |model, cx| model.stop_all(menu_sessions.clone(), cx)),
                        ProjectMenuItem::Archived => this.model.update(cx, |model, cx| {
                            model.open_archive(menu_project.id.clone(), cx)
                        }),
                        ProjectMenuItem::RevealInFinder => this.model.update(cx, |model, cx| {
                            model.reveal_project(menu_project.path.clone(), cx)
                        }),
                        ProjectMenuItem::OpenInEditor => this.model.update(cx, |model, cx| {
                            model.open_project_in_editor(menu_project.path.clone(), cx)
                        }),
                        ProjectMenuItem::RemoveWorktree
                        | ProjectMenuItem::RemoveGroup
                        | ProjectMenuItem::RemoveProject => this.model.update(cx, |model, cx| {
                            model.request_remove_project(menu_project.clone(), cx)
                        }),
                        ProjectMenuItem::NewSession | ProjectMenuItem::FolderColor => {}
                    }
                }))
                .into_any_element(),
            );

            if item == ProjectMenuItem::NewSession {
                let terminal_project = project.clone();
                rows.push(
                    popover::menu_row(theme, false, "workers-project-new-terminal")
                        .id("workers-project-new-terminal-row")
                        .pl(px(30.0))
                        .child("Terminal")
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.close_project_menu(cx);
                            this.model.update(cx, |model, cx| {
                                model.launch(
                                    WorkersLaunchRequest::terminal(terminal_project.id.clone())
                                        .with_optional_worktree(
                                            terminal_project
                                                .worktree_branch
                                                .as_ref()
                                                .map(|_| terminal_project.path.clone()),
                                            terminal_project.worktree_branch.clone(),
                                        ),
                                    cx,
                                )
                            });
                        }))
                        .into_any_element(),
                );
                for (preset_index, preset) in
                    presets.iter().filter(|preset| preset.enabled).enumerate()
                {
                    let preset_project = project.clone();
                    let preset_id = preset.id.clone();
                    rows.push(
                        popover::menu_row(
                            theme,
                            false,
                            format!("workers-project-new-preset-{preset_index}"),
                        )
                        .id(("workers-project-new-preset-row", preset_index))
                        .pl(px(30.0))
                        .child(preset.label.clone())
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.close_project_menu(cx);
                            this.model.update(cx, |model, cx| {
                                model.launch(
                                    WorkersLaunchRequest::preset(
                                        preset_project.id.clone(),
                                        preset_id.clone(),
                                    )
                                    .with_optional_worktree(
                                        preset_project
                                            .worktree_branch
                                            .as_ref()
                                            .map(|_| preset_project.path.clone()),
                                        preset_project.worktree_branch.clone(),
                                    ),
                                    cx,
                                )
                            });
                        }))
                        .into_any_element(),
                    );
                }
                if project.parent_project_id.is_none() && !project.is_group {
                    rows.push(popover::menu_separator().into_any_element());
                    rows.push(
                        popover::menu_row(theme, false, "workers-project-new-worktree-heading")
                            .pl(px(30.0))
                            .child("In a new worktree")
                            .into_any_element(),
                    );
                    let worktree_terminal_project = project.clone();
                    rows.push(
                        popover::menu_row(theme, false, "workers-project-new-worktree-terminal")
                            .id("workers-project-new-worktree-terminal-row")
                            .pl(px(46.0))
                            .child("Terminal")
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.close_project_menu(cx);
                                this.open_project_dialog(
                                    worktree_terminal_project.clone(),
                                    ProjectDialogKind::NewWorktreeSession(None),
                                    cx,
                                );
                            }))
                            .into_any_element(),
                    );
                    for (preset_index, preset) in
                        presets.iter().filter(|preset| preset.enabled).enumerate()
                    {
                        let worktree_preset_project = project.clone();
                        let worktree_preset_id = preset.id.clone();
                        rows.push(
                            popover::menu_row(
                                theme,
                                false,
                                format!("workers-project-new-worktree-preset-{preset_index}"),
                            )
                            .id(("workers-project-new-worktree-preset-row", preset_index))
                            .pl(px(46.0))
                            .child(preset.label.clone())
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.close_project_menu(cx);
                                this.open_project_dialog(
                                    worktree_preset_project.clone(),
                                    ProjectDialogKind::NewWorktreeSession(Some(
                                        worktree_preset_id.clone(),
                                    )),
                                    cx,
                                );
                            }))
                            .into_any_element(),
                        );
                    }
                }
                rows.push(
                    popover::menu_row(theme, false, "workers-project-manage-presets")
                        .id("workers-project-manage-presets-row")
                        .pl(px(30.0))
                        .child("Manage presets…")
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.close_project_menu(cx);
                            this.model.update(cx, |model, cx| {
                                model.open_settings(WorkersSettingsTab::Presets, cx)
                            });
                        }))
                        .into_any_element(),
                );
            }
            if item == ProjectMenuItem::FolderColor {
                const COLORS: [(&str, Option<&str>); 9] = [
                    ("Default", None),
                    ("Sky", Some("sky")),
                    ("Blue", Some("blue")),
                    ("Violet", Some("violet")),
                    ("Rose", Some("rose")),
                    ("Amber", Some("amber")),
                    ("Moss", Some("moss")),
                    ("Teal", Some("teal")),
                    ("Graphite", Some("graphite")),
                ];
                for (color_index, (label, color_id)) in COLORS.into_iter().enumerate() {
                    let color_project_id = project.id.clone();
                    rows.push(
                        popover::menu_row(
                            theme,
                            false,
                            format!("workers-project-color-{color_index}"),
                        )
                        .id(("workers-project-color-row", color_index))
                        .pl(px(30.0))
                        .child(format!(
                            "{}{}",
                            if project.folder_color_id.as_deref() == color_id {
                                "✓ "
                            } else {
                                ""
                            },
                            label
                        ))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.close_project_menu(cx);
                            this.model.update(cx, |model, cx| {
                                model.set_project_color(
                                    color_project_id.clone(),
                                    color_id.map(str::to_owned),
                                    cx,
                                )
                            });
                        }))
                        .into_any_element(),
                    );
                }
            }
        }
        let menu = popover::popover_card(theme)
            .id("workers-project-context-menu-card")
            .w(px(230.0))
            .max_h(px(540.0))
            .overflow_y_scroll()
            .on_mouse_down_out(cx.listener(|this, _, _, cx| this.close_project_menu(cx)))
            .flex()
            .flex_col()
            .children(rows)
            .into_any_element();
        Some(popover::menu_at(
            "workers-project-context-menu",
            position,
            menu,
            closing,
        ))
    }

    fn render_session_menu(&mut self, theme: &Theme, cx: &mut Context<Self>) -> Option<AnyElement> {
        let (session, position) = self.session_menu.get()?.clone();
        let closing = self.session_menu.closing_since();
        let move_target_projects = self
            .model
            .read(cx)
            .projects()
            .iter()
            .filter(|project| project.id != session.project_id)
            .cloned()
            .collect::<Vec<_>>();
        let move_targets = move_target_projects
            .iter()
            .map(|project| (project.name.clone(), Some(project.id.clone())))
            .collect::<Vec<_>>();
        let rows = session_menu_items(&session, &move_target_projects)
            .into_iter()
            .enumerate()
            .flat_map(|(index, item)| {
                let mut elements = Vec::new();
                if matches!(
                    item,
                    SessionMenuItem::CopyTranscript
                        | SessionMenuItem::StopAndArchive
                        | SessionMenuItem::Archive
                        | SessionMenuItem::Restore
                        | SessionMenuItem::RestoreAndResume
                ) {
                    elements.push(popover::menu_separator().into_any_element());
                }
                let menu_session = session.clone();
                let label = match item {
                    SessionMenuItem::Rename => "Rename",
                    SessionMenuItem::Pin => "Pin in project",
                    SessionMenuItem::Unpin => "Unpin from project",
                    SessionMenuItem::MoveTo => "Move to",
                    SessionMenuItem::ClearAttention => "Clear attention",
                    SessionMenuItem::ResumeAgent => "Resume Agent",
                    SessionMenuItem::Resume => "Resume",
                    SessionMenuItem::Fork => "Fork",
                    SessionMenuItem::AppendSystemContext => "Append system context…",
                    SessionMenuItem::NotifyWhenDone => {
                        if session.notify_when_done {
                            "✓ Notify when done"
                        } else {
                            "Notify when done"
                        }
                    }
                    SessionMenuItem::CopyTranscript => "Copy transcript",
                    SessionMenuItem::CopySessionId => "Copy session ID",
                    SessionMenuItem::CopyTranscript20 => "Copy transcript · Last 20 entries",
                    SessionMenuItem::CopyTranscript50 => "Copy transcript · Last 50 entries",
                    SessionMenuItem::CopyTranscriptAll => "Copy transcript · Whole conversation",
                    SessionMenuItem::StopAndArchive => "Stop and archive",
                    SessionMenuItem::Archive => "Archive",
                    SessionMenuItem::Restore => "Restore from archive",
                    SessionMenuItem::RestoreAndResume => "Restore & Resume",
                    SessionMenuItem::Remove => "Remove session",
                };
                let icon_path = match item {
                    SessionMenuItem::Rename => icons::PEN,
                    SessionMenuItem::Pin | SessionMenuItem::Unpin => icons::STAR,
                    SessionMenuItem::MoveTo => icons::FOLDER,
                    SessionMenuItem::ClearAttention => icons::CHECK,
                    SessionMenuItem::ResumeAgent | SessionMenuItem::Resume => icons::RESTART,
                    SessionMenuItem::Fork => icons::GIT_BRANCH,
                    SessionMenuItem::AppendSystemContext => icons::PEN,
                    SessionMenuItem::NotifyWhenDone => icons::BELL,
                    SessionMenuItem::CopyTranscript20
                    | SessionMenuItem::CopyTranscript50
                    | SessionMenuItem::CopyTranscriptAll
                    | SessionMenuItem::CopyTranscript
                    | SessionMenuItem::CopySessionId => icons::COPY,
                    SessionMenuItem::StopAndArchive
                    | SessionMenuItem::Archive
                    | SessionMenuItem::Restore
                    | SessionMenuItem::RestoreAndResume => icons::ARCHIVE_MINIMALISTIC,
                    SessionMenuItem::Remove => icons::TRASH_BIN_MINIMALISTIC,
                };
                let mut row =
                    popover::menu_row(theme, false, format!("workers-session-menu-{index}"))
                        .id(("workers-session-menu-row", index))
                        .child(icon(icon_path).size(px(15.0)).text_color(
                            if item == SessionMenuItem::Remove {
                                theme.danger
                            } else {
                                theme.text_muted
                            },
                        ))
                        .child(SharedString::from(label));
                if item == SessionMenuItem::Remove {
                    row = row.text_color(theme.danger);
                }
                let row = row.on_click(cx.listener(move |this, _, _, cx| {
                    this.close_session_menu(cx);
                    this.perform_session_menu_action(menu_session.clone(), item, cx);
                }));
                elements.push(row.into_any_element());
                if item == SessionMenuItem::MoveTo {
                    for (target_index, (target_name, target_id)) in
                        move_targets.iter().cloned().enumerate()
                    {
                        let target_session_id = session.id.clone();
                        elements.push(
                            popover::menu_row(
                                theme,
                                false,
                                format!("workers-session-move-{target_index}"),
                            )
                            .id(("workers-session-move-row", target_index))
                            .pl(px(30.0))
                            .child(target_name)
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.close_session_menu(cx);
                                this.model.update(cx, |model, cx| {
                                    model.move_session(
                                        target_session_id.clone(),
                                        target_id.clone(),
                                        cx,
                                    )
                                });
                            }))
                            .into_any_element(),
                        );
                    }
                }
                elements
            })
            .collect::<Vec<_>>();
        let menu = popover::popover_card(theme)
            .w(px(210.0))
            .on_mouse_down_out(cx.listener(|this, _, _, cx| this.close_session_menu(cx)))
            .flex()
            .flex_col()
            .children(rows)
            .into_any_element();
        Some(popover::menu_at(
            "workers-session-context-menu",
            position,
            menu,
            closing,
        ))
    }

    fn open_project_dialog(
        &mut self,
        project: WorkersProject,
        kind: ProjectDialogKind,
        cx: &mut Context<Self>,
    ) {
        let placeholder = match kind {
            ProjectDialogKind::Rename => "Name",
            ProjectDialogKind::NewGroup => "Group name",
            ProjectDialogKind::NewWorktree => "Branch name",
            ProjectDialogKind::NewWorktreeSession(_) => "What is this session working on?",
        };
        let input = cx.new(|cx| ComposerInput::new(placeholder, cx));
        if matches!(kind, ProjectDialogKind::Rename) {
            input.update(cx, |input, cx| input.set_text(project.name, cx));
        }
        let events = cx.subscribe(&input, |this: &mut Self, _, event, cx| {
            if matches!(event, ComposerInputEvent::Submitted) {
                this.submit_project_dialog(cx);
            }
        });
        self.project_dialog = Some(ProjectDialog {
            project_id: project.id,
            kind,
            input,
            _events: events,
        });
        cx.notify();
    }

    fn submit_project_dialog(&mut self, cx: &mut Context<Self>) {
        let Some(dialog) = self.project_dialog.take() else {
            return;
        };
        let value = dialog.input.read(cx).text().trim().to_owned();
        if value.is_empty() {
            cx.notify();
            return;
        }
        self.model.update(cx, |model, cx| match dialog.kind {
            ProjectDialogKind::Rename => model.rename_project(dialog.project_id, value, cx),
            ProjectDialogKind::NewGroup => model.create_group(dialog.project_id, value, cx),
            ProjectDialogKind::NewWorktree => {
                model.create_worktree(dialog.project_id, value, None, cx)
            }
            ProjectDialogKind::NewWorktreeSession(preset_id) => {
                let branch = worktree_branch_slug(&value);
                model.create_worktree_and_launch(dialog.project_id, value, branch, preset_id, cx)
            }
        });
        cx.notify();
    }

    fn render_project_dialog(
        &mut self,
        viewport: gpui::Size<gpui::Pixels>,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let theme = Theme::of(cx).clone();
        let dialog = self.project_dialog.as_ref()?;
        let input = dialog.input.clone();
        let (title, action) = match &dialog.kind {
            ProjectDialogKind::Rename => ("Rename", "Rename"),
            ProjectDialogKind::NewGroup => ("New group", "Create"),
            ProjectDialogKind::NewWorktree => ("New worktree", "Create"),
            ProjectDialogKind::NewWorktreeSession(_) => ("New session in worktree", "Create"),
        };
        let card = popover::dialog_card(&theme)
            .child(popover::dialog_title(&theme, title))
            .when(matches!(dialog.kind, ProjectDialogKind::NewWorktree), |el| {
                el.child(
                    div()
                        .mt(px(8.0))
                        .text_size(px(11.0))
                        .text_color(theme.text_muted)
                        .child("Creates or adopts the branch in Unpeel's managed worktrees folder."),
                )
            })
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
                        popover::btn_ghost(&theme, "Cancel", "workers-project-dialog-cancel")
                            .id("workers-project-dialog-cancel")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.project_dialog = None;
                                cx.notify();
                            })),
                    )
                    .child(
                        popover::btn_primary(&theme, action)
                            .id("workers-project-dialog-save")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.submit_project_dialog(cx)
                            })),
                    ),
            )
            .into_any_element();
        Some(popover::modal("workers-project-dialog", viewport, card))
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

    fn open_append_context(&mut self, session: WorkersSession, cx: &mut Context<Self>) {
        let input = cx.new(|cx| ComposerInput::new("System context", cx));
        let events = cx.subscribe(&input, |this: &mut Self, _, event, cx| {
            if matches!(event, ComposerInputEvent::Submitted) {
                this.submit_append_context(cx);
            }
        });
        self.append_context = Some(AppendContextDialog {
            session_id: session.id,
            input,
            _events: events,
        });
        cx.notify();
    }

    fn submit_append_context(&mut self, cx: &mut Context<Self>) {
        let Some(dialog) = self.append_context.take() else {
            return;
        };
        let context = dialog.input.read(cx).text().trim().to_owned();
        if context.is_empty() {
            cx.notify();
            return;
        }
        self.model.update(cx, |model, cx| {
            model.append_system_context(dialog.session_id, context, cx)
        });
        cx.notify();
    }

    fn render_append_context_dialog(
        &mut self,
        viewport: gpui::Size<gpui::Pixels>,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let theme = Theme::of(cx).clone();
        let input = self.append_context.as_ref()?.input.clone();
        let card = popover::dialog_card(&theme)
            .child(popover::dialog_title(&theme, "Append system context"))
            .child(
                div()
                    .mt(px(8.0))
                    .text_size(px(11.0))
                    .text_color(theme.text_muted)
                    .child("This context will be injected when the worker resumes."),
            )
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
                        popover::btn_ghost(&theme, "Cancel", "workers-append-context-cancel")
                            .id("workers-append-context-cancel")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.append_context = None;
                                cx.notify();
                            })),
                    )
                    .child(
                        popover::btn_primary(&theme, "Append")
                            .id("workers-append-context-save")
                            .on_click(cx.listener(|this, _, _, cx| this.submit_append_context(cx))),
                    ),
            )
            .into_any_element();
        Some(popover::modal(
            "workers-append-context-dialog",
            viewport,
            card,
        ))
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
                let icon_path =
                    runtime_icon_path(preset.cli_id.as_deref(), Some(preset.command.as_str()));
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
                    .child(icon(icon_path).size(px(15.0)).text_color(theme.text_muted))
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

    fn render_session(&self, session: WorkersSession, theme: &Theme) -> AnyElement {
        let live = session.is_live();
        div()
            .size_full()
            .bg(crate::terminal::view::terminal_panel_bg(theme))
            .when(live, |el| el.child(self.terminal.clone()))
            .when(!live, |el| {
                el.flex()
                    .items_center()
                    .justify_center()
                    .text_size(px(11.0))
                    .text_color(theme.text_faint)
                    .child("This worker has stopped. Use its context menu to continue.")
            })
            .into_any_element()
    }

    fn render_recent(
        &self,
        snapshot: &zeron_workers_unpeel::WorkersBootstrap,
        theme: &Theme,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_millis() as u64)
            .unwrap_or_default();
        let sections = recent_activity_sections(snapshot, now);
        if sections.is_empty() {
            return div()
                .size_full()
                .flex()
                .items_center()
                .justify_center()
                .pb(px(52.0))
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .items_center()
                        .gap(px(10.0))
                        .child(
                            icon(icons::BELL)
                                .size(px(30.0))
                                .text_color(theme.text_muted.opacity(0.7)),
                        )
                        .child(
                            div()
                                .text_size(px(14.0))
                                .font_weight(gpui::FontWeight::MEDIUM)
                                .text_color(theme.text)
                                .child("No recent activity"),
                        )
                        .child(
                            div()
                                .text_size(px(11.0))
                                .text_color(theme.text_muted)
                                .child("Session starts, finishes, and input requests will appear here."),
                        ),
                )
                .into_any_element();
        }

        let section_elements = sections
            .into_iter()
            .enumerate()
            .map(|(section_index, section)| {
                let rows = section
                    .rows
                    .into_iter()
                    .enumerate()
                    .map(|(row_index, row)| {
                        let target = row.target.clone().filter(|_| row.available);
                        let leading = if row.working {
                            let color = row
                                .spinner_tint
                                .map(|hex| gpui::Hsla::from(gpui::rgb(hex)))
                                .unwrap_or(theme.text_muted);
                            div()
                                .w(px(16.0))
                                .h(px(16.0))
                                .flex()
                                .items_center()
                                .justify_center()
                                .font_family("Menlo")
                                .text_size(px(13.0))
                                .text_color(color)
                                .child(spinner_frame(now))
                                .into_any_element()
                        } else {
                            icon(row.runtime_icon)
                                .size(px(15.0))
                                .text_color(theme.text_muted)
                                .into_any_element()
                        };
                        div()
                            .id(("workers-recent-row", section_index * 10_000 + row_index))
                            .h(px(30.0))
                            .w_full()
                            .px(px(10.0))
                            .flex()
                            .items_center()
                            .gap(px(10.0))
                            .rounded(px(7.0))
                            .opacity(if row.available { 1.0 } else { 0.55 })
                            .when(row.available, |el| {
                                el.cursor_pointer()
                                    .hover(|el| el.bg(crate::theme::ink(0.07)))
                            })
                            .when_some(target, |el, target| {
                                el.on_click(cx.listener(move |this, _, _, cx| {
                                    this.model.update(cx, |model, cx| {
                                        model.request_session_reveal(target.clone(), cx)
                                    });
                                }))
                            })
                            .child(leading)
                            .child(
                                div()
                                    .min_w_0()
                                    .flex_1()
                                    .truncate()
                                    .text_size(px(13.0))
                                    .text_color(theme.text)
                                    .child(row.title),
                            )
                            .when(!row.project.is_empty(), |el| {
                                el.child(
                                    div()
                                        .max_w(px(180.0))
                                        .truncate()
                                        .text_size(px(11.0))
                                        .text_color(theme.text_muted)
                                        .child(row.project),
                                )
                            })
                            .child(
                                div()
                                    .min_w(px(120.0))
                                    .flex()
                                    .justify_end()
                                    .text_size(px(11.0))
                                    .text_color(theme.text_muted)
                                    .child(row.event),
                            )
                            .when(row.unread, |el| {
                                el.child(div().size(px(7.0)).rounded_full().bg(theme.accent))
                            })
                    });
                div()
                    .w_full()
                    .flex()
                    .flex_col()
                    .gap(px(1.0))
                    .child(
                        div()
                            .px(px(10.0))
                            .pt(px(12.0))
                            .pb(px(3.0))
                            .text_size(px(11.0))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_color(theme.text_muted)
                            .child(section.label),
                    )
                    .children(rows)
            });

        div()
            .id("workers-recent-list")
            .size_full()
            .overflow_y_scroll()
            .child(
                div()
                    .w_full()
                    .max_w(px(820.0))
                    .mx_auto()
                    .px(px(30.0))
                    .pt(px(12.0))
                    .pb(px(30.0))
                    .flex()
                    .flex_col()
                    .children(section_elements),
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
                let confirming_remove = self.model.read(cx).confirming_remove_session_id.as_deref()
                    == Some(session.id.as_str());
                if confirming_remove {
                    let title = if session.title.trim().is_empty() {
                        "Untitled session".to_owned()
                    } else {
                        session.title.clone()
                    };
                    return div()
                        .id(("workers-archive-remove-confirm", index))
                        .min_h(px(52.0))
                        .px(px(10.0))
                        .flex()
                        .items_center()
                        .gap(px(8.0))
                        .rounded(px(7.0))
                        .bg(crate::theme::ink(0.10))
                        .child(
                            div()
                                .min_w_0()
                                .flex_1()
                                .truncate()
                                .text_size(px(12.0))
                                .text_color(theme.text)
                                .child(title),
                        )
                        .child(
                            div()
                                .text_size(px(11.0))
                                .font_weight(gpui::FontWeight::MEDIUM)
                                .text_color(theme.text)
                                .child("Delete session and history?"),
                        )
                        .child(
                            div()
                                .id(("workers-archive-remove-cancel", index))
                                .h(px(24.0))
                                .px(px(8.0))
                                .flex()
                                .items_center()
                                .rounded(px(6.0))
                                .cursor_pointer()
                                .text_size(px(11.0))
                                .text_color(theme.text_muted)
                                .bg(crate::theme::ink(0.06))
                                .hover(|el| el.bg(crate::theme::ink(0.10)))
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.model.update(cx, |model, cx| model.cancel_remove(cx));
                                }))
                                .child("Cancel"),
                        )
                        .child(
                            div()
                                .id(("workers-archive-remove-confirm-button", index))
                                .h(px(24.0))
                                .px(px(8.0))
                                .flex()
                                .items_center()
                                .rounded(px(6.0))
                                .cursor_pointer()
                                .text_size(px(11.0))
                                .text_color(theme.danger)
                                .bg(theme.danger.opacity(0.15))
                                .hover(|el| el.bg(theme.danger.opacity(0.25)))
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.model.update(cx, |model, cx| model.confirm_remove(cx));
                                }))
                                .child("Delete"),
                        )
                        .into_any_element();
                }
                let restore_session = session.clone();
                let remove_session = session.clone();
                let title = if session.title.trim().is_empty() {
                    "Untitled session".to_owned()
                } else {
                    session.title.clone()
                };
                let restore = archive_restore_presentation(&session);
                let restore_resume = restore.resume;
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
                                    model.restore(restore_session.clone(), restore_resume, cx)
                                });
                            }))
                            .child(restore.label),
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
                                this.model.update(cx, |model, cx| {
                                    model.request_remove(remove_session.id.clone(), true, cx)
                                });
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
}

fn workers_content_outlet() -> gpui::Div {
    div().flex_1().min_w_0().min_h_0().overflow_hidden()
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum WorkersWorkspacePane {
    Session,
    Launcher,
    Empty,
}

fn workers_workspace_pane(
    has_selected_session: bool,
    has_launcher_project: bool,
) -> WorkersWorkspacePane {
    if has_selected_session {
        WorkersWorkspacePane::Session
    } else if has_launcher_project {
        WorkersWorkspacePane::Launcher
    } else {
        WorkersWorkspacePane::Empty
    }
}

#[cfg(test)]
mod empty_state_tests {
    use super::{WorkersWorkspacePane, workers_workspace_pane};

    #[test]
    fn no_selected_session_uses_empty_state_until_launcher_is_explicitly_requested() {
        assert_eq!(
            workers_workspace_pane(false, false),
            WorkersWorkspacePane::Empty
        );
        assert_eq!(
            workers_workspace_pane(false, true),
            WorkersWorkspacePane::Launcher
        );
    }

    #[test]
    fn selected_session_always_owns_the_workspace_pane() {
        assert_eq!(
            workers_workspace_pane(true, true),
            WorkersWorkspacePane::Session
        );
    }
}

impl Render for WorkersContent {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = Theme::of(cx).clone();
        let (
            loading,
            error,
            snapshot,
            busy,
            selected_session,
            selected_project,
            launcher_project,
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
                model.snapshot.clone(),
                model.action_in_flight(),
                model.selected_session().cloned(),
                model.selected_project().cloned(),
                model.launcher_project().cloned(),
                model.presets().to_vec(),
                model.archive_project_id.clone(),
                model.archived_sessions.clone(),
                model.archive_loading,
                model.archive_error.clone(),
                model.route,
            )
        };

        let has_snapshot = snapshot.is_some();
        let workspace_pane =
            workers_workspace_pane(selected_session.is_some(), launcher_project.is_some());
        let content = if matches!(route, WorkersRoute::Settings(_)) {
            self.settings.clone().into_any_element()
        } else if matches!(route, WorkersRoute::Recent) {
            snapshot
                .as_ref()
                .map(|snapshot| self.render_recent(snapshot, &theme, cx))
                .unwrap_or_else(|| div().size_full().into_any_element())
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
        } else if matches!(workspace_pane, WorkersWorkspacePane::Session) {
            selected_session
                .map(|session| self.render_session(session, &theme))
                .unwrap_or_else(|| div().size_full().into_any_element())
        } else if matches!(workspace_pane, WorkersWorkspacePane::Launcher) {
            // Paint the terminal surface behind the launcher so its exact grid
            // is known before a Session is created. Opacity keeps prepaint and
            // geometry measurement active; the foreground launcher owns input.
            launcher_project
                .map(|project| {
                    div()
                        .relative()
                        .size_full()
                        .child(
                            div()
                                .absolute()
                                .inset_0()
                                .opacity(0.0)
                                .child(self.terminal.clone()),
                        )
                        .child(self.render_launcher(project, presets, busy, &theme, cx))
                        .into_any_element()
                })
                .unwrap_or_else(|| div().size_full().into_any_element())
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
                        .gap(px(16.0))
                        .child(
                            icon(icons::WORKER_UNPEEL_LOGO)
                                .size(px(56.0))
                                .text_color(theme.text_muted),
                        )
                        .child(
                            div()
                                .flex()
                                .flex_col()
                                .items_center()
                                .gap(px(8.0))
                                .child(
                                    div()
                                        .text_size(px(14.0))
                                        .text_color(theme.text_muted)
                                        .child("No session selected"),
                                )
                                .child(
                                    div()
                                        .text_size(px(11.0))
                                        .text_color(theme.text.opacity(0.35))
                                        .child(
                                            "Pick a session in the sidebar, or hit + on a project",
                                        ),
                                ),
                        )
                        .child(
                            div()
                                .text_size(px(10.0))
                                .font_weight(gpui::FontWeight::MEDIUM)
                                .text_color(theme.text.opacity(0.25))
                                .child(format!("v{}", env!("CARGO_PKG_VERSION"))),
                        ),
                )
                .into_any_element()
        };

        let rename_dialog = self.render_rename_dialog(window.viewport_size(), cx);
        let append_context_dialog = self.render_append_context_dialog(window.viewport_size(), cx);
        let project_dialog = self.render_project_dialog(window.viewport_size(), cx);
        let session_menu = self.render_session_menu(&theme, cx);
        let project_menu = self.render_project_menu(&theme, cx);

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
            .child(workers_content_outlet().child(content))
            .when_some(session_menu, |el, menu| el.child(menu))
            .when_some(project_menu, |el, menu| el.child(menu))
            .when_some(rename_dialog, |el, dialog| el.child(dialog))
            .when_some(append_context_dialog, |el, dialog| el.child(dialog))
            .when_some(project_dialog, |el, dialog| el.child(dialog))
            .into_any_element()
    }
}

#[cfg(test)]
mod layout_tests {
    use gpui::Styled as _;

    use super::{project_folder_tint, workers_content_outlet, worktree_branch_slug};

    #[test]
    fn workers_content_outlet_uses_only_the_remaining_vertical_space() {
        let mut outlet = workers_content_outlet();
        let style = outlet.style();

        assert_eq!(style.flex_grow, Some(1.0));
        assert_eq!(style.flex_shrink, Some(1.0));
        assert!(style.min_size.height.is_some());
    }

    #[test]
    fn worktree_task_names_produce_stable_branch_slugs() {
        assert_eq!(
            worktree_branch_slug("Fix Workers sidebar spacing"),
            "fix-workers-sidebar-spacing"
        );
        assert_eq!(worktree_branch_slug("  ---  "), "session");
    }

    #[test]
    fn folder_palette_matches_unpeels_light_and_dark_variants() {
        assert_ne!(
            project_folder_tint(Some("sky"), false),
            project_folder_tint(Some("sky"), true)
        );
        assert!(project_folder_tint(Some("unknown"), true).is_none());
    }
}
