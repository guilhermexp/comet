use std::sync::Arc;

use gpui::{
    AnyElement, App, FocusHandle, Image, InteractiveElement, IntoElement, KeyDownEvent, ObjectFit,
    ParentElement, Pixels, Point, Role, SharedString, Size, StatefulInteractiveElement, Styled,
    StyledImage, Window, div, img, point, px,
};

use crate::theme::{Theme, hairline, ink};
#[derive(Clone)]
pub struct MermaidPreview {
    pub image: Arc<Image>,
    pub width: f32,
    pub height: f32,
    pub source: String,
    pub svg: String,
}

pub fn mermaid_lightbox(
    viewport: Size<Pixels>,
    preview: &MermaidPreview,
    focus: &FocusHandle,
    zoom: f32,
    pan: Point<Pixels>,
    copied_svg: bool,
    copied_code: bool,
    theme: &Theme,
    on_zoom_in: impl Fn(&mut Window, &mut App) + 'static,
    on_zoom_out: impl Fn(&mut Window, &mut App) + 'static,
    on_zoom_fit: impl Fn(&mut Window, &mut App) + 'static,
    on_zoom_100: impl Fn(&mut Window, &mut App) + 'static,
    on_copy_svg: impl Fn(&mut Window, &mut App) + 'static,
    on_copy_code: impl Fn(&mut Window, &mut App) + 'static,
    on_close: impl Fn(&mut Window, &mut App) + 'static,
) -> AnyElement {
    let on_close = std::rc::Rc::new(on_close);
    let close_on_key = on_close.clone();
    let close_on_bg = on_close.clone();
    let close_btn = on_close.clone();

    let display_width = (preview.width * zoom).max(10.0);
    let display_height = (preview.height * zoom).max(10.0);

    let toolbar_btn = |id: &'static str,
                       label: SharedString,
                       icon_name: Option<&'static str>,
                       on_click: std::rc::Rc<dyn Fn(&mut Window, &mut App)>| {
        let mut btn = div()
            .id(id)
            .h(px(26.0))
            .px(px(8.0))
            .rounded(px(5.0))
            .flex()
            .items_center()
            .gap(px(4.0))
            .text_size(px(11.5))
            .text_color(theme.text_muted)
            .hover(|s| s.text_color(theme.text).bg(ink(0.08)))
            .cursor_pointer()
            .on_click(move |_, window, cx| {
                cx.stop_propagation();
                on_click(window, cx);
            });
        if let Some(icon) = icon_name {
            btn = btn.child(crate::icons::icon(icon).size(px(12.0)));
        }
        if !label.is_empty() {
            btn = btn.child(label);
        }
        btn
    };

    let on_zoom_in = std::rc::Rc::new(on_zoom_in);
    let on_zoom_out = std::rc::Rc::new(on_zoom_out);
    let on_zoom_fit = std::rc::Rc::new(on_zoom_fit);
    let on_zoom_100 = std::rc::Rc::new(on_zoom_100);
    let on_copy_svg = std::rc::Rc::new(on_copy_svg);
    let on_copy_code = std::rc::Rc::new(on_copy_code);

    let zoom_pct: SharedString = format!("{:.0}%", (zoom * 100.0).round()).into();

    let toolbar = div()
        .id("mermaid-preview-toolbar")
        .h(px(38.0))
        .px(px(10.0))
        .rounded(px(8.0))
        .bg(theme.bg)
        .border_1()
        .border_color(hairline(0.12))
        .shadow_lg()
        .flex()
        .items_center()
        .gap(px(6.0))
        .on_click(|_, _, cx| cx.stop_propagation())
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(6.0))
                .px(px(4.0))
                .text_size(px(12.0))
                .text_color(theme.text)
                .font_weight(gpui::FontWeight::MEDIUM)
                .child(
                    crate::icons::icon(crate::icons::WIDGET)
                        .size(px(13.0))
                        .text_color(theme.text_muted),
                )
                .child("Mermaid Diagram"),
        )
        .child(div().h(px(14.0)).w(px(1.0)).bg(hairline(0.15)))
        .child(toolbar_btn(
            "zoom-out",
            "".into(),
            Some(crate::icons::FOLD_VERTICAL),
            on_zoom_out.clone(),
        ))
        .child(
            div()
                .min_w(px(42.0))
                .flex()
                .items_center()
                .justify_center()
                .text_size(px(11.0))
                .text_color(theme.text_muted)
                .child(zoom_pct),
        )
        .child(toolbar_btn(
            "zoom-in",
            "".into(),
            Some(crate::icons::PLUS),
            on_zoom_in.clone(),
        ))
        .child(toolbar_btn(
            "zoom-fit",
            "Fit".into(),
            Some(crate::icons::REFRESH),
            on_zoom_fit.clone(),
        ))
        .child(toolbar_btn(
            "zoom-100",
            "1:1".into(),
            None,
            on_zoom_100.clone(),
        ))
        .child(div().h(px(14.0)).w(px(1.0)).bg(hairline(0.15)))
        .child(toolbar_btn(
            "copy-svg",
            if copied_svg {
                "Copied SVG".into()
            } else {
                "Copy SVG".into()
            },
            Some(if copied_svg {
                crate::icons::CHECK
            } else {
                crate::icons::COPY
            }),
            on_copy_svg.clone(),
        ))
        .child(toolbar_btn(
            "copy-code",
            if copied_code {
                "Copied Code".into()
            } else {
                "Copy Code".into()
            },
            Some(if copied_code {
                crate::icons::CHECK
            } else {
                crate::icons::COPY
            }),
            on_copy_code.clone(),
        ))
        .child(div().h(px(14.0)).w(px(1.0)).bg(hairline(0.15)))
        .child(toolbar_btn(
            "close-preview",
            "".into(),
            Some(crate::icons::CLOSE),
            close_btn,
        ));

    gpui::deferred(
        gpui::anchored().position(point(px(0.0), px(0.0))).child(
            div()
                .id("mermaid-lightbox")
                .role(Role::Dialog)
                .aria_label("Mermaid diagram fullscreen preview")
                .occlude()
                .track_focus(focus)
                .w(viewport.width)
                .h(viewport.height)
                .bg(crate::popover::scrim_alpha(0.85))
                .flex()
                .flex_col()
                .items_center()
                .justify_between()
                .p(px(16.0))
                .on_key_down(move |event: &KeyDownEvent, window, cx| {
                    if event.keystroke.key == "escape" {
                        cx.stop_propagation();
                        close_on_key(window, cx);
                    }
                })
                .on_click(move |_, window, cx| close_on_bg(window, cx))
                .child(toolbar)
                .child(
                    div()
                        .id("mermaid-lightbox-viewport")
                        .flex_1()
                        .w_full()
                        .overflow_hidden()
                        .flex()
                        .items_center()
                        .justify_center()
                        .on_click(|_, _, cx| cx.stop_propagation())
                        .child(
                            div()
                                .relative()
                                .w(px(display_width))
                                .h(px(display_height))
                                .left(pan.x)
                                .top(pan.y)
                                .flex_none()
                                .child(
                                    img(preview.image.clone())
                                        .w(px(display_width))
                                        .h(px(display_height))
                                        .object_fit(ObjectFit::Contain),
                                ),
                        ),
                )
                .child(div().h(px(20.0))),
        ),
    )
    .priority(3)
    .into_any_element()
}
