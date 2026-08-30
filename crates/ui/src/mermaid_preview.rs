use std::rc::Rc;
use std::sync::Arc;

use gpui::{
    AnyElement, App, CursorStyle, FocusHandle, Image, InteractiveElement, IntoElement,
    KeyDownEvent, MouseButton, MouseDownEvent, MouseMoveEvent, ObjectFit, ParentElement,
    PinchEvent, Pixels, Point, Role, ScrollDelta, ScrollWheelEvent, SharedString, Size,
    StatefulInteractiveElement, Styled, StyledImage, Window, div, img, point,
    prelude::FluentBuilder as _, px, size,
};

use crate::theme::{Theme, hairline, ink};

/// Padding between the lightbox content and the window edge.
const PAD: f32 = 16.0;
/// Toolbar height plus the gap under it — the band the drawing may not use.
const TOOLBAR_BAND: f32 = 38.0 + 12.0;
/// Spacer under the drawing, so a fitted diagram never touches the edge.
const BOTTOM_BAND: f32 = 20.0;
/// Fit never magnifies past the raster's own render scale
/// ([`gpui::SMOOTH_SVG_SCALE_FACTOR`]): past it the diagram is only blurrier.
const MAX_FIT_ZOOM: f32 = 2.0;
const MIN_ZOOM: f32 = 0.1;
const MAX_ZOOM: f32 = 8.0;
/// Zoom applied per pixel of wheel delta. A trackpad sends many small deltas
/// and a mouse notch arrives as lines, so both go through [`wheel_zoom_factor`].
const WHEEL_ZOOM_PER_PIXEL: f32 = 0.0025;
/// Pixels per line for wheel deltas the platform reports in lines.
const WHEEL_LINE_HEIGHT: f32 = 20.0;

#[derive(Clone)]
pub struct MermaidPreview {
    pub image: Arc<Image>,
    pub width: f32,
    pub height: f32,
    pub source: String,
    pub svg: String,
}

/// What the owner carries between frames.
#[derive(Clone, Copy)]
pub struct MermaidPreviewView {
    pub zoom: f32,
    pub pan: Point<Pixels>,
    pub grabbed: bool,
    pub copied_svg: bool,
    pub copied_code: bool,
}

/// Everything the lightbox can ask its owner to do.
///
/// One channel instead of a closure per control: the surface carries pinch
/// zoom, wheel pan and drag pan on top of six buttons, and a signature with a
/// closure per control is exactly where an unwired control hides.
pub enum MermaidPreviewAction {
    /// Multiply the current zoom (trackpad pinch, ⌘/ctrl + wheel, `+`, `−`).
    ZoomBy(f32),
    /// Absolute zoom (`1:1`).
    ZoomTo(f32),
    /// Scale the whole diagram into the canvas and recenter.
    Fit,
    /// Move the drawing by this delta (two-finger scroll).
    PanBy(Point<Pixels>),
    /// Pointer went down on the canvas at this window position.
    GrabAt(Point<Pixels>),
    /// Pointer moved to this window position while grabbed.
    DragTo(Point<Pixels>),
    Release,
    CopySvg,
    CopyCode,
    Close,
}

/// The area left for the drawing once toolbar and padding are removed.
pub fn canvas_size(viewport: Size<Pixels>) -> Size<Pixels> {
    size(
        px((f32::from(viewport.width) - 2.0 * PAD).max(1.0)),
        px((f32::from(viewport.height) - 2.0 * PAD - TOOLBAR_BAND - BOTTOM_BAND).max(1.0)),
    )
}

/// Zoom that shows the WHOLE diagram — what opening the lightbox must use. A
/// diagram larger than the window used to open at `1.0`, i.e. cropped on every
/// side with no way to reach the rest.
pub fn fit_zoom(viewport: Size<Pixels>, preview: &MermaidPreview) -> f32 {
    let canvas = canvas_size(viewport);
    let by_width = f32::from(canvas.width) / preview.width.max(1.0);
    let by_height = f32::from(canvas.height) / preview.height.max(1.0);
    by_width.min(by_height).min(MAX_FIT_ZOOM).max(MIN_ZOOM)
}

/// How far the drawing may be dragged on each axis: exactly its overflow past
/// the canvas, so a fitted diagram cannot be dragged at all and a zoomed one
/// can reach its own edges and no further.
pub fn pan_slack(viewport: Size<Pixels>, preview: &MermaidPreview, zoom: f32) -> Point<Pixels> {
    let canvas = canvas_size(viewport);
    point(
        px(((preview.width * zoom - f32::from(canvas.width)) / 2.0).max(0.0)),
        px(((preview.height * zoom - f32::from(canvas.height)) / 2.0).max(0.0)),
    )
}

pub fn clamp_zoom(zoom: f32) -> f32 {
    zoom.clamp(MIN_ZOOM, MAX_ZOOM)
}

pub fn clamp_pan(pan: Point<Pixels>, slack: Point<Pixels>) -> Point<Pixels> {
    point(
        pan.x.clamp(-slack.x, slack.x),
        pan.y.clamp(-slack.y, slack.y),
    )
}

/// Scroll delta in pixels, whatever unit the platform reported.
fn scroll_pixels(delta: &ScrollDelta) -> Point<Pixels> {
    match delta {
        ScrollDelta::Pixels(delta) => *delta,
        ScrollDelta::Lines(delta) => point(
            px(delta.x * WHEEL_LINE_HEIGHT),
            px(delta.y * WHEEL_LINE_HEIGHT),
        ),
    }
}

/// Two-finger scroll → pan delta. The drawing follows the fingers, so the
/// platform delta is applied as-is and inherits the user's natural-scroll
/// setting instead of second-guessing it.
pub fn wheel_pan_delta(delta: &ScrollDelta) -> Point<Pixels> {
    scroll_pixels(delta)
}

/// ⌘/ctrl + wheel → zoom multiplier, for pointers with no pinch gesture.
pub fn wheel_zoom_factor(delta: &ScrollDelta) -> f32 {
    let dy = f32::from(scroll_pixels(delta).y);
    (1.0 + dy * WHEEL_ZOOM_PER_PIXEL).clamp(0.5, 2.0)
}

/// Trackpad pinch → zoom multiplier. macOS reports each step as a fractional
/// magnification (`0.02` = +2%), negative when pinching in.
pub fn pinch_zoom_factor(magnification: f32) -> f32 {
    (1.0 + magnification).clamp(0.5, 2.0)
}

pub fn mermaid_lightbox(
    viewport: Size<Pixels>,
    preview: &MermaidPreview,
    view: &MermaidPreviewView,
    focus: &FocusHandle,
    theme: &Theme,
    on_action: impl Fn(MermaidPreviewAction, &mut Window, &mut App) + 'static,
) -> AnyElement {
    let act: Rc<dyn Fn(MermaidPreviewAction, &mut Window, &mut App)> = Rc::new(on_action);

    let zoom = clamp_zoom(view.zoom);
    let display_width = (preview.width * zoom).max(10.0);
    let display_height = (preview.height * zoom).max(10.0);
    let slack = pan_slack(viewport, preview, zoom);
    let pan = clamp_pan(view.pan, slack);
    let draggable = f32::from(slack.x) > 0.5 || f32::from(slack.y) > 0.5;

    // Every glyph carries its OWN color: `gpui::Svg` paints only when its own
    // computed style has a text color (`style.text.color` — the parent's never
    // cascades into it), so an uncolored icon is silently nothing. That is how
    // this toolbar shipped with an invisible close button.
    let toolbar_btn =
        |id: &'static str,
         label: SharedString,
         icon_name: Option<&'static str>,
         action: MermaidPreviewAction,
         act: Rc<dyn Fn(MermaidPreviewAction, &mut Window, &mut App)>| {
            let action = std::cell::Cell::new(Some(action));
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
                .hover(|s| s.bg(ink(0.08)))
                .cursor_pointer()
                .on_click(move |_, window, cx| {
                    cx.stop_propagation();
                    if let Some(action) = action.take() {
                        act(action, window, cx);
                    }
                });
            if let Some(icon) = icon_name {
                btn = btn.child(
                    crate::icons::icon(icon)
                        .size(px(12.0))
                        .text_color(theme.text_muted),
                );
            }
            if !label.is_empty() {
                btn = btn.child(label);
            }
            btn
        };

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
            Some(crate::icons::MINUS),
            MermaidPreviewAction::ZoomBy(1.0 / 1.25),
            act.clone(),
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
            MermaidPreviewAction::ZoomBy(1.25),
            act.clone(),
        ))
        .child(toolbar_btn(
            "zoom-fit",
            "Fit".into(),
            Some(crate::icons::REFRESH),
            MermaidPreviewAction::Fit,
            act.clone(),
        ))
        .child(toolbar_btn(
            "zoom-100",
            "1:1".into(),
            None,
            MermaidPreviewAction::ZoomTo(1.0),
            act.clone(),
        ))
        .child(div().h(px(14.0)).w(px(1.0)).bg(hairline(0.15)))
        .child(toolbar_btn(
            "copy-svg",
            if view.copied_svg {
                "Copied SVG".into()
            } else {
                "Copy SVG".into()
            },
            Some(if view.copied_svg {
                crate::icons::CHECK
            } else {
                crate::icons::COPY
            }),
            MermaidPreviewAction::CopySvg,
            act.clone(),
        ))
        .child(toolbar_btn(
            "copy-code",
            if view.copied_code {
                "Copied Code".into()
            } else {
                "Copy Code".into()
            },
            Some(if view.copied_code {
                crate::icons::CHECK
            } else {
                crate::icons::COPY
            }),
            MermaidPreviewAction::CopyCode,
            act.clone(),
        ))
        .child(div().h(px(14.0)).w(px(1.0)).bg(hairline(0.15)))
        .child(toolbar_btn(
            "close-preview",
            "Close".into(),
            Some(crate::icons::CLOSE),
            MermaidPreviewAction::Close,
            act.clone(),
        ));

    let canvas = {
        let pinch = act.clone();
        let wheel = act.clone();
        let grab = act.clone();
        let drag = act.clone();
        let release = act.clone();
        let release_out = act.clone();
        let grabbed = view.grabbed;
        div()
            .id("mermaid-lightbox-canvas")
            .flex_1()
            .w_full()
            .overflow_hidden()
            .flex()
            .items_center()
            .justify_center()
            .when(draggable, |canvas| {
                canvas.cursor(if grabbed {
                    CursorStyle::ClosedHand
                } else {
                    CursorStyle::OpenHand
                })
            })
            // A drag ends in a click, and the backdrop closes on click: without
            // this the first pan would dismiss the lightbox.
            .on_click(|_, _, cx| cx.stop_propagation())
            // Zoom is a PINCH, and two-finger scroll pans — the map-viewer
            // mapping. Plain wheel zooming meant a scroll gesture silently
            // resized the diagram and there was no gesture left for moving it.
            // ⌘/ctrl + wheel keeps zoom reachable from a plain mouse.
            .on_pinch(move |event: &PinchEvent, window, cx| {
                cx.stop_propagation();
                pinch(
                    MermaidPreviewAction::ZoomBy(pinch_zoom_factor(event.delta)),
                    window,
                    cx,
                );
            })
            .on_scroll_wheel(move |event: &ScrollWheelEvent, window, cx| {
                cx.stop_propagation();
                let action = if event.modifiers.secondary() || event.modifiers.control {
                    MermaidPreviewAction::ZoomBy(wheel_zoom_factor(&event.delta))
                } else {
                    MermaidPreviewAction::PanBy(wheel_pan_delta(&event.delta))
                };
                wheel(action, window, cx);
            })
            .on_mouse_down(
                MouseButton::Left,
                move |event: &MouseDownEvent, window, cx| {
                    grab(MermaidPreviewAction::GrabAt(event.position), window, cx);
                },
            )
            .on_mouse_move(move |event: &MouseMoveEvent, window, cx| {
                if event.pressed_button == Some(MouseButton::Left) {
                    drag(MermaidPreviewAction::DragTo(event.position), window, cx);
                }
            })
            .on_mouse_up(MouseButton::Left, move |_, window, cx| {
                release(MermaidPreviewAction::Release, window, cx);
            })
            // The gesture must end even when the pointer leaves the canvas,
            // or the next unrelated move keeps panning.
            .on_mouse_up_out(MouseButton::Left, move |_, window, cx| {
                release_out(MermaidPreviewAction::Release, window, cx);
            })
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
            )
    };

    let close_on_key = act.clone();
    let close_on_bg = act.clone();

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
                .p(px(PAD))
                .on_key_down(move |event: &KeyDownEvent, window, cx| {
                    if event.keystroke.key == "escape" {
                        cx.stop_propagation();
                        close_on_key(MermaidPreviewAction::Close, window, cx);
                    }
                })
                .on_click(move |_, window, cx| close_on_bg(MermaidPreviewAction::Close, window, cx))
                .child(toolbar)
                .child(canvas)
                .child(div().h(px(BOTTOM_BAND))),
        ),
    )
    .priority(3)
    .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::ImageFormat;

    fn preview(width: f32, height: f32) -> MermaidPreview {
        MermaidPreview {
            image: Arc::new(Image::from_bytes(ImageFormat::Svg, Vec::new())),
            width,
            height,
            source: String::new(),
            svg: String::new(),
        }
    }

    #[test]
    fn fit_shows_the_whole_diagram_and_leaves_nothing_to_pan() {
        let viewport = size(px(1400.0), px(900.0));
        let canvas = canvas_size(viewport);
        // The case that read as a broken preview: a diagram far wider than the
        // window, which opened at 1.0 cropped on every side.
        let wide = preview(2237.0, 78.0);
        let zoom = fit_zoom(viewport, &wide);
        assert!(wide.width * zoom <= f32::from(canvas.width) + 0.5);
        assert!(wide.height * zoom <= f32::from(canvas.height) + 0.5);
        let slack = pan_slack(viewport, &wide, zoom);
        assert_eq!(f32::from(slack.x), 0.0);
        assert_eq!(f32::from(slack.y), 0.0);

        // Fitting a small diagram magnifies it, but never past the raster's
        // own render scale.
        assert_eq!(fit_zoom(viewport, &preview(100.0, 40.0)), MAX_FIT_ZOOM);
    }

    #[test]
    fn pan_reaches_the_drawing_edges_and_stops_there() {
        let viewport = size(px(1000.0), px(800.0));
        let canvas = canvas_size(viewport);
        let diagram = preview(2000.0, 400.0);
        let slack = pan_slack(viewport, &diagram, 1.0);
        assert_eq!(f32::from(slack.x), (2000.0 - f32::from(canvas.width)) / 2.0);
        // A drag past the edge clamps instead of stranding the drawing off
        // canvas; the vertical axis has no overflow here, so it cannot move.
        let dragged = clamp_pan(point(px(9_000.0), px(9_000.0)), slack);
        assert_eq!(f32::from(dragged.x), f32::from(slack.x));
        assert_eq!(f32::from(dragged.y), 0.0);
    }

    #[test]
    fn gestures_zoom_in_both_directions() {
        // macOS reports pinch as a fractional magnification per step.
        assert!(pinch_zoom_factor(0.05) > 1.0);
        assert!(pinch_zoom_factor(-0.05) < 1.0);
        assert_eq!(pinch_zoom_factor(0.0), 1.0);
        // A runaway step cannot invert or explode the scale.
        assert_eq!(pinch_zoom_factor(-4.0), 0.5);
        assert_eq!(pinch_zoom_factor(4.0), 2.0);

        assert!(wheel_zoom_factor(&ScrollDelta::Pixels(point(px(0.0), px(40.0)))) > 1.0);
        assert!(wheel_zoom_factor(&ScrollDelta::Lines(point(0.0, -3.0))) < 1.0);

        // Plain scroll pans by the platform delta, in lines or pixels.
        let panned = wheel_pan_delta(&ScrollDelta::Lines(point(1.0, -2.0)));
        assert_eq!(f32::from(panned.x), WHEEL_LINE_HEIGHT);
        assert_eq!(f32::from(panned.y), -2.0 * WHEEL_LINE_HEIGHT);
    }
}
