//! Inline native Context budget and selection-bound, event-driven inspection.
use super::session_card::format_token_count;
use super::*;
use crate::details_sidebar::widgets::ActionTooltip;
use std::sync::Arc;
use zeron_proto::{
    ContextAvailability as Availability, ContextBreakdown, ContextCategory,
    ContextCategoryKind as Kind,
};

#[derive(Clone, PartialEq)]
pub(super) struct Selection {
    context: String,
    chat: String,
    host: String,
    config: Option<zeron_proto::ChatConfig>,
    completed: Option<String>,
    settled_usage: Option<zeron_proto::ContextUsage>,
    lifecycle: Option<(
        zeron_proto::SessionStatus,
        Option<chrono::DateTime<chrono::Utc>>,
        Option<chrono::DateTime<chrono::Utc>>,
    )>,
    compacting: bool,
    available: bool,
}

fn settled_usage(session: &zeron_proto::Session) -> Option<zeron_proto::ContextUsage> {
    matches!(
        session.status,
        zeron_proto::SessionStatus::Idle | zeron_proto::SessionStatus::Errored
    )
    .then_some(session.context_usage)
    .flatten()
}

#[derive(Default)]
pub(super) struct PanelState {
    key: Option<Selection>,
    generation: u64,
    loading: bool,
    snapshot: Option<Arc<ContextBreakdown>>,
    failure: Option<(Availability, SharedString)>,
    mcp_scroll: gpui::UniformListScrollHandle,
    memory_scroll: gpui::UniformListScrollHandle,
}
impl PanelState {
    fn fraction(&self) -> Option<f64> {
        let report = self.snapshot.as_ref()?;
        Some(report.used_tokens? as f64 / report.capacity.filter(|n| *n > 0)? as f64)
    }
    fn select(&mut self, key: Selection) -> bool {
        if self.key.as_ref() == Some(&key) {
            return false;
        }
        let same = self
            .snapshot
            .as_ref()
            .is_some_and(|report| report.chat_id == key.chat && report.host_device_id == key.host);
        if same {
            if let Some(report) = self.snapshot.as_mut() {
                Arc::make_mut(report).stale = true;
            }
        } else {
            self.snapshot = None;
            self.mcp_scroll = Default::default();
            self.memory_scroll = Default::default();
        }
        self.key = Some(key);
        self.generation = self.generation.wrapping_add(1);
        self.loading = true;
        self.failure = None;
        true
    }
    fn hide(&mut self) {
        if self.key.take().is_some() || self.loading {
            self.generation = self.generation.wrapping_add(1);
        }
        self.loading = false;
    }
    fn receive(
        &mut self,
        generation: u64,
        key: &Selection,
        result: Result<ContextBreakdown, zeron_rpc::RpcError>,
    ) -> bool {
        if self.generation != generation || self.key.as_ref() != Some(key) {
            return false;
        }
        self.loading = false;
        match result {
            Ok(report) if report.chat_id != key.chat || report.host_device_id != key.host => {
                self.failure = Some((
                    Availability::Error,
                    "Context response belongs to another Chat or host".into(),
                ));
            }
            Ok(report) if report.schema_version != 1 => {
                self.failure = Some((
                    Availability::Unsupported,
                    "This host uses an unsupported context schema".into(),
                ));
            }
            Ok(report)
                if matches!(
                    report.availability,
                    Availability::Error | Availability::NotApplicable | Availability::Unsupported
                ) && self.snapshot.is_some() =>
            {
                self.failure = Some((
                    report.availability,
                    report
                        .reason
                        .unwrap_or_else(|| "Native context is unavailable".into())
                        .into(),
                ));
            }
            Ok(report) => {
                self.snapshot = Some(Arc::new(report));
                self.failure = None;
            }
            Err(zeron_rpc::RpcError::UnknownMethod(_)) => {
                self.failure = Some((
                    Availability::Unsupported,
                    "This host does not support context inspection".into(),
                ));
            }
            Err(_) => {
                self.failure = Some((
                    Availability::Error,
                    "Context inspection failed; refresh to retry".into(),
                ));
            }
        }
        if self.failure.is_some() {
            if let Some(report) = self.snapshot.as_mut() {
                Arc::make_mut(report).stale = true;
            }
        }
        true
    }
}

impl DetailsSidebarState {
    pub(super) fn context_disclosure_expanded(&self, section: &str) -> bool {
        let marker = format!(":context:{section}");
        self.context
            .as_ref()
            .and_then(|context| self.preferences.expanded.get(&context.key))
            .is_some_and(|markers| markers.contains(&marker))
    }
    pub(super) fn toggle_context_disclosure(&mut self, section: &str) {
        let Some(context) = &self.context else {
            return;
        };
        let marker = format!(":context:{section}");
        let markers = self
            .preferences
            .expanded
            .entry(context.key.clone())
            .or_default();
        if let Some(index) = markers.iter().position(|m| *m == marker) {
            markers.remove(index);
        } else {
            markers.push(marker);
            markers.sort();
        }
    }
}

impl DetailsSidebar {
    pub(super) fn expanded_context_fraction(&self) -> Option<f64> {
        self.context_breakdown.fraction()
    }
    fn context_selection(&self, cx: &App) -> Option<Selection> {
        let context = self.sidebar.context()?;
        let chat = context.chat_id.as_ref()?;
        let state = self.app_state.read(cx);
        let row = state.chats.iter().find(|row| row.id == *chat);
        let host = context
            .target_device_id
            .clone()
            .or_else(|| row.map(|row| row.device_id.clone()))?;
        let local = state.local_device_id.as_deref() == Some(&host);
        let available = state.engine().is_some()
            && (local
                || (state.device_online(&host, chrono::Utc::now())
                    && !matches!(
                        state.connectivity.state,
                        zeron_proto::ConnectivityState::Offline
                            | zeron_proto::ConnectivityState::Reconnecting
                    )));
        Some(Selection {
            context: context.key.clone(),
            chat: chat.clone(),
            host,
            config: row.and_then(|row| row.config.clone()),
            completed: state
                .session_for(chat)
                .and_then(|session| session.last_completed_turn.clone()),
            settled_usage: state.session_for(chat).and_then(settled_usage),
            lifecycle: state.session_for(chat).map(|session| {
                (
                    session.status,
                    session.started_at,
                    session
                        .last_turn_outcome
                        .as_ref()
                        .map(|outcome| outcome.finished_at),
                )
            }),
            compacting: state.is_compacting(chat),
            available,
        })
    }
    pub(super) fn hide_context_breakdown(&mut self) {
        self.context_breakdown_task = None;
        self.context_breakdown.hide();
        self.context_breakdown_connection = None;
    }
    pub(super) fn ensure_context_breakdown(&mut self, cx: &mut Context<Self>) {
        if !self.sidebar.context_disclosure_expanded("expanded") {
            self.hide_context_breakdown();
            return;
        }
        let Some(key) = self.context_selection(cx) else {
            self.hide_context_breakdown();
            return;
        };
        let engine = self.app_state.read(cx).engine().cloned();
        let changed = match (&self.context_breakdown_connection, &engine) {
            (Some(a), Some(b)) => !a.same_connection(b),
            (None, None) => false,
            _ => true,
        };
        if changed {
            self.context_breakdown.hide();
        }
        if !self.context_breakdown.select(key.clone()) {
            return;
        }
        self.context_breakdown_task = None;
        self.context_breakdown_connection = engine.clone();
        if !key.available || engine.is_none() {
            self.context_breakdown.loading = false;
            self.context_breakdown.failure = Some((
                Availability::NotApplicable,
                "The selected host is unavailable".into(),
            ));
            return;
        }
        let engine = engine.unwrap();
        let generation = self.context_breakdown.generation;
        self.context_breakdown_task = Some(cx.spawn(async move |this, cx| {
            let params = zeron_proto::GetChatContextBreakdownParams {
                chat_id: key.chat.clone(),
                target_device_id: Some(key.host.clone()),
            };
            let call = engine.client().call_as::<ContextBreakdown>(
                methods::GET_CHAT_CONTEXT_BREAKDOWN,
                serde_json::to_value(params).expect("context params serialize"),
            );
            let result = match futures::future::select(
                Box::pin(call),
                Box::pin(
                    cx.background_executor()
                        .timer(std::time::Duration::from_secs(12)),
                ),
            )
            .await
            {
                futures::future::Either::Left((result, _)) => result,
                futures::future::Either::Right(_) => Err(zeron_rpc::RpcError::Failed(
                    "Context inspection timed out".into(),
                )),
            };
            let _ = this.update(cx, |this, cx| {
                if this.sidebar.context_disclosure_expanded("expanded")
                    && this.context_selection(cx).as_ref() == Some(&key)
                    && this.context_breakdown.receive(generation, &key, result)
                {
                    cx.notify();
                }
            });
        }));
    }
    pub(super) fn toggle_context_breakdown(&mut self, cx: &mut Context<Self>) {
        self.sidebar.toggle_context_disclosure("expanded");
        self.hide_context_breakdown();
        self.emit_preferences(cx);
        cx.notify();
    }
    pub(super) fn render_context_breakdown(
        &mut self,
        theme: &Theme,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let mut body = div()
            .id("context-breakdown")
            .debug_selector(|| "context-breakdown".into())
            .px(px(12.0))
            .pb(px(8.0))
            .flex()
            .flex_col()
            .gap(px(4.0));
        let refreshing = self.context_breakdown.loading;
        let report = self.context_breakdown.snapshot.clone();
        let state = if refreshing {
            "Loading"
        } else if self.context_breakdown.failure.is_some() {
            availability_label(self.context_breakdown.failure.as_ref().unwrap().0)
        } else if report.as_ref().is_some_and(|r| r.stale) {
            "Stale"
        } else {
            report
                .as_ref()
                .map(|r| availability_label(r.availability))
                .unwrap_or("Unavailable")
        };
        let hover = theme.element_hover;
        let focus = theme.element_active;
        body = body.child(
            div()
                .flex()
                .items_center()
                .gap(px(8.0))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .text_size(px(11.5))
                        .text_color(theme.text_muted)
                        .child(state),
                )
                .child(
                    div()
                        .id("context-refresh")
                        .tab_index(0)
                        .role(gpui::Role::Button)
                        .aria_label("Refresh context budget")
                        .px(px(4.0))
                        .h(px(24.0))
                        .rounded(px(4.0))
                        .flex()
                        .items_center()
                        .text_size(px(11.5))
                        .text_color(theme.text_muted)
                        .cursor_pointer()
                        .hover(move |s| s.bg(hover))
                        .focus_visible(move |s| s.bg(focus))
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.hide_context_breakdown();
                            cx.notify();
                        }))
                        .child("Refresh"),
                ),
        );
        if let Some(report) = report {
            let total = report
                .used_tokens
                .map(format_token_count)
                .unwrap_or_else(|| "—".into());
            let total = if report.used_tokens.is_some()
                && report.used_basis.as_deref() != Some("provider-anchor")
            {
                format!("≈{total}")
            } else {
                total
            };
            let capacity = report
                .capacity
                .map(format_token_count)
                .unwrap_or_else(|| "—".into());
            body = body.child(
                div()
                    .flex()
                    .justify_between()
                    .items_center()
                    .text_size(px(12.5))
                    .child(div().text_color(theme.text).child("Used / capacity"))
                    .child(
                        div()
                            .text_color(theme.text_muted)
                            .child(format!("{total} / {capacity}")),
                    ),
            );
            let mut bar = div()
                .h(px(5.0))
                .w_full()
                .flex()
                .rounded_full()
                .overflow_hidden()
                .bg(theme.border);
            for (index, fraction) in report.segments() {
                bar = bar.child(
                    div()
                        .h_full()
                        .flex_none()
                        .w(gpui::relative(fraction))
                        .bg(category_color(&report.categories[index], theme)),
                );
            }
            body = body.child(bar);
            for (index, row) in report.categories.iter().enumerate() {
                let nested = row.parent_id.is_some();
                let label = if nested {
                    format!("{} · included", row.label)
                } else {
                    row.label.clone()
                };
                let tokens = row
                    .tokens
                    .map(format_token_count)
                    .unwrap_or_else(|| "—".into());
                let percent = row
                    .percentage(report.capacity)
                    .map(|p| format!("{p:.1}%"))
                    .unwrap_or_else(|| "—".into());
                let tip = format!(
                    "{} · {}{}",
                    row.label,
                    row.basis,
                    row.reason
                        .as_ref()
                        .map(|s| format!(" · {s}"))
                        .unwrap_or_default()
                );
                body = body.child(
                    div()
                        .id(("context-category", index))
                        .h(px(24.0))
                        .flex()
                        .items_center()
                        .gap(px(6.0))
                        .when(nested, |s| s.pl(px(12.0)))
                        .child(
                            div()
                                .size(px(7.0))
                                .rounded(px(2.0))
                                .flex_none()
                                .bg(category_color(row, theme)),
                        )
                        .child(
                            div()
                                .debug_selector(move || format!("context-category-label-{index}"))
                                .flex_1()
                                .min_w_0()
                                .truncate()
                                .text_size(px(12.0))
                                .text_color(theme.text_muted)
                                .child(label),
                        )
                        .child(
                            div()
                                .debug_selector(move || format!("context-category-tokens-{index}"))
                                .w(px(60.0))
                                .flex_none()
                                .text_right()
                                .text_size(px(12.0))
                                .text_color(theme.text_muted)
                                .child(tokens),
                        )
                        .child(
                            div()
                                .w(px(52.0))
                                .flex_none()
                                .text_right()
                                .text_size(px(12.0))
                                .text_color(theme.text)
                                .child(percent),
                        )
                        .tooltip(move |_, cx| {
                            cx.new(|_| ActionTooltip {
                                label: tip.clone().into(),
                            })
                            .into()
                        }),
                );
            }
            body = body.child(self.render_context_items(report.clone(), false, theme, cx));
            body = body.child(self.render_context_items(report.clone(), true, theme, cx));
            let source = match report.source.as_str() {
                "omp-native-v1" if report.used_basis.as_deref() == Some("provider-anchor") => {
                    "OMP · Provider total, category estimates"
                }
                "omp-native-v1" => "OMP · Local estimates",
                "claude-summary" => "Claude · Summary estimates",
                _ => "Native runtime",
            };
            body = body.child(note(source, theme));
            if let Some(native) = &report.native {
                body = body.child(note(
                    match native.has_local_changes {
                        Some(true) => "Local changes reported since the provider snapshot",
                        Some(false) => "No local changes reported",
                        None => "Local changes unknown; fingerprints unavailable",
                    },
                    theme,
                ));
                if let Some(residual) = native.system_context.unattributed_tokens {
                    body = body.child(note(format!("System context residual: {residual:+} tokens · BPE boundary estimate, included in System context"), theme).debug_selector(|| "context-native-residual".into()));
                }
                if native.deferred.schemas_in_context == Some(false) {
                    body = body.child(note("Active schemas omitted by model policy", theme));
                }
                if native.deferred.active_estimate_reason.is_some() {
                    body = body.child(note(
                        "Tool estimates use the declared roster; provider activation is unknown",
                        theme,
                    ));
                }
                if native.deferred.last_request.tools.availability == Availability::Available {
                    body = body.child(note(
                        format!(
                            "Last request deferred: {} names · historical only",
                            native.deferred.last_request.tools.items.len()
                        ),
                        theme,
                    ));
                }
                if let Some(at) = native.captured_at_ms {
                    body = body.child(note(format!("Native snapshot {}", time_label(at)), theme));
                }
            }
            if let Some(model) = &report.model {
                body = body.child(note(format!("Model · {model}"), theme));
            }
            if report.stale {
                body = body.child(note(
                    format!(
                        "Retained snapshot{}",
                        report
                            .model
                            .as_ref()
                            .map(|m| format!(" · {m}"))
                            .unwrap_or_default()
                    ),
                    theme,
                ));
            }
            body = body.child(note(
                format!("Observed {}", time_label(report.observed_at_ms)),
                theme,
            ));
            body = body.child(note(
                report
                    .measured_at_ms
                    .map(|at| format!("Provider measurement {}", time_label(at)))
                    .unwrap_or_else(|| "Provider measurement time unknown".into()),
                theme,
            ));
            if let Some(reason) = &report.reason {
                body = body.child(note(reason.clone(), theme));
            }
        }
        if let Some((_, reason)) = &self.context_breakdown.failure {
            body = body.child(note(reason.clone(), theme));
        }
        body.into_any_element()
    }
    fn render_context_items(
        &mut self,
        report: Arc<ContextBreakdown>,
        memory: bool,
        theme: &Theme,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let (section, label, selector) = if memory {
            ("memory:expanded", "Memory files", "context-memory-list")
        } else {
            ("mcp:expanded", "MCP tools", "context-mcp-list")
        };
        let list = if memory {
            &report.memory_files
        } else {
            &report.mcp_tools
        };
        let available = matches!(
            list.availability,
            Availability::Available | Availability::Partial
        );
        let expanded = self.sidebar.context_disclosure_expanded(section);
        let count = if available {
            list.items.len().to_string()
        } else {
            "—".into()
        };
        let hover = theme.element_hover;
        let focus = theme.element_active;
        let mut body = div().flex().flex_col().child(
            div()
                .id(SharedString::from(format!("context-{section}")))
                .debug_selector(move || format!("context-{section}"))
                .h(px(28.0))
                .flex()
                .items_center()
                .gap(px(6.0))
                .tab_index(0)
                .role(gpui::Role::Button)
                .aria_label(label)
                .aria_expanded(expanded)
                .cursor_pointer()
                .rounded(px(4.0))
                .hover(move |s| s.bg(hover))
                .focus_visible(move |s| s.bg(focus))
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.sidebar.toggle_context_disclosure(section);
                    this.emit_preferences(cx);
                    cx.notify();
                }))
                .child(
                    icons::icon(if expanded {
                        icons::ALT_ARROW_DOWN
                    } else {
                        icons::ALT_ARROW_RIGHT
                    })
                    .size(px(12.0))
                    .text_color(theme.text_muted),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .truncate()
                        .text_size(px(12.0))
                        .text_color(theme.text_muted)
                        .child(label),
                )
                .child(
                    div()
                        .flex_none()
                        .text_size(px(12.0))
                        .text_color(theme.text_muted)
                        .child(count),
                ),
        );
        if expanded {
            if available && !list.items.is_empty() {
                let count = list.items.len();
                let theme = theme.clone();
                let rows = report.clone();
                let scroll = if memory {
                    &self.context_breakdown.memory_scroll
                } else {
                    &self.context_breakdown.mcp_scroll
                };
                body = body.child(
                    div()
                        .debug_selector(move || selector.into())
                        .h(px(24.0 * count.min(7) as f32))
                        .child(
                            gpui::uniform_list(selector, count, move |range, _, _| {
                                range
                                    .filter_map(|index| {
                                        let list = if memory {
                                            &rows.memory_files
                                        } else {
                                            &rows.mcp_tools
                                        };
                                        let item = list.items.get(index)?;
                                        let label = format!(
                                            "{}{}{}{}{}",
                                            item.detail
                                                .as_ref()
                                                .map(|s| format!("{s} · "))
                                                .unwrap_or_default(),
                                            if memory {
                                                item.path.as_ref().unwrap_or(&item.name)
                                            } else {
                                                item.tool_name.as_ref().unwrap_or(&item.name)
                                            },
                                            if item.deferred { " · deferred" } else { "" },
                                            if item.loaded == Some(false) {
                                                " · not loaded"
                                            } else {
                                                ""
                                            },
                                            item.state
                                                .as_deref()
                                                .map(|state| format!(
                                                    " · {}",
                                                    match state {
                                                        "omitted-by-policy" => "schemas omitted",
                                                        "last-request-deferred" =>
                                                            "deferred in last request",
                                                        other => other,
                                                    }
                                                ))
                                                .unwrap_or_default()
                                        );
                                        let tooltip = format!(
                                            "{label}{}",
                                            item.reason
                                                .as_ref()
                                                .map(|reason| format!(" · {reason}"))
                                                .unwrap_or_default()
                                        );
                                        Some(
                                            div()
                                                .id((selector, index))
                                                .h(px(24.0))
                                                .pl(px(18.0))
                                                .flex()
                                                .items_center()
                                                .gap(px(6.0))
                                                .child(
                                                    div()
                                                        .flex_1()
                                                        .min_w_0()
                                                        .truncate()
                                                        .text_size(px(11.5))
                                                        .text_color(theme.text_muted)
                                                        .child(label),
                                                )
                                                .child(
                                                    div()
                                                        .w(px(60.0))
                                                        .flex_none()
                                                        .text_right()
                                                        .text_size(px(11.5))
                                                        .text_color(theme.text_muted)
                                                        .child(
                                                            item.tokens
                                                                .map(format_token_count)
                                                                .unwrap_or_else(|| "—".into()),
                                                        ),
                                                )
                                                .tooltip(move |_, cx| {
                                                    cx.new(|_| ActionTooltip {
                                                        label: tooltip.clone().into(),
                                                    })
                                                    .into()
                                                })
                                                .into_any_element(),
                                        )
                                    })
                                    .collect::<Vec<_>>()
                            })
                            .size_full()
                            .track_scroll(scroll),
                        ),
                );
            } else {
                body = body.child(note(
                    if available {
                        "No items reported"
                    } else {
                        "This runtime did not report this list"
                    },
                    theme,
                ));
            }
            if let Some(reason) = &list.reason {
                body = body.child(note(reason.clone(), theme));
            }
        }
        body.into_any_element()
    }
}
fn availability_label(value: Availability) -> &'static str {
    match value {
        Availability::Available => "Snapshot",
        Availability::Partial => "Partial",
        Availability::Unsupported => "Unsupported",
        Availability::NotApplicable => "Unavailable",
        Availability::Error => "Failed",
    }
}
fn note(text: impl Into<SharedString>, theme: &Theme) -> gpui::Div {
    div()
        .min_w_0()
        .text_size(px(11.0))
        .line_height(px(16.0))
        .text_color(theme.text_muted)
        .child(text.into())
}
fn time_label(ms: i64) -> String {
    chrono::DateTime::from_timestamp_millis(ms)
        .filter(|_| ms > 0)
        .map(|time| {
            time.with_timezone(&chrono::Local)
                .format("%Y-%m-%d %H:%M")
                .to_string()
        })
        .unwrap_or_else(|| "unknown".into())
}
fn category_color(row: &ContextCategory, theme: &Theme) -> gpui::Hsla {
    match row.kind {
        Kind::Free => theme.border,
        Kind::Buffer | Kind::Deferred => theme.text_faint,
        _ => match row.id.as_str() {
            "messages" => theme.accent,
            "systemTools" | "system_tools" => theme.warning,
            "skills" => theme.success,
            "systemPrompt" | "system_prompt" => theme.busy,
            _ => theme.text_faint,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn selection(chat: &str, host: &str) -> Selection {
        Selection {
            context: format!("{chat}:{host}"),
            chat: chat.into(),
            host: host.into(),
            config: None,
            completed: None,
            settled_usage: None,
            lifecycle: None,
            compacting: false,
            available: true,
        }
    }
    fn report(chat: &str, host: &str) -> ContextBreakdown {
        ContextBreakdown {
            schema_version: 1,
            chat_id: chat.into(),
            host_device_id: host.into(),
            used_tokens: Some(0),
            capacity: Some(0),
            observed_at_ms: 1234,
            availability: Availability::Partial,
            ..Default::default()
        }
    }
    #[test]
    fn context_results_are_latest_only_and_never_cross_chat_host_or_model() {
        let a = selection("a", "host");
        let mut panel = PanelState::default();
        assert!(panel.select(a.clone()));
        let first = panel.generation;
        assert!(
            !panel.select(a.clone()),
            "streaming repaints do not request another inspection"
        );
        assert!(panel.receive(first, &a, Ok(report("a", "host"))));
        assert_eq!(panel.snapshot.as_ref().unwrap().used_tokens, Some(0));
        assert_eq!(panel.fraction(), None, "zero capacity is not a percentage");
        let mut measured = report("a", "host");
        measured.used_tokens = Some(828300);
        measured.capacity = Some(1000000);
        panel.receive(first, &a, Ok(measured));
        assert_eq!(panel.fraction(), Some(0.8283));
        panel.receive(first, &a, Ok(report("a", "host")));
        assert!(panel.snapshot.as_ref().unwrap().segments().is_empty());
        let mut next = a.clone();
        next.completed = Some("new-turn".into());
        assert!(panel.select(next.clone()));
        let second = panel.generation;
        assert!(panel.snapshot.as_ref().unwrap().stale);
        assert!(!panel.receive(first, &a, Ok(report("a", "host"))));
        panel.receive(second, &next, Err(zeron_rpc::RpcError::Closed));
        assert_eq!(panel.snapshot.as_ref().unwrap().observed_at_ms, 1234);
        assert_eq!(panel.snapshot.as_ref().unwrap().used_tokens, Some(0));
        assert!(panel.snapshot.as_ref().unwrap().stale);
        let mut unsupported =
            ContextBreakdown::unavailable(Availability::Unsupported, "Runtime update required");
        unsupported.chat_id = "a".into();
        unsupported.host_device_id = "host".into();
        panel.receive(second, &next, Ok(unsupported));
        assert_eq!(panel.snapshot.as_ref().unwrap().observed_at_ms, 1234);
        assert_eq!(panel.snapshot.as_ref().unwrap().used_tokens, Some(0));
        let b = selection("b", "host");
        panel.select(b.clone());
        assert!(panel.snapshot.is_none());
        let generation = panel.generation;
        panel.receive(generation, &b, Ok(report("b", "other-host")));
        assert!(panel.snapshot.is_none());
        assert_eq!(panel.failure.as_ref().unwrap().0, Availability::Error);
        let mut model = b.clone();
        model.config = Some(zeron_proto::ChatConfig {
            harness: zeron_proto::HarnessId::Omp,
            model: Some("new".into()),
            reasoning: None,
            model_options: Default::default(),
            sandbox: zeron_proto::SandboxLevel::WorkspaceWrite,
        });
        assert!(panel.select(model.clone()));
        assert!(!panel.receive(generation, &b, Ok(report("b", "host"))));
        let generation = panel.generation;
        panel.hide();
        assert!(!panel.receive(generation, &model, Ok(report("b", "host"))));
        assert!(panel.select(model));
        let mut compacted = panel.key.clone().unwrap();
        compacted.compacting = true;
        assert!(panel.select(compacted.clone()));
        compacted.compacting = false;
        assert!(panel.select(compacted.clone()));
        compacted.lifecycle = Some((
            zeron_proto::SessionStatus::Working,
            Some(chrono::Utc::now()),
            None,
        ));
        assert!(panel.select(compacted));
    }
    #[test]
    fn late_settled_usage_refreshes_without_following_active_token_counters() {
        let mut session: zeron_proto::Session = serde_json::from_value(serde_json::json!({
            "chatId":"chat","deviceId":"host","status":"working","updatedAt":chrono::Utc::now(),
            "contextUsage":{"tokens":100,"contextWindow":1000}
        }))
        .unwrap();
        let mut key = selection("chat", "host");
        key.settled_usage = settled_usage(&session);
        let mut panel = PanelState::default();
        assert!(panel.select(key.clone()));
        for tokens in [101, 102, 103] {
            session.context_usage = Some(zeron_proto::ContextUsage::reported(
                Some(tokens),
                Some(1000),
            ));
            key.settled_usage = settled_usage(&session);
            assert!(
                !panel.select(key.clone()),
                "active token counters must not request snapshots"
            );
        }
        session.status = zeron_proto::SessionStatus::Idle;
        key.completed = Some("done".into());
        key.settled_usage = settled_usage(&session);
        assert!(panel.select(key.clone()));
        session.context_usage = Some(zeron_proto::ContextUsage::reported(Some(200), Some(1000)));
        key.settled_usage = settled_usage(&session);
        assert!(
            panel.select(key),
            "late settled usage must refresh an already expanded snapshot"
        );
    }
    #[test]
    fn context_disclosures_round_trip_without_changing_sibling_preferences() {
        let context = |key: &str| DetailsContext {
            key: key.into(),
            cwd: "/fixture".into(),
            branch: None,
            chat_id: Some(key.into()),
            target_device_id: Some("host".into()),
            mode: crate::details_sidebar::context::DetailsMode::Orchestrator,
        };
        let mut state = DetailsSidebarState::new(DetailsSidebarPreferences::default());
        state.set_context(Some(context("a")));
        state.toggle_context_disclosure("expanded");
        state.toggle_context_disclosure("mcp:expanded");
        assert!(!state.context_disclosure_expanded("memory:expanded"));
        assert!(!state.projects_worked_collapsed());
        let prefs: DetailsSidebarPreferences =
            serde_json::from_value(serde_json::to_value(state.preferences()).unwrap()).unwrap();
        let mut restored = DetailsSidebarState::new(prefs);
        restored.set_context(Some(context("b")));
        assert!(!restored.context_disclosure_expanded("expanded"));
        assert!(!restored.context_disclosure_expanded("mcp:expanded"));
        restored.set_context(Some(context("a")));
        assert!(restored.context_disclosure_expanded("expanded"));
        assert!(restored.context_disclosure_expanded("mcp:expanded"));
        assert!(!restored.context_disclosure_expanded("memory:expanded"));
    }
    #[gpui::test]
    fn context_budget_light_dark_narrow_rows_and_independent_bounded_lists(
        cx: &mut gpui::TestAppContext,
    ) {
        use gpui::{AppContext, Modifiers, Render, Window};
        let data = tempfile::tempdir().unwrap();
        cx.update(|cx| {
            gpui_base::init(cx);
            crate::settings::init(crate::settings::UiSettings::default(), data.path(), cx);
            cx.set_global(Theme::dark());
        });
        let state = cx.new(|_| AppState::new());
        let workers = cx.new(|cx| WorkersModel::detached(state.clone(), cx));
        let composer = cx.new(|cx| Composer::new(state.clone(), cx));
        let pickers = composer.read_with(cx, |c, _| c.pickers().clone());
        let sidebar = cx.new(|cx| {
            DetailsSidebar::new(
                state,
                workers,
                DetailsSidebarPreferences::default(),
                pickers,
                composer,
                cx,
            )
        });
        sidebar.update(cx,|s,_|{
            s.sidebar.set_context(Some(DetailsContext{key:"budget-fixture".into(),cwd:"/fixture".into(),branch:None,chat_id:Some("chat".into()),target_device_id:Some("host".into()),mode:crate::details_sidebar::context::DetailsMode::Orchestrator}));
            s.sidebar.toggle_context_disclosure("expanded");
            let row=|id:&str,label:&str,kind,tokens,parent:Option<&str>|ContextCategory{id:id.into(),label:label.into(),kind,tokens:Some(tokens),basis:"tokenizer-estimate".into(),parent_id:parent.map(str::to_owned),reason:None};
            let items=|n|zeron_proto::ContextItemList{availability:Availability::Available,reason:None,items:(0..n).map(|i|zeron_proto::ContextItem{name:format!("/synthetic/a-very-long-metadata-path-that-must-truncate-before-the-token-column/{i}"),detail:None,tokens:if i==0{None}else{Some(12)},deferred:i==0,loaded:Some(i!=0),..Default::default()}).collect()};
            s.context_breakdown.snapshot=Some(Arc::new(ContextBreakdown{schema_version:1,chat_id:"chat".into(),host_device_id:"host".into(),source:"omp-native-v1".into(),used_tokens:Some(828300),used_basis:Some("provider-anchor".into()),capacity:Some(1000000),availability:Availability::Partial,observed_at_ms:1793900000000,categories:vec![
                row("messages","Messages with an intentionally very long native category label",Kind::Used,790800,None),
                row("systemContext","System context",Kind::Used,37500,None),row("memoryFiles","Memory files",Kind::Used,1200,Some("systemContext")),
                row("autocompact_buffer","Autocompact buffer",Kind::Buffer,33000,None),row("free_space","Free space",Kind::Free,138700,None),row("deferred","Deferred tools",Kind::Deferred,18500,None)
            ],mcp_tools:items(20),memory_files:items(10),native:Some(zeron_proto::ContextNativeMetadata {
                has_local_changes: None,
                system_context: zeron_proto::ContextAttribution { unattributed_tokens: Some(-20), ..Default::default() },
                deferred: zeron_proto::ContextDeferred { schemas_in_context: Some(false), active_estimate_reason: Some("native-estimate-counts-declared-roster-not-provider-defer-activation".into()), ..Default::default() },
                ..Default::default()
            }),..Default::default()}));
        });
        struct Probe {
            sidebar: Entity<DetailsSidebar>,
            width: f32,
            _observe: Subscription,
        }
        impl Render for Probe {
            fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
                let theme = Theme::of(cx).clone();
                let body = self
                    .sidebar
                    .update(cx, |s, cx| s.render_context_breakdown(&theme, cx));
                div().w(px(self.width)).h(px(900.0)).child(body)
            }
        }
        let (probe, window) = cx.add_window_view(|_, cx| Probe {
            sidebar: sidebar.clone(),
            width: 300.0,
            _observe: cx.observe(&sidebar, |_, _, cx| cx.notify()),
        });
        for light in [false, true] {
            for width in [300.0, 500.0] {
                window.update(|_, cx| {
                    cx.set_global(if light { Theme::light() } else { Theme::dark() })
                });
                probe.update(window, |p, cx| {
                    p.width = width;
                    cx.notify();
                });
                window.update(|w, cx| w.draw(cx).clear());
                assert!(window.debug_bounds("context-native-residual").is_some());
                let label = window.debug_bounds("context-category-label-0").unwrap();
                let tokens = window.debug_bounds("context-category-tokens-0").unwrap();
                assert!(
                    label.right() <= tokens.left(),
                    "long labels cannot overlap token counts"
                );
                assert!(tokens.right() <= gpui::px(width - 12.0));
            }
        }
        let mcp = window.debug_bounds("context-mcp:expanded").unwrap();
        window.simulate_click(mcp.center(), Modifiers::default());
        window.update(|w, cx| w.draw(cx).clear());
        assert!(window.debug_bounds("context-mcp-list").is_some());
        assert!(window.debug_bounds("context-memory-list").is_none());
        let memory = window.debug_bounds("context-memory:expanded").unwrap();
        window.simulate_click(memory.center(), Modifiers::default());
        window.update(|w, cx| w.draw(cx).clear());
        for name in ["context-mcp-list", "context-memory-list"] {
            assert_eq!(
                window.debug_bounds(name).unwrap().size.height,
                gpui::px(168.0)
            );
        }
        window.simulate_click(mcp.center(), Modifiers::default());
        window.update(|w, cx| w.draw(cx).clear());
        assert!(window.debug_bounds("context-mcp-list").is_none());
        assert!(window.debug_bounds("context-memory-list").is_some());
        println!(
            "SANITIZED GPUI context layout: light/dark at 300/500px; truncated label/count columns; two independent 7-row viewports"
        );
    }
}
