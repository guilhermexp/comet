//! Record inspector panel for Trajectory preview.
//!
//! Provides technical inspection of a selected trajectory record across five
//! tabs (Summary, Payload, Result, Schema, Timing), with support for explicit
//! device-local Raw Reveal and responsive split/narrow layout switching.

use gpui::{
    AnyElement, App, Div, ElementId, InteractiveElement, IntoElement, ParentElement, SharedString,
    StatefulInteractiveElement, Styled, div, px,
};
use zeron_proto::trajectory::{
    TrajectoryRawField, TrajectoryRecord, TrajectoryRecordKind, TrajectoryStatus,
    TrajectoryTimingMode, format_duration_ms, format_duration_or_unavailable,
};
use zeron_rpc::{RevealTrajectoryRawParams, TrajectoryUnavailableReason};

use crate::{
    icons,
    theme::Theme,
    trajectory::model::{RevealState, TrajectoryViewModel},
};

/// Breakpoint width below which Trajectory view switches from Split layout to NarrowDetail.
pub const TRAJECTORY_SPLIT_THRESHOLD: gpui::Pixels = px(600.0);

/// Presentation layout mode based on available container width.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrajectoryLayout {
    /// Side-by-side: Timeline/Ledger on left, Inspector panel on right.
    Split,
    /// Stacked/Focused: Inspector takes the whole width with a back affordance.
    NarrowDetail,
}

/// Pure decision mapping available container width to layout mode.
pub fn layout_mode(width: gpui::Pixels) -> TrajectoryLayout {
    if width >= TRAJECTORY_SPLIT_THRESHOLD {
        TrajectoryLayout::Split
    } else {
        TrajectoryLayout::NarrowDetail
    }
}

/// Tabs available in the Trajectory record inspector.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum InspectorTab {
    Summary,
    Payload,
    Result,
    Schema,
    Timing,
}

impl InspectorTab {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Summary => "Summary",
            Self::Payload => "Payload",
            Self::Result => "Result",
            Self::Schema => "Schema",
            Self::Timing => "Timing",
        }
    }
}

/// Returns the list of tabs that have corresponding data in `record`.
///
/// Invariant: Only tabs with actual data are returned. Summary is always available;
/// Payload requires payload preview; Result requires result preview; Schema requires
/// schema info on the payload; Timing requires timing information.
pub fn available_tabs(record: &TrajectoryRecord) -> Vec<InspectorTab> {
    let mut tabs = Vec::with_capacity(5);
    tabs.push(InspectorTab::Summary);

    if record.payload.is_some() {
        tabs.push(InspectorTab::Payload);
    }

    if record.result.is_some() {
        tabs.push(InspectorTab::Result);
    }

    if record
        .payload
        .as_ref()
        .and_then(|p| p.schema_info.as_ref())
        .is_some()
    {
        tabs.push(InspectorTab::Schema);
    }

    if record.timing.is_some() {
        tabs.push(InspectorTab::Timing);
    }

    tabs
}

/// Value of a summary technical field, distinguishing known values,
/// unavailable data (data never existed or cannot be known), and unsettled
/// data (the operation is still in flight).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SummaryValue {
    Present(SharedString),
    Unavailable,
    Unsettled,
}

/// A labeled technical metadata field for a trajectory record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SummaryField {
    pub label: SharedString,
    pub value: SummaryValue,
}

/// Extracts technical summary fields from a trajectory record.
///
/// Distinguishes present values, `Unavailable` (data never existed or was not
/// recorded) and `Unsettled` (operation is in-flight). Missing fields never
/// collapse to empty strings or synthetic zeroes.
pub fn summary_fields(record: &TrajectoryRecord) -> Vec<SummaryField> {
    let in_flight = matches!(
        record.status,
        TrajectoryStatus::Running | TrajectoryStatus::Unsettled
    );

    let mut fields = Vec::with_capacity(10);

    // 1. Record ID
    fields.push(SummaryField {
        label: "Record ID".into(),
        value: SummaryValue::Present(record.id.key().into()),
    });

    // 2. Run ID
    fields.push(SummaryField {
        label: "Run".into(),
        value: SummaryValue::Present(record.run_id.clone().into()),
    });

    // 3. Turn ID
    fields.push(SummaryField {
        label: "Turn".into(),
        value: if let Some(turn) = &record.turn_id {
            SummaryValue::Present(turn.clone().into())
        } else if in_flight {
            SummaryValue::Unsettled
        } else {
            SummaryValue::Unavailable
        },
    });

    // 4. Step ID
    fields.push(SummaryField {
        label: "Step".into(),
        value: if let Some(step) = &record.step_id {
            SummaryValue::Present(step.clone().into())
        } else if in_flight {
            SummaryValue::Unsettled
        } else {
            SummaryValue::Unavailable
        },
    });

    // 5. Hierarchy / Kind
    fields.push(SummaryField {
        label: "Kind".into(),
        value: SummaryValue::Present(format_record_kind(&record.kind).into()),
    });

    // 6. Lane
    fields.push(SummaryField {
        label: "Lane".into(),
        value: SummaryValue::Present(record.lane.as_str().into()),
    });

    // 7. Status
    fields.push(SummaryField {
        label: "Status".into(),
        value: if record.status == TrajectoryStatus::Unsettled {
            SummaryValue::Unsettled
        } else {
            SummaryValue::Present(format_record_status(record.effective_status()).into())
        },
    });

    // 8. Error State
    fields.push(SummaryField {
        label: "Error".into(),
        value: if let Some(err) = &record.error_message {
            SummaryValue::Present(err.clone().into())
        } else if record.effective_status().is_error() {
            SummaryValue::Present("Failed".into())
        } else if in_flight {
            SummaryValue::Unsettled
        } else {
            SummaryValue::Unavailable
        },
    });

    // 9. Duration
    fields.push(SummaryField {
        label: "Duration".into(),
        value: if let Some(timing) = &record.timing {
            if timing.mode == TrajectoryTimingMode::SequenceOnly {
                SummaryValue::Unavailable
            } else if let Some(d) = timing.effective_duration_ms() {
                SummaryValue::Present(format_duration_ms(d).into())
            } else if in_flight {
                SummaryValue::Unsettled
            } else {
                SummaryValue::Unavailable
            }
        } else if in_flight {
            SummaryValue::Unsettled
        } else {
            SummaryValue::Unavailable
        },
    });

    // 10. Usage / Tokens
    fields.push(SummaryField {
        label: "Tokens".into(),
        value: if let Some(usage) = &record.usage {
            if let Some(total) = usage.total_tokens {
                SummaryValue::Present(format!("{total} tokens").into())
            } else if let (Some(input), Some(output)) = (usage.input_tokens, usage.output_tokens) {
                SummaryValue::Present(format!("{input} in / {output} out").into())
            } else if in_flight {
                SummaryValue::Unsettled
            } else {
                SummaryValue::Unavailable
            }
        } else if in_flight {
            SummaryValue::Unsettled
        } else {
            SummaryValue::Unavailable
        },
    });

    // Optional call identifiers
    if let Some(call_id) = &record.call_id {
        fields.push(SummaryField {
            label: "Call ID".into(),
            value: SummaryValue::Present(call_id.clone().into()),
        });
    }

    if let Some(parent) = &record.parent_tool_use_id {
        fields.push(SummaryField {
            label: "Parent Call".into(),
            value: SummaryValue::Present(parent.clone().into()),
        });
    }

    fields
}

/// Returns the request parameters for raw reveal of `field` on `record`, or `None`
/// if no raw reference exists.
pub fn reveal_params(
    chat_id: &str,
    record: &TrajectoryRecord,
    field: TrajectoryRawField,
) -> Option<RevealTrajectoryRawParams> {
    let raw_ref = match field {
        TrajectoryRawField::Payload => record.payload.as_ref().and_then(|p| p.raw_ref.as_ref()),
        TrajectoryRawField::Result => record.result.as_ref().and_then(|r| r.raw_ref.as_ref()),
    }?;

    let effective_chat_id = if !raw_ref.chat_id.is_empty() {
        raw_ref.chat_id.clone()
    } else {
        chat_id.to_string()
    };

    Some(RevealTrajectoryRawParams {
        chat_id: effective_chat_id,
        source_seq: raw_ref.source_seq,
        parent_tool_use_id: raw_ref.parent_tool_use_id.clone(),
        call_id: raw_ref.call_id.clone(),
        field,
        source_version: raw_ref.source_version,
    })
}

// ---------------------------------------------------------------------------
// Render
// ---------------------------------------------------------------------------

/// Render the Trajectory inspector panel.
pub fn render_inspector(
    model: &TrajectoryViewModel,
    tab: InspectorTab,
    theme: &Theme,
    layout: TrajectoryLayout,
    on_select_tab: impl Fn(InspectorTab, &mut App) + 'static,
    on_reveal: impl Fn(TrajectoryRawField, RevealTrajectoryRawParams, &mut App) + 'static,
    on_clear_reveal: impl Fn(TrajectoryRawField, &mut App) + 'static,
    on_back: Option<impl Fn(&mut App) + 'static>,
) -> AnyElement {
    let Some(record) = model.selected_record() else {
        return render_empty_inspector(theme);
    };

    let tabs = available_tabs(record);
    let active_tab = if tabs.contains(&tab) {
        tab
    } else {
        tabs.first().copied().unwrap_or(InspectorTab::Summary)
    };

    let chat_id = model.chat_id();

    div()
        .id("trajectory-inspector")
        .size_full()
        .flex()
        .flex_col()
        .bg(theme.bg)
        .border_l_1()
        .border_color(theme.border)
        // Header
        .child(render_inspector_header(record, theme, layout, on_back))
        // Tab Strip
        .child(render_tab_strip(&tabs, active_tab, theme, on_select_tab))
        // Tab Content Body
        .child(
            div()
                .flex_1()
                .overflow_hidden()
                .p(px(12.0))
                .child(match active_tab {
                    InspectorTab::Summary => render_summary_tab(record, theme),
                    InspectorTab::Payload => render_payload_tab(
                        chat_id,
                        record,
                        model,
                        theme,
                        on_reveal,
                        on_clear_reveal,
                    ),
                    InspectorTab::Result => {
                        render_result_tab(chat_id, record, model, theme, on_reveal, on_clear_reveal)
                    }
                    InspectorTab::Schema => render_schema_tab(record, theme),
                    InspectorTab::Timing => render_timing_tab(record, theme),
                }),
        )
        .into_any_element()
}

fn render_empty_inspector(theme: &Theme) -> AnyElement {
    div()
        .id("trajectory-inspector-empty")
        .size_full()
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .p(px(24.0))
        .gap(px(8.0))
        .child(
            icons::icon(icons::INFO_CIRCLE)
                .size(px(24.0))
                .text_color(theme.text_faint),
        )
        .child(
            div()
                .text_size(px(13.0))
                .text_color(theme.text_muted)
                .child("Select a record to inspect technical details"),
        )
        .into_any_element()
}

fn render_inspector_header(
    record: &TrajectoryRecord,
    theme: &Theme,
    layout: TrajectoryLayout,
    on_back: Option<impl Fn(&mut App) + 'static>,
) -> Div {
    let header = div()
        .h(px(40.0))
        .flex_none()
        .px(px(12.0))
        .flex()
        .items_center()
        .justify_between()
        .border_b_1()
        .border_color(theme.border);

    let left = div().flex().items_center().gap(px(8.0));

    let left = if layout == TrajectoryLayout::NarrowDetail {
        if let Some(on_back) = on_back {
            left.child(
                div()
                    .id("trajectory-inspector-back")
                    .size(px(24.0))
                    .rounded(px(4.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .cursor_pointer()
                    .hover(|s| s.bg(theme.element_hover))
                    .on_click(move |_, _, cx| on_back(cx))
                    .child(
                        icons::icon(icons::ARROW_LEFT)
                            .size(px(14.0))
                            .text_color(theme.text_muted),
                    ),
            )
        } else {
            left
        }
    } else {
        left
    };

    let title_text = if !record.title.is_empty() {
        record.title.clone()
    } else if !record.summary.is_empty() {
        record.summary.clone()
    } else {
        format_record_kind(&record.kind)
    };

    let left = left.child(
        div()
            .text_size(px(13.0))
            .font_weight(gpui::FontWeight::SEMIBOLD)
            .text_color(theme.text)
            .child(title_text),
    );

    let status_color = if record.effective_status().is_error() {
        theme.danger
    } else if record.status == TrajectoryStatus::Running
        || record.status == TrajectoryStatus::Unsettled
    {
        theme.warning
    } else {
        theme.success
    };

    let right = div()
        .px(px(6.0))
        .py(px(2.0))
        .rounded(px(4.0))
        .bg(theme.surface_raised)
        .text_size(px(11.0))
        .text_color(status_color)
        .child(format_record_status(record.effective_status()));

    header.child(left).child(right)
}

fn render_tab_strip(
    tabs: &[InspectorTab],
    active_tab: InspectorTab,
    theme: &Theme,
    on_select_tab: impl Fn(InspectorTab, &mut App) + 'static,
) -> Div {
    let mut strip = div()
        .h(px(32.0))
        .flex_none()
        .px(px(8.0))
        .flex()
        .items_center()
        .gap(px(4.0))
        .bg(theme.surface)
        .border_b_1()
        .border_color(theme.border);

    let on_select = std::rc::Rc::new(on_select_tab);

    for &t in tabs {
        let is_active = t == active_tab;
        let on_click = on_select.clone();
        strip = strip.child(
            div()
                .id(ElementId::from(SharedString::from(format!(
                    "inspector-tab-{}",
                    t.label()
                ))))
                .h(px(24.0))
                .px(px(8.0))
                .rounded(px(4.0))
                .flex()
                .items_center()
                .justify_center()
                .cursor_pointer()
                .bg(if is_active {
                    theme.bg
                } else {
                    gpui::transparent_black()
                })
                .hover(|s| {
                    if !is_active {
                        s.bg(theme.element_hover)
                    } else {
                        s
                    }
                })
                .text_size(px(12.0))
                .font_weight(if is_active {
                    gpui::FontWeight::MEDIUM
                } else {
                    gpui::FontWeight::NORMAL
                })
                .text_color(if is_active {
                    theme.text
                } else {
                    theme.text_muted
                })
                .on_click(move |_, _, cx| on_click(t, cx))
                .child(t.label()),
        );
    }

    strip
}

fn render_summary_tab(record: &TrajectoryRecord, theme: &Theme) -> Div {
    let fields = summary_fields(record);

    let mut list = div().flex().flex_col().gap(px(6.0));

    for field in fields {
        list = list.child(
            div()
                .flex()
                .items_baseline()
                .justify_between()
                .py(px(4.0))
                .border_b_1()
                .border_color(theme.border)
                .gap(px(8.0))
                .child(
                    div()
                        .flex_none()
                        .text_size(px(12.0))
                        .text_color(theme.text_muted)
                        .child(field.label),
                )
                // Run and record ids are full UUIDs. `truncate()` under a
                // right-aligned flex cell clips the HEAD (the identifying
                // prefix), so the value cell wraps instead: the inspector is
                // not a fixed-height list, and a summary that hides which run
                // you are looking at defeats the panel.
                .child(match field.value {
                    SummaryValue::Present(val) => div()
                        .flex_1()
                        .min_w_0()
                        .text_size(px(12.0))
                        .font_weight(gpui::FontWeight::MEDIUM)
                        .text_color(theme.text)
                        .child(val),
                    SummaryValue::Unavailable => div()
                        .flex_1()
                        .min_w_0()
                        .text_size(px(12.0))
                        .text_color(theme.text_faint)
                        .child("Unavailable"),
                    SummaryValue::Unsettled => div()
                        .flex_1()
                        .min_w_0()
                        .text_size(px(12.0))
                        .text_color(theme.warning)
                        .child("Unsettled…"),
                }),
        );
    }

    list
}

fn render_payload_tab(
    chat_id: &str,
    record: &TrajectoryRecord,
    model: &TrajectoryViewModel,
    theme: &Theme,
    on_reveal: impl Fn(TrajectoryRawField, RevealTrajectoryRawParams, &mut App) + 'static,
    on_clear_reveal: impl Fn(TrajectoryRawField, &mut App) + 'static,
) -> Div {
    let mut body = div().flex().flex_col().gap(px(12.0));

    let Some(payload) = &record.payload else {
        return body.child(
            div()
                .text_size(px(12.0))
                .text_color(theme.text_faint)
                .child("No payload data recorded for this entry."),
        );
    };

    // Summary header
    if !payload.summary.is_empty() {
        body = body.child(
            div()
                .p(px(8.0))
                .rounded(px(4.0))
                .bg(theme.surface_raised)
                .text_size(px(12.0))
                .text_color(theme.text)
                .child(payload.summary.clone()),
        );
    }

    // Raw Reveal or Sanitized Display
    let reveal_state = model.reveal_state(TrajectoryRawField::Payload);
    let raw_params = reveal_params(chat_id, record, TrajectoryRawField::Payload);

    match reveal_state {
        RevealState::Revealed(raw_text) => {
            let on_clear = std::rc::Rc::new(on_clear_reveal);
            body = body
                .child(
                    div()
                        .flex()
                        .items_center()
                        .justify_between()
                        .px(px(8.0))
                        .py(px(4.0))
                        .rounded(px(4.0))
                        .bg(theme.accent_wash)
                        .child(
                            div()
                                .text_size(px(11.0))
                                .font_weight(gpui::FontWeight::MEDIUM)
                                .text_color(theme.accent)
                                .child("Raw Payload (device-local)"),
                        )
                        .child(
                            div()
                                .id("hide-raw-payload-btn")
                                .cursor_pointer()
                                .text_size(px(11.0))
                                .text_color(theme.text_muted)
                                .hover(|s| s.text_color(theme.text))
                                .on_click(move |_, _, cx| on_clear(TrajectoryRawField::Payload, cx))
                                .child("Hide raw"),
                        ),
                )
                .child(
                    div()
                        .p(px(8.0))
                        .rounded(px(4.0))
                        .bg(theme.input_bg)
                        .border_1()
                        .border_color(theme.border)
                        .text_size(px(12.0))
                        .text_color(theme.code_text)
                        .child(raw_text.clone()),
                );
        }
        RevealState::Pending => {
            body = body.child(
                div()
                    .p(px(8.0))
                    .rounded(px(4.0))
                    .bg(theme.surface_raised)
                    .text_size(px(12.0))
                    .text_color(theme.warning)
                    .child("Revealing raw payload from device journal…"),
            );
        }
        RevealState::Unavailable(reason) => {
            body = body.child(
                div()
                    .p(px(8.0))
                    .rounded(px(4.0))
                    .bg(theme.surface_raised)
                    .text_size(px(12.0))
                    .text_color(theme.danger)
                    .child(format!(
                        "Raw payload unavailable: {}",
                        format_unavailable_reason(*reason)
                    )),
            );
        }
        RevealState::Hidden => {
            if let Some(text) = &payload.sanitized_text {
                body = body.child(
                    div()
                        .p(px(8.0))
                        .rounded(px(4.0))
                        .bg(theme.input_bg)
                        .border_1()
                        .border_color(theme.border)
                        .text_size(px(12.0))
                        .text_color(theme.text)
                        .child(text.clone()),
                );
            }

            if let Some(params) = raw_params {
                let on_rev = std::rc::Rc::new(on_reveal);
                body = body.child(
                    div()
                        .id("reveal-raw-payload-btn")
                        .h(px(28.0))
                        .px(px(10.0))
                        .rounded(px(4.0))
                        .bg(theme.surface_raised)
                        .hover(|s| s.bg(theme.element_hover))
                        .cursor_pointer()
                        .flex()
                        .items_center()
                        .gap(px(6.0))
                        .on_click(move |_, _, cx| {
                            on_rev(TrajectoryRawField::Payload, params.clone(), cx)
                        })
                        .child(
                            icons::icon(icons::DETAILS_EYE)
                                .size(px(14.0))
                                .text_color(theme.text_muted),
                        )
                        .child(
                            div()
                                .text_size(px(12.0))
                                .text_color(theme.text)
                                .child("Reveal raw payload"),
                        ),
                );
            }
        }
    }

    body
}

fn render_result_tab(
    chat_id: &str,
    record: &TrajectoryRecord,
    model: &TrajectoryViewModel,
    theme: &Theme,
    on_reveal: impl Fn(TrajectoryRawField, RevealTrajectoryRawParams, &mut App) + 'static,
    on_clear_reveal: impl Fn(TrajectoryRawField, &mut App) + 'static,
) -> Div {
    let mut body = div().flex().flex_col().gap(px(12.0));

    let Some(result) = &record.result else {
        return body.child(
            div()
                .text_size(px(12.0))
                .text_color(theme.text_faint)
                .child("No result data recorded for this entry."),
        );
    };

    // Summary header
    if !result.summary.is_empty() {
        body = body.child(
            div()
                .p(px(8.0))
                .rounded(px(4.0))
                .bg(if result.is_error {
                    theme.danger_muted
                } else {
                    theme.surface_raised
                })
                .text_size(px(12.0))
                .text_color(if result.is_error {
                    theme.danger
                } else {
                    theme.text
                })
                .child(result.summary.clone()),
        );
    }

    if let Some(code) = result.exit_code {
        body = body.child(
            div()
                .text_size(px(11.0))
                .text_color(theme.text_muted)
                .child(format!("Exit code: {code}")),
        );
    }

    // Raw Reveal or Sanitized Display
    let reveal_state = model.reveal_state(TrajectoryRawField::Result);
    let raw_params = reveal_params(chat_id, record, TrajectoryRawField::Result);

    match reveal_state {
        RevealState::Revealed(raw_text) => {
            let on_clear = std::rc::Rc::new(on_clear_reveal);
            body = body
                .child(
                    div()
                        .flex()
                        .items_center()
                        .justify_between()
                        .px(px(8.0))
                        .py(px(4.0))
                        .rounded(px(4.0))
                        .bg(theme.accent_wash)
                        .child(
                            div()
                                .text_size(px(11.0))
                                .font_weight(gpui::FontWeight::MEDIUM)
                                .text_color(theme.accent)
                                .child("Raw Result (device-local)"),
                        )
                        .child(
                            div()
                                .id("hide-raw-result-btn")
                                .cursor_pointer()
                                .text_size(px(11.0))
                                .text_color(theme.text_muted)
                                .hover(|s| s.text_color(theme.text))
                                .on_click(move |_, _, cx| on_clear(TrajectoryRawField::Result, cx))
                                .child("Hide raw"),
                        ),
                )
                .child(
                    div()
                        .p(px(8.0))
                        .rounded(px(4.0))
                        .bg(theme.input_bg)
                        .border_1()
                        .border_color(theme.border)
                        .text_size(px(12.0))
                        .text_color(theme.code_text)
                        .child(raw_text.clone()),
                );
        }
        RevealState::Pending => {
            body = body.child(
                div()
                    .p(px(8.0))
                    .rounded(px(4.0))
                    .bg(theme.surface_raised)
                    .text_size(px(12.0))
                    .text_color(theme.warning)
                    .child("Revealing raw result from device journal…"),
            );
        }
        RevealState::Unavailable(reason) => {
            body = body.child(
                div()
                    .p(px(8.0))
                    .rounded(px(4.0))
                    .bg(theme.surface_raised)
                    .text_size(px(12.0))
                    .text_color(theme.danger)
                    .child(format!(
                        "Raw result unavailable: {}",
                        format_unavailable_reason(*reason)
                    )),
            );
        }
        RevealState::Hidden => {
            if let Some(text) = &result.sanitized_text {
                body = body.child(
                    div()
                        .p(px(8.0))
                        .rounded(px(4.0))
                        .bg(theme.input_bg)
                        .border_1()
                        .border_color(theme.border)
                        .text_size(px(12.0))
                        .text_color(theme.text)
                        .child(text.clone()),
                );
            }

            if let Some(params) = raw_params {
                let on_rev = std::rc::Rc::new(on_reveal);
                body = body.child(
                    div()
                        .id("reveal-raw-result-btn")
                        .h(px(28.0))
                        .px(px(10.0))
                        .rounded(px(4.0))
                        .bg(theme.surface_raised)
                        .hover(|s| s.bg(theme.element_hover))
                        .cursor_pointer()
                        .flex()
                        .items_center()
                        .gap(px(6.0))
                        .on_click(move |_, _, cx| {
                            on_rev(TrajectoryRawField::Result, params.clone(), cx)
                        })
                        .child(
                            icons::icon(icons::DETAILS_EYE)
                                .size(px(14.0))
                                .text_color(theme.text_muted),
                        )
                        .child(
                            div()
                                .text_size(px(12.0))
                                .text_color(theme.text)
                                .child("Reveal raw result"),
                        ),
                );
            }
        }
    }

    body
}

fn render_schema_tab(record: &TrajectoryRecord, theme: &Theme) -> Div {
    let mut body = div().flex().flex_col().gap(px(8.0));

    let schema_info = record.payload.as_ref().and_then(|p| p.schema_info.as_ref());

    if let Some(schema) = schema_info {
        body = body.child(
            div()
                .p(px(8.0))
                .rounded(px(4.0))
                .bg(theme.input_bg)
                .border_1()
                .border_color(theme.border)
                .text_size(px(12.0))
                .text_color(theme.code_text)
                .child(schema.clone()),
        );
    } else {
        body = body.child(
            div()
                .text_size(px(12.0))
                .text_color(theme.text_faint)
                .child("No schema information available for this record."),
        );
    }

    body
}

fn render_timing_tab(record: &TrajectoryRecord, theme: &Theme) -> Div {
    let mut body = div().flex().flex_col().gap(px(8.0));

    let Some(timing) = &record.timing else {
        return body.child(
            div()
                .text_size(px(12.0))
                .text_color(theme.text_faint)
                .child("No timing information recorded for this entry."),
        );
    };

    let in_flight = matches!(
        record.status,
        TrajectoryStatus::Running | TrajectoryStatus::Unsettled
    );

    let mode_str = match timing.mode {
        TrajectoryTimingMode::Recorded => "Recorded (timestamps available)",
        TrajectoryTimingMode::SequenceOnly => "Sequence-Only (equal-width geometry)",
    };

    body = body.child(render_timing_row("Mode", mode_str, theme, false));

    let started_str = timing
        .started_at
        .map(|t| t.to_rfc3339())
        .unwrap_or_else(|| {
            if in_flight {
                "Unsettled…".to_string()
            } else {
                "—".to_string()
            }
        });
    body = body.child(render_timing_row("Started At", &started_str, theme, false));

    let ended_str = timing.ended_at.map(|t| t.to_rfc3339()).unwrap_or_else(|| {
        if in_flight {
            "Unsettled…".to_string()
        } else {
            "—".to_string()
        }
    });
    body = body.child(render_timing_row("Ended At", &ended_str, theme, false));

    let duration_str = format_duration_or_unavailable(Some(timing));
    body = body.child(render_timing_row(
        "Duration",
        &duration_str,
        theme,
        duration_str == "—",
    ));

    let ttft_str = timing.ttft_ms.map(format_duration_ms).unwrap_or_else(|| {
        if in_flight {
            "Unsettled…".to_string()
        } else {
            "—".to_string()
        }
    });
    body = body.child(render_timing_row("TTFT", &ttft_str, theme, false));

    if timing.mode == TrajectoryTimingMode::SequenceOnly {
        body = body.child(
            div()
                .mt(px(8.0))
                .p(px(8.0))
                .rounded(px(4.0))
                .bg(theme.surface_raised)
                .text_size(px(11.5))
                .text_color(theme.text_muted)
                .child("Sequence-only mode uses equal-width chronology. Measured durations are intentionally unavailable."),
        );
    }

    body
}

fn render_timing_row(label: &str, value: &str, theme: &Theme, faint: bool) -> Div {
    div()
        .flex()
        .items_baseline()
        .justify_between()
        .py(px(4.0))
        .border_b_1()
        .border_color(theme.border)
        .child(
            div()
                .text_size(px(12.0))
                .text_color(theme.text_muted)
                .child(label.to_string()),
        )
        .child(
            div()
                .text_size(px(12.0))
                .font_weight(gpui::FontWeight::MEDIUM)
                .text_color(if faint { theme.text_faint } else { theme.text })
                .child(value.to_string()),
        )
}

fn format_record_kind(kind: &TrajectoryRecordKind) -> String {
    match kind {
        TrajectoryRecordKind::SessionStarted => "Session Started".to_string(),
        TrajectoryRecordKind::UserMessage => "User Message".to_string(),
        TrajectoryRecordKind::InputRequested => "Input Requested".to_string(),
        TrajectoryRecordKind::InputResolved => "Input Resolved".to_string(),
        TrajectoryRecordKind::Steered => "Steered".to_string(),
        TrajectoryRecordKind::ContextUsage => "Context Usage".to_string(),
        TrajectoryRecordKind::AvailableCommands => "Available Commands".to_string(),
        TrajectoryRecordKind::AssistantMessage => "Assistant Message".to_string(),
        TrajectoryRecordKind::Reasoning => "Reasoning".to_string(),
        TrajectoryRecordKind::WorkflowTask => "Workflow Task".to_string(),
        TrajectoryRecordKind::ToolCall { tool_name } => format!("Tool Call ({tool_name})"),
        TrajectoryRecordKind::ToolResult { tool_name } => format!("Tool Result ({tool_name})"),
        TrajectoryRecordKind::ToolDiff { tool_name } => format!("Tool Diff ({tool_name})"),
        TrajectoryRecordKind::Error => "Error".to_string(),
        TrajectoryRecordKind::Done => "Done".to_string(),
        TrajectoryRecordKind::Interrupted => "Interrupted".to_string(),
        TrajectoryRecordKind::Degraded => "Degraded".to_string(),
        TrajectoryRecordKind::Custom { name } => format!("Custom ({name})"),
    }
}

fn format_record_status(status: TrajectoryStatus) -> &'static str {
    match status {
        TrajectoryStatus::Running => "Running",
        TrajectoryStatus::Completed => "Completed",
        TrajectoryStatus::Error => "Error",
        TrajectoryStatus::Interrupted => "Interrupted",
        TrajectoryStatus::Unsettled => "Unsettled",
        TrajectoryStatus::Degraded => "Degraded",
        TrajectoryStatus::Unknown => "Unknown",
    }
}

fn format_unavailable_reason(reason: TrajectoryUnavailableReason) -> &'static str {
    match reason {
        TrajectoryUnavailableReason::NotFound => "Journal event not found",
        TrajectoryUnavailableReason::ForeignDevice => "Payload belongs to a remote device",
        TrajectoryUnavailableReason::ChatDeleted => "Chat was deleted",
        TrajectoryUnavailableReason::SourceCorrupt => "Source journal event is corrupt",
        TrajectoryUnavailableReason::SourceOversized => "Source journal event is oversized",
        TrajectoryUnavailableReason::MismatchedReference => {
            "Reference does not match current state"
        }
        TrajectoryUnavailableReason::UnsupportedSourceVersion => "Unsupported source version",
        TrajectoryUnavailableReason::StoreUnavailable => "Local store is unavailable",
    }
}

// ---------------------------------------------------------------------------
// Unit Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use zeron_proto::trajectory::{
        TrajectoryLane, TrajectoryPayloadPreview, TrajectoryRawField, TrajectoryRawRef,
        TrajectoryRecordId, TrajectoryResultPreview, TrajectoryTiming, TrajectoryUsage,
    };

    fn sample_record(status: TrajectoryStatus) -> TrajectoryRecord {
        TrajectoryRecord {
            id: TrajectoryRecordId::new("run-1", 10, 0),
            chat_id: "chat-123".to_string(),
            run_id: "run-1".to_string(),
            source_seq: 10,
            sub_seq: 0,
            lane: TrajectoryLane::Tools,
            kind: TrajectoryRecordKind::ToolCall {
                tool_name: "ReadFile".to_string(),
            },
            status,
            is_partial: false,
            title: "Read src/main.rs".to_string(),
            summary: "Read file contents".to_string(),
            turn_id: Some("run-1:t0".to_string()),
            step_id: Some("run-1:t0:s0".to_string()),
            call_id: Some("call-abc".to_string()),
            parent_tool_use_id: None,
            timing: Some(TrajectoryTiming::recorded(
                Some(Utc::now()),
                Some(Utc::now() + chrono::Duration::milliseconds(150)),
                Some(150),
                Some(25),
            )),
            usage: Some(TrajectoryUsage {
                input_tokens: Some(100),
                output_tokens: Some(50),
                total_tokens: Some(150),
                context_window: Some(200_000),
            }),
            payload: Some(TrajectoryPayloadPreview {
                summary: "Path: src/main.rs".to_string(),
                sanitized_text: Some("path: src/main.rs".to_string()),
                schema_info: Some("path: string".to_string()),
                raw_ref: Some(TrajectoryRawRef::new(
                    "chat-123",
                    10,
                    None,
                    Some("call-abc".to_string()),
                    TrajectoryRawField::Payload,
                )),
            }),
            result: Some(TrajectoryResultPreview {
                summary: "Completed (512 bytes)".to_string(),
                sanitized_text: Some("Output: 512 bytes".to_string()),
                is_error: false,
                exit_code: Some(0),
                raw_ref: Some(TrajectoryRawRef::new(
                    "chat-123",
                    10,
                    None,
                    Some("call-abc".to_string()),
                    TrajectoryRawField::Result,
                )),
            }),
            error_message: None,
            is_degraded: false,
        }
    }

    #[test]
    fn test_trajectory_inspector_available_tabs_only_returns_matching_data() {
        let mut rec = sample_record(TrajectoryStatus::Completed);
        assert_eq!(
            available_tabs(&rec),
            vec![
                InspectorTab::Summary,
                InspectorTab::Payload,
                InspectorTab::Result,
                InspectorTab::Schema,
                InspectorTab::Timing,
            ]
        );

        // Remove schema info
        if let Some(payload) = &mut rec.payload {
            payload.schema_info = None;
        }
        assert_eq!(
            available_tabs(&rec),
            vec![
                InspectorTab::Summary,
                InspectorTab::Payload,
                InspectorTab::Result,
                InspectorTab::Timing,
            ]
        );

        // Remove payload, result, timing
        rec.payload = None;
        rec.result = None;
        rec.timing = None;
        assert_eq!(available_tabs(&rec), vec![InspectorTab::Summary]);
    }

    #[test]
    fn test_trajectory_inspector_summary_fields_distinguishes_unavailable_from_unsettled() {
        // In-flight / Running record without turn_id / step_id / duration
        let mut running_rec = sample_record(TrajectoryStatus::Running);
        running_rec.turn_id = None;
        running_rec.step_id = None;
        running_rec.timing = None;
        running_rec.usage = None;
        running_rec.error_message = None;

        let fields = summary_fields(&running_rec);
        let turn_field = fields.iter().find(|f| f.label == "Turn").unwrap();
        let step_field = fields.iter().find(|f| f.label == "Step").unwrap();
        let duration_field = fields.iter().find(|f| f.label == "Duration").unwrap();
        let tokens_field = fields.iter().find(|f| f.label == "Tokens").unwrap();
        let error_field = fields.iter().find(|f| f.label == "Error").unwrap();

        assert_eq!(turn_field.value, SummaryValue::Unsettled);
        assert_eq!(step_field.value, SummaryValue::Unsettled);
        assert_eq!(duration_field.value, SummaryValue::Unsettled);
        assert_eq!(tokens_field.value, SummaryValue::Unsettled);
        assert_eq!(error_field.value, SummaryValue::Unsettled);

        // Completed record without turn_id / step_id / duration
        let mut completed_rec = sample_record(TrajectoryStatus::Completed);
        completed_rec.turn_id = None;
        completed_rec.step_id = None;
        completed_rec.timing = None;
        completed_rec.usage = None;
        completed_rec.error_message = None;

        let fields = summary_fields(&completed_rec);
        let turn_field = fields.iter().find(|f| f.label == "Turn").unwrap();
        let step_field = fields.iter().find(|f| f.label == "Step").unwrap();
        let duration_field = fields.iter().find(|f| f.label == "Duration").unwrap();
        let tokens_field = fields.iter().find(|f| f.label == "Tokens").unwrap();
        let error_field = fields.iter().find(|f| f.label == "Error").unwrap();

        assert_eq!(turn_field.value, SummaryValue::Unavailable);
        assert_eq!(step_field.value, SummaryValue::Unavailable);
        assert_eq!(duration_field.value, SummaryValue::Unavailable);
        assert_eq!(tokens_field.value, SummaryValue::Unavailable);
        assert_eq!(error_field.value, SummaryValue::Unavailable);

        // Invariant: no summary field produces empty string
        for f in fields {
            if let SummaryValue::Present(s) = f.value {
                assert!(!s.is_empty(), "field {} has empty string", f.label);
            }
        }
    }

    #[test]
    fn test_trajectory_inspector_sequence_only_timing_is_unavailable() {
        let mut rec = sample_record(TrajectoryStatus::Completed);
        rec.timing = Some(TrajectoryTiming::sequence_only());

        let fields = summary_fields(&rec);
        let duration_field = fields.iter().find(|f| f.label == "Duration").unwrap();
        assert_eq!(duration_field.value, SummaryValue::Unavailable);
    }

    #[test]
    fn test_trajectory_inspector_summary_fields_presents_valid_data() {
        let rec = sample_record(TrajectoryStatus::Completed);
        let fields = summary_fields(&rec);

        let rec_id_field = fields.iter().find(|f| f.label == "Record ID").unwrap();
        assert_eq!(
            rec_id_field.value,
            SummaryValue::Present("run-1:10:0".into())
        );

        let run_field = fields.iter().find(|f| f.label == "Run").unwrap();
        assert_eq!(run_field.value, SummaryValue::Present("run-1".into()));

        let turn_field = fields.iter().find(|f| f.label == "Turn").unwrap();
        assert_eq!(turn_field.value, SummaryValue::Present("run-1:t0".into()));

        let duration_field = fields.iter().find(|f| f.label == "Duration").unwrap();
        assert_eq!(duration_field.value, SummaryValue::Present("150ms".into()));

        let tokens_field = fields.iter().find(|f| f.label == "Tokens").unwrap();
        assert_eq!(
            tokens_field.value,
            SummaryValue::Present("150 tokens".into())
        );
    }

    #[test]
    fn test_trajectory_inspector_reveal_params_requires_raw_ref() {
        let rec_with_raw = sample_record(TrajectoryStatus::Completed);
        let payload_params = reveal_params("chat-123", &rec_with_raw, TrajectoryRawField::Payload);
        assert!(payload_params.is_some());
        let p = payload_params.unwrap();
        assert_eq!(p.chat_id, "chat-123");
        assert_eq!(p.source_seq, 10);
        assert_eq!(p.field, TrajectoryRawField::Payload);

        let result_params = reveal_params("chat-123", &rec_with_raw, TrajectoryRawField::Result);
        assert!(result_params.is_some());
        let r = result_params.unwrap();
        assert_eq!(r.chat_id, "chat-123");
        assert_eq!(r.source_seq, 10);
        assert_eq!(r.field, TrajectoryRawField::Result);

        // When record has no raw_ref
        let mut rec_no_raw = sample_record(TrajectoryStatus::Completed);
        if let Some(p) = &mut rec_no_raw.payload {
            p.raw_ref = None;
        }
        if let Some(r) = &mut rec_no_raw.result {
            r.raw_ref = None;
        }

        assert_eq!(
            reveal_params("chat-123", &rec_no_raw, TrajectoryRawField::Payload),
            None
        );
        assert_eq!(
            reveal_params("chat-123", &rec_no_raw, TrajectoryRawField::Result),
            None
        );
    }

    #[test]
    fn test_trajectory_inspector_layout_mode_threshold() {
        assert_eq!(layout_mode(px(599.0)), TrajectoryLayout::NarrowDetail);
        assert_eq!(layout_mode(px(600.0)), TrajectoryLayout::Split);
        assert_eq!(layout_mode(px(800.0)), TrajectoryLayout::Split);
    }
}
