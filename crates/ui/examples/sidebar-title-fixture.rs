//! Native sidebar and title-bar fixture; captures its GPUI window renderer.
//! Run with `--features appshots-fixture,project-palette-fixture`.
use gpui::{AppContext, Bounds, WindowBounds, WindowOptions, px, size};
use std::path::{Path, PathBuf};
use std::time::Duration;
use zeron_ui::*;

fn isolate(root: &Path) -> anyhow::Result<()> {
    for name in ["home", "unpeel", "data"] {
        std::fs::create_dir_all(root.join(name))?;
    }
    // SAFETY: called before the runtime or GPUI starts any threads.
    unsafe {
        std::env::set_var("HOME", root.join("home"));
        std::env::set_var("UNPEEL_HOME", root.join("unpeel"));
        std::env::set_var("ZERON_DATA_DIR", root.join("data"));
        std::env::set_var("ZERON_WORKTREES_DIR", root.join("worktrees"));
        std::env::set_var("ZERON_WORKTREE_OWNERSHIP_FILE", root.join("ownership.json"));
        std::env::set_var("ZERON_AUTO_UPDATE", "0");
        std::env::remove_var("ZERON_SIDEBAR_MODE");
    }
    Ok(())
}

fn main() -> anyhow::Result<()> {
    let mut args = std::env::args().skip(1);
    let output = PathBuf::from(
        args.next()
            .ok_or_else(|| anyhow::anyhow!("missing output PNG"))?,
    );
    let root = PathBuf::from(
        args.next()
            .ok_or_else(|| anyhow::anyhow!("missing isolated state directory"))?,
    );
    let width: f32 = args
        .next()
        .ok_or_else(|| anyhow::anyhow!("missing sidebar width"))?
        .parse()?;
    let light = match args.next().as_deref() {
        Some("light") => true,
        Some("dark") | None => false,
        Some(_) => anyhow::bail!("theme must be dark or light"),
    };
    let compact = match args.next().as_deref() {
        Some("compact") => true,
        Some("normal") | None => false,
        Some(_) => anyhow::bail!("layout must be normal or compact"),
    };
    anyhow::ensure!(args.next().is_none(), "unexpected extra argument");
    anyhow::ensure!(
        [242.0, 256.0, 400.0].contains(&width),
        "unsupported sidebar width"
    );
    anyhow::ensure!(!root.exists(), "fixture state directory must be new");
    std::fs::create_dir_all(&root)?;
    isolate(&root)?;
    let runtime = tokio::runtime::Runtime::new()?;
    let _guard = runtime.enter();
    let data = root.join("data");
    gpui_platform::application().with_assets(icons::Assets).run(move |cx| {
        gpui_tokio::init(cx);
        gpui_base::init(cx);
        let mut settings = settings::UiSettings::default();
        settings.sidebar_width = width;
        settings.sidebar_compact = compact;
        settings.sidebar_show_branch = true;
        settings.sidebar_show_worked_projects = true;
        settings.sidebar_show_pull_request = true;
        settings::init(settings.clone(), data.clone(), cx);
        let fonts = typography::register_fonts(cx);
        typography::init(settings.ui_font_family.clone(), settings.ui_font_size, settings.terminal_font_family.clone(), settings.terminal_font_size, settings.code_font_family.clone(), settings.code_font_size, fonts, cx);
        theme_library::init(data.clone(), cx);
        appearance::init(if light { appearance::AppearanceMode::Light } else { appearance::AppearanceMode::Dark }, settings.theme_selection, settings.accent, settings.surface, cx);
        history::init(settings.git_history_columns, settings.git_history_column_widths, settings.git_history_column_order, settings.git_history_author_display, cx);
        composer::init(cx, settings.composer_send_behavior);
        terminal::panel::init(cx);
        app_menus::init(cx);
        let state = cx.new(|_| {
            let mut s = state::AppState::new();
            s.connection = zeron_proto::view::ConnectionStatus::Ready;
            s.workspace_scope = Some(zeron_proto::WorkspaceScope::Local);
            s.local_device_id = Some("local".into());
            s.devices = vec![serde_json::from_value(serde_json::json!({"id":"local","name":"Fixture device","platform":std::env::consts::OS,"lastSeenAt":null})).unwrap()];
            s.spaces = ["studio", "another-long-project-name"].into_iter().map(|name| serde_json::from_value(serde_json::json!({"id":name,"deviceId":"local","path":format!("/fixture/{name}"),"createdAt":chrono::Utc::now()})).unwrap()).collect();
            for (id, branch, archived) in [("short", "main", false), ("long", "feature/a-very-long-branch-name-for-rendering", false), ("archived", "main", true)] {
                let chat: zeron_proto::Chat = serde_json::from_value(serde_json::json!({
                    "id":id, "title":if id == "short" { "Review [studio](zeron-project:/fixture/studio/)" } else { "Synthetic fixture Chat" },
                    "deviceId":"local", "archived":archived, "createdAt":chrono::Utc::now(),
                    "sourceContext":{"checkoutId":id,"repoRoot":"/fixture","cwd":"/fixture","branch":branch,"observedAt":chrono::Utc::now()},
                    "workedProjects":{"projectIds":["studio","another-long-project-name"],"complete":true},
                    "config":{"harness":"claude-code","model":"fixture","reasoning":null,"sandbox":"workspace-write"}
                })).unwrap();
                if id == "long" {
                    s.fixture_sidebar_change_request(zeron_proto::CheckoutChangeRequestStatus {
                        checkout_id:id.into(), device_id:"local".into(), cwd:"/fixture".into(), branch:branch.into(), updated_at:chrono::Utc::now(),
                        change_request:Some(zeron_proto::ChangeRequestSummary { provider:"github".into(), number:42, title:"Fixture PR".into(), url:"https://example.test/pull/42".into(), state:zeron_proto::ChangeRequestState::Open, base_ref:"main".into(), head_ref:branch.into() }),
                    });
                }
                s.chats.push(chat);
            }
            s.selected_chat = Some("short".into());
            s.auto_selected = true;
            s.chats_synced = true;
            s.spaces_synced = true;
            s
        });
        let boot = EngineBootConfig { data_dir:data, ipc_port:0, edge_url:String::new(), edge_token:None, org_id:None, workos_client_id:None, default_harness:HarnessId::Mock };
        let window = cx.open_window(WindowOptions {
            window_bounds:Some(WindowBounds::Windowed(Bounds::new(gpui::point(px(12.0), px(30.0)), size(px(1100.0), px(800.0))))),
            titlebar:Some(gpui::TitlebarOptions { title:None, appears_transparent:true, traffic_light_position:Some(gpui::point(px(14.0), px(14.0))) }),
            app_owns_titlebar_drag:true,
            window_background:theme::Theme::of(cx).window_background_appearance(),
            ..Default::default()
        }, |_, cx| cx.new(|cx| shell::detached_fixture_shell(state.clone(), boot, cx))).unwrap();
        appearance::reapply_window_background(cx);
        cx.activate(true);
        cx.spawn(async move |cx| {
            cx.background_executor().timer(Duration::from_millis(700)).await;
            let result: anyhow::Result<()> = async {
                gpui::AnyWindowHandle::from(window).update(cx, |_, window, cx| {
                    window.draw(cx).clear();
                    window.render_to_image()?.save(&output)?;
                    anyhow::Ok(())
                })??;
                let image = image::open(&output)?.to_rgb8();
                let first = image.pixels().next().copied();
                anyhow::ensure!(image.pixels().any(|pixel| Some(*pixel) != first), "capture is uniform");
                println!("captured GPUI window renderer {}", output.display());
                Ok(())
            }.await;
            if let Err(error) = result {
                eprintln!("sidebar-title-fixture: {error:#}");
                cx.update(|cx| cx.shutdown());
                std::process::exit(1);
            }
            cx.update(|cx| cx.quit());
        }).detach();
    });
    Ok(())
}
