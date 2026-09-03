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

/// Whether the ledger viewport sits away from the bottom of the list.
///
/// Read from the scroll handle rather than from a wheel handler: position is
/// the actual signal, so this covers wheel, trackpad, drag, and keyboard
/// scrolling in one place instead of one gesture.
///
/// `offset_y` is <= 0 and grows negative downward; `max_offset_y` is the total
/// scrollable height. The tolerance absorbs the sub-row rounding that a
/// programmatic `scroll_to_item` leaves behind, so following the live edge is
/// not mistaken for the user having scrolled away from it.
pub fn is_away_from_live_edge(
    offset_y: gpui::Pixels,
    max_offset_y: gpui::Pixels,
    tolerance: gpui::Pixels,
) -> bool {
    if max_offset_y <= gpui::px(0.0) {
        // Nothing to scroll: the whole list fits, so the live edge is visible.
        return false;
    }
    -offset_y < max_offset_y - tolerance
}

/// Decide whether live following survives an arriving record. Judged before
/// the catch-up scroll is queued, so a stream faster than the frame rate can
/// never starve the check.
///
/// `pending_jump_from` is the offset recorded when a live-edge jump was queued
/// that prepaint has not applied yet. Until it lands, only the user can move
/// the offset, so any change means they scrolled and they win. With no jump
/// pending, offset and `max_offset_y` describe the same frame and position
/// alone decides.
pub fn keep_following_live(
    offset_y: gpui::Pixels,
    max_offset_y: gpui::Pixels,
    pending_jump_from: Option<gpui::Pixels>,
    tolerance: gpui::Pixels,
) -> bool {
    match pending_jump_from {
        Some(from) => offset_y == from,
        None => !is_away_from_live_edge(offset_y, max_offset_y, tolerance),
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
        // Accent wash, not a neutral one. In light appearance the neutral
        // candidates land at 1.13:1 (`glass_selected_bg`) and 1.16:1
        // (`element_active`) against the white ledger background — measured in
        // the native light pass, and in a list this dense that is not enough to
        // tell which row the inspector is describing. The accent wash carries a
        // hue shift on top of the luminance step, so selection survives both
        // appearances.
        row_div = row_div.bg(theme.accent_wash);
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
    use super::*;

    #[test]
    fn test_trajectory_ledger_fixed_row_height() {
        assert_eq!(ROW_HEIGHT, px(26.0));
    }

    /// The gap the native pass exposed: `following_live` never went false, so
    /// an arriving record kept yanking the viewport and the toolbar never
    /// offered a way back. Scroll position is the signal.
    #[test]
    fn test_trajectory_ledger_away_from_live_edge_detection() {
        let row = gpui::px(28.0);
        let tolerance = row * 2.0;

        // Parked at the bottom: offset consumes the whole scrollable height.
        assert!(!is_away_from_live_edge(-row * 20.0, row * 20.0, tolerance));

        // Sub-row rounding left behind by a programmatic scroll_to_item must
        // NOT read as the user having scrolled away.
        assert!(!is_away_from_live_edge(
            -row * 20.0 + gpui::px(9.0),
            row * 20.0,
            tolerance
        ));

        // Scrolled back by more than the tolerance: away from the live edge.
        assert!(is_away_from_live_edge(-row * 15.0, row * 20.0, tolerance));

        // A list shorter than the viewport has no scroll range; the live edge
        // is on screen by construction and must never suspend following.
        assert!(!is_away_from_live_edge(
            gpui::px(0.0),
            gpui::px(0.0),
            tolerance
        ));
    }

    /// A stream faster than the frame rate keeps a catch-up scroll pending at
    /// every render; the decision must still read the user's scroll-back.
    #[test]
    fn test_trajectory_ledger_keep_following_live_decision() {
        let row = gpui::px(26.0);
        let tolerance = row * 2.0;
        let bottom = -row * 20.0;

        // No jump pending: position decides.
        assert!(keep_following_live(bottom, row * 20.0, None, tolerance));
        assert!(!keep_following_live(
            -row * 10.0,
            row * 20.0,
            None,
            tolerance
        ));

        // Follow Live queued a jump from the top that prepaint has not applied:
        // an unchanged offset is our own pending jump, not the user scrolling.
        assert!(keep_following_live(
            gpui::px(0.0),
            row * 20.0,
            Some(gpui::px(0.0)),
            tolerance
        ));

        // Jump pending but the offset moved: only the user could have done it.
        assert!(!keep_following_live(
            -row * 5.0,
            row * 20.0,
            Some(bottom),
            tolerance
        ));
    }
}
