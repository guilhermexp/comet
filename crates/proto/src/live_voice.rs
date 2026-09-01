use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum LiveVoicePhase {
    Idle,
    Connecting,
    Listening,
    Speaking,
    Working,
    Muted,
    Stopping,
    Error,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum LiveVoiceRole {
    User,
    Assistant,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum LiveVoiceUnavailableReason {
    RemoteChat,
    NonOmp,
    Archived,
    ActiveRun,
    UnsupportedOmp,
    AnotherLiveCall,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LiveVoiceTranscript {
    pub role: LiveVoiceRole,
    pub turn: u64,
    pub text: String,
    pub final_text: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LiveVoiceState {
    pub chat_id: Option<String>,
    pub phase: LiveVoicePhase,
    pub muted: bool,
    pub input_level: f32,
    pub output_level: f32,
    pub transcript: Option<LiveVoiceTranscript>,
    pub error: Option<String>,
}

impl Default for LiveVoiceState {
    fn default() -> Self {
        Self {
            chat_id: None,
            phase: LiveVoicePhase::Idle,
            muted: false,
            input_level: 0.0,
            output_level: 0.0,
            transcript: None,
            error: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LiveVoiceAvailability {
    pub available: bool,
    pub reason: Option<LiveVoiceUnavailableReason>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn live_voice_state_serializes_camel_case() {
        let state = LiveVoiceState {
            chat_id: Some("chat-1".into()),
            phase: LiveVoicePhase::Working,
            muted: false,
            input_level: 0.25,
            output_level: 0.5,
            transcript: Some(LiveVoiceTranscript {
                role: LiveVoiceRole::User,
                turn: 2,
                text: "Inspect auth".into(),
                final_text: true,
            }),
            error: None,
        };

        let value = serde_json::to_value(state).unwrap();
        assert_eq!(value["chatId"], "chat-1");
        assert_eq!(value["phase"], "working");
        assert_eq!(value["transcript"]["finalText"], true);
    }

    #[test]
    fn live_voice_unavailable_reasons_round_trip_lower_camel_case() {
        let cases = [
            (LiveVoiceUnavailableReason::RemoteChat, "remoteChat"),
            (LiveVoiceUnavailableReason::NonOmp, "nonOmp"),
            (LiveVoiceUnavailableReason::Archived, "archived"),
            (LiveVoiceUnavailableReason::ActiveRun, "activeRun"),
            (LiveVoiceUnavailableReason::UnsupportedOmp, "unsupportedOmp"),
            (
                LiveVoiceUnavailableReason::AnotherLiveCall,
                "anotherLiveCall",
            ),
        ];

        for (reason, expected) in cases {
            let json = serde_json::to_string(&reason).unwrap();
            assert_eq!(json, format!("\"{expected}\""));
            assert_eq!(
                serde_json::from_str::<LiveVoiceUnavailableReason>(&json).unwrap(),
                reason
            );
        }
    }

    #[test]
    fn live_voice_state_defaults_to_idle() {
        assert_eq!(
            LiveVoiceState::default(),
            LiveVoiceState {
                chat_id: None,
                phase: LiveVoicePhase::Idle,
                muted: false,
                input_level: 0.0,
                output_level: 0.0,
                transcript: None,
                error: None,
            }
        );
    }
}
