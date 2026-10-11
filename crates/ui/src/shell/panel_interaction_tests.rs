//! The panel lifecycle through Shell's actions and actual GPUI layout.
use super::*;
use gpui::{AppContext, Bounds, TestAppContext, VisualTestContext};
use zeron_doc::{MessagePart, MessageRole, SessionMessageEntry};
use zeron_proto::ToolCall;
use zeron_workers_unpeel::{WorkersProject, WorkersSessionSort};

fn setup(cx: &mut TestAppContext) -> (tempfile::TempDir, Entity<Shell>, &mut VisualTestContext) {
    let dir = tempfile::tempdir().unwrap();
    cx.update(|cx| {
        gpui_base::init(cx);
        cx.set_global(Theme::default());
        crate::app_menus::init(cx);
        crate::history::init(
            Default::default(),
            Default::default(),
            Default::default(),
            Default::default(),
            cx,
        );
        let mut config = UiSettings::default();
        config.sidebar_width = 300.0;
        config.files_panel_width = 377.0;
        settings::init(config, dir.path(), cx);
    });
    let (shell, cx) = cx.add_window_view(|_, cx| {
        let state = cx.new(|_| {
            let mut state = AppState::new();
            state.no_project = true;
            state.selected_chat = Some("a".into());
            state.chats = ["a", "b"]
                .into_iter()
                .map(|id| {
                    serde_json::from_value(serde_json::json!({
                        "id": id, "title": id, "deviceId": "local", "archived": false,
                        "cwd": dir.path().to_string_lossy().to_string(),
                        "createdAt": Utc::now(),
                    }))
                    .unwrap()
                })
                .collect();
            state
        });
        let mut shell = test_shell(
            state,
            EngineBootConfig {
                data_dir: dir.path().into(),
                ipc_port: 0,
                edge_url: String::new(),
                edge_token: None,
                org_id: None,
                workos_client_id: None,
                default_harness: zeron_proto::HarnessId::Mock,
            },
            cx,
        );
        shell.debug_gate = Some(GatePhase::Ready);
        shell.splash = SplashPhase::Gone;
        shell.active_chat = "a".into();
        shell.sidebar_opened_at = 1;
        shell.panel_open_sequence = 1;
        shell.test_time = Some(std::time::Instant::now());
        shell
    });
    cx.simulate_resize(gpui::size(px(1600.0), px(900.0)));
    (dir, shell, cx)
}

fn change(
    shell: &Entity<Shell>,
    cx: &mut VisualTestContext,
    action: impl FnOnce(&mut Shell, &mut Window, &mut Context<Shell>),
) {
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            action(shell, window, cx);
            cx.notify();
        })
    });
}

enum Panel {
    Sidebar,
    Files,
    Right,
}

impl Panel {
    fn toggle(self, shell: &mut Shell, window: &mut Window, cx: &mut Context<Shell>) {
        match self {
            Self::Sidebar => shell.toggle_sidebar(cx),
            Self::Files => shell.toggle_files_panel(window, cx),
            Self::Right => shell.toggle_right_pane(window, cx),
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct Frame {
    // Sidebar, Files, surface host, visible conversation.
    widths: [f32; 4],
    columns: [Option<Bounds<Pixels>>; 4],
    composer: Bounds<Pixels>,
    strip: Option<Bounds<Pixels>>,
}

fn draw(shell: &Entity<Shell>, cx: &mut VisualTestContext, millis: u64) -> Frame {
    shell.update(cx, |shell, cx| {
        *shell.test_time.as_mut().unwrap() +=
            Duration::from_millis(millis).mul_f32(motion::speed_scale());
        cx.notify();
    });
    cx.update(|window, cx| window.draw(cx).clear());
    let columns = [
        "sidebar-column",
        "files-panel",
        "right-panel",
        "conversation-column",
    ]
    .map(|id| cx.debug_bounds(id));
    Frame {
        widths: columns.map(|bounds| bounds.map_or(0.0, |b| f32::from(b.size.width))),
        columns,
        composer: shell.read_with(cx, |s, cx| {
            s.composer.read(cx).surface_bounds().get().unwrap()
        }),
        strip: cx.debug_bounds("right-titlebar-controls"),
    }
}

#[track_caller]
fn near(actual: f32, expected: f32) {
    assert!(
        (actual - expected).abs() <= 0.5,
        "expected {expected}px, got {actual}px"
    );
}

#[track_caller]
fn expect(frame: Frame, widths: [f32; 4]) {
    for (actual, expected) in frame.widths.into_iter().zip(widths) {
        near(actual, expected);
    }
}

#[track_caller]
fn continuous(before: Frame, after: Frame) {
    expect(after, before.widths);
    let before_left = f32::from(before.composer.left());
    let after_left = f32::from(after.composer.left());
    assert!(
        (after_left - before_left).abs() <= 0.5,
        "composer left changed in a zero-time panel frame: {before_left}px -> {after_left}px; before={before:#?}; after={after:#?}"
    );
    near(
        f32::from(after.composer.size.width),
        f32::from(before.composer.size.width),
    );
    if let (Some(before), Some(after)) = (before.strip, after.strip) {
        near(f32::from(after.left()), f32::from(before.left()));
    }
}

fn settled(shell: &Entity<Shell>, cx: &mut VisualTestContext) -> Frame {
    // A closing column may make room for a hidden neighbour to return.
    draw(shell, cx, 600);
    draw(shell, cx, 600)
}

fn select(shell: &mut Shell, chat: Option<&str>, cx: &mut Context<Shell>) {
    let state = shell.state.clone();
    state.update(cx, |s, cx| s.select_chat(chat.map(str::to_owned), cx));
    shell.on_state_changed(&state, cx);
}

#[gpui::test]
fn panels_share_space_and_keep_rendered_geometry_through_their_lifecycle(cx: &mut TestAppContext) {
    let (_dir, shell, cx) = setup(cx);
    // A right-pane toggle can only open a live surface in the fork. Open a
    // real Browser tab through Shell's normal lifecycle, then hide its pane
    // while keeping the tab. The empty-Picker behavior is covered separately.
    change(&shell, cx, |shell, window, cx| {
        shell.add_browser_surface(None, window, cx);
        shell.set_surfaces_open(false, cx);
        // This fixture starts from a settled closed state; the loop below
        // measures the next user-driven opening transition.
        shell.right_tween = None;
        shell.right_content_tween = None;
    });
    let initial = settled(&shell, cx);
    expect(initial, [300.0, 0.0, 0.0, 1300.0]);

    // Each column responds gently, with no click jump or overshoot. Measure
    // the rendered widths, not a second implementation of the easing curve.
    for (panel, column, target) in [
        (Panel::Right, 2, 650.0),
        (Panel::Files, 1, 377.0),
        (Panel::Sidebar, 0, 0.0),
    ] {
        let before = draw(&shell, cx, 0);
        change(&shell, cx, |s, window, cx| panel.toggle(s, window, cx));
        continuous(before, draw(&shell, cx, 0));
        let source = before.widths[column];
        let first = draw(&shell, cx, 16);
        let progress = (first.widths[column] - source) / (target - source);
        assert!(
            progress > 0.0 && progress < 0.1,
            "column {column} lurches: {source}px -> {}px toward {target}px in 16ms (progress {progress})",
            first.widths[column]
        );
        let mut previous = first.widths[column];
        for _ in 0..15 {
            let frame = draw(&shell, cx, 16);
            let width = frame.widths[column];
            assert!(width >= source.min(target) - 0.5 && width <= source.max(target) + 0.5);
            assert!((width - previous) * (target - source) >= -0.5);
            // The visible composer follows its column throughout panel motion.
            let composer_width = f32::from(frame.composer.size.width);
            near(
                composer_width,
                (frame.widths[3] - 2.0 * Theme::SPACE_LG)
                    .min(f32::from(initial.composer.size.width)),
            );
            near(
                f32::from(frame.composer.left()),
                frame.widths[0] + (frame.widths[3] - composer_width) / 2.0,
            );
            if frame.widths[2] > 128.0 {
                let strip = frame.strip.unwrap();
                near(f32::from(strip.left()), frame.widths[0] + frame.widths[3]);
                near(f32::from(strip.size.width), frame.widths[2]);
            }
            previous = width;
        }
        near(previous, target);
    }
    let wide = settled(&shell, cx);
    expect(wide, [0.0, 377.0, 611.5, 611.5]);
    near(f32::from(wide.strip.unwrap().left()), 611.5);
    // A collapsed sidebar leaves the left edge to the window's own resize.
    let edge = |x| gpui::point(px(x), px(400.0));
    cx.simulate_mouse_down(edge(3.0), MouseButton::Left, Default::default());
    for x in [40.0, 80.0] {
        cx.simulate_mouse_move(edge(x), Some(MouseButton::Left), Default::default());
    }
    cx.simulate_mouse_up(edge(80.0), MouseButton::Left, Default::default());
    expect(settled(&shell, cx), wide.widths);

    // Mid-flight reversal, then a different panel interrupting the return.
    change(&shell, cx, |s, window, cx| s.toggle_right_pane(window, cx));
    let closing = draw(&shell, cx, 75);
    change(&shell, cx, |s, window, cx| s.toggle_right_pane(window, cx));
    continuous(closing, draw(&shell, cx, 0));
    let reopening = draw(&shell, cx, 40);
    change(&shell, cx, |s, _, cx| s.toggle_sidebar(cx));
    continuous(reopening, draw(&shell, cx, 0));
    expect(settled(&shell, cx), [300.0, 377.0, 461.5, 461.5]);

    // A drag holds until a toggle; a seam reset restores the allocation.
    let seam = gpui::point(px(761.5), px(400.0));
    let dragged = gpui::point(px(723.0), px(400.0));
    cx.simulate_mouse_down(seam, MouseButton::Left, Default::default());
    for _ in 0..2 {
        cx.simulate_mouse_move(dragged, Some(MouseButton::Left), Default::default());
    }
    cx.simulate_mouse_up(dragged, MouseButton::Left, Default::default());
    expect(draw(&shell, cx, 0), [300.0, 377.0, 500.0, 423.0]);
    expect(draw(&shell, cx, 300), [300.0, 377.0, 500.0, 423.0]);
    // A stored Changes flag is not a live surface in the fork. Select B first,
    // then open a real Browser tab through the same lifecycle as the app.
    change(&shell, cx, |s, window, cx| {
        s.panels.update("b", |p| p.changes_open = true);
        select(s, Some("b"), cx);
        assert!(!s.right_pane_open(cx));
        s.add_browser_surface(None, window, cx);
        assert!(s.right_pane_open(cx));
    });
    expect(settled(&shell, cx), [300.0, 0.0, 500.0, 800.0]);
    change(&shell, cx, |s, window, cx| {
        // Close B through the same action as the titlebar/keyboard. The tab
        // stays owned by B, but its pane visibility is per-chat state.
        s.toggle_right_pane(window, cx);
        assert!(!s.right_pane_open(cx));
        select(s, Some("a"), cx);
    });
    expect(settled(&shell, cx), [300.0, 377.0, 500.0, 423.0]);
    change(&shell, cx, |s, _, cx| {
        s.reset_panel_widths(PaneResizeKind::Right, cx)
    });
    expect(settled(&shell, cx), [300.0, 377.0, 461.5, 461.5]);
    change(&shell, cx, |s, _, cx| {
        s.reset_panel_widths(PaneResizeKind::Files, cx)
    });
    expect(settled(&shell, cx), [300.0, 286.0, 507.0, 507.0]);

    // Narrowing hides the oldest open column without closing it. Asking
    // for it promotes it; closing a neighbour makes room for its return.
    cx.simulate_resize(gpui::size(px(1000.0), px(900.0)));
    let narrow = draw(&shell, cx, 0);
    expect(narrow, [300.0, 0.0, 360.0, 340.0]);
    assert!(shell.read_with(cx, |s, cx| s.files_panel_open(cx)));
    change(&shell, cx, |s, window, cx| s.toggle_files_panel(window, cx));
    expect(settled(&shell, cx), [300.0, 286.0, 0.0, 414.0]);
    change(&shell, cx, |s, window, cx| s.toggle_files_panel(window, cx));
    expect(settled(&shell, cx), [300.0, 0.0, 360.0, 340.0]);
    cx.simulate_resize(gpui::size(px(1600.0), px(900.0)));
    change(&shell, cx, |s, window, cx| s.toggle_files_panel(window, cx));
    let ordinary = settled(&shell, cx);
    expect(ordinary, [300.0, 286.0, 507.0, 507.0]);
    // The sidebar's seam follows the pointer, not a tween behind it.
    let sidebar_seam = |x| gpui::point(px(x), px(400.0));
    cx.simulate_mouse_down(sidebar_seam(300.0), MouseButton::Left, Default::default());
    // The first move starts the drag; the following ones move the seam.
    for x in [310.0, 320.0, 340.0] {
        cx.simulate_mouse_move(sidebar_seam(x), Some(MouseButton::Left), Default::default());
        let width = draw(&shell, cx, 16).widths[0];
        if x > 310.0 {
            near(width, x);
        }
    }
    cx.simulate_mouse_move(
        sidebar_seam(300.0),
        Some(MouseButton::Left),
        Default::default(),
    );
    cx.simulate_mouse_up(sidebar_seam(300.0), MouseButton::Left, Default::default());
    // A squeezed sidebar lays its rows and menus out within its column.
    cx.simulate_resize(gpui::size(px(1130.0), px(900.0)));
    near(draw(&shell, cx, 0).widths[0], 250.0);
    near(shell.read_with(cx, |s, _| s.sidebar_content_width()), 250.0);
    cx.simulate_resize(gpui::size(px(1600.0), px(900.0)));
    expect(settled(&shell, cx), ordinary.widths);

    // Fullscreen clips the conversation, retaining its composer layout.
    change(&shell, cx, |s, _, cx| s.toggle_right_pane_expand(cx));
    for millis in [0, 75, 180] {
        let frame = draw(&shell, cx, millis);
        near(
            f32::from(frame.composer.size.width),
            f32::from(ordinary.composer.size.width),
        );
    }
    expect(draw(&shell, cx, 0), [300.0, 286.0, 1014.0, 0.0]);
    change(&shell, cx, |s, _, cx| select(s, Some("b"), cx));
    // B's Browser tab remains live, but its pane was explicitly closed above.
    expect(settled(&shell, cx), [300.0, 0.0, 0.0, 1300.0]);
    assert!(!shell.read_with(cx, |s, cx| s.right_pane_open(cx)));
    assert!(!shell.read_with(cx, |s, _| s.right_pane_expanded));
    change(&shell, cx, |s, _, cx| select(s, Some("a"), cx));
    expect(draw(&shell, cx, 0), [300.0, 286.0, 1014.0, 0.0]);
    change(&shell, cx, |s, _, cx| s.toggle_right_pane_expand(cx));
    expect(settled(&shell, cx), ordinary.widths);

    // Navigation interrupts a close and lands directly on the destination.
    change(&shell, cx, |s, window, cx| s.toggle_right_pane(window, cx));
    draw(&shell, cx, 75);
    change(&shell, cx, |s, _, cx| select(s, None, cx));
    expect(draw(&shell, cx, 0), [300.0, 0.0, 0.0, 1300.0]);
    expect(settled(&shell, cx), [300.0, 0.0, 0.0, 1300.0]);
    change(&shell, cx, |s, _, cx| select(s, Some("a"), cx));
    expect(settled(&shell, cx), [300.0, 286.0, 0.0, 1014.0]);
    change(&shell, cx, |s, _, cx| {
        s.open_settings(SettingsSection::Appearance, cx)
    });
    draw(&shell, cx, 0);
    assert!(cx.debug_bounds("conversation-column").is_none());
    change(&shell, cx, |s, _, cx| s.close_settings(cx));
    expect(draw(&shell, cx, 0), [300.0, 286.0, 0.0, 1014.0]);

    // Window resizing and reduced motion apply directly, without a lagging tween.
    for width in [1024.0, 800.0, 600.0, 800.0, 1600.0] {
        cx.simulate_resize(gpui::size(px(width), px(900.0)));
        let frame = draw(&shell, cx, 0);
        near(frame.widths.iter().sum(), width);
        expect(draw(&shell, cx, 300), frame.widths);
    }
    // A side chat's composer keeps its column's layout under the pane's mask.
    change(&shell, cx, |s, _, cx| {
        let chat = serde_json::from_value(serde_json::json!({
            "id": "side", "parentChatId": "a", "deviceId": "local",
            "archived": false, "createdAt": Utc::now(),
        }))
        .unwrap();
        s.open_side_chat(chat, s.panel_key(cx), cx);
    });
    settled(&shell, cx);
    let side_composer = |cx: &mut VisualTestContext| {
        shell.read_with(cx, |s, cx| {
            let tab = s.side_chats.values().next().unwrap();
            tab.composer.read(cx).surface_bounds().get().unwrap()
        })
    };
    let resting = side_composer(cx);
    for _ in 0..2 {
        change(&shell, cx, |s, window, cx| s.toggle_right_pane(window, cx));
        draw(&shell, cx, 75);
        let moving = side_composer(cx);
        near(f32::from(moving.size.width), f32::from(resting.size.width));
        near(
            f32::from(moving.size.height),
            f32::from(resting.size.height),
        );
        settled(&shell, cx);
    }
    change(&shell, cx, |s, window, cx| s.toggle_right_pane(window, cx));
    settled(&shell, cx);
    cx.update(|_, cx| motion::set_reduced_motion(cx, true));
    for panel in [
        Panel::Sidebar,
        Panel::Right,
        Panel::Files,
        Panel::Files,
        Panel::Right,
        Panel::Sidebar,
    ] {
        change(&shell, cx, |s, window, cx| panel.toggle(s, window, cx));
        let landed = draw(&shell, cx, 0);
        near(landed.widths.iter().sum(), 1600.0);
        expect(settled(&shell, cx), landed.widths);
    }
}

#[gpui::test]
fn resetting_details_width_restores_default_and_preserves_right_preference(
    cx: &mut TestAppContext,
) {
    let (_dir, shell, cx) = setup(cx);
    shell.update(cx, |shell, cx| {
        shell.settings.details_sidebar_open = true;
        shell.settings.details_sidebar_width = 380.0;
        shell.settings.right_pane_width = 460.0;
        shell.details_tween = None;
        shell.reset_panel_widths(PaneResizeKind::Details, cx);
        assert_eq!(
            shell.settings.details_sidebar_width,
            DETAILS_SIDEBAR_DEFAULT
        );
        assert_eq!(shell.settings.right_pane_width, 460.0);
    });
}

#[gpui::test]
fn details_owns_session_location_metadata_while_session_actions_remain(cx: &mut TestAppContext) {
    let (_dir, shell, cx) = setup(cx);
    shell.update(cx, |shell, cx| {
        shell.settings.session_info_open = true;
        shell.settings.details_sidebar_open = true;
        shell.session_info_tween = None;
        shell.details_tween = None;
        cx.notify();
    });
    cx.update(|window, cx| window.draw(cx).clear());

    assert!(cx.debug_bounds("session-info").is_some());
    assert!(cx.debug_bounds("session-info-device").is_none());
    assert!(cx.debug_bounds("session-info-side-chat").is_some());

    // Details remains opted in, but at 620px its responsive budget is below
    // the minimum even after the sidebar is hidden; SessionInfo keeps the
    // location metadata only when Details actually has no width.
    cx.simulate_resize(gpui::size(px(620.0), px(900.0)));
    shell.read_with(cx, |shell, cx| assert_eq!(shell.details_target(cx), 0.0));
    cx.update(|window, cx| window.draw(cx).clear());
    assert!(cx.debug_bounds("session-info-device").is_some());
    cx.simulate_resize(gpui::size(px(1600.0), px(900.0)));
    cx.update(|window, cx| window.draw(cx).clear());
    assert!(cx.debug_bounds("session-info-device").is_none());

    shell.update(cx, |shell, cx| {
        shell.settings.details_sidebar_open = false;
        shell.details_tween = None;
        cx.notify();
    });
    cx.update(|window, cx| window.draw(cx).clear());
    assert!(cx.debug_bounds("session-info-device").is_some());
    assert!(cx.debug_bounds("session-info-side-chat").is_some());
}

#[gpui::test]
fn worked_project_diff_watch_tracks_effective_details_visibility(cx: &mut TestAppContext) {
    fn project(id: &str, path: &str) -> WorkersProject {
        WorkersProject {
            id: id.into(),
            name: id.into(),
            path: path.into(),
            folder_id: None,
            parent_project_id: None,
            is_group: false,
            worktree_branch: None,
            git_branch: None,
            archived_session_count: 0,
            folder_color_id: None,
            session_sort: WorkersSessionSort::Custom,
            repository_id: None,
            repository_name: None,
            repository_path: None,
            checkout_kind: None,
            checkout_ownership: None,
            checkout_availability: None,
            checkout_archived: false,
            checkout_detached: false,
            space_id: None,
        }
    }

    fn entry(paths: [&str; 2]) -> SessionMessageEntry {
        SessionMessageEntry {
            id: "msg-1".into(),
            role: MessageRole::Assistant,
            parts: paths
                .into_iter()
                .enumerate()
                .map(|(index, path)| MessagePart::Tool {
                    id: format!("tool-{index}"),
                    call: ToolCall::ReadFile { path: path.into() },
                    diff: None,
                    output: None,
                    output_ref: None,
                    output_bytes: None,
                    diff_ref: None,
                    diff_stats: None,
                    file_preview: None,
                    subagent_ref: None,
                    subagent_status: None,
                    subagent_tail: None,
                    subagent_end: None,
                    execution: None,
                    resolved: true,
                    is_error: false,
                })
                .collect(),
            created_at: 0,
            device_id: "local".into(),
            status: None,
            duration_ms: None,
            continuation_of: None,
        }
    }

    fn drain_requests(
        requests: &mut tokio::sync::mpsc::Receiver<String>,
        replies: &tokio::sync::mpsc::Sender<String>,
        watches: &mut Vec<u64>,
        cancellations: &mut Vec<u64>,
    ) {
        while let Ok(raw) = requests.try_recv() {
            let request: zeron_rpc::ClientFrame = serde_json::from_str(&raw).unwrap();
            if request.cancel {
                cancellations.push(request.id);
                continue;
            }
            if request.method.as_deref() == Some(methods::WATCH_CHECKOUT_DIFFS)
                && request.params.get("cwds").is_some()
            {
                assert_eq!(
                    request.params["cwds"],
                    serde_json::json!(["/tmp/worked-first", "/tmp/worked-second"])
                );
                watches.push(request.id);
            }
            replies
                .try_send(
                    serde_json::to_string(&zeron_rpc::ServerFrame {
                        id: request.id,
                        ok: Some(serde_json::Value::Null),
                        ..Default::default()
                    })
                    .unwrap(),
                )
                .unwrap();
        }
    }

    fn settle_watch(
        shell: &Entity<Shell>,
        cx: &mut VisualTestContext,
        runtime: &tokio::runtime::Runtime,
        requests: &mut tokio::sync::mpsc::Receiver<String>,
        replies: &tokio::sync::mpsc::Sender<String>,
        watches: &mut Vec<u64>,
        cancellations: &mut Vec<u64>,
    ) {
        for _ in 0..20 {
            shell.update(cx, |_, cx| cx.notify());
            cx.update(|window, cx| window.draw(cx).clear());
            cx.run_until_parked();
            runtime.block_on(tokio::task::yield_now());
            drain_requests(requests, replies, watches, cancellations);
        }
    }

    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let _guard = runtime.enter();
    let (_dir, shell, cx) = setup(cx);
    let (out, mut requests) = tokio::sync::mpsc::channel::<String>(128);
    let (replies, inbound) = tokio::sync::mpsc::channel::<String>(128);
    change(&shell, cx, |shell, _, cx| {
        shell.route = Route::Chat;
        shell.sidebar_mode = SidebarMode::Orchestrator;
        shell.debug_gate = Some(GatePhase::Ready);
        shell.splash = SplashPhase::Gone;
        shell.settings.details_sidebar_open = true;
        shell.details_tween = None;
        settings::update(settings::SavePolicy::Immediate, cx, |settings| {
            settings.details_sidebar_open = true;
        });
        shell.state.update(cx, |state, _| {
            state.connection = crate::state::ConnectionStatus::Ready;
            state.workspace_scope = Some(zeron_proto::WorkspaceScope::Local);
            state.local_device_id = Some("local".into());
            state.selected_chat = Some("a".into());
            state.transcript = vec![entry([
                "/tmp/worked-first/file.txt",
                "/tmp/worked-second/file.txt",
            ])];
            state.set_test_engine(crate::state::EngineHandle::from_test_client(
                zeron_rpc::RpcClient::new(out, inbound),
            ));
        });
        shell.workers_model.update(cx, |workers, _| {
            workers.snapshot = Some(zeron_workers_unpeel::WorkersBootstrap {
                mac_name: "fixture".into(),
                protocol: zeron_workers_unpeel::WorkersProtocol {
                    major_version: 1,
                    minor_version: 0,
                    capabilities: vec![],
                },
                projects: vec![
                    project("first", "/tmp/worked-first"),
                    project("second", "/tmp/worked-second"),
                ],
                presets: vec![],
                sessions: vec![],
                activity_log: vec![],
            });
        });
    });

    cx.simulate_resize(gpui::size(px(400.0), px(900.0)));
    let mut watches = Vec::new();
    let mut cancellations = Vec::new();
    settle_watch(
        &shell,
        cx,
        &runtime,
        &mut requests,
        &replies,
        &mut watches,
        &mut cancellations,
    );
    assert!(
        watches.is_empty(),
        "zero-width Details must not start a watch"
    );

    cx.simulate_resize(gpui::size(px(1600.0), px(900.0)));
    settle_watch(
        &shell,
        cx,
        &runtime,
        &mut requests,
        &replies,
        &mut watches,
        &mut cancellations,
    );
    assert_eq!(watches.len(), 1, "visible Details starts one local watch");

    cx.simulate_resize(gpui::size(px(400.0), px(900.0)));
    settle_watch(
        &shell,
        cx,
        &runtime,
        &mut requests,
        &replies,
        &mut watches,
        &mut cancellations,
    );
    assert!(
        cancellations.contains(&watches[0]),
        "losing Details width cancels its local watch"
    );
    assert_eq!(
        watches.len(),
        1,
        "hidden Details does not restart the watch"
    );

    cx.simulate_resize(gpui::size(px(1600.0), px(900.0)));
    settle_watch(
        &shell,
        cx,
        &runtime,
        &mut requests,
        &replies,
        &mut watches,
        &mut cancellations,
    );
    assert_eq!(watches.len(), 2, "visible Details starts a fresh watch");
    assert_ne!(watches[0], watches[1]);
}
