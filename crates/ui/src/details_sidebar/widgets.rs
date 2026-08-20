use gpui::{Div, SharedString, div, prelude::*, px};

use crate::{icons, theme::Theme};

pub fn widget_card(
    id: &'static str,
    icon_path: &'static str,
    title: impl Into<SharedString>,
    body: Div,
    theme: &Theme,
) -> gpui::Stateful<Div> {
    div()
        .id(id)
        .w_full()
        .rounded(px(10.0))
        .border_1()
        .border_color(theme.border)
        .overflow_hidden()
        .child(
            div()
                .h(px(36.0))
                .px(px(10.0))
                .flex()
                .items_center()
                .gap(px(8.0))
                .bg(crate::theme::ink(0.025))
                .child(
                    icons::icon(icon_path)
                        .size(px(15.0))
                        .text_color(theme.text_muted),
                )
                .child(
                    div()
                        .text_size(px(13.0))
                        .font_weight(gpui::FontWeight::MEDIUM)
                        .text_color(theme.text)
                        .child(title.into()),
                ),
        )
        .child(body)
}

pub fn property_row(
    icon_path: &'static str,
    label: impl Into<SharedString>,
    value: impl Into<SharedString>,
    theme: &Theme,
) -> Div {
    div()
        .h(px(30.0))
        .px(px(10.0))
        .flex()
        .items_center()
        .child(
            div()
                .w(px(108.0))
                .flex_none()
                .flex()
                .items_center()
                .gap(px(7.0))
                .child(
                    icons::icon(icon_path)
                        .size(px(14.0))
                        .text_color(theme.text_muted),
                )
                .child(
                    div()
                        .text_size(px(12.0))
                        .text_color(theme.text_muted)
                        .child(label.into()),
                ),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .truncate()
                .text_size(px(12.0))
                .text_color(theme.text)
                .child(value.into()),
        )
}
