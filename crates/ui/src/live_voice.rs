use gpui::{Context, Render, SharedString, Window, div, prelude::*, px};

use crate::theme::Theme;

use zeron_proto::{
    LiveVoiceAvailability, LiveVoicePhase, LiveVoiceRole, LiveVoiceState, LiveVoiceTranscript,
    LiveVoiceUnavailableReason,
};

const MAX_CAPTION_BYTES: usize = 64 * 1024;

#[derive(Debug, Clone, PartialEq)]
pub struct LiveVoiceViewModel {
    pub show_microphone: bool,
    pub microphone_enabled: bool,
    pub microphone_tooltip: String,
    pub replaces_editor: bool,
    pub is_error: bool,
    pub status: String,
    pub caption: Option<String>,
    pub caption_role: Option<LiveVoiceRole>,
    pub input_level: f32,
    pub output_level: f32,
    pub muted: bool,
}

impl LiveVoiceViewModel {
    pub fn derive(
        selected_chat_id: Option<&str>,
        availability: Option<&LiveVoiceAvailability>,
        state: &LiveVoiceState,
    ) -> Self {
        let active = state.phase != LiveVoicePhase::Idle;
        let active_for_selected = active && state.chat_id.as_deref() == selected_chat_id;
        let active_for_other = active && !active_for_selected;
        let (microphone_enabled, microphone_tooltip) = if active_for_other {
            (
                false,
                unavailable_message(LiveVoiceUnavailableReason::AnotherLiveCall).to_owned(),
            )
        } else {
            match availability {
                Some(availability) if availability.available && !active => {
                    (true, "Start Live Voice".into())
                }
                Some(availability) => (
                    false,
                    availability
                        .reason
                        .map(unavailable_message)
                        .unwrap_or("Live Voice is unavailable")
                        .to_owned(),
                ),
                None => (false, "Checking Live Voice availability…".into()),
            }
        };
        let status = match state.phase {
            LiveVoicePhase::Idle => "Ready",
            LiveVoicePhase::Connecting => "Connecting…",
            LiveVoicePhase::Listening => "Listening",
            LiveVoicePhase::Speaking => "Speaking",
            LiveVoicePhase::Working => "Working",
            LiveVoicePhase::Muted => "Muted",
            LiveVoicePhase::Stopping => "Ending…",
            LiveVoicePhase::Error => state
                .error
                .as_deref()
                .unwrap_or("Live Voice ended unexpectedly"),
        }
        .to_owned();
        Self {
            show_microphone: selected_chat_id.is_some() && !active_for_selected,
            microphone_enabled,
            microphone_tooltip,
            replaces_editor: active_for_selected,
            is_error: state.phase == LiveVoicePhase::Error,
            caption_role: state.transcript.as_ref().map(|transcript| transcript.role),
            status,
            caption: state
                .transcript
                .as_ref()
                .map(|transcript| transcript.text.clone()),
            input_level: clamp_level(state.input_level),
            output_level: clamp_level(state.output_level),
            muted: state.muted || state.phase == LiveVoicePhase::Muted,
        }
    }
}

pub(crate) fn capture_state(
    spec: &str,
    selected_chat_id: &str,
) -> Option<(LiveVoiceAvailability, LiveVoiceState)> {
    let available = LiveVoiceAvailability {
        available: true,
        reason: None,
    };
    if spec == "inactive" {
        return Some((available, LiveVoiceState::default()));
    }
    if spec == "unsupported" {
        return Some((
            LiveVoiceAvailability {
                available: false,
                reason: Some(LiveVoiceUnavailableReason::UnsupportedOmp),
            },
            LiveVoiceState::default(),
        ));
    }
    let phase = match spec {
        "connecting" => LiveVoicePhase::Connecting,
        "listening" | "other" => LiveVoicePhase::Listening,
        "working" => LiveVoicePhase::Working,
        "speaking" | "long" => LiveVoicePhase::Speaking,
        "muted" => LiveVoicePhase::Muted,
        _ => return None,
    };
    let caption = match spec {
        "connecting" => None,
        "long" => Some(
            "I’m tracing the request across the local runtime, durable queue, and OMP Live session while keeping the conversation transient and bounded."
                .repeat(5),
        ),
        "working" => Some("Inspect the repository and fix the failing build.".into()),
        "speaking" => Some("The requested change is complete and the targeted checks pass.".into()),
        _ => Some("Can you check the current branch?".into()),
    };
    Some((
        available,
        LiveVoiceState {
            chat_id: Some(if spec == "other" {
                "capture-other-chat".into()
            } else {
                selected_chat_id.to_owned()
            }),
            phase,
            transcript: caption.map(|text| LiveVoiceTranscript {
                role: if spec == "listening" || spec == "muted" {
                    LiveVoiceRole::User
                } else {
                    LiveVoiceRole::Assistant
                },
                turn: 1,
                text,
                final_text: spec != "listening",
            }),
            input_level: if spec == "listening" { 0.72 } else { 0.18 },
            output_level: if spec == "speaking" || spec == "long" {
                0.68
            } else {
                0.12
            },
            muted: spec == "muted",
            ..LiveVoiceState::default()
        },
    ))
}

pub fn unavailable_message(reason: LiveVoiceUnavailableReason) -> &'static str {
    match reason {
        LiveVoiceUnavailableReason::RemoteChat => "Open this Chat on its host device",
        LiveVoiceUnavailableReason::NonOmp => "Live Voice is available for OMP Chats",
        LiveVoiceUnavailableReason::Archived => "Unarchive this Chat to use Live Voice",
        LiveVoiceUnavailableReason::ActiveRun => "Stop the coding run before starting Live Voice",
        LiveVoiceUnavailableReason::UnsupportedOmp => "Update OMP to use Live Voice",
        LiveVoiceUnavailableReason::AnotherLiveCall => "End the active Live Voice call first",
    }
}

pub fn coalesce_caption(
    previous: Option<&LiveVoiceTranscript>,
    next: Option<LiveVoiceTranscript>,
) -> Option<LiveVoiceTranscript> {
    let mut next = next?;
    truncate_caption(&mut next.text);
    let Some(previous) =
        previous.filter(|previous| previous.role == next.role && previous.turn == next.turn)
    else {
        return Some(next);
    };
    let mut previous_text = previous.text.clone();
    truncate_caption(&mut previous_text);
    if next.text == previous_text || next.text.starts_with(&previous_text) {
        return Some(next);
    }
    if previous_text.starts_with(&next.text) {
        next.text = previous_text;
        next.final_text |= previous.final_text;
        return Some(next);
    }
    let remaining = MAX_CAPTION_BYTES.saturating_sub(previous_text.len());
    let mut boundary = remaining.min(next.text.len());
    while !next.text.is_char_boundary(boundary) {
        boundary -= 1;
    }
    previous_text.push_str(&next.text[..boundary]);
    next.text = previous_text;
    Some(next)
}

fn truncate_caption(text: &mut String) {
    if text.len() <= MAX_CAPTION_BYTES {
        return;
    }
    let mut boundary = MAX_CAPTION_BYTES;
    while !text.is_char_boundary(boundary) {
        boundary -= 1;
    }
    text.truncate(boundary);
}

fn clamp_level(level: f32) -> f32 {
    if level.is_finite() {
        level.clamp(0.0, 1.0)
    } else {
        0.0
    }
}

pub struct LiveVoiceTooltip {
    text: SharedString,
}

impl LiveVoiceTooltip {
    pub fn new(text: impl Into<SharedString>) -> Self {
        Self { text: text.into() }
    }
}

impl Render for LiveVoiceTooltip {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = Theme::of(cx);
        div()
            .max_w(px(260.0))
            .px(px(10.0))
            .py(px(7.0))
            .rounded(px(8.0))
            .border_1()
            .border_color(theme.border_strong)
            .bg(theme.surface_raised)
            .shadow_md()
            .text_size(px(11.5))
            .line_height(px(15.0))
            .text_color(theme.text)
            .child(self.text.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use zeron_proto::{
        LiveVoiceAvailability, LiveVoicePhase, LiveVoiceRole, LiveVoiceState, LiveVoiceTranscript,
        LiveVoiceUnavailableReason,
    };

    fn availability(
        available: bool,
        reason: Option<LiveVoiceUnavailableReason>,
    ) -> LiveVoiceAvailability {
        LiveVoiceAvailability { available, reason }
    }

    fn state(phase: LiveVoicePhase, chat_id: Option<&str>) -> LiveVoiceState {
        LiveVoiceState {
            chat_id: chat_id.map(str::to_owned),
            phase,
            ..LiveVoiceState::default()
        }
    }

    #[test]
    fn live_voice_available_local_omp_chat_enables_microphone() {
        let availability = availability(true, None);
        let state = LiveVoiceState::default();
        let model = LiveVoiceViewModel::derive(Some("chat-1"), Some(&availability), &state);

        assert!(model.show_microphone);
        assert!(model.microphone_enabled);
        assert_eq!(model.microphone_tooltip, "Start Live Voice");
        assert!(!model.replaces_editor);
    }

    #[test]
    fn live_voice_unavailable_reasons_are_actionable() {
        let cases = [
            (
                LiveVoiceUnavailableReason::RemoteChat,
                "Open this Chat on its host device",
            ),
            (
                LiveVoiceUnavailableReason::NonOmp,
                "Live Voice is available for OMP Chats",
            ),
            (
                LiveVoiceUnavailableReason::Archived,
                "Unarchive this Chat to use Live Voice",
            ),
            (
                LiveVoiceUnavailableReason::ActiveRun,
                "Stop the coding run before starting Live Voice",
            ),
            (
                LiveVoiceUnavailableReason::UnsupportedOmp,
                "Update OMP to use Live Voice",
            ),
            (
                LiveVoiceUnavailableReason::AnotherLiveCall,
                "End the active Live Voice call first",
            ),
        ];

        for (reason, expected) in cases {
            assert_eq!(unavailable_message(reason), expected);
            let availability = availability(false, Some(reason));
            let model = LiveVoiceViewModel::derive(
                Some("chat-1"),
                Some(&availability),
                &LiveVoiceState::default(),
            );
            assert!(!model.microphone_enabled);
            assert_eq!(model.microphone_tooltip, expected);
        }
    }

    #[test]
    fn live_voice_active_other_chat_disables_start() {
        let availability = availability(true, None);
        let model = LiveVoiceViewModel::derive(
            Some("chat-2"),
            Some(&availability),
            &state(LiveVoicePhase::Listening, Some("chat-1")),
        );

        assert!(!model.microphone_enabled);
        assert_eq!(
            model.microphone_tooltip,
            "End the active Live Voice call first"
        );
        assert!(!model.replaces_editor);
    }

    #[test]
    fn live_voice_phase_labels_and_active_surface_are_stable() {
        for (phase, expected) in [
            (LiveVoicePhase::Working, "Working"),
            (LiveVoicePhase::Speaking, "Speaking"),
            (LiveVoicePhase::Muted, "Muted"),
        ] {
            let model =
                LiveVoiceViewModel::derive(Some("chat-1"), None, &state(phase, Some("chat-1")));
            assert_eq!(model.status, expected);
            assert!(model.replaces_editor);
        }
    }

    #[test]
    fn live_voice_caption_coalesces_same_role_and_turn() {
        let first = LiveVoiceTranscript {
            role: LiveVoiceRole::User,
            turn: 7,
            text: "Inspect".into(),
            final_text: false,
        };
        let delta = LiveVoiceTranscript {
            role: LiveVoiceRole::User,
            turn: 7,
            text: " auth".into(),
            final_text: true,
        };
        assert_eq!(
            coalesce_caption(Some(&first), Some(delta)).unwrap().text,
            "Inspect auth"
        );

        let next_turn = LiveVoiceTranscript {
            role: LiveVoiceRole::Assistant,
            turn: 8,
            text: "Done".into(),
            final_text: true,
        };
        assert_eq!(
            coalesce_caption(Some(&first), Some(next_turn))
                .unwrap()
                .text,
            "Done"
        );
    }

    #[test]
    fn live_voice_caption_buffer_is_bounded() {
        let transcript = LiveVoiceTranscript {
            role: LiveVoiceRole::Assistant,
            turn: 9,
            text: "é".repeat(MAX_CAPTION_BYTES),
            final_text: false,
        };

        let bounded = coalesce_caption(None, Some(transcript)).unwrap();
        assert!(bounded.text.len() <= MAX_CAPTION_BYTES);
        assert!(bounded.text.is_char_boundary(bounded.text.len()));
    }

    #[test]
    fn live_voice_levels_clamp_before_rendering() {
        let mut live = state(LiveVoicePhase::Speaking, Some("chat-1"));
        live.input_level = -0.5;
        live.output_level = 1.5;
        let model = LiveVoiceViewModel::derive(Some("chat-1"), None, &live);

        assert_eq!(model.input_level, 0.0);
        assert_eq!(model.output_level, 1.0);
    }

    #[test]
    fn live_voice_capture_states_cover_visual_matrix() {
        for (spec, phase) in [
            ("connecting", LiveVoicePhase::Connecting),
            ("listening", LiveVoicePhase::Listening),
            ("working", LiveVoicePhase::Working),
            ("speaking", LiveVoicePhase::Speaking),
            ("muted", LiveVoicePhase::Muted),
            ("long", LiveVoicePhase::Speaking),
        ] {
            let (availability, live) = capture_state(spec, "chat-1").expect("known capture state");
            assert!(availability.available);
            assert_eq!(live.phase, phase);
            assert_eq!(live.chat_id.as_deref(), Some("chat-1"));
        }

        let (availability, live) =
            capture_state("unsupported", "chat-1").expect("unsupported capture");
        assert!(!availability.available);
        assert_eq!(
            availability.reason,
            Some(LiveVoiceUnavailableReason::UnsupportedOmp)
        );
        assert_eq!(live.phase, LiveVoicePhase::Idle);

        let (_, live) = capture_state("other", "chat-1").expect("other-chat capture");
        assert_ne!(live.chat_id.as_deref(), Some("chat-1"));
        assert!(capture_state("unknown", "chat-1").is_none());
    }
}
