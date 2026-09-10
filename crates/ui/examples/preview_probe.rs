//! Isolated native preview repaint probe (no daemon or Chat data).
//!
//! `cargo run -p zeron-ui --example preview_probe -- /absolute/report.md`
//! Measures frame intervals, including display scheduling and UI work, after
//! 20 warmup frames. This is not a cold-open or render-only CPU benchmark.
//! Set PREVIEW_FRAME_BUDGET_MS for an opt-in, machine-dependent p95 gate.
use gpui::{
    AppContext, Bounds, Context, Entity, Render, Window, WindowBounds, WindowOptions, div, point,
    prelude::*, px, size,
};
use std::{path::PathBuf, time::Instant};
use zeron_ui::{
    file_preview::view::FilePreview,
    theme::{Appearance, Theme},
};
struct Probe {
    preview: Entity<FilePreview>,
    last: Instant,
    frames: Vec<f64>,
    n: usize,
}
impl Render for Probe {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.frames.push(self.last.elapsed().as_secs_f64() * 1000.);
        self.last = Instant::now();
        self.n += 1;
        if self.n == 140 {
            let mut steady = self.frames[20..].to_vec();
            steady.sort_by(f64::total_cmp);
            println!(
                "PREVIEW_FRAME_MS median={:.2} p95={:.2} max={:.2}",
                steady[steady.len() / 2],
                steady[steady.len() * 95 / 100],
                steady.last().unwrap()
            );
            if let Ok(budget) = std::env::var("PREVIEW_FRAME_BUDGET_MS") {
                if steady[steady.len() * 95 / 100] > budget.parse::<f64>().unwrap() {
                    eprintln!("preview repaint exceeded frame budget");
                    std::process::exit(1);
                }
            }
            std::process::exit(0);
        }
        let weak = cx.weak_entity();
        window.on_next_frame(move |_, cx| {
            weak.update(cx, |this, cx| {
                this.preview.update(cx, |_, cx| cx.notify());
                cx.notify();
            })
            .ok();
        });
        div()
            .size_full()
            .bg(Theme::of(cx).bg)
            .child(self.preview.clone())
    }
}
fn main() {
    let path = PathBuf::from(std::env::args().nth(1).expect("path"));
    gpui_platform::application()
        .with_assets(zeron_ui::icons::Assets)
        .run(move |cx| {
            zeron_ui::typography::register_fonts(cx);
            Theme::install(Appearance::Dark, cx);
            cx.open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(Bounds {
                        origin: point(px(80.), px(80.)),
                        size: size(px(800.), px(900.)),
                    })),
                    ..Default::default()
                },
                |_, cx| {
                    let preview = cx.new(|cx| {
                        let mut p = FilePreview::new();
                        p.open(
                            "probe".into(),
                            path.parent().unwrap().into(),
                            path.file_name().unwrap().to_str().unwrap().into(),
                            cx,
                        );
                        p
                    });
                    cx.new(|_| Probe {
                        preview,
                        last: Instant::now(),
                        frames: vec![],
                        n: 0,
                    })
                },
            )
            .unwrap();
            cx.activate(true);
        });
}
