//! zeron-ui — the gpui viewport. Shell, sidebar, conversation, composer, terminal,
//! diff pane.
//!
//! Design: ARCHITECTURE.md §4; animation catalog docs/research/feature-inventory.md
//! §1.12; virtualization/markdown techniques docs/research/mugen-pretext.md.
//!
//! M3a foundation:
//! - [`theme`] — always-dark monochrome theme (oklch-derived neutrals), a gpui Global;
//! - [`motion`] — the zeron animation catalog over gpui `Animation` + cubic-bezier;
//! - [`state`] — `AppState` entity + `EngineHandle` (connect-or-embed engine);
//! - [`settings`] — persisted pane widths/collapse flags;
//! - [`shell`] — sidebar + main panel + right-pane scaffold + gate;
//! - [`loaders`] — zeron pulse loader, gradient spinner, boot splash.

pub mod app_menus;
pub mod appearance;
pub mod attachments;
pub mod badges;
mod capture;
pub mod change_requests;
pub mod changes;
pub(crate) mod chat_export;
pub mod comments;
pub mod composer;
pub mod details_sidebar;
pub mod edge_fade;
pub mod file_change;
pub mod file_preview;
pub mod frost;
pub mod history;
pub mod icons;
pub mod inline_media;
#[cfg(debug_assertions)]
pub mod inspector;
pub mod links;
pub mod live_voice;
pub mod loaders;
pub mod markdown;
pub mod markdown_decor;
pub mod mermaid_preview;
pub mod motion;
pub mod notify;
pub mod pickers;
pub mod popover;
pub mod rail;
pub mod settings;
pub mod shell;
pub mod sound;
pub mod state;
pub mod syntax_cache;
pub mod terminal;
pub mod theme;
pub mod theme_library;
pub mod tool_icons;
pub mod transcript;
mod turn_steps;
pub mod typography;
mod url_chips;
pub mod workers;

use std::path::PathBuf;

use futures::StreamExt as _;
use gpui::{App, AppContext as _, Bounds, TitlebarOptions, WindowBounds, WindowOptions, px, size};

pub use state::EngineBootConfig;
pub use zeron_proto::HarnessId;

/// Everything the headed binary passes in (config/env resolution lives in
/// `apps/zeron`, not here).
#[derive(Debug, Clone)]
pub struct UiConfig {
    /// Data directory — engine stores + `ui-settings.json`.
    pub data_dir: PathBuf,
    /// Localhost IPC port: connect if an engine daemon is listening, embed if not.
    pub ipc_port: u16,
    /// Edge base URL for the embedded engine.
    pub edge_url: String,
    /// Edge bearer; `None` runs offline.
    pub edge_token: Option<String>,
    /// Workspace org override for explicit dev-mode runs.
    pub org_id: Option<String>,
    /// WorkOS client id; `Some` makes the embedded headed engine require a
    /// production session before opening identity-scoped stores.
    pub workos_client_id: Option<String>,
    /// Harness for doc-command runs until per-chat config lands (M4).
    pub default_harness: HarnessId,
    /// Conversation URL passed by the OS on a cold launch.
    pub initial_url: Option<String>,
}

impl UiConfig {
    fn boot(&self) -> EngineBootConfig {
        EngineBootConfig {
            data_dir: self.data_dir.clone(),
            ipc_port: self.ipc_port,
            edge_url: self.edge_url.clone(),
            edge_token: self.edge_token.clone(),
            org_id: self.org_id.clone(),
            workos_client_id: self.workos_client_id.clone(),
            default_harness: self.default_harness,
        }
    }
}

/// What a dock-icon reopen needs to rebuild the main window after ⌘W closed it
/// (macOS keeps the process alive with just the menu bar, like zed).
struct ReopenState {
    state: gpui::Entity<state::AppState>,
    boot: EngineBootConfig,
    workers_model: gpui::Entity<workers::model::WorkersModel>,
}

impl gpui::Global for ReopenState {}

/// Run the headed app: tokio bridge up, engine bootstrap kicked off (probe →
/// connect-or-embed), 1320×880 window (min 900×600) with [`shell::Shell`] as the
/// root view, boot splash overlaid until the engine reports ready.
pub fn run_app(config: UiConfig) {
    let app = gpui_platform::application().with_assets(icons::Assets);
    let (url_tx, mut url_rx) = futures::channel::mpsc::unbounded::<String>();
    let callback_tx = url_tx.clone();
    app.on_open_urls(move |urls| {
        for url in urls {
            let _ = callback_tx.unbounded_send(url);
        }
    });
    if let Some(url) = config.initial_url.clone() {
        let _ = url_tx.unbounded_send(url);
    }
    // Dock-icon click with no window (⌘W closed it): rebuild the main window
    // around the still-running engine — zed does the same via `on_reopen`
    // (crates/zed/src/main.rs `app.on_reopen`).
    app.on_reopen(|cx| {
        if cx.windows().is_empty()
            && let Some(reopen) = cx.try_global::<ReopenState>()
        {
            let (state, boot, workers_model) = (
                reopen.state.clone(),
                reopen.boot.clone(),
                reopen.workers_model.clone(),
            );
            open_main_window(state, boot, workers_model, cx);
        }
    });
    app.run(move |cx: &mut App| {
        // NB: pinned-rev API — `gpui_tokio::init(cx)` free function (not `Tokio::init`).
        gpui_tokio::init(cx);
        let data_dir = config.boot().data_dir.clone();
        let ui_settings = settings::UiSettings::load(&data_dir);
        settings::init(ui_settings.clone(), data_dir.clone(), cx);
        let font_availability = typography::register_fonts(cx);
        typography::init(
            ui_settings.ui_font_family.clone(),
            ui_settings.ui_font_size,
            font_availability,
            cx,
        );
        theme_library::init(data_dir, cx);
        appearance::init(
            ui_settings.appearance,
            ui_settings.theme_selection,
            ui_settings.accent,
            ui_settings.surface,
            cx,
        );
        composer::init(cx);
        terminal::panel::init(cx);
        app_menus::init(cx);
        #[cfg(debug_assertions)]
        inspector::init(cx);

        cx.register_url_scheme("zeron").detach();

        let state = cx.new(|_| state::AppState::new());
        let url_state = state.clone();
        cx.spawn(async move |cx| {
            while let Some(url) = url_rx.next().await {
                url_state.update(cx, |state, cx| state.open_deep_link(&url, cx));
            }
        })
        .detach();
        let workers_model = cx.new({
            let state = state.clone();
            move |cx| workers::model::WorkersModel::new(state, cx)
        });
        let workers_resource_monitor = cx.new({
            let workers_model = workers_model.clone();
            move |cx| workers::resource_monitor::WorkersResourceMonitor::new(workers_model, cx)
        });
        cx.set_global(workers::resource_monitor::WorkersResourceGlobal {
            monitor: workers_resource_monitor,
        });
        let workers_menu_bar = cx.new({
            let workers_model = workers_model.clone();
            move |cx| workers::menu_bar::WorkersMenuBarController::new(workers_model, cx)
        });
        cx.set_global(workers::menu_bar::WorkersMenuBarGlobal {
            controller: workers_menu_bar,
        });
        state::AppState::bootstrap(state.clone(), config.boot(), cx);

        // Graceful teardown: an in-process engine drains live runs and flushes
        // doc snapshots before the process exits (remote engines outlive us).
        let quit_state = state.clone();
        cx.on_app_quit(move |cx| {
            let task = quit_state.read(cx).engine().cloned().map(|handle| {
                let executor = cx.background_executor().clone();
                gpui_tokio::Tokio::spawn(cx, async move {
                    let _ = attachments::call_with_timeout(
                        &handle,
                        &executor,
                        zeron_rpc::methods::STOP_LIVE_VOICE,
                        serde_json::Value::Null,
                        std::time::Duration::from_secs(2),
                    )
                    .await;
                    handle.shutdown().await;
                })
            });
            async move {
                if let Some(task) = task {
                    let _ = task.await;
                }
            }
        })
        .detach();

        cx.set_global(ReopenState {
            state: state.clone(),
            boot: config.boot(),
            workers_model: workers_model.clone(),
        });
        open_main_window(state, config.boot(), workers_model, cx);
        // Native menu bar — macOS gets the standard app menu (About/Services/
        // Hide/Quit ⌘Q), Edit clipboard verbs routed to the focused input, and
        // a Window menu (⌘M/⌘W). Without this, `NSApp.mainMenu` stays nil: no
        // Cmd+Q, and nothing for the system menu bar to show. Set after
        // `open_main_window` because `Shell::new` ran `apply_keymap`
        // synchronously, so `set_menus` reads the final bindings for the ⌘-key
        // equivalents (gpui snapshots the keymap at set time).
        cx.set_menus(app_menus::app_menus());
        cx.activate(true);
    });
}

/// Open the 1320×880 main window (min 900×600) with [`shell::Shell`] as the
/// root view. Called at boot and again from `on_reopen` if the dock icon is
/// clicked after ⌘W closed the window.
fn open_main_window(
    state: gpui::Entity<state::AppState>,
    boot: EngineBootConfig,
    workers_model: gpui::Entity<workers::model::WorkersModel>,
    cx: &mut App,
) {
    let window_size = if capture::knob("ZERON_DEMO_NARROW").is_some() {
        size(px(900.), px(600.))
    } else {
        size(px(1320.), px(880.))
    };
    let bounds = Bounds::centered(None, window_size, cx);
    let shell_state = state.clone();
    let window = cx
        .open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                window_min_size: Some(size(px(900.), px(600.))),
                titlebar: Some(TitlebarOptions {
                    title: None,
                    appears_transparent: true,
                    traffic_light_position: Some(gpui::point(px(14.), px(14.))),
                }),
                app_owns_titlebar_drag: true,
                window_decorations: cfg!(target_os = "linux")
                    .then_some(gpui::WindowDecorations::Client),
                window_background: theme::Theme::of(cx).window_background_appearance(),
                app_id: Some("zeron".into()),
                ..Default::default()
            },
            move |window, cx| {
                appearance::observe_window(window, cx).detach();
                cx.new(|cx| shell::Shell::new(shell_state, boot, workers_model, cx))
            },
        )
        .expect("failed to open window");
    let window_id = window.window_id();
    cx.on_window_closed(move |cx, closed| {
        if closed == window_id {
            state.update(cx, |state, cx| {
                if state.live_voice_active() {
                    state.stop_live_voice(cx);
                }
            });
        }
    })
    .detach();
    appearance::reapply_window_background(cx);
}
