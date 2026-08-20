use std::collections::{HashMap, HashSet};
use std::time::{SystemTime, UNIX_EPOCH};

use gpui::{
    AnyElement, AppContext as _, Context, Entity, IntoElement, Render, Subscription, Window, div,
    prelude::*, px,
};
use zeron_workers_unpeel::resources::WorkersSessionResource;
use zeron_workers_unpeel::{
    PresetPatch, WorkersAppearanceSettings, WorkersNotificationSettings, WorkersResourceSettings,
    WorkersTranscriptSettings,
};

use crate::composer::{ComposerInput, ComposerInputEvent};
use crate::icons::{self, icon};
use crate::settings::widgets;
use crate::theme::Theme;

use super::model::{WorkersModel, WorkersRoute, WorkersSettingsTab};
use super::presentation::{runtime_icon_path, spinner_frame};
use super::resource_monitor::{WorkersResourceGlobal, WorkersResourceMonitor};

fn format_memory_bytes(bytes: u64) -> String {
    const KIB: f64 = 1024.0;
    const MIB: f64 = KIB * 1024.0;
    const GIB: f64 = MIB * 1024.0;
    let bytes_f64 = bytes as f64;
    if bytes_f64 >= GIB {
        format!("{:.1} GiB", bytes_f64 / GIB)
    } else if bytes_f64 >= MIB {
        format!("{:.1} MiB", bytes_f64 / MIB)
    } else if bytes_f64 >= KIB {
        format!("{:.1} KiB", bytes_f64 / KIB)
    } else {
        format!("{bytes} B")
    }
}

fn format_cpu_percent(cpu_percent: f64) -> String {
    format!("{:.1}%", cpu_percent.max(0.0))
}

fn sort_resource_sessions(sessions: &mut [WorkersSessionResource]) {
    sessions.sort_by(|left, right| {
        right
            .physical_footprint_bytes
            .cmp(&left.physical_footprint_bytes)
            .then_with(|| left.session_id.cmp(&right.session_id))
    });
}

#[derive(Clone, Copy)]
enum ThresholdKind {
    Warning,
    Critical,
}

fn threshold_settings(
    settings: &WorkersResourceSettings,
    kind: ThresholdKind,
    delta: i16,
) -> WorkersResourceSettings {
    let mut next = settings.clone();
    match kind {
        ThresholdKind::Warning => {
            next.per_worker_warning_gib = add_signed_u16(
                next.per_worker_warning_gib,
                delta,
                1,
                next.per_worker_critical_gib,
            );
        }
        ThresholdKind::Critical => {
            next.per_worker_critical_gib = add_signed_u16(
                next.per_worker_critical_gib,
                delta,
                next.per_worker_warning_gib,
                1_024,
            );
        }
    }
    next
}

fn add_signed_u16(value: u16, delta: i16, minimum: u16, maximum: u16) -> u16 {
    (i32::from(value) + i32::from(delta)).clamp(i32::from(minimum), i32::from(maximum)) as u16
}

fn resource_metric_card(theme: &Theme, label: &'static str, value: String) -> AnyElement {
    div()
        .flex_1()
        .min_h(px(68.0))
        .p(px(12.0))
        .rounded(px(10.0))
        .border_1()
        .border_color(theme.border.opacity(0.72))
        .bg(crate::theme::ink(0.025))
        .child(
            div()
                .text_size(px(10.5))
                .text_color(theme.text_muted)
                .child(label),
        )
        .child(
            div()
                .mt(px(5.0))
                .text_size(px(17.0))
                .font_weight(gpui::FontWeight::SEMIBOLD)
                .text_color(theme.text)
                .child(value),
        )
        .into_any_element()
}

fn resource_setting_row(
    theme: &Theme,
    title: &'static str,
    description: &'static str,
) -> gpui::Div {
    div()
        .min_h(px(58.0))
        .px(px(12.0))
        .flex()
        .items_center()
        .gap(px(12.0))
        .rounded(px(10.0))
        .border_1()
        .border_color(theme.border.opacity(0.72))
        .bg(crate::theme::ink(0.025))
        .child(
            div()
                .flex_1()
                .child(
                    div()
                        .text_size(px(12.5))
                        .text_color(theme.text)
                        .child(title),
                )
                .child(
                    div()
                        .mt(px(2.0))
                        .text_size(px(10.5))
                        .text_color(theme.text_muted)
                        .child(description),
                ),
        )
}

fn resource_stepper(
    theme: &Theme,
    id: &'static str,
    value: u16,
    decrement: WorkersResourceSettings,
    increment: WorkersResourceSettings,
    cx: &mut Context<WorkersSettingsView>,
) -> AnyElement {
    div()
        .flex()
        .items_center()
        .gap(px(4.0))
        .child(
            div()
                .mr(px(4.0))
                .text_size(px(11.0))
                .text_color(theme.text_muted)
                .child(format!("{value} GiB")),
        )
        .child(
            div()
                .id(format!("{id}-minus"))
                .size(px(26.0))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(7.0))
                .cursor_pointer()
                .hover(|el| el.bg(crate::theme::ink(0.1)))
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.model.update(cx, |model, cx| {
                        model.set_resource_settings(decrement.clone(), cx)
                    });
                }))
                .child("−"),
        )
        .child(
            div()
                .id(format!("{id}-plus"))
                .size(px(26.0))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(7.0))
                .cursor_pointer()
                .hover(|el| el.bg(crate::theme::ink(0.1)))
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.model.update(cx, |model, cx| {
                        model.set_resource_settings(increment.clone(), cx)
                    });
                }))
                .child("+"),
        )
        .into_any_element()
}

fn normalize_preset_command(raw: &str) -> Option<(String, String)> {
    let command = raw.trim();
    (!command.is_empty()).then(|| (command.to_owned(), command.to_owned()))
}

pub struct WorkersSettingsView {
    model: Entity<WorkersModel>,
    resource_monitor: Entity<WorkersResourceMonitor>,
    command_input: Entity<ComposerInput>,
    editing_preset_id: Option<String>,
    expanded_resource_sessions: HashSet<String>,
    _model_observation: Subscription,
    _resource_observation: Subscription,
    _command_events: Subscription,
}

impl WorkersSettingsView {
    pub fn new(model: Entity<WorkersModel>, cx: &mut Context<Self>) -> Self {
        let resource_monitor = cx.global::<WorkersResourceGlobal>().monitor.clone();
        let command_input = cx.new(|cx| ComposerInput::new("Add command (e.g. claude --plan)", cx));
        let command_events = cx.subscribe(&command_input, |this: &mut Self, _, event, cx| {
            if matches!(event, ComposerInputEvent::Submitted) {
                this.submit_preset(cx);
            }
        });
        let observed_monitor = resource_monitor.clone();
        let model_observation = cx.observe(&model, move |_, model, cx| {
            let details_requested = matches!(
                model.read(cx).route,
                WorkersRoute::Settings(WorkersSettingsTab::Resources)
            );
            observed_monitor.update(cx, |monitor, cx| {
                monitor.set_details_requested(details_requested, cx)
            });
            cx.notify();
        });
        let resource_observation = cx.observe(&resource_monitor, |_, _, cx| cx.notify());
        let details_requested = matches!(
            model.read(cx).route,
            WorkersRoute::Settings(WorkersSettingsTab::Resources)
        );
        resource_monitor.update(cx, |monitor, cx| {
            monitor.set_details_requested(details_requested, cx)
        });
        Self {
            model,
            resource_monitor,
            command_input,
            editing_preset_id: None,
            expanded_resource_sessions: HashSet::new(),
            _model_observation: model_observation,
            _resource_observation: resource_observation,
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
                "Reserve desktop banners for times when Zeron is not focused.",
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

    fn render_resources(&mut self, theme: &Theme, cx: &mut Context<Self>) -> AnyElement {
        let monitor = self.resource_monitor.read(cx);
        let snapshot = monitor.snapshot().cloned();
        let last_error = monitor.last_error().map(str::to_owned);
        let sampling = monitor.is_sampling();
        let pressure_label = monitor.pressure_level().label();
        let settings = self
            .model
            .read(cx)
            .settings
            .as_ref()
            .map(|settings| settings.resources.clone())
            .unwrap_or_else(|| monitor.settings().clone());
        let session_metadata = self
            .model
            .read(cx)
            .sessions()
            .iter()
            .map(|session| (session.id.clone(), session.title.clone()))
            .collect::<HashMap<_, _>>();
        let mut sessions = snapshot
            .as_ref()
            .map(|snapshot| snapshot.sessions.clone())
            .unwrap_or_default();
        sort_resource_sessions(&mut sessions);

        let total_cpu = sessions
            .iter()
            .map(|session| session.cpu_percent)
            .sum::<f64>();
        let total_memory = sessions
            .iter()
            .map(|session| session.physical_footprint_bytes)
            .sum::<u64>();
        let total_processes = sessions
            .iter()
            .map(|session| session.process_count)
            .sum::<usize>();
        let sample_age = snapshot.as_ref().map(|snapshot| {
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64;
            now.saturating_sub(snapshot.sampled_at_unix_ms) / 1_000
        });

        let monitoring_next = WorkersResourceSettings {
            monitoring_enabled: !settings.monitoring_enabled,
            ..settings.clone()
        };
        let notifications_next = WorkersResourceSettings {
            notifications_enabled: !settings.notifications_enabled,
            ..settings.clone()
        };
        let warning_decrement = threshold_settings(&settings, ThresholdKind::Warning, -1);
        let warning_increment = threshold_settings(&settings, ThresholdKind::Warning, 1);
        let critical_decrement = threshold_settings(&settings, ThresholdKind::Critical, -1);
        let critical_increment = threshold_settings(&settings, ThresholdKind::Critical, 1);

        let controls = div()
            .mt(px(24.0))
            .flex()
            .flex_col()
            .gap(px(8.0))
            .child(
                resource_setting_row(
                    theme,
                    "Background monitoring",
                    "Samples hosted workers without adding metrics to the terminal or sidebar.",
                )
                .child(
                    widgets::toggle_switch(theme, settings.monitoring_enabled)
                        .id("workers-resource-monitoring")
                        .cursor_pointer()
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.model.update(cx, |model, cx| {
                                model.set_resource_settings(monitoring_next.clone(), cx)
                            });
                        })),
                ),
            )
            .child(
                resource_setting_row(
                    theme,
                    "Exceptional alerts",
                    "Notify only when a worker crosses a configured memory threshold.",
                )
                .child(
                    widgets::toggle_switch(theme, settings.notifications_enabled)
                        .id("workers-resource-notifications")
                        .cursor_pointer()
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.model.update(cx, |model, cx| {
                                model.set_resource_settings(notifications_next.clone(), cx)
                            });
                        })),
                ),
            )
            .child(
                resource_setting_row(
                    theme,
                    "Warning per worker",
                    "Memory threshold for a discreet notification.",
                )
                .child(resource_stepper(
                    theme,
                    "workers-resource-warning",
                    settings.per_worker_warning_gib,
                    warning_decrement,
                    warning_increment,
                    cx,
                )),
            )
            .child(
                resource_setting_row(
                    theme,
                    "Critical per worker",
                    "Critical alerts require complete process attribution.",
                )
                .child(resource_stepper(
                    theme,
                    "workers-resource-critical",
                    settings.per_worker_critical_gib,
                    critical_decrement,
                    critical_increment,
                    cx,
                )),
            );

        let session_rows =
            sessions.into_iter().enumerate().map(|(index, session)| {
                let expanded = self
                    .expanded_resource_sessions
                    .contains(&session.session_id);
                let session_id = session.session_id.clone();
                let title = session_metadata
                    .get(&session.session_id)
                    .cloned()
                    .unwrap_or_else(|| session.session_id.clone());
                let processes = session.top_processes.clone();
                div()
                    .rounded(px(10.0))
                    .border_1()
                    .border_color(theme.border.opacity(0.72))
                    .bg(crate::theme::ink(0.025))
                    .child(
                        div()
                            .id(("workers-resource-session", index))
                            .min_h(px(46.0))
                            .px(px(12.0))
                            .flex()
                            .items_center()
                            .gap(px(8.0))
                            .cursor_pointer()
                            .on_click(cx.listener(move |this, _, _, cx| {
                                if !this.expanded_resource_sessions.remove(&session_id) {
                                    this.expanded_resource_sessions.insert(session_id.clone());
                                }
                                cx.notify();
                            }))
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .truncate()
                                    .text_size(px(12.5))
                                    .text_color(theme.text)
                                    .child(title),
                            )
                            .when(!session.attribution_complete, |el| {
                                el.child(
                                    div()
                                        .text_size(px(9.5))
                                        .text_color(theme.warning)
                                        .child("Partial"),
                                )
                            })
                            .child(
                                div()
                                    .text_size(px(10.5))
                                    .text_color(theme.text_muted)
                                    .child(format!(
                                        "{} · {} · {}",
                                        format_memory_bytes(session.physical_footprint_bytes),
                                        format_cpu_percent(session.cpu_percent),
                                        session.process_count
                                    )),
                            )
                            .child(
                                div()
                                    .w(px(14.0))
                                    .text_center()
                                    .text_color(theme.text_faint)
                                    .child(if expanded { "⌄" } else { "›" }),
                            ),
                    )
                    .when(expanded, |el| {
                        el.children(processes.into_iter().enumerate().map(
                            |(process_index, process)| {
                                div()
                                    .id(("workers-resource-process", index * 16 + process_index))
                                    .h(px(27.0))
                                    .pl(px(18.0))
                                    .pr(px(10.0))
                                    .flex()
                                    .items_center()
                                    .gap(px(8.0))
                                    .text_size(px(10.5))
                                    .text_color(theme.text_muted)
                                    .child(
                                        div().flex_1().truncate().font_family("monospace").child(
                                            format!("{} · PID {}", process.name, process.pid),
                                        ),
                                    )
                                    .child(format_memory_bytes(process.physical_footprint_bytes))
                                    .child(format_cpu_percent(process.cpu_percent))
                            },
                        ))
                    })
            });

        let body = div()
            .flex()
            .flex_col()
            .child(
                div()
                    .text_size(px(21.0))
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .text_color(theme.text)
                    .child("Resources"),
            )
            .child(
                div()
                    .mt(px(5.0))
                    .text_size(px(12.5))
                    .text_color(theme.text_muted)
                    .child("Background worker diagnostics, available here on demand."),
            )
            .child(div().mt(px(22.0)).flex().gap(px(10.0)).children([
                resource_metric_card(theme, "Memory", format_memory_bytes(total_memory)),
                resource_metric_card(theme, "CPU", format_cpu_percent(total_cpu)),
                resource_metric_card(theme, "Processes", total_processes.to_string()),
                resource_metric_card(theme, "Pressure", pressure_label.to_owned()),
            ]))
            .child(
                div()
                    .mt(px(9.0))
                    .text_size(px(10.5))
                    .text_color(theme.text_faint)
                    .child(match (sample_age, sampling) {
                        (Some(age), true) => format!("Updating · last sample {age}s ago"),
                        (Some(age), false) => format!("Last sample {age}s ago"),
                        (None, true) => "Sampling…".to_owned(),
                        (None, false) => "No sample yet".to_owned(),
                    }),
            )
            .when_some(last_error, |el, error| {
                el.child(
                    div()
                        .mt(px(8.0))
                        .text_size(px(10.5))
                        .text_color(theme.warning)
                        .child(error),
                )
            })
            .child(controls)
            .child(
                div()
                    .mt(px(28.0))
                    .mb(px(9.0))
                    .text_size(px(11.0))
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .text_color(theme.text_muted)
                    .child("WORKERS"),
            )
            .when(
                snapshot
                    .as_ref()
                    .is_some_and(|snapshot| snapshot.sessions.is_empty()),
                |el| {
                    el.child(
                        div()
                            .py(px(18.0))
                            .text_size(px(12.0))
                            .text_color(theme.text_faint)
                            .child("No hosted worker process is currently attached."),
                    )
                },
            )
            .children(session_rows);

        self.page_shell(
            WorkersSettingsTab::Resources,
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
            WorkersSettingsTab::Resources => self.render_resources(&theme, cx),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{format_cpu_percent, format_memory_bytes, normalize_preset_command};
    use zeron_workers_unpeel::resources::WorkersSessionResource;

    #[test]
    fn preset_add_row_uses_the_command_as_its_label_like_unpeel() {
        assert_eq!(
            normalize_preset_command("  codex --plan  "),
            Some(("codex --plan".to_owned(), "codex --plan".to_owned()))
        );
        assert_eq!(normalize_preset_command("   "), None);
    }

    #[test]
    fn resource_values_are_compact_and_stable() {
        assert_eq!(format_memory_bytes(512), "512 B");
        assert_eq!(format_memory_bytes(1024 * 1024), "1.0 MiB");
        assert_eq!(format_memory_bytes(3 * 1024 * 1024 * 1024), "3.0 GiB");
        assert_eq!(format_cpu_percent(0.0), "0.0%");
        assert_eq!(format_cpu_percent(143.24), "143.2%");
    }

    #[test]
    fn resource_sessions_sort_by_footprint_then_id() {
        let mut sessions = vec![session("b", 10), session("a", 10), session("c", 20)];
        super::sort_resource_sessions(&mut sessions);
        assert_eq!(
            sessions
                .iter()
                .map(|session| session.session_id.as_str())
                .collect::<Vec<_>>(),
            vec!["c", "a", "b"]
        );
    }

    fn session(session_id: &str, bytes: u64) -> WorkersSessionResource {
        WorkersSessionResource {
            session_id: session_id.to_owned(),
            sampled_at_unix_ms: 0,
            root_pid: Some(1),
            root_pid_started_at: Some(1),
            cpu_percent: 0.0,
            physical_footprint_bytes: bytes,
            resident_bytes: bytes,
            process_count: 1,
            attribution_complete: true,
            top_processes: Vec::new(),
        }
    }
}
