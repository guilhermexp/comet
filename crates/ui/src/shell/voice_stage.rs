//! The voice orchestrator's chrome: the sidebar footer trigger and the
//! full-window stage. A voice session follows no chat selection, so neither
//! surface lives inside the conversation column — the user moves between
//! threads freely while the orchestrator keeps listening.

use super::*;
use zeron_proto::voice::{VoicePhase, VoiceWork};

// Geometry and presenters are shared with the hermetic review fixture.
use crate::voice_surfaces::VOICE_STAGE_ORB_SCALE;
use crate::voice_surfaces::{
    VoiceCallBarProps, VoiceCommand, VoiceCommands, VoiceStageProps, VoiceWidgetPlacement,
    VoiceWidgetProps, tooltip as stage_tooltip,
};
const STAGE_ENTER_MS: f32 = 460.0;
const STAGE_EXIT_MS: f32 = 220.0;
/// Captions show the tail of a long utterance rather than wrapping off-stage.
pub(super) const STAGE_CAPTION_CHARS: usize = 160;
const FOOTER_HOVER: &str = "voice-footer-orb";

/// Which voice-orb presentations are on screen: the sidebar trigger, the
/// full stage and the compact widget. One predicate feeds both the
/// presenters and the per-frame sync, so a hidden orb never keeps ticking.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct VoiceOrbsShown {
    pub footer: bool,
    pub stage: bool,
    pub widget: bool,
}

/// Stage visibility for a transition that flipped `elapsed_ms` ago.
pub(super) fn stage_reveal(open: bool, elapsed_ms: f32, reduced: bool) -> f32 {
    match (open, reduced) {
        (true, true) => 1.0,
        (false, true) => 0.0,
        (true, false) => motion::EASE_OUT_EXPO.eval(elapsed_ms / STAGE_ENTER_MS),
        (false, false) => 1.0 - motion::EASE.eval(elapsed_ms / STAGE_EXIT_MS),
    }
}

impl Shell {
    /// The footer microphone: a new orchestrator in a projectless Codex chat
    /// on this device. The chat's model follows the composer when it already
    /// targets Codex; otherwise Codex's own default model runs the delegations.
    pub(super) fn start_voice(&mut self, cx: &mut Context<Self>) {
        let state = self.state.read(cx);
        let (Some(engine), Some(device)) = (state.engine().cloned(), state.local_device_id.clone())
        else {
            return;
        };
        let config = self
            .composer
            .read(cx)
            .pickers()
            .read(cx)
            .resolved(cx)
            .chat_config()
            .filter(|config| config.harness == HarnessId::Codex)
            .unwrap_or_else(|| ChatConfig {
                harness: HarnessId::Codex,
                model: None,
                reasoning: None,
                model_options: Default::default(),
                sandbox: zeron_proto::SandboxLevel::WorkspaceWrite,
            });
        let saved = settings::current(cx);
        let device = saved.codex_voice_device.unwrap_or(device);
        let voice = saved.codex_voice;
        let host_name = state
            .device_name(&device)
            .unwrap_or("This device")
            .to_owned();
        self.voice.update(cx, |controller, cx| {
            controller.start(engine, device, config, voice, cx);
            controller.host_name = Some(host_name);
            controller.set_stage_open(true, cx);
        });
    }

    /// Read whether this device's Codex can take a call; `force` re-reads it
    /// after Settings → Agents changes or on coming back to a window that
    /// hides voice (Codex may have been installed meanwhile).
    pub(super) fn check_voice_codex(&mut self, force: bool, cx: &mut Context<Self>) {
        let Some(engine) = self.state.read(cx).engine().cloned() else {
            return;
        };
        self.voice
            .update(cx, |voice, cx| voice.check_codex(&engine, force, cx));
    }

    pub(super) fn end_voice(&mut self, cx: &mut Context<Self>) {
        self.voice.update(cx, |voice, cx| voice.cancel(cx));
    }

    pub(super) fn set_voice_stage_open(&mut self, open: bool, cx: &mut Context<Self>) {
        self.voice
            .update(cx, |voice, cx| voice.set_stage_open(open, cx));
    }

    /// The stage covers the page, including its exit fade.
    pub(super) fn voice_stage_presented(&self, cx: &App) -> bool {
        self.voice_stage_reveal(cx) > 0.001
    }

    /// The compact call widget: a live call whose stage is not presented.
    pub(super) fn voice_widget_shown(&self, cx: &App) -> bool {
        self.voice.read(cx).is_live() && !self.voice_stage_presented(cx)
    }

    pub(super) fn voice_orbs_shown(&self, cx: &App) -> VoiceOrbsShown {
        if !settings::current(cx).voice_orb_enabled {
            return VoiceOrbsShown {
                footer: false,
                stage: false,
                widget: false,
            };
        }
        let voice = self.voice.read(cx);
        // The trigger lives in the orchestrator sidebar footer (and the
        // Settings page's own footer); a collapsed sidebar shows neither.
        let footer_displayed = match self.route {
            Route::Settings(_) => self.sidebar_mode == SidebarMode::Orchestrator,
            _ => self.sidebar_mode.shows_orchestrator_content() && self.sidebar_now() > 0.5,
        };
        VoiceOrbsShown {
            footer: voice.is_live() && footer_displayed,
            stage: voice.stage_open,
            widget: self.voice_widget_shown(cx),
        }
    }

    /// Hide every orb whose presentation is not on screen this frame —
    /// collapsed sidebar, closed stage, preference off — so retained
    /// entities drop their animation timers even when no presenter runs.
    pub(super) fn sync_voice_orbs(&mut self, cx: &mut Context<Self>) {
        let shown = self.voice_orbs_shown(cx);
        for (orb, visible) in [
            (self.voice_footer_orb.clone(), shown.footer),
            (self.voice_stage_orb.clone(), shown.stage),
            (self.voice_widget_orb.clone(), shown.widget),
        ] {
            orb.update(cx, |orb, cx| orb.set_visible(visible, cx));
        }
    }

    /// Track stage open/close flips for the transition clock and hand focus
    /// over: the stage owns the keyboard while it is up. Navigating to
    /// another thread or to Settings steps the stage aside; voice continues.
    pub(super) fn sync_voice_stage(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let selected = self.state.read(cx).selected_chat.clone();
        let navigating_to_settings =
            self.route != self.voice_stage_route && matches!(self.route, Route::Settings(_));
        // Record every render, even with no visibility change: leaving and
        // reentering the same settings page must count as navigation too.
        self.voice_stage_route = self.route;
        if self.voice.read(cx).stage_open
            && self.voice_stage_was_open
            && (navigating_to_settings || selected != self.voice_stage_selection)
        {
            self.set_voice_stage_open(false, cx);
        }
        let open = self.voice.read(cx).stage_open;
        if open == self.voice_stage_was_open {
            return;
        }
        self.voice_stage_was_open = open;
        self.voice_stage_selection = selected;
        self.voice_stage_changed_at = Some(std::time::Instant::now());
        let focus = if open {
            self.voice_stage_focus.clone()
        } else {
            self.composer.focus_handle(cx)
        };
        if window.is_window_active() {
            window.focus(&focus, cx);
        }
    }

    pub(super) fn render_voice_trigger(
        &mut self,
        theme: &Theme,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        self.check_voice_codex(false, cx);
        let voice = self.voice.read(cx);
        // Without Codex on this device a call could only fail.
        if !voice.offered() {
            return None;
        }
        let live = voice.is_live();
        let stage_open = voice.stage_open;
        let failure = (voice.phase == VoicePhase::Failed)
            .then(|| voice.reason_text())
            .filter(|text| !text.is_empty());
        let (orb_state, microphone, speaker) = (
            voice.orb_state(),
            voice.microphone_level(),
            voice.speaker_level(),
        );
        let reduced = self.reduced_motion;
        let orb_shown = self.voice_orbs_shown(cx).footer;
        self.voice_footer_orb.update(cx, |orb, cx| {
            orb.set_visible(orb_shown, cx);
            orb.set_state(orb_state, cx);
            orb.set_audio_levels(microphone, speaker, cx);
            orb.set_reduced_motion(reduced, cx);
        });

        let button = div()
            .id("voice-trigger")
            .debug_selector(|| "voice-trigger".into())
            .role(gpui::Role::Button)
            .tab_index(0)
            .relative()
            .size(px(SIDEBAR_FOOTER_BUTTON_SIZE))
            .flex_none()
            .rounded(px(8.0))
            .flex()
            .items_center()
            .justify_center()
            .cursor_pointer()
            .bg(motion::hover_blend(
                FOOTER_HOVER,
                theme.glass_hover().opacity(0.0),
                theme.glass_hover(),
            ))
            .on_hover(motion::hover_listener(FOOTER_HOVER))
            .focus_visible(|s| s.border_2().border_color(theme.accent));

        if live {
            return button
                .aria_label(if stage_open {
                    "Hide voice"
                } else {
                    "Open voice"
                })
                .tooltip(stage_tooltip(if stage_open {
                    "Hide voice · Esc"
                } else {
                    "Voice is live · open"
                }))
                .on_click(
                    cx.listener(move |this, _, _, cx| this.set_voice_stage_open(!stage_open, cx)),
                )
                .child(if orb_shown {
                    self.voice_footer_orb.clone().into_any_element()
                } else {
                    // Orbs off: a static, identified way back to the call.
                    icon(icons::MICROPHONE)
                        .size(px(15.0))
                        .text_color(theme.success)
                        .into_any_element()
                })
                .into_any_element()
                .into();
        }

        let tooltip: SharedString = failure
            .map(SharedString::from)
            .unwrap_or_else(|| "Start voice · Codex orchestrator".into());
        let button = button
            .aria_label("Start voice")
            .tooltip(move |_, cx| {
                let text = tooltip.clone();
                cx.new(|_| SurfaceTabTooltip { text }).into()
            })
            .on_click(cx.listener(|this, _, _, cx| this.start_voice(cx)))
            .child(
                icon(icons::MICROPHONE)
                    .size(px(15.0))
                    .text_color(motion::hover_blend(
                        FOOTER_HOVER,
                        theme.text_muted,
                        theme.text,
                    )),
            );
        let Some(failure) = failure else {
            return Some(button.into_any_element());
        };
        // A failed start explains itself once, anchored to the microphone;
        // the next press retries.
        let popup_theme = theme.for_popup();
        let card = popover::popover_card(&popup_theme)
            .w(px(232.0))
            .flex()
            .flex_row()
            .items_start()
            .gap(px(8.0))
            .child(
                icon(icons::DANGER_TRIANGLE)
                    .mt(px(1.0))
                    .size(px(14.0))
                    .text_color(popup_theme.warning),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .text_size(crate::typography::ui_rems(12.0))
                    .text_color(popup_theme.text_muted)
                    .child(SharedString::from(failure)),
            )
            .child(
                div()
                    .id("voice-failure-dismiss")
                    .flex_none()
                    .cursor_pointer()
                    .rounded(px(4.0))
                    .on_click(cx.listener(|this, _, _, cx| {
                        cx.stop_propagation();
                        this.voice.update(cx, |voice, cx| voice.dismiss_reason(cx));
                    }))
                    .role(gpui::Role::Button)
                    .aria_label("Dismiss")
                    .tooltip(stage_tooltip("Dismiss"))
                    .child(
                        icon(icons::CLOSE)
                            .size(px(14.0))
                            .text_color(popup_theme.text_faint),
                    ),
            )
            .into_any_element();
        button
            .child(
                div()
                    .absolute()
                    .top(px(4.0))
                    .right(px(4.0))
                    .size(px(5.0))
                    .rounded_full()
                    .bg(theme.warning),
            )
            .child(popover::anchored_menu_above_end(
                "voice-failure",
                card,
                None,
            ))
            .into_any_element()
            .into()
    }

    /// How far the stage has arrived: 0 hidden, 1 fully covering the page.
    pub(super) fn voice_stage_reveal(&self, cx: &App) -> f32 {
        let elapsed = self
            .voice_stage_changed_at
            .map_or(f32::INFINITY, |at| at.elapsed().as_secs_f32() * 1000.0);
        stage_reveal(self.voice.read(cx).stage_open, elapsed, self.reduced_motion)
    }

    /// Opacity for the page the stage covers. A glass stage has no fill, so
    /// what it covers fades out and only the window frost shows through.
    pub(super) fn voice_stage_underlay_opacity(&self, cx: &App) -> f32 {
        if Theme::of(cx).is_glass() {
            1.0 - self.voice_stage_reveal(cx)
        } else {
            1.0
        }
    }

    /// Full-window stage: the session's orb over the new-thread hero artwork,
    /// live caption, and the session controls. Escape returns to the chats
    /// without ending voice.
    pub(super) fn render_voice_stage(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let reveal = self.voice_stage_reveal(cx);
        if reveal > 0.0 && reveal < 1.0 {
            self.motion_active.set(true);
        }
        let voice = self.voice.read(cx);
        let (orb_state, microphone, speaker) = (
            voice.orb_state(),
            voice.microphone_level(),
            voice.speaker_level(),
        );
        let (caption_item, partial) = (
            voice.caption_item().map(str::to_owned),
            voice.caption.text().to_owned(),
        );
        let snapshot = voice.snapshot.clone();
        let chat_id = voice.chat_id.clone();
        let awaiting = snapshot
            .as_ref()
            .is_some_and(|s| s.work == VoiceWork::AwaitingInput);
        let reduced = self.reduced_motion;
        let orb_shown = self.voice_orbs_shown(cx).stage;
        self.voice_stage_orb.update(cx, |orb, cx| {
            orb.set_visible(orb_shown, cx);
            orb.set_state(orb_state, cx);
            orb.set_audio_levels(microphone, speaker, cx);
            orb.set_reduced_motion(reduced, cx);
            // The orb blooms out of the footer as the stage arrives.
            orb.set_scale(VOICE_STAGE_ORB_SCALE * (0.82 + 0.18 * reveal), cx);
        });
        if reveal <= 0.001 {
            return None;
        }

        // Streamed words veil in like transcript text; a new speaker turn
        // fades the previous caption out (zeron-veil, shared with mobile).
        let caption = self.voice_caption.advance(
            caption_item.as_deref(),
            &partial,
            std::time::Instant::now(),
            self.reduced_motion,
        );
        if self.voice_caption.is_animating() {
            self.motion_active.set(true);
        }

        let theme = Theme::of(cx).clone();
        let viewport = window.viewport_size();
        // The sidebar stays usable beside the stage; collapsed, the stage
        // spans the whole window. Settings always shows its own sidebar.
        let left = if matches!(self.route, Route::Settings(_)) {
            self.settings.sidebar_width
        } else {
            self.sidebar_now()
        };
        let width = (f32::from(viewport.width) - left).max(0.0);
        let ui_settings = settings::current(cx);
        let artwork = ui_settings
            .new_thread_composer_background
            .as_ref()
            .and_then(|background| {
                crate::new_thread_background_effects::prepare(
                    ui_settings.new_thread_background_effect,
                    &theme,
                    std::path::Path::new(&background.path),
                    cx,
                )
            });
        let adjustment = ui_settings
            .new_thread_composer_background
            .as_ref()
            .map(|background| background.adjustment)
            .unwrap_or_default();
        // The new-thread hero's artwork with only its soft bottom fade: no
        // cutout around the orb, so no light channel splits the picture.
        let hero = artwork.map(|artwork| {
            div()
                .absolute()
                .top_0()
                .left_0()
                .w(px(width))
                .h(px(new_thread_background_height(f32::from(viewport.height))))
                .opacity(new_thread_background_opacity(theme.is_frost()))
                .child(
                    gpui::canvas(
                        |_, _, _| {},
                        move |bounds, _, window, _| {
                            crate::new_thread_background_mask::paint(
                                artwork.clone(),
                                bounds,
                                bounds,
                                adjustment,
                                false,
                                window,
                            );
                        },
                    )
                    .absolute()
                    .inset_0(),
                )
        });

        // Orbs off: no orb and no orb-sized placeholder; the caption and
        // the call bar carry the stage on their own.
        let orb = settings::current(cx)
            .voice_orb_enabled
            .then(|| self.voice_stage_orb.clone().into_any_element());
        let commands = self.voice_commands(cx);
        let voice = self.voice.read(cx);
        let controls = crate::voice_surfaces::voice_call_bar(
            &theme,
            VoiceCallBarProps {
                muted: voice.muted(),
                can_mute: voice.can_mute(),
                awaiting,
                elapsed: voice
                    .active_since
                    .map(|since| crate::voice::format_elapsed(since.elapsed().as_secs())),
                host_name: voice.host_name.clone().map(SharedString::from),
                transcript: chat_id.is_some(),
            },
            &commands,
        );
        // The stage occludes the conversation titlebar. Restore its native
        // drag and double-click behavior above the stage artwork and controls.
        let titlebar = self.titlebar_drag_region(
            "voice-stage-titlebar",
            div()
                .absolute()
                .top_0()
                .left_0()
                .right_0()
                .h(px(Theme::TITLEBAR_HEIGHT)),
            cx,
        );
        Some(crate::voice_surfaces::voice_stage(
            &theme,
            VoiceStageProps {
                reveal,
                left,
                width,
                focus: self.voice_stage_focus.clone(),
                hero: hero.map(IntoElement::into_any_element),
                orb,
                caption: crate::voice_surfaces::veiled_caption(
                    caption.text,
                    &caption.spans,
                    &theme,
                    window,
                ),
                previous_caption: caption
                    .previous
                    .map(|(previous, opacity)| (previous.into_any_element(), opacity)),
                controls,
                titlebar: titlebar.into_any_element(),
            },
            &commands,
        ))
    }

    /// The Shell's handler for every voice-surface intent: the presenters
    /// hold no state, they only report what the user asked for.
    pub(super) fn voice_commands(&self, cx: &mut Context<Self>) -> VoiceCommands {
        let shell = cx.entity().downgrade();
        std::rc::Rc::new(move |command, _window, cx| {
            shell
                .update(cx, |shell, cx| shell.apply_voice_command(command, cx))
                .ok();
        })
    }

    fn apply_voice_command(&mut self, command: VoiceCommand, cx: &mut Context<Self>) {
        match command {
            VoiceCommand::ToggleMute => self.voice.update(cx, |voice, cx| voice.toggle_mute(cx)),
            VoiceCommand::OpenStage => self.set_voice_stage_open(true, cx),
            VoiceCommand::CloseStage => self.set_voice_stage_open(false, cx),
            VoiceCommand::End => self.end_voice(cx),
            VoiceCommand::OpenTranscript => {
                if let Some(chat_id) = self.voice.read(cx).chat_id.clone() {
                    self.open_chat(chat_id, cx);
                }
            }
            VoiceCommand::WidgetHover(hovered) => {
                if self.voice_widget_hovered != hovered {
                    self.voice_widget_hovered = hovered;
                    cx.notify();
                }
            }
        }
    }

    /// Where the widget sits and which bottom band native views must leave
    /// uncovered. Native views (the Browser's web view, rich file previews)
    /// paint above GPUI content: the widget moves beside them when the
    /// window holds it, and otherwise keeps the corner with its band cropped
    /// out of the native view (painting and hit testing), never under it.
    pub(super) fn voice_widget_placement(
        &self,
        viewport_width: f32,
        cx: &App,
    ) -> VoiceWidgetPlacement {
        crate::voice_surfaces::voice_widget_placement(
            self.voice_widget_shown(cx),
            viewport_width,
            self.voice_widget_right_inset(cx),
        )
    }

    /// The native view's occupancy follows its animated visible width, not
    /// whether the pane is (still) shown: a pane closing, or slid out by the
    /// fit, keeps presenting the page until its width reaches zero, and the
    /// widget and the native crop must account for it on every such frame.
    fn voice_widget_right_inset(&self, cx: &App) -> f32 {
        let native = matches!(self.route, Route::Chat)
            && matches!(
                self.resolved_right_active(cx),
                RightSurface::Browser(_) | RightSurface::Preview(_)
            );
        if !native {
            return 0.0;
        }
        crate::voice_surfaces::native_right_inset(
            true,
            self.right_visible_width(cx),
            self.files_visible_width(cx) + self.details_visible_width(cx),
        )
    }

    /// The compact live-call widget: one capsule 16px from the window's
    /// lower-right corner while a call is live and its stage is hidden, on
    /// any Chat, Workers or Settings route. It drives the same
    /// `VoiceController` as the stage: mute, return to the stage and hang
    /// up. Hang-up owns a fixed slot revealed by hovering the capsule or
    /// focusing any of its controls, so no target moves or changes meaning.
    pub(super) fn render_voice_widget(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let shown = self.voice_widget_shown(cx);
        let orb_shown = self.voice_orbs_shown(cx).widget;
        let voice = self.voice.read(cx);
        let (orb_state, microphone, speaker) = (
            voice.orb_state(),
            voice.microphone_level(),
            voice.speaker_level(),
        );
        let (muted, can_mute) = (voice.muted(), voice.can_mute());
        let reduced = self.reduced_motion;
        self.voice_widget_orb.update(cx, |orb, cx| {
            orb.set_visible(orb_shown, cx);
            orb.set_state(orb_state, cx);
            orb.set_audio_levels(microphone, speaker, cx);
            orb.set_reduced_motion(reduced, cx);
        });
        if !shown {
            self.voice_widget_hovered = false;
            return None;
        }

        let revealed =
            self.voice_widget_hovered || self.voice_widget_focus.contains_focused(window, cx);
        let right = self
            .voice_widget_placement(f32::from(window.viewport_size().width), cx)
            .right;
        let commands = self.voice_commands(cx);
        Some(crate::voice_surfaces::voice_widget(
            VoiceWidgetProps {
                muted,
                can_mute,
                revealed,
                orb: orb_shown.then(|| self.voice_widget_orb.clone().into_any_element()),
                focus: self.voice_widget_focus.clone(),
                right,
            },
            &commands,
            cx,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn voice_test_shell(data_dir: &std::path::Path, cx: &mut Context<Shell>) -> Shell {
        let state = cx.new(|_| AppState::new());
        crate::shell::test_shell(
            state,
            EngineBootConfig {
                data_dir: data_dir.into(),
                ipc_port: 0,
                edge_url: "http://127.0.0.1:1".into(),
                edge_token: None,
                org_id: None,
                workos_client_id: None,
                default_harness: HarnessId::Mock,
            },
            cx,
        )
    }

    #[gpui::test]
    fn voice_stage_topbar_accepts_window_drag_presses(cx: &mut gpui::TestAppContext) {
        struct StageHost(Entity<Shell>);
        impl Render for StageHost {
            fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
                self.0.update(cx, |shell, cx| {
                    div()
                        .size_full()
                        .relative()
                        .children(shell.render_voice_stage(window, cx))
                })
            }
        }

        let dir = tempfile::tempdir().unwrap();
        cx.update(|cx| {
            gpui_base::init(cx);
            cx.set_global(Theme::default());
            crate::app_menus::init(cx);
            settings::init(settings::UiSettings::default(), dir.path(), cx);
        });
        let (host, cx) = cx.add_window_view(|_, cx| {
            StageHost(cx.new(|cx| {
                let mut shell = voice_test_shell(dir.path(), cx);
                shell.reduced_motion = true;
                shell.voice.update(cx, |voice, _| {
                    voice.phase = VoicePhase::Active;
                    voice.stage_open = true;
                });
                shell
            }))
        });
        let shell = host.read_with(cx, |host, _| host.0.clone());
        cx.update(|window, cx| window.draw(cx).clear());
        let bounds = cx.debug_bounds("voice-stage").unwrap();
        let topbar = gpui::point(
            bounds.center().x,
            bounds.top() + px(Theme::TITLEBAR_HEIGHT / 2.0),
        );
        cx.simulate_mouse_down(topbar, MouseButton::Left, gpui::Modifiers::default());
        shell.read_with(cx, |shell, _| assert!(shell.titlebar_should_move));
        cx.simulate_mouse_up(topbar, MouseButton::Left, gpui::Modifiers::default());
        shell.read_with(cx, |shell, _| assert!(!shell.titlebar_should_move));

        let body = gpui::point(
            bounds.center().x,
            bounds.top() + px(Theme::TITLEBAR_HEIGHT + 32.0),
        );
        cx.simulate_mouse_down(body, MouseButton::Left, gpui::Modifiers::default());
        shell.read_with(cx, |shell, _| assert!(!shell.titlebar_should_move));
        cx.simulate_mouse_up(body, MouseButton::Left, gpui::Modifiers::default());
    }

    #[gpui::test]
    fn voice_stage_can_start_and_reopen_in_settings_until_navigation(
        cx: &mut gpui::TestAppContext,
    ) {
        struct StageHost(Entity<Shell>);
        impl Render for StageHost {
            fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
                self.0.update(cx, |shell, cx| {
                    shell.sync_voice_stage(window, cx);
                    div()
                        .size_full()
                        .children(shell.render_voice_stage(window, cx))
                })
            }
        }

        let dir = tempfile::tempdir().unwrap();
        cx.update(|cx| {
            gpui_base::init(cx);
            cx.set_global(Theme::default());
            crate::app_menus::init(cx);
            settings::init(settings::UiSettings::default(), dir.path(), cx);
        });
        let (host, cx) = cx.add_window_view(|_, cx| {
            StageHost(cx.new(|cx| {
                let mut shell = voice_test_shell(dir.path(), cx);
                shell.reduced_motion = true;
                shell.route = Route::Settings(SettingsSection::Voice);
                // The initial stage opens while native voice is still preparing.
                shell.voice.update(cx, |voice, cx| {
                    voice.phase = VoicePhase::Checking;
                    voice.set_stage_open(true, cx);
                });
                shell
            }))
        });
        let shell = host.read_with(cx, |host, _| host.0.clone());
        let stays_open = |cx: &mut gpui::VisualTestContext| {
            for _ in 0..3 {
                cx.update(|window, cx| window.draw(cx).clear());
                shell.read_with(cx, |shell, cx| assert!(shell.voice.read(cx).stage_open));
                assert!(cx.debug_bounds("voice-stage").is_some());
            }
        };
        stays_open(cx);

        shell.update(cx, |shell, cx| {
            shell
                .voice
                .update(cx, |voice, _| voice.phase = VoicePhase::Active);
            shell.set_voice_stage_open(false, cx);
        });
        cx.update(|window, cx| window.draw(cx).clear());
        shell.update(cx, |shell, cx| shell.set_voice_stage_open(true, cx));
        stays_open(cx);

        // Choosing another settings page hides the stage without ending voice.
        shell.update(cx, |shell, cx| {
            shell.open_settings(SettingsSection::General, cx)
        });
        cx.update(|window, cx| window.draw(cx).clear());
        shell.read_with(cx, |shell, cx| {
            assert!(!shell.voice.read(cx).stage_open);
            assert!(shell.voice.read(cx).is_live());
        });
        shell.update(cx, |shell, cx| shell.set_voice_stage_open(true, cx));
        stays_open(cx);

        // Leaving and reentering the same settings page is navigation too.
        shell.update(cx, |shell, _| shell.route = Route::Chat);
        cx.update(|window, cx| window.draw(cx).clear());
        shell.update(cx, |shell, cx| {
            shell.open_settings(SettingsSection::General, cx)
        });
        cx.update(|window, cx| window.draw(cx).clear());
        shell.read_with(cx, |shell, cx| {
            assert!(!shell.voice.read(cx).stage_open);
            assert!(shell.voice.read(cx).is_live());
        });

        shell.update(cx, |shell, cx| shell.set_voice_stage_open(true, cx));
        stays_open(cx);
        shell.update(cx, |shell, cx| {
            shell.state.update(cx, |state, _| {
                state.selected_chat = Some("another-chat".into())
            });
        });
        cx.update(|window, cx| window.draw(cx).clear());
        shell.read_with(cx, |shell, cx| {
            assert!(!shell.voice.read(cx).stage_open);
            assert!(shell.voice.read(cx).is_live());
        });
    }

    #[gpui::test]
    fn opening_any_session_steps_the_stage_aside_without_ending_voice(
        cx: &mut gpui::TestAppContext,
    ) {
        struct StageHost(Entity<Shell>);
        impl Render for StageHost {
            fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
                self.0.update(cx, |shell, cx| {
                    shell.sync_voice_stage(window, cx);
                    div()
                        .size_full()
                        .children(shell.render_voice_stage(window, cx))
                })
            }
        }

        let dir = tempfile::tempdir().unwrap();
        cx.update(|cx| {
            gpui_base::init(cx);
            cx.set_global(Theme::default());
            crate::app_menus::init(cx);
            settings::init(settings::UiSettings::default(), dir.path(), cx);
        });
        let (host, cx) = cx.add_window_view(|_, cx| {
            StageHost(cx.new(|cx| {
                let mut shell = voice_test_shell(dir.path(), cx);
                shell.reduced_motion = true;
                shell
                    .state
                    .update(cx, |state, _| state.selected_chat = Some("current".into()));
                shell.voice.update(cx, |voice, cx| {
                    voice.phase = VoicePhase::Active;
                    voice.set_stage_open(true, cx);
                });
                shell
            }))
        });
        let shell = host.read_with(cx, |host, _| host.0.clone());
        let open_stage = |cx: &mut gpui::VisualTestContext| {
            shell.update(cx, |shell, cx| shell.set_voice_stage_open(true, cx));
            cx.update(|window, cx| window.draw(cx).clear());
            shell.read_with(cx, |shell, cx| assert!(shell.voice.read(cx).stage_open));
        };
        let stepped_aside = |cx: &mut gpui::VisualTestContext, selected: &str| {
            cx.update(|window, cx| window.draw(cx).clear());
            shell.read_with(cx, |shell, cx| {
                assert!(!shell.voice.read(cx).stage_open);
                assert!(shell.voice.read(cx).is_live(), "the call keeps running");
                assert_eq!(shell.route, Route::Chat);
                assert_eq!(
                    shell.state.read(cx).selected_chat.as_deref(),
                    Some(selected)
                );
            });
        };
        cx.update(|window, cx| window.draw(cx).clear());

        // The session already under the stage: the selection doesn't change,
        // so only an explicit step-aside shows it.
        shell.update(cx, |shell, cx| shell.open_chat("current".into(), cx));
        stepped_aside(cx, "current");

        // From Settings back to the session the stage was opened over.
        shell.update(cx, |shell, _| {
            shell.route = Route::Settings(SettingsSection::Voice)
        });
        open_stage(cx);
        shell.update(cx, |shell, cx| shell.open_chat("current".into(), cx));
        stepped_aside(cx, "current");

        // Another session.
        open_stage(cx);
        shell.update(cx, |shell, cx| shell.open_chat("other".into(), cx));
        stepped_aside(cx, "other");
    }

    #[test]
    fn stage_reveal_eases_in_and_out_and_snaps_for_reduced_motion() {
        assert_eq!(stage_reveal(true, 0.0, false), 0.0);
        assert_eq!(stage_reveal(true, f32::INFINITY, false), 1.0);
        assert!(stage_reveal(true, STAGE_ENTER_MS / 4.0, false) > 0.5);
        assert_eq!(stage_reveal(false, 0.0, false), 1.0);
        assert_eq!(stage_reveal(false, f32::INFINITY, false), 0.0);
        assert_eq!(stage_reveal(true, 0.0, true), 1.0);
        assert_eq!(stage_reveal(false, 0.0, true), 0.0);
    }
}
