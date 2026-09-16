#![cfg_attr(target_family = "wasm", no_main)]

//! Two side-by-side windows comparing macOS window-background blur.
//!
//! Left: WindowServer blur with a declared radius of 24.
//! Right: AppKit `UnderWindowBackground` material (no declared radius).
//!
//! Run with:
//!
//! ```sh
//! cargo run -p gpui --example window_blur
//! ```

use gpui::{
    App, Bounds, Context, Pixels, SharedString, TitlebarOptions, Window,
    WindowBackgroundAppearance, WindowBounds, WindowOptions, div, hsla, point, prelude::*, px, rgb,
    size,
};
use gpui_platform::application;

struct BlurLabel {
    label: SharedString,
}

impl Render for BlurLabel {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .size_full()
            .items_center()
            .justify_center()
            .bg(hsla(0.0, 0.0, 0.12, 0.35))
            .text_color(rgb(0xffffff))
            .child(div().text_xl().child(self.label.clone()))
    }
}

fn open_comparison_window(
    cx: &mut App,
    title: &str,
    origin_x: f32,
    blur_radius: Option<Pixels>,
    label: SharedString,
) {
    let window_size = size(px(420.), px(520.));
    let bounds = Bounds::new(point(px(origin_x), px(120.)), window_size);
    cx.open_window(
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            window_background: WindowBackgroundAppearance::Blurred,
            titlebar: Some(TitlebarOptions {
                title: Some(title.into()),
                appears_transparent: true,
                traffic_light_position: None,
            }),
            ..Default::default()
        },
        move |window, cx| {
            window.set_background_blur_radius(blur_radius);
            cx.new(|_| BlurLabel {
                label: label.clone(),
            })
        },
    )
    .unwrap();
}

fn run_example() {
    application().run(|cx: &mut App| {
        open_comparison_window(
            cx,
            "WindowServer radius 24",
            80.0,
            Some(px(24.)),
            "WindowServer radius 24".into(),
        );
        open_comparison_window(
            cx,
            "AppKit material",
            524.0,
            None,
            "AppKit material (no radius)".into(),
        );
        cx.activate(true);
    });
}

#[cfg(not(target_family = "wasm"))]
fn main() {
    run_example();
}

#[cfg(target_family = "wasm")]
#[wasm_bindgen::prelude::wasm_bindgen(start)]
pub fn start() {
    gpui_platform::web_init();
    run_example();
}
