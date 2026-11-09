//! Latest-only, in-memory inspection on an already running Chat host.
use super::*;
use std::sync::atomic::{AtomicU64, Ordering};
use zeron_proto::{ChatConfig, ContextAvailability, ContextBreakdown};

// These facts belong to one process and the configuration that produced its
// stream. Session continuity is separate and may retain values across processes.
#[derive(Clone, PartialEq, Eq)]
pub(super) struct RuntimeContextUsage {
    run_id: String,
    resume_binding: Option<(String, String, Option<HarnessId>)>,
    model: Option<String>,
    usage: zeron_proto::ContextUsage,
    tokens_at: Option<i64>,
    capacity_at: Option<i64>,
}
impl RuntimeContextUsage {
    fn observed_at(&self) -> i64 {
        self.tokens_at
            .into_iter()
            .chain(self.capacity_at)
            .min()
            .unwrap_or_default()
    }
}
impl Inner {
    pub(super) fn record_runtime_context_usage(
        &self,
        chat: &str,
        run_id: &str,
        binding: &RuntimeConfig,
        usage: zeron_proto::ContextUsage,
    ) {
        let resume_binding = lock(&self.harness_sessions).get(chat).map(|native| {
            (
                native.session_id.clone(),
                native.cwd.clone(),
                native.harness,
            )
        });
        let mut runs = lock(&self.runs);
        let Some(run) = runs.get_mut(chat).filter(|run| {
            run.run_id == run_id
                && run.runtime_config.request.model == binding.request.model
                && binding.can_route(run.runtime_config.harness_id, &run.runtime_config.request)
        }) else {
            return;
        };
        if usage.reported_tokens().is_none() && usage.reported_window().is_none() {
            return;
        }
        let previous = run.context_usage.as_ref().filter(|previous| {
            previous.run_id == run_id
                && previous.resume_binding == resume_binding
                && previous.model == binding.request.model
        });
        let now = now_ms();
        run.context_usage = Some(RuntimeContextUsage {
            run_id: run_id.into(),
            resume_binding,
            model: binding.request.model.clone(),
            usage: usage.merge(previous.map(|p| p.usage)),
            tokens_at: usage
                .reported_tokens()
                .map(|_| now)
                .or_else(|| previous.and_then(|p| p.tokens_at)),
            capacity_at: usage
                .reported_window()
                .map(|_| now)
                .or_else(|| previous.and_then(|p| p.capacity_at)),
        });
    }
}
fn has_measurement(report: &ContextBreakdown) -> bool {
    matches!(
        report.availability,
        ContextAvailability::Available | ContextAvailability::Partial
    ) && (report.used_tokens.is_some()
        || report.capacity.is_some()
        || report.categories.iter().any(|r| r.tokens.is_some()))
}
fn aggregate_report(report: &mut ContextBreakdown, aggregate: &RuntimeContextUsage, failed: bool) {
    report.availability = ContextAvailability::Partial;
    report.used_tokens = aggregate.usage.reported_tokens();
    report.capacity = aggregate.usage.reported_window();
    report.used_basis = report.used_tokens.map(|_| "native-aggregate".into());
    report.model = aggregate.model.clone();
    // Conservative age when independently reported fields have different ages.
    report.observed_at_ms = aggregate.observed_at();
    report.measured_at_ms = None; // Native provider time was not reported.
    report.stale = failed;
}

#[derive(Default)]
pub(super) struct ContextCache {
    gate: tokio::sync::Mutex<()>,
    latest: Mutex<Option<Cached>>,
    touched: AtomicU64,
}
struct Cached {
    report: ContextBreakdown,
    runtime_config: RuntimeConfig,
    aggregate: bool,
    stream_usage: Option<RuntimeContextUsage>,
    config: Option<ChatConfig>,
    resume_binding: Option<(String, String, Option<HarnessId>)>,
    completed: tokio::time::Instant,
}

const MAX_CACHED_CHATS: usize = 32;
static NEXT_CACHE_READ: AtomicU64 = AtomicU64::new(0);
fn cache_for(
    caches: &mut HashMap<String, Arc<ContextCache>>,
    chat_id: &str,
) -> Option<Arc<ContextCache>> {
    if !caches.contains_key(chat_id) && caches.len() >= MAX_CACHED_CHATS {
        // Only the map owns an idle entry. Never split an in-flight reader's
        // coalescing gate by replacing its cache while another caller holds it.
        let oldest = caches
            .iter()
            .filter(|(_, cache)| Arc::strong_count(cache) == 1)
            .min_by_key(|(_, cache)| cache.touched.load(Ordering::Relaxed))
            .map(|(id, _)| id.clone())?;
        caches.remove(&oldest);
    }
    let cache = caches.entry(chat_id.to_owned()).or_default().clone();
    cache.touched.store(
        NEXT_CACHE_READ.fetch_add(1, Ordering::Relaxed),
        Ordering::Relaxed,
    );
    Some(cache)
}

// Keep the ownership sweep separate from admission so its lock boundary is testable.
fn prune_caches(
    caches: &Mutex<HashMap<String, Arc<ContextCache>>>,
    mut is_local: impl FnMut(&str) -> bool,
) {
    let candidates = lock(caches)
        .iter()
        .map(|(id, cache)| (id.clone(), Arc::downgrade(cache)))
        .collect::<Vec<_>>();
    // Registry/storage reads must never run under the cache-map mutex.
    let invalid = candidates
        .into_iter()
        .filter(|(id, _)| !is_local(id))
        .collect::<Vec<_>>();
    let mut caches = lock(caches);
    for (id, original) in invalid {
        let removable = caches.get(&id).is_some_and(|current| {
            // A replacement was not the entry whose ownership was checked.
            std::sync::Weak::ptr_eq(&original, &Arc::downgrade(current))
                // As in cache_for, active/queued readers own another Arc.
                && Arc::strong_count(current) == 1
        });
        if removable {
            caches.remove(&id);
        }
    }
}

impl SessionsEngine {
    pub async fn context_breakdown(
        &self,
        chat_id: &str,
        selected: Option<ChatConfig>,
    ) -> ContextBreakdown {
        let requested = tokio::time::Instant::now();
        let unavailable = |reason: &str| {
            let mut report =
                ContextBreakdown::unavailable(ContextAvailability::NotApplicable, reason);
            report.chat_id = chat_id.to_owned();
            report.host_device_id = self.inner.device_id.clone();
            report.observed_at_ms = now_ms();
            report
        };
        let cache = {
            if let Some(workspace) = self.inner.workspace() {
                let inner = self.inner.clone();
                let pruning = tokio::task::spawn_blocking(move || {
                    prune_caches(&inner.context_breakdowns, |id| {
                        workspace
                            .chat(id)
                            .ok()
                            .flatten()
                            .is_some_and(|chat| chat.device_id == inner.device_id)
                    });
                })
                .await;
                if pruning.is_err() {
                    let mut report = unavailable("Context inspection workspace lookup failed");
                    report.availability = ContextAvailability::Error;
                    return report;
                }
            }
            cache_for(&mut lock(&self.inner.context_breakdowns), chat_id)
        };
        let Some(cache) = cache else {
            let mut report = unavailable("Context inspection is busy; refresh to retry");
            report.availability = ContextAvailability::Error;
            return report;
        };
        let discard = || {
            let mut caches = lock(&self.inner.context_breakdowns);
            if caches
                .get(chat_id)
                .is_some_and(|entry| Arc::ptr_eq(entry, &cache))
            {
                caches.remove(chat_id);
            }
        };
        let _gate = cache.gate.lock().await;
        let live = lock(&self.inner.runs)
            .get(chat_id)
            .filter(|run| {
                run.started.load(std::sync::atomic::Ordering::Acquire)
                    && !run.retiring.load(std::sync::atomic::Ordering::Acquire)
            })
            .map(|run| {
                (
                    run.run_id.clone(),
                    run.runtime_config.clone(),
                    run.turn.clone(),
                    run.context_usage.clone().filter(|usage| {
                        usage.run_id == run.run_id
                            && usage.model == run.runtime_config.request.model
                    }),
                )
            });
        let selected_matches = |runtime: &RuntimeConfig| {
            selected.as_ref().is_none_or(|selected| {
                let mut request = runtime.request.clone();
                request.model = selected.model.clone();
                request.reasoning = selected.reasoning;
                request.model_options = selected.model_options.clone();
                request.sandbox = selected.sandbox;
                request.access = selected.access;
                request.plan = selected.plan;
                runtime.can_route(selected.harness, &request)
            })
        };
        let selection_current = || {
            self.inner.workspace().is_none_or(|workspace| {
                workspace.chat(chat_id).ok().flatten().is_some_and(|chat| {
                    chat.config == selected && chat.device_id == self.inner.device_id
                })
            })
        };
        if !selection_current() {
            discard();
            return unavailable("Context changed while inspection was queued");
        }
        let Some((run_id, config, control, aggregate)) = live else {
            return lock(&cache.latest)
                .as_ref()
                .filter(|c| c.config == selected && has_measurement(&c.report))
                .map(|cached| {
                    let mut report = cached.report.clone();
                    report.stale = true;
                    report.reason = Some(match report.reason.take() {
                        Some(reason) => format!("Runtime ended; retained snapshot · {reason}"),
                        None => "Runtime ended; retained snapshot".into(),
                    });
                    report
                })
                .unwrap_or_else(|| unavailable("No live runtime to inspect"));
        };
        if !selected_matches(&config) {
            return unavailable("The selected model has no matching live runtime");
        }
        // Resume locators (OMP uses a session-file path) and native session
        // UUIDs are distinct identities. Validate the locator before/after the
        // read without comparing it to the report's native UUID.
        let resume_binding = || {
            self.inner
                .owned_session(chat_id)
                .map(|native| (native.session_id, native.cwd, native.harness))
        };
        let native_before = resume_binding();
        let aggregate = aggregate.filter(|usage| usage.resume_binding == native_before);
        if let Some(cached) = lock(&cache.latest).as_ref().filter(|cached| {
            cached.config == selected
                && cached.runtime_config.request.model == config.request.model
                && cached
                    .runtime_config
                    .can_route(config.harness_id, &config.request)
                && cached.report.runtime_id == run_id
                && cached.resume_binding == native_before
                // A queued reader may already have received a newer stream
                // measurement while the preceding inspection was in flight.
                && cached.stream_usage.as_ref() == aggregate.as_ref()
                && (cached.completed >= requested
                    || (cached.report.availability == ContextAvailability::Unsupported
                        && aggregate.is_none()))
        }) {
            return cached.report.clone();
        }
        let result = control.inspect_context().await;
        // Validate again after the read. Replacement, reconfiguration or selection
        // changes never publish a response as current for a different runtime.
        let still_current = lock(&self.inner.runs).get(chat_id).is_some_and(|run| {
            run.run_id == run_id
                && run.runtime_config.request.model == config.request.model
                && !run.retiring.load(std::sync::atomic::Ordering::Acquire)
                && config.can_route(run.runtime_config.harness_id, &run.runtime_config.request)
        });
        let selection_is_current = selection_current();
        if !selection_is_current {
            discard();
        }
        if !still_current || !selection_is_current {
            return unavailable("Context changed during inspection; refresh to inspect it");
        }
        if resume_binding() != native_before {
            return unavailable("Native session changed during inspection");
        }
        let mut report = match result {
            Ok(report) => report,
            Err(zeron_harness::HarnessError::Unsupported(reason)) => {
                ContextBreakdown::unavailable(ContextAvailability::Unsupported, reason)
            }
            Err(zeron_harness::HarnessError::Protocol(reason)) => {
                ContextBreakdown::unavailable(ContextAvailability::Error, reason)
            }
            Err(_) => ContextBreakdown::unavailable(
                ContextAvailability::Error,
                "Context inspection failed or timed out",
            ),
        };
        if matches!(
            report.availability,
            ContextAvailability::Error
                | ContextAvailability::NotApplicable
                | ContextAvailability::Unsupported
        ) && let Some(cached) = lock(&cache.latest).as_mut().filter(|cached| {
            has_measurement(&cached.report)
                && !aggregate.as_ref().is_some_and(|aggregate| {
                    report.availability == ContextAvailability::Unsupported
                        || (cached.aggregate && cached.stream_usage.as_ref() != Some(aggregate))
                })
                && cached.config == selected
                && cached.runtime_config.request.model == config.request.model
                && cached
                    .runtime_config
                    .can_route(config.harness_id, &config.request)
                && cached.report.runtime_id == run_id
                && cached.resume_binding == native_before
        }) {
            let mut retained = cached.report.clone();
            retained.stale = true;
            retained.reason = Some(format!(
                "{}; retained snapshot",
                report
                    .reason
                    .as_deref()
                    .unwrap_or("Native context unavailable")
            ));
            cached.report = retained.clone();
            cached.completed = tokio::time::Instant::now();
            return retained;
        }
        let aggregate_used = matches!(
            report.availability,
            ContextAvailability::Unsupported
                | ContextAvailability::Error
                | ContextAvailability::NotApplicable
        ) && aggregate.is_some();
        if let Some(aggregate) = aggregate.as_ref().filter(|_| aggregate_used) {
            let failed = report.availability == ContextAvailability::Error;
            aggregate_report(&mut report, aggregate, failed);
        }
        if report.source.is_empty() {
            report.source = config.harness_id.as_str().into();
        }
        report.chat_id = chat_id.to_owned();
        report.host_device_id = self.inner.device_id.clone();
        report.runtime_id = run_id;
        if !aggregate_used {
            report.observed_at_ms = now_ms();
        }
        *lock(&cache.latest) = Some(Cached {
            report: report.clone(),
            runtime_config: config,
            aggregate: aggregate_used || report.source == "omp-aggregate",
            stream_usage: aggregate,
            config: selected,
            resume_binding: native_before,
            completed: tokio::time::Instant::now(),
        });
        report
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    struct AggregateHarness {
        id: HarnessId,
        inspection_error: bool,
        receiver: Mutex<
            Option<tokio::sync::mpsc::Receiver<Result<AgentEvent, zeron_harness::HarnessError>>>,
        >,
    }
    #[async_trait::async_trait]
    impl Harness for AggregateHarness {
        fn id(&self) -> HarnessId {
            self.id
        }
        fn display_name(&self) -> &str {
            "Synthetic aggregate"
        }
        fn supports_steering(&self) -> bool {
            true
        }
        fn steering_mode(&self) -> zeron_proto::SteeringMode {
            zeron_proto::SteeringMode::StepBoundary
        }
        fn reasoning_levels(&self) -> &[zeron_proto::ReasoningLevel] {
            &[]
        }
        fn deterministic_turn_end(&self) -> bool {
            true
        }
        async fn models(&self) -> Result<Vec<zeron_proto::Model>, zeron_harness::HarnessError> {
            Ok(vec![])
        }
        async fn run(
            &self,
            _: RunRequest,
            controls: RunControls,
        ) -> Result<
            futures::stream::BoxStream<'static, Result<AgentEvent, zeron_harness::HarnessError>>,
            zeron_harness::HarnessError,
        > {
            use futures::StreamExt;
            if self.inspection_error {
                let mut requests = controls.turn.context_requests();
                tokio::spawn(async move {
                    loop {
                        tokio::select! {
                            _ = controls.interrupt.cancelled() => break,
                            request = requests.recv() => { let Some(request) = request else { break; }; let _ = request.reply.send(Err(zeron_harness::HarnessError::Protocol("synthetic read failure".into()))); }
                        }
                    }
                });
            }
            let rx = lock(&self.receiver).take().unwrap();
            Ok(futures::stream::unfold(
                rx,
                |mut rx| async move { rx.recv().await.map(|e| (e, rx)) },
            )
            .boxed())
        }
    }
    async fn aggregate_runtime(
        id: HarnessId,
        inspection_error: bool,
    ) -> (
        tempfile::TempDir,
        crate::EngineCore,
        tokio::sync::mpsc::Sender<Result<AgentEvent, zeron_harness::HarnessError>>,
    ) {
        let dir = tempfile::tempdir().unwrap();
        let registry = crate::HarnessRegistry::new();
        let (tx, rx) = tokio::sync::mpsc::channel(16);
        registry.register(Arc::new(AggregateHarness {
            id,
            inspection_error,
            receiver: Mutex::new(Some(rx)),
        }));
        let core = crate::EngineCore::assemble(dir.path(), Arc::new(registry), id, None).unwrap();
        core.workspace
            .create_chat("aggregate", None, Some(&core.device_id), None, None)
            .unwrap();
        let request = RunRequest {
            prompt: "synthetic".into(),
            harness: Some(id),
            model: Some("model-a".into()),
            reasoning: None,
            model_options: Default::default(),
            cwd: dir.path().to_string_lossy().into_owned(),
            sandbox: zeron_proto::SandboxLevel::WorkspaceWrite,
            auto_approve: false,
            access: Default::default(),
            plan: false,
            resume: None,
            attachments: vec![],
            worktree: None,
            enable_workers_mcp: false,
            workers_parent_chat_id: None,
            sessions: None,
            env: Default::default(),
            extra_mcp: vec![],
            mcp: None,
        };
        core.sessions
            .dispatch("aggregate", id, request, None)
            .await
            .unwrap();
        tx.send(Ok(AgentEvent::SessionStarted {
            harness: id,
            model: "model-a".into(),
            tools: vec![],
            cwd: dir.path().to_string_lossy().into_owned(),
            session_id: "native-a".into(),
            assistant_message_id: "a".into(),
        }))
        .await
        .unwrap();
        tx.send(Ok(AgentEvent::ContextUsage {
            tokens: Some(0),
            window: None,
        }))
        .await
        .unwrap();
        tx.send(Ok(AgentEvent::Done {
            status: zeron_proto::DoneStatus::Completed,
            result: None,
            error: None,
            session_id: Some("native-a".into()),
        }))
        .await
        .unwrap();
        tokio::time::timeout(Duration::from_secs(5), async {
            while core
                .sessions
                .session_status("aggregate")
                .is_none_or(|s| s.status != SessionStatus::Idle || s.context_usage.is_none())
            {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        (dir, core, tx)
    }
    #[tokio::test]
    async fn context_breakdown_aggregate_codex_pi_zero_partial_and_read_timestamp() {
        for id in [HarnessId::Codex, HarnessId::Pi] {
            let (_dir, core, tx) = aggregate_runtime(id, false).await;
            let first = core.sessions.context_breakdown("aggregate", None).await;
            assert_eq!(first.availability, ContextAvailability::Partial);
            assert_eq!(first.used_tokens, Some(0));
            assert_eq!(first.capacity, None);
            assert_eq!(first.model.as_deref(), Some("model-a"));
            assert_eq!(first.host_device_id, core.device_id);
            assert!(!first.runtime_id.is_empty());
            tokio::time::sleep(Duration::from_millis(5)).await;
            let reread = core.sessions.context_breakdown("aggregate", None).await;
            assert_eq!(
                reread.observed_at_ms, first.observed_at_ms,
                "inspection must not redate the cached measurement"
            );
            tx.send(Ok(AgentEvent::ContextUsage {
                tokens: None,
                window: Some(1000),
            }))
            .await
            .unwrap();
            tokio::time::timeout(Duration::from_secs(2), async {
                while core
                    .sessions
                    .session_status("aggregate")
                    .unwrap()
                    .context_usage
                    .unwrap()
                    .context_window
                    != 1000
                {
                    tokio::task::yield_now().await;
                }
            })
            .await
            .unwrap();
            let merged = core.sessions.context_breakdown("aggregate", None).await;
            assert_eq!((merged.used_tokens, merged.capacity), (Some(0), Some(1000)));
            assert_eq!(
                merged.observed_at_ms, first.observed_at_ms,
                "a capacity-only update cannot freshen older occupancy"
            );
            core.shutdown().await;
        }
    }
    #[tokio::test]
    async fn context_breakdown_aggregate_rejects_late_model_and_previous_runtime() {
        let (_dir, core, tx) = aggregate_runtime(HarnessId::Codex, false).await;
        let first = core.sessions.context_breakdown("aggregate", None).await;
        {
            let mut runs = lock(&core.sessions.inner.runs);
            runs.get_mut("aggregate")
                .unwrap()
                .runtime_config
                .request
                .model = Some("model-b".into());
        }
        tx.send(Ok(AgentEvent::ContextUsage {
            tokens: Some(300),
            window: Some(2000),
        }))
        .await
        .unwrap();
        tokio::time::timeout(Duration::from_secs(2), async {
            while core
                .sessions
                .session_status("aggregate")
                .unwrap()
                .context_usage
                .unwrap()
                .tokens
                != 300
            {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        let changed = core.sessions.context_breakdown("aggregate", None).await;
        assert_eq!(
            changed.used_tokens, None,
            "an unbound late event cannot be attributed to the new model"
        );
        // The existing native delivery boundary confirms which queued configuration
        // was actually adopted; an unacknowledged request is insufficient.
        {
            let runs = lock(&core.sessions.inner.runs);
            let run = runs.get("aggregate").unwrap();
            lock(&run.routed_steers).push_back(RoutedSteer {
                prompt: "new model".into(),
                message_id: "model-b-input".into(),
                fork_history: false,
                after_stop: false,
                request: Some(Box::new(run.runtime_config.request.clone())),
            });
        }
        tx.send(Ok(AgentEvent::Steered {
            assistant_message_id: Some("a".into()),
            next_assistant_message_id: Some("b".into()),
        }))
        .await
        .unwrap();
        tx.send(Ok(AgentEvent::ContextUsage {
            tokens: Some(400),
            window: Some(2000),
        }))
        .await
        .unwrap();
        tx.send(Ok(AgentEvent::Done {
            status: zeron_proto::DoneStatus::Completed,
            result: None,
            error: None,
            session_id: Some("native-a".into()),
        }))
        .await
        .unwrap();
        tokio::time::timeout(Duration::from_secs(2), async {
            while core.sessions.session_status("aggregate").is_none_or(|row| {
                row.status != SessionStatus::Idle || row.context_usage.unwrap().tokens != 400
            }) {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        let confirmed = core.sessions.context_breakdown("aggregate", None).await;
        assert_eq!(
            confirmed.used_tokens,
            Some(400),
            "acknowledged reconfiguration must accept its new measurement"
        );
        assert_eq!(confirmed.model.as_deref(), Some("model-b"));
        {
            let mut runs = lock(&core.sessions.inner.runs);
            let run = runs.get_mut("aggregate").unwrap();
            run.run_id = "replacement".into();
            run.runtime_config.request.model = Some("model-a".into());
        }
        let replacement = core.sessions.context_breakdown("aggregate", None).await;
        assert_eq!(
            replacement.used_tokens, None,
            "an old process aggregate cannot cross run identity"
        );
        assert_ne!(first.runtime_id, replacement.runtime_id);
        core.shutdown().await;
    }
    #[tokio::test]
    async fn context_breakdown_aggregate_failed_refresh_keeps_time_and_capacity_only_is_unknown() {
        let (_dir, core, tx) = aggregate_runtime(HarnessId::Codex, true).await;
        let first = core.sessions.context_breakdown("aggregate", None).await;
        assert_eq!(first.used_tokens, Some(0));
        assert!(
            first.stale,
            "a failed inspector cannot make the fallback current"
        );
        let second = core.sessions.context_breakdown("aggregate", None).await;
        assert_eq!(second.observed_at_ms, first.observed_at_ms);
        assert!(second.stale);
        core.shutdown().await;
        drop(tx);
        let (_dir, core, tx) = aggregate_runtime(HarnessId::Pi, false).await;
        lock(&core.sessions.inner.runs)
            .get_mut("aggregate")
            .unwrap()
            .context_usage = None;
        tx.send(Ok(AgentEvent::ContextUsage {
            tokens: None,
            window: Some(4000),
        }))
        .await
        .unwrap();
        tokio::time::timeout(Duration::from_secs(2), async {
            while core
                .sessions
                .session_status("aggregate")
                .unwrap()
                .context_usage
                .unwrap()
                .context_window
                != 4000
            {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        let capacity_only = core.sessions.context_breakdown("aggregate", None).await;
        assert_eq!(
            (capacity_only.used_tokens, capacity_only.capacity),
            (None, Some(4000))
        );
        core.shutdown().await;
    }
    #[tokio::test]
    async fn context_breakdown_aggregate_native_session_change_rejects_old_measurement() {
        let (_dir, core, tx) = aggregate_runtime(HarnessId::Pi, false).await;
        core.sessions.context_breakdown("aggregate", None).await;
        tx.send(Ok(AgentEvent::Steered {
            assistant_message_id: Some("a".into()),
            next_assistant_message_id: Some("b".into()),
        }))
        .await
        .unwrap();
        tx.send(Ok(AgentEvent::SessionStarted {
            harness: HarnessId::Pi,
            model: "model-a".into(),
            tools: vec![],
            cwd: _dir.path().to_string_lossy().into_owned(),
            session_id: "native-b".into(),
            assistant_message_id: "b".into(),
        }))
        .await
        .unwrap();
        tokio::time::timeout(Duration::from_secs(2), async {
            while core
                .sessions
                .inner
                .owned_session("aggregate")
                .is_none_or(|native| native.session_id != "native-b")
            {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        let changed = core.sessions.context_breakdown("aggregate", None).await;
        assert_eq!(
            changed.used_tokens, None,
            "the current process alone does not bind an old native session's aggregate"
        );
        tx.send(Ok(AgentEvent::ContextUsage {
            tokens: None,
            window: Some(8000),
        }))
        .await
        .unwrap();
        tokio::time::timeout(Duration::from_secs(2), async {
            while core
                .sessions
                .session_status("aggregate")
                .unwrap()
                .context_usage
                .unwrap()
                .context_window
                != 8000
            {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        let new_partial = core.sessions.context_breakdown("aggregate", None).await;
        assert_eq!(
            (new_partial.used_tokens, new_partial.capacity),
            (None, Some(8000)),
            "partial native-session updates cannot merge old-session occupancy"
        );
        core.shutdown().await;
    }
    #[tokio::test]
    async fn context_breakdown_aggregate_capacity_only_failure_yields_to_newer_facts() {
        let (_dir, core, tx) = aggregate_runtime(HarnessId::Codex, true).await;
        lock(&core.sessions.inner.runs)
            .get_mut("aggregate")
            .unwrap()
            .context_usage = None;
        tx.send(Ok(AgentEvent::ContextUsage {
            tokens: None,
            window: Some(4000),
        }))
        .await
        .unwrap();
        tokio::time::timeout(Duration::from_secs(2), async {
            while core
                .sessions
                .session_status("aggregate")
                .unwrap()
                .context_usage
                .unwrap()
                .context_window
                != 4000
            {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        let first = core.sessions.context_breakdown("aggregate", None).await;
        assert_eq!((first.used_tokens, first.capacity), (None, Some(4000)));
        assert!(first.stale);
        tx.send(Ok(AgentEvent::ContextUsage {
            tokens: Some(500),
            window: Some(5000),
        }))
        .await
        .unwrap();
        tokio::time::timeout(Duration::from_secs(2), async {
            while core
                .sessions
                .session_status("aggregate")
                .unwrap()
                .context_usage
                .unwrap()
                .context_window
                != 5000
            {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        let second = core.sessions.context_breakdown("aggregate", None).await;
        assert_eq!(
            (second.used_tokens, second.capacity),
            (Some(500), Some(5000)),
            "failed detail reads cannot suppress newer normalized measurements"
        );
        assert!(second.stale);
        core.shutdown().await;
    }
    #[tokio::test]
    async fn context_breakdown_aggregate_queued_reader_does_not_reuse_superseded_stream_usage() {
        for initial in [Some(0_u64), None] {
            let (_dir, core, tx) = aggregate_runtime(HarnessId::Codex, false).await;
            if initial.is_none() {
                lock(&core.sessions.inner.runs)
                    .get_mut("aggregate")
                    .unwrap()
                    .context_usage = None;
            }
            let turn = lock(&core.sessions.inner.runs)
                .get("aggregate")
                .unwrap()
                .turn
                .clone();
            let mut requests = turn.context_requests();
            let sessions = core.sessions.clone();
            let first =
                tokio::spawn(async move { sessions.context_breakdown("aggregate", None).await });
            let native_first = tokio::time::timeout(Duration::from_secs(3), requests.recv())
                .await
                .unwrap()
                .unwrap();
            let sessions = core.sessions.clone();
            let second = sessions.context_breakdown("aggregate", None);
            tokio::pin!(second);
            assert!(matches!(
                futures::poll!(second.as_mut()),
                std::task::Poll::Pending
            ));
            tx.send(Ok(AgentEvent::ContextUsage {
                tokens: Some(500),
                window: Some(5000),
            }))
            .await
            .unwrap();
            tokio::time::timeout(Duration::from_secs(3), async {
                while core
                    .sessions
                    .session_status("aggregate")
                    .unwrap()
                    .context_usage
                    .unwrap()
                    .tokens
                    != 500
                {
                    tokio::task::yield_now().await;
                }
            })
            .await
            .unwrap();
            let unsupported = || {
                ContextBreakdown::unavailable(
                    ContextAvailability::Unsupported,
                    "fixture aggregate only",
                )
            };
            native_first.reply.send(Ok(unsupported())).unwrap();
            let first_report = first.await.unwrap();
            let responder = tokio::spawn(async move {
                while let Some(request) = requests.recv().await {
                    let _ = request.reply.send(Ok(ContextBreakdown::unavailable(
                        ContextAvailability::Unsupported,
                        "fixture aggregate only",
                    )));
                }
            });
            let second_report = tokio::time::timeout(Duration::from_secs(3), second)
                .await
                .unwrap();
            responder.abort();
            core.shutdown().await;
            assert_eq!(first_report.used_tokens, initial);
            assert_eq!(
                (second_report.used_tokens, second_report.capacity),
                (Some(500), Some(5000)),
                "coalescing cannot discard an already received aggregate B in favor of A"
            );
        }
    }
    #[test]
    fn cache_pruning_queries_without_map_lock_and_keeps_active_gates() {
        let caches = Mutex::new(HashMap::new());
        for id in ["local", "missing", "foreign", "read-error"] {
            drop(cache_for(&mut lock(&caches), id).unwrap());
        }
        let active = cache_for(&mut lock(&caches), "active-missing").unwrap();
        let gate = active.gate.try_lock().unwrap();
        let mut checked = Vec::new();
        let mut available_during_query = true;
        prune_caches(&caches, |id| {
            available_during_query &= caches.try_lock().is_ok();
            checked.push(id.to_owned());
            id == "local"
        });
        let mut remaining = lock(&caches).keys().cloned().collect::<Vec<_>>();
        remaining.sort();
        drop(gate);
        drop(active);
        prune_caches(&caches, |id| id == "local");
        let after_release = lock(&caches).keys().cloned().collect::<Vec<_>>();
        assert_eq!(checked.len(), 5, "all captured IDs must be queried");
        assert!(
            available_during_query,
            "workspace lookup must run without the cache-map mutex"
        );
        assert_eq!(
            remaining,
            ["active-missing", "local"],
            "active gates cannot be removed by ownership pruning"
        );
        assert_eq!(
            after_release,
            ["local"],
            "inactive missing/foreign/unreadable entries must be removed"
        );
    }

    #[test]
    fn cache_pruning_does_not_remove_a_replacement_created_during_lookup() {
        let caches = Mutex::new(HashMap::new());
        drop(cache_for(&mut lock(&caches), "missing").unwrap());
        let mut replacement = None;
        prune_caches(&caches, |id| {
            assert_eq!(id, "missing");
            if let Ok(mut map) = caches.try_lock() {
                let next = Arc::new(ContextCache::default());
                replacement = Some(Arc::downgrade(&next));
                map.insert(id.to_owned(), next);
            }
            false
        });
        assert!(
            replacement.is_some(),
            "lookup must allow concurrent cache admission"
        );
        let map = lock(&caches);
        assert!(
            map.get("missing").is_some_and(|current| {
                std::sync::Weak::ptr_eq(replacement.as_ref().unwrap(), &Arc::downgrade(current))
            }),
            "the stale lookup result must not remove a new cache with the same ID"
        );
    }

    #[test]
    fn context_cache_bounds_chats_and_spares_inflight_readers() {
        let mut caches = HashMap::new();
        for index in 0..MAX_CACHED_CHATS * 2 {
            drop(cache_for(&mut caches, &index.to_string()).unwrap());
            assert!(
                caches.len() <= MAX_CACHED_CHATS,
                "retained Chat snapshots must be bounded"
            );
        }
        caches.clear();
        let mut active = Vec::new();
        for index in 0..MAX_CACHED_CHATS {
            active.push(cache_for(&mut caches, &index.to_string()).unwrap());
        }
        assert!(
            cache_for(&mut caches, "overflow").is_none(),
            "all retained readers are active"
        );
        assert!(Arc::ptr_eq(
            &active[0],
            &cache_for(&mut caches, "0").unwrap()
        ));
        active.remove(0);
        assert!(cache_for(&mut caches, "overflow").is_some());
        assert_eq!(caches.len(), MAX_CACHED_CHATS);
    }
}
