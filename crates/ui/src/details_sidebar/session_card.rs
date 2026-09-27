//! The Session card at the top of Details: context usage, the project's git
//! state, the last turn's stats and the agent's context sources, as one
//! bordered card with divided sections.

use super::*;
use crate::details_sidebar::widgets::{card_divider, card_header, card_row, details_card};

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
                this.context_sources_task = None;
                this.context_sources_pending = None;
                this.context_sources = Some(ContextSources {
                    key,
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
        let session = context
            .chat_id
            .as_deref()
            .and_then(|chat_id| self.app_state.read(cx).session_for(chat_id).cloned());
        let mut card = details_card("workspace-widget", theme);

        // ── Session ────────────────────────────────────────────────────
        if let Some(session) = session.as_ref() {
            let fraction = session.context_usage.and_then(|usage| usage.fraction());
            let fill = match fraction {
                Some(f) if f >= 0.9 => theme.danger,
                Some(f) if f >= 0.75 => theme.warning,
                _ => theme.success,
            };
            let percent: SharedString = fraction
                .map(|f| format!("{:.1}%", f * 100.0))
                .unwrap_or_else(|| "—".into())
                .into();
            card = card
                .child(card_header("Session", None, theme))
                .child(
                    div()
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
                                .child("Context"),
                        )
                        .child(
                            div()
                                .text_size(px(12.5))
                                .text_color(theme.text_muted)
                                .child(percent),
                        ),
                )
                .child(
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
            let model_ms = stats.model_ms.unwrap_or(stats.duration_ms);
            let response_rate = stats.output_rate(model_ms);
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
                    ("Model time", format_duration_ms(model_ms).into()),
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
                    (
                        "Cache",
                        stats
                            .cache_fraction()
                            .map(|f| format!("{}%", (f * 100.0).round() as u64))
                            .unwrap_or_else(|| "—".into())
                            .into(),
                    ),
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
                    card = card.child(card_row(None, label, value(text), theme));
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
                // Inline, not a nested scroller: the Details pane already
                // scrolls, and a second scroll region fought it (laggy).
                let skills = sources.skills.clone();
                card = card.child(
                    div()
                        .pb(px(4.0))
                        .children(skills.into_iter().enumerate().map(|(ix, skill)| {
                            let path = skill.relative_path.clone();
                            div()
                                .id(("context-source-skill", ix))
                                .h(px(24.0))
                                .mx(px(6.0))
                                .pl(px(29.0))
                                .pr(px(6.0))
                                .rounded(px(6.0))
                                .flex()
                                .items_center()
                                .cursor_pointer()
                                .hover(|row| row.bg(theme.element_hover.opacity(0.45)))
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.emit_open_file(&path, cx);
                                }))
                                .child(
                                    div()
                                        .min_w_0()
                                        .truncate()
                                        .text_size(px(12.0))
                                        .text_color(theme.text_muted)
                                        .child(skill.name),
                                )
                        })),
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
    let root = std::path::Path::new(project_root);
    skills
        .into_iter()
        .filter_map(|skill| {
            if skill.path.is_empty() {
                return None;
            }
            let path = std::path::Path::new(&skill.path);
            let relative = if path.is_relative() {
                path.to_path_buf()
            } else {
                path.strip_prefix(root).ok()?.to_path_buf()
            };
            Some(ProjectSkill {
                name: skill.name.into(),
                relative_path: relative.to_string_lossy().into_owned(),
            })
        })
        .collect()
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
    use super::{format_cost, format_duration_ms, format_token_count, project_skills};

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
