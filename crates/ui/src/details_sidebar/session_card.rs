//! The Session card at the top of Details: context usage, the project's git
//! state, the last turn's stats and the agent's context sources, as one
//! bordered card with divided sections.

use super::*;
use crate::details_sidebar::widgets::{
    ActionTooltip, card_divider, card_header, card_row, details_card,
};

/// Working-tree totals for the context's checkout (`WatchCheckoutDiffs`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(super) struct SessionDiffTotals {
    pub files: usize,
    pub additions: u32,
    pub deletions: u32,
}

/// Skills the chat's agent sees, keyed by what they were listed for.
pub(super) struct ContextSources {
    pub key: String,
    pub skills: Vec<ProjectSkill>,
    pub catalog: Vec<zeron_proto::skill_usage::SkillUsageCatalogEntry>,
}

/// A project skill: its name and its file, relative to the project folder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ProjectSkill {
    pub name: SharedString,
    pub relative_path: String,
}

impl DetailsSidebar {
    /// Diff totals ride the same stream as the Changes view, pinned to the
    /// context's cwd. Restarted only when the context changes.
    pub(super) fn ensure_session_diff_watch(&mut self, cx: &mut Context<Self>) {
        let Some(context) = self.sidebar.context().cloned() else {
            self.session_diff_watch = None;
            self.session_diff_key = None;
            self.session_diff = None;
            return;
        };
        if self.session_diff_key.as_ref() == Some(&context.key) {
            return;
        }
        if self.checkout_not_git {
            self.session_diff_key = Some(context.key.clone());
            self.session_diff_watch = None;
            self.session_diff = None;
            return;
        }
        let Some(engine) = self.app_state.read(cx).engine().cloned() else {
            return;
        };
        self.session_diff_key = Some(context.key.clone());
        self.session_diff = None;
        let cwd = context.cwd.to_string_lossy().into_owned();
        let device = context.target_device_id.clone();
        let key = context.key.clone();
        self.session_diff_watch = Some(cx.spawn(async move |this, cx| {
            let mut params = serde_json::Map::new();
            params.insert("cwd".into(), serde_json::Value::String(cwd.clone()));
            if let Some(device) = &device {
                params.insert(
                    "targetDeviceId".into(),
                    serde_json::Value::String(device.clone()),
                );
            }
            let Ok(mut rx) = engine
                .client()
                .subscribe(
                    methods::WATCH_CHECKOUT_DIFFS,
                    serde_json::Value::Object(params),
                )
                .await
            else {
                return;
            };
            let mut diffs = Vec::new();
            while let Some(value) = rx.recv().await {
                if !crate::changes::apply_diff_frame(&mut diffs, value) {
                    continue;
                }
                let totals = diffs
                    .iter()
                    .find(|diff| diff.cwd == cwd)
                    .or_else(|| diffs.first())
                    .map(|diff| SessionDiffTotals {
                        files: diff.files.len(),
                        additions: diff.additions,
                        deletions: diff.deletions,
                    });
                let alive = this.update(cx, |this, cx| {
                    if this.session_diff_key.as_deref() == Some(key.as_str())
                        && this.session_diff != totals
                    {
                        this.session_diff = totals;
                        cx.notify();
                    }
                });
                if alive.is_err() {
                    return;
                }
            }
        }));
    }

    /// Skills the chat's harness would load here (`ListSkills`), fetched once
    /// per chat/harness/cwd.
    pub(super) fn ensure_context_sources(&mut self, cx: &mut Context<Self>) {
        let Some(context) = self.sidebar.context().cloned() else {
            return;
        };
        let Some(chat_id) = context.chat_id.clone() else {
            return;
        };
        let state = self.app_state.read(cx);
        let Some(harness) = state
            .chats
            .iter()
            .find(|chat| chat.id == chat_id)
            .and_then(|chat| chat.config.as_ref())
            .map(|config| config.harness)
        else {
            return;
        };
        let key = format!("{harness:?}:{}", context.key);
        if self
            .context_sources
            .as_ref()
            .is_some_and(|sources| sources.key == key)
            || self.context_sources_task.is_some()
                && self.context_sources_pending.as_deref() == Some(key.as_str())
        {
            return;
        }
        let Some(engine) = state.engine().cloned() else {
            return;
        };
        let mut params = serde_json::json!({
            "harness": harness,
            "chatId": chat_id,
        });
        if let Some(device) = &context.target_device_id {
            params["targetDeviceId"] = device.clone().into();
        }
        let project_root = context.cwd.to_string_lossy().into_owned();
        self.context_sources_pending = Some(key.clone());
        self.context_sources_task = Some(cx.spawn(async move |this, cx| {
            let skills = engine
                .client()
                .call(methods::LIST_SKILLS, params)
                .await
                .ok()
                .and_then(|value| {
                    serde_json::from_value::<Option<Vec<zeron_proto::invocation::Skill>>>(value)
                        .ok()
                })
                .flatten()
                .unwrap_or_default();
            let _ = this.update(cx, |this, cx| {
                if this.context_sources_pending.as_deref() != Some(key.as_str()) {
                    return;
                }
                this.context_sources_task = None;
                this.context_sources_pending = None;
                this.context_sources = Some(ContextSources {
                    key,
                    catalog: zeron_proto::skill_usage::project_skill_catalog(
                        &skills,
                        std::path::Path::new(&project_root),
                    ),
                    skills: project_skills(skills, &project_root),
                });
                cx.notify();
            });
        }));
    }

    /// `project_extra` carries the Orchestrator's worked projects and idle
    /// recap under the Project section.
    pub(super) fn render_session_card(
        &mut self,
        context: &DetailsContext,
        branch_control: AnyElement,
        has_git: bool,
        project_extra: Option<gpui::Div>,
        theme: &Theme,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.ensure_session_diff_watch(cx);
        self.ensure_context_sources(cx);
        self.ensure_project_skill_usage(cx);
        self.ensure_context_breakdown(cx);
        let context_expanded = self.sidebar.context_disclosure_expanded("expanded");
        let session = context
            .chat_id
            .as_deref()
            .and_then(|chat_id| self.app_state.read(cx).session_for(chat_id).cloned());
        let mut card = details_card("workspace-widget", theme);

        // ── Session ────────────────────────────────────────────────────
        if context.chat_id.is_some() {
            let fraction = if context_expanded {
                self.expanded_context_fraction()
            } else {
                session
                    .as_ref()
                    .and_then(|session| session.context_usage)
                    .and_then(|usage| usage.fraction())
            };
            let fill = match fraction {
                Some(f) if f >= 0.9 => theme.danger,
                Some(f) if f >= 0.75 => theme.warning,
                _ => theme.success,
            };
            let percent: SharedString = fraction
                .map(|f| format!("{:.1}%", f * 100.0))
                .unwrap_or_else(|| "—".into())
                .into();
            let chat_button = |id: &'static str, glyph: &'static str, label: &'static str| {
                div()
                    .id(id)
                    .debug_selector(move || id.into())
                    .size(px(22.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(5.0))
                    .cursor_pointer()
                    .hover(|style| style.bg(crate::theme::ink(0.08)))
                    .aria_label(label)
                    .tooltip(move |_, cx| {
                        cx.new(|_| ActionTooltip {
                            label: label.into(),
                        })
                        .into()
                    })
                    .child(
                        icons::icon(glyph)
                            .size(px(14.0))
                            .text_color(theme.text_muted),
                    )
            };
            let chat_actions = div()
                .flex()
                .items_center()
                .gap(px(2.0))
                .child(
                    chat_button("details-new-side-chat", icons::PLUS, "New side chat").on_click(
                        cx.listener(|_, _, _, cx| cx.emit(DetailsSidebarEvent::NewSideChat)),
                    ),
                )
                .child(
                    chat_button("details-fork-chat", icons::FORK, "Fork chat").on_click(
                        cx.listener(|_, _, _, cx| cx.emit(DetailsSidebarEvent::ForkChat)),
                    ),
                )
                .into_any_element();
            card = card
                .child(card_header(
                    "Session",
                    session.as_ref().map(|_| chat_actions),
                    theme,
                ))
                .child(
                    div()
                        .id("context-disclosure")
                        .debug_selector(|| "context-disclosure".into())
                        .tab_index(0)
                        .role(gpui::Role::Button)
                        .aria_label("Context budget")
                        .aria_expanded(context_expanded)
                        .cursor_pointer()
                        .hover(|style| style.bg(theme.element_hover))
                        .focus_visible(|style| style.bg(theme.element_active))
                        .on_click(cx.listener(|this, _, _, cx| {
                            cx.stop_propagation();
                            this.toggle_context_breakdown(cx);
                        }))
                        .h(px(28.0))
                        .px(px(12.0))
                        .flex()
                        .items_center()
                        .gap(px(8.0))
                        .child(div().size(px(15.0)).flex_none().child(
                            crate::loaders::context_progress_ring(
                                fraction.unwrap_or(0.0) as f32,
                                15.0,
                                crate::theme::ink(0.14),
                                fill,
                            ),
                        ))
                        .child(
                            div()
                                .flex_1()
                                .text_size(px(12.5))
                                .text_color(theme.text)
                                .flex()
                                .items_center()
                                .gap(px(4.0))
                                .child("Context")
                                .child(
                                    icons::icon(if context_expanded {
                                        icons::ALT_ARROW_DOWN
                                    } else {
                                        icons::ALT_ARROW_RIGHT
                                    })
                                    .size(px(12.0))
                                    .text_color(theme.text_muted),
                                ),
                        )
                        .child(
                            div()
                                .text_size(px(12.5))
                                .text_color(theme.text_muted)
                                .child(percent),
                        ),
                )
                .when(!context_expanded, |card| {
                    card.child(
                        div()
                            .mx(px(12.0))
                            .mb(px(6.0))
                            .h(px(4.0))
                            .rounded_full()
                            .bg(crate::theme::ink(0.10))
                            .child(div().h_full().rounded_full().bg(fill).w(gpui::relative(
                                fraction.unwrap_or(0.0).clamp(0.0, 1.0) as f32,
                            ))),
                    )
                })
                .when(context_expanded, |card| {
                    card.child(self.render_context_breakdown(theme, cx))
                })
                .child(card_divider(theme));
        }

        // ── Project ────────────────────────────────────────────────────
        let folder: SharedString = context
            .cwd
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("Workspace")
            .to_owned()
            .into();
        card = card.child(card_header(
            "Project",
            Some(folder.into_any_element()),
            theme,
        ));
        let (device_name, device_online): (SharedString, bool) = {
            let state = self.app_state.read(cx);
            let device_id = context
                .target_device_id
                .as_deref()
                .or(state.local_device_id.as_deref());
            let name = device_id
                .and_then(|device_id| state.device_name(device_id))
                .unwrap_or("Unknown device")
                .to_owned()
                .into();
            // Mesma presenca de Settings -> Devices (heartbeat do engine).
            let online = device_id.is_none_or(|id| state.device_online(id, chrono::Utc::now()));
            (name, online)
        };
        let device_tooltip: SharedString = format!(
            "{device_name} · {}",
            if device_online { "Online" } else { "Offline" }
        )
        .into();
        // Presenca como glifo, nao palavra: ponto verde online, wifi-off
        // amarelo offline (o mesmo dos pickers de space).
        let device_presence = if device_online {
            div()
                .flex_none()
                .size(px(6.0))
                .rounded_full()
                .bg(theme.success)
                .into_any_element()
        } else {
            icons::icon(icons::WIFI_OFF)
                .size(px(12.0))
                .flex_none()
                .text_color(theme.warning.opacity(0.8))
                .into_any_element()
        };
        card = card.child(
            div()
                .h(px(28.0))
                .px(px(12.0))
                .flex()
                .items_center()
                .gap(px(8.0))
                .child(
                    icons::icon(icons::MONITOR)
                        .size(px(15.0))
                        .flex_none()
                        .text_color(theme.text_muted),
                )
                .child(
                    div()
                        .flex_none()
                        .text_size(px(12.5))
                        .text_color(theme.text)
                        .child("Device"),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .flex()
                        .justify_end()
                        .items_center()
                        .gap(px(6.0))
                        .child(device_presence)
                        .child(
                            div()
                                .id("session-device-name")
                                .min_w_0()
                                .truncate()
                                .text_size(px(12.5))
                                .text_color(theme.text_muted)
                                .text_right()
                                .aria_label(device_name.clone())
                                .tooltip(move |_, cx| {
                                    cx.new(|_| ActionTooltip {
                                        label: device_tooltip.clone(),
                                    })
                                    .into()
                                })
                                .child(device_name),
                        ),
                ),
        );
        if has_git {
            let ahead_behind: Option<SharedString> =
                self.checkout_status.as_ref().and_then(|status| {
                    let mut parts = Vec::new();
                    if status.ahead > 0 {
                        parts.push(format!("↑{}", status.ahead));
                    }
                    if status.behind > 0 {
                        parts.push(format!("↓{}", status.behind));
                    }
                    (!parts.is_empty()).then(|| parts.join(" ").into())
                });
            card = card.child(card_row(
                Some(icons::GIT_BRANCH),
                div().flex().items_center().child(branch_control),
                ahead_behind.map(IntoElement::into_any_element),
                theme,
            ));
            if let Some(diff) = self.session_diff {
                let label: SharedString = match diff.files {
                    0 => "No changes".into(),
                    1 => "1 file changed".into(),
                    n => format!("{n} files changed").into(),
                };
                let totals = (diff.files > 0).then(|| {
                    div()
                        .flex()
                        .items_center()
                        .gap(px(4.0))
                        .child(
                            div()
                                .text_color(theme.success)
                                .child(format!("+{}", diff.additions)),
                        )
                        .child(div().text_color(theme.text_faint).child("/"))
                        .child(
                            div()
                                .text_color(theme.danger)
                                .child(format!("−{}", diff.deletions)),
                        )
                        .into_any_element()
                });
                card = card.child(card_row(Some(icons::DIFF), label, totals, theme));
            }
        }
        if let Some(extra) = project_extra {
            card = card.child(extra);
        }

        // ── Turn stats ─────────────────────────────────────────────────
        if let Some(stats) = session.as_ref().and_then(|session| session.turn_stats) {
            card = card.child(card_divider(theme));
            let model_ms = stats.model_ms;
            let response_rate = model_ms.and_then(|ms| stats.output_rate(ms));
            let rate = |rate: Option<f64>| -> SharedString {
                rate.map(|r| format!("~{} tok/s", r.round() as u64))
                    .unwrap_or_else(|| "—".into())
                    .into()
            };
            let expanded = !self.turn_stats_collapsed;
            let header = div()
                .id("turn-stats-header")
                .h(px(30.0))
                .px(px(12.0))
                .flex()
                .items_center()
                .gap(px(8.0))
                .cursor_pointer()
                .on_click(cx.listener(|this, _, _, cx| {
                    this.turn_stats_collapsed = !this.turn_stats_collapsed;
                    cx.notify();
                }))
                .child(
                    icons::icon(icons::DETAILS_GAUGE)
                        .size(px(15.0))
                        .text_color(theme.text_muted),
                )
                .child(
                    div()
                        .text_size(px(13.0))
                        .font_weight(gpui::FontWeight::SEMIBOLD)
                        .text_color(theme.text)
                        .child("Turn stats"),
                )
                .child(
                    icons::icon(if expanded {
                        icons::ALT_ARROW_DOWN
                    } else {
                        icons::ALT_ARROW_RIGHT
                    })
                    .size(px(12.0))
                    .text_color(theme.text_muted),
                )
                .child(div().flex_1())
                .child(
                    div()
                        .text_size(px(12.5))
                        .text_color(theme.text_muted)
                        .debug_selector(move || {
                            format!("turn-stats-response:{}", rate(response_rate))
                        })
                        .child(rate(response_rate)),
                );
            card = card.child(header);
            if expanded {
                let prompt_tokens = stats.input_tokens
                    + stats.cache_read_tokens.unwrap_or(0)
                    + stats.cache_write_tokens.unwrap_or(0);
                let value = |text: SharedString| Some(text.into_any_element());
                let rows: Vec<(&str, SharedString)> = vec![
                    ("Response", rate(response_rate)),
                    ("Whole turn", rate(stats.output_rate(stats.duration_ms))),
                    (
                        "Model time",
                        model_ms
                            .map(format_duration_ms)
                            .unwrap_or_else(|| "—".into())
                            .into(),
                    ),
                    (
                        "Tool time",
                        stats
                            .tool_ms
                            .map(format_duration_ms)
                            .unwrap_or_else(|| "—".into())
                            .into(),
                    ),
                    ("Steps", stats.steps.to_string().into()),
                    (
                        "Tokens",
                        format!(
                            "{} ↑ · {} ↓",
                            format_token_count(prompt_tokens),
                            format_token_count(stats.output_tokens)
                        )
                        .into(),
                    ),
                    ("Cache", format_cache(&stats).into()),
                    (
                        "Cost",
                        stats
                            .cost_usd
                            .map(format_cost)
                            .unwrap_or_else(|| "—".into())
                            .into(),
                    ),
                ];
                for (label, text) in rows {
                    let selector = format!("turn-stat-{label}:{text}");
                    card = card.child(
                        card_row(None, label, value(text), theme)
                            .debug_selector(move || selector.clone()),
                    );
                }
            }
        }

        // ── Context sources ────────────────────────────────────────────
        if let Some(sources) = self
            .context_sources
            .as_ref()
            .filter(|sources| sources.key.ends_with(&format!(":{}", context.key)))
        {
            let count = sources.skills.len();
            let expanded = self.context_sources_expanded && count > 0;
            card = card.child(card_divider(theme)).child(
                div()
                    .id("context-sources-header")
                    .debug_selector(|| "context-sources-header".into())
                    .h(px(30.0))
                    .px(px(12.0))
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .when(count > 0, |row| {
                        row.cursor_pointer().on_click(cx.listener(|this, _, _, cx| {
                            this.context_sources_expanded = !this.context_sources_expanded;
                            cx.notify();
                        }))
                    })
                    .child(
                        icons::icon(icons::MAGIC_STICK_3)
                            .size(px(15.0))
                            .text_color(theme.text_muted),
                    )
                    .child(
                        div()
                            .text_size(px(13.0))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_color(theme.text)
                            .child("Context sources"),
                    )
                    .when(count > 0, |row| {
                        row.child(
                            icons::icon(if expanded {
                                icons::ALT_ARROW_DOWN
                            } else {
                                icons::ALT_ARROW_RIGHT
                            })
                            .size(px(12.0))
                            .text_color(theme.text_muted),
                        )
                    })
                    .child(div().flex_1())
                    .child(
                        div()
                            .text_size(px(12.5))
                            .text_color(theme.text_muted)
                            .child(match count {
                                0 => "No project skills".to_owned(),
                                1 => "1 skill".to_owned(),
                                n => format!("{n} skills"),
                            }),
                    ),
            );
            if expanded {
                // Capped box with its own scroll, virtualized: only the
                // visible rows render, so scrolling stays smooth with dozens
                // of skills.
                const ROW: f32 = 24.0;
                const MAX_ROWS: usize = 7;
                let skills = sources.skills.clone();
                let usage = self.project_skill_usage.clone();
                let entity = cx.entity();
                let muted = theme.text_muted;
                let hover = theme.element_hover.opacity(0.45);
                let height = ROW * count.min(MAX_ROWS) as f32 + 4.0;
                card = card.child(
                    div()
                        .debug_selector(|| "project-skills-viewport".into())
                        .h(px(height))
                        .pb(px(4.0))
                        .child(
                            gpui::uniform_list(
                                "context-sources-list",
                                count,
                                move |range, _window, _app| {
                                    range
                                        .filter_map(|ix| {
                                            let skill = skills.get(ix)?.clone();
                                            let entity = entity.clone();
                                            let path = skill.relative_path.clone();
                                            let (label, tooltip) = usage.presentation(&path);
                                            Some(
                                                div()
                                                    .id(("context-source-skill", ix))
                                                    .debug_selector(move || {
                                                        format!("project-skill-row-{ix}")
                                                    })
                                                    .h(px(ROW))
                                                    .mx(px(6.0))
                                                    .pl(px(29.0))
                                                    .pr(px(6.0))
                                                    .rounded(px(6.0))
                                                    .flex()
                                                    .items_center()
                                                    .gap(px(8.0))
                                                    .cursor_pointer()
                                                    .hover(move |row| row.bg(hover))
                                                    .on_click(move |_, _, app| {
                                                        entity.update(app, |this, cx| {
                                                            this.emit_open_file(&path, cx)
                                                        });
                                                    })
                                                    .child(
                                                        div()
                                                            .debug_selector(move || {
                                                                format!("project-skill-name-{ix}")
                                                            })
                                                            .flex_1()
                                                            .min_w_0()
                                                            .truncate()
                                                            .text_size(px(12.0))
                                                            .text_color(muted)
                                                            .child(skill.name),
                                                    )
                                                    .child(
                                                        div()
                                                            .id(("project-skill-count", ix))
                                                            .debug_selector(move || {
                                                                format!("project-skill-count-{ix}")
                                                            })
                                                            .flex_shrink_0()
                                                            .text_size(px(12.0))
                                                            .text_color(muted)
                                                            .child(label)
                                                            .tooltip(move |_, cx| {
                                                                cx.new(|_| ActionTooltip {
                                                                    label: tooltip.clone().into(),
                                                                })
                                                                .into()
                                                            }),
                                                    )
                                                    .into_any_element(),
                                            )
                                        })
                                        .collect::<Vec<_>>()
                                },
                            )
                            .size_full()
                            .track_scroll(&self.context_sources_scroll),
                        ),
                );
            }
        }
        card.into_any_element()
    }
}

/// Only skills that live in the project: `ListSkills` also returns the
/// user-global ones (`~/.claude/skills`, `~/.codex/skills`, …), which are not
/// this project's context. A relative path is already project-relative.
pub(super) fn project_skills(
    skills: Vec<zeron_proto::invocation::Skill>,
    project_root: &str,
) -> Vec<ProjectSkill> {
    skills
        .into_iter()
        .filter_map(|skill| {
            let relative_path =
                zeron_proto::skill_usage::project_skill_relative_path(&skill.path, project_root)?;
            Some(ProjectSkill {
                name: skill.name.into(),
                relative_path,
            })
        })
        .collect()
}

/// Prompt tokens served from cache, with their share of the prompt:
/// `781.2K (99.8%)`, or `read · write` when the turn also wrote cache.
pub(super) fn format_cache(stats: &zeron_proto::TurnStats) -> String {
    let Some(read) = stats.cache_read_tokens else {
        return "—".to_owned();
    };
    let share = stats
        .cache_fraction()
        .map(|f| format!(" ({:.1}%)", f * 100.0))
        .unwrap_or_default();
    match stats.cache_write_tokens.filter(|write| *write > 0) {
        Some(write) => format!(
            "{} read · {} write{share}",
            format_token_count(read),
            format_token_count(write)
        ),
        None => format!("{}{share}", format_token_count(read)),
    }
}

/// `12m56s`, `1h02m`, `4.2s`.
pub(super) fn format_duration_ms(ms: u64) -> String {
    let seconds = ms / 1000;
    if seconds >= 3600 {
        format!("{}h{:02}m", seconds / 3600, (seconds % 3600) / 60)
    } else if seconds >= 60 {
        format!("{}m{:02}s", seconds / 60, seconds % 60)
    } else {
        format!("{:.1}s", ms as f64 / 1000.0)
    }
}

/// `1.7M`, `16.1K`, `950`.
pub(super) fn format_token_count(tokens: u64) -> String {
    match tokens {
        n if n >= 1_000_000 => format!("{:.1}M", n as f64 / 1_000_000.0),
        n if n >= 1_000 => format!("{:.1}K", n as f64 / 1_000.0),
        n => n.to_string(),
    }
}

/// `$0`, `$0.42`, `$12.30`.
pub(super) fn format_cost(usd: f64) -> String {
    if usd <= 0.0 {
        "$0".to_owned()
    } else if usd < 0.01 {
        "<$0.01".to_owned()
    } else {
        format!("${usd:.2}")
    }
}

#[cfg(test)]
mod tests {
    use super::{
        format_cache, format_cost, format_duration_ms, format_token_count, project_skills,
    };

    #[gpui::test]
    fn turn_stats_keep_missing_model_time_unavailable_in_the_rendered_card(
        cx: &mut gpui::TestAppContext,
    ) {
        use super::super::*;
        use gpui::{AppContext, Render, Window};
        let data = tempfile::tempdir().unwrap();
        cx.update(|cx| {
            gpui_base::init(cx);
            crate::settings::init(crate::settings::UiSettings::default(), data.path(), cx);
            cx.set_global(Theme::dark());
        });
        let state = cx.new(|_| AppState::new());
        state.update(cx, |state, _| {
            state.local_device_id = Some("host".into());
            state.sessions = vec![
                serde_json::from_value(serde_json::json!({
                    "chatId": "chat", "deviceId": "host", "status": "idle",
                    "updatedAt": chrono::Utc::now()
                }))
                .unwrap(),
            ];
        });
        let workers = cx.new(|cx| WorkersModel::detached(state.clone(), cx));
        let composer = cx.new(|cx| Composer::new(state.clone(), cx));
        let pickers = composer.read_with(cx, |c, _| c.pickers().clone());
        let sidebar = cx.new(|cx| {
            DetailsSidebar::new(
                state.clone(),
                workers,
                DetailsSidebarPreferences::default(),
                pickers,
                composer,
                cx,
            )
        });
        sidebar.update(cx, |s, _| {
            s.sidebar.set_context(Some(DetailsContext {
                key: "fixture:chat".into(),
                cwd: data.path().into(),
                branch: None,
                chat_id: Some("chat".into()),
                target_device_id: Some("host".into()),
                mode: crate::details_sidebar::context::DetailsMode::Orchestrator,
            }));
            s.checkout_not_git = true;
            s.turn_stats_collapsed = false;
        });
        struct Probe {
            sidebar: Entity<DetailsSidebar>,
            _observe: Subscription,
        }
        impl Render for Probe {
            fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
                let theme = Theme::of(cx).clone();
                let card = self.sidebar.update(cx, |s, cx| {
                    let context = s.sidebar.context().unwrap().clone();
                    s.render_session_card(
                        &context,
                        div().into_any_element(),
                        false,
                        None,
                        &theme,
                        cx,
                    )
                });
                div().w(px(300.0)).h(px(1000.0)).child(card)
            }
        }
        let (_, window) = cx.add_window_view(|_, cx| Probe {
            sidebar: sidebar.clone(),
            _observe: cx.observe(&sidebar, |_, _, cx| cx.notify()),
        });
        for (model_ms, duration_ms, selectors) in [
            (
                None,
                10_000,
                [
                    "turn-stats-response:—",
                    "turn-stat-Response:—",
                    "turn-stat-Model time:—",
                    "turn-stat-Whole turn:~50 tok/s",
                ],
            ),
            (
                Some(0),
                10_000,
                [
                    "turn-stats-response:—",
                    "turn-stat-Response:—",
                    "turn-stat-Model time:0.0s",
                    "turn-stat-Whole turn:~50 tok/s",
                ],
            ),
            (
                Some(2_000),
                10_000,
                [
                    "turn-stats-response:~250 tok/s",
                    "turn-stat-Response:~250 tok/s",
                    "turn-stat-Model time:2.0s",
                    "turn-stat-Whole turn:~50 tok/s",
                ],
            ),
            (
                None,
                0,
                [
                    "turn-stats-response:—",
                    "turn-stat-Response:—",
                    "turn-stat-Model time:—",
                    "turn-stat-Whole turn:—",
                ],
            ),
        ] {
            state.update(window, |state, _| {
                state.sessions[0].turn_stats = Some(zeron_proto::TurnStats {
                    model_ms,
                    duration_ms,
                    output_tokens: 500,
                    ..Default::default()
                });
            });
            sidebar.update(window, |_, cx| cx.notify());
            window.update(|w, cx| w.draw(cx).clear());
            for selector in selectors {
                assert!(
                    window.debug_bounds(selector).is_some(),
                    "missing rendered {selector} for model_ms={model_ms:?}, duration_ms={duration_ms}"
                );
            }
        }
    }

    #[gpui::test]
    fn context_disclosure_is_inline_keyboard_accessible_and_scoped(cx: &mut gpui::TestAppContext) {
        use super::super::*;
        use gpui::{
            AppContext, InputEvent as _, KeyDownEvent, KeyUpEvent, Keystroke, Modifiers, Render,
            Window,
        };
        let data = tempfile::tempdir().unwrap();
        cx.update(|cx| {
            gpui_base::init(cx);
            crate::settings::init(crate::settings::UiSettings::default(), data.path(), cx);
            cx.set_global(Theme::dark());
        });
        let state = cx.new(|_| AppState::new());
        state.update(cx, |state,_| {
            state.local_device_id=Some("host".into());
            state.sessions=vec![serde_json::from_value(serde_json::json!({"chatId":"chat","deviceId":"host","status":"idle","updatedAt":chrono::Utc::now(),"contextUsage":{"tokens":828300,"contextWindow":1000000}})).unwrap()];
        });
        let workers = cx.new(|cx| WorkersModel::detached(state.clone(), cx));
        let composer = cx.new(|cx| Composer::new(state.clone(), cx));
        let pickers = composer.read_with(cx, |c, _| c.pickers().clone());
        let sidebar = cx.new(|cx| {
            DetailsSidebar::new(
                state.clone(),
                workers,
                DetailsSidebarPreferences::default(),
                pickers,
                composer,
                cx,
            )
        });
        let context = DetailsContext {
            key: "fixture:chat".into(),
            cwd: data.path().into(),
            branch: None,
            chat_id: Some("chat".into()),
            target_device_id: Some("host".into()),
            mode: crate::details_sidebar::context::DetailsMode::Orchestrator,
        };
        sidebar.update(cx, |s, _| {
            s.sidebar.set_context(Some(context.clone()));
            s.checkout_not_git = true;
        });
        struct Probe {
            sidebar: Entity<DetailsSidebar>,
            _observe: Subscription,
        }
        impl Render for Probe {
            fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
                let theme = Theme::of(cx).clone();
                let card = self.sidebar.update(cx, |s, cx| {
                    let context = s.sidebar.context().unwrap().clone();
                    s.render_session_card(
                        &context,
                        div().into_any_element(),
                        false,
                        None,
                        &theme,
                        cx,
                    )
                });
                div().w(px(300.0)).h(px(1000.0)).child(card)
            }
        }
        let (_, window) = cx.add_window_view(|_, cx| Probe {
            sidebar: sidebar.clone(),
            _observe: cx.observe(&sidebar, |_, _, cx| cx.notify()),
        });
        window.update(|w, cx| w.draw(cx).clear());
        let toggle = window
            .debug_bounds("context-disclosure")
            .expect("Context has an inline disclosure");
        assert!(window.debug_bounds("context-breakdown").is_none());
        window.simulate_click(toggle.center(), Modifiers::default());
        window.update(|w, cx| w.draw(cx).clear());
        assert!(window.debug_bounds("context-breakdown").is_some());
        let preferences = sidebar.read_with(window, |s, _| s.preferences());
        assert!(
            preferences.expanded["fixture:chat"]
                .iter()
                .any(|m| m == ":context:expanded")
        );
        sidebar.update(window, |s, cx| {
            let mut other = context.clone();
            other.key = "fixture:other".into();
            other.chat_id = Some("other".into());
            s.set_context(Some(other), cx);
        });
        window.update(|w, cx| w.draw(cx).clear());
        assert!(window.debug_bounds("context-breakdown").is_none());
        sidebar.update(window, |s, cx| s.set_context(Some(context), cx));
        window.update(|w, cx| w.draw(cx).clear());
        assert!(window.debug_bounds("context-breakdown").is_some());
        // Keyboard activates the same disclosure, without toggling sibling sections.
        window.simulate_click(toggle.center(), Modifiers::default());
        window.update(|w, cx| w.draw(cx).clear());
        assert!(window.debug_bounds("context-breakdown").is_none());
        let press = |window: &mut gpui::VisualTestContext, key: &str| {
            let keystroke = Keystroke::parse(key).unwrap();
            window.update(|w, cx| {
                w.dispatch_event(
                    KeyDownEvent {
                        keystroke: keystroke.clone(),
                        is_held: false,
                        prefer_character_input: false,
                    }
                    .to_platform_input(),
                    cx,
                );
            });
            window.run_until_parked();
            window.update(|w, cx| w.draw(cx).clear());
            window.update(|w, cx| {
                w.dispatch_event(KeyUpEvent { keystroke }.to_platform_input(), cx);
            });
            window.run_until_parked();
        };
        press(window, "enter");
        window.update(|w, cx| w.draw(cx).clear());
        assert!(window.debug_bounds("context-breakdown").is_some());
        press(window, "space");
        window.update(|w, cx| w.draw(cx).clear());
        assert!(window.debug_bounds("context-breakdown").is_none());
        assert!(sidebar.read_with(window, |s, _| s.turn_stats_collapsed));
    }

    #[test]
    fn cache_reads_as_token_counts_with_their_share() {
        let stats = |read, write| zeron_proto::TurnStats {
            input_tokens: 1_600,
            cache_read_tokens: read,
            cache_write_tokens: write,
            ..Default::default()
        };
        assert_eq!(format_cache(&stats(Some(781_200), None)), "781.2K (99.8%)");
        assert_eq!(
            format_cache(&stats(Some(781_200), Some(2_000))),
            "781.2K read · 2.0K write (99.5%)"
        );
        assert_eq!(format_cache(&stats(None, None)), "—");
    }

    #[test]
    fn context_sources_count_only_project_skills() {
        let skill = |name: &str, path: &str| zeron_proto::invocation::Skill {
            name: name.into(),
            path: path.into(),
            description: String::new(),
            enabled: true,
            command: None,
        };
        let names = project_skills(
            vec![
                skill("local", "/work/app/.claude/skills/local/SKILL.md"),
                skill("global", "/Users/me/.claude/skills/global/SKILL.md"),
                skill("sibling", "/work/app-other/.claude/skills/x/SKILL.md"),
                skill("relative", ".agents/skills/rel/SKILL.md"),
            ],
            "/work/app",
        );
        let names: Vec<_> = names.iter().map(|skill| skill.name.as_ref()).collect();
        assert_eq!(names, ["local", "relative"]);
        let paths: Vec<_> = project_skills(
            vec![skill("local", "/work/app/.claude/skills/local/SKILL.md")],
            "/work/app",
        )
        .into_iter()
        .map(|skill| skill.relative_path)
        .collect();
        assert_eq!(paths, [".claude/skills/local/SKILL.md"]);
    }

    #[test]
    fn turn_stat_values_read_like_the_reference_card() {
        assert_eq!(format_duration_ms(776_000), "12m56s");
        assert_eq!(format_duration_ms(4_200), "4.2s");
        assert_eq!(format_duration_ms(3_720_000), "1h02m");
        assert_eq!(format_token_count(1_700_000), "1.7M");
        assert_eq!(format_token_count(16_100), "16.1K");
        assert_eq!(format_token_count(950), "950");
        assert_eq!(format_cost(0.0), "$0");
        assert_eq!(format_cost(0.004), "<$0.01");
        assert_eq!(format_cost(1.234), "$1.23");
    }
}
