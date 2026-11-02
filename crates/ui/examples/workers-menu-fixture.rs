//! Native acceptance fixture for W2: the real Workers menu-bar status item
//! and NSPopover (`workers::menu_bar`, production `NativeMenuBar`) fed by the
//! real projector through the production menu-bar row order
//! (`MenuBarRowOrder`, the same state the controller keeps) from a synthetic
//! snapshot of Workers A/B/C. No Shell, `WorkersModel`, engine, provider or
//! audio.
//!
//! It proves, on the real AppKit views:
//! 1. bursts that reorder the snapshot by recent activity (and change title,
//!    command and an allowed status, so the popover really rebuilds) keep each
//!    Worker on the same native row, frame and tag;
//! 2. a real click — hit-test of the content view, then `performClick:` on the
//!    returned NSButton through the production target/action and bridge —
//!    reaches the receiver as the exact project/session;
//! 3. a newer Worker appends below — also one created at the same instant
//!    as an existing Worker whose id sorts first (GF13) — a real section
//!    change moves only that Worker, and the "All recent" footer stays.
//!
//! Each state is captured from the popover's own NSWindow into
//! `<out-dir>/<state>.png`; `result.json` lists captures, checks, frames and
//! the intents actually received, the status-item anchor state before the
//! show, every stage of the production popover op and every NSPopover
//! notification. The popover opens as in the product: a native click on the
//! status-item button, the bridge's TogglePopover, then the controller
//! toggle. A failure exits 1 before AppKit `terminate:` (which never returns
//! from `run`). The status item is removed
//! at the end; nothing outside this process is changed.
//!
//! cargo build --locked -p zeron-ui --features appshots-fixture --example workers-menu-fixture
//! cargo run --locked -p zeron-ui --features appshots-fixture --example workers-menu-fixture -- <out-dir>

#[cfg(not(target_os = "macos"))]
fn main() {
    eprintln!("workers-menu-fixture: the Workers menu-bar popover is AppKit-only; run it on macOS");
    std::process::exit(2);
}

#[cfg(target_os = "macos")]
fn main() -> anyhow::Result<()> {
    mac::run()
}

// Compiled on every platform so its logic type-checks anywhere; it only
// runs on macOS, where the popover exists.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
mod mac {
    use std::path::{Path, PathBuf};
    use std::sync::mpsc::{self, Receiver};
    use std::sync::{Arc, Mutex};
    use std::time::{Duration, Instant};

    use gpui::{
        AppContext, AsyncApp, Bounds, Context, IntoElement, Render, Window, WindowBounds,
        WindowOptions, div, point, px, size,
    };
    use serde_json::{Value, json};
    use zeron_ui::workers::activity_menu::{
        MenuBarRowOrder, WorkersActivityMenu, menu_popover_size,
    };
    use zeron_ui::workers::menu_bar::fixture::{AnchorState, FixtureMenuBar, NativeRow};
    use zeron_ui::workers::menu_bar::{ALL_RECENT_TAG, MenuBarIntent};
    use zeron_ui::workers::model::WorkersSessionTarget;
    use zeron_workers_unpeel::{
        WorkersBootstrap, WorkersProject, WorkersProtocol, WorkersSession,
        WorkersSessionCapabilities,
    };

    async fn pause(cx: &mut AsyncApp, ms: u64) {
        cx.background_executor()
            .timer(Duration::from_millis(ms))
            .await;
    }

    /// The whole budget for one native wait (anchor readiness, popover
    /// shown, popover closed): the same 3 s the fixture always allowed.
    const NATIVE_BUDGET: Duration = Duration::from_millis(3000);
    /// One condition check per frame while waiting on AppKit.
    const FRAME: u64 = 16;

    /// Empty activation host: a regular app with no window is not reliably
    /// made active, and a transient popover in an inactive app does not stay
    /// shown. It renders nothing and owns no product state.
    struct Host;

    impl Render for Host {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            div()
        }
    }

    /// Waits, frame by frame within `NATIVE_BUDGET`, until the status-item
    /// button is anchored on screen and the app is active. Records every
    /// distinct anchor state seen, with its time.
    async fn wait_for_anchor(cx: &mut AsyncApp, bar: &FixtureMenuBar) -> (AnchorState, Vec<Value>) {
        let started = Instant::now();
        let mut timeline: Vec<Value> = Vec::new();
        let mut last: Option<AnchorState> = None;
        loop {
            let state = bar.anchor();
            if last.as_ref() != Some(&state) {
                timeline.push(json!({ "at_ms": started.elapsed().as_millis(), "state": state }));
                last = Some(state.clone());
            }
            if (state.ready() && state.app_active) || started.elapsed() >= NATIVE_BUDGET {
                return (state, timeline);
            }
            pause(cx, FRAME).await;
        }
    }

    /// Waits within `NATIVE_BUDGET` for the popover to post a notification
    /// whose name contains `name` (e.g. `DidShow`); raw names are recorded.
    async fn wait_for_event(
        cx: &mut AsyncApp,
        bar: &FixtureMenuBar,
        name: &str,
        after: usize,
    ) -> bool {
        let started = Instant::now();
        loop {
            if bar
                .popover_events()
                .iter()
                .skip(after)
                .any(|event| event.name.contains(name))
            {
                return true;
            }
            if started.elapsed() >= NATIVE_BUDGET {
                return false;
            }
            pause(cx, FRAME).await;
        }
    }

    /// Waits within `NATIVE_BUDGET` for the bridge to send an intent,
    /// without blocking the main thread.
    async fn wait_for_intent(
        cx: &mut AsyncApp,
        receiver: &Receiver<MenuBarIntent>,
    ) -> Option<MenuBarIntent> {
        let started = Instant::now();
        loop {
            if let Ok(intent) = receiver.try_recv() {
                return Some(intent);
            }
            if started.elapsed() >= NATIVE_BUDGET {
                return None;
            }
            pause(cx, FRAME).await;
        }
    }

    fn project(id: &str, name: &str) -> WorkersProject {
        WorkersProject {
            id: id.to_owned(),
            name: name.to_owned(),
            path: format!("/tmp/{id}"),
            folder_id: None,
            parent_project_id: None,
            is_group: false,
            worktree_branch: None,
            git_branch: None,
            archived_session_count: 0,
            folder_color_id: None,
            session_sort: Default::default(),
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

    #[derive(Clone)]
    struct Worker {
        id: &'static str,
        project: &'static str,
        title: String,
        command: &'static str,
        activity: &'static str,
        created: u64,
        updated: u64,
    }

    impl Worker {
        fn new(id: &'static str, project: &'static str, created: u64) -> Self {
            Self {
                id,
                project,
                title: format!("{id} · task"),
                command: "claude",
                activity: "working",
                created,
                updated: created,
            }
        }

        fn session(&self) -> WorkersSession {
            WorkersSession {
                id: self.id.to_owned(),
                project_id: self.project.to_owned(),
                title: self.title.clone(),
                command: self.command.to_owned(),
                state: "running".to_owned(),
                activity: self.activity.to_owned(),
                unread: false,
                pinned: false,
                archived: false,
                provider_id: Some("claude".to_owned()),
                active_runtime_id: Some("claude".to_owned()),
                runtime_launch_pending: false,
                runtime_generation: 1,
                notify_when_done: false,
                terminal_background_hex: None,
                worktree_branch: None,
                created_at_unix_ms: self.created,
                updated_at_unix_ms: self.updated,
                idle_since_unix_ms: None,
                idle_confirmed_by_hook: false,
                resumable_conversation: false,
                total_tokens: None,
                model_usage: Vec::new(),
                capabilities: WorkersSessionCapabilities::default(),
            }
        }
    }

    /// The snapshot arrives ordered by recent activity, like the sidebar,
    /// and goes through the popover's retained row order, as in production.
    fn menu(order: &mut MenuBarRowOrder, workers: &[Worker]) -> WorkersActivityMenu {
        let mut sessions = workers.iter().map(Worker::session).collect::<Vec<_>>();
        sessions.sort_by_key(|session| std::cmp::Reverse(session.updated_at_unix_ms));
        order.arrange(&WorkersBootstrap {
            mac_name: "Fixture Mac".to_owned(),
            protocol: WorkersProtocol {
                major_version: 1,
                minor_version: 0,
                capabilities: Vec::new(),
            },
            projects: vec![project("p-alpha", "Alpha"), project("p-beta", "Beta")],
            presets: Vec::new(),
            sessions,
            activity_log: Vec::new(),
        })
    }

    /// A session row as observed natively: which Worker (from its title
    /// label), its tag and its frame measured from the popover's top.
    #[derive(Clone, Debug, PartialEq)]
    struct Observed {
        worker: String,
        tag: i64,
        title: String,
        from_top: [f64; 4],
    }

    fn session_rows(rows: &[NativeRow], height: f64) -> Vec<Observed> {
        let mut observed = rows
            .iter()
            .filter(|row| row.tag != ALL_RECENT_TAG)
            .map(|row| {
                let [x, y, w, h] = row.frame;
                Observed {
                    worker: row.title.split(" · ").next().unwrap_or_default().to_owned(),
                    tag: row.tag,
                    title: row.title.clone(),
                    from_top: [x, height - y - h, w, h],
                }
            })
            .collect::<Vec<_>>();
        observed.sort_by(|a, b| a.from_top[1].total_cmp(&b.from_top[1]));
        observed
    }

    fn order(rows: &[Observed]) -> Vec<String> {
        rows.iter().map(|row| row.worker.clone()).collect()
    }

    fn observed_json(rows: &[Observed]) -> Value {
        json!(
            rows.iter()
                .map(|row| json!({
                    "worker": row.worker,
                    "tag": row.tag,
                    "title": row.title,
                    "frame_from_top": row.from_top,
                }))
                .collect::<Vec<_>>()
        )
    }

    #[derive(Default)]
    struct Report {
        captures: Vec<Value>,
        checks: Vec<Value>,
        intents: Vec<Value>,
        anchor_before_show: Value,
        failed: bool,
    }

    impl Report {
        fn check(&mut self, name: &str, ok: bool, detail: Value) {
            self.failed |= !ok;
            self.checks
                .push(json!({ "check": name, "ok": ok, "detail": detail }));
        }

        async fn shot(&mut self, cx: &mut AsyncApp, bar: &FixtureMenuBar, out: &Path, name: &str) {
            let result = match bar.popover_window_number() {
                Some(number) => {
                    let path = out.join(format!("{name}.png"));
                    // Keep AppKit free to commit the newly shown window while
                    // screencapture waits for its WindowServer image.
                    cx.background_executor()
                        .spawn(async move { capture(number, &path) })
                        .await
                }
                None => Err(anyhow::anyhow!("the popover has no window")),
            };
            self.failed |= result.is_err();
            self.captures.push(json!({
                "state": name,
                "captured": result.is_ok(),
                "popover_shown": bar.is_shown(),
                "popover_window_number": bar.popover_window_number(),
                "anchor": bar.anchor(),
                "error": result.err().map(|error| error.to_string()),
            }));
        }
    }

    /// The popover's own window, from the window server, rejected if empty
    /// or uniform.
    fn capture(number: i64, path: &Path) -> anyhow::Result<()> {
        let status = std::process::Command::new("/usr/sbin/screencapture")
            .args(["-x", "-o", "-l", &number.to_string()])
            .arg(path)
            .status()?;
        anyhow::ensure!(status.success(), "screencapture failed for window {number}");
        let image = image::open(path)?.to_rgb8();
        let first = image.pixels().next().copied();
        anyhow::ensure!(
            image.pixels().any(|pixel| Some(*pixel) != first),
            "capture of window {number} is a uniform frame"
        );
        Ok(())
    }

    /// Clicks the centre of `worker`'s native row and reads what the real
    /// bridge sent.
    async fn click_worker(
        cx: &mut AsyncApp,
        bar: &FixtureMenuBar,
        receiver: &Receiver<MenuBarIntent>,
        rows: &[NativeRow],
        height: f64,
        worker: &str,
        expected: WorkersSessionTarget,
        report: &mut Report,
        label: &str,
    ) {
        let observed = session_rows(rows, height);
        let Some(row) = observed.iter().find(|row| row.worker == worker) else {
            report.check(
                label,
                false,
                json!({ "error": format!("no native row for {worker}") }),
            );
            return;
        };
        let native = rows.iter().find(|native| native.tag == row.tag).unwrap();
        let [x, y, w, h] = native.frame;
        let point = (x + w / 2.0, y + h / 2.0);
        while receiver.try_recv().is_ok() {}
        let hit = bar.click_at(point.0, point.1);
        let intent = wait_for_intent(cx, receiver).await;
        let received = match &intent {
            Some(MenuBarIntent::SelectSession(target)) => {
                json!({ "select_session": { "project_id": target.project_id, "session_id": target.session_id } })
            }
            Some(other) => json!(format!("{other:?}")),
            None => Value::Null,
        };
        report.intents.push(json!({
            "step": label,
            "worker": worker,
            "click_point": [point.0, point.1],
            "hit_tag": hit,
            "received": received,
        }));
        let ok =
            hit == Some(row.tag) && intent == Some(MenuBarIntent::SelectSession(expected.clone()));
        report.check(
            label,
            ok,
            json!({
                "expected": { "project_id": expected.project_id, "session_id": expected.session_id },
                "hit_tag": hit,
                "row_tag": row.tag,
                "received": received,
            }),
        );
    }

    pub fn run() -> anyhow::Result<()> {
        let out: PathBuf = std::env::args()
            .nth(1)
            .map(PathBuf::from)
            .ok_or_else(|| anyhow::anyhow!("usage: workers-menu-fixture <out-dir>"))?;
        std::fs::create_dir_all(&out)?;
        let result_path = out.join("result.json");
        let outcome: Arc<Mutex<Option<Result<(), String>>>> = Arc::default();
        let finished = outcome.clone();
        gpui_platform::application().run(move |cx| {
            let out = out.clone();
            cx.spawn(async move |cx| {
                let mut report = Report::default();
                let (sender, receiver) = mpsc::channel::<MenuBarIntent>();
                let mut workers = vec![
                    Worker::new("A", "p-alpha", 10),
                    Worker::new("B", "p-beta", 20),
                    Worker::new("C", "p-alpha", 30),
                ];
                let mut row_order = MenuBarRowOrder::default();
                let first = menu(&mut row_order, &workers);

                // 0. Activation host only (no Shell, model, engine, provider).
                let host = cx.open_window(
                    WindowOptions {
                        window_bounds: Some(WindowBounds::Windowed(Bounds::new(
                            point(px(40.), px(40.)),
                            size(px(220.), px(60.)),
                        ))),
                        ..Default::default()
                    },
                    |_, cx| cx.new(|_| Host),
                );
                report.check(
                    "activation_host_window",
                    host.is_ok(),
                    json!({ "error": host.as_ref().err().map(|error| error.to_string()) }),
                );
                cx.update(|cx| cx.activate(true));

                let mut bar = FixtureMenuBar::new(sender, &first);
                bar.observe_popover();

                // 1. Wait for the native anchor, then open the real popover
                // the way the product does: a native click on the status-item
                // button, the bridge's own intent, then the controller's toggle.
                let (anchor, anchor_timeline) = wait_for_anchor(cx, &bar).await;
                let anchor_ready = anchor.ready() && anchor.app_active;
                report.check(
                    "status_item_anchor_ready",
                    anchor_ready,
                    json!({ "anchor": anchor, "app_active": anchor.app_active }),
                );
                report.anchor_before_show = json!({ "final": anchor, "timeline": anchor_timeline });
                let mut shown = false;
                if anchor_ready {
                    while receiver.try_recv().is_ok() {}
                    let clicked = bar.click_status_button();
                    let intent = wait_for_intent(cx, &receiver).await;
                    let toggle_emitted = matches!(intent, Some(MenuBarIntent::TogglePopover));
                    report.check(
                        "status_button_click_emits_toggle",
                        clicked && toggle_emitted,
                        json!({
                            "clicked": clicked,
                            "received": intent.as_ref().map(|intent| format!("{intent:?}")),
                        }),
                    );
                    if toggle_emitted {
                        // What `WorkersMenuBarController` does with that intent.
                        bar.toggle();
                        let did_show = wait_for_event(cx, &bar, "DidShow", 0).await;
                        shown = did_show && bar.is_shown();
                    }
                }
                report.check(
                    "popover_shown",
                    shown,
                    json!({
                        "is_shown": bar.is_shown(),
                        "popover_window_number": bar.popover_window_number(),
                        "events": bar.popover_events(),
                        "ops": bar.popover_ops(),
                        "anchor_after_show": bar.anchor(),
                    }),
                );
                let height = menu_popover_size(&first).1;
                let baseline = session_rows(&bar.rows(), height);
                report.check(
                    "initial_creation_order",
                    order(&baseline) == ["A", "B", "C"],
                    json!({ "rows": observed_json(&baseline) }),
                );
                report.shot(cx, &bar, &out, "01-initial-popover").await;

                // 2. Bursts: recent-activity reorders plus title, command and
                // an allowed status change, so every update really rebuilds.
                let bursts: [[u64; 3]; 4] =
                    [[900, 200, 300], [100, 950, 300], [100, 200, 999], [50, 40, 30]];
                for (index, updates) in bursts.iter().enumerate() {
                    for (worker, updated) in workers.iter_mut().zip(updates) {
                        worker.updated = *updated;
                        worker.title = format!("{} · burst {}", worker.id, index + 1);
                        worker.command = if index % 2 == 0 { "claude --continue" } else { "claude" };
                        worker.activity = if index % 2 == 0 { "starting" } else { "working" };
                    }
                    let next = menu(&mut row_order, &workers);
                    bar.update(&next);
                    pause(cx, 250).await;
                    let rows = session_rows(&bar.rows(), menu_popover_size(&next).1);
                    let refreshed = rows
                        .iter()
                        .all(|row| row.title.ends_with(&format!("burst {}", index + 1)));
                    let same_rows = rows.len() == baseline.len()
                        && rows.iter().zip(&baseline).all(|(now, before)| {
                            now.worker == before.worker
                                && now.tag == before.tag
                                && now.from_top == before.from_top
                        });
                    let recent_order = {
                        let mut ids = workers.clone();
                        ids.sort_by_key(|w| std::cmp::Reverse(w.updated));
                        ids.iter().map(|w| w.id).collect::<Vec<_>>()
                    };
                    report.check(
                        &format!("burst_{}_rows_and_frames_unchanged", index + 1),
                        refreshed && same_rows && bar.is_shown(),
                        json!({
                            "snapshot_order_by_recent": recent_order,
                            "refreshed_titles": refreshed,
                            "rows": observed_json(&rows),
                        }),
                    );
                }
                report.shot(cx, &bar, &out, "02-after-bursts").await;

                // 3. Real clicks on the same place, through the real bridge.
                let rows = bar.rows();
                let current = menu_popover_size(&menu(&mut row_order, &workers)).1;
                if std::env::var("ZERON_FIXTURE_NEGATIVE").as_deref() == Ok("no-worker-intent") {
                    let (_silent_sender, silent_receiver) = mpsc::channel();
                    let responsive = std::rc::Rc::new(std::cell::Cell::new(false));
                    let tick = responsive.clone();
                    cx.spawn(async move |cx| {
                        pause(cx, FRAME).await;
                        tick.set(true);
                    }).detach();
                    click_worker(cx, &bar, &silent_receiver, &rows, current, "B",
                        WorkersSessionTarget::new("p-beta", "B"), &mut report, "click_B_after_bursts").await;
                    report.check("no_intent_keeps_main_thread_responsive", responsive.get(), json!({"timer_ran_during_wait": responsive.get()}));
                } else {
                click_worker(cx, &bar, &receiver, &rows, current, "B",
                    WorkersSessionTarget::new("p-beta", "B"), &mut report, "click_B_after_bursts").await;
                }
                click_worker(cx, &bar, &receiver, &rows, current, "C",
                    WorkersSessionTarget::new("p-alpha", "C"), &mut report, "click_C_after_bursts").await;

                // 4a. A newer Worker appends; the others keep their place.
                workers.push(Worker::new("D", "p-beta", 40));
                workers.last_mut().unwrap().updated = 10_000;
                let appended = menu(&mut row_order, &workers);
                bar.update(&appended);
                pause(cx, 250).await;
                let appended_height = menu_popover_size(&appended).1;
                let rows = bar.rows();
                let observed = session_rows(&rows, appended_height);
                let kept = observed.iter().take(3).zip(&baseline).all(|(now, before)| {
                    now.worker == before.worker && now.from_top == before.from_top
                });
                report.check(
                    "new_worker_appends_below",
                    order(&observed) == ["A", "B", "C", "D"] && kept,
                    json!({ "rows": observed_json(&observed) }),
                );
                report.shot(cx, &bar, &out, "03-new-worker-appended").await;
                click_worker(cx, &bar, &receiver, &rows, appended_height, "D",
                    WorkersSessionTarget::new("p-beta", "D"), &mut report, "click_D_appended").await;

                // 4a'. GF13: a Worker created at the same instant as D, whose
                // id sorts before every other, still appends below.
                workers.push(Worker::new("0", "p-alpha", 40));
                workers.last_mut().unwrap().updated = 20_000;
                let same_time = menu(&mut row_order, &workers);
                bar.update(&same_time);
                pause(cx, 250).await;
                let same_time_height = menu_popover_size(&same_time).1;
                let rows = bar.rows();
                let observed_same_time = session_rows(&rows, same_time_height);
                let kept = observed_same_time.iter().take(4).zip(&observed).all(|(now, before)| {
                    now.worker == before.worker && now.tag == before.tag && now.from_top == before.from_top
                });
                report.check(
                    "equal_time_new_worker_appends_below",
                    order(&observed_same_time) == ["A", "B", "C", "D", "0"] && kept,
                    json!({ "rows": observed_json(&observed_same_time) }),
                );
                click_worker(cx, &bar, &receiver, &rows, same_time_height, "0",
                    WorkersSessionTarget::new("p-alpha", "0"), &mut report, "click_equal_time_appended").await;

                // 4b. A real section change (B now needs input) moves only B.
                let footer_before = rows.iter().find(|row| row.tag == ALL_RECENT_TAG).cloned();
                workers.iter_mut().find(|w| w.id == "B").unwrap().activity = "blocked";
                let transitioned = menu(&mut row_order, &workers);
                bar.update(&transitioned);
                pause(cx, 250).await;
                let transitioned_height = menu_popover_size(&transitioned).1;
                let rows = bar.rows();
                let observed = session_rows(&rows, transitioned_height);
                let footer_after = rows.iter().find(|row| row.tag == ALL_RECENT_TAG).cloned();
                report.check(
                    "section_change_moves_only_that_worker",
                    order(&observed) == ["B", "A", "C", "D", "0"]
                        && transitioned.blockers.len() == 1
                        && transitioned.jobs.len() == 4,
                    json!({ "rows": observed_json(&observed) }),
                );
                report.check(
                    "all_recent_footer_unchanged",
                    footer_before.as_ref().map(|row| (&row.title, row.frame))
                        == footer_after.as_ref().map(|row| (&row.title, row.frame))
                        && footer_after.is_some(),
                    json!({
                        "before": footer_before.map(|row| json!({ "title": row.title, "frame": row.frame })),
                        "after": footer_after.map(|row| json!({ "title": row.title, "frame": row.frame })),
                    }),
                );
                report.shot(cx, &bar, &out, "04-section-transition").await;
                click_worker(cx, &bar, &receiver, &rows, transitioned_height, "B",
                    WorkersSessionTarget::new("p-beta", "B"), &mut report, "click_B_after_transition").await;
                click_worker(cx, &bar, &receiver, &rows, transitioned_height, "C",
                    WorkersSessionTarget::new("p-alpha", "C"), &mut report, "click_C_after_transition").await;

                // Close may already have been posted by AppKit on deactivation.
                bar.close();
                let did_close = wait_for_event(cx, &bar, "DidClose", 0).await;
                report.check(
                    "popover_closed",
                    !bar.is_shown() && (did_close || !shown),
                    json!({ "did_close_notification": did_close }),
                );
                let popover_events = bar.popover_events();
                let popover_ops = bar.popover_ops();
                drop(bar);
                pause(cx, 200).await;

                let failed = report.failed;
                let written = serde_json::to_vec_pretty(&json!({
                    "status": if failed { "fail" } else { "pass" },
                    "platform": std::env::consts::OS,
                    "captures": report.captures,
                    "checks": report.checks,
                    "intents": report.intents,
                    "anchor_before_show": report.anchor_before_show,
                    "popover_events": popover_events,
                    "popover_ops": popover_ops,
                }))
                .map_err(anyhow::Error::from)
                .and_then(|bytes| Ok(std::fs::write(out.join("result.json"), bytes)?));
                let result: Result<(), String> = match written {
                    Err(error) => Err(format!("could not write {}: {error}", out.join("result.json").display())),
                    Ok(()) if failed => Err(format!("fixture failed: see {}", out.join("result.json").display())),
                    Ok(()) => Ok(()),
                };
                *finished.lock().unwrap() = Some(result.clone());
                // macOS `quit` is NSApplication terminate:, which never
                // returns from run: report a failure before it.
                if let Err(reason) = result {
                    eprintln!("workers-menu-fixture: {reason}");
                    cx.update(|cx| cx.shutdown());
                    std::process::exit(1);
                }
                cx.update(|cx| cx.quit());
            })
            .detach();
        });
        match outcome.lock().unwrap().take() {
            Some(Ok(())) => Ok(()),
            Some(Err(reason)) => anyhow::bail!(reason),
            None => anyhow::bail!(
                "fixture did not finish; no report at {}",
                result_path.display()
            ),
        }
    }
}
