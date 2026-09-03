//! Pure timeline geometry, layout calculation, hit testing, and GPUI rendering
//! for Chat Trajectory preview.
//!
//! Three fixed horizontal lanes represent execution chronology:
//! - `TrajectoryLane::Input` (system prompt, user messages, context usage, available commands)
//! - `TrajectoryLane::Model` (assistant messages, reasoning, thoughts)
//! - `TrajectoryLane::Tools` (tool calls, tool results, diffs, and subagents)
//!
//! All geometric calculations are pure and deterministic: no I/O, no database access.

use std::collections::HashSet;

use chrono::{DateTime, Utc};
use gpui::{
    AnyElement, App, ElementId, InteractiveElement, IntoElement, ParentElement, SharedString,
    StatefulInteractiveElement, Styled, div, px,
};
use zeron_proto::trajectory::{
    TrajectoryLane, TrajectoryRecord, TrajectoryRecordId, TrajectoryStatus, TrajectoryTiming,
    TrajectoryTimingMode,
};

use super::model::{DurationMode, TrajectoryViewModel};
use crate::theme::Theme;

/// Fixed timeline lane order.
pub const LANES: [TrajectoryLane; 3] = [
    TrajectoryLane::Input,
    TrajectoryLane::Model,
    TrajectoryLane::Tools,
];

/// A single horizontal span occupying a segment in one timeline lane.
#[derive(Debug, Clone, PartialEq)]
pub struct LaneSpan {
    pub record: TrajectoryRecordId,
    pub start_fraction: f32,
    pub width_fraction: f32,
    pub status: TrajectoryStatus,
    pub is_error: bool,
    pub measured: bool,
}

/// Geometric layout across the three timeline lanes.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct LaneLayout {
    pub lanes: [Vec<LaneSpan>; 3],
}

impl LaneLayout {
    pub fn spans_for_lane(&self, lane: TrajectoryLane) -> &[LaneSpan] {
        match lane {
            TrajectoryLane::Input => &self.lanes[0],
            TrajectoryLane::Model => &self.lanes[1],
            TrajectoryLane::Tools | TrajectoryLane::Unknown => &self.lanes[2],
        }
    }

    fn push_span(&mut self, lane: TrajectoryLane, span: LaneSpan) {
        match lane {
            TrajectoryLane::Input => self.lanes[0].push(span),
            TrajectoryLane::Model => self.lanes[1].push(span),
            TrajectoryLane::Tools | TrajectoryLane::Unknown => self.lanes[2].push(span),
        }
    }
}

/// Compute the pure geometric layout of all records in the model.
pub fn lane_layout(model: &TrajectoryViewModel) -> LaneLayout {
    let records: Vec<&TrajectoryRecord> = model
        .runs()
        .iter()
        .flat_map(|r| r.turns.iter())
        .flat_map(|t| t.steps.iter())
        .flat_map(|s| s.records.iter())
        .collect();

    if records.is_empty() {
        return LaneLayout::default();
    }

    let mut layout = LaneLayout::default();

    if model.duration_mode() == DurationMode::Recorded {
        if let Some(recorded_layout) = compute_recorded_layout(&records) {
            return recorded_layout;
        }
    }

    // Sequence Mode or Fallback: equal widths
    let count = records.len();
    let width_fraction = 1.0 / count as f32;

    for (idx, record) in records.iter().enumerate() {
        let start_fraction = idx as f32 / count as f32;
        let effective_status = record.effective_status();
        let is_error = effective_status.is_error();

        layout.push_span(
            record.lane,
            LaneSpan {
                record: record.id.clone(),
                start_fraction,
                width_fraction,
                status: effective_status,
                is_error,
                measured: false,
            },
        );
    }

    layout
}

/// End instant of a recorded span. `None` when a corrupt `duration_ms` would
/// overflow the clock, so the caller falls back to sequence layout instead of
/// panicking mid-render.
fn span_end(timing: &TrajectoryTiming, started_at: DateTime<Utc>) -> Option<DateTime<Utc>> {
    if let Some(ended_at) = timing.ended_at {
        return Some(ended_at);
    }
    match timing.effective_duration_ms() {
        Some(ms) => {
            let delta = chrono::TimeDelta::try_milliseconds(i64::try_from(ms).ok()?)?;
            started_at.checked_add_signed(delta)
        }
        None => Some(started_at),
    }
}

fn compute_recorded_layout(records: &[&TrajectoryRecord]) -> Option<LaneLayout> {
    // Validate all records have recorded timing with valid started_at
    for (i, record) in records.iter().enumerate() {
        let timing = record.timing.as_ref()?;
        if timing.mode != TrajectoryTimingMode::Recorded {
            return None;
        }
        let started_at = timing.started_at?;
        let end_time = span_end(timing, started_at)?;

        if end_time < started_at {
            return None;
        }

        if i > 0 {
            let prev_timing = records[i - 1].timing.as_ref()?;
            let prev_start = prev_timing.started_at?;
            if started_at < prev_start {
                return None;
            }
        }
    }

    let min_start = records[0].timing.as_ref()?.started_at?;
    let mut max_end = min_start;
    for record in records {
        let timing = record.timing.as_ref()?;
        let started_at = timing.started_at?;
        let end_time = span_end(timing, started_at)?;
        if end_time > max_end {
            max_end = end_time;
        }
    }

    let total_ms = (max_end - min_start).num_milliseconds() as f32;
    if total_ms <= 0.0 {
        if records.len() == 1 {
            let mut layout = LaneLayout::default();
            let record = records[0];
            let effective_status = record.effective_status();
            layout.push_span(
                record.lane,
                LaneSpan {
                    record: record.id.clone(),
                    start_fraction: 0.0,
                    width_fraction: 1.0,
                    status: effective_status,
                    is_error: effective_status.is_error(),
                    measured: true,
                },
            );
            return Some(layout);
        } else {
            return None;
        }
    }

    let mut layout = LaneLayout::default();
    let count = records.len();
    let min_unit = (1.0 / (count as f32 * 10.0)).clamp(0.002, 0.05);

    for record in records {
        let timing = record.timing.as_ref()?;
        let started_at = timing.started_at?;
        let end_time = span_end(timing, started_at)?;

        let offset_ms = (started_at - min_start).num_milliseconds().max(0) as f32;
        let duration_ms = (end_time - started_at).num_milliseconds().max(0) as f32;

        let start_fraction = (offset_ms / total_ms).clamp(0.0, 1.0);
        let raw_width = (duration_ms / total_ms).clamp(0.0, 1.0 - start_fraction);
        let width_fraction = if raw_width <= 0.0001 {
            min_unit.min(1.0 - start_fraction).max(0.001)
        } else {
            raw_width.min(1.0 - start_fraction)
        };

        let effective_status = record.effective_status();
        let is_error = effective_status.is_error();

        layout.push_span(
            record.lane,
            LaneSpan {
                record: record.id.clone(),
                start_fraction,
                width_fraction,
                status: effective_status,
                is_error,
                measured: true,
            },
        );
    }

    Some(layout)
}

/// Render the pure timeline lanes in GPUI.
pub fn render_timeline<S>(model: &TrajectoryViewModel, theme: &Theme, on_select: S) -> AnyElement
where
    S: Fn(TrajectoryRecordId, &mut App) + Clone + 'static,
{
    let layout = lane_layout(model);

    let mut root = div()
        .id(ElementId::from(SharedString::from("trajectory-timeline")))
        .w_full()
        .flex()
        .flex_col()
        .gap(px(4.0))
        .py(px(6.0))
        .px(px(8.0))
        .bg(theme.bg)
        .border_b_1()
        .border_color(theme.border);

    let selected_record_id = model.selected_record().map(|r| &r.id);
    let dimmed: HashSet<&TrajectoryRecordId> = model
        .rows()
        .iter()
        .filter(|row| row.dimmed)
        .filter_map(|row| row.record.as_ref())
        .collect();

    for lane in LANES {
        let spans = layout.spans_for_lane(lane);
        let lane_label = match lane {
            TrajectoryLane::Input => "Input",
            TrajectoryLane::Model => "Model",
            TrajectoryLane::Tools | TrajectoryLane::Unknown => "Tools",
        };

        let mut track = div()
            .id(ElementId::from(SharedString::from(format!(
                "timeline-track-{}",
                lane_label.to_lowercase()
            ))))
            .relative()
            .flex_1()
            .h(px(14.0))
            .bg(theme.surface)
            .rounded(px(3.0))
            .overflow_hidden();

        for span in spans {
            let is_selected = selected_record_id == Some(&span.record);
            let is_dimmed = dimmed.contains(&span.record);

            let span_bg = if span.is_error {
                theme.danger
            } else {
                match span.status {
                    TrajectoryStatus::Running => theme.accent,
                    TrajectoryStatus::Completed => theme.accent_strong,
                    TrajectoryStatus::Unsettled | TrajectoryStatus::Unknown => theme.text_muted,
                    TrajectoryStatus::Error
                    | TrajectoryStatus::Interrupted
                    | TrajectoryStatus::Degraded => theme.danger,
                }
            };

            let record_id = span.record.clone();
            let mut span_el = div()
                .id(ElementId::from(SharedString::from(format!(
                    "timeline-span-{}",
                    span.record.key()
                ))))
                .absolute()
                .left(gpui::relative(span.start_fraction))
                .w(gpui::relative(span.width_fraction))
                .h_full()
                .bg(span_bg)
                .rounded(px(2.0))
                .cursor_pointer()
                .on_click({
                    let on_select = on_select.clone();
                    move |_, _, cx| on_select(record_id.clone(), cx)
                });

            if is_selected {
                span_el = span_el.border_1().border_color(theme.border_strong);
            }
            if is_dimmed {
                span_el = span_el.opacity(0.35);
            }

            track = track.child(span_el);
        }

        let lane_row = div()
            .id(ElementId::from(SharedString::from(format!(
                "timeline-lane-{}",
                lane_label.to_lowercase()
            ))))
            .flex()
            .items_center()
            .gap(px(8.0))
            .w_full()
            .child(
                div()
                    .w(px(44.0))
                    .flex_none()
                    .text_size(px(10.0))
                    .text_color(theme.text_muted)
                    .child(lane_label),
            )
            .child(track);

        root = root.child(lane_row);
    }

    root.into_any_element()
}

// ---------------------------------------------------------------------------
// Unit Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use chrono::{TimeZone, Utc};
    use zeron_proto::trajectory::{
        TrajectoryPayloadPreview, TrajectoryRecord, TrajectoryRecordId, TrajectoryRecordKind,
        TrajectoryResultPreview, TrajectoryStatus, TrajectoryTiming,
    };
    use zeron_rpc::TrajectoryWatchItem;

    use super::*;

    fn make_test_record(
        source_seq: u64,
        sub_seq: u32,
        lane: TrajectoryLane,
        kind: TrajectoryRecordKind,
        status: TrajectoryStatus,
        is_error: bool,
        timing: Option<TrajectoryTiming>,
    ) -> TrajectoryRecord {
        TrajectoryRecord {
            id: TrajectoryRecordId::new("run-1", source_seq, sub_seq),
            chat_id: "chat-1".into(),
            run_id: "run-1".into(),
            source_seq,
            sub_seq,
            lane,
            kind,
            status,
            is_partial: false,
            title: format!("Record {source_seq}.{sub_seq}"),
            summary: "Summary".into(),
            turn_id: Some("run-1:t1".into()),
            step_id: Some("run-1:t1:s1".into()),
            call_id: None,
            parent_tool_use_id: None,
            timing,
            usage: None,
            payload: Some(TrajectoryPayloadPreview {
                summary: "payload".into(),
                sanitized_text: None,
                schema_info: None,
                raw_ref: None,
            }),
            result: if is_error {
                Some(TrajectoryResultPreview {
                    summary: "error result".into(),
                    sanitized_text: None,
                    raw_ref: None,
                    is_error: true,
                    exit_code: Some(1),
                })
            } else {
                None
            },
            error_message: if is_error {
                Some("Failed execution".into())
            } else {
                None
            },
            is_degraded: false,
        }
    }

    #[test]
    fn test_trajectory_timeline_fixed_lanes_classification_and_error_preservation() {
        let mut model = TrajectoryViewModel::new("chat-1");

        let rec_input = make_test_record(
            1,
            0,
            TrajectoryLane::Input,
            TrajectoryRecordKind::UserMessage,
            TrajectoryStatus::Completed,
            false,
            None,
        );
        let rec_model = make_test_record(
            2,
            0,
            TrajectoryLane::Model,
            TrajectoryRecordKind::AssistantMessage,
            TrajectoryStatus::Completed,
            false,
            None,
        );
        let rec_tools_err = make_test_record(
            3,
            0,
            TrajectoryLane::Tools,
            TrajectoryRecordKind::ToolCall {
                tool_name: "bash".into(),
            },
            TrajectoryStatus::Error,
            true,
            None,
        );

        model.apply_watch_item(TrajectoryWatchItem::Snapshot {
            records: vec![rec_input, rec_model, rec_tools_err],
            watermark: None,
            degraded: Vec::new(),
            has_more: false,
        });

        let layout = lane_layout(&model);

        // LANES constants must be Input, Model, Tools in exact order
        assert_eq!(LANES[0], TrajectoryLane::Input);
        assert_eq!(LANES[1], TrajectoryLane::Model);
        assert_eq!(LANES[2], TrajectoryLane::Tools);

        // Verify Input lane contains only Input record
        let input_spans = layout.spans_for_lane(TrajectoryLane::Input);
        assert_eq!(input_spans.len(), 1);
        assert_eq!(input_spans[0].record.source_seq, 1);
        assert!(!input_spans[0].is_error);

        // Verify Model lane contains only Model record
        let model_spans = layout.spans_for_lane(TrajectoryLane::Model);
        assert_eq!(model_spans.len(), 1);
        assert_eq!(model_spans[0].record.source_seq, 2);
        assert!(!model_spans[0].is_error);

        // Verify Tools lane contains Tools record, error state is preserved without altering lane
        let tools_spans = layout.spans_for_lane(TrajectoryLane::Tools);
        assert_eq!(tools_spans.len(), 1);
        assert_eq!(tools_spans[0].record.source_seq, 3);
        assert!(tools_spans[0].is_error);
        assert_eq!(tools_spans[0].status, TrajectoryStatus::Error);
    }

    #[test]
    fn test_trajectory_timeline_sequence_mode_equal_widths() {
        let mut model = TrajectoryViewModel::new("chat-1");
        model.set_duration_mode(DurationMode::Sequence);

        let records = vec![
            make_test_record(
                1,
                0,
                TrajectoryLane::Input,
                TrajectoryRecordKind::UserMessage,
                TrajectoryStatus::Completed,
                false,
                None,
            ),
            make_test_record(
                2,
                0,
                TrajectoryLane::Model,
                TrajectoryRecordKind::AssistantMessage,
                TrajectoryStatus::Completed,
                false,
                None,
            ),
            make_test_record(
                3,
                0,
                TrajectoryLane::Tools,
                TrajectoryRecordKind::ToolCall {
                    tool_name: "test".into(),
                },
                TrajectoryStatus::Completed,
                false,
                None,
            ),
            make_test_record(
                4,
                0,
                TrajectoryLane::Model,
                TrajectoryRecordKind::AssistantMessage,
                TrajectoryStatus::Completed,
                false,
                None,
            ),
        ];

        model.apply_watch_item(TrajectoryWatchItem::Snapshot {
            records,
            watermark: None,
            degraded: Vec::new(),
            has_more: false,
        });

        let layout = lane_layout(&model);

        let input_span = &layout.spans_for_lane(TrajectoryLane::Input)[0];
        assert_eq!(input_span.start_fraction, 0.0);
        assert_eq!(input_span.width_fraction, 0.25);
        assert!(!input_span.measured);

        let model_spans = layout.spans_for_lane(TrajectoryLane::Model);
        assert_eq!(model_spans[0].start_fraction, 0.25);
        assert_eq!(model_spans[0].width_fraction, 0.25);
        assert!(!model_spans[0].measured);

        assert_eq!(model_spans[1].start_fraction, 0.75);
        assert_eq!(model_spans[1].width_fraction, 0.25);
        assert!(!model_spans[1].measured);

        let tool_span = &layout.spans_for_lane(TrajectoryLane::Tools)[0];
        assert_eq!(tool_span.start_fraction, 0.5);
        assert_eq!(tool_span.width_fraction, 0.25);
        assert!(!tool_span.measured);
    }

    #[test]
    fn test_trajectory_timeline_recorded_mode_timing_validation_and_fallback() {
        let mut model = TrajectoryViewModel::new("chat-1");
        model.set_duration_mode(DurationMode::Recorded);

        let t0 = Utc.with_ymd_and_hms(2026, 9, 2, 12, 0, 0).unwrap();
        let t1 = Utc.with_ymd_and_hms(2026, 9, 2, 12, 0, 1).unwrap();
        let t2 = Utc.with_ymd_and_hms(2026, 9, 2, 12, 0, 3).unwrap();
        let t3 = Utc.with_ymd_and_hms(2026, 9, 2, 12, 0, 4).unwrap();

        // Valid recorded timing: total duration = 4s (4000ms)
        // Record 1: t0..t1 (1s = 0.25 width, start = 0.0)
        // Record 2: t1..t2 (2s = 0.50 width, start = 0.25)
        // Record 3: t2..t3 (1s = 0.25 width, start = 0.75)
        let records = vec![
            make_test_record(
                1,
                0,
                TrajectoryLane::Input,
                TrajectoryRecordKind::UserMessage,
                TrajectoryStatus::Completed,
                false,
                Some(TrajectoryTiming::recorded(
                    Some(t0),
                    Some(t1),
                    Some(1000),
                    None,
                )),
            ),
            make_test_record(
                2,
                0,
                TrajectoryLane::Model,
                TrajectoryRecordKind::AssistantMessage,
                TrajectoryStatus::Completed,
                false,
                Some(TrajectoryTiming::recorded(
                    Some(t1),
                    Some(t2),
                    Some(2000),
                    Some(500),
                )),
            ),
            make_test_record(
                3,
                0,
                TrajectoryLane::Tools,
                TrajectoryRecordKind::ToolCall {
                    tool_name: "bash".into(),
                },
                TrajectoryStatus::Completed,
                false,
                Some(TrajectoryTiming::recorded(
                    Some(t2),
                    Some(t3),
                    Some(1000),
                    None,
                )),
            ),
        ];

        model.apply_watch_item(TrajectoryWatchItem::Snapshot {
            records,
            watermark: None,
            degraded: Vec::new(),
            has_more: false,
        });

        let layout = lane_layout(&model);
        let input_span = &layout.spans_for_lane(TrajectoryLane::Input)[0];
        let model_span = &layout.spans_for_lane(TrajectoryLane::Model)[0];
        let tools_span = &layout.spans_for_lane(TrajectoryLane::Tools)[0];

        assert!(input_span.measured);
        assert!(model_span.measured);
        assert!(tools_span.measured);

        assert!((input_span.start_fraction - 0.0).abs() < 1e-4);
        assert!((input_span.width_fraction - 0.25).abs() < 1e-4);

        assert!((model_span.start_fraction - 0.25).abs() < 1e-4);
        assert!((model_span.width_fraction - 0.50).abs() < 1e-4);

        assert!((tools_span.start_fraction - 0.75).abs() < 1e-4);
        assert!((tools_span.width_fraction - 0.25).abs() < 1e-4);

        // Now test fallback when one record lacks timing or has SequenceOnly mode
        let mut model_degraded = TrajectoryViewModel::new("chat-1");
        model_degraded.set_duration_mode(DurationMode::Recorded);

        let records_degraded = vec![
            make_test_record(
                1,
                0,
                TrajectoryLane::Input,
                TrajectoryRecordKind::UserMessage,
                TrajectoryStatus::Completed,
                false,
                Some(TrajectoryTiming::sequence_only()), // Missing recorded timing
            ),
            make_test_record(
                2,
                0,
                TrajectoryLane::Model,
                TrajectoryRecordKind::AssistantMessage,
                TrajectoryStatus::Completed,
                false,
                Some(TrajectoryTiming::recorded(
                    Some(t1),
                    Some(t2),
                    Some(2000),
                    None,
                )),
            ),
        ];

        model_degraded.apply_watch_item(TrajectoryWatchItem::Snapshot {
            records: records_degraded,
            watermark: None,
            degraded: Vec::new(),
            has_more: false,
        });

        let degraded_layout = lane_layout(&model_degraded);
        let span1 = &degraded_layout.spans_for_lane(TrajectoryLane::Input)[0];
        let span2 = &degraded_layout.spans_for_lane(TrajectoryLane::Model)[0];

        // Must fallback to equal widths and measured = false
        assert!(!span1.measured);
        assert!(!span2.measured);
        assert_eq!(span1.width_fraction, 0.5);
        assert_eq!(span2.width_fraction, 0.5);
    }

    #[test]
    fn test_trajectory_timeline_no_zero_width_for_single_record() {
        let mut model = TrajectoryViewModel::new("chat-1");
        model.apply_watch_item(TrajectoryWatchItem::Snapshot {
            records: vec![make_test_record(
                1,
                0,
                TrajectoryLane::Input,
                TrajectoryRecordKind::UserMessage,
                TrajectoryStatus::Completed,
                false,
                None,
            )],
            watermark: None,
            degraded: Vec::new(),
            has_more: false,
        });

        let layout = lane_layout(&model);
        let span = &layout.spans_for_lane(TrajectoryLane::Input)[0];
        assert_eq!(span.start_fraction, 0.0);
        assert_eq!(span.width_fraction, 1.0);
    }

    /// A corrupt `duration_ms` must degrade to sequence layout, never panic
    /// the render with a clock overflow.
    #[test]
    fn test_trajectory_timeline_recorded_mode_overflowing_duration_falls_back() {
        let mut model = TrajectoryViewModel::new("chat-1");
        model.set_duration_mode(DurationMode::Recorded);
        let start = Utc::now();
        let mut corrupt = make_test_record(
            1,
            0,
            TrajectoryLane::Input,
            TrajectoryRecordKind::UserMessage,
            TrajectoryStatus::Completed,
            false,
            Some(TrajectoryTiming::recorded(
                Some(start),
                None,
                Some(u64::MAX),
                None,
            )),
        );
        corrupt.timing.as_mut().unwrap().ended_at = None;
        let sane = make_test_record(
            2,
            0,
            TrajectoryLane::Model,
            TrajectoryRecordKind::AssistantMessage,
            TrajectoryStatus::Completed,
            false,
            Some(TrajectoryTiming::recorded(
                Some(start + chrono::TimeDelta::seconds(1)),
                Some(start + chrono::TimeDelta::seconds(2)),
                None,
                None,
            )),
        );
        model.apply_watch_item(TrajectoryWatchItem::Snapshot {
            records: vec![corrupt, sane],
            watermark: None,
            degraded: Vec::new(),
            has_more: false,
        });

        let layout = lane_layout(&model);
        let span = &layout.spans_for_lane(TrajectoryLane::Input)[0];
        assert!(
            !span.measured,
            "overflowing duration must fall back to sequence layout"
        );
        assert_eq!(span.width_fraction, 0.5);
    }
}
