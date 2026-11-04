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
        // The session card shifts the composer while it fits; these panels
        // measure the composer centered in its column.
        config.session_info_hidden = true;
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
    cx.update(|_, cx| motion::set_reduced_motion(cx, true));
    cx.simulate_resize(gpui::size(px(2000.0), px(900.0)));
    shell.update(cx, |shell, cx| {
        shell.settings.session_info_hidden = false;
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
    shell.update(cx, |shell, cx| shell.toggle_session_info(cx));
    cx.update(|window, cx| window.draw(cx).clear());
    assert!(cx.debug_bounds("session-info-device").is_some());
    cx.simulate_resize(gpui::size(px(2000.0), px(900.0)));
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
fn session_info_reserves_the_rendered_transcript_column(cx: &mut TestAppContext) {
    fn paint(shell: &Entity<Shell>, cx: &mut VisualTestContext) {
        for _ in 0..3 {
            cx.run_until_parked();
            cx.update(|window, cx| {
                shell.update(cx, |shell, cx| {
                    shell.transcript.update(cx, |_, cx| cx.notify());
                    cx.notify();
                });
                window.draw(cx).clear();
            });
        }
    }

    let (_dir, shell, cx) = setup(cx);
    change(&shell, cx, |shell, _, cx| {
        motion::set_reduced_motion(cx, true);
        shell.settings.sidebar_collapsed = true;
        shell.sidebar_tween = None;
        shell.settings.details_sidebar_open = false;
        shell.details_tween = None;
        shell.settings.session_info_hidden = true;
        shell.session_info_tween = None;
        shell.state.update(cx, |state, cx| {
            state.transcript = vec![SessionMessageEntry {
                id: "session-info-geometry".into(),
                role: MessageRole::Assistant,
                parts: vec![MessagePart::Text {
                    id: "session-info-text".into(),
                    text: "A rendered paragraph with a hard break.  \nAnother line.\n\n| Column | Value |\n| --- | --- |\n| Content | Geometry |\n\n".repeat(12),
                }],
                created_at: 0,
                device_id: "local".into(),
                status: Some(zeron_doc::MessageStatus::Complete),
                duration_ms: None,
                continuation_of: None,
            }];
            state.transcript_replayed = true;
            state.transcript_revision += 1;
            cx.notify();
        });
    });

    for (width, open_column_width, gap) in [
        (700.0, 668.0, None),
        (804.0, 736.0, None),
        (1000.0, 736.0, None),
        (1200.0, 736.0, Some(24.0)),
        (1400.0, 736.0, Some(48.0)),
    ] {
        cx.simulate_resize(gpui::size(px(width), px(900.0)));
        paint(&shell, cx);
        let closed = cx.debug_bounds("transcript-column").unwrap();
        let viewport = cx.debug_bounds("transcript-viewport").unwrap();
        near(f32::from(viewport.size.width), width);
        near(f32::from(closed.size.width), (width - 32.0).min(736.0));
        near(f32::from(closed.center().x), f32::from(viewport.center().x));
        assert!(cx.debug_bounds("session-info").is_none());

        change(&shell, cx, |shell, _, cx| shell.toggle_session_info(cx));
        paint(&shell, cx);
        let column = cx.debug_bounds("transcript-column").unwrap();
        let viewport = cx.debug_bounds("transcript-viewport").unwrap();
        let card = cx.debug_bounds("session-info").unwrap();
        near(f32::from(column.size.width), open_column_width);
        near(f32::from(column.center().x), f32::from(viewport.center().x));
        near(f32::from(card.right()), width - 12.0);
        if let Some(gap) = gap {
            near(f32::from(card.left() - column.right()), gap);
        } else {
            near(f32::from(viewport.size.width), width);
            near(f32::from(column.left()), f32::from(closed.left()));
        }
        let composer = shell.read_with(cx, |shell, cx| {
            shell.composer.read(cx).surface_bounds().get().unwrap()
        });
        near(f32::from(column.center().x), f32::from(composer.center().x));

        change(&shell, cx, |shell, _, cx| shell.toggle_session_info(cx));
        paint(&shell, cx);
        let restored = cx.debug_bounds("transcript-column").unwrap();
        near(f32::from(restored.left()), f32::from(closed.left()));
        near(f32::from(restored.size.width), f32::from(closed.size.width));
        assert!(cx.debug_bounds("session-info").is_none());
    }
}

#[gpui::test]
fn side_chat_menu_rename_keeps_the_editor_visible_without_opening_files(cx: &mut TestAppContext) {
    let (_dir, shell, cx) = setup(cx);
    change(&shell, cx, |shell, _, cx| {
        shell.state.update(cx, |state, cx| {
            state.chats.push(
                serde_json::from_value(serde_json::json!({
                    "id": "side", "title": "Side chat", "parentChatId": "a",
                    "deviceId": "local", "archived": false, "createdAt": Utc::now(),
                }))
                .unwrap(),
            );
            cx.notify();
        });
        shell.chat_menu.open(ChatMenuState {
            chat_id: "side".into(),
            tab: None,
            position: gpui::point(px(800.0), px(200.0)),
            page: ChatMenuPage::Root,
        });
    });
    settled(&shell, cx);
    assert!(shell.read_with(cx, |shell, cx| !shell.files_panel_open(cx)));

    let rename = cx.debug_bounds("chat-menu-rename").unwrap();
    cx.simulate_click(rename.center(), Default::default());
    settled(&shell, cx);

    shell.read_with(cx, |shell, cx| {
        assert!(
            !shell.files_panel_open(cx),
            "Rename must not open the explorer"
        );
        assert!(shell.session_info_visible(cx));
        assert_eq!(shell.chat_rename.as_ref().unwrap().chat_id, "side");
    });
    let field = cx
        .debug_bounds("session-lanes-chat-side-title-editor")
        .unwrap();
    let card = cx.debug_bounds("session-lanes").unwrap();
    assert!(field.top() >= card.top() && field.bottom() <= card.bottom());
}

#[gpui::test]
fn side_chat_rename_leaves_takeover_to_show_its_editor(cx: &mut TestAppContext) {
    let (_dir, shell, cx) = setup(cx);
    change(&shell, cx, |shell, window, cx| {
        shell.state.update(cx, |state, cx| {
            state.chats.push(
                serde_json::from_value(serde_json::json!({
                    "id": "side", "title": "Side chat", "parentChatId": "a",
                    "deviceId": "local", "archived": false, "createdAt": Utc::now(),
                }))
                .unwrap(),
            );
            cx.notify();
        });
        // Takeover needs a live mounted surface under the fork's pane contract.
        shell.add_browser_surface(None, window, cx);
    });
    settled(&shell, cx);
    assert!(shell.read_with(cx, |shell, cx| shell.right_pane_shown(cx)));
    change(&shell, cx, |s, _, cx| s.toggle_right_pane_expand(cx));
    assert!(shell.read_with(cx, |shell, _| shell.right_pane_expanded));
    near(settled(&shell, cx).widths[3], 0.0);

    change(&shell, cx, |s, _, cx| s.open_rename_chat("side".into(), cx));
    let frame = settled(&shell, cx);
    assert!(!shell.read_with(cx, |s, _| s.right_pane_expanded));
    assert!(frame.widths[3] > 0.0, "the transcript column must return");
    let field = cx
        .debug_bounds("session-lanes-chat-side-title-editor")
        .unwrap();
    let card = cx.debug_bounds("session-lanes").unwrap();
    assert!(field.top() >= card.top() && field.bottom() <= card.bottom());
}

fn check_side_chat_rename_scroll(cx: &mut TestAppContext, width: f32, reduced_motion: bool) {
    let (_dir, shell, cx) = setup(cx);
    cx.simulate_resize(gpui::size(px(width), px(900.0)));
    cx.update(|_, cx| motion::set_reduced_motion(cx, reduced_motion));
    change(&shell, cx, |shell, _, cx| {
        shell.state.update(cx, |state, cx| {
            let now = Utc::now();
            for index in 0..40 {
                state.chats.push(
                    serde_json::from_value(serde_json::json!({
                        "id": format!("side-{index}"), "title": format!("Side {index}"),
                        "parentChatId": "a", "deviceId": "local", "archived": false,
                        "createdAt": now - chrono::Duration::minutes(index),
                    }))
                    .unwrap(),
                );
            }
            cx.notify();
        });
    });
    settled(&shell, cx);
    assert!(cx.debug_bounds("session-lanes").is_none());

    // Reveal a paged-out row from a hidden card, then a row above the new
    // scroll position. Both directions must leave the whole editor visible.
    for (chat, selector) in [
        ("side-39", "session-lanes-chat-side-39-title-editor"),
        ("side-0", "session-lanes-chat-side-0-title-editor"),
    ] {
        change(&shell, cx, |shell, _, cx| {
            shell.open_rename_chat(chat.into(), cx);
        });
        settled(&shell, cx);
        for _ in 0..4 {
            cx.run_until_parked();
            draw(&shell, cx, 100);
        }
        let field = cx.debug_bounds(selector).unwrap();
        let card = cx.debug_bounds("session-lanes").unwrap();
        assert!(
            field.top() >= card.top() && field.bottom() <= card.bottom(),
            "{chat}'s editor {field:?} is outside the card {card:?}"
        );
        cx.update(|window, cx| {
            shell.read_with(cx, |shell, cx| {
                let rename = shell.chat_rename.as_ref().unwrap();
                assert_eq!(rename.chat_id, chat);
                assert!(rename.input.focus_handle(cx).is_focused(window));
            });
        });
    }
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
                    exit_code: Default::default(),
                    output_truncated: Default::default(),
                    output_lines: Default::default(),

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
            if request.method.as_deref() == Some(methods::WATCH_CHECKOUT_DIFFS) {
                if request.params.get("cwds").is_some() {
                    assert_eq!(
                        request.params["cwds"],
                        serde_json::json!(["/tmp/worked-first", "/tmp/worked-second"])
                    );
                }
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
            shell.update(cx, |shell, cx| {
                *shell.test_time.as_mut().unwrap() +=
                    Duration::from_millis(100).mul_f32(motion::speed_scale());
                cx.notify();
            });
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

    // Session information has its own host-wide subscription, without `cwds`.
    // Its lifetime must end at hidden routes/cards, independently of Details.
    change(&shell, cx, |shell, _, cx| {
        shell.settings.details_sidebar_open = false;
        shell.settings.session_info_hidden = false;
        settings::update(settings::SavePolicy::Immediate, cx, |settings| {
            settings.details_sidebar_open = false;
            settings.session_info_hidden = false;
        });
    });
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
        cancellations.contains(&watches[1]),
        "Details releases its watch"
    );
    assert_eq!(
        watches.len(),
        3,
        "Session information starts one host watch"
    );
    let session_watch = watches[2];
    change(&shell, cx, |shell, _, _| {
        shell.route = Route::Settings(SettingsSection::General)
    });
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
        cancellations.contains(&session_watch),
        "Settings cancels the hidden card's scoped watch"
    );
    assert_eq!(watches.len(), 3, "Settings cannot restart a hidden watch");
    change(&shell, cx, |shell, _, _| shell.route = Route::Chat);
    settle_watch(
        &shell,
        cx,
        &runtime,
        &mut requests,
        &replies,
        &mut watches,
        &mut cancellations,
    );
    assert_eq!(
        watches.len(),
        4,
        "Returning to the card starts a fresh watch"
    );
    let session_watch = watches[3];
    change(&shell, cx, |shell, _, cx| {
        shell.settings.session_info_hidden = true;
        settings::update(settings::SavePolicy::Immediate, cx, |settings| {
            settings.session_info_hidden = true
        });
    });
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
        cancellations.contains(&session_watch),
        "Hiding the card cancels its scoped watch"
    );
    assert_eq!(watches.len(), 4, "The hidden card stays unsubscribed");
}

#[gpui::test]
fn side_chat_rename_scrolls_to_its_row_in_a_docked_card(cx: &mut TestAppContext) {
    check_side_chat_rename_scroll(cx, 1600.0, false);
}

#[gpui::test]
fn side_chat_rename_scrolls_to_its_row_in_a_floating_card_without_motion(cx: &mut TestAppContext) {
    check_side_chat_rename_scroll(cx, 1000.0, true);
}

#[gpui::test]
fn the_session_card_lands_with_a_chat_switch_and_eases_on_its_toggle(cx: &mut TestAppContext) {
    let (_dir, shell, cx) = setup(cx);
    change(&shell, cx, |s, window, cx| {
        s.settings.session_info_hidden = false;
        // b's surface host leaves its transcript no room for the card. In the
        // fork a stored Changes flag is not a live surface: b gets a real
        // Browser tab, then a is selected again.
        select(s, Some("b"), cx);
        s.add_browser_surface(None, window, cx);
        select(s, Some("a"), cx);
    });
    settled(&shell, cx);
    let card = cx
        .debug_bounds("session-info")
        .expect("a's wide column shows the card");
    // Where the card's reveal is easing to, if it eases at all.
    let easing_to = |cx: &mut VisualTestContext| {
        shell.read_with(cx, |s, _| s.session_info_tween.map(|tween| tween.to))
    };

    // The panels snap to b's; the card must not fade out over b's transcript.
    change(&shell, cx, |s, _, cx| select(s, Some("b"), cx));
    draw(&shell, cx, 0);
    assert!(cx.debug_bounds("session-info").is_none());
    assert_eq!(easing_to(cx), None);

    // Back on a, it is at rest from the first frame.
    change(&shell, cx, |s, _, cx| select(s, Some("a"), cx));
    draw(&shell, cx, 0);
    let landed = cx
        .debug_bounds("session-info")
        .expect("a shows its card from the first frame");
    near(f32::from(landed.left()), f32::from(card.left()));
    assert_eq!(easing_to(cx), None);

    // Its own toggle still eases it out, then back in.
    for (reveal, shown) in [(0.0, false), (1.0, true)] {
        change(&shell, cx, |s, _, cx| s.toggle_session_info(cx));
        draw(&shell, cx, 0);
        assert_eq!(easing_to(cx), Some(reveal));
        settled(&shell, cx);
        assert_eq!(cx.debug_bounds("session-info").is_some(), shown);
    }
}

/// V1: the full voice stage never shows Details (body, header or resize
/// target) through or above itself, including its exit transition, and
/// leaves the saved Details preferences untouched for the return trip.
#[gpui::test]
fn voice_stage_excludes_details_without_saving_preferences(cx: &mut TestAppContext) {
    let (_dir, shell, cx) = setup(cx);
    cx.update(|_, cx| motion::set_reduced_motion(cx, true));
    shell.update(cx, |shell, cx| {
        shell.reduced_motion = true;
        shell.settings.details_sidebar_open = true;
        shell.settings.details_sidebar_width = 380.0;
        shell.details_tween = None;
        cx.notify();
    });
    let paint = |cx: &mut VisualTestContext| {
        for _ in 0..2 {
            cx.update(|window, cx| window.draw(cx).clear());
        }
    };
    let details_presented = |cx: &mut VisualTestContext| {
        (
            cx.debug_bounds("details-sidebar-column").is_some(),
            cx.debug_bounds("details-header-overlay").is_some(),
        )
    };
    paint(cx);
    assert_eq!(
        details_presented(cx),
        (true, true),
        "baseline shows Details"
    );

    shell.update(cx, |shell, cx| {
        shell.voice.update(cx, |voice, _| {
            voice.phase = zeron_proto::voice::VoicePhase::Active;
        });
        shell.set_voice_stage_open(true, cx);
    });
    paint(cx);
    assert!(cx.debug_bounds("voice-stage").is_some());
    assert_eq!(
        details_presented(cx),
        (false, false),
        "no Details body or header under the stage"
    );

    // The exit transition still presents the stage: Details stays out.
    cx.update(|_, cx| motion::set_reduced_motion(cx, false));
    shell.update(cx, |shell, cx| {
        shell.reduced_motion = false;
        shell.set_voice_stage_open(false, cx);
    });
    cx.update(|window, cx| window.draw(cx).clear());
    shell.update(cx, |shell, cx| {
        // Pin the exit clock to its first frame.
        shell.voice_stage_changed_at = Some(std::time::Instant::now() + Duration::from_secs(3600));
        cx.notify();
    });
    cx.update(|window, cx| window.draw(cx).clear());
    assert!(
        cx.debug_bounds("voice-stage").is_some(),
        "stage is fading out"
    );
    assert_eq!(details_presented(cx), (false, false));

    // Leaving the stage restores Details with its saved width.
    cx.update(|_, cx| motion::set_reduced_motion(cx, true));
    shell.update(cx, |shell, cx| {
        shell.reduced_motion = true;
        shell.voice_stage_changed_at = None;
        cx.notify();
    });
    paint(cx);
    assert!(cx.debug_bounds("voice-stage").is_none());
    assert_eq!(details_presented(cx), (true, true));
    shell.read_with(cx, |shell, cx| {
        assert!(shell.settings.details_sidebar_open);
        assert_eq!(shell.settings.details_sidebar_width, 380.0);
        assert!(shell.voice.read(cx).is_live(), "the call is untouched");
    });
}

fn make_call_live(shell: &Entity<Shell>, cx: &mut VisualTestContext, with_audio: bool) {
    shell.update(cx, |shell, cx| {
        shell.reduced_motion = true;
        shell.voice.update(cx, |voice, _| {
            // No engine in these fixtures: a call bound to an engine-owned
            // chat would be ended by `on_state_changed`, so it stays unbound.
            voice.phase = zeron_proto::voice::VoicePhase::Active;
            if with_audio {
                voice.snapshot = Some(zeron_proto::voice::VoiceSnapshot {
                    session_id: "s".into(),
                    chat_id: "voice-orchestrator".into(),
                    generation: 1,
                    phase: zeron_proto::voice::VoicePhase::Active,
                    muted: false,
                    playing: false,
                    work: zeron_proto::voice::VoiceWork::Idle,
                    reason: None,
                    voice: None,
                    voices: Vec::new(),
                });
            }
        });
        cx.notify();
    });
}

fn paint_twice(cx: &mut VisualTestContext) {
    for _ in 0..2 {
        cx.update(|window, cx| window.draw(cx).clear());
    }
}

/// V3: one widget follows the live call through Chats, Workers, Settings
/// and a collapsed sidebar, never changing the selection or the call.
#[gpui::test]
fn voice_widget_follows_live_call_across_routes(cx: &mut TestAppContext) {
    let (_dir, shell, cx) = setup(cx);
    cx.update(|_, cx| motion::set_reduced_motion(cx, true));
    make_call_live(&shell, cx, false);
    paint_twice(cx);
    let widget = cx.debug_bounds("voice-widget").expect("widget on Chat a");
    // Lower-right corner, 16px margins, its own small hitbox.
    let viewport = cx.update(|window, _| window.viewport_size());
    assert!((f32::from(viewport.width - widget.right()) - 16.0).abs() < 1.0);
    assert!((f32::from(viewport.height - widget.bottom()) - 16.0).abs() < 1.0);
    assert!(f32::from(widget.size.width) < 240.0 && f32::from(widget.size.height) < 80.0);
    assert!(cx.debug_bounds("voice-composer-orb").is_none());

    shell.update(cx, |shell, cx| shell.open_chat("b".into(), cx));
    paint_twice(cx);
    assert!(cx.debug_bounds("voice-widget").is_some(), "another Chat");
    shell.update(cx, |shell, cx| {
        shell.open_settings(SettingsSection::General, cx)
    });
    paint_twice(cx);
    assert!(cx.debug_bounds("voice-widget").is_some(), "Settings");
    shell.update(cx, |shell, cx| {
        shell.route = Route::Chat;
        shell.select_sidebar_mode(SidebarMode::Workers, cx);
    });
    paint_twice(cx);
    assert!(cx.debug_bounds("voice-widget").is_some(), "Workers");
    shell.update(cx, |shell, cx| {
        shell.select_sidebar_mode(SidebarMode::Orchestrator, cx);
        shell.toggle_sidebar(cx);
    });
    paint_twice(cx);
    assert!(
        cx.debug_bounds("voice-widget").is_some(),
        "collapsed sidebar"
    );

    shell.read_with(cx, |shell, cx| {
        let voice = shell.voice.read(cx);
        assert!(voice.is_live());
        assert!(!voice.stage_open);
        assert_eq!(shell.state.read(cx).selected_chat.as_deref(), Some("b"));
    });

    // The stage hides the widget; a narrow window keeps it on screen.
    shell.update(cx, |shell, cx| shell.set_voice_stage_open(true, cx));
    paint_twice(cx);
    assert!(cx.debug_bounds("voice-widget").is_none());
    shell.update(cx, |shell, cx| shell.set_voice_stage_open(false, cx));
    cx.simulate_resize(gpui::size(px(420.0), px(560.0)));
    paint_twice(cx);
    let narrow = cx.debug_bounds("voice-widget").unwrap();
    assert!(narrow.left() >= px(0.0) && narrow.right() <= px(420.0));
    assert!(narrow.bottom() <= px(560.0));
}

/// MR9: beside a native Browser the widget sits left of it when the window
/// holds it; in the Browser takeover (the page fills the content) it keeps
/// the corner and the page leaves its band uncovered — painting and hit
/// testing are cropped, so the capsule is never under WebKit.
#[gpui::test]
fn voice_widget_never_sits_under_the_native_browser(cx: &mut TestAppContext) {
    let (_dir, shell, cx) = setup(cx);
    cx.update(|_, cx| motion::set_reduced_motion(cx, true));
    cx.simulate_resize(gpui::size(px(1600.0), px(900.0)));
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| shell.add_browser_surface(None, window, cx))
    });
    make_call_live(&shell, cx, false);
    paint_twice(cx);
    let band = |cx: &mut VisualTestContext| {
        shell.read_with(cx, |shell, cx| {
            let id = *shell.browsers.keys().next().expect("a Browser");
            (
                f32::from(shell.browsers[&id].read(cx).bottom_occlusion()),
                shell.file_preview.read(cx).bottom_reserve(),
            )
        })
    };
    let viewport = cx.update(|window, _| window.viewport_size());
    let widget = cx
        .debug_bounds("voice-widget")
        .expect("widget beside the Browser");
    let pane_left = shell.read_with(cx, |shell, cx| {
        f32::from(viewport.width)
            - shell.right_visible_width(cx)
            - shell.files_visible_width(cx)
            - shell.details_target(cx)
    });
    assert!(
        f32::from(widget.right()) <= pane_left + 0.5,
        "left of the page"
    );
    assert_eq!(band(cx), (0.0, 0.0), "nothing cropped while beside it");

    // Takeover beside the open sidebar: the widget still fits left of the
    // page, over the sidebar, and nothing is cropped.
    shell.update(cx, |shell, cx| shell.toggle_right_pane_expand(cx));
    paint_twice(cx);
    let widget = cx.debug_bounds("voice-widget").expect("widget in takeover");
    let page_left = shell.read_with(cx, |shell, cx| {
        f32::from(viewport.width) - shell.right_visible_width(cx) - shell.files_visible_width(cx)
    });
    assert!(
        f32::from(widget.right()) <= page_left + 0.5,
        "beside the takeover page"
    );
    assert_eq!(band(cx), (0.0, 0.0));

    // Takeover with the sidebar collapsed: the page spans the window, the
    // widget keeps the corner and the page's bottom band is cropped for it.
    shell.update(cx, |shell, cx| shell.toggle_sidebar(cx));
    paint_twice(cx);
    let widget = cx.debug_bounds("voice-widget").expect("widget in takeover");
    assert!((f32::from(viewport.width - widget.right()) - 16.0).abs() < 1.0);
    let (browser_band, preview_band) = band(cx);
    assert_eq!(browser_band, crate::voice_surfaces::VOICE_WIDGET_BAND);
    assert_eq!(preview_band, crate::voice_surfaces::VOICE_WIDGET_BAND);
    assert!(
        f32::from(widget.top()) >= f32::from(viewport.height) - browser_band,
        "the capsule lies inside the band the page leaves uncovered"
    );

    // No live call: nothing is cropped.
    shell.update(cx, |shell, cx| {
        shell.voice.update(cx, |voice, _| {
            voice.phase = zeron_proto::voice::VoicePhase::Closed
        });
        cx.notify();
    });
    paint_twice(cx);
    assert!(cx.debug_bounds("voice-widget").is_none());
    assert_eq!(band(cx), (0.0, 0.0));
}

/// GR4: the placement budget is the capsule as rendered, so the fit test
/// and the real hitbox can never disagree.
#[gpui::test]
fn voice_widget_capsule_matches_its_placement_budget(cx: &mut TestAppContext) {
    let (_dir, shell, cx) = setup(cx);
    cx.update(|_, cx| motion::set_reduced_motion(cx, true));
    make_call_live(&shell, cx, false);
    paint_twice(cx);
    let capsule = cx.debug_bounds("voice-widget").expect("widget");
    assert!(
        (f32::from(capsule.size.width) - crate::voice_surfaces::WIDGET_WIDTH).abs() < 0.5,
        "rendered {} vs budget {}",
        f32::from(capsule.size.width),
        crate::voice_surfaces::WIDGET_WIDTH
    );
    assert!(
        (f32::from(capsule.size.height) - crate::voice_surfaces::WIDGET_HEIGHT).abs() < 0.5,
        "rendered {} vs budget {}",
        f32::from(capsule.size.height),
        crate::voice_surfaces::WIDGET_HEIGHT
    );
}

/// V3: mute and return drive the same call; mute is refused until the call
/// has audio state, and never selects a Chat or hangs up.
#[gpui::test]
fn voice_widget_mute_and_return_preserve_call(cx: &mut TestAppContext) {
    let (_dir, shell, cx) = setup(cx);
    cx.update(|_, cx| motion::set_reduced_motion(cx, true));
    make_call_live(&shell, cx, false);
    paint_twice(cx);
    let mic = cx.debug_bounds("voice-widget-mic").unwrap();
    cx.simulate_click(mic.center(), gpui::Modifiers::default());
    shell.read_with(cx, |shell, cx| {
        let voice = shell.voice.read(cx);
        assert!(!voice.can_mute() && !voice.muted(), "no fictitious mute");
        assert!(voice.is_live() && !voice.stage_open);
    });

    make_call_live(&shell, cx, true);
    let mut controls = shell.update(cx, |shell, cx| {
        shell
            .voice
            .update(cx, |voice, _| voice.attach_test_controls())
    });
    paint_twice(cx);
    let mic = cx.debug_bounds("voice-widget-mic").unwrap();
    cx.simulate_click(mic.center(), gpui::Modifiers::default());
    assert!(matches!(
        controls.try_recv(),
        Ok(crate::voice::VoiceControl::Mute(true))
    ));
    shell.read_with(cx, |shell, cx| {
        let voice = shell.voice.read(cx);
        assert!(voice.muted() && voice.is_live() && !voice.stage_open);
        assert_eq!(shell.state.read(cx).selected_chat.as_deref(), Some("a"));
    });

    paint_twice(cx);
    let back = cx.debug_bounds("voice-widget-return").unwrap();
    cx.simulate_click(back.center(), gpui::Modifiers::default());
    paint_twice(cx);
    shell.read_with(cx, |shell, cx| {
        let voice = shell.voice.read(cx);
        assert!(voice.stage_open && voice.muted(), "same call, still muted");
        assert_eq!(shell.state.read(cx).selected_chat.as_deref(), Some("a"));
    });
    assert!(cx.debug_bounds("voice-widget").is_none());
    assert!(cx.debug_bounds("voice-stage").is_some());
}

/// V3: hover and keyboard focus reveal hang-up in its fixed slot; ending
/// removes the widget for good without reopening the stage.
#[gpui::test]
fn voice_widget_hover_focus_and_end(cx: &mut TestAppContext) {
    let (_dir, shell, cx) = setup(cx);
    cx.update(|_, cx| motion::set_reduced_motion(cx, true));
    make_call_live(&shell, cx, false);
    paint_twice(cx);
    let targets = |cx: &mut VisualTestContext| {
        [
            cx.debug_bounds("voice-widget-mic").unwrap(),
            cx.debug_bounds("voice-widget-return").unwrap(),
            cx.debug_bounds("voice-widget-end").unwrap(),
        ]
    };
    let resting = targets(cx);
    shell.read_with(cx, |shell, _| assert!(!shell.voice_widget_hovered));

    // A stationary pointer over the capsule: revealed, nothing moves.
    cx.simulate_mouse_move(resting[0].center(), None, gpui::Modifiers::default());
    paint_twice(cx);
    shell.read_with(cx, |shell, _| assert!(shell.voice_widget_hovered));
    assert_eq!(targets(cx), resting);
    paint_twice(cx);
    shell.read_with(cx, |shell, _| {
        assert!(shell.voice_widget_hovered, "no flicker")
    });

    // Leaving hides it again; keyboard focus inside reveals it too.
    cx.simulate_mouse_move(
        gpui::point(px(5.0), px(5.0)),
        None,
        gpui::Modifiers::default(),
    );
    paint_twice(cx);
    shell.read_with(cx, |shell, _| assert!(!shell.voice_widget_hovered));
    cx.update(|window, cx| {
        let focus = shell.read(cx).voice_widget_focus.clone();
        window.focus(&focus, cx);
    });
    paint_twice(cx);
    cx.update(|window, cx| {
        assert!(
            shell
                .read(cx)
                .voice_widget_focus
                .contains_focused(window, cx)
        )
    });
    assert_eq!(targets(cx), resting);

    // Hang up: the call ends, the stage stays closed, the Chat stays put.
    cx.simulate_mouse_move(resting[2].center(), None, gpui::Modifiers::default());
    cx.simulate_click(resting[2].center(), gpui::Modifiers::default());
    paint_twice(cx);
    shell.read_with(cx, |shell, cx| {
        let voice = shell.voice.read(cx);
        assert!(!voice.is_live() && !voice.stage_open);
        assert_eq!(shell.state.read(cx).selected_chat.as_deref(), Some("a"));
    });
    assert!(cx.debug_bounds("voice-widget").is_none());
    shell.update(cx, |shell, cx| shell.open_chat("b".into(), cx));
    paint_twice(cx);
    assert!(cx.debug_bounds("voice-widget").is_none(), "does not return");

    // Connecting calls can be cancelled; failed calls show no widget.
    shell.update(cx, |shell, cx| {
        shell.voice.update(cx, |voice, _| {
            voice.phase = zeron_proto::voice::VoicePhase::Checking
        });
        cx.notify();
    });
    paint_twice(cx);
    let end = cx.debug_bounds("voice-widget-end").unwrap();
    cx.simulate_mouse_move(end.center(), None, gpui::Modifiers::default());
    cx.simulate_click(end.center(), gpui::Modifiers::default());
    paint_twice(cx);
    shell.read_with(cx, |shell, cx| assert!(!shell.voice.read(cx).is_live()));
    shell.update(cx, |shell, cx| {
        shell.voice.update(cx, |voice, _| {
            voice.phase = zeron_proto::voice::VoicePhase::Failed
        });
        cx.notify();
    });
    paint_twice(cx);
    assert!(cx.debug_bounds("voice-widget").is_none());
}

/// V2: turning the orb off removes every orb presentation and its timer
/// immediately, keeps the call and its controls, and leaves no orb-sized
/// gap on the stage; turning it back on presents the current call.
#[gpui::test]
fn voice_orb_disabled_removes_visuals_without_ending_call(cx: &mut TestAppContext) {
    let (_dir, shell, cx) = setup(cx);
    cx.update(|_, cx| motion::set_reduced_motion(cx, false));
    make_call_live(&shell, cx, true);
    shell.update(cx, |shell, _| shell.reduced_motion = false);
    paint_twice(cx);
    let orbs = |cx: &mut VisualTestContext| {
        shell.read_with(cx, |shell, cx| {
            [
                &shell.voice_footer_orb,
                &shell.voice_stage_orb,
                &shell.voice_widget_orb,
            ]
            .map(|orb| {
                let orb = orb.read(cx);
                (orb.is_visible(), orb.has_pending_tick())
            })
        })
    };
    let [footer, _, widget] = orbs(cx);
    assert!(footer.0 && widget.0, "orbs on by default");

    cx.update(|_, cx| {
        settings::update(settings::SavePolicy::Immediate, cx, |s| {
            s.voice_orb_enabled = false
        });
    });
    shell.update(cx, |_, cx| cx.notify());
    paint_twice(cx);
    assert_eq!(orbs(cx), [(false, false); 3], "no orb drawn or ticking");
    assert!(cx.debug_bounds("voice-widget-return").is_some());
    assert!(cx.debug_bounds("voice-widget-mic").is_some());
    shell.read_with(cx, |shell, cx| assert!(shell.voice.read(cx).is_live()));

    shell.update(cx, |shell, cx| {
        shell.reduced_motion = true;
        shell.set_voice_stage_open(true, cx);
    });
    cx.update(|_, cx| motion::set_reduced_motion(cx, true));
    paint_twice(cx);
    assert!(cx.debug_bounds("voice-stage").is_some());
    assert!(
        cx.debug_bounds("voice-stage-orb").is_none(),
        "no placeholder"
    );
    assert!(cx.debug_bounds("voice-stage-caption").is_some());
    assert_eq!(orbs(cx), [(false, false); 3]);
    // Global motion preference is untouched by the voice preference.
    cx.update(|_, cx| {
        assert_eq!(
            settings::current(cx).reduce_motion,
            crate::motion::ReduceMotion::default()
        )
    });

    cx.update(|_, cx| {
        settings::update(settings::SavePolicy::Immediate, cx, |s| {
            s.voice_orb_enabled = true
        });
    });
    shell.update(cx, |_, cx| cx.notify());
    paint_twice(cx);
    assert!(cx.debug_bounds("voice-stage-orb").is_some());
    shell.read_with(cx, |shell, cx| {
        assert!(shell.voice_stage_orb.read(cx).is_visible());
        assert!(shell.voice.read(cx).is_live(), "same call");
    });

    // Collapsing the sidebar hides the footer orb's timer as well.
    shell.update(cx, |shell, cx| {
        shell.set_voice_stage_open(false, cx);
        shell.toggle_sidebar(cx);
    });
    paint_twice(cx);
    shell.read_with(cx, |shell, cx| {
        let footer = shell.voice_footer_orb.read(cx);
        assert!(!footer.is_visible() && !footer.has_pending_tick());
    });
}

/// B1: every way of activating the Browser closes Details through its own
/// close sequence; file previews keep Details; a manual reopen while the
/// Browser stays selected is respected.
#[gpui::test]
fn browser_activation_closes_details_but_file_previews_do_not(cx: &mut TestAppContext) {
    let (dir, shell, cx) = setup(cx);
    cx.update(|_, cx| motion::set_reduced_motion(cx, true));
    let open_details = |shell: &Entity<Shell>, cx: &mut VisualTestContext| {
        shell.update(cx, |shell, cx| {
            shell.settings.details_sidebar_open = true;
            shell.settings.details_sidebar_width = 360.0;
            shell.details_tween = None;
            cx.notify();
        });
        paint_twice(cx);
    };
    let details_open = |shell: &Entity<Shell>, cx: &mut VisualTestContext| {
        shell.read_with(cx, |shell, _| shell.settings.details_sidebar_open)
    };

    // A file preview (PDF) leaves Details open.
    std::fs::write(dir.path().join("report.pdf"), b"%PDF-1.4\n").unwrap();
    open_details(&shell, cx);
    shell.update(cx, |shell, cx| {
        let key = shell.panel_key(cx);
        shell.open_preview_surface(key, dir.path().to_path_buf(), "report.pdf".into(), cx);
    });
    paint_twice(cx);
    assert!(details_open(&shell, cx), "file preview keeps Details");

    // Creating a Browser closes it, keeping its saved width.
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| shell.add_browser_surface(None, window, cx))
    });
    paint_twice(cx);
    assert!(!details_open(&shell, cx), "new Browser closes Details");
    assert!(cx.debug_bounds("details-sidebar-column").is_none());
    shell.read_with(cx, |shell, _| {
        assert_eq!(shell.settings.details_sidebar_width, 360.0)
    });
    let browser = shell.read_with(cx, |shell, _| shell.browser_seq);

    // Reopened by hand while the Browser stays selected: it stays open.
    shell.update(cx, |shell, cx| shell.toggle_details_sidebar(cx));
    shell.update(cx, |shell, cx| {
        shell.set_right_active(RightSurface::Browser(browser), cx)
    });
    paint_twice(cx);
    assert!(details_open(&shell, cx), "manual reopen is respected");

    // Selecting the preview tab and then the Browser tab again closes it.
    let preview = shell.read_with(cx, |shell, cx| {
        shell
            .right_surface_rows(cx)
            .into_iter()
            .find_map(|(surface, ..)| {
                matches!(surface, RightSurface::Preview(_)).then_some(surface)
            })
            .unwrap()
    });
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            shell.activate_right_surface(preview, window, cx)
        })
    });
    assert!(details_open(&shell, cx));
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            shell.activate_right_surface(RightSurface::Browser(browser), window, cx)
        })
    });
    paint_twice(cx);
    assert!(!details_open(&shell, cx), "tab selection closes Details");

    // Already closed: activating another Browser starts no Details tween.
    shell.update(cx, |shell, _| shell.details_tween = None);
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            shell.add_browser_surface(Some("http://127.0.0.1:9/".into()), window, cx)
        })
    });
    shell.read_with(cx, |shell, _| {
        assert!(!shell.settings.details_sidebar_open);
        assert!(shell.details_tween.is_none());
    });
}

fn emit_open_server(shell: &Entity<Shell>, cx: &mut VisualTestContext, chat: &str, port: u16) {
    shell.update(cx, |shell, cx| {
        shell.details_sidebar.update(cx, |_, cx| {
            cx.emit(DetailsSidebarEvent::OpenServer {
                chat_id: chat.into(),
                server: serde_json::from_value(serde_json::json!({
                    "pid":7,"startedAt":1,"port":port,"cwd":"/srv/p","root":"/srv/p","command":"vite","previewEligible":true
                })).unwrap(),
            })
        })
    });
    paint_twice(cx);
}

fn active_browser_url(
    shell: &Entity<Shell>,
    cx: &mut VisualTestContext,
) -> Option<(u64, Option<String>)> {
    shell.read_with(cx, |shell, cx| {
        let RightSurface::Browser(id) = shell.resolved_right_active(cx) else {
            return None;
        };
        Some((id, shell.browsers[&id].read(cx).page.url.clone()))
    })
}

/// B2: a Details Servers row opens the Chat's server in the internal
/// browser — directly for a local host, through the preview route for a
/// remote host — honours an explicit external preference, and opens nothing
/// for a Chat that is no longer selected.
#[gpui::test]
fn details_server_rows_open_in_the_internal_browser(cx: &mut TestAppContext) {
    let (_dir, shell, cx) = setup(cx);
    cx.update(|_, cx| motion::set_reduced_motion(cx, true));
    shell.update(cx, |shell, cx| {
        shell
            .state
            .update(cx, |state, _| state.local_device_id = Some("local".into()));
    });
    paint_twice(cx);

    // Stale Chat context: nothing opens, nothing leaves Zeron.
    emit_open_server(&shell, cx, "b", 3210);
    assert_eq!(active_browser_url(&shell, cx), None);
    assert_eq!(cx.opened_url(), None);

    // Local host: the internal Browser loads the loopback server.
    emit_open_server(&shell, cx, "a", 3210);
    let (_, url) = active_browser_url(&shell, cx).expect("internal Browser");
    assert!(
        url.as_deref()
            .is_some_and(|url| url.starts_with("http://127.0.0.1:3210")),
        "{url:?}"
    );
    assert_eq!(cx.opened_url(), None, "no external launch");

    // An explicit external preference is respected.
    cx.update(|_, cx| {
        settings::update(settings::SavePolicy::Immediate, cx, |s| {
            s.open_web_links_in_zeron = false
        });
    });
    let browsers = shell.read_with(cx, |shell, _| shell.browsers.len());
    emit_open_server(&shell, cx, "a", 4000);
    assert!(
        cx.opened_url()
            .is_some_and(|url| url.starts_with("http://127.0.0.1:4000"))
    );
    shell.read_with(cx, |shell, _| assert_eq!(shell.browsers.len(), browsers));

    // Remote host: never its loopback on this Mac — the preview route, once
    // discovery lists the port.
    shell.update(cx, |shell, cx| {
        shell.state.update(cx, |state, _| {
            state.local_device_id = Some("this-mac".into());
        });
    });
    emit_open_server(&shell, cx, "a", 5173);
    let (id, url) = active_browser_url(&shell, cx).expect("internal Browser");
    assert_eq!(url, None, "waits for the preview route");
    let browser = shell.read_with(cx, |shell, _| shell.browsers[&id].clone());
    browser.read_with(cx, |browser, _| {
        assert_eq!(browser.pending_server_port(), Some(5173))
    });
    browser.update(cx, |browser, cx| {
        browser.set_test_previews(zeron_proto::PreviewSnapshot {
            services: vec![zeron_proto::PreviewService {
                process: Default::default(),

                id: "svc".into(),
                project_id: "p".into(),
                project_name: "p".into(),
                project_cwd: "/srv/p".into(),
                device_id: "local".into(),
                device_name: "Build box".into(),
                hostname: "unrelated.preview.localhost".into(),
                name: "vite".into(),
                port: 5173,
                pid: 6,
                cwd: "/srv/p".into(),
                started_at: 1,
                zeron_owned: true,
                chat_id: Some("a".into()),
            }],
            remote: true,
            ..Default::default()
        });
        cx.notify();
    });
    paint_twice(cx);
    assert_eq!(
        active_browser_url(&shell, cx).unwrap().1,
        None,
        "the Details event keeps the selected process identity"
    );
    browser.update(cx, |browser, cx| {
        browser.set_test_previews(zeron_proto::PreviewSnapshot {
            services: vec![zeron_proto::PreviewService {
                process: Default::default(),

                id: "svc".into(),
                project_id: "p".into(),
                project_name: "p".into(),
                project_cwd: "/srv/p".into(),
                device_id: "local".into(),
                device_name: "Build box".into(),
                hostname: "p5173.preview.localhost".into(),
                name: "vite".into(),
                port: 5173,
                pid: 7,
                cwd: "/srv/p".into(),
                started_at: 1,
                zeron_owned: true,
                chat_id: Some("a".into()),
            }],
            remote: true,
            ..Default::default()
        });
        cx.notify();
    });
    paint_twice(cx);
    let (_, url) = active_browser_url(&shell, cx).unwrap();
    assert!(
        url.as_deref()
            .is_some_and(|url| url.starts_with("http://p5173.preview.localhost:7331")),
        "{url:?}"
    );
}

/// MR8: the FIRST Browser of a Chat closes Details (no earlier surface), and
/// reopening a hidden pane onto its Browser closes Details again; a manual
/// reopen while that same Browser stays visible is respected.
#[gpui::test]
fn first_browser_and_reopened_browser_pane_close_details(cx: &mut TestAppContext) {
    let (_dir, shell, cx) = setup(cx);
    cx.update(|_, cx| motion::set_reduced_motion(cx, true));
    let details_open = |cx: &mut VisualTestContext| {
        shell.read_with(cx, |shell, _| shell.settings.details_sidebar_open)
    };
    shell.update(cx, |shell, cx| {
        shell.settings.details_sidebar_open = true;
        shell.details_tween = None;
        cx.notify();
    });
    paint_twice(cx);
    assert!(shell.read_with(cx, |shell, cx| shell.right_surface_rows(cx).is_empty()));

    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| shell.add_browser_surface(None, window, cx))
    });
    paint_twice(cx);
    assert!(!details_open(cx), "the first Browser closes Details");

    // Same Browser still visible: a manual reopen survives repaints.
    shell.update(cx, |shell, cx| shell.toggle_details_sidebar(cx));
    paint_twice(cx);
    assert!(details_open(cx));

    // Hide the pane, reopen Details, then reopen the pane onto the Browser.
    cx.update(|window, cx| shell.update(cx, |shell, cx| shell.toggle_right_pane(window, cx)));
    paint_twice(cx);
    shell.read_with(cx, |shell, cx| assert!(!shell.right_pane_open(cx)));
    assert!(details_open(cx), "hiding the pane does not touch Details");
    cx.update(|window, cx| shell.update(cx, |shell, cx| shell.toggle_right_pane(window, cx)));
    paint_twice(cx);
    shell.read_with(cx, |shell, cx| assert!(shell.right_pane_open(cx)));
    assert!(
        !details_open(cx),
        "reopening the Browser pane closes Details"
    );
}

/// B2 (current upstream comparison): a GitHub/YouTube link the user pasted
/// in their own message is a Chat link like any other. Its chip routes
/// through the Chat link router: the session Browser by default, the
/// external browser only when the user explicitly chose it.
#[gpui::test]
fn user_message_url_chips_follow_the_chat_link_preference(cx: &mut TestAppContext) {
    let (_dir, shell, cx) = setup(cx);
    cx.update(|_, cx| motion::set_reduced_motion(cx, true));
    let url = "https://www.youtube.com/watch?v=dQw4w9WgXcQ";
    shell.update(cx, |shell, cx| {
        shell.state.update(cx, |state, cx| {
            let entries = serde_json::from_value(serde_json::json!([{
                "id": "user-link",
                "role": "user",
                "parts": [{ "id": "text", "kind": "text", "text": url }],
                "createdAt": 1_788_900_000_000_i64,
                "deviceId": "local"
            }]))
            .unwrap();
            state
                .receive_transcript_frame(zeron_doc::TranscriptFrame::Reset { reset: entries }, cx)
                .unwrap();
        });
    });
    paint_twice(cx);
    let click_chip = |cx: &mut VisualTestContext| {
        // The message is only the URL, so its chip opens the first line of
        // the bubble text.
        let text = cx
            .debug_bounds("user-bubble-text")
            .expect("the user bubble's text");
        cx.simulate_click(
            gpui::point(text.left() + px(12.0), text.top() + px(10.0)),
            gpui::Modifiers::default(),
        );
        paint_twice(cx);
    };
    assert!(
        cx.update(|_, cx| settings::current(cx).open_web_links_in_zeron),
        "the default is internal"
    );
    click_chip(cx);
    assert_eq!(cx.opened_url(), None, "no external launch by default");
    assert_eq!(
        active_browser_url(&shell, cx)
            .and_then(|(_, address)| address)
            .as_deref(),
        Some(url),
        "the session Browser opened the chip's address"
    );

    // An explicit external preference is honored.
    cx.update(|_, cx| {
        settings::update(settings::SavePolicy::Immediate, cx, |s| {
            s.open_web_links_in_zeron = false
        })
    });
    paint_twice(cx);
    click_chip(cx);
    assert_eq!(cx.opened_url().as_deref(), Some(url));
}

/// MR10: an explicit navigation supersedes a pending server intent, so a
/// late preview snapshot cannot replace the page the user chose; without a
/// newer navigation the pending server still resolves.
#[gpui::test]
fn manual_navigation_cancels_a_pending_remote_server(cx: &mut TestAppContext) {
    let (_dir, shell, cx) = setup(cx);
    cx.update(|_, cx| motion::set_reduced_motion(cx, true));
    shell.update(cx, |shell, cx| {
        shell.state.update(cx, |state, _| {
            state.local_device_id = Some("this-mac".into())
        });
    });
    paint_twice(cx);
    let snapshot = |port: u16| zeron_proto::PreviewSnapshot {
        services: vec![zeron_proto::PreviewService {
            process: Default::default(),

            id: format!("svc-{port}"),
            project_id: "p".into(),
            project_name: "p".into(),
            project_cwd: "/srv/p".into(),
            device_id: "local".into(),
            device_name: "Build box".into(),
            hostname: format!("p{port}.preview.localhost"),
            name: "vite".into(),
            port,
            pid: 7,
            cwd: "/srv/p".into(),
            started_at: 1,
            zeron_owned: true,
            chat_id: Some("a".into()),
        }],
        remote: true,
        ..Default::default()
    };

    emit_open_server(&shell, cx, "a", 5173);
    let (id, _) = active_browser_url(&shell, cx).expect("internal Browser");
    let browser = shell.read_with(cx, |shell, _| shell.browsers[&id].clone());
    cx.update(|window, cx| {
        browser.update(cx, |browser, cx| {
            browser.navigate("https://example.test/chosen", window, cx)
        })
    });
    browser.read_with(cx, |browser, _| {
        assert_eq!(browser.pending_server_port(), None)
    });
    browser.update(cx, |browser, cx| {
        browser.set_test_previews(snapshot(5173));
        cx.notify();
    });
    paint_twice(cx);
    let (_, url) = active_browser_url(&shell, cx).unwrap();
    assert!(
        url.as_deref()
            .is_some_and(|url| url.starts_with("https://example.test/chosen")),
        "late discovery must not replace the chosen page: {url:?}"
    );

    // No newer navigation: the pending server still opens when listed.
    emit_open_server(&shell, cx, "a", 6006);
    let (id, url) = active_browser_url(&shell, cx).unwrap();
    assert_eq!(url, None);
    let browser = shell.read_with(cx, |shell, _| shell.browsers[&id].clone());
    browser.update(cx, |browser, cx| {
        browser.set_test_previews(snapshot(6006));
        cx.notify();
    });
    paint_twice(cx);
    let (_, url) = active_browser_url(&shell, cx).unwrap();
    assert!(
        url.as_deref()
            .is_some_and(|url| url.starts_with("http://p6006.preview.localhost")),
        "{url:?}"
    );
}

/// GF7: a server that resolves late (remote discovery, render-time) opens
/// once without taking keyboard focus. Typing in the composer or in the
/// address field continues in the same focused input with its text intact.
#[gpui::test]
fn late_server_discovery_keeps_the_exact_editing_focus(cx: &mut TestAppContext) {
    let (_dir, shell, cx) = setup(cx);
    cx.update(|_, cx| motion::set_reduced_motion(cx, true));
    shell.update(cx, |shell, cx| {
        shell.state.update(cx, |state, _| {
            state.local_device_id = Some("this-mac".into())
        });
    });
    paint_twice(cx);
    let snapshot = |port: u16| zeron_proto::PreviewSnapshot {
        services: vec![zeron_proto::PreviewService {
            process: Default::default(),

            id: format!("svc-{port}"),
            project_id: "p".into(),
            project_name: "p".into(),
            project_cwd: "/srv/p".into(),
            device_id: "local".into(),
            device_name: "Build box".into(),
            hostname: format!("p{port}.preview.localhost"),
            name: "vite".into(),
            port,
            pid: 7,
            cwd: "/srv/p".into(),
            started_at: 1,
            zeron_owned: true,
            chat_id: Some("a".into()),
        }],
        remote: true,
        ..Default::default()
    };
    let composer = shell.read_with(cx, |shell, _| shell.composer.clone());
    let composer_text = |cx: &mut VisualTestContext| {
        composer.read_with(cx, |composer, cx| composer.input.read(cx).text().to_owned())
    };

    for (port, in_address) in [(5173, false), (6006, true)] {
        emit_open_server(&shell, cx, "a", port);
        let (id, url) = active_browser_url(&shell, cx).expect("internal Browser");
        assert_eq!(url, None, "waits for discovery");
        let browser = shell.read_with(cx, |shell, _| shell.browsers[&id].clone());
        let changes = std::rc::Rc::new(std::cell::Cell::new(0));
        let _counting = cx.update(|_, cx| {
            let changes = changes.clone();
            cx.subscribe(
                &browser,
                move |_, event: &crate::browser::BrowserEvent, _| {
                    if matches!(event, crate::browser::BrowserEvent::Changed) {
                        changes.set(changes.get() + 1);
                    }
                },
            )
        });
        let editing = if in_address {
            browser.read_with(cx, |browser, cx| browser.address_input().focus_handle(cx))
        } else {
            composer.read_with(cx, |composer, cx| composer.focus_handle(cx))
        };
        cx.update(|window, cx| window.focus(&editing, cx));
        paint_twice(cx);
        cx.simulate_input("draft ");
        let typed_before = if in_address {
            browser.read_with(cx, |browser, cx| {
                browser.address_input().read(cx).text().to_owned()
            })
        } else {
            composer_text(cx)
        };
        assert!(typed_before.ends_with("draft "), "{typed_before:?}");

        // Discovery lists the port; the next render resolves the server.
        browser.update(cx, |browser, cx| {
            browser.set_test_previews(snapshot(port));
            cx.notify();
        });
        paint_twice(cx);
        paint_twice(cx);
        let (_, url) = active_browser_url(&shell, cx).unwrap();
        assert!(
            url.as_deref()
                .is_some_and(|url| url.starts_with(&format!("http://p{port}.preview.localhost"))),
            "{url:?}"
        );
        browser.read_with(cx, |browser, _| {
            assert_eq!(browser.pending_server_port(), None)
        });
        assert!(changes.get() >= 1, "BrowserEvent::Changed still emitted");
        assert!(
            cx.update(|window, _| editing.is_focused(window)),
            "the exact editing focus survives (address: {in_address})"
        );

        // Typing continues in the same input, with nothing replaced.
        cx.simulate_input("more");
        let typed_after = if in_address {
            browser.read_with(cx, |browser, cx| {
                browser.address_input().read(cx).text().to_owned()
            })
        } else {
            composer_text(cx)
        };
        assert_eq!(typed_after, format!("{typed_before}more"));
    }
}

/// How the native Browser pane leaves in a GF3 scenario.
#[derive(Clone, Copy, Debug)]
enum PaneExit {
    /// The user closes the pane: its close tween runs.
    Close,
    /// Reopening the (newer) sidebar makes the fit evict the older pane,
    /// which slides out on its fit-exit tween.
    FitEvictBySidebar,
    /// The window shrinks below the takeover's minimum: direct manipulation,
    /// so the fit drops the pane at once, with no exit tween.
    FitEvictByResize,
}

/// GF3 helper: with motion on, starts from a native Browser either beside
/// the widget (`corner == false`) or in the full-width takeover whose band
/// the widget sits in (`corner == true`), then lets the pane leave. Every
/// intermediate frame keeps each widget control out of the native page's
/// painted rect, or inside the bottom band the page leaves uncovered
/// (painting and hit testing are cropped there).
fn widget_clear_of_exiting_browser(cx: &mut TestAppContext, corner: bool, exit: PaneExit) {
    let (_dir, shell, cx) = setup(cx);
    cx.update(|_, cx| motion::set_reduced_motion(cx, false));
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| shell.add_browser_surface(None, window, cx))
    });
    make_call_live(&shell, cx, false);
    if corner {
        shell.update(cx, |shell, cx| {
            shell.toggle_right_pane_expand(cx);
            shell.toggle_sidebar(cx);
        });
    } else if matches!(exit, PaneExit::FitEvictBySidebar) {
        // Room for the conversation and the pane, not for the sidebar too.
        shell.update(cx, |shell, cx| shell.toggle_sidebar(cx));
        cx.simulate_resize(gpui::size(px(880.0), px(900.0)));
    }
    for _ in 0..20 {
        draw(&shell, cx, 100);
    }
    let start = cx.debug_bounds("right-panel").expect("the Browser pane");
    assert!(f32::from(start.size.width) > 0.0);
    let band = |cx: &mut VisualTestContext| {
        shell.read_with(cx, |shell, cx| {
            let id = *shell.browsers.keys().next().expect("a Browser");
            f32::from(shell.browsers[&id].read(cx).bottom_occlusion())
        })
    };
    if corner {
        assert_eq!(
            band(cx),
            crate::voice_surfaces::VOICE_WIDGET_BAND,
            "corner band"
        );
    } else {
        assert_eq!(band(cx), 0.0, "beside the page");
    }

    match exit {
        PaneExit::Close => change(&shell, cx, |shell, window, cx| {
            shell.toggle_right_pane(window, cx)
        }),
        PaneExit::FitEvictBySidebar => shell.update(cx, |shell, cx| shell.toggle_sidebar(cx)),
        PaneExit::FitEvictByResize => {
            cx.simulate_resize(gpui::size(px(RIGHT_PANE_MIN - 20.0), px(900.0)))
        }
    }
    let mut exiting_frames = 0;
    for frame in 0..60 {
        draw(&shell, cx, 16);
        let viewport = cx.update(|window, _| window.viewport_size());
        let Some(pane) = cx
            .debug_bounds("right-panel")
            .filter(|pane| f32::from(pane.size.width) > 0.5)
        else {
            continue;
        };
        exiting_frames += 1;
        let uncovered_from = f32::from(viewport.height) - band(cx);
        for control in [
            "voice-widget-mic",
            "voice-widget-return",
            "voice-widget-end",
        ] {
            let bounds = cx
                .debug_bounds(control)
                .unwrap_or_else(|| panic!("{control} painted at frame {frame}"));
            let beside = f32::from(bounds.right()) <= f32::from(pane.left()) + 0.5;
            let in_band = f32::from(bounds.top()) >= uncovered_from - 0.5;
            assert!(
                beside || in_band,
                "frame {frame} (corner {corner}, {exit:?}): {control} {bounds:?} under the \
                 exiting page {pane:?} with band {}",
                band(cx)
            );
        }
    }
    match exit {
        PaneExit::FitEvictByResize => assert_eq!(exiting_frames, 0, "direct: no exit tween"),
        _ => assert!(exiting_frames > 0, "the pane animated out ({exit:?})"),
    }
    assert!(
        cx.debug_bounds("right-panel")
            .is_none_or(|pane| f32::from(pane.size.width) <= 0.5),
        "the pane finished leaving ({exit:?})"
    );
    let open = shell.read_with(cx, |shell, cx| shell.right_pane_open(cx));
    assert_eq!(
        open,
        !matches!(exit, PaneExit::Close),
        "a fit eviction keeps the pane open for later"
    );
}

#[gpui::test]
fn voice_widget_clear_while_a_beside_browser_closes(cx: &mut TestAppContext) {
    widget_clear_of_exiting_browser(cx, false, PaneExit::Close);
}

#[gpui::test]
fn voice_widget_clear_while_a_beside_browser_is_fit_evicted(cx: &mut TestAppContext) {
    widget_clear_of_exiting_browser(cx, false, PaneExit::FitEvictBySidebar);
}

#[gpui::test]
fn voice_widget_clear_while_a_takeover_browser_closes(cx: &mut TestAppContext) {
    widget_clear_of_exiting_browser(cx, true, PaneExit::Close);
}

#[gpui::test]
fn voice_widget_clear_while_a_takeover_browser_is_fit_evicted(cx: &mut TestAppContext) {
    widget_clear_of_exiting_browser(cx, true, PaneExit::FitEvictByResize);
}

/// GH5: returning to a Chat whose saved destination restores a Browser
/// closes Details once, like any Browser coming into view; a manual reopen
/// on that same Browser then survives notifications and repaints; a Chat
/// whose destination is not a Browser clears the tracked Browser.
#[gpui::test]
fn restored_browser_destination_closes_details_once(cx: &mut TestAppContext) {
    let (_dir, shell, cx) = setup(cx);
    cx.update(|_, cx| motion::set_reduced_motion(cx, true));
    let details_open = |cx: &mut VisualTestContext| {
        shell.read_with(cx, |shell, _| shell.settings.details_sidebar_open)
    };
    let select = |cx: &mut VisualTestContext, chat: &str| {
        shell.update(cx, |shell, cx| {
            shell.state.update(cx, |state, cx| {
                state.selected_chat = Some(chat.into());
                cx.notify();
            })
        });
        paint_twice(cx);
    };
    let open_details = |cx: &mut VisualTestContext| {
        shell.update(cx, |shell, cx| {
            if !shell.settings.details_sidebar_open {
                shell.toggle_details_sidebar(cx);
            }
        });
        paint_twice(cx);
    };

    // Chat A shows a Browser (which closed Details when it appeared).
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| shell.add_browser_surface(None, window, cx))
    });
    paint_twice(cx);
    assert!(!details_open(cx));

    // Chat B has no Browser: switching there clears the tracked Browser.
    select(cx, "b");
    shell.read_with(cx, |shell, cx| {
        assert_eq!(shell.active_chat, "b");
        assert!(!matches!(
            shell.resolved_right_active(cx),
            RightSurface::Browser(_)
        ));
        assert_eq!(shell.presented_browser, None, "non-Browser destination");
    });
    open_details(cx);
    assert!(details_open(cx), "Details open on B");

    // Back to A: its restored Browser comes into view and closes Details.
    select(cx, "a");
    shell.read_with(cx, |shell, cx| {
        assert_eq!(shell.active_chat, "a");
        assert!(matches!(
            shell.resolved_right_active(cx),
            RightSurface::Browser(_)
        ));
        assert!(shell.right_pane_open(cx));
    });
    assert!(!details_open(cx), "the restored Browser closes Details");

    // A manual reopen on that same Browser survives notifications/repaints.
    open_details(cx);
    shell.update(cx, |shell, cx| {
        shell.state.update(cx, |_, cx| cx.notify());
        cx.notify();
    });
    paint_twice(cx);
    assert!(
        details_open(cx),
        "manual reopen on the same Browser is kept"
    );
}
