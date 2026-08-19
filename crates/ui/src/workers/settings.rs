use std::collections::{HashMap, HashSet};
use std::time::{SystemTime, UNIX_EPOCH};

use gpui::{
    AnyElement, Context, Entity, IntoElement, Render, Subscription, Window, div, prelude::*, px,
};
use zeron_workers_unpeel::{
    PresetPatch, WorkersAppearanceSettings, WorkersNotificationSettings, WorkersTranscriptSettings,
};

use crate::composer::{ComposerInput, ComposerInputEvent};
use crate::icons::{self, icon};
use crate::settings::widgets;
use crate::theme::Theme;

use super::model::{WorkersModel, WorkersRoute, WorkersSettingsTab};
use super::presentation::{runtime_icon_path, spinner_frame};

fn normalize_preset_command(raw: &str) -> Option<(String, String)> {
    let command = raw.trim();
    (!command.is_empty()).then(|| (command.to_owned(), command.to_owned()))
}

pub struct WorkersSettingsView {
    model: Entity<WorkersModel>,
    command_input: Entity<ComposerInput>,
    editing_preset_id: Option<String>,
    _model_observation: Subscription,
    _command_events: Subscription,
}

impl WorkersSettingsView {
    pub fn new(model: Entity<WorkersModel>, cx: &mut Context<Self>) -> Self {
        let command_input = cx.new(|cx| ComposerInput::new("Add command (e.g. claude --plan)", cx));
        let command_events = cx.subscribe(&command_input, |this: &mut Self, _, event, cx| {
            if matches!(event, ComposerInputEvent::Submitted) {
                this.submit_preset(cx);
            }
        });
        let model_observation = cx.observe(&model, |_, _, cx| cx.notify());
        Self {
            model,
            command_input,
            editing_preset_id: None,
            _model_observation: model_observation,
            _command_events: command_events,
        }
    }

    fn submit_preset(&mut self, cx: &mut Context<Self>) {
        let Some((label, command)) = normalize_preset_command(self.command_input.read(cx).text())
        else {
            return;
        };
        if let Some(id) = self.editing_preset_id.take() {
            self.model.update(cx, |model, cx| {
                model.update_preset(
                    id,
                    PresetPatch {
                        label: Some(label),
                        command: Some(command),
                        ..PresetPatch::default()
                    },
                    cx,
                );
            });
        } else {
            self.model
                .update(cx, |model, cx| model.add_preset(label, command, cx));
        }
        self.command_input
            .update(cx, |input, cx| input.set_text("", cx));
        cx.notify();
    }

    fn begin_edit(&mut self, id: String, command: String, cx: &mut Context<Self>) {
        self.editing_preset_id = Some(id);
        self.command_input
            .update(cx, |input, cx| input.set_text(command, cx));
        cx.notify();
    }

    fn page_shell(&self, tab: WorkersSettingsTab, body: AnyElement, theme: &Theme) -> AnyElement {
        div()
            .id("workers-settings-page")
            .size_full()
            .overflow_y_scroll()
            .child(
                div()
                    .w_full()
                    .max_w(px(720.0))
                    .mx_auto()
                    .px(px(28.0))
                    .pt(px(12.0))
                    .pb(px(64.0))
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .mb(px(34.0))
                            .text_size(px(12.0))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_color(theme.text_muted)
                            .child(format!("Settings  /  {}", tab.label())),
                    )
                    .child(body),
            )
            .into_any_element()
    }

    fn render_presets(&mut self, theme: &Theme, cx: &mut Context<Self>) -> AnyElement {
        let (settings, loading, busy, error, installing, install_errors) = {
            let model = self.model.read(cx);
            let runtime_ids = model
                .settings
                .as_ref()
                .map(|settings| {
                    settings
                        .runtimes
                        .iter()
                        .map(|runtime| runtime.cli_id.as_str())
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            (
                model.settings.clone(),
                model.settings_loading,
                model.action_in_flight(),
                model.settings_error.clone(),
                runtime_ids
                    .iter()
                    .filter(|cli_id| model.runtime_install_in_progress(cli_id))
                    .map(|cli_id| (*cli_id).to_owned())
                    .collect::<HashSet<_>>(),
                runtime_ids
                    .iter()
                    .filter_map(|cli_id| {
                        model
                            .runtime_install_error(cli_id)
                            .map(|error| ((*cli_id).to_owned(), error.to_owned()))
                    })
                    .collect::<HashMap<_, _>>(),
            )
        };
        let presets = settings
            .as_ref()
            .map(|settings| settings.presets.clone())
            .unwrap_or_default();
        let runtimes = settings
            .as_ref()
            .map(|settings| settings.runtimes.clone())
            .unwrap_or_default();
        let rows = presets.into_iter().enumerate().map(|(index, preset)| {
            let provider_icon =
                runtime_icon_path(preset.cli_id.as_deref(), Some(preset.command.as_str()));
            let edit_id = preset.id.clone();
            let edit_command = preset.command.clone();
            let favorite_id = preset.id.clone();
            let enabled_id = preset.id.clone();
            let move_id = preset.id.clone();
            let delete_id = preset.id.clone();
            let quick_launch = preset.quick_launch;
            let enabled = preset.enabled;
            div()
                .id(("workers-preset-setting", index))
                .min_h(px(48.0))
                .px(px(12.0))
                .flex()
                .items_center()
                .gap(px(10.0))
                .rounded(px(10.0))
                .border_1()
                .border_color(theme.border.opacity(0.72))
                .bg(crate::theme::ink(0.025))
                .when(!enabled, |el| el.opacity(0.62))
                .child(
                    icon(provider_icon)
                        .size(px(15.0))
                        .text_color(theme.text_muted),
                )
                .child(
                    div()
                        .id(("workers-preset-edit", index))
                        .min_w_0()
                        .flex_1()
                        .flex()
                        .items_center()
                        .gap(px(8.0))
                        .cursor_pointer()
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.begin_edit(edit_id.clone(), edit_command.clone(), cx);
                        }))
                        .child(
                            div()
                                .truncate()
                                .font_family("monospace")
                                .text_size(px(13.0))
                                .text_color(theme.text)
                                .child(preset.command),
                        )
                        .when(preset.risky, |el| {
                            el.child(
                                div()
                                    .px(px(6.0))
                                    .py(px(2.0))
                                    .rounded(px(5.0))
                                    .bg(theme.danger.opacity(0.16))
                                    .text_size(px(9.5))
                                    .text_color(theme.danger_muted)
                                    .child("Risky"),
                            )
                        }),
                )
                .child(
                    div()
                        .id(("workers-preset-favorite", index))
                        .size(px(26.0))
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded(px(7.0))
                        .cursor_pointer()
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.model.update(cx, |model, cx| {
                                model.update_preset(
                                    favorite_id.clone(),
                                    PresetPatch {
                                        quick_launch: Some(!quick_launch),
                                        ..PresetPatch::default()
                                    },
                                    cx,
                                )
                            });
                        }))
                        .child(
                            icon(if quick_launch {
                                icons::STAR_BOLD
                            } else {
                                icons::STAR
                            })
                            .size(px(14.0))
                            .text_color(if quick_launch {
                                theme.text
                            } else {
                                theme.text_faint
                            }),
                        ),
                )
                .child(
                    widgets::toggle_switch(theme, enabled)
                        .id(("workers-preset-enabled", index))
                        .cursor_pointer()
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.model.update(cx, |model, cx| {
                                model.update_preset(
                                    enabled_id.clone(),
                                    PresetPatch {
                                        enabled: Some(!enabled),
                                        ..PresetPatch::default()
                                    },
                                    cx,
                                )
                            });
                        })),
                )
                .child(
                    div()
                        .id(("workers-preset-up", index))
                        .size(px(26.0))
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded(px(7.0))
                        .cursor_pointer()
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.model.update(cx, |model, cx| {
                                model.move_preset(move_id.clone(), index.saturating_sub(1), cx)
                            });
                        }))
                        .child(
                            icon(icons::ARROW_UP)
                                .size(px(12.0))
                                .text_color(theme.text_faint),
                        ),
                )
                .child(
                    div()
                        .id(("workers-preset-delete", index))
                        .size(px(26.0))
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded(px(7.0))
                        .cursor_pointer()
                        .hover(|el| el.bg(theme.danger.opacity(0.08)))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.model
                                .update(cx, |model, cx| model.delete_preset(delete_id.clone(), cx));
                        }))
                        .child(
                            icon(icons::TRASH_BIN_MINIMALISTIC)
                                .size(px(12.0))
                                .text_color(theme.text_faint),
                        ),
                )
        });
        let add_command = self.command_input.read(cx).text().trim().to_owned();
        let add_ready = !add_command.is_empty();
        let add_icon = runtime_icon_path(None, Some(&add_command));
        let not_installed = runtimes
            .into_iter()
            .filter(|runtime| !runtime.installed)
            .enumerate()
            .map(|(index, runtime)| {
                let cli_id = runtime.cli_id.clone();
                let official_url = runtime.official_url.clone();
                let website_url = official_url.clone();
                let has_install_command = runtime.install_command.is_some();
                let is_installing = installing.contains(&runtime.cli_id);
                let install_error = install_errors.get(&runtime.cli_id).cloned();
                let provider_icon = runtime_icon_path(Some(&runtime.cli_id), None);
                let now_ms = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_millis() as u64;
                div()
                    .id(("workers-runtime-not-installed", index))
                    .min_h(px(46.0))
                    .px(px(12.0))
                    .flex()
                    .items_center()
                    .gap(px(10.0))
                    .rounded(px(10.0))
                    .border_1()
                    .border_color(theme.border.opacity(0.55))
                    .child(
                        icon(provider_icon)
                            .size(px(15.0))
                            .text_color(theme.text_faint),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .items_center()
                            .gap(px(8.0))
                            .child(
                                div()
                                    .text_size(px(12.5))
                                    .font_weight(gpui::FontWeight::MEDIUM)
                                    .text_color(theme.text_muted)
                                    .child(runtime.label),
                            )
                            .when_some(install_error.clone(), |el, error| {
                                el.child(
                                    div()
                                        .min_w_0()
                                        .truncate()
                                        .text_size(px(10.5))
                                        .text_color(theme.danger_muted)
                                        .child(error),
                                )
                            }),
                    )
                    .child(
                        div()
                            .id(("workers-runtime-install", index))
                            .px(px(9.0))
                            .py(px(5.0))
                            .rounded(px(7.0))
                            .bg(crate::theme::ink(0.08))
                            .when(!is_installing, |el| el.cursor_pointer())
                            .when(is_installing, |el| el.opacity(0.7))
                            .text_size(px(10.5))
                            .text_color(theme.text_muted)
                            .on_click(cx.listener(move |this, _, _, cx| {
                                if is_installing {
                                    return;
                                }
                                if has_install_command {
                                    this.model.update(cx, |model, cx| {
                                        model.install_runtime(cli_id.clone(), cx)
                                    });
                                } else if let Some(url) = &official_url {
                                    cx.open_url(url);
                                }
                            }))
                            .child(if is_installing {
                                format!("{} Installing…", spinner_frame(now_ms))
                            } else if has_install_command {
                                "Install".to_owned()
                            } else {
                                "Website".to_owned()
                            }),
                    )
                    .when(install_error.is_some() && website_url.is_some(), |el| {
                        el.child(
                            div()
                                .id(("workers-runtime-website", index))
                                .px(px(9.0))
                                .py(px(5.0))
                                .rounded(px(7.0))
                                .bg(crate::theme::ink(0.08))
                                .cursor_pointer()
                                .text_size(px(10.5))
                                .text_color(theme.text_muted)
                                .on_click(cx.listener(move |_, _, _, cx| {
                                    if let Some(url) = &website_url {
                                        cx.open_url(url);
                                    }
                                }))
                                .child("Website"),
                        )
                    })
            });
        let body = div()
            .flex()
            .flex_col()
            .child(
                div()
                    .flex()
                    .items_center()
                    .child(
                        div()
                            .flex_1()
                            .text_size(px(21.0))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_color(theme.text)
                            .child("Presets"),
                    )
                    .child(
                        div()
                            .id("workers-rescan-path")
                            .h(px(30.0))
                            .px(px(10.0))
                            .flex()
                            .items_center()
                            .gap(px(6.0))
                            .rounded(px(7.0))
                            .bg(crate::theme::ink(0.08))
                            .cursor_pointer()
                            .text_size(px(10.5))
                            .text_color(theme.text_muted)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.model.update(cx, |model, cx| model.refresh_settings(cx));
                            }))
                            .child(icon(icons::REFRESH).size(px(12.0)).text_color(theme.text_muted))
                            .child("Rescan PATH"),
                    ),
            )
            .child(
                div()
                    .mt(px(5.0))
                    .text_size(px(12.5))
                    .text_color(theme.text_muted)
                    .child("Launch commands for your agents. Drag to reorder — a CLI's topmost preset is its default."),
            )
            .when_some(error, |el, error| {
                el.child(div().mt(px(12.0)).text_size(px(11.0)).text_color(theme.danger_muted).child(error))
            })
            .when(loading, |el| {
                el.child(div().mt(px(20.0)).text_size(px(11.0)).text_color(theme.text_faint).child("Loading presets…"))
            })
            .child(div().mt(px(18.0)).flex().flex_col().gap(px(6.0)).children(rows))
            .child(
                div()
                    .mt(px(10.0))
                    .flex()
                    .items_center()
                    .gap(px(10.0))
                    .px(px(12.0))
                    .py(px(8.0))
                    .rounded(px(10.0))
                    .border_1()
                    .border_color(theme.border.opacity(0.72))
                    .bg(crate::theme::ink(0.025))
                    .child(icon(add_icon).size(px(15.0)).text_color(theme.text_muted))
                    .child(div().min_w_0().flex_1().child(self.command_input.clone()))
                    .child(
                        div()
                            .id("workers-preset-save")
                            .px(px(9.0))
                            .py(px(5.0))
                            .flex()
                            .items_center()
                            .rounded(px(7.0))
                            .bg(crate::theme::ink(0.08))
                            .text_size(px(10.5))
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .text_color(theme.text_muted)
                            .when(add_ready && !busy, |el| el.cursor_pointer())
                            .when(!add_ready || busy, |el| el.opacity(0.45))
                            .on_click(cx.listener(|this, _, _, cx| this.submit_preset(cx)))
                            .child(if self.editing_preset_id.is_some() { "Save" } else { "Add" }),
                    ),
            )
            .child(div().mt(px(24.0)).mb(px(10.0)).text_size(px(11.0)).font_weight(gpui::FontWeight::SEMIBOLD).text_color(theme.text_muted).child("NOT INSTALLED"))
            .child(div().flex().flex_col().gap(px(6.0)).children(not_installed));
        self.page_shell(WorkersSettingsTab::Presets, body.into_any_element(), theme)
    }

    fn transcript_row(
        &self,
        index: usize,
        title: &'static str,
        description: &'static str,
        enabled: bool,
        next: WorkersTranscriptSettings,
        theme: &Theme,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .id(("workers-transcript-toggle", index))
            .min_h(px(58.0))
            .px(px(16.0))
            .flex()
            .items_center()
            .gap(px(12.0))
            .when(index > 0, |el| el.border_t_1().border_color(theme.border))
            .child(
                div()
                    .flex_1()
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .text_size(px(13.0))
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .text_color(theme.text)
                            .child(title),
                    )
                    .child(
                        div()
                            .mt(px(3.0))
                            .text_size(px(11.0))
                            .text_color(theme.text_muted)
                            .child(description),
                    ),
            )
            .child(
                widgets::toggle_switch(theme, enabled)
                    .id(("workers-transcript-switch", index))
                    .cursor_pointer()
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.model.update(cx, |model, cx| {
                            model.set_transcript_settings(next.clone(), cx)
                        });
                    })),
            )
            .into_any_element()
    }

    fn render_transcripts(&self, theme: &Theme, cx: &mut Context<Self>) -> AnyElement {
        let current = self
            .model
            .read(cx)
            .settings
            .as_ref()
            .map(|settings| settings.transcripts.clone())
            .unwrap_or_default();
        let definitions: [(&str, &str, bool, fn(&mut WorkersTranscriptSettings)); 7] = [
            (
                "Session info header",
                "Title, session id, CLI, model, and command.",
                current.include_session_info,
                |s| s.include_session_info = !s.include_session_info,
            ),
            (
                "User messages",
                "Include prompts sent to the worker.",
                current.include_user,
                |s| s.include_user = !s.include_user,
            ),
            (
                "Assistant messages",
                "Include responses from the agent.",
                current.include_assistant,
                |s| s.include_assistant = !s.include_assistant,
            ),
            (
                "Reasoning",
                "Include reasoning and thinking entries.",
                current.include_reasoning,
                |s| s.include_reasoning = !s.include_reasoning,
            ),
            (
                "Tool calls & results",
                "Include tool invocation details and output.",
                current.include_tools,
                |s| s.include_tools = !s.include_tools,
            ),
            (
                "File changes & diffs",
                "Include edits and patches produced by the worker.",
                current.include_file_changes,
                |s| s.include_file_changes = !s.include_file_changes,
            ),
            (
                "Plan updates",
                "Include plan and task-list changes.",
                current.include_plan_updates,
                |s| s.include_plan_updates = !s.include_plan_updates,
            ),
        ];
        let rows = definitions
            .into_iter()
            .enumerate()
            .map(|(index, (title, description, enabled, toggle))| {
                let mut next = current.clone();
                toggle(&mut next);
                self.transcript_row(index, title, description, enabled, next, theme, cx)
            })
            .collect::<Vec<_>>();
        let ranges = [
            (0usize, "Whole transcript"),
            (20, "20 entries"),
            (50, "50 entries"),
            (100, "100 entries"),
        ]
        .into_iter()
        .enumerate()
        .map(|(index, (value, label))| {
            let mut next = current.clone();
            next.max_entries = value;
            div()
                .id(("workers-transcript-range", index))
                .h(px(30.0))
                .px(px(10.0))
                .flex()
                .items_center()
                .rounded(px(7.0))
                .cursor_pointer()
                .text_size(px(11.0))
                .text_color(if current.max_entries == value {
                    theme.text
                } else {
                    theme.text_muted
                })
                .when(current.max_entries == value, |el| {
                    el.bg(crate::theme::ink(0.12))
                })
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.model.update(cx, |model, cx| {
                        model.set_transcript_settings(next.clone(), cx)
                    });
                }))
                .child(label)
        });
        let body = div()
            .flex()
            .flex_col()
            .child(
                div()
                    .text_size(px(21.0))
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .text_color(theme.text)
                    .child("Transcripts"),
            )
            .child(
                div()
                    .mt(px(5.0))
                    .text_size(px(12.5))
                    .text_color(theme.text_muted)
                    .child("Choose what is included when a worker transcript is copied or read."),
            )
            .child(
                div()
                    .mt(px(18.0))
                    .rounded(px(12.0))
                    .border_1()
                    .border_color(theme.border)
                    .overflow_hidden()
                    .children(rows),
            )
            .child(
                div()
                    .mt(px(20.0))
                    .text_size(px(12.0))
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .text_color(theme.text)
                    .child("Conversation range"),
            )
            .child(div().mt(px(8.0)).flex().gap(px(6.0)).children(ranges));
        self.page_shell(
            WorkersSettingsTab::Transcripts,
            body.into_any_element(),
            theme,
        )
    }

    fn render_notifications(&self, theme: &Theme, cx: &mut Context<Self>) -> AnyElement {
        let current = self
            .model
            .read(cx)
            .settings
            .as_ref()
            .map(|settings| settings.notifications.clone())
            .unwrap_or_default();
        let rows: [(&str, &str, bool, fn(&mut WorkersNotificationSettings)); 4] = [
            (
                "Flag menus waiting for a choice",
                "Show the amber attention indicator for interactive CLI menus.",
                current.menu_attention_detection,
                |s| s.menu_attention_detection = !s.menu_attention_detection,
            ),
            (
                "Desktop notifications",
                "Show a system banner when a worker needs attention or finishes.",
                current.desktop_notifications,
                |s| s.desktop_notifications = !s.desktop_notifications,
            ),
            (
                "Sounds",
                "Play distinct chimes for attention and completion.",
                current.sound_enabled,
                |s| s.sound_enabled = !s.sound_enabled,
            ),
            (
                "Only when in the background",
                "Reserve desktop banners for times when Comet is not focused.",
                current.background_only,
                |s| s.background_only = !s.background_only,
            ),
        ];
        let controls =
            rows.into_iter()
                .enumerate()
                .map(|(index, (title, description, enabled, toggle))| {
                    let mut next = current.clone();
                    toggle(&mut next);
                    div()
                        .id(("workers-notification-toggle", index))
                        .min_h(px(62.0))
                        .px(px(16.0))
                        .flex()
                        .items_center()
                        .gap(px(12.0))
                        .when(index > 0, |el| el.border_t_1().border_color(theme.border))
                        .child(
                            div()
                                .flex_1()
                                .flex()
                                .flex_col()
                                .child(
                                    div()
                                        .text_size(px(13.0))
                                        .font_weight(gpui::FontWeight::MEDIUM)
                                        .text_color(theme.text)
                                        .child(title),
                                )
                                .child(
                                    div()
                                        .mt(px(3.0))
                                        .text_size(px(11.0))
                                        .text_color(theme.text_muted)
                                        .child(description),
                                ),
                        )
                        .child(
                            widgets::toggle_switch(theme, enabled)
                                .id(("workers-notification-switch", index))
                                .cursor_pointer()
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.model.update(cx, |model, cx| {
                                        model.set_notification_settings(next.clone(), cx)
                                    });
                                })),
                        )
                });
        let body = div()
            .flex()
            .flex_col()
            .child(
                div()
                    .text_size(px(21.0))
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .text_color(theme.text)
                    .child("Notifications"),
            )
            .child(
                div()
                    .mt(px(5.0))
                    .text_size(px(12.5))
                    .text_color(theme.text_muted)
                    .child("How local worker activity reaches you."),
            )
            .child(
                div()
                    .mt(px(18.0))
                    .rounded(px(12.0))
                    .border_1()
                    .border_color(theme.border)
                    .overflow_hidden()
                    .children(controls),
            )
            .child(
                div()
                    .id("workers-test-notification")
                    .mt(px(16.0))
                    .h(px(34.0))
                    .self_start()
                    .px(px(12.0))
                    .flex()
                    .items_center()
                    .gap(px(7.0))
                    .rounded(px(8.0))
                    .bg(crate::theme::ink(0.08))
                    .cursor_pointer()
                    .text_size(px(11.0))
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .text_color(theme.text)
                    .on_click(cx.listener(|this, _, _, cx| this.model.read(cx).test_notification()))
                    .child(
                        icon(icons::BELL)
                            .size(px(13.0))
                            .text_color(theme.text_muted),
                    )
                    .child("Send test notification"),
            );
        self.page_shell(
            WorkersSettingsTab::Notifications,
            body.into_any_element(),
            theme,
        )
    }

    fn render_appearance(&self, theme: &Theme, cx: &mut Context<Self>) -> AnyElement {
        let current = self
            .model
            .read(cx)
            .settings
            .as_ref()
            .map(|settings| settings.appearance.clone())
            .unwrap_or_default();
        let mut next = current.clone();
        next.show_session_gallery = !next.show_session_gallery;
        let body = div()
            .flex()
            .flex_col()
            .child(
                div()
                    .text_size(px(21.0))
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .text_color(theme.text)
                    .child("Appearance"),
            )
            .child(
                div()
                    .mt(px(5.0))
                    .text_size(px(12.5))
                    .text_color(theme.text_muted)
                    .child("How local Workers looks and behaves."),
            )
            .child(
                div()
                    .mt(px(22.0))
                    .text_size(px(11.0))
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .text_color(theme.text_muted)
                    .child("TERMINAL"),
            )
            .child(
                div()
                    .mt(px(8.0))
                    .min_h(px(70.0))
                    .px(px(16.0))
                    .flex()
                    .items_center()
                    .gap(px(12.0))
                    .rounded(px(12.0))
                    .border_1()
                    .border_color(theme.border)
                    .child(
                        div()
                            .flex_1()
                            .flex()
                            .flex_col()
                            .child(
                                div()
                                    .text_size(px(13.0))
                                    .font_weight(gpui::FontWeight::MEDIUM)
                                    .text_color(theme.text)
                                    .child("Session gallery"),
                            )
                            .child(
                                div()
                                    .mt(px(3.0))
                                    .text_size(px(11.0))
                                    .text_color(theme.text_muted)
                                    .child("Show session captures and screenshot controls in the terminal title bar."),
                            ),
                    )
                    .child(
                        widgets::toggle_switch(theme, current.show_session_gallery)
                            .id("workers-session-gallery-switch")
                            .cursor_pointer()
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.model.update(cx, |model, cx| {
                                    model.set_appearance_settings(next.clone(), cx)
                                });
                            })),
                    ),
            );
        self.page_shell(
            WorkersSettingsTab::Appearance,
            body.into_any_element(),
            theme,
        )
    }
}

impl Render for WorkersSettingsView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = Theme::of(cx).clone();
        let tab = match self.model.read(cx).route {
            WorkersRoute::Settings(tab) => tab,
            WorkersRoute::Workspace | WorkersRoute::Recent => WorkersSettingsTab::Presets,
        };
        match tab {
            WorkersSettingsTab::Presets => self.render_presets(&theme, cx),
            WorkersSettingsTab::Appearance => self.render_appearance(&theme, cx),
            WorkersSettingsTab::Transcripts => self.render_transcripts(&theme, cx),
            WorkersSettingsTab::Notifications => self.render_notifications(&theme, cx),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::normalize_preset_command;

    #[test]
    fn preset_add_row_uses_the_command_as_its_label_like_unpeel() {
        assert_eq!(
            normalize_preset_command("  codex --plan  "),
            Some(("codex --plan".to_owned(), "codex --plan".to_owned()))
        );
        assert_eq!(normalize_preset_command("   "), None);
    }
}
