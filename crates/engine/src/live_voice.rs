use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use tokio::sync::{mpsc, watch};
use tokio::task::JoinHandle;
use zeron_doc::{MessagePart, MessageRole, SessionMessageEntry};
use zeron_harness::{CancellationToken, LiveVoiceControl, LiveVoiceEvent};
use zeron_proto::view::tool_presentation;
use zeron_proto::{AgentEvent, DoneStatus, LiveVoicePhase, LiveVoiceState, SessionStatus};

use crate::{EngineError, new_id};

pub const MAX_LIVE_TEXT_BYTES: usize = 64 * 1024;
const LIVE_STOP_TIMEOUT: Duration = Duration::from_secs(2);
/// Minimum spacing between text-only context updates. OMP streams a turn as
/// `TextDelta`s with no message-boundary event, so streamed text has to reach
/// the voice model on a clock; one second is slow enough that a long answer
/// costs a few dozen updates instead of one per token, and fast enough that
/// the voice model never talks about a paragraph the coding run has moved past.
pub(crate) const LIVE_TEXT_UPDATE_INTERVAL: Duration = Duration::from_secs(1);
/// The operational context carries only the tail of the current turn's text,
/// so each update is bounded by this window rather than by everything streamed
/// so far: an N-byte answer costs O(N) bytes over its updates, not O(N²).
pub(crate) const LIVE_VISIBLE_TEXT_WINDOW_BYTES: usize = 2 * 1024;

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum BackendSpeechUpdate {
    None,
    Progress(String),
    Final(String),
}

/// Speech for one delegation. Stays disarmed until the event that opens the
/// delegated turn (`Steered` on the steer path, `SessionStarted` on the
/// new-turn fallback): anything before it, including the in-flight turn's
/// `Done`, belongs to a previous turn and is not this delegation's result.
#[derive(Default)]
pub(crate) struct BackendSpeechAccumulator {
    armed: bool,
    current: String,
    last_segment: Option<String>,
}

impl BackendSpeechAccumulator {
    pub(crate) fn observe(&mut self, event: &AgentEvent) -> BackendSpeechUpdate {
        if !self.armed {
            if matches!(
                event,
                AgentEvent::Steered { .. } | AgentEvent::SessionStarted { .. }
            ) {
                self.armed = true;
                self.current.clear();
                self.last_segment = None;
            }
            return BackendSpeechUpdate::None;
        }
        match event {
            AgentEvent::TextDelta { text } => {
                push_bounded(&mut self.current, text);
                BackendSpeechUpdate::None
            }
            AgentEvent::AssistantMessageCompleted { .. } => {
                let Some(segment) = take_trimmed(&mut self.current) else {
                    return BackendSpeechUpdate::None;
                };
                self.last_segment = Some(segment.clone());
                BackendSpeechUpdate::Progress(segment)
            }
            AgentEvent::Done {
                status,
                result,
                error,
                ..
            } => {
                if let Some(segment) = take_trimmed(&mut self.current) {
                    self.last_segment = Some(segment);
                }
                let final_text = if *status == DoneStatus::Errored {
                    non_empty(error)
                        .or_else(|| non_empty(result))
                        .unwrap_or_else(|| "The coding run failed without an error message.".into())
                } else {
                    non_empty(result)
                        .or_else(|| self.last_segment.clone())
                        .unwrap_or_else(|| match status {
                            DoneStatus::Completed => {
                                "The coding run completed without a final text response.".into()
                            }
                            DoneStatus::Interrupted => "The coding run was interrupted.".into(),
                            DoneStatus::Errored => unreachable!(),
                        })
                };
                BackendSpeechUpdate::Final(truncate_live_text(final_text))
            }
            _ => BackendSpeechUpdate::None,
        }
    }
}

pub(crate) struct LiveOperationalContext {
    status: SessionStatus,
    visible_text: String,
    active_tool: Option<(String, &'static str)>,
    visible_error: Option<String>,
    last_text_update: Option<Instant>,
}

impl LiveOperationalContext {
    pub(crate) fn new(status: SessionStatus, visible_text: &str) -> Self {
        let mut context = Self {
            status,
            visible_text: String::new(),
            active_tool: None,
            visible_error: None,
            last_text_update: None,
        };
        context.push_visible_text(visible_text);
        context
    }

    fn push_visible_text(&mut self, text: &str) {
        self.visible_text.push_str(text);
        let excess = self
            .visible_text
            .len()
            .saturating_sub(LIVE_VISIBLE_TEXT_WINDOW_BYTES);
        if excess > 0 {
            let mut boundary = excess;
            while !self.visible_text.is_char_boundary(boundary) {
                boundary += 1;
            }
            self.visible_text.drain(..boundary);
        }
    }

    /// Latest-value status transition; reports whether it actually moved.
    fn set_status(&mut self, next: SessionStatus) -> bool {
        let changed = self.status != next;
        self.status = next;
        changed
    }

    /// Turn boundary: the visible text describes the current turn only.
    fn clear_visible_text(&mut self) -> bool {
        let had_text = !self.visible_text.trim().is_empty();
        self.visible_text.clear();
        had_text
    }

    pub(crate) fn observe(&mut self, event: &AgentEvent) -> bool {
        self.observe_at(event, Instant::now())
    }

    /// Reports whether the rendered context should be republished. Boundaries
    /// (status, tool, error, message end, turn start) publish at once; streamed
    /// text publishes on the [`LIVE_TEXT_UPDATE_INTERVAL`] clock.
    pub(crate) fn observe_at(&mut self, event: &AgentEvent, now: Instant) -> bool {
        let changed = self.apply(event, now);
        if changed {
            self.last_text_update = Some(now);
        }
        changed
    }

    fn apply(&mut self, event: &AgentEvent, now: Instant) -> bool {
        match event {
            AgentEvent::SessionStarted { .. } => {
                let had_tool = self.active_tool.take().is_some();
                let had_error = self.visible_error.take().is_some();
                let had_text = self.clear_visible_text();
                self.set_status(SessionStatus::Working) || had_tool || had_error || had_text
            }
            AgentEvent::TextDelta { text } => {
                self.push_visible_text(text);
                !self.visible_text.trim().is_empty()
                    && self
                        .last_text_update
                        .is_none_or(|last| now.duration_since(last) >= LIVE_TEXT_UPDATE_INTERVAL)
            }
            AgentEvent::AssistantMessageCompleted { .. } => !self.visible_text.trim().is_empty(),
            AgentEvent::ToolCall { id, call } | AgentEvent::ToolCallPreview { id, call } => {
                let label = tool_presentation(call, false, false).label;
                let next = (id.clone(), label);
                if self.active_tool.as_ref() == Some(&next) {
                    false
                } else {
                    self.active_tool = Some(next);
                    true
                }
            }
            AgentEvent::ToolResult { id, is_error, .. } => {
                let cleared = self
                    .active_tool
                    .as_ref()
                    .is_some_and(|(active_id, _)| active_id == id);
                if cleared {
                    self.active_tool = None;
                }
                let next_error = is_error.then(|| "The current tool failed.".to_owned());
                let error_changed = self.visible_error != next_error;
                self.visible_error = next_error;
                cleared || error_changed
            }
            AgentEvent::InputRequested { .. } => self.set_status(SessionStatus::AwaitingInput),
            AgentEvent::InputResolved { .. } => self.set_status(SessionStatus::Working),
            AgentEvent::Steered { .. } => {
                let had_text = self.clear_visible_text();
                self.set_status(SessionStatus::Working) || had_text
            }
            AgentEvent::Error { message } => {
                let next = truncate_live_text(message.trim().to_owned());
                if self.visible_error.as_deref() == Some(next.as_str()) {
                    false
                } else {
                    self.visible_error = Some(next);
                    true
                }
            }
            AgentEvent::Done {
                status,
                result,
                error,
                ..
            } => {
                let next_status = if *status == DoneStatus::Errored {
                    SessionStatus::Errored
                } else {
                    SessionStatus::Idle
                };
                let next_error = if *status == DoneStatus::Errored {
                    non_empty(error)
                        .or_else(|| non_empty(result))
                        .or_else(|| Some("The coding run failed without an error message.".into()))
                } else {
                    None
                };
                let changed = self.status != next_status
                    || self.active_tool.is_some()
                    || self.visible_error != next_error;
                self.status = next_status;
                self.active_tool = None;
                self.visible_error = next_error;
                changed
            }
            _ => false,
        }
    }

    pub(crate) fn render(&self) -> String {
        let mut output = format!("Session status: {}", session_status_label(self.status));
        if let Some((_, label)) = self.active_tool.as_ref() {
            output.push_str("\nCurrent action: ");
            output.push_str(label);
        }
        let visible_text = self.visible_text.trim();
        if !visible_text.is_empty() {
            output.push_str("\nVisible assistant update: ");
            output.push_str(visible_text);
        }
        if let Some(error) = self
            .visible_error
            .as_deref()
            .map(str::trim)
            .filter(|v| !v.is_empty())
        {
            output.push_str("\nVisible error: ");
            output.push_str(error);
        }
        truncate_live_text(output)
    }
}

pub(crate) fn latest_visible_assistant_text(entries: &[SessionMessageEntry]) -> String {
    let mut text = String::new();
    for entry in entries
        .iter()
        .rev()
        .filter(|entry| entry.role == MessageRole::Assistant)
    {
        text.clear();
        for part in &entry.parts {
            if let MessagePart::Text { text: value, .. } = part {
                push_bounded(&mut text, value);
            }
        }
        if !text.trim().is_empty() {
            return text;
        }
    }
    text
}

fn session_status_label(status: SessionStatus) -> &'static str {
    match status {
        SessionStatus::Idle => "Idle",
        SessionStatus::Working => "Working",
        SessionStatus::AwaitingInput => "AwaitingInput",
        SessionStatus::Errored => "Errored",
    }
}

fn push_bounded(target: &mut String, value: &str) -> bool {
    let remaining = MAX_LIVE_TEXT_BYTES.saturating_sub(target.len());
    if remaining == 0 {
        return false;
    }
    let mut boundary = remaining.min(value.len());
    while !value.is_char_boundary(boundary) {
        boundary -= 1;
    }
    target.push_str(&value[..boundary]);
    boundary != 0
}

fn take_trimmed(value: &mut String) -> Option<String> {
    let trimmed = value.trim();
    let result = (!trimmed.is_empty()).then(|| trimmed.to_owned());
    value.clear();
    result
}

fn non_empty(value: &Option<String>) -> Option<String> {
    value
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

pub(crate) struct LiveDelegationOwnership {
    pub(crate) command_id: String,
    pub(crate) message_id: String,
    pub(crate) newly_created: bool,
    pub(crate) cancellation: CancellationToken,
}

struct ActiveLiveVoice {
    call_id: String,
    chat_id: String,
    controls: Option<mpsc::Sender<LiveVoiceControl>>,
    phase_before_mute: LiveVoicePhase,
    active_delegation_id: Option<String>,
    owned_command_id: Option<String>,
    owned_message_id: Option<String>,
    delegation_cancellation: Option<CancellationToken>,
    cancellation: CancellationToken,
    task: Option<JoinHandle<()>>,
}

struct CoordinatorInner {
    state: watch::Sender<LiveVoiceState>,
    active: Mutex<Option<ActiveLiveVoice>>,
}

#[derive(Clone)]
pub(crate) struct LiveVoiceCoordinator {
    inner: Arc<CoordinatorInner>,
}

impl LiveVoiceCoordinator {
    pub(crate) fn new() -> Self {
        let (state, _) = watch::channel(LiveVoiceState::default());
        Self {
            inner: Arc::new(CoordinatorInner {
                state,
                active: Mutex::new(None),
            }),
        }
    }

    pub(crate) fn watch(&self) -> watch::Receiver<LiveVoiceState> {
        self.inner.state.subscribe()
    }

    pub(crate) fn is_active(&self) -> bool {
        lock(&self.inner.active).is_some()
    }

    pub(crate) fn active_chat_id(&self) -> Option<String> {
        lock(&self.inner.active)
            .as_ref()
            .map(|active| active.chat_id.clone())
    }

    pub(crate) fn reserve(&self, chat_id: &str) -> Result<String, EngineError> {
        let mut active = lock(&self.inner.active);
        if active.is_some() {
            return Err(EngineError::Other(
                "another Live Voice call is already active".into(),
            ));
        }
        let call_id = new_id();
        *active = Some(ActiveLiveVoice {
            call_id: call_id.clone(),
            chat_id: chat_id.to_owned(),
            controls: None,
            phase_before_mute: LiveVoicePhase::Listening,
            active_delegation_id: None,
            owned_command_id: None,
            owned_message_id: None,
            delegation_cancellation: None,
            cancellation: CancellationToken::new(),
            task: None,
        });
        self.inner.state.send_replace(LiveVoiceState {
            chat_id: Some(chat_id.to_owned()),
            phase: LiveVoicePhase::Connecting,
            ..LiveVoiceState::default()
        });
        Ok(call_id)
    }

    pub(crate) fn attach_controls(
        &self,
        call_id: &str,
        controls: mpsc::Sender<LiveVoiceControl>,
    ) -> bool {
        let mut active = lock(&self.inner.active);
        let Some(active) = active.as_mut().filter(|active| active.call_id == call_id) else {
            return false;
        };
        active.controls = Some(controls);
        true
    }

    pub(crate) fn claim_delegation(
        &self,
        call_id: &str,
        delegation_id: &str,
    ) -> Result<LiveDelegationOwnership, EngineError> {
        let mut active = lock(&self.inner.active);
        let active = active
            .as_mut()
            .filter(|active| active.call_id == call_id)
            .ok_or_else(|| EngineError::Other("Live Voice call is no longer active".into()))?;
        match active.active_delegation_id.as_deref() {
            Some(existing) if existing != delegation_id => {
                return Err(EngineError::Other(
                    "another Live Voice delegation is already active".into(),
                ));
            }
            Some(_) => {
                return Ok(LiveDelegationOwnership {
                    command_id: active
                        .owned_command_id
                        .clone()
                        .expect("active delegation owns a command id"),
                    message_id: active
                        .owned_message_id
                        .clone()
                        .expect("active delegation owns a message id"),
                    cancellation: active
                        .delegation_cancellation
                        .as_ref()
                        .expect("active delegation owns a cancellation token")
                        .clone(),
                    newly_created: false,
                });
            }
            None => {}
        }
        let command_id = new_id();
        let message_id = new_id();
        let cancellation = active.cancellation.child_token();
        active.active_delegation_id = Some(delegation_id.to_owned());
        active.owned_command_id = Some(command_id.clone());
        active.owned_message_id = Some(message_id.clone());
        active.delegation_cancellation = Some(cancellation.clone());
        Ok(LiveDelegationOwnership {
            command_id,
            message_id,
            newly_created: true,
            cancellation,
        })
    }

    pub(crate) fn owns_command(&self, chat_id: &str, command_id: &str) -> bool {
        lock(&self.inner.active).as_ref().is_some_and(|active| {
            active.chat_id == chat_id && active.owned_command_id.as_deref() == Some(command_id)
        })
    }

    pub(crate) fn complete_command(&self, chat_id: &str, command_id: &str) {
        let mut active = lock(&self.inner.active);
        let Some(active) = active.as_mut().filter(|active| {
            active.chat_id == chat_id && active.owned_command_id.as_deref() == Some(command_id)
        }) else {
            return;
        };
        clear_delegation(active);
    }

    pub(crate) async fn append_delegation_context(
        &self,
        call_id: &str,
        delegation_id: &str,
        kind: zeron_harness::LiveVoiceContextKind,
        text: String,
    ) -> bool {
        let controls = lock(&self.inner.active)
            .as_ref()
            .filter(|active| {
                active.call_id == call_id
                    && active.active_delegation_id.as_deref() == Some(delegation_id)
            })
            .and_then(|active| active.controls.clone());
        let Some(controls) = controls else {
            return false;
        };
        if controls
            .send(LiveVoiceControl::AppendContext {
                delegation_id: delegation_id.to_owned(),
                kind,
                text: truncate_live_text(text),
            })
            .await
            .is_err()
        {
            self.fail(call_id, "Live Voice control channel closed");
            return false;
        }
        true
    }

    pub(crate) async fn append_session_context(&self, call_id: &str, text: String) -> bool {
        let controls = lock(&self.inner.active)
            .as_ref()
            .filter(|active| active.call_id == call_id)
            .and_then(|active| active.controls.clone());
        let Some(controls) = controls else {
            return false;
        };
        controls
            .send(LiveVoiceControl::AppendSessionContext {
                text: truncate_live_text(text),
            })
            .await
            .is_ok()
    }

    pub(crate) fn complete_delegation(&self, call_id: &str, delegation_id: &str) {
        let mut active = lock(&self.inner.active);
        let Some(active) = active.as_mut().filter(|active| {
            active.call_id == call_id
                && active.active_delegation_id.as_deref() == Some(delegation_id)
        }) else {
            return;
        };
        clear_delegation(active);
    }

    pub(crate) fn cancellation(&self, call_id: &str) -> Option<CancellationToken> {
        lock(&self.inner.active)
            .as_ref()
            .filter(|active| active.call_id == call_id)
            .map(|active| active.cancellation.clone())
    }

    pub(crate) fn attach_task(&self, call_id: &str, task: JoinHandle<()>) {
        let mut active = lock(&self.inner.active);
        if let Some(active) = active.as_mut().filter(|active| active.call_id == call_id) {
            active.task = Some(task);
        } else {
            task.abort();
        }
    }

    pub(crate) fn handle_event(&self, call_id: &str, event: LiveVoiceEvent) {
        match event {
            LiveVoiceEvent::Ended { error: Some(error) } => self.fail(call_id, &error),
            LiveVoiceEvent::Ended { error: None } => self.finish(call_id),
            LiveVoiceEvent::Phase(phase) => {
                let mut active = lock(&self.inner.active);
                let Some(active) = active.as_mut().filter(|active| active.call_id == call_id)
                else {
                    return;
                };
                let mut state = self.inner.state.borrow().clone();
                if state.muted && phase != LiveVoicePhase::Muted {
                    active.phase_before_mute = phase;
                } else {
                    state.phase = phase;
                    if phase == LiveVoicePhase::Muted {
                        state.muted = true;
                    }
                }
                self.inner.state.send_replace(state);
            }
            LiveVoiceEvent::Levels { input, output } => {
                if !self.matches(call_id) {
                    return;
                }
                let mut state = self.inner.state.borrow().clone();
                state.input_level = input;
                state.output_level = output;
                self.inner.state.send_replace(state);
            }
            LiveVoiceEvent::Transcript(mut transcript) => {
                if !self.matches(call_id) {
                    return;
                }
                transcript.text = truncate_live_text(transcript.text);
                let mut state = self.inner.state.borrow().clone();
                state.transcript = Some(transcript);
                self.inner.state.send_replace(state);
            }
            LiveVoiceEvent::Delegation { .. } => {}
        }
    }

    pub(crate) async fn set_muted(&self, muted: bool) -> Result<(), EngineError> {
        let (call_id, controls) = {
            let mut active = lock(&self.inner.active);
            let active = active
                .as_mut()
                .ok_or_else(|| EngineError::Other("Live Voice is not active".into()))?;
            let mut state = self.inner.state.borrow().clone();
            if state.muted == muted {
                return Ok(());
            }
            if muted {
                active.phase_before_mute = state.phase;
                state.phase = LiveVoicePhase::Muted;
            } else {
                state.phase = active.phase_before_mute;
            }
            state.muted = muted;
            self.inner.state.send_replace(state);
            (
                active.call_id.clone(),
                active
                    .controls
                    .clone()
                    .ok_or_else(|| EngineError::Other("Live Voice is still starting".into()))?,
            )
        };
        if controls
            .send(LiveVoiceControl::SetMuted(muted))
            .await
            .is_err()
        {
            self.fail(&call_id, "Live Voice control channel closed");
            return Err(EngineError::Other(
                "Live Voice control channel closed".into(),
            ));
        }
        Ok(())
    }

    pub(crate) async fn stop(&self) -> Result<(), EngineError> {
        let Some(mut active) = lock(&self.inner.active).take() else {
            self.inner.state.send_replace(LiveVoiceState::default());
            return Ok(());
        };
        let mut stopping = self.inner.state.borrow().clone();
        stopping.phase = LiveVoicePhase::Stopping;
        stopping.muted = false;
        stopping.input_level = 0.0;
        stopping.output_level = 0.0;
        self.inner.state.send_replace(stopping);

        let send_result = if let Some(controls) = active.controls.take() {
            controls
                .send(LiveVoiceControl::Stop)
                .await
                .map_err(|_| EngineError::Other("Live Voice control channel closed".into()))
        } else {
            Ok(())
        };
        if let Some(mut task) = active.task.take()
            && tokio::time::timeout(LIVE_STOP_TIMEOUT, &mut task)
                .await
                .is_err()
        {
            active.cancellation.cancel();
            let _ = task.await;
        }
        active.cancellation.cancel();
        self.inner.state.send_replace(LiveVoiceState::default());
        send_result
    }

    pub(crate) fn fail(&self, call_id: &str, message: &str) {
        let mut guard = lock(&self.inner.active);
        let Some(active) = guard.take_if(|active| active.call_id == call_id) else {
            return;
        };
        active.cancellation.cancel();
        let mut state = self.inner.state.borrow().clone();
        state.phase = LiveVoicePhase::Error;
        state.input_level = 0.0;
        state.output_level = 0.0;
        state.error = Some(truncate_live_text(message.to_owned()));
        self.inner.state.send_replace(state);
    }

    fn finish(&self, call_id: &str) {
        let mut guard = lock(&self.inner.active);
        let Some(active) = guard.take_if(|active| active.call_id == call_id) else {
            return;
        };
        active.cancellation.cancel();
        self.inner.state.send_replace(LiveVoiceState::default());
    }

    pub(crate) fn matches(&self, call_id: &str) -> bool {
        lock(&self.inner.active)
            .as_ref()
            .is_some_and(|active| active.call_id == call_id)
    }
}

fn clear_delegation(active: &mut ActiveLiveVoice) {
    if let Some(cancellation) = active.delegation_cancellation.take() {
        cancellation.cancel();
    }
    active.active_delegation_id = None;
    active.owned_command_id = None;
    active.owned_message_id = None;
}

fn truncate_live_text(mut text: String) -> String {
    if text.len() <= MAX_LIVE_TEXT_BYTES {
        return text;
    }
    let mut boundary = MAX_LIVE_TEXT_BYTES;
    while !text.is_char_boundary(boundary) {
        boundary -= 1;
    }
    text.truncate(boundary);
    text
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use futures::{StreamExt, stream::BoxStream};
    use std::sync::{Arc, Mutex};
    use std::time::Duration;
    use tokio::sync::mpsc;
    use zeron_doc::{MessagePart, MessageRole, SessionMessageEntry};
    use zeron_harness::{
        Harness, HarnessError, LiveVoiceControl, LiveVoiceEvent, LiveVoiceHandle, LiveVoiceRequest,
        LiveVoiceSupport, RunControls,
    };
    use zeron_proto::{
        AgentEvent, ChatConfig, DoneStatus, HarnessId, LiveVoicePhase, LiveVoiceRole,
        LiveVoiceTranscript, LiveVoiceUnavailableReason, Model, ReasoningLevel, RunRequest,
        SandboxLevel, SessionStatus, SteeringMode, ToolCall,
    };

    fn steered() -> AgentEvent {
        AgentEvent::Steered {
            assistant_message_id: Some("active-assistant".into()),
            next_assistant_message_id: Some("steered-assistant".into()),
        }
    }

    fn armed_accumulator() -> BackendSpeechAccumulator {
        let mut speech = BackendSpeechAccumulator::default();
        assert_eq!(speech.observe(&steered()), BackendSpeechUpdate::None);
        speech
    }

    #[test]
    fn live_voice_delegation_speech_ignores_the_turn_that_finishes_before_the_steer() {
        let mut speech = BackendSpeechAccumulator::default();
        assert_eq!(
            speech.observe(&AgentEvent::TextDelta {
                text: "previous turn text".into(),
            }),
            BackendSpeechUpdate::None
        );
        assert_eq!(
            speech.observe(&AgentEvent::AssistantMessageCompleted {
                assistant_message_id: "previous-assistant".into(),
            }),
            BackendSpeechUpdate::None
        );
        assert_eq!(
            speech.observe(&AgentEvent::Done {
                status: DoneStatus::Completed,
                result: Some("previous turn result".into()),
                error: None,
                session_id: None,
            }),
            BackendSpeechUpdate::None
        );
        assert_eq!(speech.observe(&steered()), BackendSpeechUpdate::None);
        assert_eq!(
            speech.observe(&AgentEvent::TextDelta {
                text: "delegated answer".into(),
            }),
            BackendSpeechUpdate::None
        );
        assert_eq!(
            speech.observe(&AgentEvent::Done {
                status: DoneStatus::Completed,
                result: None,
                error: None,
                session_id: None,
            }),
            BackendSpeechUpdate::Final("delegated answer".into())
        );

        let mut fallback = BackendSpeechAccumulator::default();
        assert_eq!(
            fallback.observe(&AgentEvent::Done {
                status: DoneStatus::Completed,
                result: Some("previous turn result".into()),
                error: None,
                session_id: None,
            }),
            BackendSpeechUpdate::None
        );
        assert_eq!(
            fallback.observe(&AgentEvent::SessionStarted {
                harness: HarnessId::Omp,
                model: "omp-default".into(),
                tools: Vec::new(),
                cwd: "/tmp".into(),
                session_id: "session".into(),
                assistant_message_id: "assistant".into(),
            }),
            BackendSpeechUpdate::None
        );
        assert_eq!(
            fallback.observe(&AgentEvent::Done {
                status: DoneStatus::Completed,
                result: Some("fallback result".into()),
                error: None,
                session_id: None,
            }),
            BackendSpeechUpdate::Final("fallback result".into())
        );
    }

    #[test]
    fn live_voice_delegation_speech_accumulator_uses_only_visible_backend_text() {
        let mut speech = armed_accumulator();
        assert_eq!(
            speech.observe(&AgentEvent::ReasoningDelta {
                text: "private reasoning".into(),
            }),
            BackendSpeechUpdate::None
        );
        assert_eq!(
            speech.observe(&AgentEvent::Usage {
                input_tokens: 1,
                output_tokens: 1,
                context_usage: None,
            }),
            BackendSpeechUpdate::None
        );
        assert_eq!(
            speech.observe(&AgentEvent::TextDelta {
                text: " Inspecting ".into(),
            }),
            BackendSpeechUpdate::None
        );
        assert_eq!(
            speech.observe(&AgentEvent::AssistantMessageCompleted {
                assistant_message_id: "assistant-1".into(),
            }),
            BackendSpeechUpdate::Progress("Inspecting".into())
        );
        assert_eq!(
            speech.observe(&AgentEvent::AssistantMessageCompleted {
                assistant_message_id: "assistant-1".into(),
            }),
            BackendSpeechUpdate::None
        );
        assert_eq!(
            speech.observe(&AgentEvent::Done {
                status: DoneStatus::Completed,
                result: None,
                error: None,
                session_id: None,
            }),
            BackendSpeechUpdate::Final("Inspecting".into())
        );

        let mut result_wins = armed_accumulator();
        result_wins.observe(&AgentEvent::TextDelta {
            text: "fallback".into(),
        });
        assert_eq!(
            result_wins.observe(&AgentEvent::Done {
                status: DoneStatus::Completed,
                result: Some(" final result ".into()),
                error: None,
                session_id: None,
            }),
            BackendSpeechUpdate::Final("final result".into())
        );

        let mut errored = armed_accumulator();
        assert_eq!(
            errored.observe(&AgentEvent::Done {
                status: DoneStatus::Errored,
                result: Some("ignored".into()),
                error: Some("actual backend error".into()),
                session_id: None,
            }),
            BackendSpeechUpdate::Final("actual backend error".into())
        );

        let mut empty = armed_accumulator();
        assert_eq!(
            empty.observe(&AgentEvent::Done {
                status: DoneStatus::Completed,
                result: None,
                error: None,
                session_id: None,
            }),
            BackendSpeechUpdate::Final(
                "The coding run completed without a final text response.".into()
            )
        );
    }

    #[test]
    fn live_operational_context_exposes_only_visible_status_text_and_tool_label() {
        let mut context = LiveOperationalContext::new(SessionStatus::Working, "Already visible");
        assert!(context.observe(&AgentEvent::ToolCall {
            id: "tool-1".into(),
            call: ToolCall::Exec {
                command: "secret command".into(),
            },
        }));
        assert!(!context.observe(&AgentEvent::TextDelta {
            text: " Tests are running.".into(),
        }));
        assert!(context.observe(&AgentEvent::AssistantMessageCompleted {
            assistant_message_id: "assistant-1".into(),
        }));

        let rendered = context.render();
        assert!(rendered.contains("Session status: Working"));
        assert!(rendered.contains("Current action: Running command"));
        assert!(rendered.contains("Visible assistant update: Already visible Tests are running."));
        assert!(!rendered.contains("secret command"));
    }

    #[test]
    fn live_operational_context_publishes_streamed_text_on_a_clock_without_message_end() {
        let start = Instant::now();
        let mut context = LiveOperationalContext::new(SessionStatus::Idle, "");
        assert!(context.observe_at(&steered(), start));
        let delta = |text: &str| AgentEvent::TextDelta { text: text.into() };
        assert!(!context.observe_at(&delta("First "), start + Duration::from_millis(300)));
        assert!(context.observe_at(&delta("second "), start + LIVE_TEXT_UPDATE_INTERVAL));
        assert!(!context.observe_at(
            &delta("third "),
            start + LIVE_TEXT_UPDATE_INTERVAL + Duration::from_millis(900)
        ));
        assert!(context.observe_at(&delta("fourth"), start + LIVE_TEXT_UPDATE_INTERVAL * 2));
        assert_eq!(
            context.render(),
            "Session status: Working\nVisible assistant update: First second third fourth"
        );

        let mut heavy = LiveOperationalContext::new(SessionStatus::Idle, "");
        assert!(heavy.observe_at(&steered(), start));
        let deltas = 500;
        let step = Duration::from_millis(10);
        let chunk = "streamed text chunk ";
        let mut updates = 0;
        let mut payload = 0;
        for index in 0..deltas {
            let now = start + step * index;
            if heavy.observe_at(&delta(chunk), now) {
                updates += 1;
                payload += heavy.render().len();
            }
        }
        let total_text = chunk.len() * deltas as usize;
        let elapsed_secs = (step * deltas).as_secs_f64();
        assert!(
            updates <= elapsed_secs.ceil() as usize + 1,
            "{updates} updates for {elapsed_secs}s of streaming"
        );
        assert!(
            payload <= updates * (LIVE_VISIBLE_TEXT_WINDOW_BYTES + 64),
            "each update is bounded by the text window, got {payload} bytes"
        );
        assert!(
            payload < total_text * 2,
            "{payload} bytes for {total_text} bytes of text"
        );
        assert!(heavy.render().ends_with(chunk.trim_end()));
        assert!(heavy.render().len() <= LIVE_VISIBLE_TEXT_WINDOW_BYTES + 64);
    }

    #[test]
    fn live_operational_context_resets_visible_text_at_turn_boundaries() {
        let mut context = LiveOperationalContext::new(SessionStatus::Working, "Previous turn");
        assert!(context.observe(&steered()));
        assert_eq!(context.render(), "Session status: Working");
        assert!(!context.observe(&AgentEvent::TextDelta {
            text: "Current turn".into(),
        }));
        assert!(context.observe(&AgentEvent::AssistantMessageCompleted {
            assistant_message_id: "assistant-1".into(),
        }));
        assert_eq!(
            context.render(),
            "Session status: Working\nVisible assistant update: Current turn"
        );
        assert!(context.observe(&AgentEvent::SessionStarted {
            harness: HarnessId::Omp,
            model: "omp-default".into(),
            tools: Vec::new(),
            cwd: "/tmp".into(),
            session_id: "session".into(),
            assistant_message_id: "assistant-2".into(),
        }));
        assert_eq!(context.render(), "Session status: Working");
    }

    #[test]
    fn live_operational_context_excludes_reasoning_and_tool_payloads() {
        let mut context = LiveOperationalContext::new(SessionStatus::Working, "");
        assert!(!context.observe(&AgentEvent::ReasoningDelta {
            text: "private reasoning".into(),
        }));
        assert!(context.observe(&AgentEvent::ToolCall {
            id: "tool-1".into(),
            call: ToolCall::Exec {
                command: "private command".into(),
            },
        }));
        assert!(context.observe(&AgentEvent::ToolResult {
            id: "tool-1".into(),
            is_error: true,
            output: Some("private output".into()),
            diff: None,
            execution: None,
        }));

        let rendered = context.render();
        assert!(rendered.contains("Visible error: The current tool failed."));
        assert!(!rendered.contains("private reasoning"));
        assert!(!rendered.contains("private command"));
        assert!(!rendered.contains("private output"));
    }

    #[test]
    fn live_operational_context_tracks_input_and_terminal_state_without_question_payloads() {
        let mut context = LiveOperationalContext::new(SessionStatus::Working, "");
        assert!(context.observe(&AgentEvent::InputRequested {
            request_id: "input-1".into(),
            questions: Vec::new(),
        }));
        assert_eq!(context.render(), "Session status: AwaitingInput");
        assert!(context.observe(&AgentEvent::Done {
            status: DoneStatus::Errored,
            result: None,
            error: Some("visible failure".into()),
            session_id: None,
        }));
        assert_eq!(
            context.render(),
            "Session status: Errored\nVisible error: visible failure"
        );
    }

    #[test]
    fn latest_live_assistant_text_excludes_non_text_transcript_parts() {
        let entries = vec![
            SessionMessageEntry {
                id: "user".into(),
                role: MessageRole::User,
                parts: vec![MessagePart::Text {
                    id: "user-text".into(),
                    text: "user prompt".into(),
                }],
                created_at: 1,
                device_id: "device".into(),
                status: None,
                duration_ms: None,
                continuation_of: None,
            },
            SessionMessageEntry {
                id: "assistant".into(),
                role: MessageRole::Assistant,
                parts: vec![
                    MessagePart::Reasoning {
                        id: "reasoning".into(),
                        text: "private reasoning".into(),
                        completed: false,
                        duration_ms: None,
                    },
                    MessagePart::Text {
                        id: "answer".into(),
                        text: "Visible answer".into(),
                    },
                ],
                created_at: 2,
                device_id: "device".into(),
                status: None,
                duration_ms: None,
                continuation_of: None,
            },
            SessionMessageEntry {
                id: "assistant-protected".into(),
                role: MessageRole::Assistant,
                parts: vec![MessagePart::Reasoning {
                    id: "newer-reasoning".into(),
                    text: "newer private reasoning".into(),
                    completed: false,
                    duration_ms: None,
                }],
                created_at: 3,
                device_id: "device".into(),
                status: None,
                duration_ms: None,
                continuation_of: None,
            },
        ];

        assert_eq!(latest_visible_assistant_text(&entries), "Visible answer");
    }

    #[test]
    fn terminal_live_command_releases_delegation_without_backend_done() {
        let coordinator = LiveVoiceCoordinator::new();
        let call_id = coordinator.reserve("chat-1").unwrap();
        let first = coordinator
            .claim_delegation(&call_id, "delegation-1")
            .unwrap();

        coordinator.complete_command("chat-1", "different-command");
        assert!(!first.cancellation.is_cancelled());
        assert!(
            coordinator
                .claim_delegation(&call_id, "delegation-2")
                .is_err()
        );

        coordinator.complete_command("chat-1", &first.command_id);
        assert!(first.cancellation.is_cancelled());
        assert!(
            coordinator
                .claim_delegation(&call_id, "delegation-2")
                .is_ok()
        );
    }

    #[tokio::test]
    async fn live_voice_state_tracks_transient_events_and_exact_mute() {
        let coordinator = LiveVoiceCoordinator::new();
        let state = coordinator.watch();
        let (controls, mut received) = mpsc::channel(4);
        let call_id = coordinator.reserve("chat-1").unwrap();
        assert!(coordinator.attach_controls(&call_id, controls));
        assert_eq!(state.borrow().phase, LiveVoicePhase::Connecting);

        coordinator.handle_event(&call_id, LiveVoiceEvent::Phase(LiveVoicePhase::Listening));
        coordinator.handle_event(
            &call_id,
            LiveVoiceEvent::Levels {
                input: 0.25,
                output: 0.5,
            },
        );
        coordinator.handle_event(
            &call_id,
            LiveVoiceEvent::Transcript(LiveVoiceTranscript {
                role: LiveVoiceRole::User,
                turn: 2,
                text: "x".repeat(MAX_LIVE_TEXT_BYTES + 16),
                final_text: true,
            }),
        );
        assert_eq!(state.borrow().phase, LiveVoicePhase::Listening);
        assert_eq!(state.borrow().input_level, 0.25);
        assert_eq!(state.borrow().output_level, 0.5);
        assert_eq!(
            state.borrow().transcript.as_ref().unwrap().text.len(),
            MAX_LIVE_TEXT_BYTES
        );

        coordinator.set_muted(true).await.unwrap();
        assert_eq!(
            received.recv().await,
            Some(LiveVoiceControl::SetMuted(true))
        );
        assert_eq!(state.borrow().phase, LiveVoicePhase::Muted);
        coordinator.set_muted(false).await.unwrap();
        assert_eq!(
            received.recv().await,
            Some(LiveVoiceControl::SetMuted(false))
        );
        assert_eq!(state.borrow().phase, LiveVoicePhase::Listening);
        assert!(coordinator.reserve("chat-2").is_err());
    }

    #[tokio::test]
    async fn live_voice_state_stop_is_ordered_and_idempotent() {
        let coordinator = LiveVoiceCoordinator::new();
        let mut state = coordinator.watch();
        let (controls, mut received) = mpsc::channel(1);
        controls
            .send(LiveVoiceControl::SetMuted(false))
            .await
            .unwrap();
        let call_id = coordinator.reserve("chat-1").unwrap();
        assert!(coordinator.attach_controls(&call_id, controls));
        let cancellation = coordinator.cancellation(&call_id).unwrap();
        let child = cancellation.child_token();
        state.borrow_and_update();

        let stopping = {
            let coordinator = coordinator.clone();
            tokio::spawn(async move { coordinator.stop().await })
        };
        state.changed().await.unwrap();
        assert_eq!(state.borrow().phase, LiveVoicePhase::Stopping);
        assert_eq!(
            received.recv().await,
            Some(LiveVoiceControl::SetMuted(false))
        );
        assert_eq!(received.recv().await, Some(LiveVoiceControl::Stop));
        stopping.await.unwrap().unwrap();
        assert_eq!(state.borrow().phase, LiveVoicePhase::Idle);
        assert!(cancellation.is_cancelled());
        assert!(child.is_cancelled());
        coordinator.stop().await.unwrap();
        assert_eq!(state.borrow().phase, LiveVoicePhase::Idle);
    }

    #[test]
    fn live_voice_state_terminal_error_clears_ownership_once() {
        let coordinator = LiveVoiceCoordinator::new();
        let mut state = coordinator.watch();
        let call_id = coordinator.reserve("chat-1").unwrap();
        coordinator.fail(&call_id, "transport failed");
        assert_eq!(state.borrow().phase, LiveVoicePhase::Error);
        assert_eq!(state.borrow().error.as_deref(), Some("transport failed"));
        assert!(!coordinator.is_active());
        state.borrow_and_update();
        coordinator.fail(&call_id, "duplicate");
        assert!(!state.has_changed().unwrap());
    }

    #[derive(Default)]
    struct FakeLiveHarness {
        supported: std::sync::atomic::AtomicBool,
        session_context_supported: std::sync::atomic::AtomicBool,
        controls: Arc<Mutex<Vec<LiveVoiceControl>>>,
        live_requests: Mutex<Vec<LiveVoiceRequest>>,
        run_requests: Mutex<Vec<RunRequest>>,
    }

    impl FakeLiveHarness {
        fn supported() -> Self {
            Self {
                supported: std::sync::atomic::AtomicBool::new(true),
                session_context_supported: std::sync::atomic::AtomicBool::new(true),
                controls: Arc::new(Mutex::new(Vec::new())),
                live_requests: Mutex::new(Vec::new()),
                run_requests: Mutex::new(Vec::new()),
            }
        }

        fn set_supported(&self, supported: bool) {
            self.supported
                .store(supported, std::sync::atomic::Ordering::SeqCst);
        }

        fn set_session_context_supported(&self, supported: bool) {
            self.session_context_supported
                .store(supported, std::sync::atomic::Ordering::SeqCst);
        }
    }

    #[async_trait]
    impl Harness for FakeLiveHarness {
        fn id(&self) -> HarnessId {
            HarnessId::Omp
        }

        fn display_name(&self) -> &str {
            "Fake OMP"
        }

        fn supports_steering(&self) -> bool {
            true
        }

        fn steering_mode(&self) -> SteeringMode {
            SteeringMode::StepBoundary
        }

        fn reasoning_levels(&self) -> &[ReasoningLevel] {
            &[ReasoningLevel::Medium]
        }

        async fn models(&self) -> Result<Vec<Model>, HarnessError> {
            Ok(Vec::new())
        }

        async fn probe_live_voice(
            &self,
            _cwd: &std::path::Path,
        ) -> Result<LiveVoiceSupport, HarnessError> {
            let available = self.supported.load(std::sync::atomic::Ordering::SeqCst);
            Ok(LiveVoiceSupport {
                available,
                session_context: self
                    .session_context_supported
                    .load(std::sync::atomic::Ordering::SeqCst),
            })
        }

        async fn start_live_voice(
            &self,
            request: LiveVoiceRequest,
        ) -> Result<LiveVoiceHandle, HarnessError> {
            if !self.supported.load(std::sync::atomic::Ordering::SeqCst) {
                return Err(HarnessError::Unsupported("update OMP".into()));
            }
            lock(&self.live_requests).push(request);
            let (control_tx, mut control_rx) = mpsc::channel::<LiveVoiceControl>(16);
            let (event_tx, event_rx) = mpsc::channel::<Result<LiveVoiceEvent, HarnessError>>(16);
            let seen = Arc::clone(&self.controls);
            tokio::spawn(async move {
                let _ = event_tx
                    .send(Ok(LiveVoiceEvent::Phase(LiveVoicePhase::Connecting)))
                    .await;
                let _ = event_tx
                    .send(Ok(LiveVoiceEvent::Phase(LiveVoicePhase::Listening)))
                    .await;
                let _ = event_tx
                    .send(Ok(LiveVoiceEvent::Levels {
                        input: 0.2,
                        output: 0.4,
                    }))
                    .await;
                let _ = event_tx
                    .send(Ok(LiveVoiceEvent::Transcript(LiveVoiceTranscript {
                        role: LiveVoiceRole::Assistant,
                        turn: 1,
                        text: "Ready".into(),
                        final_text: true,
                    })))
                    .await;
                while let Some(control) = control_rx.recv().await {
                    lock(&seen).push(control.clone());
                    match control {
                        LiveVoiceControl::SetMuted(muted) => {
                            let phase = if muted {
                                LiveVoicePhase::Muted
                            } else {
                                LiveVoicePhase::Listening
                            };
                            let _ = event_tx.send(Ok(LiveVoiceEvent::Phase(phase))).await;
                        }
                        LiveVoiceControl::AppendSessionContext { .. } => {}
                        LiveVoiceControl::Stop => {
                            let _ = event_tx
                                .send(Ok(LiveVoiceEvent::Ended { error: None }))
                                .await;
                            break;
                        }
                        LiveVoiceControl::AppendContext { .. } => {}
                    }
                }
            });
            Ok(LiveVoiceHandle {
                session_id: "/tmp/live-created.jsonl".into(),
                events: futures::stream::unfold(event_rx, |mut receiver| async move {
                    receiver.recv().await.map(|event| (event, receiver))
                })
                .boxed(),
                controls: control_tx,
            })
        }

        async fn run(
            &self,
            request: RunRequest,
            controls: RunControls,
        ) -> Result<BoxStream<'static, Result<AgentEvent, HarnessError>>, HarnessError> {
            let await_input = request.prompt == "await-input";
            lock(&self.run_requests).push(request);
            let (tx, rx) = mpsc::channel(4);
            tokio::spawn(async move {
                let _ = tx
                    .send(Ok(AgentEvent::SessionStarted {
                        harness: HarnessId::Omp,
                        model: "fake".into(),
                        tools: Vec::new(),
                        cwd: "/tmp".into(),
                        session_id: "fake-session".into(),
                        assistant_message_id: "assistant-1".into(),
                    }))
                    .await;
                if await_input {
                    let answers = (controls.request_input)(vec![zeron_proto::UserInputQuestion {
                        id: "q1".into(),
                        header: "Pick".into(),
                        question: "Private question?".into(),
                        options: vec!["a".into(), "b".into()],
                        multi_select: false,
                    }]);
                    tokio::pin!(answers);
                    tokio::select! {
                        _ = &mut answers => {}
                        _ = controls.interrupt.cancelled() => {}
                    }
                } else {
                    controls.interrupt.cancelled().await;
                }
                let _ = tx
                    .send(Ok(AgentEvent::Done {
                        status: DoneStatus::Interrupted,
                        result: None,
                        error: None,
                        session_id: Some("fake-session".into()),
                    }))
                    .await;
            });
            Ok(futures::stream::unfold(rx, |mut receiver| async move {
                receiver.recv().await.map(|event| (event, receiver))
            })
            .boxed())
        }
    }

    fn omp_config() -> ChatConfig {
        ChatConfig {
            harness: HarnessId::Omp,
            model: None,
            reasoning: None,
            model_options: Default::default(),
            sandbox: SandboxLevel::WorkspaceWrite,
        }
    }

    fn run_request() -> RunRequest {
        RunRequest {
            prompt: "busy".into(),
            harness: Some(HarnessId::Omp),
            model: None,
            reasoning: None,
            model_options: Default::default(),
            cwd: "/tmp".into(),
            sandbox: SandboxLevel::WorkspaceWrite,
            auto_approve: true,
            enable_workers_mcp: false,
            workers_parent_chat_id: None,
            resume: None,
            attachments: Vec::new(),
            worktree: None,
        }
    }

    async fn wait_for_phase(engine: &crate::sessions::SessionsEngine, phase: LiveVoicePhase) {
        let mut state = engine.watch_live_voice();
        tokio::time::timeout(Duration::from_secs(2), async {
            loop {
                if state.borrow().phase == phase {
                    return;
                }
                state.changed().await.unwrap();
            }
        })
        .await
        .unwrap();
    }

    /// Polls a condition on the same 5ms/1s budget every operational-context
    /// assertion in this module needs.
    async fn wait_until(label: &str, mut condition: impl FnMut() -> bool) {
        tokio::time::timeout(Duration::from_secs(1), async {
            while !condition() {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .unwrap_or_else(|_| panic!("timed out waiting for {label}"));
    }

    #[tokio::test]
    async fn live_voice_preconditions_and_transient_lifecycle() {
        let temp = tempfile::tempdir().unwrap();
        let harness = Arc::new(FakeLiveHarness::supported());
        let registry = crate::registry::HarnessRegistry::new();
        registry.register(harness.clone());
        let core =
            crate::EngineCore::assemble(temp.path(), Arc::new(registry), HarnessId::Omp, None)
                .unwrap();
        let local_device = core.device_id.clone();

        for (id, device, config) in [
            ("remote", "other-device", Some(omp_config())),
            (
                "non-omp",
                local_device.as_str(),
                Some(ChatConfig {
                    harness: HarnessId::Mock,
                    ..omp_config()
                }),
            ),
            ("archived", local_device.as_str(), Some(omp_config())),
            ("active", local_device.as_str(), Some(omp_config())),
            ("awaiting", local_device.as_str(), Some(omp_config())),
            ("live-a", local_device.as_str(), Some(omp_config())),
            ("live-b", local_device.as_str(), Some(omp_config())),
        ] {
            core.workspace
                .create_chat(id, None, Some(device), config, Some("/tmp".into()))
                .unwrap();
        }
        core.workspace.set_chat_archived("archived", true).unwrap();

        assert_eq!(
            core.sessions
                .probe_live_voice("remote")
                .await
                .unwrap()
                .reason,
            Some(LiveVoiceUnavailableReason::RemoteChat)
        );
        assert_eq!(
            core.sessions
                .probe_live_voice("non-omp")
                .await
                .unwrap()
                .reason,
            Some(LiveVoiceUnavailableReason::NonOmp)
        );
        assert_eq!(
            core.sessions
                .probe_live_voice("archived")
                .await
                .unwrap()
                .reason,
            Some(LiveVoiceUnavailableReason::Archived)
        );

        core.sessions
            .dispatch(
                "active",
                HarnessId::Omp,
                run_request(),
                Some("busy-1".into()),
            )
            .await
            .unwrap();
        assert!(
            core.sessions
                .probe_live_voice("active")
                .await
                .unwrap()
                .available
        );
        core.sessions.start_live_voice("active").await.unwrap();
        wait_for_phase(&core.sessions, LiveVoicePhase::Listening).await;
        wait_until("Working operational context", || {
            lock(&harness.controls).iter().any(|control| {
                matches!(
                    control,
                    LiveVoiceControl::AppendSessionContext { text }
                        if text.contains("Session status: Working")
                )
            })
        })
        .await;
        assert_eq!(
            lock(&harness.run_requests)
                .iter()
                .filter(|request| request.prompt == "busy")
                .count(),
            1
        );
        core.sessions.stop_live_voice().await.unwrap();
        wait_for_phase(&core.sessions, LiveVoicePhase::Idle).await;
        assert_eq!(
            core.sessions.session_status("active").unwrap().status,
            SessionStatus::Working
        );
        harness.set_session_context_supported(false);
        assert_eq!(
            core.sessions
                .probe_live_voice("active")
                .await
                .unwrap()
                .reason,
            Some(LiveVoiceUnavailableReason::UnsupportedOmp)
        );
        core.sessions.interrupt("active").await.unwrap();
        assert!(
            core.sessions
                .probe_live_voice("active")
                .await
                .unwrap()
                .available
        );
        core.sessions.start_live_voice("active").await.unwrap();
        wait_for_phase(&core.sessions, LiveVoicePhase::Listening).await;
        core.sessions.stop_live_voice().await.unwrap();
        wait_for_phase(&core.sessions, LiveVoicePhase::Idle).await;
        harness.set_session_context_supported(true);

        let mut awaiting_request = run_request();
        awaiting_request.prompt = "await-input".into();
        core.sessions
            .dispatch(
                "awaiting",
                HarnessId::Omp,
                awaiting_request,
                Some("awaiting-1".into()),
            )
            .await
            .unwrap();
        wait_until("awaiting-input run status", || {
            core.sessions
                .session_status("awaiting")
                .is_some_and(|session| session.status == SessionStatus::AwaitingInput)
        })
        .await;
        core.sessions.start_live_voice("awaiting").await.unwrap();
        wait_for_phase(&core.sessions, LiveVoicePhase::Listening).await;
        wait_until("AwaitingInput operational context", || {
            lock(&harness.controls).iter().any(|control| {
                matches!(
                    control,
                    LiveVoiceControl::AppendSessionContext { text }
                        if text == "Session status: AwaitingInput"
                )
            })
        })
        .await;
        core.sessions.stop_live_voice().await.unwrap();
        wait_for_phase(&core.sessions, LiveVoicePhase::Idle).await;
        core.sessions.interrupt("awaiting").await.unwrap();

        harness.set_supported(false);
        assert_eq!(
            core.sessions
                .probe_live_voice("live-a")
                .await
                .unwrap()
                .reason,
            Some(LiveVoiceUnavailableReason::UnsupportedOmp)
        );
        harness.set_supported(true);

        let doc = core.doc_host.open("live-a").unwrap();
        let entries_before = doc.doc().read_entries().unwrap();
        let commands_before = doc.doc().read_commands().unwrap();
        core.sessions.start_live_voice("live-a").await.unwrap();
        wait_for_phase(&core.sessions, LiveVoicePhase::Listening).await;
        assert_eq!(
            core.sessions
                .probe_live_voice("live-b")
                .await
                .unwrap()
                .reason,
            Some(LiveVoiceUnavailableReason::AnotherLiveCall)
        );
        assert!(core.sessions.start_live_voice("live-b").await.is_err());
        core.sessions.set_live_voice_muted(true).await.unwrap();
        wait_for_phase(&core.sessions, LiveVoicePhase::Muted).await;
        core.sessions.stop_live_voice().await.unwrap();
        wait_for_phase(&core.sessions, LiveVoicePhase::Idle).await;
        core.sessions.stop_live_voice().await.unwrap();

        assert_eq!(doc.doc().read_entries().unwrap(), entries_before);
        assert_eq!(doc.doc().read_commands().unwrap(), commands_before);
        assert_eq!(
            lock(&harness.controls).as_slice(),
            [
                LiveVoiceControl::AppendSessionContext {
                    text: "Session status: Working".into(),
                },
                LiveVoiceControl::Stop,
                LiveVoiceControl::Stop,
                LiveVoiceControl::AppendSessionContext {
                    text: "Session status: AwaitingInput".into(),
                },
                LiveVoiceControl::Stop,
                LiveVoiceControl::AppendSessionContext {
                    text: "Session status: Idle".into(),
                },
                LiveVoiceControl::SetMuted(true),
                LiveVoiceControl::Stop,
            ]
        );
        core.sessions.shutdown().await;
    }

    #[tokio::test]
    async fn live_voice_reuses_existing_and_created_sessions_for_live_and_text_runs() {
        let temp = tempfile::tempdir().unwrap();
        let harness = Arc::new(FakeLiveHarness::supported());
        let registry = crate::registry::HarnessRegistry::new();
        registry.register(harness.clone());
        let core =
            crate::EngineCore::assemble(temp.path(), Arc::new(registry), HarnessId::Omp, None)
                .unwrap();
        let local_device = core.device_id.clone();
        for chat_id in ["existing", "new"] {
            core.workspace
                .create_chat(
                    chat_id,
                    None,
                    Some(local_device.as_str()),
                    Some(omp_config()),
                    Some("/tmp".into()),
                )
                .unwrap();
        }
        core.workspace
            .set_chat_harness_session("existing", "/tmp/existing-session.jsonl", "/tmp");

        core.sessions.start_live_voice("existing").await.unwrap();
        wait_for_phase(&core.sessions, LiveVoicePhase::Listening).await;
        assert_eq!(
            lock(&harness.live_requests)[0].resume.as_deref(),
            Some("/tmp/existing-session.jsonl")
        );
        core.sessions.stop_live_voice().await.unwrap();
        wait_for_phase(&core.sessions, LiveVoicePhase::Idle).await;

        core.sessions.start_live_voice("new").await.unwrap();
        wait_for_phase(&core.sessions, LiveVoicePhase::Listening).await;
        assert_eq!(lock(&harness.live_requests)[1].resume, None);
        assert_eq!(
            core.workspace.chat_harness_session("new"),
            Some(("/tmp/live-created.jsonl".into(), Some("/tmp".into())))
        );
        core.sessions.stop_live_voice().await.unwrap();
        wait_for_phase(&core.sessions, LiveVoicePhase::Idle).await;

        core.sessions.start_live_voice("new").await.unwrap();
        wait_for_phase(&core.sessions, LiveVoicePhase::Listening).await;
        assert_eq!(
            lock(&harness.live_requests)[2].resume.as_deref(),
            Some("/tmp/live-created.jsonl")
        );
        core.sessions.stop_live_voice().await.unwrap();
        wait_for_phase(&core.sessions, LiveVoicePhase::Idle).await;

        core.sessions
            .dispatch(
                "new",
                HarnessId::Omp,
                run_request(),
                Some("text-after-live".into()),
            )
            .await
            .unwrap();
        tokio::time::timeout(Duration::from_secs(2), async {
            while !lock(&harness.run_requests)
                .iter()
                .any(|request| request.prompt == "busy")
            {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        let requests = lock(&harness.run_requests);
        let request = requests
            .iter()
            .find(|request| request.prompt == "busy")
            .unwrap();
        assert_eq!(
            (request.cwd.as_str(), request.resume.as_deref()),
            ("/tmp", Some("/tmp/live-created.jsonl"))
        );
        drop(requests);
        core.sessions.interrupt("new").await.unwrap();
        core.sessions.shutdown().await;
    }
}
