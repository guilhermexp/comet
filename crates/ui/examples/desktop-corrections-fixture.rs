//! Native review fixture for the desktop corrections (V1–V3, B1/MR9, C1):
//! the production presenters and components mounted on their own, with no
//! Shell, Workers model, engine, provider, call or audio device.
//!
//! Mounted for real: the voice stage, call bar, live-call widget and
//! "Show voice orb" preference row (`zeron_ui::voice_surfaces`, the same
//! presenters the Shell renders), the Composer over synthetic `AppState`
//! with its real Todo panel, and a `BrowserSurface` on an ephemeral,
//! isolated profile showing a loopback page served by this process.
//!
//! Each state is captured from the window server (native web content
//! included) into `<out-dir>/<state>.png`: on macOS the fixture's own
//! NSWindow, on Linux its own X11 window (`support/own_window_capture.rs`;
//! other Linux backends fail explicitly), never desktop pixels. A uniform
//! frame is rejected. `result.json` lists every capture and check, including
//! the Browser's exit frames (GF3), where the widget must stay beside the
//! page's animated width or inside the band it leaves. The process exits nonzero when any capture is
//! missing, the native page never became visible, or a check failed.
//!
//! cargo run -p zeron-ui --features appshots-fixture,browser-fixture \
//!   --example desktop-corrections-fixture -- <out-dir>
//! ZERON_PALETTE_LIGHT=1 renders the light theme; ZERON_FIXTURE_SURFACE=opaque
//! renders the opaque surface instead of the frosted one.
//! ZERON_FIXTURE_NEGATIVE=hide-native-during-exit is a negative acceptance
//! run: the real native page is presented hidden during one nonzero exit
//! sample, so that sample's checks and the whole run must fail (exit 1).
//! On macOS, ZERON_FIXTURE_NEGATIVE=fail-native-init forces construction
//! failure; hit probes are unavailable and result.json must still report failure.
//! On macOS, ZERON_FIXTURE_NEGATIVE=close-window-after-first-capture closes
//! this fixture window to prove partial failure reporting, without reopening it.
//! Probe points (widget controls and page) are derived from the live
//! viewport and placement for every checked state (GH2), and every exit
//! sample requires the page's own current native visibility (GH3).
use anyhow::Context as _;
use gpui::{
    AppContext, AsyncApp, Bounds, Context, Entity, FocusHandle, IntoElement, ParentElement, Render,
    Styled, Window, WindowBounds, WindowOptions, div, prelude::*, px, size,
};
use serde_json::json;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use zeron_ui::browser::{BrowserContext, BrowserSurface};
use zeron_ui::voice_surfaces::{self as voice, VoiceCommand, VoiceCommands};
use zeron_ui::*;

#[cfg(target_os = "linux")]
#[path = "support/own_window_capture.rs"]
mod own_window_capture;

const SIDEBAR: f32 = 260.0;
const PANE: f32 = 480.0;
/// GH2: the non-default size the window takes before the takeover and exit
/// checks (the initial window is 1440×900).
const RESIZED: (f32, f32) = (1280.0, 780.0);

#[derive(Clone, Copy, PartialEq, Eq)]
enum BrowserMode {
    Hidden,
    Pane,
    Takeover,
}

/// The fixture's own call state: what the Shell reads from its
/// `VoiceController`, held here so no controller or provider exists.
struct Call {
    live: bool,
    stage_open: bool,
    muted: bool,
    can_mute: bool,
    hovered: bool,
    ended: bool,
}

struct Fixture {
    composer: Entity<composer::Composer>,
    /// Released before quitting so the leak detector can prove it drops.
    browser: Option<Entity<BrowserSurface>>,
    stage_orb: Entity<orb::Orb>,
    widget_orb: Entity<orb::Orb>,
    stage_focus: FocusHandle,
    widget_focus: FocusHandle,
    call: Call,
    browser_mode: BrowserMode,
    /// While the page leaves (close or fit eviction): the share of its
    /// settled width still painted, as the Shell's exit tween paints it.
    exit: Option<f32>,
    /// Controlled frame time of an enlarged opening Details tween.
    details_progress: Option<f32>,
    painted_page: std::rc::Rc<std::cell::Cell<Option<Bounds<gpui::Pixels>>>>,
    painted_details: std::rc::Rc<std::cell::Cell<Option<Bounds<gpui::Pixels>>>>,
    /// Negative acceptance only (`ZERON_FIXTURE_NEGATIVE`): present the
    /// real native page as hidden for one exit sample.
    hide_native: bool,
    /// Presentation barrier (fixture only): the frame the script last asked
    /// for, and the last one this view actually rendered.
    frame_request: u64,
    rendered_frame: u64,
    sidebar_collapsed: bool,
    dialog: bool,
    settings_row: bool,
    /// The last placement the widget used, for the run's checks.
    placement: voice::VoiceWidgetPlacement,
}

/// Live probe points for one checked state (see [`Fixture::probes`]).
#[derive(Clone, Copy, Debug)]
struct Probes {
    viewport: (f32, f32),
    /// Mic, return and hang-up centers.
    controls: [(f32, f32); 3],
    controls_inside: bool,
    page: (f32, f32),
    page_inside: bool,
}

impl Probes {
    fn json(&self) -> serde_json::Value {
        json!({
            "viewport": [self.viewport.0, self.viewport.1],
            "controls": self.controls.map(|(x, y)| [x, y]),
            "controls_inside_window_and_capsule": self.controls_inside,
            "page": [self.page.0, self.page.1],
            "page_inside_window_and_page": self.page_inside,
        })
    }
}

impl Fixture {
    fn commands(cx: &mut Context<Self>) -> VoiceCommands {
        let this = cx.entity().downgrade();
        std::rc::Rc::new(move |command, _, cx| {
            this.update(cx, |this, cx| {
                match command {
                    VoiceCommand::ToggleMute if this.call.can_mute => {
                        this.call.muted = !this.call.muted
                    }
                    VoiceCommand::ToggleMute | VoiceCommand::OpenTranscript => {}
                    VoiceCommand::OpenStage => this.call.stage_open = true,
                    VoiceCommand::CloseStage => this.call.stage_open = false,
                    VoiceCommand::End => {
                        this.call.live = false;
                        this.call.ended = true;
                    }
                    VoiceCommand::WidgetHover(hovered) => this.call.hovered = hovered,
                }
                cx.notify();
            })
            .ok();
        })
    }

    fn details_target(&self, width: f32) -> f32 {
        if self.details_progress.is_some() {
            (width - 980.0).clamp(0.0, 600.0)
        } else {
            0.0
        }
    }

    fn details_width(&self, width: f32) -> f32 {
        let target = self.details_target(width);
        voice::painted_details_width(target, || {
            self.details_progress
                .filter(|t| *t < 1.0)
                .map_or(target, |t| {
                    motion::lerp(0.0, 460.0, motion::RESIZE.curve.eval(t))
                })
        })
    }

    fn sidebar_width(&self) -> f32 {
        if self.sidebar_collapsed { 0.0 } else { SIDEBAR }
    }

    /// The page's painted width: settled, or part way through its exit.
    fn page_width(&self, viewport_width: f32) -> f32 {
        let settled = match self.browser_mode {
            BrowserMode::Hidden => 0.0,
            BrowserMode::Pane => PANE,
            BrowserMode::Takeover => viewport_width - self.sidebar_width(),
        };
        settled * self.exit.unwrap_or(1.0)
    }

    /// GH2: probe points derived from the live viewport and the placement in
    /// force, with the production widget constants: the centers of the mic,
    /// return and hang-up controls, and a page point above the band the page
    /// leaves. Each is checked to lie inside the window and its own region,
    /// so a probe can never pass by falling outside the window.
    fn probes(&self, viewport: gpui::Size<gpui::Pixels>) -> Probes {
        let (width, height) = (f32::from(viewport.width), f32::from(viewport.height));
        let capsule_right = width - self.placement.right;
        let capsule_left = capsule_right - voice::WIDGET_WIDTH;
        let capsule_bottom = height - voice::WIDGET_MARGIN;
        let capsule_top = capsule_bottom - voice::WIDGET_HEIGHT;
        let y = capsule_bottom - voice::WIDGET_HEIGHT / 2.0;
        let mic =
            capsule_left + voice::WIDGET_BORDER + voice::WIDGET_PAD + voice::WIDGET_CONTROL / 2.0;
        let back =
            mic + voice::WIDGET_CONTROL / 2.0 + voice::WIDGET_GAP + voice::WIDGET_RETURN / 2.0;
        let end =
            back + voice::WIDGET_RETURN / 2.0 + voice::WIDGET_GAP + voice::WIDGET_CONTROL / 2.0;
        let controls = [(mic, y), (back, y), (end, y)];
        let in_window = |(x, y): (f32, f32)| x > 0.0 && x < width && y > 0.0 && y < height;
        let controls_inside = controls.iter().all(|&(x, y)| {
            in_window((x, y))
                && x > capsule_left
                && x < capsule_right
                && y > capsule_top
                && y < capsule_bottom
        });
        let page_width = self.page_width(width);
        let page_left = width - self.details_width(width) - page_width;
        let uncovered_from = height - self.placement.native_band;
        let page = (page_left + page_width / 2.0, uncovered_from / 2.0);
        let page_inside =
            page_width > 0.0 && in_window(page) && page.0 > page_left && page.1 < uncovered_from;
        Probes {
            viewport: (width, height),
            controls,
            controls_inside,
            page,
            page_inside,
        }
    }

    /// How much of the window's right edge the native page covers, by the
    /// production rule (its animated visible width).
    fn native_inset(&self, viewport_width: f32) -> f32 {
        voice::native_right_inset(
            self.browser_mode != BrowserMode::Hidden,
            self.page_width(viewport_width),
            self.details_width(viewport_width),
        )
    }
}

impl Render for Fixture {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.rendered_frame = self.frame_request;
        let theme = theme::Theme::of(cx).clone();
        let viewport = window.viewport_size();
        let width = f32::from(viewport.width);
        let orb_enabled = settings::current(cx).voice_orb_enabled;
        let widget_shown = self.call.live && !self.call.stage_open;
        self.placement =
            voice::voice_widget_placement(widget_shown, width, self.native_inset(width));
        let band = px(self.placement.native_band);
        let presentation = if self.browser_mode == BrowserMode::Hidden || self.hide_native {
            browser::model::Presentation::Hidden
        } else {
            browser::model::presentation(true, false)
        };
        if let Some(browser) = &self.browser {
            browser.update(cx, |browser, cx| {
                browser.set_bottom_occlusion(band, cx);
                browser.set_presentation(presentation, cx);
            });
        }
        let (stage_orb_on, widget_orb_on) = (
            orb_enabled && self.call.stage_open,
            orb_enabled && widget_shown,
        );
        self.stage_orb.update(cx, |orb, cx| {
            orb.set_visible(stage_orb_on, cx);
            orb.set_reduced_motion(true, cx);
        });
        self.widget_orb.update(cx, |orb, cx| {
            orb.set_visible(widget_orb_on, cx);
            orb.set_reduced_motion(true, cx);
        });
        let commands = Self::commands(cx);

        let sidebar = div()
            .w(px(self.sidebar_width()))
            .h_full()
            .flex_none()
            .overflow_hidden()
            .border_r_1()
            .border_color(theme.border)
            .p(px(16.0))
            .text_color(theme.text_muted)
            .child("Chats");
        let main = div()
            .flex_1()
            .min_w_0()
            .h_full()
            .flex()
            .flex_col()
            .justify_end()
            .children(self.settings_row.then(|| {
                div().p(px(24.0)).child(
                    settings::widgets::section_card(&theme)
                        .child(voice::show_voice_orb_row(&theme, cx)),
                )
            }))
            .child(div().flex_1())
            .child(self.composer.clone());
        let painted_page = self.painted_page.clone();
        let page = div()
            .relative()
            .child(
                gpui::canvas(
                    |_, _, _| {},
                    move |bounds, _, _, _| painted_page.set(Some(bounds)),
                )
                .absolute()
                .inset_0()
                .size_full(),
            )
            .h_full()
            .border_l_1()
            .border_color(theme.border)
            .children(self.browser.clone());
        let row = div().size_full().flex().flex_row().child(sidebar);
        let row = match (self.browser_mode, self.exit) {
            (BrowserMode::Hidden, _) => row.child(main),
            (_, Some(_)) => row
                .child(main)
                .child(page.w(px(self.page_width(width))).flex_none()),
            (BrowserMode::Pane, None) => row.child(main).child(page.w(px(PANE)).flex_none()),
            (BrowserMode::Takeover, None) => row.child(page.flex_1()),
        };

        let painted_details = self.painted_details.clone();
        let row = row.children((self.details_target(width) > 0.5).then(|| {
            div()
                .relative()
                .flex_none()
                .w(px(self.details_width(width)))
                .h_full()
                .bg(theme.bg)
                .border_l_1()
                .border_color(theme.border)
                .child("Details")
                .child(
                    gpui::canvas(
                        |_, _, _| {},
                        move |bounds, _, _, _| painted_details.set(Some(bounds)),
                    )
                    .absolute()
                    .inset_0()
                    .size_full(),
                )
        }));

        let stage = self.call.stage_open.then(|| {
            let left = self.sidebar_width();
            voice::voice_stage(
                &theme,
                voice::VoiceStageProps {
                    reveal: 1.0,
                    left,
                    width: width - left,
                    focus: self.stage_focus.clone(),
                    hero: None,
                    orb: orb_enabled.then(|| self.stage_orb.clone().into_any_element()),
                    caption: voice::veiled_caption(
                        "Vou abrir o projeto e te conto o que achei.".into(),
                        &[],
                        &theme,
                        window,
                    ),
                    previous_caption: None,
                    controls: voice::voice_call_bar(
                        &theme,
                        voice::VoiceCallBarProps {
                            muted: self.call.muted,
                            can_mute: self.call.can_mute,
                            awaiting: false,
                            elapsed: Some("2:14".into()),
                            host_name: Some("This Mac".into()),
                            transcript: true,
                        },
                        &commands,
                    ),
                    titlebar: div().into_any_element(),
                },
                &commands,
            )
        });
        let widget = widget_shown.then(|| {
            voice::voice_widget(
                voice::VoiceWidgetProps {
                    muted: self.call.muted,
                    can_mute: self.call.can_mute,
                    revealed: self.call.hovered || self.widget_focus.contains_focused(window, cx),
                    orb: widget_orb_on.then(|| self.widget_orb.clone().into_any_element()),
                    focus: self.widget_focus.clone(),
                    right: self.placement.right,
                },
                &commands,
                cx,
            )
        });
        // Dialogs sit above the widget, as in the Shell's layer order.
        let dialog = self.dialog.then(|| {
            div()
                .absolute()
                .inset_0()
                .flex()
                .items_center()
                .justify_center()
                .bg(gpui::black().opacity(0.35))
                .occlude()
                .child(
                    popover::dialog_card(&theme)
                        .gap(px(8.0))
                        .child(popover::dialog_title(&theme, "Archive this chat?"))
                        .child(popover::dialog_body(
                            &theme,
                            "The call keeps running while this dialog is open.",
                        )),
                )
        });
        div()
            .size_full()
            .relative()
            .bg(theme.bg)
            .text_color(theme.text)
            .child(row)
            .children(stage)
            .children(widget)
            .children(dialog)
    }
}

async fn pause(cx: &mut AsyncApp, ms: u64) {
    cx.background_executor()
        .timer(Duration::from_millis(ms))
        .await;
}

/// The fixture's own NSWindow number, read from its GPUI window handle on
/// the main thread (never NSApp's key or main window, which another app can
/// hold). Fails if the view has no window or the window is not visible.
#[cfg(target_os = "macos")]
fn own_window_number(window: &Window) -> anyhow::Result<isize> {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    let handle = HasWindowHandle::window_handle(window)
        .map_err(|error| anyhow::anyhow!("the fixture window has no native handle: {error}"))?;
    let RawWindowHandle::AppKit(appkit) = handle.as_raw() else {
        anyhow::bail!("the fixture window is not an AppKit window");
    };
    // SAFETY: GPUI's AppKit handle points at its live NSView; read on the
    // main thread, inside the window update that keeps it alive.
    let view: &objc2_app_kit::NSView = unsafe { appkit.ns_view.cast().as_ref() };
    let ns_window = view
        .window()
        .ok_or_else(|| anyhow::anyhow!("the fixture view has no NSWindow"))?;
    anyhow::ensure!(ns_window.isVisible(), "the fixture NSWindow is not visible");
    Ok(ns_window.windowNumber())
}

/// Presentation barrier before a state-dependent native sample or capture:
/// asks the fixture to render a new frame request, draws and presents the
/// window now (the existing `browser-fixture` pattern), and proves that the
/// fixture's render ran for that request, so the drawn and presented frame,
/// native view geometry included, is the current scripted state.
fn present_current(cx: &mut AsyncApp, window: gpui::WindowHandle<Fixture>) -> anyhow::Result<u64> {
    let requested = window.update(cx, |f, _, _| {
        f.frame_request += 1;
        f.frame_request
    })?;
    gpui::AnyWindowHandle::from(window).update(cx, |_, w, cx| {
        w.refresh();
        let arena = w.draw(cx);
        w.present_if_needed();
        arena.clear();
    })?;
    let rendered = window.update(cx, |f, _, _| f.rendered_frame)?;
    anyhow::ensure!(
        rendered == requested,
        "the presented frame did not render the current state ({rendered} != {requested})"
    );
    Ok(requested)
}

/// One state capture: present the current state, re-resolve the fixture's
/// own window identity on the main thread (it must equal the identity
/// resolved at start), then capture that window on a background thread so
/// AppKit keeps running while `screencapture` waits for its image. Only the
/// window number and the path cross threads.
async fn shot(
    report: &mut Report,
    cx: &mut AsyncApp,
    window: gpui::WindowHandle<Fixture>,
    identity: Option<i64>,
    out: &Path,
    name: &str,
) {
    let path = out.join(format!("{name}.png"));
    let presented = present_current(cx, window);
    let frame = presented.as_ref().ok().copied();
    let result = match presented {
        Err(error) => Err(error),
        Ok(_) => {
            #[cfg(target_os = "macos")]
            let target = gpui::AnyWindowHandle::from(window)
                .update(cx, |_, w, _| own_window_number(w))
                .map_err(anyhow::Error::from)
                .and_then(|number| number)
                .and_then(|number| {
                    let number = number as i64;
                    anyhow::ensure!(
                        identity == Some(number),
                        "the fixture window identity changed ({identity:?} -> {number})"
                    );
                    Ok(number)
                });
            #[cfg(not(target_os = "macos"))]
            let target: anyhow::Result<i64> = Ok(identity.unwrap_or_default());
            match target {
                Err(error) => Err(error),
                Ok(number) => {
                    cx.background_executor()
                        .spawn(async move { capture(number, &path) })
                        .await
                }
            }
        }
    };
    report.failed |= result.is_err();
    report.captures.push(json!({
        "state": name,
        "captured": result.as_ref().is_ok(),
        "presented_frame": frame,
        "window": result.as_ref().ok(),
        "expected_window": identity,
        "error": result.as_ref().err().map(|e| e.to_string()),
    }));
}

/// Background part of a capture: only a window number and a path. On macOS
/// the fixture's own NSWindow by number; on Linux its own X11 window (found
/// by `_NET_WM_PID`, other backends fail). Returns the captured window.
fn capture(number: i64, path: &Path) -> anyhow::Result<i64> {
    #[cfg(target_os = "macos")]
    let captured = {
        let status = std::process::Command::new("/usr/sbin/screencapture")
            .args(["-x", "-o", "-l", &number.to_string()])
            .arg(path)
            .status()?;
        anyhow::ensure!(status.success(), "screencapture failed for window {number}");
        number
    };
    #[cfg(target_os = "linux")]
    let captured = {
        let _ = number;
        i64::from(own_window_capture::capture_own_window(path)?)
    };
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    let captured: i64 = {
        let _ = number;
        anyhow::bail!("own-window capture is not supported on this platform")
    };
    anyhow::ensure!(
        std::fs::metadata(path).is_ok_and(|m| m.len() > 0),
        "capture wrote no image"
    );
    // A uniform frame (for example a compositor that could not present
    // GPU content) is a missing capture, not evidence.
    let image = image::open(path)?.to_rgb8();
    let first = image.pixels().next().copied();
    anyhow::ensure!(
        image.pixels().any(|pixel| Some(*pixel) != first),
        "capture is a uniform frame (nothing was presented)"
    );
    Ok(captured)
}

fn isolate(root: &Path) {
    let home = root.join("home");
    std::fs::create_dir_all(&home).unwrap();
    // SAFETY: single-threaded, before any runtime or window starts.
    unsafe {
        std::env::set_var("HOME", &home);
        std::env::set_var("ZERON_DATA_DIR", root.join("data"));
        std::env::set_var("UNPEEL_HOME", root.join("unpeel"));
        std::env::set_var("ZERON_WORKTREES_DIR", root.join("worktrees"));
        std::env::set_var("ZERON_WORKTREE_OWNERSHIP_FILE", root.join("ownership.json"));
        std::env::set_var("ZERON_AUTO_UPDATE", "0");
    }
}

/// An ephemeral loopback page the native Browser loads; nothing leaves
/// this machine.
fn serve_page() -> anyhow::Result<String> {
    let listener = std::net::TcpListener::bind("127.0.0.1:0")?;
    let origin = format!("http://{}", listener.local_addr()?);
    std::thread::spawn(move || {
        for mut stream in listener.incoming().flatten() {
            let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
            let mut request = [0; 4096];
            let _ = stream.read(&mut request);
            let body = "<!doctype html><meta charset=utf-8><title>Fieldnotes</title><style>body{margin:0;padding:42px 32px;background:#f5f2eb;color:#263d35;font:15px/1.6 system-ui;min-height:100vh;box-sizing:border-box;background-image:linear-gradient(0deg,#d9ddd0 1px,transparent 1px);background-size:100% 40px}h1{font:500 45px/1.1 Georgia}</style><h1>Make room<br>for good work.</h1><p>The lower edge of this page is ruled so a covered band is visible.</p>";
            let _ = write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
        }
    });
    Ok(origin)
}

fn todo_entry() -> zeron_doc::SessionMessageEntry {
    zeron_doc::SessionMessageEntry {
        id: "a1".into(),
        role: zeron_doc::MessageRole::Assistant,
        parts: vec![zeron_doc::MessagePart::Tool {
            exit_code: Default::default(),
            output_truncated: Default::default(),
            output_lines: Default::default(),

            id: "todo".into(),
            call: zeron_proto::ToolCall::Todo {
                items: (0..16)
                    .map(|i| {
                        zeron_proto::TodoItem::new(
                            format!("Implement step {i}"),
                            if i < 12 {
                                zeron_proto::TodoStatus::Completed
                            } else {
                                zeron_proto::TodoStatus::Pending
                            },
                        )
                    })
                    .collect(),
            },
            is_error: false,
            resolved: true,
            output: None,
            diff: None,
            output_ref: None,
            output_bytes: None,
            diff_ref: None,
            diff_stats: None,
            subagent_ref: None,
            subagent_status: None,
            subagent_tail: None,
            subagent_end: None,
            execution: None,
            file_preview: None,
        }],
        created_at: 1_788_900_000_000,
        device_id: "local".into(),
        status: None,
        continuation_of: None,
        duration_ms: None,
    }
}

#[derive(Default)]
struct Report {
    captures: Vec<serde_json::Value>,
    checks: Vec<serde_json::Value>,
    failed: bool,
}

impl Report {
    fn check(&mut self, name: &str, ok: bool, detail: serde_json::Value) {
        self.failed |= !ok;
        self.checks
            .push(json!({ "check": name, "ok": ok, "detail": detail }));
    }
}

fn main() -> anyhow::Result<()> {
    let out: PathBuf = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .ok_or_else(|| anyhow::anyhow!("usage: desktop-corrections-fixture <out-dir>"))?;
    std::fs::create_dir_all(&out)?;
    let temp = tempfile::tempdir()?;
    isolate(temp.path());
    let runtime = tokio::runtime::Runtime::new()?;
    let _guard = runtime.enter();
    let data = temp.path().join("data");
    std::fs::create_dir_all(&data)?;
    let remote_mismatch =
        std::env::var("ZERON_FIXTURE_CASE").as_deref() == Ok("remote-server-identity");
    let remote_server = remote_mismatch
        || std::env::var("ZERON_FIXTURE_CASE").as_deref() == Ok("remote-ineligible-server");
    let origin = serve_page()?;
    let opaque = std::env::var("ZERON_FIXTURE_SURFACE").as_deref() == Ok("opaque");
    // GH3 negative acceptance: hide the real native page during one nonzero
    // exit sample; that sample and the run must fail (nonzero exit).
    let negative =
        std::env::var("ZERON_FIXTURE_NEGATIVE").as_deref() == Ok("hide-native-during-exit");
    let fail_init = std::env::var("ZERON_FIXTURE_NEGATIVE").as_deref() == Ok("fail-native-init");
    let close_window = std::env::var("ZERON_FIXTURE_NEGATIVE").as_deref()
        == Ok("close-window-after-first-capture");
    let result_path = out.join("result.json");
    let outcome: Arc<Mutex<Option<Result<(), String>>>> = Arc::default();
    let finished = outcome.clone();
    gpui_platform::application()
        .with_assets(icons::Assets)
        .run(move |cx| {
            gpui_tokio::init(cx);
            gpui_base::init(cx);
            let mut ui = settings::UiSettings::default();
            ui.session_info_hidden = true;
            ui.surface = if opaque {
                zeron_theme::SurfacePreference::Opaque
            } else {
                zeron_theme::SurfacePreference::Frosted
            };
            settings::init(ui.clone(), data.clone(), cx);
            let fonts = typography::register_fonts(cx);
            typography::init(ui.ui_font_family.clone(), ui.ui_font_size, ui.terminal_font_family.clone(), ui.terminal_font_size, ui.code_font_family.clone(), ui.code_font_size, fonts, cx);
            theme_library::init(data.clone(), cx);
            let mode = if std::env::var_os("ZERON_PALETTE_LIGHT").is_some() {
                appearance::AppearanceMode::Light
            } else {
                appearance::AppearanceMode::Dark
            };
            appearance::init(mode, ui.theme_selection, ui.accent, ui.surface, cx);
            composer::init(cx, ui.composer_send_behavior);
            let state = cx.new(|_| {
                let mut s = state::AppState::new();
                s.connection = zeron_proto::view::ConnectionStatus::Ready;
                s.workspace_scope = Some(zeron_proto::WorkspaceScope::Local);
                s.local_device_id = Some("local".into());
                s.devices = vec![serde_json::from_value(json!({"id":"local","name":"This Mac","platform":std::env::consts::OS,"lastSeenAt":null})).unwrap()];
                s.chats = vec![serde_json::from_value(json!({
                    "id": "chat-a", "deviceId": "local", "title": "Fixture chat", "archived": false,
                    "createdAt": "2026-11-01T00:00:00Z",
                    "config": {"harness":"claude-code","model":null,"reasoning":null,"sandbox":"workspace-write"}
                })).unwrap()];
                s.selected_chat = Some("chat-a".into());
                s.auto_selected = true;
                s.chats_synced = true;
                s.spaces_synced = true;
                s.no_project = true;
                s
            });
            let window = cx
                .open_window(
                    WindowOptions {
                        window_bounds: Some(WindowBounds::Windowed(Bounds::new(gpui::point(px(40.), px(40.)), size(px(1440.), px(900.))))),
                        titlebar: Some(gpui::TitlebarOptions { title: None, appears_transparent: true, traffic_light_position: Some(gpui::point(px(14.), px(14.))) }),
                        app_owns_titlebar_drag: true,
                        window_background: theme::Theme::of(cx).window_background_appearance(),
                        ..Default::default()
                    },
                    |window, cx| {
                        let composer = cx.new(|cx| composer::Composer::new(state.clone(), cx));
                        // An ephemeral default context: no saved profile.
                        let browser = cx.new(|cx| BrowserSurface::new(BrowserContext::default(), remote_server, window, cx));
                        cx.new(|cx| Fixture {
                            composer,
                            browser: Some(browser),
                            stage_orb: cx.new(|_| voice::stage_orb()),
                            widget_orb: cx.new(|_| voice::widget_orb()),
                            stage_focus: cx.focus_handle(),
                            widget_focus: cx.focus_handle(),
                            call: Call { live: false, stage_open: false, muted: false, can_mute: true, hovered: false, ended: false },
                            browser_mode: BrowserMode::Hidden,
                            exit: None,
                            details_progress: None,
                            painted_page: Default::default(),
                            painted_details: Default::default(),
                            hide_native: false,
                            frame_request: 0,
                            rendered_frame: 0,
                            sidebar_collapsed: false,
                            dialog: false,
                            settings_row: false,
                            placement: voice::voice_widget_placement(false, 1440.0, 0.0),
                        })
                    },
                )
                .unwrap();
            appearance::reapply_window_background(cx);
            cx.activate(true);
            let out = out.clone();
            cx.spawn(async move |cx| {
                let mut report = Report::default();
                let mut phase = String::from("initial frame");
                let run: anyhow::Result<()> = async {
                let update = |cx: &mut AsyncApp, f: &dyn Fn(&mut Fixture, &mut Window, &mut Context<Fixture>)| {
                    window.update(cx, |fixture, window, cx| { f(fixture, window, cx); cx.notify(); }).context("update fixture window")
                };
                let resize = |cx: &mut AsyncApp, w: f32, h: f32| {
                    gpui::AnyWindowHandle::from(window).update(cx, |_, window, _| window.resize(size(px(w), px(h)))).context("resize fixture window")
                };
                pause(cx, 700).await;
                // The fixture's own window identity, resolved once from its
                // GPUI handle on the main thread and kept for every capture.
                present_current(cx, window).context("present initial frame")?;
                #[cfg(target_os = "macos")]
                let identity: Option<i64> = {
                    let resolved = gpui::AnyWindowHandle::from(window)
                        .update(cx, |_, w, _| own_window_number(w))
                        .context("resolve fixture window identity")?;
                    report.check(
                        "own_window_identity",
                        resolved.is_ok(),
                        json!({ "window_number": resolved.as_ref().ok(), "pid": std::process::id(), "error": resolved.as_ref().err().map(|e| e.to_string()) }),
                    );
                    Some(resolved.context("resolve own native window")? as i64)
                };
                // Linux finds its own X11 window by `_NET_WM_PID` at capture.
                #[cfg(not(target_os = "macos"))]
                let identity: Option<i64> = None;
                if remote_server {
                    phase = "remote server unavailable".into();
                    update(cx, &|f, window, cx| {
                        f.browser_mode = BrowserMode::Takeover;
                        f.browser.as_ref().unwrap().update(cx, |browser, cx| {
                            let selected: zeron_proto::DevServer = serde_json::from_value(json!({
                                "pid":8,"startedAt":2,"port":3210,"cwd":"/b","root":"/b","command":"next dev","previewEligible":false
                            })).expect("synthetic selected server");
                            let unrelated: zeron_proto::PreviewService = serde_json::from_value(json!({
                                "id":"a","projectId":"a","projectName":"a","projectCwd":"/a",
                                "deviceId":"remote","deviceName":"Build box","hostname":"unrelated.preview.localhost",
                                "name":"vite","port":3210,"pid":7,"cwd":"/a","startedAt":1,"zeronOwned":true,"chatId":"a"
                            })).expect("synthetic unrelated preview");
                            browser.set_test_previews(zeron_proto::PreviewSnapshot {
                                services: if remote_mismatch { vec![unrelated] } else { vec![] },
                                remote: true, ..Default::default()
                            });
                            browser.open_server(&selected, window, cx);
                        });
                    })?;
                    pause(cx, 300).await;
                    shot(&mut report, cx, window, identity, &out, "remote-server-unavailable").await;
                    let safe = window.update(cx, |f, _, cx| {
                        let browser = f.browser.as_ref().unwrap().read(cx);
                        browser.page.url.is_none() && browser.pending_server_port().is_none()
                    }).context("inspect remote server outcome")?;
                    report.check("remote_ineligible_has_no_navigation_or_pending", safe, json!({"port":3210,"selectedPid":8,"selectedStartedAt":2,"unrelatedPid": if remote_mismatch { Some(7) } else { None }}));
                    update(cx, &|f, _, _| { f.browser = None; f.browser_mode = BrowserMode::Hidden; })?;
                    return Ok(());
                }
                state.update(cx, |s, cx| {
                    s.receive_transcript_frame(zeron_doc::TranscriptFrame::Reset { reset: vec![todo_entry()] }, cx).ok();
                    cx.notify();
                });
                pause(cx, 300).await;

                phase = "stage controls".into();
                // V1/V2: the stage, orb on then off.
                update(cx, &|f, _, _| { f.call.live = true; f.call.stage_open = true; f.call.can_mute = false; })?;
                pause(cx, 400).await;
                shot(&mut report, cx, window, identity, &out, "00-stage-connecting").await;
                if close_window {
                    phase = "controlled window removal after first capture".into();
                    window.update(cx, |_, window, _| window.remove_window())
                        .context("remove fixture window")?;
                }
                phase = "stage ready after first capture".into();
                update(cx, &|f, _, _| f.call.can_mute = true)?;
                pause(cx, 200).await;
                shot(&mut report, cx, window, identity, &out, "01-stage-orb-on").await;
                cx.update(|cx| settings::update(settings::SavePolicy::Immediate, cx, |s| s.voice_orb_enabled = false));
                update(cx, &|_, _, _| {})?;
                pause(cx, 300).await;
                shot(&mut report, cx, window, identity, &out, "02-stage-orb-off").await;
                cx.update(|cx| settings::update(settings::SavePolicy::Immediate, cx, |s| s.voice_orb_enabled = true));

                phase = "voice preference".into();
                // The preference row itself, both states.
                update(cx, &|f, _, _| { f.call.stage_open = false; f.settings_row = true; })?;
                pause(cx, 300).await;
                shot(&mut report, cx, window, identity, &out, "03-preference-row-on").await;
                cx.update(|cx| settings::update(settings::SavePolicy::Immediate, cx, |s| s.voice_orb_enabled = false));
                update(cx, &|_, _, _| {})?;
                pause(cx, 300).await;
                shot(&mut report, cx, window, identity, &out, "04-preference-row-off-widget-static").await;
                cx.update(|cx| settings::update(settings::SavePolicy::Immediate, cx, |s| s.voice_orb_enabled = true));
                update(cx, &|f, _, _| f.settings_row = false)?;

                phase = "voice widget".into();
                // V3: widget normal, muted, hovered (End revealed), focused.
                pause(cx, 300).await;
                shot(&mut report, cx, window, identity, &out, "05-widget-normal").await;
                update(cx, &|f, _, _| f.call.muted = true)?;
                pause(cx, 200).await;
                shot(&mut report, cx, window, identity, &out, "06-widget-muted").await;
                update(cx, &|f, _, _| { f.call.muted = false; f.call.hovered = true; })?;
                pause(cx, 200).await;
                shot(&mut report, cx, window, identity, &out, "07-widget-hover-end-revealed").await;
                update(cx, &|f, w, cx| { f.call.hovered = false; w.focus(&f.widget_focus, cx); })?;
                pause(cx, 200).await;
                shot(&mut report, cx, window, identity, &out, "08-widget-keyboard-focus").await;

                phase = "composer paste".into();
                // C1: long paste staged inside the composer under the Todo.
                window.update(cx, |f, _, cx| {
                    f.composer.update(cx, |composer, cx| {
                        composer.fixture_stage_long_paste(format!("# Entender Projeto Comet\n{}", "linha de contexto\n".repeat(2000)), cx);
                        composer.fixture_set_draft("sim pode fazer, dps ja faz um teste real", cx);
                    });
                }).context("stage composer paste")?;
                pause(cx, 400).await;
                shot(&mut report, cx, window, identity, &out, "09-composer-pasted-card-inside-under-todo").await;

                phase = "dialog".into();
                // Dialog over the widget.
                update(cx, &|f, _, _| f.dialog = true)?;
                pause(cx, 300).await;
                shot(&mut report, cx, window, identity, &out, "10-dialog-over-widget").await;
                update(cx, &|f, _, _| f.dialog = false)?;

                phase = "native browser loading".into();
                // Native Browser, wide: the widget sits beside the page.
                update(cx, &|f, _, _| f.browser_mode = BrowserMode::Pane)?;
                let browser = window.update(cx, |f, w, cx| {
                    let browser = f.browser.clone().expect("the Browser is mounted");
                    browser.update(cx, |b, cx| {
                        if fail_init { b.fixture_fail_native_init(); }
                        b.navigate(&origin, w, cx);
                    });
                    browser
                }).context("read fixture window state")?;
                let deadline = std::time::Instant::now() + Duration::from_secs(25);
                // Only the loopback page itself counts: a failed navigation can
                // leave an older native frame visible, so require no error and
                // the page's own title as well as native visibility.
                let mut visible = false;
                while std::time::Instant::now() < deadline {
                    // Present the current state before reading native state.
                    present_current(cx, window).context("present loading native page")?;
                    let (loaded, failed) = browser.read_with(cx, |b, _| {
                        (
                            !b.page.loading
                                && b.page.error.is_none()
                                && b.page.title == "Fieldnotes"
                                && b.fixture_native_visible(),
                            b.page.error.is_some(),
                        )
                    });
                    if loaded || failed {
                        visible = loaded;
                        break;
                    }
                    pause(cx, 50).await;
                }
                let presented = present_current(cx, window).context("present measured frame")?;
                let (title, error, native) = browser.read_with(cx, |b, _| {
                    (b.page.title.to_string(), b.page.error.clone(), b.fixture_native_visible())
                });
                report.check("native_page_visible", visible, json!({
                    "presented_frame": Some(presented),
                    "origin": origin,
                    "title": title,
                    "error": error,
                    "native_visible": native,
                }));
                pause(cx, 300).await;
                let presented = present_current(cx, window).context("present measured frame")?;
                let placement = window.update(cx, |f, _, _| f.placement).context("read fixture window state")?;
                report.check("wide_widget_beside_page", native && placement.native_band == 0.0 && placement.right > voice::WIDGET_MARGIN, json!({ "presented_frame": Some(presented), "native_visible": native, "right": placement.right, "band": placement.native_band }));
                shot(&mut report, cx, window, identity, &out, "11-wide-widget-beside-native-page").await;

                phase = "Details opening resize".into();
                // GJ6: opening enlarged Details retains its 460px tween endpoint
                // while resize lowers the responsive target. Sample the actual
                // layout/crop/native hit test at controlled intermediate times.
                update(cx, &|f, _, _| f.details_progress = Some(0.45))?;
                present_current(cx, window).context("present opening Details")?;
                resize(cx, RESIZED.0, RESIZED.1)?;
                pause(cx, 300).await;
                for progress in [0.55_f32, 0.75, 0.9] {
                    update(cx, &|f, _, _| f.details_progress = Some(progress))?;
                    let presented = present_current(cx, window).context("present measured frame")?;
                    let (probes, page, details, target, placement) = window.update(cx, |f, w, _| (
                        f.probes(w.viewport_size()), f.painted_page.get(), f.painted_details.get(),
                        f.details_target(f32::from(w.viewport_size().width)), f.placement
                    )).context("read fixture window state")?;
                    #[cfg(target_os = "macos")]
                    {
                        let hits = probes.controls.map(|(x, y)| browser.read_with(cx, |b, _| b.fixture_page_hit(x as f64, y as f64)));
                        let page_hit = browser.read_with(cx, |b, _| b.fixture_page_hit(probes.page.0 as f64, probes.page.1 as f64));
                        let clear = page.is_some_and(|page| probes.controls.iter().all(|&(x, _)| x + voice::WIDGET_RETURN / 2.0 < f32::from(page.left())))
                            || placement.native_band == voice::VOICE_WIDGET_BAND;
                        let rect = |b: Option<Bounds<gpui::Pixels>>| b.map(|b| [f32::from(b.left()), f32::from(b.top()), f32::from(b.size.width), f32::from(b.size.height)]);
                        report.check(&format!("opening_details_resize_{:.0}", progress * 100.0),
                            clear && probes.controls_inside && hits.iter().all(|hit| *hit == Some(false)) && page_hit == Some(true)
                                && details.is_some_and(|b| f32::from(b.size.width) > target),
                            json!({"presented_frame": Some(presented), "progress": progress, "target": target,
                                "painted_details": rect(details), "painted_page": rect(page), "probes": probes.json(),
                                "control_points_hit_page": hits, "native_error": browser.read_with(cx, |b, _| b.page.error.clone()), "page_point_hits_page": page_hit, "band": placement.native_band}));
                    }
                    shot(&mut report, cx, window, identity, &out, &format!("11-details-opening-resized-{:.0}", progress * 100.0)).await;
                }
                update(cx, &|f, _, _| f.details_progress = None)?;

                phase = "resize before takeover".into();
                // GH2: a non-default window size before the takeover and exit
                // checks, so every probe below must follow the live geometry.
                resize(cx, RESIZED.0, RESIZED.1)?;
                pause(cx, 600).await;
                let resized = window.update(cx, |_, w, _| (f32::from(w.viewport_size().width), f32::from(w.viewport_size().height))).context("read fixture window state")?;
                report.check("viewport_resized_before_takeover", resized != (1440.0, 900.0), json!({ "viewport": [resized.0, resized.1], "requested": [RESIZED.0, RESIZED.1] }));

                phase = "takeover".into();
                // Takeover with a collapsed sidebar: the corner, the page's
                // bottom band left to the widget.
                update(cx, &|f, _, _| { f.browser_mode = BrowserMode::Takeover; f.sidebar_collapsed = true; })?;
                pause(cx, 500).await;
                let presented = present_current(cx, window).context("present measured frame")?;
                let (placement, probes) = window.update(cx, |f, w, _| (f.placement, f.probes(w.viewport_size()))).context("read fixture window state")?;
                let occlusion = browser.read_with(cx, |b, _| f32::from(b.bottom_occlusion()));
                let native_now = browser.read_with(cx, |b, _| b.fixture_native_visible());
                report.check("takeover_band_reserved", native_now && placement.right == voice::WIDGET_MARGIN && occlusion == voice::VOICE_WIDGET_BAND, json!({ "presented_frame": Some(presented), "right": placement.right, "occlusion": occlusion, "native_visible": native_now }));
                #[cfg(target_os = "macos")]
                {
                    let widget_hits = probes.controls.map(|(x, y)| browser.read_with(cx, |b, _| b.fixture_page_hit(x as f64, y as f64)));
                    let page_hit = browser.read_with(cx, |b, _| b.fixture_page_hit(probes.page.0 as f64, probes.page.1 as f64));
                    report.check("takeover_widget_not_under_page", native_now && probes.controls_inside && probes.page_inside && widget_hits.iter().all(|hit| *hit == Some(false)) && page_hit == Some(true), json!({ "presented_frame": Some(presented), "native_visible": native_now, "probes": probes.json(), "control_points_hit_page": widget_hits, "native_error": browser.read_with(cx, |b, _| b.page.error.clone()), "page_point_hits_page": page_hit }));
                }
                #[cfg(not(target_os = "macos"))]
                report.check("takeover_probes_inside", probes.controls_inside && probes.page_inside, json!({ "probes": probes.json() }));
                shot(&mut report, cx, window, identity, &out, "12-takeover-collapsed-sidebar-widget-corner").await;
                update(cx, &|f, _, _| f.call.hovered = true)?;
                pause(cx, 200).await;
                shot(&mut report, cx, window, identity, &out, "13-takeover-widget-hover-end").await;

                phase = "native exit".into();
                // GF3: the page leaving (close or fit eviction) still
                // occupies its animated width: on every intermediate frame
                // the widget stays beside it or inside the band it leaves.
                update(cx, &|f, _, _| f.call.hovered = false)?;
                for (mode, sidebar_collapsed, capture_name) in [
                    (BrowserMode::Takeover, true, "13a-takeover-exit-midway"),
                    (BrowserMode::Pane, false, "13b-pane-exit-midway"),
                ] {
                    for remaining in [0.75_f32, 0.5, 0.25] {
                        phase = format!("{capture_name} at {remaining}");
                        // Negative acceptance: the real native page is hidden
                        // during this nonzero exit sample, which must fail.
                        let hide = negative && mode == BrowserMode::Takeover && remaining == 0.5;
                        window.update(cx, |f, _, cx| {
                            f.browser_mode = mode;
                            f.sidebar_collapsed = sidebar_collapsed;
                            f.exit = Some(remaining);
                            f.hide_native = hide;
                            cx.notify();
                        }).context("update native exit sample")?;
                        pause(cx, 250).await;
                        // Present this exit frame before every native read.
                        let presented = present_current(cx, window).context("present measured frame")?;
                        let (placement, visible_width, probes) = window
                            .update(cx, |f, w, _| (f.placement, f.page_width(f32::from(w.viewport_size().width)), f.probes(w.viewport_size())))
                            .context("read fixture window state")?;
                        let occlusion = browser.read_with(cx, |b, _| f32::from(b.bottom_occlusion()));
                        // GH3: this sample's own native visibility, required.
                        let native_now = browser.read_with(cx, |b, _| b.fixture_native_visible());
                        let beside = placement.native_band == 0.0
                            && placement.right >= visible_width + voice::WIDGET_MARGIN - 0.5;
                        let banded = placement.native_band == voice::VOICE_WIDGET_BAND
                            && occlusion == voice::VOICE_WIDGET_BAND;
                        report.check(
                            &format!("exit_{}_{:.0}_widget_clear", capture_name, remaining * 100.0),
                            native_now && visible_width > 0.0 && probes.controls_inside && probes.page_inside && (beside || banded),
                            json!({ "presented_frame": Some(presented), "native_visible": native_now, "hidden_by_negative_mode": hide, "visible_width": visible_width, "right": placement.right, "band": placement.native_band, "occlusion": occlusion, "probes": probes.json() }),
                        );
                        #[cfg(target_os = "macos")]
                        {
                            let widget_hits = probes.controls.map(|(x, y)| browser.read_with(cx, |b, _| b.fixture_page_hit(x as f64, y as f64)));
                            let page_hit = browser.read_with(cx, |b, _| b.fixture_page_hit(probes.page.0 as f64, probes.page.1 as f64));
                            report.check(
                                &format!("exit_{}_{:.0}_widget_not_under_page", capture_name, remaining * 100.0),
                                native_now && probes.controls_inside && probes.page_inside && widget_hits.iter().all(|hit| *hit == Some(false)) && page_hit == Some(true),
                                json!({ "presented_frame": Some(presented), "native_visible": native_now, "control_points_hit_page": widget_hits, "native_error": browser.read_with(cx, |b, _| b.page.error.clone()), "page_point_hits_page": page_hit, "probes": probes.json() }),
                            );
                        }
                        if remaining == 0.5 {
                            shot(&mut report, cx, window, identity, &out, capture_name).await;
                        }
                    }
                }
                update(cx, &|f, _, _| { f.exit = None; f.hide_native = false; f.sidebar_collapsed = true; })?;

                phase = "narrow window".into();
                // Narrow window with the page: the corner again, band kept.
                update(cx, &|f, _, _| { f.call.hovered = false; f.browser_mode = BrowserMode::Pane; })?;
                resize(cx, 580.0, 720.0)?;
                pause(cx, 600).await;
                let presented = present_current(cx, window).context("present measured frame")?;
                let (placement, viewport) = window
                    .update(cx, |f, w, _| (f.placement, f32::from(w.viewport_size().width)))
                    .context("read fixture window state")?;
                report.check("narrow_band_reserved", viewport <= 600.0 && placement.native_band == voice::VOICE_WIDGET_BAND, json!({ "presented_frame": Some(presented), "viewport_width": viewport, "right": placement.right, "band": placement.native_band }));
                shot(&mut report, cx, window, identity, &out, "14-narrow-widget-corner-band").await;

                phase = "end call".into();
                // End from the widget: the call is gone, nothing is cropped.
                // Through the same handler the widget's End control calls.
                let commands = window.update(cx, |_, _, cx| Fixture::commands(cx)).context("read fixture window state")?;
                gpui::AnyWindowHandle::from(window)
                    .update(cx, |_, w, cx| commands(VoiceCommand::End, w, cx))
                    .context("end fixture call")?;
                pause(cx, 300).await;
                let presented = present_current(cx, window).context("present measured frame")?;
                let (ended, placement) = window.update(cx, |f, _, _| (f.call.ended && !f.call.live, f.placement)).context("read fixture window state")?;
                let occlusion = browser.read_with(cx, |b, _| f32::from(b.bottom_occlusion()));
                report.check("end_releases_band", ended && placement.native_band == 0.0 && occlusion == 0.0, json!({ "presented_frame": Some(presented), "occlusion": occlusion }));
                shot(&mut report, cx, window, identity, &out, "15-ended-no-widget").await;

                phase = "component release".into();
                // Unmount and drop the Browser while the app still runs, so
                // the leak detector sees it released (the last window
                // closing would quit the app on Linux).
                let released = browser.downgrade();
                drop(browser);
                update(cx, &|f, _, _| {
                    f.browser_mode = BrowserMode::Hidden;
                    f.browser = None;
                })?;
                pause(cx, 300).await;
                let alive = released.upgrade().is_some();
                report.check("components_released_before_quit", !alive, json!({ "browser_alive": alive }));
                Ok(())
                }.await;
                if let Err(error) = run {
                    report.check("fixture_execution", false, json!({ "phase": phase, "error": format!("{error:#}") }));
                }
                let failed = report.failed;
                let written = serde_json::to_vec_pretty(&json!({
                    "status": if failed { "fail" } else { "pass" },
                    "platform": std::env::consts::OS,
                    "negative_mode": if close_window { Some("close-window-after-first-capture") } else if fail_init { Some("fail-native-init") } else { negative.then_some("hide-native-during-exit") },
                    "captures": report.captures,
                    "checks": report.checks,
                }))
                .map_err(anyhow::Error::from)
                .and_then(|json| Ok(std::fs::write(out.join("result.json"), json)?));
                // The report is required evidence: a run whose report could
                // not be written fails, whatever its checks said.
                let result: Result<(), String> = match written {
                    Err(error) => Err(format!(
                        "could not write {}: {error}",
                        out.join("result.json").display()
                    )),
                    Ok(()) if failed => Err(format!(
                        "fixture failed: see {}",
                        out.join("result.json").display()
                    )),
                    Ok(()) => Ok(()),
                };
                // Stored (lock released at once) for platforms whose run loop
                // returns; reported here too because on macOS `quit` calls
                // NSApplication terminate:, which never returns from run.
                *finished.lock().unwrap() = Some(result.clone());
                if let Err(reason) = result {
                    eprintln!("desktop-corrections-fixture: {reason}");
                    // GPUI's own shutdown (quit observers, windows) then a
                    // failing exit, so no runner can read a false success.
                    cx.update(|cx| cx.shutdown());
                    std::process::exit(1);
                }
                cx.update(|cx| cx.quit());
            })
            .detach();
        });
    match outcome.lock().unwrap().take() {
        Some(Ok(())) => Ok(()),
        Some(Err(reason)) => {
            eprintln!("desktop-corrections-fixture: {reason}");
            anyhow::bail!(reason)
        }
        None => anyhow::bail!(
            "fixture did not finish; no report at {}",
            result_path.display()
        ),
    }
}
