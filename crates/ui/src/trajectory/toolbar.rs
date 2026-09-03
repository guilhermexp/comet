//! Toolbar controls for Trajectory preview.
//!
//! Provides top-level presentation controls: Duration mode toggle (Sequence vs. Recorded),
//! independent Turn/Call folding toggles, de-emphasizing search, and live edge catchup.

use std::rc::Rc;

use gpui::{
    AnyElement, App, Div, InteractiveElement, IntoElement, ParentElement, SharedString,
    StatefulInteractiveElement, Styled, div, px,
};

use crate::{
    icons,
    theme::Theme,
    trajectory::model::{DurationMode, TrajectoryViewModel},
};

/// Actions dispatched from the Trajectory toolbar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToolbarAction {
    /// Toggle between Sequence (equal-width) and Recorded (measured) duration modes.
    ToggleDuration,
    /// Toggle folding of all Turns across all Runs.
    ToggleTurns,
    /// Toggle folding of all Steps/Calls across all Turns.
    ToggleCalls,
    /// Update the trajectory search query. Note: search dims non-matching rows, never filters them out.
    Search(SharedString),
    /// Catch up to the live streaming edge.
    FollowLive,
    /// Clear the active search query.
    ClearSearch,
}

/// Pure state transition handler applying a toolbar action to the view model.
pub fn handle_toolbar_action(model: &mut TrajectoryViewModel, action: ToolbarAction) {
    match action {
        ToolbarAction::ToggleDuration => {
            let next = match model.duration_mode() {
                DurationMode::Sequence => DurationMode::Recorded,
                DurationMode::Recorded => DurationMode::Sequence,
            };
            model.set_duration_mode(next);
        }
        ToolbarAction::ToggleTurns => {
            let next = !model.turns_folded();
            model.set_turns_folded(next);
        }
        ToolbarAction::ToggleCalls => {
            let next = !model.calls_folded();
            model.set_calls_folded(next);
        }
        ToolbarAction::Search(query) => {
            model.set_search(query.as_ref());
        }
        ToolbarAction::ClearSearch => {
            model.set_search("");
        }
        ToolbarAction::FollowLive => {
            model.set_following_live(true);
        }
    }
}

/// Render the Trajectory toolbar.
pub fn render_toolbar(
    model: &TrajectoryViewModel,
    theme: &Theme,
    on_action: impl Fn(ToolbarAction, &mut App) + 'static,
) -> AnyElement {
    let on_action = Rc::new(on_action);

    div()
        .id("trajectory-toolbar")
        .h(px(36.0))
        .flex_none()
        .px(px(8.0))
        .flex()
        .items_center()
        .justify_between()
        .gap(px(8.0))
        .bg(theme.surface)
        .border_b_1()
        .border_color(theme.border)
        // Left controls group: Duration, Folds, Live follower
        .child(render_left_controls(model, theme, on_action.clone()))
        // Right controls group: Search
        .child(render_search_control(model, theme, on_action))
        .into_any_element()
}

fn render_left_controls(
    model: &TrajectoryViewModel,
    theme: &Theme,
    on_action: Rc<impl Fn(ToolbarAction, &mut App) + 'static>,
) -> Div {
    let mut left = div().flex().items_center().gap(px(6.0));

    // 1. Duration Mode Button
    let is_recorded = model.duration_mode() == DurationMode::Recorded;
    let on_toggle_duration = on_action.clone();
    left = left.child(
        div()
            .id("trajectory-toolbar-duration-toggle")
            .h(px(26.0))
            .px(px(8.0))
            .rounded(px(5.0))
            .flex()
            .items_center()
            .gap(px(5.0))
            .cursor_pointer()
            .bg(if is_recorded {
                theme.surface_raised
            } else {
                gpui::transparent_black()
            })
            .hover(|s| s.bg(theme.element_hover))
            .text_size(px(12.0))
            .text_color(if is_recorded {
                theme.text
            } else {
                theme.text_muted
            })
            .on_click(move |_, _, cx| on_toggle_duration(ToolbarAction::ToggleDuration, cx))
            .child(
                icons::icon(icons::CLOCK_CIRCLE)
                    .size(px(13.0))
                    .text_color(if is_recorded {
                        theme.accent
                    } else {
                        theme.text_muted
                    }),
            )
            .child(if is_recorded { "Recorded" } else { "Sequence" }),
    );

    // 2. Turns Fold Button
    let turns_folded = model.turns_folded();
    let on_toggle_turns = on_action.clone();
    left =
        left.child(
            div()
                .id("trajectory-toolbar-turns-toggle")
                .h(px(26.0))
                .px(px(8.0))
                .rounded(px(5.0))
                .flex()
                .items_center()
                .gap(px(5.0))
                .cursor_pointer()
                .bg(if turns_folded {
                    theme.surface_raised
                } else {
                    gpui::transparent_black()
                })
                .hover(|s| s.bg(theme.element_hover))
                .text_size(px(12.0))
                .text_color(if turns_folded {
                    theme.text
                } else {
                    theme.text_muted
                })
                .on_click(move |_, _, cx| on_toggle_turns(ToolbarAction::ToggleTurns, cx))
                .child(icons::icon(icons::FOLD_VERTICAL).size(px(13.0)).text_color(
                    if turns_folded {
                        theme.accent
                    } else {
                        theme.text_muted
                    },
                ))
                .child(if turns_folded {
                    "Turns Folded"
                } else {
                    "Fold Turns"
                }),
        );

    // 3. Calls Fold Button
    let calls_folded = model.calls_folded();
    let on_toggle_calls = on_action.clone();
    left = left.child(
        div()
            .id("trajectory-toolbar-calls-toggle")
            .h(px(26.0))
            .px(px(8.0))
            .rounded(px(5.0))
            .flex()
            .items_center()
            .gap(px(5.0))
            .cursor_pointer()
            .bg(if calls_folded {
                theme.surface_raised
            } else {
                gpui::transparent_black()
            })
            .hover(|s| s.bg(theme.element_hover))
            .text_size(px(12.0))
            .text_color(if calls_folded {
                theme.text
            } else {
                theme.text_muted
            })
            .on_click(move |_, _, cx| on_toggle_calls(ToolbarAction::ToggleCalls, cx))
            .child(
                icons::icon(icons::TERMINAL)
                    .size(px(13.0))
                    .text_color(if calls_folded {
                        theme.accent
                    } else {
                        theme.text_muted
                    }),
            )
            .child(if calls_folded {
                "Calls Folded"
            } else {
                "Fold Calls"
            }),
    );

    // 4. Live Edge Follower (shown if pending updates or not following)
    if !model.following_live() || model.pending_live() > 0 {
        let pending = model.pending_live();
        let on_follow = on_action;
        left = left.child(
            div()
                .id("trajectory-toolbar-follow-live")
                .h(px(26.0))
                .px(px(8.0))
                .rounded(px(5.0))
                .flex()
                .items_center()
                .gap(px(5.0))
                .cursor_pointer()
                .bg(theme.accent_wash)
                .hover(|s| s.bg(theme.element_hover))
                .text_size(px(12.0))
                .font_weight(gpui::FontWeight::MEDIUM)
                .text_color(theme.accent)
                .on_click(move |_, _, cx| on_follow(ToolbarAction::FollowLive, cx))
                .child(
                    icons::icon(icons::ARROW_DOWN)
                        .size(px(13.0))
                        .text_color(theme.accent),
                )
                .child(if pending > 0 {
                    format!("{pending} new")
                } else {
                    "Follow Live".to_string()
                }),
        );
    }

    left
}

fn render_search_control(
    model: &TrajectoryViewModel,
    theme: &Theme,
    on_action: Rc<impl Fn(ToolbarAction, &mut App) + 'static>,
) -> AnyElement {
    let query = model.search();
    let has_query = !query.is_empty();

    let mut search_box = div()
        .id("trajectory-toolbar-search")
        .h(px(26.0))
        .w(px(200.0))
        .px(px(6.0))
        .rounded(px(5.0))
        .flex()
        .items_center()
        .gap(px(6.0))
        .bg(theme.input_bg)
        .border_1()
        .border_color(theme.border)
        .child(
            icons::icon(icons::MAGNIFER)
                .size(px(13.0))
                .text_color(theme.text_muted),
        )
        .child(
            div()
                .flex_1()
                .text_size(px(12.0))
                .text_color(if has_query {
                    theme.text
                } else {
                    theme.text_faint
                })
                .child(if has_query {
                    query.to_string()
                } else {
                    "Search trajectory…".to_string()
                }),
        );

    if has_query {
        let on_clear = on_action;
        search_box = search_box.child(
            div()
                .id("trajectory-toolbar-search-clear")
                .size(px(16.0))
                .rounded(px(3.0))
                .flex()
                .items_center()
                .justify_center()
                .cursor_pointer()
                .hover(|s| s.bg(theme.element_hover))
                .on_click(move |_, _, cx| on_clear(ToolbarAction::ClearSearch, cx))
                .child(
                    icons::icon(icons::CLOSE)
                        .size(px(11.0))
                        .text_color(theme.text_muted),
                ),
        );
    }

    search_box.into_any_element()
}

// ---------------------------------------------------------------------------
// Unit Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use zeron_proto::trajectory::{
        TrajectoryRecord, TrajectoryRecordId, TrajectoryRecordKind, TrajectoryStatus,
    };
    use zeron_rpc::TrajectoryWatchItem;

    fn test_model() -> TrajectoryViewModel {
        let mut model = TrajectoryViewModel::new("chat-test");
        let rec = TrajectoryRecord {
            id: TrajectoryRecordId::new("run-1", 1, 0),
            chat_id: "chat-test".to_string(),
            run_id: "run-1".to_string(),
            source_seq: 1,
            sub_seq: 0,
            lane: zeron_proto::trajectory::TrajectoryLane::Input,
            kind: TrajectoryRecordKind::UserMessage,
            status: TrajectoryStatus::Completed,
            is_partial: false,
            title: "Hello world".to_string(),
            summary: "User message".to_string(),
            turn_id: Some("run-1:t0".to_string()),
            step_id: Some("run-1:t0:s0".to_string()),
            call_id: None,
            parent_tool_use_id: None,
            timing: None,
            usage: None,
            payload: None,
            result: None,
            error_message: None,
            is_degraded: false,
        };
        model.apply_watch_item(TrajectoryWatchItem::Snapshot {
            records: vec![rec],
            watermark: None,
            degraded: vec![],
            has_more: false,
        });
        model
    }

    #[test]
    fn test_trajectory_toolbar_toggle_duration() {
        let mut model = test_model();
        assert_eq!(model.duration_mode(), DurationMode::Sequence);

        handle_toolbar_action(&mut model, ToolbarAction::ToggleDuration);
        assert_eq!(model.duration_mode(), DurationMode::Recorded);

        handle_toolbar_action(&mut model, ToolbarAction::ToggleDuration);
        assert_eq!(model.duration_mode(), DurationMode::Sequence);
    }

    #[test]
    fn test_trajectory_toolbar_independent_folds() {
        let mut model = test_model();
        assert!(!model.turns_folded());
        assert!(!model.calls_folded());

        // Toggle turns
        handle_toolbar_action(&mut model, ToolbarAction::ToggleTurns);
        assert!(model.turns_folded());
        assert!(!model.calls_folded());

        // Toggle calls
        handle_toolbar_action(&mut model, ToolbarAction::ToggleCalls);
        assert!(model.turns_folded());
        assert!(model.calls_folded());

        // Toggle turns back
        handle_toolbar_action(&mut model, ToolbarAction::ToggleTurns);
        assert!(!model.turns_folded());
        assert!(model.calls_folded());
    }

    #[test]
    fn test_trajectory_toolbar_search_dims_without_filtering() {
        let mut model = test_model();
        let initial_row_count = model.rows().len();

        handle_toolbar_action(&mut model, ToolbarAction::Search("nomatch".into()));
        assert_eq!(model.search(), "nomatch");
        // Invariant: total rows remains unchanged (no filtering)
        assert_eq!(model.rows().len(), initial_row_count);
        // Rows that don't match are dimmed
        assert!(model.rows().iter().all(|r| r.dimmed));

        // Matching query
        handle_toolbar_action(&mut model, ToolbarAction::Search("Hello".into()));
        assert_eq!(model.search(), "Hello");
        assert_eq!(model.rows().len(), initial_row_count);
    }

    #[test]
    fn test_trajectory_toolbar_clear_search() {
        let mut model = test_model();
        handle_toolbar_action(&mut model, ToolbarAction::Search("search_term".into()));
        assert_eq!(model.search(), "search_term");

        handle_toolbar_action(&mut model, ToolbarAction::ClearSearch);
        assert_eq!(model.search(), "");
    }

    #[test]
    fn test_trajectory_toolbar_follow_live() {
        let mut model = test_model();
        model.set_following_live(false);

        // Apply deltas while not following live increments pending_live
        let rec2 = TrajectoryRecord {
            id: TrajectoryRecordId::new("run-1", 2, 0),
            chat_id: "chat-test".to_string(),
            run_id: "run-1".to_string(),
            source_seq: 2,
            sub_seq: 0,
            lane: zeron_proto::trajectory::TrajectoryLane::Model,
            kind: TrajectoryRecordKind::AssistantMessage,
            status: TrajectoryStatus::Completed,
            is_partial: false,
            title: "Reply".to_string(),
            summary: "Assistant message".to_string(),
            turn_id: Some("run-1:t0".to_string()),
            step_id: Some("run-1:t0:s0".to_string()),
            call_id: None,
            parent_tool_use_id: None,
            timing: None,
            usage: None,
            payload: None,
            result: None,
            error_message: None,
            is_degraded: false,
        };
        model.apply_watch_item(TrajectoryWatchItem::Deltas {
            records: vec![rec2],
            watermark: None,
        });

        assert!(!model.following_live());
        assert_eq!(model.pending_live(), 1);

        handle_toolbar_action(&mut model, ToolbarAction::FollowLive);
        assert!(model.following_live());
        assert_eq!(model.pending_live(), 0);
    }
}
