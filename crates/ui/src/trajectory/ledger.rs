//! Pure ledger geometry, scrolling target calculations, viewport anchoring,
//! and virtualized GPUI rendering for Chat Trajectory preview.
//!
//! All geometric calculations are pure: no I/O, no database access.

use gpui::{
    AnyElement, App, ElementId, InteractiveElement, IntoElement, ParentElement, Pixels, Role,
    SharedString, StatefulInteractiveElement, Styled, UniformListScrollHandle, div, px,
    uniform_list,
};
use zeron_proto::trajectory::TrajectoryLane;

use super::model::{LedgerRow, LedgerRowKind, RowId, TrajectoryViewModel};
use crate::{icons, theme::Theme};

/// Fixed ledger row height in pixels for virtualized rendering.
/// Invariant: No state (loading, error, reveal) is allowed to alter this height.
pub const ROW_HEIGHT: Pixels = px(26.0);

/// Visible viewport bounds in virtualized row units.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LedgerViewport {
    pub first_visible: usize,
    pub visible_count: usize,
}

/// Compute the target `first_visible` index to bring a selected row into the viewport.
///
/// Returns `None` if:
/// - The row `id` is not found in `rows`.
/// - The row is already fully visible within `viewport`.
///
/// Returns `Some(target_index)` when scrolling is necessary:
/// - If the row is above the viewport (`target_idx < first_visible`), returns `Some(target_idx)`.
/// - If the row is below the viewport (`target_idx >= first_visible + visible_count`),
///   returns `Some(target_idx + 1 - visible_count)`.
pub fn scroll_target_for_row(
    rows: &[LedgerRow],
    id: &RowId,
    viewport: LedgerViewport,
) -> Option<usize> {
    let target_idx = rows.iter().position(|r| r.id == *id)?;
    if viewport.visible_count == 0 {
        return Some(target_idx);
    }
    let end_visible = viewport.first_visible + viewport.visible_count;
    if target_idx < viewport.first_visible {
        Some(target_idx)
    } else if target_idx >= end_visible {
        Some(
            target_idx
                .saturating_add(1)
                .saturating_sub(viewport.visible_count),
        )
    } else {
        None
    }
}

/// Resolve the new row index for an anchor after historical prepend.
///
/// When older items are prepended to the ledger (e.g. historical backfill/paging),
/// this function locates the anchored row in `rows_after` so the scroller can
/// adjust its offset and preserve visual continuity.
///
/// Returns `Some(new_index)` if `anchor` exists in `rows_after`, or `None` if missing.
pub fn anchor_after_prepend(
    _rows_before: &[LedgerRow],
    rows_after: &[LedgerRow],
    anchor: &RowId,
) -> Option<usize> {
    rows_after.iter().position(|r| r.id == *anchor)
}

/// Compute whether live append should follow the edge or remain anchored.
pub fn should_follow_live_edge(model: &TrajectoryViewModel) -> bool {
    model.following_live()
}

/// Compute scroll target for live edge append.
pub fn live_edge_scroll_target(
    rows: &[LedgerRow],
    following_live: bool,
    viewport: LedgerViewport,
) -> Option<usize> {
    if !following_live || rows.is_empty() {
        return None;
    }
    let last_idx = rows.len().saturating_sub(1);
    let end_visible = viewport.first_visible + viewport.visible_count;
    if last_idx >= end_visible {
        Some(
            last_idx
                .saturating_add(1)
                .saturating_sub(viewport.visible_count),
        )
    } else {
        None
    }
}

/// Render the virtualized ledger list using GPUI's uniform_list.
pub fn render_ledger<S, F>(
    model: &TrajectoryViewModel,
    theme: &Theme,
    scroll_handle: &UniformListScrollHandle,
    on_select: S,
    on_toggle_fold: F,
) -> AnyElement
where
    S: Fn(RowId, &mut App) + Clone + 'static,
    F: Fn(RowId, &mut App) + Clone + 'static,
{
    let rows = model.rows().to_vec();
    let row_count = rows.len();
    let selected_row_id = model.selected_row().cloned();
    let theme_clone = theme.clone();

    div()
        .id(ElementId::from(SharedString::from(
            "trajectory-ledger-container",
        )))
        .size_full()
        .bg(theme.bg)
        .child(
            uniform_list(
                SharedString::from("trajectory-ledger-list"),
                row_count,
                move |range, _window, _cx| {
                    range
                        .filter_map(|idx| {
                            let row = rows.get(idx)?;
                            let is_selected = selected_row_id.as_ref() == Some(&row.id);
                            Some(render_ledger_row(
                                row,
                                is_selected,
                                &theme_clone,
                                on_select.clone(),
                                on_toggle_fold.clone(),
                            ))
                        })
                        .collect::<Vec<AnyElement>>()
                },
            )
            .size_full()
            .track_scroll(scroll_handle),
        )
        .into_any_element()
}

/// Render a single ledger row according to hierarchical depth, fold, and lane status.
///
/// Clicking the row selects its record (timeline, ledger and inspector share one
/// selection); clicking the chevron only folds. The chevron stops propagation so
/// folding a turn never also re-selects it.
pub fn render_ledger_row<S, F>(
    row: &LedgerRow,
    is_selected: bool,
    theme: &Theme,
    on_select: S,
    on_toggle_fold: F,
) -> AnyElement
where
    S: Fn(RowId, &mut App) + Clone + 'static,
    F: Fn(RowId, &mut App) + Clone + 'static,
{
    let indent = px(row.depth as f32 * 14.0);
    let row_id = row.id.clone();

    let mut row_div = div()
        .id(ElementId::from(SharedString::from(format!(
            "ledger-row-{}",
            row.id.as_str()
        ))))
        .role(Role::ListItem)
        .aria_label(row.label.clone())
        .h(ROW_HEIGHT)
        .w_full()
        .flex()
        .items_center()
        .pl(indent)
        .pr(px(8.0))
        .gap(px(6.0))
        .cursor_pointer()
        .on_click({
            let row_id = row_id.clone();
            move |_, _, cx| on_select(row_id.clone(), cx)
        });

    if is_selected {
        row_div = row_div.bg(theme.element_active);
    } else {
        row_div = row_div.hover(|s| s.bg(theme.element_hover));
    }

    if row.dimmed {
        row_div = row_div.opacity(0.35);
    }

    // Fold chevron slot (14px)
    if row.foldable {
        let chevron_icon = if row.folded {
            icons::ALT_ARROW_RIGHT
        } else {
            icons::ALT_ARROW_DOWN
        };
        let fold_id = row_id.clone();
        row_div = row_div.child(
            div()
                .id(ElementId::from(SharedString::from(format!(
                    "ledger-fold-{}",
                    row.id.as_str()
                ))))
                .flex_none()
                .w(px(14.0))
                .flex()
                .items_center()
                .justify_center()
                .on_click(move |_, _, cx| {
                    cx.stop_propagation();
                    on_toggle_fold(fold_id.clone(), cx);
                })
                .child(
                    icons::icon(chevron_icon)
                        .size(px(12.0))
                        .text_color(theme.text_muted),
                ),
        );
    } else {
        row_div = row_div.child(div().flex_none().w(px(14.0)));
    }

    // Kind / Status icon (14px)
    let (icon_name, icon_color) = if row.is_error {
        (icons::DANGER_TRIANGLE, theme.danger)
    } else {
        match row.kind {
            LedgerRowKind::Run => (icons::CHAT_ROUND_LINE, theme.accent),
            LedgerRowKind::Turn => (icons::BOT, theme.text_muted),
            LedgerRowKind::Step => (icons::CHECKLIST, theme.text_muted),
            LedgerRowKind::Event => match row.lane {
                Some(TrajectoryLane::Input) => (icons::PEN, theme.text_muted),
                Some(TrajectoryLane::Model) => (icons::THOUGHT_SPARKLE, theme.accent),
                Some(TrajectoryLane::Tools | TrajectoryLane::Unknown) => {
                    (icons::TERMINAL, theme.text_muted)
                }
                None => (icons::DOCUMENT, theme.text_muted),
            },
        }
    };

    row_div = row_div.child(
        div()
            .flex_none()
            .w(px(14.0))
            .flex()
            .items_center()
            .justify_center()
            .child(icons::icon(icon_name).size(px(13.0)).text_color(icon_color)),
    );

    // Label
    let text_color = if row.is_error {
        theme.danger
    } else if is_selected {
        theme.text
    } else if row.dimmed {
        theme.text_faint
    } else {
        theme.text
    };

    row_div = row_div.child(
        div()
            .flex_1()
            .min_w_0()
            .truncate()
            .text_size(px(11.5))
            .text_color(text_color)
            .child(row.label.clone()),
    );

    // Status or Lane badge
    if let Some(lane) = row.lane {
        let lane_str = match lane {
            TrajectoryLane::Input => "input",
            TrajectoryLane::Model => "model",
            TrajectoryLane::Tools | TrajectoryLane::Unknown => "tool",
        };
        row_div = row_div.child(
            div()
                .flex_none()
                .px(px(4.0))
                .py(px(1.0))
                .rounded(px(2.0))
                .bg(theme.surface_raised)
                .text_size(px(9.5))
                .text_color(theme.text_muted)
                .child(lane_str),
        );
    }

    row_div.into_any_element()
}

// ---------------------------------------------------------------------------
// Unit Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use zeron_proto::trajectory::TrajectoryStatus;

    use super::*;

    fn make_test_row(id: &str, depth: u8, kind: LedgerRowKind) -> LedgerRow {
        LedgerRow {
            id: RowId::from(id),
            kind,
            depth,
            label: SharedString::from(id),
            record: None,
            lane: None,
            status: Some(TrajectoryStatus::Completed),
            is_error: false,
            dimmed: false,
            foldable: false,
            folded: false,
        }
    }

    #[test]
    fn test_trajectory_ledger_fixed_row_height() {
        assert_eq!(ROW_HEIGHT, px(26.0));
    }

    #[test]
    fn test_trajectory_ledger_scroll_target_for_row_above_and_below_viewport() {
        let rows: Vec<LedgerRow> = (0..50)
            .map(|i| make_test_row(&format!("row-{i}"), 0, LedgerRowKind::Event))
            .collect();

        let viewport = LedgerViewport {
            first_visible: 10,
            visible_count: 5, // rows 10..15 visible (10, 11, 12, 13, 14)
        };

        // Target above viewport (row 4): returns 4 so it scrolls up to place row 4 as first_visible
        let target_above = RowId::from("row-4");
        assert_eq!(
            scroll_target_for_row(&rows, &target_above, viewport),
            Some(4)
        );

        // Target below viewport (row 20): returns 20 + 1 - 5 = 16 so it scrolls down to place row 20 at bottom of viewport
        let target_below = RowId::from("row-20");
        assert_eq!(
            scroll_target_for_row(&rows, &target_below, viewport),
            Some(16)
        );

        // Target exactly at bottom edge (row 15): index 15 is >= 10 + 5 (15), so returns 15 + 1 - 5 = 11
        let target_edge_below = RowId::from("row-15");
        assert_eq!(
            scroll_target_for_row(&rows, &target_edge_below, viewport),
            Some(11)
        );
    }

    #[test]
    fn test_trajectory_ledger_scroll_target_for_row_already_visible() {
        let rows: Vec<LedgerRow> = (0..50)
            .map(|i| make_test_row(&format!("row-{i}"), 0, LedgerRowKind::Event))
            .collect();

        let viewport = LedgerViewport {
            first_visible: 10,
            visible_count: 5, // rows 10..15 visible (10, 11, 12, 13, 14)
        };

        // Targets currently inside visible range: should return None (no scroll needed)
        assert_eq!(
            scroll_target_for_row(&rows, &RowId::from("row-10"), viewport),
            None
        );
        assert_eq!(
            scroll_target_for_row(&rows, &RowId::from("row-12"), viewport),
            None
        );
        assert_eq!(
            scroll_target_for_row(&rows, &RowId::from("row-14"), viewport),
            None
        );

        // Non-existent row ID: should return None
        assert_eq!(
            scroll_target_for_row(&rows, &RowId::from("row-unknown"), viewport),
            None
        );
    }

    #[test]
    fn test_trajectory_ledger_anchor_after_prepend_preserves_position() {
        let rows_before: Vec<LedgerRow> = vec![
            make_test_row("row-10", 0, LedgerRowKind::Event),
            make_test_row("row-11", 0, LedgerRowKind::Event),
            make_test_row("row-12", 0, LedgerRowKind::Event),
        ];

        let anchor = RowId::from("row-10");

        // 5 older rows prepended (row-5..row-9)
        let mut rows_after: Vec<LedgerRow> = (5..10)
            .map(|i| make_test_row(&format!("row-{i}"), 0, LedgerRowKind::Event))
            .collect();
        rows_after.extend(rows_before.clone());

        // The anchor "row-10" was at index 0 in rows_before, now at index 5 in rows_after
        let new_index = anchor_after_prepend(&rows_before, &rows_after, &anchor);
        assert_eq!(new_index, Some(5));

        // Unknown anchor returns None
        let missing_anchor = RowId::from("row-nonexistent");
        assert_eq!(
            anchor_after_prepend(&rows_before, &rows_after, &missing_anchor),
            None
        );
    }

    #[test]
    fn test_trajectory_ledger_live_append_following_vs_anchored() {
        let mut model = TrajectoryViewModel::new("chat-1");

        // By default, following_live is true
        assert!(should_follow_live_edge(&model));

        let rows: Vec<LedgerRow> = (0..30)
            .map(|i| make_test_row(&format!("row-{i}"), 0, LedgerRowKind::Event))
            .collect();

        let viewport = LedgerViewport {
            first_visible: 0,
            visible_count: 10, // rows 0..10 visible
        };

        // When following_live == true, new append (up to row 29) yields scroll target to bring the edge into view
        let target = live_edge_scroll_target(&rows, true, viewport);
        assert_eq!(target, Some(29 + 1 - 10)); // 20

        // When following_live == false (user scrolled away or paused), live_edge_scroll_target returns None
        model.set_following_live(false);
        assert!(!should_follow_live_edge(&model));
        assert_eq!(live_edge_scroll_target(&rows, false, viewport), None);
    }
}
