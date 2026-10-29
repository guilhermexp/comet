//! Native rendering of production error cards; no Shell, engine or providers.
use gpui::{
    AppContext, Bounds, Context, IntoElement, Render, Window, WindowBounds, WindowOptions, div,
    point, prelude::*, px, size,
};
use std::{path::PathBuf, time::Duration};
use zeron_ui::{icons, theme::Theme, transcript::provider_limit_fixture, typography};

struct Fixture;
impl Render for Fixture {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = Theme::of(cx);
        div()
            .size_full()
            .p(px(24.0))
            .bg(theme.surface)
            .child(provider_limit_fixture(theme))
    }
}

fn main() {
    let output = PathBuf::from(std::env::args().nth(1).expect("PNG path"));
    if let Some(parent) = output.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    gpui_platform::application()
        .with_assets(icons::Assets)
        .run(move |cx| {
            let _ = typography::register_fonts(cx);
            cx.set_global(Theme::default());
            let window = cx
                .open_window(
                    WindowOptions {
                        window_bounds: Some(WindowBounds::Windowed(Bounds::new(
                            point(px(60.0), px(60.0)),
                            size(px(720.0), px(300.0)),
                        ))),
                        ..Default::default()
                    },
                    |_, cx| cx.new(|_| Fixture),
                )
                .unwrap();
            let window: gpui::AnyWindowHandle = window.into();
            cx.activate(true);
            cx.spawn(async move |cx| {
                cx.background_executor()
                    .timer(Duration::from_millis(300))
                    .await;
                let captured = window.update(cx, |_, window, cx| -> anyhow::Result<()> {
                    window.draw(cx).clear();
                    window.render_to_image()?.save(&output)?;
                    Ok(())
                });
                if let Err(error) = captured.and_then(|result| result) {
                    eprintln!("FAIL: native provider limit capture: {error:#}");
                    std::process::exit(1);
                }
                println!("PASS: native provider limit fixture {}", output.display());
                cx.update(|cx| cx.quit());
            })
            .detach();
        });
}
