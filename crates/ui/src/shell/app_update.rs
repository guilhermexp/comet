//! Sidebar update strip for the app itself (the fork's release feed, see
//! `crates/update`). The engine's `UpdateStatus` stream says WHETHER a newer
//! release exists; this module owns how far this process got with it.
//!
//! On a macOS bundle install the download starts on its own as soon as the
//! release is seen, and the strip turns into "restart to update". Elsewhere
//! (source/managed installs) the strip is advisory and a click dismisses it.

use super::*;

pub(super) enum UpdateFlow {
    Idle,
    Downloading,
    /// Staged bundle ready to swap in — one click restarts into it.
    Ready(PathBuf),
    Failed(SharedString),
}

pub(super) struct AppUpdate {
    flow: UpdateFlow,
    task: Option<Task<()>>,
    /// Version already auto-downloaded (or attempted) in this process, so a
    /// failure waits for a click instead of looping.
    attempted: Option<String>,
    /// Version whose advisory strip the user dismissed.
    dismissed: Option<String>,
    /// Cached: `detect_install` stats `current_exe` and the strip renders per frame.
    install: zeron_update::InstallKind,
}

impl AppUpdate {
    pub(super) fn new() -> Self {
        Self {
            flow: UpdateFlow::Idle,
            task: None,
            attempted: None,
            dismissed: None,
            install: zeron_update::detect_install(),
        }
    }
}

impl Shell {
    pub(super) fn render_update_strip(
        &mut self,
        theme: &Theme,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let status = self.state.read(cx).update.clone()?;
        if !status.update_available {
            return None;
        }
        let latest = status.latest_version.clone()?;
        let desktop = self.app_update.install.supports_desktop_update();
        if !desktop && self.app_update.dismissed.as_deref() == Some(latest.as_str()) {
            return None;
        }
        if desktop
            && matches!(self.app_update.flow, UpdateFlow::Idle)
            && self.app_update.attempted.as_deref() != Some(latest.as_str())
        {
            self.app_update.attempted = Some(latest.clone());
            self.begin_update_download(cx);
        }

        let (label, clickable): (SharedString, bool) = if desktop {
            match &self.app_update.flow {
                UpdateFlow::Idle => (format!("Update available — v{latest}").into(), true),
                UpdateFlow::Downloading => (format!("Downloading v{latest}…").into(), false),
                UpdateFlow::Ready(_) => (format!("Restart to update to v{latest}").into(), true),
                UpdateFlow::Failed(message) => (format!("Update failed: {message}").into(), true),
            }
        } else {
            (
                format!("Update available — v{latest} · run `zeron update`").into(),
                true,
            )
        };
        let failed = matches!(self.app_update.flow, UpdateFlow::Failed(_));
        let tone = if failed { theme.danger } else { theme.accent };
        let (chip_bg, chip_bg_hover) = if failed {
            (theme.danger.opacity(0.14), theme.danger.opacity(0.22))
        } else {
            (theme.accent.opacity(0.12), theme.accent.opacity(0.20))
        };

        let mut strip = div()
            .id("update-strip")
            .debug_selector(|| "update-strip".into())
            .mx(px(Theme::SPACE_SM))
            .px(px(Theme::SPACE_SM))
            .py(px(6.0))
            .flex_none()
            .rounded(px(Theme::CONTROL_RADIUS))
            .bg(chip_bg)
            .flex()
            .flex_row()
            .items_center()
            .text_size(px(11.0))
            .font_weight(gpui::FontWeight::MEDIUM)
            .text_color(tone)
            .child(div().flex_1().min_w_0().truncate().child(label));
        if clickable {
            strip = strip
                .cursor_pointer()
                .hover(move |s| s.bg(chip_bg_hover))
                .on_click(cx.listener(|this, _, _, cx| this.on_update_strip_click(cx)));
        }
        Some(strip.into_any_element())
    }

    /// Idle/Failed → download; Ready → swap + relaunch; advisory → dismiss.
    fn on_update_strip_click(&mut self, cx: &mut Context<Self>) {
        if !self.app_update.install.supports_desktop_update() {
            self.app_update.dismissed = self
                .state
                .read(cx)
                .update
                .as_ref()
                .and_then(|s| s.latest_version.clone());
            cx.notify();
            return;
        }
        match std::mem::replace(&mut self.app_update.flow, UpdateFlow::Idle) {
            UpdateFlow::Idle | UpdateFlow::Failed(_) => self.begin_update_download(cx),
            UpdateFlow::Downloading => self.app_update.flow = UpdateFlow::Downloading,
            UpdateFlow::Ready(staged) => self.apply_staged_update(staged, cx),
        }
    }

    /// Fetch the manifest and stage the new bundle under the data dir (tokio —
    /// reqwest, sha256-verified); the strip flips to "restart" when done.
    fn begin_update_download(&mut self, cx: &mut Context<Self>) {
        let edge_url = self.boot.edge_url.clone();
        let data_dir = self.data_dir.clone();
        let install = self.app_update.install.clone();
        self.app_update.flow = UpdateFlow::Downloading;
        let download = Tokio::spawn(cx, async move {
            let manifest = zeron_update::fetch_latest(&edge_url).await?;
            install.stage_desktop(&edge_url, &manifest, &data_dir).await
        });
        self.app_update.task = Some(cx.spawn(async move |this, cx| {
            let outcome = match download.await {
                Ok(Ok(staged)) => Ok(staged),
                Ok(Err(err)) => Err(format!("{err:#}")),
                Err(join_err) => Err(join_err.to_string()),
            };
            this.update(cx, |shell, cx| {
                shell.app_update.flow = match outcome {
                    Ok(staged) => UpdateFlow::Ready(staged),
                    Err(message) => {
                        tracing::warn!(%message, "update download failed");
                        UpdateFlow::Failed(message.into())
                    }
                };
                cx.notify();
            })
            .ok();
        }));
        cx.notify();
    }

    /// Swap the staged bundle over the installed one, arm the detached
    /// relauncher, and quit — it reopens the app once this process is gone.
    fn apply_staged_update(&mut self, staged: PathBuf, cx: &mut Context<Self>) {
        match self.app_update.install.apply_desktop(&staged) {
            Ok(()) => cx.quit(),
            Err(err) => {
                tracing::error!(error = %err, "update apply failed");
                self.app_update.flow = UpdateFlow::Failed(format!("{err:#}").into());
                cx.notify();
            }
        }
    }
}
