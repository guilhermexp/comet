//! Interactive tool-image preview session with isolated data and mock images.
//! Images load through the real engine RPC (ReadAttachmentChunk) from a temp
//! workspace. Close the window to exit. No agent runs.
use gpui::{AppContext, Bounds, WindowBounds, WindowOptions, px, size};
use image::{ImageEncoder, Rgba, RgbaImage};
use serde_json::json;
use std::{path::Path, sync::Arc};
use zeron_proto::ToolCall;
use zeron_ui::*;

fn port() -> u16 {
    std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

fn gradient(width: u32, height: u32, seed: u8) -> RgbaImage {
    RgbaImage::from_fn(width, height, |x, y| {
        let (fx, fy) = (x as f32 / width as f32, y as f32 / height as f32);
        let ring = (((fx - 0.5).powi(2) + (fy - 0.5).powi(2)).sqrt() * 18.0).sin() > 0.6;
        Rgba([
            (fx * 255.0) as u8 ^ seed,
            (fy * 220.0) as u8,
            if ring { 255 } else { 90 + seed / 2 },
            255,
        ])
    })
}

fn write_png(path: &Path, image: &RgbaImage) -> anyhow::Result<()> {
    std::fs::create_dir_all(path.parent().unwrap())?;
    let file = std::io::BufWriter::new(std::fs::File::create(path)?);
    image::codecs::png::PngEncoder::new_with_quality(
        file,
        image::codecs::png::CompressionType::Fast,
        image::codecs::png::FilterType::NoFilter,
    )
    .write_image(
        image,
        image.width(),
        image.height(),
        image::ExtendedColorType::Rgba8,
    )?;
    Ok(())
}

fn seed_workspace(root: &Path) -> anyhow::Result<()> {
    write_png(&root.join("screenshots/home.png"), &gradient(1600, 1000, 0))?;
    let mut chart = RgbaImage::from_pixel(800, 500, Rgba([24, 24, 28, 255]));
    for (i, h) in [120u32, 260, 180, 420, 330, 210].iter().enumerate() {
        for x in (60 + i as u32 * 120)..(140 + i as u32 * 120) {
            for y in (480 - h)..480 {
                chart.put_pixel(x, y, Rgba([90, 160 + i as u8 * 15, 255, 255]));
            }
        }
    }
    write_png(&root.join("charts/chart.png"), &chart)?;
    write_png(&root.join("user/attached.png"), &gradient(900, 600, 77))?;
    write_png(&root.join("generated/gen.png"), &gradient(1024, 1024, 150))?;
    std::fs::create_dir_all(root.join("photos"))?;
    image::DynamicImage::ImageRgba8(gradient(1920, 1080, 33))
        .to_rgb8()
        .save(root.join("photos/sunset.jpg"))?;
    std::fs::create_dir_all(root.join("assets"))?;
    image::DynamicImage::ImageRgba8(gradient(512, 512, 200)).save(root.join("assets/icon.webp"))?;
    {
        let file = std::fs::File::create(root.join("assets/spinner.gif"))?;
        let mut encoder = image::codecs::gif::GifEncoder::new(file);
        for seed in [10, 120] {
            encoder.encode_frame(image::Frame::new(gradient(300, 300, seed)))?;
        }
    }
    std::fs::write(
        root.join("assets/logo.svg"),
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="420" height="160"><rect width="420" height="160" rx="24" fill="#1d1f27"/><circle cx="80" cy="80" r="48" fill="#7aa2ff"/><text x="150" y="98" font-size="44" fill="#e6e6e6" font-family="Geist">zeron</text></svg>"##,
    )?;
    std::fs::write(root.join("assets/notes.txt"), "not an image")?;
    // Decode-bound check: 15 MP source, previewed as a ≤1280×640 thumbnail.
    write_png(&root.join("big/huge.png"), &gradient(5000, 3000, 5))?;
    for i in 0..12 {
        write_png(
            &root.join(format!("stress/frame-{i:02}.png")),
            &gradient(1400, 900, i * 20),
        )?;
    }
    Ok(())
}

fn tool(id: &str, call: ToolCall, output: &str, is_error: bool) -> serde_json::Value {
    json!({"id": id, "kind": "tool", "call": call, "isError": is_error, "resolved": true, "output": output})
}

async fn pause(cx: &mut gpui::AsyncApp, ms: u64) {
    cx.background_executor()
        .timer(std::time::Duration::from_millis(ms))
        .await;
}

fn capture(
    window: gpui::AnyWindowHandle,
    cx: &mut gpui::AsyncApp,
    dir: &Path,
    name: &str,
) -> anyhow::Result<()> {
    // Best effort: some platforms (Windows) have no render_to_image.
    window.update(cx, |_, w, cx| {
        w.draw(cx).clear();
        if let Ok(image) = w.render_to_image() {
            image.save(dir.join(format!("{name}.png"))).ok();
        }
    })?;
    Ok(())
}

/// Expand, spam-toggle, collapse and copy through the production views,
/// asserting residency at each step. Restores the user's clipboard.
async fn self_check(
    window: gpui::WindowHandle<shell::Shell>,
    dir: &Path,
    chart: &str,
    cx: &mut gpui::AsyncApp,
) -> anyhow::Result<()> {
    std::fs::create_dir_all(dir)?;
    let transcript = window.update(cx, |shell, _, _| shell.fixture_transcript())?;
    let stats =
        |cx: &mut gpui::AsyncApp| transcript.read_with(cx, |t, _| t.fixture_tool_image_stats());
    let snapshot =
        |cx: &mut gpui::AsyncApp| transcript.read_with(cx, |t, _| t.fixture_tool_image_snapshot());
    let toggle = |open: bool, cx: &mut gpui::AsyncApp| {
        transcript.update(cx, |t, cx| t.fixture_tool_images(open, cx))
    };
    let toggle_chart = |open: bool, cx: &mut gpui::AsyncApp| {
        transcript.update(cx, |t, cx| t.fixture_tool_image_chip(chart, open, cx))
    };
    let mut log = String::new();
    let checkpoint = |log: &str| std::fs::write(dir.join("diagnostics.txt"), log);
    pause(cx, 1500).await;
    capture(window.into(), cx, dir, "1-collapsed")?;
    let collapsed = stats(cx);
    log += &format!("collapsed: {collapsed:?}\n");
    log += &format!("collapsed keys: {:?}\n", snapshot(cx));
    checkpoint(&log)?;
    anyhow::ensure!(collapsed == (0, 0), "collapsed chips hold previews");

    toggle(true, cx);
    anyhow::ensure!(
        toggle_chart(true, cx),
        "fixture chart path did not match an image chip"
    );
    let chart_wait_started = std::time::Instant::now();
    let mut expanded_snapshot = snapshot(cx);
    let mut chart_identity = expanded_snapshot
        .iter()
        .find(|(_, path, _, _)| path == chart)
        .map(|(_, _, loads, texture)| (*loads, *texture));
    while !chart_identity.is_some_and(|(loads, texture)| loads > 0 && texture.is_some())
        && chart_wait_started.elapsed() < std::time::Duration::from_secs(20)
    {
        pause(cx, 100).await;
        expanded_snapshot = snapshot(cx);
        chart_identity = expanded_snapshot
            .iter()
            .find(|(_, path, _, _)| path == chart)
            .map(|(_, _, loads, texture)| (*loads, *texture));
    }
    let chart_wait = chart_wait_started.elapsed();
    let expanded = stats(cx);
    log += &format!("chart readiness wait: {chart_wait:?}\n");
    log += &format!("expanded: {expanded:?}\n");
    log += &format!("expanded keys: {expanded_snapshot:?}\n");
    checkpoint(&log)?;
    anyhow::ensure!(
        expanded.0 > 0 && expanded.1 > 0,
        "expanded chips decoded nothing"
    );
    anyhow::ensure!(
        chart_identity.is_some_and(|(loads, texture)| loads > 0 && texture.is_some()),
        "chart did not become ready within 20s: waited={chart_wait:?}, keys={expanded_snapshot:?}"
    );
    capture(window.into(), cx, dir, "2-expanded")?;

    // The chip remains closed past the 400 ms fold tween but reopens within
    // the 600 ms cache grace. Its exact RenderImage and load count must survive.
    anyhow::ensure!(toggle_chart(false, cx), "focused chart chip disappeared");
    pause(cx, 450).await;
    let during_grace = snapshot(cx);
    log += &format!("450ms into chart collapse: {during_grace:?}\n");
    checkpoint(&log)?;
    let grace_identity = during_grace
        .iter()
        .find(|(_, path, _, _)| path == chart)
        .map(|(_, _, loads, texture)| (*loads, *texture));
    anyhow::ensure!(
        grace_identity == chart_identity,
        "chart texture did not survive into its release grace: expanded={chart_identity:?}, during_grace={grace_identity:?}, keys={during_grace:?}"
    );
    anyhow::ensure!(toggle_chart(true, cx), "focused chart chip disappeared");
    pause(cx, 100).await;
    let reopened = snapshot(cx);
    log += &format!("reopened chart: {reopened:?}\n");
    checkpoint(&log)?;
    let reopened_identity = reopened
        .iter()
        .find(|(_, path, _, _)| path == chart)
        .map(|(_, _, loads, texture)| (*loads, *texture));
    anyhow::ensure!(
        reopened_identity == chart_identity,
        "reopening chart reloaded its texture: expanded={chart_identity:?}, reopened={reopened_identity:?}, keys={reopened:?}"
    );

    let started = std::time::Instant::now();
    for i in 0..60 {
        anyhow::ensure!(
            toggle_chart(i % 2 == 1, cx),
            "focused chart chip disappeared during toggle {i}"
        );
        pause(cx, 16).await;
    }
    pause(cx, 300).await;
    let spammed = stats(cx);
    let spammed_snapshot = snapshot(cx);
    log += &format!(
        "after 60 chart toggles in {:?}: {spammed:?}\n",
        started.elapsed()
    );
    log += &format!("after 60 chart toggles keys: {spammed_snapshot:?}\n");
    checkpoint(&log)?;
    let spammed_identity = spammed_snapshot
        .iter()
        .find(|(_, path, _, _)| path == chart)
        .map(|(_, _, loads, texture)| (*loads, *texture));
    anyhow::ensure!(
        spammed_identity == chart_identity,
        "chart preview reloaded or dropped during chip toggles: expanded={chart_identity:?}, after={spammed_identity:?}, keys={spammed_snapshot:?}"
    );

    toggle(false, cx);
    pause(cx, 250).await;
    let grace = stats(cx);
    log += &format!("250ms after full collapse: {grace:?}\n");
    checkpoint(&log)?;
    pause(cx, 1200).await;
    capture(window.into(), cx, dir, "3-collapsed-again")?;
    let freed = stats(cx);
    log += &format!("1450ms after full collapse: {freed:?}\n");
    checkpoint(&log)?;
    anyhow::ensure!(freed == (0, 0), "collapsed previews were not freed");

    let previous = cx.update(|cx| cx.read_from_clipboard());
    gpui::AnyWindowHandle::from(window).update(cx, |_, w, cx| {
        transcript.update(cx, |t, cx| t.fixture_image_menu(chart.into(), None, w, cx))
    })?;
    pause(cx, 2500).await;
    capture(window.into(), cx, dir, "4-menu")?;
    gpui::AnyWindowHandle::from(window).update(cx, |_, w, cx| {
        transcript.update(cx, |t, cx| {
            t.fixture_image_menu(chart.into(), Some(0), w, cx)
        })
    })?;
    let text = cx.update(|cx| cx.read_from_clipboard().and_then(|c| c.text()));
    log += &format!(
        "copy path: {text:?}
"
    );
    anyhow::ensure!(text.as_deref() == Some(chart), "copy path mismatch");
    gpui::AnyWindowHandle::from(window).update(cx, |_, w, cx| {
        transcript.update(cx, |t, cx| {
            t.fixture_image_menu(chart.into(), Some(1), w, cx)
        })
    })?;
    pause(cx, 2000).await;
    let image = cx.update(|cx| {
        cx.read_from_clipboard().and_then(|c| {
            c.entries().iter().find_map(|e| match e {
                gpui::ClipboardEntry::Image(image) => Some(image.bytes().len()),
                _ => None,
            })
        })
    });
    log += &format!(
        "copy image bytes: {image:?}
"
    );
    if let Some(previous) = previous {
        cx.update(|cx| cx.write_to_clipboard(previous));
    }
    anyhow::ensure!(
        image.is_some_and(|n| n > 0),
        "copy image put no image on the clipboard"
    );
    log += "PASS
";
    print!("{log}");
    std::fs::write(dir.join("result.txt"), log)?;
    Ok(())
}

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter("warn,zeron_ui::tool_images=debug")
        .init();
    let temp = tempfile::tempdir()?;
    let workspace = temp.path().join("workspace");
    seed_workspace(&workspace)?;
    let cwd = workspace.to_string_lossy().into_owned();
    let abs = |rel: &str| {
        workspace
            .join(rel.replace('/', std::path::MAIN_SEPARATOR_STR))
            .to_string_lossy()
            .into_owned()
    };
    let runtime = tokio::runtime::Runtime::new()?;
    let core = runtime.block_on(async {
        zeron_engine::EngineCore::assemble(
            &temp.path().join("engine"),
            Arc::new(zeron_engine::default_registry()),
            zeron_proto::HarnessId::ClaudeCode,
            None,
        )
    })?;
    core.workspace.create_chat(
        "tool-images",
        None,
        Some(&core.device_id),
        None,
        Some(cwd.clone()),
    )?;
    core.workspace
        .rename_chat("tool-images", "Tool image previews")?;
    let ipc_port = port();
    let _ipc = runtime.block_on(zeron_engine::serve_ipc(ipc_port, core.rpc_service()))?;
    let data = temp.path().join("ui");
    std::fs::create_dir(&data)?;
    let boot = EngineBootConfig {
        data_dir: data.clone(),
        ipc_port,
        edge_url: String::new(),
        edge_token: None,
        org_id: None,
        workos_client_id: None,
        default_harness: zeron_proto::HarnessId::ClaudeCode,
    };
    let handle = runtime.block_on(state::EngineHandle::bootstrap(boot.clone()))?;
    let chats = core.workspace.read_chats()?;
    let device = core.device_id.clone();

    let read = |path: &str| ToolCall::ReadFile { path: path.into() };
    let exec = |command: &str| ToolCall::Exec {
        command: command.into(),
    };
    let outside = if cfg!(windows) {
        r"C:\Windows\Web\Wallpaper\Windows\img0.jpg"
    } else {
        "/usr/share/backgrounds/outside.png"
    };
    let capture = std::env::args()
        .skip_while(|arg| arg != "--capture")
        .nth(1)
        .map(std::path::PathBuf::from);
    let chart = abs("charts/chart.png");
    let entries = json!([
        {"id": "u1", "role": "user", "createdAt": 1788900000000_i64, "deviceId": device,
         "parts": [{"id": "t", "kind": "text", "text": attachments::with_attachments(
            "Here is the mockup I attached. Render every image the tools touch.",
            &[abs("user/attached.png")])}]},
        {"id": "a1", "role": "assistant", "createdAt": 1788900001000_i64, "deviceId": device, "status": "complete",
         "parts": [
            {"id": "p0", "kind": "text", "text": "Open a tool group, then expand a chip. Its image decodes only while the chip is open. Right-click any image for **Copy path** / **Copy image**."},
            tool("t1", read("screenshots/home.png"), "Read image (1600×1000)", false),
            tool("t2", exec("python scripts/plot.py --out charts/chart.png"), "Saved chart to charts/chart.png", false),
            tool("t3", ToolCall::Mcp { server: "browser".into(), tool: "screenshot".into(),
                input: Some(json!({"path": abs("photos/sunset.jpg"), "fullPage": true})) }, "Captured viewport", false),
            tool("t4", exec("ls assets"), "icon.webp\nlogo.svg\nnotes.txt\nspinner.gif", false),
            tool("t5", read("big/huge.png"), "Read image (5000×3000)", false),
            tool("t6", read("missing/nope.png"), "File not found", true),
            tool("t7", read(outside), "Read image", false),
            tool("t8", read("src/main.rs"), "fn main() {}", false),
            {"id": "p1", "kind": "text", "text": "Above: PNG, chart, JPEG via MCP, WebP/SVG/GIF from output, a 15 MP PNG, a missing file and a path outside the workspace jail (both show *Image unavailable*)."},
            {"id": "g1", "kind": "image", "path": abs("generated/gen.png"), "name": "gen.png", "mimeType": "image/png"},
         ]},
        {"id": "a2", "role": "assistant", "createdAt": 1788900002000_i64, "deviceId": device, "status": "complete",
         "parts": (0..12).map(|i| tool(&format!("s{i}"), read(&format!("stress/frame-{i:02}.png")), "Read image (1400×900)", false))
            .chain(std::iter::once(json!({"id": "p2", "kind": "text", "text": "Stress group: spam-toggle these chips. Previews reuse their texture inside the 600 ms grace and free after it (watch the `tool_images` debug log)."})))
            .collect::<Vec<_>>()},
        {"id": "a3", "role": "assistant", "createdAt": 1788900003000_i64, "deviceId": device, "status": "complete",
         "parts": std::iter::once(json!({"id": "p3", "kind": "text", "text": "Fixture lifecycle rows: open Details > Workers > Subagents. Running stays visible; click the collapsed Finished disclosure to reveal Completed, Failed and Cancelled. Completed has more than one page; scroll and use Show more."}))
            .chain((0..17).flat_map(|index| {
                let status = match index {
                    0 => "running",
                    1..=13 => "completed",
                    14..=15 => "failed",
                    _ => "cancelled",
                };
                let task_id = format!("fixture-subagent-{index:02}");
                let description = format!("Native fixture {status} subagent row {index:02}");
                let workflow = json!({"id": format!("workflow-{task_id}"), "kind": "workflowTask",
                    "task": {"taskId": task_id, "status": status, "description": description,
                        "taskType": "subagent", "subagentType": format!("Fixture {status} {index:02}")}});
                let mut parts = vec![workflow];
                let spawn_status = match status {
                    "running" => Some("running"),
                    "completed" => Some("done"),
                    "failed" => Some("failed"),
                    "cancelled" => None,
                    _ => unreachable!(),
                };
                let mut spawn = tool(
                    &task_id,
                    ToolCall::Unknown {
                        name: format!("Agent: {description}"),
                        input: None,
                    },
                    &description,
                    false,
                );
                spawn["subagentRef"] = json!(format!("chat--sub--{task_id}"));
                if let Some(spawn_status) = spawn_status {
                    spawn["subagentStatus"] = json!(spawn_status);
                }
                spawn["subagentTail"] = json!(description);
                parts.push(spawn);
                parts
            }))
            .collect::<Vec<_>>()},
    ]);

    gpui_platform::application()
        .with_assets(icons::Assets)
        .run(move |cx| {
            eprintln!("tool-images fixture: GPUI run callback started");
            gpui_tokio::init(cx);
            gpui_base::init(cx);
            let settings = settings::UiSettings::default();
            settings::init(settings.clone(), data.clone(), cx);
            let fonts = typography::register_fonts(cx);
            typography::init(
                settings.ui_font_family.clone(),
                settings.ui_font_size,
                settings.terminal_font_family.clone(),
                settings.terminal_font_size,
                settings.code_font_family.clone(),
                settings.code_font_size,
                fonts,
                cx,
            );
            theme_library::init(data.clone(), cx);
            appearance::init(
                appearance::AppearanceMode::Dark,
                settings.theme_selection,
                settings.accent,
                settings.surface,
                cx,
            );
            history::init(
                settings.git_history_columns,
                settings.git_history_column_widths,
                settings.git_history_column_order,
                settings.git_history_author_display,
                cx,
            );
            composer::init(cx, settings.composer_send_behavior);
            terminal::panel::init(cx);
            app_menus::init(cx);
            let state = cx.new(|_| {
                let mut s = state::AppState::new();
                s.fixture_attachment_engine(handle);
                s.connection = zeron_proto::view::ConnectionStatus::Ready;
                s.workspace_scope = Some(zeron_proto::WorkspaceScope::Development);
                s.local_device_id = Some(device.clone());
                s.devices = vec![
                    serde_json::from_value(json!({"id": device, "name": "This device",
                    "platform": std::env::consts::OS, "lastSeenAt": null}))
                    .unwrap(),
                ];
                s.chats = chats;
                s.selected_chat = Some("tool-images".into());
                s.auto_selected = true;
                s.chats_synced = true;
                s.spaces_synced = true;
                s.no_project = true;
                s
            });
            let window = cx
                .open_window(
                    WindowOptions {
                        window_background: theme::Theme::of(cx).window_background_appearance(),
                        titlebar: Some(gpui::TitlebarOptions {
                            title: Some("Comet Sync Fixture".into()),
                            ..Default::default()
                        }),
                        window_bounds: Some(WindowBounds::Windowed(Bounds::new(
                            gpui::point(px(40.), px(40.)),
                            size(px(1200.), px(900.)),
                        ))),
                        ..Default::default()
                    },
                    |_, cx| cx.new(|cx| shell::test_shell(state.clone(), boot, cx)),
                )
                .unwrap();
            eprintln!("tool-images fixture: window opened");
            cx.on_window_closed(|cx, _| cx.quit()).detach();
            state.update(cx, |_, cx| cx.notify());
            cx.activate(true);
            cx.spawn(async move |cx| {
                cx.background_executor()
                    .timer(std::time::Duration::from_millis(800))
                    .await;
                state.update(cx, |s, cx| {
                    s.receive_transcript_frame(
                        zeron_doc::TranscriptFrame::Reset {
                            reset: serde_json::from_value(entries).unwrap(),
                        },
                        cx,
                    )
                    .unwrap();
                    cx.notify();
                });
                if let Some(dir) = capture {
                    if let Err(error) = self_check(window, &dir, &chart, cx).await {
                        eprintln!("self-check failed: {error:#}");
                        std::fs::write(
                            dir.join("result.txt"),
                            format!(
                                "FAILED: {error:#}
"
                            ),
                        )
                        .ok();
                    }
                    cx.update(|cx| cx.quit());
                }
            })
            .detach();
        });
    runtime.block_on(core.shutdown());
    Ok(())
}
