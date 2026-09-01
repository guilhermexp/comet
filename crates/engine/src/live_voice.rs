use futures::{StreamExt, stream::BoxStream};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;
use tokio::sync::{mpsc, watch};
use tokio::task::JoinHandle;
use zeron_harness::{
    CancellationToken, HarnessError, LiveVoiceControl, LiveVoiceEvent, LiveVoiceHandle,
};
use zeron_proto::{LiveVoicePhase, LiveVoiceState};

use crate::{EngineError, new_id};

pub const MAX_LIVE_TEXT_BYTES: usize = 64 * 1024;
const LIVE_STOP_TIMEOUT: Duration = Duration::from_secs(2);

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

struct ActiveLiveVoice {
    call_id: String,
    chat_id: String,
    controls: Option<mpsc::Sender<LiveVoiceControl>>,
    phase_before_mute: LiveVoicePhase,
    pub(crate) active_delegation_id: Option<String>,
    pub(crate) owned_command_id: Option<String>,
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
        let Some(active) = active
            .as_mut()
            .filter(|active| active.call_id == call_id)
        else {
            return false;
        };
        active.controls = Some(controls);
        true
    }

    pub(crate) fn cancellation(&self, call_id: &str) -> Option<CancellationToken> {
        lock(&self.inner.active)
            .as_ref()
            .filter(|active| active.call_id == call_id)
            .map(|active| active.cancellation.clone())
    }

    pub(crate) fn attach_task(&self, call_id: &str, task: JoinHandle<()>) {
        let mut active = lock(&self.inner.active);
        if let Some(active) = active
            .as_mut()
            .filter(|active| active.call_id == call_id)
        {
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
                let Some(active) = active
                    .as_mut()
                    .filter(|active| active.call_id == call_id)
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
            controls.send(LiveVoiceControl::Stop).await.map_err(|_| {
                EngineError::Other("Live Voice control channel closed".into())
            })
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
        self.inner.state.send_replace(LiveVoiceState::default());
        send_result
    }

    pub(crate) fn fail(&self, call_id: &str, message: &str) {
        let mut active = lock(&self.inner.active);
        if !active
            .as_ref()
            .is_some_and(|active| active.call_id == call_id)
        {
            return;
        }
        if let Some(active) = active.take() {
            active.cancellation.cancel();
        }
        let mut state = self.inner.state.borrow().clone();
        state.phase = LiveVoicePhase::Error;
        state.input_level = 0.0;
        state.output_level = 0.0;
        state.error = Some(truncate_live_text(message.to_owned()));
        self.inner.state.send_replace(state);
    }

    fn finish(&self, call_id: &str) {
        let mut active = lock(&self.inner.active);
        if !active
            .as_ref()
            .is_some_and(|active| active.call_id == call_id)
        {
            return;
        }
        if let Some(active) = active.take() {
            active.cancellation.cancel();
        }
        self.inner.state.send_replace(LiveVoiceState::default());
    }

    pub(crate) fn matches(&self, call_id: &str) -> bool {
        lock(&self.inner.active)
            .as_ref()
            .is_some_and(|active| active.call_id == call_id)
    }
}

pub(crate) fn spawn_live_event_task(
    coordinator: LiveVoiceCoordinator,
    call_id: String,
    mut events: BoxStream<'static, Result<LiveVoiceEvent, HarnessError>>,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        let Some(cancellation) = coordinator.cancellation(&call_id) else {
            return;
        };
        loop {
            tokio::select! {
                _ = cancellation.cancelled() => return,
                event = events.next() => match event {
                    Some(Ok(event)) => {
                        let terminal = matches!(event, LiveVoiceEvent::Ended { .. });
                        coordinator.handle_event(&call_id, event);
                        if terminal {
                            return;
                        }
                    }
                    Some(Err(error)) => {
                        coordinator.fail(&call_id, &error.to_string());
                        return;
                    }
                    None => {
                        coordinator.fail(&call_id, "Live Voice event stream closed unexpectedly");
                        return;
                    }
                }
            }
        }
    })
}

pub(crate) fn attach_live_handle(
    coordinator: &LiveVoiceCoordinator,
    call_id: &str,
    handle: LiveVoiceHandle,
) -> Result<(), LiveVoiceHandle> {
    if !coordinator.attach_controls(call_id, handle.controls.clone()) {
        return Err(handle);
    }
    let task = spawn_live_event_task(
        coordinator.clone(),
        call_id.to_owned(),
        handle.events,
    );
    coordinator.attach_task(call_id, task);
    Ok(())
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
    use zeron_harness::{
        Harness, HarnessError, LiveVoiceControl, LiveVoiceEvent, LiveVoiceHandle,
        LiveVoiceRequest, RunControls,
    };
    use zeron_proto::{
        AgentEvent, ChatConfig, DoneStatus, HarnessId, LiveVoicePhase, LiveVoiceRole,
        LiveVoiceTranscript, LiveVoiceUnavailableReason, Model, ReasoningLevel, RunRequest,
        SandboxLevel, SteeringMode,
    };

    #[tokio::test]
    async fn live_voice_state_tracks_transient_events_and_exact_mute() {
        let coordinator = LiveVoiceCoordinator::new();
        let state = coordinator.watch();
        let (controls, mut received) = mpsc::channel(4);
        let call_id = coordinator.reserve("chat-1").unwrap();
        assert!(coordinator.attach_controls(&call_id, controls));
        assert_eq!(state.borrow().phase, LiveVoicePhase::Connecting);

        coordinator.handle_event(
            &call_id,
            LiveVoiceEvent::Phase(LiveVoicePhase::Listening),
        );
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
        assert_eq!(received.recv().await, Some(LiveVoiceControl::SetMuted(true)));
        assert_eq!(state.borrow().phase, LiveVoicePhase::Muted);
        coordinator.set_muted(false).await.unwrap();
        assert_eq!(received.recv().await, Some(LiveVoiceControl::SetMuted(false)));
        assert_eq!(state.borrow().phase, LiveVoicePhase::Listening);
        assert!(coordinator.reserve("chat-2").is_err());
    }

    #[tokio::test]
    async fn live_voice_state_stop_is_ordered_and_idempotent() {
        let coordinator = LiveVoiceCoordinator::new();
        let mut state = coordinator.watch();
        let (controls, mut received) = mpsc::channel(1);
        controls.send(LiveVoiceControl::SetMuted(false)).await.unwrap();
        let call_id = coordinator.reserve("chat-1").unwrap();
        assert!(coordinator.attach_controls(&call_id, controls));
        state.borrow_and_update();

        let stopping = {
            let coordinator = coordinator.clone();
            tokio::spawn(async move { coordinator.stop().await })
        };
        state.changed().await.unwrap();
        assert_eq!(state.borrow().phase, LiveVoicePhase::Stopping);
        assert_eq!(received.recv().await, Some(LiveVoiceControl::SetMuted(false)));
        assert_eq!(received.recv().await, Some(LiveVoiceControl::Stop));
        stopping.await.unwrap().unwrap();
        assert_eq!(state.borrow().phase, LiveVoicePhase::Idle);
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
        controls: Arc<Mutex<Vec<LiveVoiceControl>>>,
    }

    impl FakeLiveHarness {
        fn supported() -> Self {
            Self {
                supported: std::sync::atomic::AtomicBool::new(true),
                controls: Arc::new(Mutex::new(Vec::new())),
            }
        }

        fn set_supported(&self, supported: bool) {
            self.supported
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

        async fn probe_live_voice(&self, _cwd: &std::path::Path) -> Result<bool, HarnessError> {
            Ok(self
                .supported
                .load(std::sync::atomic::Ordering::SeqCst))
        }

        async fn start_live_voice(
            &self,
            _request: LiveVoiceRequest,
        ) -> Result<LiveVoiceHandle, HarnessError> {
            if !self
                .supported
                .load(std::sync::atomic::Ordering::SeqCst)
            {
                return Err(HarnessError::Unsupported("update OMP".into()));
            }
            let (control_tx, mut control_rx) = mpsc::channel::<LiveVoiceControl>(16);
            let (event_tx, event_rx) =
                mpsc::channel::<Result<LiveVoiceEvent, HarnessError>>(16);
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
                events: futures::stream::unfold(event_rx, |mut receiver| async move {
                    receiver.recv().await.map(|event| (event, receiver))
                })
                .boxed(),
                controls: control_tx,
            })
        }

        async fn run(
            &self,
            _request: RunRequest,
            controls: RunControls,
        ) -> Result<BoxStream<'static, Result<AgentEvent, HarnessError>>, HarnessError> {
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
                controls.interrupt.cancelled().await;
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

    #[tokio::test]
    async fn live_voice_preconditions_and_transient_lifecycle() {
        let temp = tempfile::tempdir().unwrap();
        let harness = Arc::new(FakeLiveHarness::supported());
        let registry = crate::registry::HarnessRegistry::new();
        registry.register(harness.clone());
        let core = crate::EngineCore::assemble(
            temp.path(),
            Arc::new(registry),
            HarnessId::Omp,
            None,
        )
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
            .dispatch("active", HarnessId::Omp, run_request(), Some("busy-1".into()))
            .await
            .unwrap();
        assert_eq!(
            core.sessions
                .probe_live_voice("active")
                .await
                .unwrap()
                .reason,
            Some(LiveVoiceUnavailableReason::ActiveRun)
        );
        core.sessions.interrupt("active").await.unwrap();

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
            [LiveVoiceControl::SetMuted(true), LiveVoiceControl::Stop]
        );
        core.sessions.shutdown().await;
    }
}
