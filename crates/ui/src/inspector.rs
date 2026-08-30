//! GPUI Dev Inspector — native element inspection for development builds.
//!
//! Provides a toggleable inspector panel and element picking mode (⌥⌘I) to
//! inspect rendered GPUI elements, revealing their `source_location` (file:line)
//! and `instance_id`.
//!
//! Entirely gated under `#[cfg(debug_assertions)]` with zero runtime overhead or
//! code present in release builds.

#![cfg(debug_assertions)]

use gpui::{
    AnyElement, App, ClipboardItem, Context, Inspector, IntoElement, Window, actions, div,
    prelude::*, px,
};

use crate::icons::{self, icon};
use crate::theme::Theme;

actions!(dev, [ToggleInspector]);

/// Initialize the dev inspector: register the `ToggleInspector` action handler
/// and install the GPUI inspector renderer.
pub fn init(cx: &mut App) {
    cx.on_action(|_: &ToggleInspector, cx| {
        let Some(active_window) = cx.active_window() else {
            return;
        };
        // Deferred dispatch is mandatory to prevent double lease on the window.
        cx.defer(move |cx| {
            active_window
                .update(cx, |_, window, cx| window.toggle_inspector(cx))
                .ok();
        });
    });

    cx.set_inspector_renderer(Box::new(render_inspector));
}

/// Renders the Comet dev inspector panel.
fn render_inspector(
    inspector: &mut Inspector,
    window: &mut Window,
    cx: &mut Context<Inspector>,
) -> AnyElement {
    let theme = Theme::of(cx).clone();
    let is_picking = inspector.is_picking();
    let active_element_id = inspector.active_element_id().cloned();
    let states = inspector.render_inspector_states(window, cx);

    div()
        .id("gpui-dev-inspector")
        .size_full()
        .flex()
        .flex_col()
        .bg(theme.surface_overlay)
        .border_l_1()
        .border_color(theme.border)
        .text_color(theme.text)
        .font_family(theme.font_sans.clone())
        .overflow_hidden()
        // Header
        .child(
            div()
                .h(px(Theme::TITLEBAR_HEIGHT))
                .px_3()
                .flex()
                .items_center()
                .justify_between()
                .border_b_1()
                .border_color(theme.border)
                .bg(theme.surface_card)
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(icon(icons::MAGNIFER).size(px(14.0)).text_color(theme.accent))
                        .child(
                            div()
                                .text_size(px(13.0))
                                .font_weight(gpui::FontWeight::SEMIBOLD)
                                .child("GPUI Inspector"),
                        )
                        .child(
                            div()
                                .px_1p5()
                                .py_0p5()
                                .rounded(px(Theme::CONTROL_RADIUS))
                                .bg(theme.surface_raised)
                                .text_size(px(10.0))
                                .font_weight(gpui::FontWeight::BOLD)
                                .text_color(theme.accent)
                                .child("DEV"),
                        ),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(
                            div()
                                .px_1p5()
                                .py_0p5()
                                .rounded(px(Theme::CONTROL_RADIUS))
                                .text_size(px(11.0))
                                .text_color(theme.text_faint)
                                .child("⌥⌘I"),
                        )
                        .child(
                            div()
                                .id("btn-inspector-close")
                                .p_1()
                                .rounded(px(Theme::CONTROL_RADIUS))
                                .hover(|s| s.bg(theme.element_hover))
                                .cursor_pointer()
                                .on_click(|_, window, cx| {
                                    window.toggle_inspector(cx);
                                })
                                .child(
                                    icon(icons::CLOSE)
                                        .size(px(14.0))
                                        .text_color(theme.text_muted),
                                ),
                        ),
                ),
        )
        // Toolbar / Controls
        .child(
            div()
                .p_3()
                .flex()
                .flex_col()
                .gap_2()
                .border_b_1()
                .border_color(theme.border)
                .child(if is_picking {
                    div()
                        .id("btn-inspector-pick")
                        .flex()
                        .items_center()
                        .justify_center()
                        .gap_2()
                        .py_2()
                        .px_3()
                        .rounded(px(Theme::CONTROL_RADIUS))
                        .bg(theme.accent)
                        .text_color(theme.on_accent)
                        .cursor_pointer()
                        .child(icon(icons::MAGNIFER).size(px(14.0)).text_color(theme.on_accent))
                        .child(
                            div()
                                .text_size(px(12.0))
                                .font_weight(gpui::FontWeight::SEMIBOLD)
                                .child("Picking Active (Click UI Element)"),
                        )
                } else {
                    div()
                        .id("btn-inspector-pick")
                        .flex()
                        .items_center()
                        .justify_center()
                        .gap_2()
                        .py_2()
                        .px_3()
                        .rounded(px(Theme::CONTROL_RADIUS))
                        .bg(theme.surface_raised)
                        .border_1()
                        .border_color(theme.border)
                        .hover(|s| s.bg(theme.element_hover))
                        .cursor_pointer()
                        .on_click(cx.listener(|inspector, _, window, cx| {
                            inspector.start_picking();
                            window.refresh();
                            cx.notify();
                        }))
                        .child(icon(icons::MAGNIFER).size(px(14.0)).text_color(theme.text))
                        .child(
                            div()
                                .text_size(px(12.0))
                                .font_weight(gpui::FontWeight::MEDIUM)
                                .text_color(theme.text)
                                .child("Start Picking Element"),
                        )
                })
                .when(is_picking, |el| {
                    el.child(
                        div()
                            .p_2()
                            .rounded(px(Theme::CONTROL_RADIUS))
                            .bg(theme.surface_card)
                            .border_1()
                            .border_color(theme.border)
                            .text_size(px(11.0))
                            .text_color(theme.text_muted)
                            .child("Hover elements to preview hitboxes. Click to inspect, or use trackpad scroll to move up/down depth."),
                    )
                }),
        )
        // Content body: selected element details or empty state
        .child(match active_element_id {
            Some(id) => {
                let loc = id.path.source_location;
                let file = loc.file();
                let line = loc.line();
                let col = loc.column();
                let loc_str = format!("{}:{}", file, line);
                let copy_loc = loc_str.clone();
                div()
                    .id("inspector-content")
                    .flex_1()
                    .overflow_y_scroll()
                    .p_3()
                    .flex()
                    .flex_col()
                    .gap_3()
                    .child(
                        div()
                            .p_3()
                            .rounded(px(Theme::PANEL_RADIUS))
                            .bg(theme.surface_card)
                            .border_1()
                            .border_color(theme.border)
                            .flex()
                            .flex_col()
                            .gap_2()
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .justify_between()
                                    .child(
                                        div()
                                            .text_size(px(11.0))
                                            .font_weight(gpui::FontWeight::BOLD)
                                            .text_color(theme.text_muted)
                                            .child("SOURCE LOCATION"),
                                    )
                                    .child(
                                        div()
                                            .id("btn-copy-location")
                                            .flex()
                                            .items_center()
                                            .gap_1()
                                            .px_1p5()
                                            .py_0p5()
                                            .rounded(px(Theme::CONTROL_RADIUS))
                                            .hover(|s| s.bg(theme.element_hover))
                                            .cursor_pointer()
                                            .on_click(move |_, _, cx| {
                                                cx.stop_propagation();
                                                cx.write_to_clipboard(ClipboardItem::new_string(
                                                    copy_loc.clone(),
                                                ));
                                            })
                                            .child(
                                                icon(icons::COPY)
                                                    .size(px(11.0))
                                                    .text_color(theme.text_muted),
                                            )
                                            .child(
                                                div()
                                                    .text_size(px(10.0))
                                                    .text_color(theme.text_muted)
                                                    .child("Copy"),
                                            ),
                                    ),
                            )
                            .child(
                                div()
                                    .p_2()
                                    .rounded(px(Theme::CONTROL_RADIUS))
                                    .bg(theme.surface_overlay)
                                    .border_1()
                                    .border_color(theme.border)
                                    .font_family(theme.font_mono.clone())
                                    .text_size(px(11.0))
                                    .text_color(theme.accent)
                                    .child(loc_str),
                            )
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .justify_between()
                                    .text_size(px(11.0))
                                    .child(div().text_color(theme.text_muted).child("Instance ID"))
                                    .child(
                                        div()
                                            .font_family(theme.font_mono.clone())
                                            .text_color(theme.text)
                                            .child(format!("{}", id.instance_id)),
                                    ),
                            )
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .justify_between()
                                    .text_size(px(11.0))
                                    .child(div().text_color(theme.text_muted).child("Line : Column"))
                                    .child(
                                        div()
                                            .font_family(theme.font_mono.clone())
                                            .text_color(theme.text)
                                            .child(format!("{}:{}", line, col)),
                                    ),
                            ),
                    )
                    .when(!states.is_empty(), |el| {
                        el.child(
                            div()
                                .flex()
                                .flex_col()
                                .gap_2()
                                .child(
                                    div()
                                        .text_size(px(11.0))
                                        .font_weight(gpui::FontWeight::BOLD)
                                        .text_color(theme.text_muted)
                                        .child("ELEMENT STATES"),
                                )
                                .children(states),
                        )
                    })
            }
            None => div()
                .id("inspector-empty")
                .flex_1()
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
                .p_6()
                .gap_3()
                .child(
                    icon(icons::MAGNIFER)
                        .size(px(32.0))
                        .text_color(theme.text_faint),
                )
                .child(
                    div()
                        .text_size(px(13.0))
                        .font_weight(gpui::FontWeight::MEDIUM)
                        .text_color(theme.text)
                        .child("No Element Selected"),
                )
                .child(
                    div()
                        .text_size(px(11.0))
                        .text_color(theme.text_muted)
                        .line_height(px(16.0))
                        .child("Click 'Start Picking Element' above (or press ⌥⌘I), then hover over any UI component to inspect its code origin."),
                ),
        })
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::Action as _;

    #[test]
    fn toggle_inspector_action_name() {
        assert_eq!(ToggleInspector.name(), "dev::ToggleInspector");
    }
}
