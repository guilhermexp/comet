//! Production Worker terminal + private synthetic journal. No Shell, engine,
//! bootstrap, provider, or real PTY. Captures and assertions use this window only.
use gpui::{AppContext, Bounds, WindowBounds, WindowOptions, point, px, size};
use std::{
    fs,
    io::Write,
    path::PathBuf,
    time::{Duration, Instant},
};
use zeron_ui::{icons, theme::Theme, typography, workers::terminal::WorkersTerminal};

fn rows(terminal: &WorkersTerminal) -> Vec<String> {
    terminal
        .active_grid_snapshot()
        .map_or_else(Vec::new, |grid| {
            grid.lines
                .iter()
                .map(|row| {
                    row.iter()
                        .map(|cell| cell.ch)
                        .collect::<String>()
                        .trim_end()
                        .to_owned()
                })
                .collect()
        })
}

async fn wait_offset(
    window: gpui::WindowHandle<WorkersTerminal>,
    cx: &mut gpui::AsyncApp,
    offset: u64,
) -> anyhow::Result<()> {
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        let ready = window.update(cx, |terminal, _, _| {
            terminal.fixture_output_offset() >= offset && terminal.active_grid_snapshot().is_some()
        })?;
        if ready {
            return Ok(());
        }
        anyhow::ensure!(
            Instant::now() < deadline,
            "synthetic journal recovery timed out"
        );
        cx.background_executor()
            .timer(Duration::from_millis(20))
            .await;
    }
}

fn capture(
    window: gpui::WindowHandle<WorkersTerminal>,
    cx: &mut gpui::AsyncApp,
    path: PathBuf,
) -> anyhow::Result<()> {
    let window: gpui::AnyWindowHandle = window.into();
    window.update(cx, |_, window, cx| {
        window.draw(cx).clear();
        window.render_to_image()?.save(path)?;
        Ok(())
    })?
}

fn main() -> anyhow::Result<()> {
    let output = PathBuf::from(std::env::args().nth(1).expect("capture directory"));
    fs::create_dir_all(&output)?;
    let home = tempfile::tempdir()?;
    // Set before GPUI, clients, or background threads. Neither variable points
    // at user data; LocalWorkersClient cannot register ingress in real hooks.
    unsafe {
        std::env::set_var("UNPEEL_HOME", home.path());
        std::env::set_var("COMET_WORKERS_HOOKS_DIR", home.path().join("hooks"));
    }
    let id = "isolated-terminal-history";
    let session = home.path().join("app-sessions").join(id);
    fs::create_dir_all(&session)?;
    let journal = session.join("output.bin");
    let floor = 4096_u64;
    let mut bytes = vec![0; floor as usize];
    bytes.extend(b"EARLIEST AVAILABLE - synthetic fixture\r\n");
    for row in 0..512 {
        bytes.extend(b"\x1b[0m".repeat(4096));
        bytes.extend(format!("Synthetic retained row {row:04}\r\n").bytes());
    }
    // Stale TUI mouse modes at exit exercise local wheel routing.
    bytes.extend(b"\x1b[?1000h\x1b[?1006hLAST RETAINED ROW\r\n");
    fs::write(&journal, &bytes)?;
    fs::write(
        session.join("output-retention.json"),
        format!(r#"{{"version":1,"retained_from":{floor}}}"#),
    )?;
    fs::write(
        session.join("pty-geometry.jsonl"),
        format!("{{\"offset\":{floor},\"cols\":80,\"rows\":24}}\n"),
    )?;
    let end = bytes.len() as u64;
    drop(bytes);
    gpui_platform::application().with_assets(icons::Assets).run(move |cx| {
        let _ = typography::register_fonts(cx);
        cx.set_global(Theme::default());
        let started = Instant::now();
        let window = cx.open_window(WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(Bounds::new(point(px(80.0), px(80.0)), size(px(720.0), px(480.0))))),
            ..Default::default()
        }, |_, cx| cx.new(|cx| {
            let mut terminal = WorkersTerminal::new(cx);
            terminal.set_session(Some(id.into()), cx);
            terminal.set_stopped(true, cx); // No socket/resize/input requests, only journal polling.
            terminal
        })).unwrap();
        cx.activate(true);
        cx.spawn(async move |cx| {
            // Keep the private home alive through the final capture.
            let _home = home;
            let result: anyhow::Result<()> = async {
                wait_offset(window, cx, end).await?;
                let attach_ms = started.elapsed().as_millis();
                capture(window, cx, output.join("01-attached.png"))?;
                window.update(cx, |terminal, _, _| {
                    assert!(rows(terminal).iter().any(|row| row.contains("LAST RETAINED ROW")));
                })?;
                let handle: gpui::AnyWindowHandle = window.into();
                handle.update(cx, |_, window, cx| {
                    window.dispatch_event(gpui::PlatformInput::ScrollWheel(gpui::ScrollWheelEvent {
                        position: point(px(100.0), px(100.0)),
                        delta: gpui::ScrollDelta::Lines(point(0.0, 10_000.0)),
                        modifiers: Default::default(), touch_phase: gpui::TouchPhase::Moved,
                    }), cx);
                })?;
                cx.background_executor().timer(Duration::from_millis(100)).await;
                let anchor = window.update(cx, |terminal, _, _| rows(terminal))?;
                anyhow::ensure!(anchor.first().is_some_and(|row| row.starts_with("EARLIEST AVAILABLE")), "wheel did not reach retained start: {anchor:?}");
                capture(window, cx, output.join("02-earliest.png"))?;
                let live = b"LIVE OUTPUT WHILE READING HISTORY\r\n";
                fs::OpenOptions::new().append(true).open(&journal)?.write_all(live)?;
                wait_offset(window, cx, end + live.len() as u64).await?;
                let after = window.update(cx, |terminal, _, _| rows(terminal))?;
                anyhow::ensure!(anchor == after, "live bytes moved the reader");
                capture(window, cx, output.join("03-live-preserves-anchor.png"))?;
                // Resize only our isolated native window; no PTY exists here.
                handle.update(cx, |_, window, _| window.resize(size(px(380.0), px(480.0))))?;
                cx.background_executor().timer(Duration::from_millis(100)).await;
                capture(window, cx, output.join("04-narrow.png"))?;
                fs::write(output.join("result.txt"), format!("PASS: {end} journal bytes, attach_ms={attach_ms}; earliest row reached by wheel; appended output preserved visible rows; stopped synthetic Worker only\n"))?;
                Ok(())
            }.await;
            match result {
                Ok(()) => { println!("PASS: isolated Worker terminal fixture"); cx.update(|cx| cx.quit()); }
                Err(error) => { eprintln!("FAIL: {error:#}"); drop(_home); std::process::exit(1); }
            }
        }).detach();
    });
    Ok(())
}
