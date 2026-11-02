//! Presenters for the voice surfaces: the full-window stage, its call bar,
//! the compact live-call widget and the "Show voice orb" preference row.
//!
//! They hold no call state and own no entity. The host (the Shell in the
//! app, the hermetic review fixture in `examples/`) reads its call state,
//! passes it in as props and receives every user intent as a
//! [`VoiceCommand`], so production and the fixture render the same
//! elements from the same code.

use std::rc::Rc;

use gpui::{
    AnyElement, App, FocusHandle, IntoElement, ParentElement, SharedString, Styled, Window, div,
    prelude::*, px,
};

use crate::icons::{self, icon};
use crate::settings::{self, widgets};
use crate::theme::Theme;
use crate::{motion, popover};

pub const BAR_CONTROL: f32 = 44.0;
pub const BAR_PAD: f32 = 8.0;
pub const WIDGET_CONTROL: f32 = 40.0;
/// The return control (orb or static icon) is a little larger.
pub const WIDGET_RETURN: f32 = WIDGET_CONTROL + 8.0;
pub const WIDGET_GAP: f32 = 6.0;
pub const WIDGET_PAD: f32 = 6.0;
/// `border_1` on the capsule.
pub const WIDGET_BORDER: f32 = 1.0;
pub const WIDGET_MARGIN: f32 = 16.0;
/// The capsule as rendered: mic, return and end in fixed slots, two gaps,
/// padding and border. Placement budgets exactly this.
pub const WIDGET_WIDTH: f32 = 2.0 * WIDGET_CONTROL
    + WIDGET_RETURN
    + 2.0 * WIDGET_GAP
    + 2.0 * WIDGET_PAD
    + 2.0 * WIDGET_BORDER;
/// The capsule's height: its tallest control, padding and border.
pub const WIDGET_HEIGHT: f32 = WIDGET_RETURN + 2.0 * WIDGET_PAD + 2.0 * WIDGET_BORDER;
/// The band along the window's bottom edge a native view leaves to the
/// widget when it falls back to the corner: margin, capsule, margin.
pub const VOICE_WIDGET_BAND: f32 = 2.0 * WIDGET_MARGIN + WIDGET_HEIGHT;
pub const VOICE_STAGE_ORB_SCALE: f32 = 2.25;
pub const VOICE_WIDGET_ORB_SCALE: f32 = 0.7;
/// The stage's orb box: the scaled orb plus room for its bloom.
pub const STAGE_ORB_BOX: f32 = 128.0 * VOICE_STAGE_ORB_SCALE + 112.0;

/// The widget's right offset and the bottom band native views leave it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VoiceWidgetPlacement {
    pub right: f32,
    pub native_band: f32,
}

/// How much of the window's right edge native views occupy, for widget
/// placement and the native crop: their *animated* visible width (and the
/// columns right of it), not whether the pane is still meant to be shown.
/// A pane closing or slid out by the fit keeps presenting its page until
/// that width reaches zero.
pub fn native_right_inset(native_surface: bool, visible_width: f32, right_of_it: f32) -> f32 {
    if !native_surface || visible_width <= 0.0 {
        return 0.0;
    }
    visible_width + right_of_it
}

/// Details paints nothing for a closed target, even if an old tween is live.
/// Otherwise both layout and native clearance use the evaluated frame width.
pub fn painted_details_width(target: f32, evaluate: impl FnOnce() -> f32) -> f32 {
    if target <= 0.5 { 0.0 } else { evaluate() }
}

/// Where the widget sits. Native views (a Browser's web view, rich file
/// previews) paint above GPUI: `native_inset` is how much of the window's
/// right edge they cover (0 when none is visible). The widget moves beside
/// them when the window holds it; otherwise it keeps the corner and the
/// native views leave its band uncovered (painting and hit testing), so it
/// is never under them.
pub fn voice_widget_placement(
    shown: bool,
    viewport_width: f32,
    native_inset: f32,
) -> VoiceWidgetPlacement {
    let corner = VoiceWidgetPlacement {
        right: WIDGET_MARGIN,
        native_band: 0.0,
    };
    if !shown || native_inset <= 0.0 {
        return corner;
    }
    let beside = WIDGET_MARGIN + native_inset;
    // The whole capsule must stay inside the window beside the view.
    if beside + WIDGET_WIDTH <= viewport_width {
        return VoiceWidgetPlacement {
            right: beside,
            native_band: 0.0,
        };
    }
    VoiceWidgetPlacement {
        right: WIDGET_MARGIN,
        native_band: VOICE_WIDGET_BAND,
    }
}

/// The stage's orb, hidden until a stage presents it.
pub fn stage_orb() -> crate::orb::Orb {
    crate::orb::Orb::new()
        .size(crate::orb::OrbSize::Hero)
        .state_transition(std::time::Duration::from_millis(300))
        .scale(VOICE_STAGE_ORB_SCALE)
        .visible(false)
}

/// The widget's Avatar orb, hidden until the widget presents it.
pub fn widget_orb() -> crate::orb::Orb {
    crate::orb::Orb::new()
        .size(crate::orb::OrbSize::Avatar)
        .scale(VOICE_WIDGET_ORB_SCALE)
        .state_transition(std::time::Duration::from_millis(300))
        .visible(false)
}

/// A user intent on a voice surface, handed to the host.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VoiceCommand {
    ToggleMute,
    OpenStage,
    CloseStage,
    End,
    OpenTranscript,
    WidgetHover(bool),
}

/// The host's handler for [`VoiceCommand`]s.
pub type VoiceCommands = Rc<dyn Fn(VoiceCommand, &mut Window, &mut App)>;

fn on(
    commands: &VoiceCommands,
    command: VoiceCommand,
) -> impl Fn(&gpui::ClickEvent, &mut Window, &mut App) + 'static {
    let commands = commands.clone();
    move |_, window, cx| {
        cx.stop_propagation();
        commands(command, window, cx);
    }
}

/// The surfaces' tooltip: the same card as the Shell's surface tabs.
pub fn tooltip(text: &'static str) -> impl Fn(&mut Window, &mut App) -> gpui::AnyView + 'static {
    move |_, cx| {
        cx.new(|_| crate::shell::SurfaceTabTooltip { text: text.into() })
            .into()
    }
}

/// A round call-bar control; `active` inverts it onto the solid plate.
pub fn round_control(id: &'static str, theme: &Theme, active: bool) -> gpui::Stateful<gpui::Div> {
    div()
        .id(id)
        .role(gpui::Role::Button)
        .tab_index(0)
        .relative()
        .size(px(BAR_CONTROL))
        .flex_none()
        .rounded_full()
        .flex()
        .items_center()
        .justify_center()
        .cursor_pointer()
        .text_color(if active { theme.on_solid } else { theme.text })
        .bg(if active {
            theme.solid
        } else {
            motion::hover_blend(id, theme.glass_hover().opacity(0.5), theme.glass_hover())
        })
        .on_hover(motion::hover_listener(id))
        .focus_visible(|s| s.border_2().border_color(theme.accent))
}

/// The caption with its fading words recolored, paint-only (`apply_veil`).
pub fn veiled_caption(
    text: String,
    spans: &[zeron_veil::VeilSpan],
    theme: &Theme,
    window: &Window,
) -> gpui::StyledText {
    let mut run = window.text_style().to_run(text.len());
    run.color = theme.text_muted;
    let runs = crate::markdown::veil::apply_veil(vec![run], spans);
    gpui::StyledText::new(text).with_runs(runs)
}

/// What the call bar shows.
pub struct VoiceCallBarProps {
    pub muted: bool,
    /// Audio state and the existing call control channel are ready.
    pub can_mute: bool,
    /// The orchestrator is waiting for the user's answer.
    pub awaiting: bool,
    /// Formatted session time, when the call has started.
    pub elapsed: Option<String>,
    pub host_name: Option<SharedString>,
    /// The call has a transcript Chat to open.
    pub transcript: bool,
}

/// Video-call style bar: session time, the call controls grouped around a
/// red hang-up, and the way back to the chats.
pub fn voice_call_bar(
    theme: &Theme,
    props: VoiceCallBarProps,
    commands: &VoiceCommands,
) -> AnyElement {
    let popup = theme.for_popup();
    let VoiceCallBarProps {
        muted,
        can_mute,
        awaiting,
        elapsed,
        host_name,
        transcript,
    } = props;
    let live_tone = if awaiting {
        popup.warning
    } else if muted {
        popup.text_faint
    } else {
        popup.success
    };

    // Session clock, like a meeting's running time.
    let clock = div()
        .flex()
        .items_center()
        .gap(px(8.0))
        .pl(px(10.0))
        .pr(px(6.0))
        .child(div().size(px(7.0)).rounded_full().bg(live_tone))
        .child(
            div()
                .min_w(px(40.0))
                .font_family(popup.font_mono.clone())
                .text_size(crate::typography::ui_rems(13.0))
                .text_color(popup.text_muted)
                .child(SharedString::from(elapsed.unwrap_or_else(|| "–:––".into()))),
        )
        .children(host_name.map(|name| {
            div()
                .text_size(crate::typography::ui_rems(12.0))
                .text_color(popup.text_faint)
                .child(name)
        }));

    // Microphone: inverted plate while muted.
    let mic = round_control("voice-bar-mic", &popup, muted)
        .debug_selector(|| "voice-bar-mic".into())
        .aria_label(match (can_mute, muted) {
            (false, _) => "Microphone available once the call connects",
            (true, true) => "Unmute microphone",
            (true, false) => "Mute microphone",
        })
        .tooltip(tooltip(match (can_mute, muted) {
            (false, _) => "Connecting…",
            (true, true) => "Unmute · M",
            (true, false) => "Mute · M",
        }))
        .when(!can_mute, |el| el.opacity(0.45).cursor_default())
        .when(can_mute, |el| {
            el.on_click(on(commands, VoiceCommand::ToggleMute))
        })
        .child(
            icon(if muted {
                icons::MICROPHONE_OFF
            } else {
                icons::MICROPHONE
            })
            .size(px(19.0))
            .text_color(if muted { popup.on_solid } else { popup.text }),
        );

    let transcript = transcript.then(|| {
        round_control("voice-bar-transcript", &popup, false)
            .aria_label(if awaiting {
                "Answer Codex in the transcript"
            } else {
                "Open the voice transcript"
            })
            .tooltip(tooltip(if awaiting {
                "Codex is asking you something"
            } else {
                "Transcript"
            }))
            .on_click(on(commands, VoiceCommand::OpenTranscript))
            .child(
                icon(icons::CHAT_ROUND_LINE)
                    .size(px(19.0))
                    .text_color(if awaiting { popup.warning } else { popup.text }),
            )
            .when(awaiting, |el| {
                el.child(
                    div()
                        .absolute()
                        .top(px(9.0))
                        .right(px(9.0))
                        .size(px(8.0))
                        .rounded_full()
                        .border_2()
                        .border_color(popover::surface_bg(&popup))
                        .bg(popup.warning),
                )
            })
    });

    // The hang-up: the one saturated control, wide enough to never be
    // mistaken for its neighbours.
    let end = div()
        .id("voice-bar-end")
        .role(gpui::Role::Button)
        .aria_label("End voice")
        .tab_index(0)
        .h(px(BAR_CONTROL))
        .px(px(20.0))
        .rounded_full()
        .flex()
        .items_center()
        .gap(px(8.0))
        .cursor_pointer()
        .bg(motion::hover_blend(
            "voice-bar-end",
            popup.danger,
            popup.danger.blend(gpui::black().opacity(0.14)),
        ))
        .on_hover(motion::hover_listener("voice-bar-end"))
        .focus_visible(|s| s.border_2().border_color(popup.accent))
        .tooltip(tooltip("End voice"))
        .on_click(on(commands, VoiceCommand::End))
        .child(
            icon(icons::PHONE_HANG_UP)
                .size(px(19.0))
                .text_color(gpui::white()),
        )
        .child(
            div()
                .text_size(crate::typography::ui_rems(13.0))
                .font_weight(gpui::FontWeight::SEMIBOLD)
                .text_color(gpui::white())
                .child(SharedString::from("End")),
        );

    let back = round_control("voice-bar-back", &popup, false)
        .aria_label("Back to chats")
        .tooltip(tooltip("Back to chats · Esc"))
        .on_click(on(commands, VoiceCommand::CloseStage))
        .child(
            icon(icons::ALT_ARROW_DOWN)
                .size(px(19.0))
                .text_color(popup.text_muted),
        );

    let divider = || div().w(px(1.0)).h(px(24.0)).mx(px(4.0)).bg(popup.border);
    let radius = BAR_CONTROL / 2.0 + BAR_PAD;
    let bar = div()
        .flex()
        .items_center()
        .gap(px(6.0))
        .p(px(BAR_PAD))
        .rounded(px(radius))
        .border_1()
        .border_color(popup.border.opacity(0.7))
        .bg(popover::surface_bg(&popup))
        .when(!popup.is_frost(), |el| el.shadow_lg())
        .text_color(popup.text)
        .child(clock)
        .child(divider())
        .child(mic)
        .children(transcript)
        .child(end)
        .child(divider())
        .child(back);
    crate::frost::frosted(radius, 18.0, bar).into_any_element()
}

/// What the stage shows. The host builds the artwork, orb and titlebar
/// elements it owns; the stage lays them out.
pub struct VoiceStageProps {
    /// How far the stage has arrived: 0 hidden, 1 fully covering.
    pub reveal: f32,
    /// Where the stage begins: the visible sidebar width.
    pub left: f32,
    /// The stage's own width.
    pub width: f32,
    pub focus: FocusHandle,
    /// The new-thread hero artwork, already sized.
    pub hero: Option<AnyElement>,
    /// The orb, when orbs are shown. With orbs off there is no orb and no
    /// orb-sized placeholder.
    pub orb: Option<AnyElement>,
    pub caption: gpui::StyledText,
    /// The previous caption fading out, with its opacity.
    pub previous_caption: Option<(AnyElement, f32)>,
    pub controls: AnyElement,
    /// The window-drag region restored above the stage.
    pub titlebar: AnyElement,
}

/// Full-window stage: the session's orb over the new-thread hero artwork,
/// live caption, and the session controls. Escape returns to the chats
/// without ending voice; plain M toggles mute.
pub fn voice_stage(theme: &Theme, props: VoiceStageProps, commands: &VoiceCommands) -> AnyElement {
    let VoiceStageProps {
        reveal,
        left,
        width,
        focus,
        hero,
        orb,
        caption,
        previous_caption,
        controls,
        titlebar,
    } = props;
    let orb_block = orb.map(|orb| {
        div()
            .debug_selector(|| "voice-stage-orb".into())
            .relative()
            .size(px(STAGE_ORB_BOX))
            .flex()
            .items_center()
            .justify_center()
            .child(orb)
    });
    let center = div()
        .relative()
        .top(px(18.0 * (1.0 - reveal)))
        .flex()
        .flex_col()
        .items_center()
        .children(orb_block)
        .child(
            div()
                .debug_selector(|| "voice-stage-caption".into())
                .relative()
                .mt(px(10.0))
                .w(px(560.0))
                .max_w(px((width - 48.0).max(200.0)))
                .h(px(48.0))
                .overflow_hidden()
                .text_center()
                .text_size(crate::typography::ui_rems(14.0))
                .line_height(px(22.0))
                .text_color(theme.text_muted)
                .child(caption)
                .children(previous_caption.map(|(previous, opacity)| {
                    div()
                        .absolute()
                        .top_0()
                        .left_0()
                        .right_0()
                        .opacity(opacity)
                        .child(previous)
                })),
        );
    let keys = commands.clone();
    div()
        .id("voice-stage")
        .debug_selector(|| "voice-stage".into())
        // The focused stage is an identified group for assistive technology.
        .role(gpui::Role::Group)
        .aria_label("Voice call")
        .track_focus(&focus)
        .absolute()
        .top_0()
        .bottom_0()
        .right_0()
        .left(px(left))
        .occlude()
        .opacity(reveal)
        // On glass the stage shows the window frost like the new-thread
        // page; the page beneath fades out instead.
        .when(!theme.is_glass(), |stage| stage.bg(theme.bg))
        .on_key_down(move |event: &gpui::KeyDownEvent, window, cx| {
            let key = event.keystroke.key.as_str();
            let plain = !event.keystroke.modifiers.modified();
            if key == "escape" {
                cx.stop_propagation();
                keys(VoiceCommand::CloseStage, window, cx);
            } else if key == "m" && plain {
                cx.stop_propagation();
                keys(VoiceCommand::ToggleMute, window, cx);
            }
        })
        .child(div().absolute().inset_0().children(hero))
        .child(
            div()
                .absolute()
                .inset_0()
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
                .pb(px(64.0))
                .child(center),
        )
        .child(
            div()
                .absolute()
                .left_0()
                .right_0()
                .bottom(px(32.0 - 16.0 * (1.0 - reveal)))
                .flex()
                .justify_center()
                .child(controls),
        )
        .child(titlebar)
        .into_any_element()
}

/// What the compact live-call widget shows.
pub struct VoiceWidgetProps {
    pub muted: bool,
    /// Mute is offered only once the call has audio state.
    pub can_mute: bool,
    /// Hang-up is revealed (capsule hovered or a control focused).
    pub revealed: bool,
    /// The orb inside the return control, when orbs are shown.
    pub orb: Option<AnyElement>,
    pub focus: FocusHandle,
    /// Offset from the window's right edge.
    pub right: f32,
}

/// The compact live-call widget: one capsule `WIDGET_MARGIN` from the
/// window's lower-right corner (or beside a native view). It drives the
/// same call as the stage: mute, return to the stage and hang up. Hang-up
/// owns a fixed slot revealed by hover or focus, so no target moves or
/// changes meaning.
pub fn voice_widget(props: VoiceWidgetProps, commands: &VoiceCommands, cx: &App) -> AnyElement {
    let VoiceWidgetProps {
        muted,
        can_mute,
        revealed,
        orb,
        focus,
        right,
    } = props;
    let theme = Theme::of(cx).for_popup();
    let mic = round_control("voice-widget-mic", &theme, muted)
        .size(px(WIDGET_CONTROL))
        .debug_selector(|| "voice-widget-mic".into())
        .aria_label(match (can_mute, muted) {
            (false, _) => "Microphone available once the call connects",
            (true, true) => "Unmute microphone",
            (true, false) => "Mute microphone",
        })
        .tooltip(tooltip(match (can_mute, muted) {
            (false, _) => "Connecting…",
            (true, true) => "Unmute",
            (true, false) => "Mute",
        }))
        .when(!can_mute, |el| el.opacity(0.45).cursor_default())
        .on_click(on(commands, VoiceCommand::ToggleMute))
        .child(
            icon(if muted {
                icons::MICROPHONE_OFF
            } else {
                icons::MICROPHONE
            })
            .size(px(17.0))
            .text_color(if muted { theme.on_solid } else { theme.text }),
        );

    let back = div()
        .id("voice-widget-return")
        .debug_selector(|| "voice-widget-return".into())
        .role(gpui::Role::Button)
        .aria_label("Return to voice")
        .tab_index(0)
        .size(px(WIDGET_RETURN))
        .flex_none()
        .rounded_full()
        .flex()
        .items_center()
        .justify_center()
        .cursor_pointer()
        .focus_visible(|s| s.border_2().border_color(theme.accent))
        .tooltip(tooltip("Return to voice"))
        .on_click(on(commands, VoiceCommand::OpenStage))
        .child(orb.unwrap_or_else(|| {
            // Orbs off: a static, identified return control.
            div()
                .size(px(WIDGET_CONTROL))
                .rounded_full()
                .flex()
                .items_center()
                .justify_center()
                .bg(motion::hover_blend(
                    "voice-widget-return",
                    theme.glass_hover().opacity(0.5),
                    theme.glass_hover(),
                ))
                .child(
                    icon(icons::VOLUME_LOUD)
                        .size(px(17.0))
                        .text_color(theme.success),
                )
                .into_any_element()
        }))
        .on_hover(motion::hover_listener("voice-widget-return"));

    // Hang-up keeps its slot whether revealed or not: revealing never
    // shifts the microphone or the return target.
    let end = div()
        .id("voice-widget-end")
        .debug_selector(|| "voice-widget-end".into())
        .role(gpui::Role::Button)
        .aria_label("End voice")
        .tab_index(0)
        .size(px(WIDGET_CONTROL))
        .flex_none()
        .rounded_full()
        .flex()
        .items_center()
        .justify_center()
        .cursor_pointer()
        .opacity(if revealed { 1.0 } else { 0.0 })
        .bg(motion::hover_blend(
            "voice-widget-end",
            theme.danger,
            theme.danger.blend(gpui::black().opacity(0.14)),
        ))
        .on_hover(motion::hover_listener("voice-widget-end"))
        .focus_visible(|s| s.border_2().border_color(theme.accent))
        .tooltip(tooltip("End voice"))
        .on_click(on(commands, VoiceCommand::End))
        .child(
            icon(icons::PHONE_HANG_UP)
                .size(px(17.0))
                .text_color(gpui::white()),
        );

    let hover = commands.clone();
    let radius = WIDGET_RETURN / 2.0 + WIDGET_PAD;
    let capsule = div()
        .id("voice-widget")
        .debug_selector(|| "voice-widget".into())
        .role(gpui::Role::Group)
        .aria_label("Voice call")
        .track_focus(&focus)
        .flex()
        .items_center()
        .gap(px(WIDGET_GAP))
        .p(px(WIDGET_PAD))
        .rounded(px(radius))
        .border_1()
        .border_color(theme.border.opacity(0.7))
        .bg(popover::surface_bg(&theme))
        .when(!theme.is_frost(), |el| el.shadow_lg())
        .text_color(theme.text)
        // Only the capsule takes input; the page around it stays live.
        .occlude()
        .on_hover(move |hovered: &bool, window, cx| {
            hover(VoiceCommand::WidgetHover(*hovered), window, cx)
        })
        .child(mic)
        .child(back)
        .child(end);
    div()
        .absolute()
        .bottom(px(WIDGET_MARGIN))
        .right(px(right))
        .child(crate::frost::frosted(radius, 18.0, capsule))
        .into_any_element()
}

/// Settings ▸ Voice "Show voice orb": a UI preference, independent of
/// Reduce motion and of the call itself. Toggling it applies at once to
/// every voice surface in every window.
pub fn show_voice_orb_row(theme: &Theme, cx: &App) -> AnyElement {
    let orb_on = settings::current(cx).voice_orb_enabled;
    let toggle_orb = |cx: &mut App| {
        settings::update(settings::SavePolicy::Immediate, cx, |s| {
            s.voice_orb_enabled = !s.voice_orb_enabled
        });
        cx.refresh_windows();
    };
    let orb_switch = widgets::toggle_switch(theme, orb_on, "voice-orb-visible")
        .id("voice-orb-visible")
        .debug_selector(|| "voice-orb-visible".into())
        .cursor_pointer()
        .tab_index(0)
        .role(gpui::Role::Switch)
        .aria_label("Show voice orb")
        .aria_toggled(if orb_on {
            gpui::Toggled::True
        } else {
            gpui::Toggled::False
        })
        .focus_visible(|s| s.border_2().border_color(theme.accent).opacity(1.0))
        // Enter and Space reach on_click as GPUI's synthesized keyboard
        // activation (one per complete press); a key-down handler here would
        // toggle a second time.
        .on_click(move |_, _, cx| toggle_orb(cx))
        .on_a11y_action(gpui::AccessibleAction::Click, move |_, _, cx| {
            toggle_orb(cx)
        });
    widgets::card_row(theme, true)
        .child(
            div()
                .flex_1()
                .min_w(px(160.0))
                .flex()
                .flex_col()
                .child(widgets::row_title(theme, "Show voice orb"))
                .child(widgets::meta_line(
                    theme,
                    vec!["Hide voice visuals without ending the call.".into_any_element()],
                )),
        )
        .child(orb_switch)
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::cell::{Cell, RefCell};

    use gpui::{
        Context, InputEvent as _, IntoElement, KeyDownEvent, KeyUpEvent, Keystroke, Render,
        TestAppContext,
    };

    fn init_keyboard(cx: &mut TestAppContext, data_dir: &std::path::Path) {
        cx.update(|cx| {
            gpui_base::init(cx);
            cx.set_global(Theme::default());
            settings::init(settings::UiSettings::default(), data_dir, cx);
        });
    }

    /// One complete key press on whatever is focused, repainting between
    /// down and up and after, as a real frame loop would.
    fn press(cx: &mut gpui::VisualTestContext, key: &str) {
        let keystroke = Keystroke::parse(key).unwrap();
        cx.update(|window, cx| {
            window.dispatch_event(
                KeyDownEvent {
                    keystroke: keystroke.clone(),
                    is_held: false,
                    prefer_character_input: false,
                }
                .to_platform_input(),
                cx,
            );
        });
        cx.run_until_parked();
        cx.update(|window, cx| window.draw(cx).clear());
        cx.update(|window, cx| {
            window.dispatch_event(KeyUpEvent { keystroke }.to_platform_input(), cx);
        });
        cx.run_until_parked();
        cx.update(|window, cx| window.draw(cx).clear());
    }

    /// Focuses the `nth` tab stop (1-based) of the drawn window.
    fn focus_tab_stop(cx: &mut gpui::VisualTestContext, nth: usize) {
        cx.update(|window, cx| {
            window.draw(cx).clear();
            window.blur();
            for _ in 0..nth {
                window.focus_next(cx);
            }
        });
        cx.run_until_parked();
        cx.update(|window, cx| window.draw(cx).clear());
    }

    struct OrbRow;

    impl Render for OrbRow {
        fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            let theme = Theme::of(cx).clone();
            div().size_full().child(show_voice_orb_row(&theme, cx))
        }
    }

    /// GF4: the "Show voice orb" switch changes the preference exactly once
    /// per complete Enter or Space press (it used to toggle on key-down and
    /// again on the synthesized click at key-up, ending where it started).
    #[gpui::test]
    fn orb_switch_toggles_once_per_keyboard_press(cx: &mut TestAppContext) {
        let data = tempfile::tempdir().unwrap();
        init_keyboard(cx, data.path());
        let (_, cx) = cx.add_window_view(|_, _| OrbRow);
        focus_tab_stop(cx, 1);
        for key in ["enter", "space", "enter", "space"] {
            let before = cx.update(|_, cx| settings::current(cx).voice_orb_enabled);
            press(cx, key);
            let after = cx.update(|_, cx| settings::current(cx).voice_orb_enabled);
            assert_eq!(after, !before, "one preference change for {key}");
        }
    }

    /// The live-call widget hosted as the Shell hosts it: return and end
    /// unmount it, as opening the stage or hanging up does.
    struct WidgetHost {
        mounted: Rc<Cell<bool>>,
        commands: VoiceCommands,
        focus: gpui::FocusHandle,
    }

    impl Render for WidgetHost {
        fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            let root = div().size_full();
            if !self.mounted.get() {
                return root;
            }
            root.child(voice_widget(
                VoiceWidgetProps {
                    muted: false,
                    can_mute: true,
                    revealed: true,
                    orb: None,
                    focus: self.focus.clone(),
                    right: WIDGET_MARGIN,
                },
                &self.commands,
                cx,
            ))
        }
    }

    /// GF4: every widget control runs its command once per complete Enter
    /// or Space press, including return and end, which unmount the widget
    /// between key-down and key-up.
    #[gpui::test]
    fn widget_controls_run_one_command_per_keyboard_press(cx: &mut TestAppContext) {
        let data = tempfile::tempdir().unwrap();
        init_keyboard(cx, data.path());
        let received: Rc<RefCell<Vec<VoiceCommand>>> = Rc::default();
        let mounted = Rc::new(Cell::new(true));
        let commands: VoiceCommands = {
            let received = received.clone();
            let mounted = mounted.clone();
            Rc::new(move |command, window, _| {
                received.borrow_mut().push(command);
                if matches!(command, VoiceCommand::OpenStage | VoiceCommand::End) {
                    mounted.set(false);
                    window.refresh();
                }
            })
        };
        let (_, cx) = cx.add_window_view({
            let mounted = mounted.clone();
            move |_, cx| WidgetHost {
                mounted,
                commands,
                focus: cx.focus_handle(),
            }
        });
        for (stop, expected) in [
            (1, VoiceCommand::ToggleMute),
            (2, VoiceCommand::OpenStage),
            (3, VoiceCommand::End),
        ] {
            for key in ["enter", "space"] {
                mounted.set(true);
                cx.update(|window, _| window.refresh());
                focus_tab_stop(cx, stop);
                received.borrow_mut().clear();
                press(cx, key);
                assert_eq!(
                    received.borrow().as_slice(),
                    &[expected],
                    "one {expected:?} for {key}"
                );
            }
        }
    }

    struct CallBarHost {
        voice: gpui::Entity<crate::voice::VoiceController>,
        commands: VoiceCommands,
    }
    impl Render for CallBarHost {
        fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            let voice = self.voice.read(cx);
            div().size_full().child(voice_call_bar(
                Theme::of(cx),
                VoiceCallBarProps {
                    muted: voice.muted(),
                    can_mute: voice.can_mute(),
                    awaiting: false,
                    elapsed: None,
                    host_name: None,
                    transcript: false,
                },
                &self.commands,
            ))
        }
    }

    #[gpui::test]
    fn stage_mute_waits_for_readiness_then_controls_the_same_call_once(cx: &mut TestAppContext) {
        use crate::voice::{VoiceControl, VoiceController};
        use zeron_proto::voice::{VoicePhase, VoiceSnapshot, VoiceWork};
        let data = tempfile::tempdir().unwrap();
        init_keyboard(cx, data.path());
        let controller = cx.new(|_| VoiceController::default());
        let mut controls = controller.update(cx, |voice, _| {
            voice.phase = VoicePhase::Checking;
            voice.chat_id = Some("same-call-chat".into());
            voice.attach_test_controls()
        });
        let received = Rc::new(Cell::new(0));
        let commands: VoiceCommands = {
            let controller = controller.clone();
            let received = received.clone();
            Rc::new(move |command, _, cx| {
                if command == VoiceCommand::ToggleMute {
                    received.set(received.get() + 1);
                    controller.update(cx, |voice, cx| voice.toggle_mute(cx));
                }
            })
        };
        let (host, cx) = cx.add_window_view({
            let controller = controller.clone();
            move |_, _| CallBarHost {
                voice: controller,
                commands,
            }
        });
        focus_tab_stop(cx, 1);
        press(cx, "enter");
        assert_eq!(
            received.get(),
            0,
            "unavailable stage mic has no actionable click"
        );
        assert!(controls.try_recv().is_err());
        controller.update(cx, |voice, _| {
            voice.phase = VoicePhase::Active;
            voice.snapshot = Some(VoiceSnapshot {
                session_id: "same-call".into(),
                chat_id: "same-call-chat".into(),
                generation: 1,
                phase: VoicePhase::Active,
                muted: false,
                playing: false,
                work: VoiceWork::Idle,
                reason: None,
                voice: None,
                voices: Vec::new(),
            });
        });
        host.update(cx, |_, cx| cx.notify());
        focus_tab_stop(cx, 1);
        press(cx, "enter");
        assert_eq!(received.get(), 1);
        assert!(matches!(controls.try_recv(), Ok(VoiceControl::Mute(true))));
        assert!(controls.try_recv().is_err(), "one command only");
        controller.read_with(cx, |voice, _| {
            assert!(voice.muted());
            assert_eq!(voice.chat_id.as_deref(), Some("same-call-chat"));
            assert_eq!(voice.snapshot.as_ref().unwrap().session_id, "same-call");
        });
    }

    /// Component-only host for the production widget beside a real GPUI
    /// page column and a Details opening frame; never constructs a Shell.
    struct DetailsWidgetHost {
        progress: f32,
        open: bool,
        focus: gpui::FocusHandle,
    }
    impl Render for DetailsWidgetHost {
        fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            let width = f32::from(window.viewport_size().width);
            let target = if self.open {
                (width - 980.0).clamp(0.0, 600.0)
            } else {
                0.0
            };
            let evaluated = motion::lerp(0.0, 460.0, motion::RESIZE.curve.eval(self.progress));
            let painted = painted_details_width(target, || evaluated);
            let placement =
                voice_widget_placement(true, width, native_right_inset(true, 480.0, painted));
            let commands: VoiceCommands = Rc::new(|_, _, _| {});
            div()
                .relative()
                .size_full()
                .child(
                    div()
                        .size_full()
                        .flex()
                        .child(div().w(px(260.0)).flex_none())
                        .child(div().flex_1().min_w_0())
                        .child(
                            div()
                                .debug_selector(|| "fixture-page".into())
                                .w(px(480.0))
                                .h_full()
                                .flex_none(),
                        )
                        .child(
                            div()
                                .debug_selector(|| "fixture-details".into())
                                .w(px(painted))
                                .h_full()
                                .flex_none(),
                        ),
                )
                .child(voice_widget(
                    VoiceWidgetProps {
                        muted: false,
                        can_mute: true,
                        revealed: true,
                        orb: None,
                        focus: self.focus.clone(),
                        right: placement.right,
                    },
                    &commands,
                    cx,
                ))
        }
    }

    #[gpui::test]
    fn opening_details_resize_keeps_actual_widget_controls_clear(cx: &mut TestAppContext) {
        let data = tempfile::tempdir().unwrap();
        init_keyboard(cx, data.path());
        let (host, cx) = cx.add_window_view(|_, cx| DetailsWidgetHost {
            progress: 0.45,
            open: true,
            focus: cx.focus_handle(),
        });
        cx.simulate_resize(gpui::size(px(1440.0), px(900.0)));
        cx.run_until_parked();
        cx.update(|w, cx| w.draw(cx).clear());
        cx.simulate_resize(gpui::size(px(1280.0), px(780.0)));
        for progress in [0.55, 0.75, 0.9] {
            host.update(cx, |host, cx| {
                host.progress = progress;
                cx.notify();
            });
            cx.run_until_parked();
            cx.update(|w, cx| w.draw(cx).clear());
            let page = cx.debug_bounds("fixture-page").unwrap();
            let details = cx.debug_bounds("fixture-details").unwrap();
            assert!(
                details.size.width > px(300.0),
                "painted opening width exceeds resized target"
            );
            for id in [
                "voice-widget-mic",
                "voice-widget-return",
                "voice-widget-end",
            ] {
                let control = cx.debug_bounds(id).unwrap();
                assert!(
                    control.left() >= px(0.0) && control.right() < page.left(),
                    "{id} at {control:?} overlaps painted page {page:?} at {progress}"
                );
                assert!(control.bottom() <= px(780.0));
            }
        }
        host.update(cx, |host, cx| {
            host.open = false;
            cx.notify();
        });
        cx.run_until_parked();
        cx.update(|w, cx| w.draw(cx).clear());
        assert_eq!(
            cx.debug_bounds("fixture-details").unwrap().size.width,
            px(0.0)
        );
        let end = cx.debug_bounds("voice-widget-end").unwrap();
        assert!(end.right() < cx.debug_bounds("fixture-page").unwrap().left());
        assert!(
            end.right() > px(700.0),
            "closed target reserves no lingering tween"
        );
    }

    /// GR4: the widget sits beside a native view only when the whole capsule
    /// (154px: mic, return, end, gaps, padding and border) fits inside the
    /// window; one pixel short, it keeps the corner with the native band.
    #[test]
    fn placement_budgets_the_whole_capsule() {
        let inset = 480.0;
        let beside = WIDGET_MARGIN + inset;
        let exact = beside + WIDGET_WIDTH;
        assert_eq!(
            voice_widget_placement(true, exact, inset),
            VoiceWidgetPlacement {
                right: beside,
                native_band: 0.0
            }
        );
        assert_eq!(
            voice_widget_placement(true, exact - 1.0, inset),
            VoiceWidgetPlacement {
                right: WIDGET_MARGIN,
                native_band: VOICE_WIDGET_BAND
            }
        );
        // The old 120px budget let these widths push the mic off-window.
        for width in [beside + 121.0, beside + 153.0] {
            assert_eq!(
                voice_widget_placement(true, width, inset).right,
                WIDGET_MARGIN
            );
        }
        // Wide windows and no native view keep their placements.
        assert_eq!(voice_widget_placement(true, 1600.0, inset).right, beside);
        assert_eq!(
            voice_widget_placement(true, 300.0, 0.0),
            VoiceWidgetPlacement {
                right: WIDGET_MARGIN,
                native_band: 0.0
            }
        );
    }
}
