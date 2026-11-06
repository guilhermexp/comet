//! Actual normalizer + host + serialized RPC; the CLI peer is deterministic.
use serde_json::json;
use std::{collections::HashMap, sync::Arc, time::Duration};
use zeron_engine::{EngineCore, HarnessRegistry};
use zeron_harness::OmpHarness;
use zeron_proto::{
    ContextAvailability, ContextBreakdown, HarnessId, RunRequest, SandboxLevel, SessionStatus,
};

/// Opt-in cross-project proof against the coordinated source-built launcher.
/// No installed runtime, credentials or app UI; only metadata leaves the test.
#[tokio::test]
#[ignore = "requires isolated COMET_OMP_CONTEXT_LAUNCH and COMET_OMP_CONTEXT_ORIGIN_FILE"]
#[cfg(unix)]
async fn source_built_omp_context_inspection_preserves_provider_and_turn_state() {
    use std::os::unix::fs::PermissionsExt;
    use zeron_harness::omp::process::{OmpLaunch, OmpProcess};
    let launcher = std::env::var("COMET_OMP_CONTEXT_LAUNCH").unwrap();
    let origin_file = std::env::var("COMET_OMP_CONTEXT_ORIGIN_FILE").unwrap();
    let dir = tempfile::tempdir().unwrap();
    let executable = dir.path().join("omp-source");
    std::fs::write(
        &executable,
        "#!/bin/sh\nexec \"$COMET_OMP_CONTEXT_LAUNCH\" --hold\n",
    )
    .unwrap();
    std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o755)).unwrap();
    let env = HashMap::from([("COMET_OMP_CONTEXT_LAUNCH".into(), launcher)]);
    let http = reqwest::Client::new();
    let origin = || {
        let url = std::fs::read_to_string(&origin_file).unwrap();
        let parsed = reqwest::Url::parse(&url).unwrap();
        assert_eq!(parsed.host_str(), Some("127.0.0.1"));
        url
    };
    async fn status(http: &reqwest::Client, origin: &str) -> serde_json::Value {
        http.get(format!("{origin}/_status"))
            .send()
            .await
            .unwrap()
            .error_for_status()
            .unwrap()
            .json()
            .await
            .unwrap()
    }
    // First prove an existing native idle process through the adapter's actual
    // transport/normalizer. Engine's explicit idle-start API is Codex-only;
    // GetChatContextBreakdown must never manufacture an OMP run to inspect.
    let native = OmpProcess::start(OmpLaunch {
        executable: executable.clone(),
        cwd: dir.path().into(),
        ephemeral: true,
        system_prompt_append: None,
        env: Some(env.clone()),
        handshake_timeout: Duration::from_secs(30),
        request_timeout: Duration::from_secs(10),
    })
    .await
    .unwrap();
    let idle_origin = origin();
    let idle_a = native
        .request(json!({"type":"get_context_breakdown"}))
        .await
        .unwrap();
    let idle_b = native
        .request(json!({"type":"get_context_breakdown"}))
        .await
        .unwrap();
    assert_eq!(idle_a["identity"], idle_b["identity"]);
    assert_eq!(idle_a["total"], idle_b["total"]);
    assert_eq!(idle_a["identity"]["messageCount"], 0);
    assert_eq!(status(&http, &idle_origin).await["requests"], 0);
    let idle_report = zeron_harness::context_breakdown::omp(&idle_b);
    assert_eq!(
        idle_report.native.as_ref().unwrap().identity.message_count,
        Some(0)
    );
    native.shutdown().await.unwrap();
    // Shutdown is awaited before starting the second launcher: its runtime dir
    // and provider origin are single-instance resources owned by this fixture.
    let harness = OmpHarness::new()
        .with_executable(executable)
        .with_env(env)
        .with_timeouts(Duration::from_secs(30), Duration::from_secs(15));
    let registry = HarnessRegistry::new();
    registry.register(Arc::new(harness));
    let core = EngineCore::assemble(dir.path(), Arc::new(registry), HarnessId::Omp, None).unwrap();
    core.workspace
        .create_chat("source-chat", None, Some(&core.device_id), None, None)
        .unwrap();
    let client = zeron_rpc::memory_client(core.rpc_service());
    let no_runtime: ContextBreakdown = serde_json::from_value(
        client
            .call("GetChatContextBreakdown", json!({"chatId":"source-chat"}))
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(no_runtime.availability, ContextAvailability::NotApplicable);
    assert!(!core.sessions.has_live_run("source-chat"));
    let request = RunRequest {
        prompt: "first synthetic fixture prompt".into(),
        harness: Some(HarnessId::Omp),
        model: Some("fixture/fixture".into()),
        reasoning: None,
        model_options: Default::default(),
        access: Default::default(),
        plan: Default::default(),
        cwd: dir.path().to_string_lossy().into_owned(),
        sandbox: SandboxLevel::WorkspaceWrite,
        auto_approve: false,
        enable_workers_mcp: false,
        workers_parent_chat_id: None,
        sessions: None,
        resume: None,
        attachments: vec![],
        worktree: None,
        mcp: None,
        extra_mcp: Default::default(),
        env: Default::default(),
    };
    let (_, mut events) = core.sessions.subscribe("source-chat", 0).unwrap();
    core.sessions
        .dispatch("source-chat", HarnessId::Omp, request.clone(), None)
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(30), async {
        loop {
            if matches!(
                events.recv().await.unwrap().event,
                zeron_proto::AgentEvent::TextDelta { .. }
            ) {
                break;
            }
        }
    })
    .await
    .unwrap();
    let host_origin = origin();
    let active_before = status(&http, &host_origin).await;
    assert_eq!(active_before["requests"], 1);
    let journal_before = core.sessions.subscribe("source-chat", 0).unwrap().0;
    let active_a: ContextBreakdown = serde_json::from_value(
        client
            .call("GetChatContextBreakdown", json!({"chatId":"source-chat"}))
            .await
            .unwrap(),
    )
    .unwrap();
    let active_b: ContextBreakdown = serde_json::from_value(
        client
            .call("GetChatContextBreakdown", json!({"chatId":"source-chat"}))
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(active_a.streaming, Some(true));
    assert_eq!(active_b.pending_request, Some(true));
    assert_eq!(
        active_a.native.as_ref().unwrap().identity,
        active_b.native.as_ref().unwrap().identity
    );
    assert_eq!(active_a.used_tokens, active_b.used_tokens);
    assert_eq!(active_a.categories, active_b.categories);
    assert_eq!(
        active_a.native.as_ref().unwrap().has_local_changes,
        active_b.native.as_ref().unwrap().has_local_changes
    );
    assert_eq!(
        status(&http, &host_origin).await,
        active_before,
        "inspection must not change provider count/payload hashes"
    );
    let journal_after = core.sessions.subscribe("source-chat", 0).unwrap().0;
    assert_eq!(
        journal_before.len(),
        journal_after.len(),
        "no visible turn/event from inspection"
    );
    let entries_before = core
        .doc_host
        .open("source-chat")
        .unwrap()
        .doc()
        .read_entries()
        .unwrap();
    let _again: ContextBreakdown = serde_json::from_value(
        client
            .call("GetChatContextBreakdown", json!({"chatId":"source-chat"}))
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(
        core.doc_host
            .open("source-chat")
            .unwrap()
            .doc()
            .read_entries()
            .unwrap(),
        entries_before
    );
    http.get(format!("{host_origin}/_release"))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap();
    tokio::time::timeout(Duration::from_secs(15), async {
        loop {
            if matches!(
                events.recv().await.unwrap().event,
                zeron_proto::AgentEvent::Done { .. }
            ) {
                break;
            }
        }
    })
    .await
    .unwrap();
    let completed: ContextBreakdown = serde_json::from_value(
        client
            .call("GetChatContextBreakdown", json!({"chatId":"source-chat"}))
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(completed.streaming, Some(false));
    assert_eq!(completed.used_basis.as_deref(), Some("provider-anchor"));
    assert_eq!(completed.used_tokens, Some(1234));
    assert_eq!(status(&http, &host_origin).await["requests"], 1);
    let mut next = request;
    next.prompt = "second synthetic fixture prompt".into();
    core.sessions
        .dispatch("source-chat", HarnessId::Omp, next, None)
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(15), async {
        loop {
            if matches!(
                events.recv().await.unwrap().event,
                zeron_proto::AgentEvent::Done { .. }
            ) {
                break;
            }
        }
    })
    .await
    .unwrap();
    let final_provider = status(&http, &host_origin).await;
    assert_eq!(final_provider["requests"], 2);
    assert_eq!(
        final_provider["summaries"][0]["toolsHash"],
        final_provider["summaries"][1]["toolsHash"]
    );
    let replay = core.sessions.subscribe("source-chat", 0).unwrap().0;
    assert_eq!(
        replay
            .iter()
            .filter(|e| matches!(e.event, zeron_proto::AgentEvent::Done { .. }))
            .count(),
        2
    );
    let receipt = json!({"sourceBuilt":true,"installedRuntime":false,
        "nativeIdleTransport":{"requests":0,"messageCount":0,"report":idle_report},
        "hostBeforeRuntime":no_runtime,"activeBefore":active_before,"activeReports":[active_a,active_b],
        "completed":completed,"finalProvider":final_provider,"inspectionAddedEvents":0,"visibleDoneCount":2,
        "limitations":["Initial zero-request native idle is adapter transport/normalizer proof; host never spawns for inspection. Full Engine/RPC proof covers active, between turns and next prompt."]});
    core.sessions.shutdown().await;
    if let Ok(path) = std::env::var("COMET_CONTEXT_SMOKE_RECEIPT") {
        std::fs::write(path, serde_json::to_vec_pretty(&receipt).unwrap()).unwrap();
    }
    println!(
        "SANITIZED source-built OMP smoke: native idle0, host active inspections1, completed1, next prompt2; stable identity/categories/provider hashes, no inspection events; two normal turn ends"
    );
}

#[tokio::test]
#[cfg(unix)]
async fn context_rpc_never_spawns_and_reads_only_the_selected_live_host() {
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("wire");
    let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../harness/tests/fixtures/fake-omp-rpc.sh");
    let harness = OmpHarness::new()
        .with_executable(fixture)
        .with_env(HashMap::from([
            ("FAKE_OMP_SCENARIO".into(), "multi-turn".into()),
            ("FAKE_OMP_LOG".into(), log.to_string_lossy().into_owned()),
        ]))
        .with_timeouts(Duration::from_secs(2), Duration::from_secs(2));
    let registry = HarnessRegistry::new();
    registry.register(Arc::new(harness));
    let core = EngineCore::assemble(dir.path(), Arc::new(registry), HarnessId::Omp, None).unwrap();
    core.workspace
        .create_chat("chat", None, Some(&core.device_id), None, None)
        .unwrap();
    core.workspace
        .create_chat("foreign", None, Some("another-host"), None, None)
        .unwrap();
    let client = Arc::new(zeron_rpc::memory_client(core.rpc_service()));
    let empty: ContextBreakdown = serde_json::from_value(
        client
            .call("GetChatContextBreakdown", json!({"chatId":"chat"}))
            .await
            .expect("native context RPC is registered"),
    )
    .unwrap();
    assert_eq!(empty.availability, ContextAvailability::NotApplicable);
    assert!(!log.exists(), "inspection must never spawn a process");
    assert!(
        client
            .call("GetChatContextBreakdown", json!({"chatId":"foreign"}))
            .await
            .is_err()
    );
    assert!(
        client
            .call("GetChatContextBreakdown", json!({"chatId":"missing"}))
            .await
            .is_err()
    );
    let request = RunRequest {
        prompt: "first".into(),
        harness: Some(HarnessId::Omp),
        model: None,
        reasoning: None,
        model_options: Default::default(),
        access: Default::default(),
        plan: Default::default(),
        cwd: dir.path().to_string_lossy().into_owned(),
        sandbox: SandboxLevel::WorkspaceWrite,
        auto_approve: false,
        enable_workers_mcp: false,
        workers_parent_chat_id: None,
        sessions: None,
        resume: None,
        attachments: vec![],
        worktree: None,
        mcp: None,
        extra_mcp: Default::default(),
        env: Default::default(),
    };
    core.sessions
        .dispatch("chat", HarnessId::Omp, request, None)
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(5), async {
        while core
            .sessions
            .session_status("chat")
            .is_none_or(|s| s.status != SessionStatus::Idle)
        {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    let (a, b) = tokio::join!(
        client.call("GetChatContextBreakdown", json!({"chatId":"chat"})),
        client.call("GetChatContextBreakdown", json!({"chatId":"chat"}))
    );
    let first: ContextBreakdown = serde_json::from_value(a.unwrap()).unwrap();
    let second: ContextBreakdown = serde_json::from_value(b.unwrap()).unwrap();
    assert_eq!(first, second);
    assert_eq!(first.used_tokens, Some(828300));
    assert_eq!(first.host_device_id, core.device_id);
    assert_eq!(first.chat_id, "chat");
    assert!(!first.stale);
    assert_eq!(first.native_session_id.as_deref(), Some("s-1"));
    assert!(!first.runtime_id.is_empty());
    assert_eq!(
        std::fs::read_to_string(&log)
            .unwrap()
            .lines()
            .filter(|l| l.ends_with(" get_context_breakdown"))
            .count(),
        1,
        "concurrent host reads coalesce"
    );
    core.sessions.terminate("chat").await.unwrap();
    let retained: ContextBreakdown = serde_json::from_value(
        client
            .call("GetChatContextBreakdown", json!({"chatId":"chat"}))
            .await
            .unwrap(),
    )
    .unwrap();
    assert!(retained.stale);
    assert_eq!(retained.used_tokens, first.used_tokens);
    assert_eq!(
        std::fs::read_to_string(&log)
            .unwrap()
            .lines()
            .filter(|l| l.ends_with(" prompt"))
            .count(),
        1
    );
    println!(
        "SANITIZED host/RPC smoke: no-spawn, ownership, coalescing, native categories, stale retained snapshot; one prompt, one inspection"
    );
    core.sessions.shutdown().await;
}

struct ControlledInspection {
    pending: tokio::sync::mpsc::UnboundedSender<
        zeron_harness::context_inspection::ContextInspectionRequest,
    >,
    starts: Arc<std::sync::atomic::AtomicUsize>,
}
#[async_trait::async_trait]
impl zeron_harness::Harness for ControlledInspection {
    fn id(&self) -> HarnessId {
        HarnessId::Omp
    }
    fn display_name(&self) -> &str {
        "Controlled inspection"
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
        request: RunRequest,
        controls: zeron_harness::RunControls,
    ) -> Result<
        futures::stream::BoxStream<
            'static,
            Result<zeron_proto::AgentEvent, zeron_harness::HarnessError>,
        >,
        zeron_harness::HarnessError,
    > {
        use futures::StreamExt;
        let count = self
            .starts
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst)
            + 1;
        let mut inspection = controls.turn.context_requests();
        let pending = self.pending.clone();
        let (tx, rx) = tokio::sync::mpsc::channel(16);
        tokio::spawn(async move {
            tx.send(Ok(zeron_proto::AgentEvent::SessionStarted {
                harness: HarnessId::Omp,
                model: request.model.unwrap_or_default(),
                tools: vec![],
                cwd: request.cwd,
                session_id: format!("session-{count}"),
                assistant_message_id: format!("a-{count}"),
            }))
            .await
            .unwrap();
            tx.send(Ok(zeron_proto::AgentEvent::Done {
                status: zeron_proto::DoneStatus::Completed,
                result: None,
                error: None,
                session_id: Some(format!("session-{count}")),
            }))
            .await
            .unwrap();
            loop {
                tokio::select! {
                    _=controls.interrupt.cancelled()=>break,
                    request=inspection.recv()=>{let Some(request)=request else{break;};if pending.send(request).is_err(){break;}}
                }
            }
        });
        Ok(futures::stream::unfold(rx, |mut rx| async move {
            rx.recv().await.map(|event| (event, rx))
        })
        .boxed())
    }
}

#[tokio::test]
async fn only_available_context_reports_are_retained_after_failures_and_runtime_exit() {
    use zeron_harness::HarnessError;
    let (pending, mut native) = tokio::sync::mpsc::unbounded_channel();
    let registry = HarnessRegistry::new();
    registry.register(Arc::new(ControlledInspection {
        pending,
        starts: Arc::new(std::sync::atomic::AtomicUsize::new(0)),
    }));
    let dir = tempfile::tempdir().unwrap();
    let core = EngineCore::assemble(dir.path(), Arc::new(registry), HarnessId::Omp, None).unwrap();
    let request = RunRequest {
        prompt: "synthetic".into(),
        harness: Some(HarnessId::Omp),
        model: Some("selected-model".into()),
        reasoning: None,
        model_options: Default::default(),
        access: Default::default(),
        plan: false,
        cwd: dir.path().to_string_lossy().into_owned(),
        sandbox: SandboxLevel::WorkspaceWrite,
        auto_approve: false,
        enable_workers_mcp: false,
        workers_parent_chat_id: None,
        sessions: None,
        resume: None,
        attachments: vec![],
        worktree: None,
        mcp: None,
        extra_mcp: Default::default(),
        env: Default::default(),
    };
    let client = Arc::new(zeron_rpc::memory_client(core.rpc_service()));
    let mut actual = Vec::new();
    let mut expected = Vec::new();
    for (index, availability) in [
        ContextAvailability::Error,
        ContextAvailability::NotApplicable,
        ContextAvailability::Partial,
        ContextAvailability::Unsupported,
        ContextAvailability::Available,
    ]
    .into_iter()
    .enumerate()
    {
        let chat = format!("retention-{index}");
        core.workspace
            .create_chat(&chat, None, Some(&core.device_id), None, None)
            .unwrap();
        core.sessions
            .dispatch(&chat, HarnessId::Omp, request.clone(), None)
            .await
            .unwrap();
        tokio::time::timeout(Duration::from_secs(5), async {
            while core
                .sessions
                .session_status(&chat)
                .is_none_or(|s| s.status != SessionStatus::Idle)
            {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        let caller = client.clone();
        let id = chat.clone();
        let first = tokio::spawn(async move {
            caller
                .call("GetChatContextBreakdown", json!({"chatId":id}))
                .await
        });
        let first_request = tokio::time::timeout(Duration::from_secs(2), native.recv())
            .await
            .unwrap()
            .unwrap();
        let reply = match availability {
            ContextAvailability::Error => Err(HarnessError::Protocol("first failure".into())),
            ContextAvailability::Available | ContextAvailability::Partial => Ok(ContextBreakdown {
                availability,
                used_tokens: Some(123),
                ..Default::default()
            }),
            _ => Ok(ContextBreakdown::unavailable(
                availability,
                "first unavailable",
            )),
        };
        first_request.reply.send(reply).unwrap();
        let first: ContextBreakdown =
            serde_json::from_value(first.await.unwrap().unwrap()).unwrap();
        assert!(!first.stale);
        let caller = client.clone();
        let id = chat.clone();
        let mut second = tokio::spawn(async move {
            caller
                .call("GetChatContextBreakdown", json!({"chatId":id}))
                .await
        });
        // Answer an unexpected Unsupported re-probe too, so losing the
        // short-circuit fails an assertion rather than a timeout.
        let (second, inspected) = tokio::select! {
            result = &mut second => (result.unwrap().unwrap(), false),
            pending = native.recv() => {
                pending.unwrap().reply.send(Err(HarnessError::Protocol("second failure".into()))).unwrap();
                (second.await.unwrap().unwrap(), true)
            }
        };
        let second: ContextBreakdown = serde_json::from_value(second).unwrap();
        let available = availability == ContextAvailability::Available;
        let unsupported = availability == ContextAvailability::Unsupported;
        let expected_second = if available {
            ContextAvailability::Available
        } else if unsupported {
            ContextAvailability::Unsupported
        } else {
            ContextAvailability::Error
        };
        actual.push((
            availability,
            "live",
            second.availability,
            second.stale,
            second.used_tokens,
            second
                .reason
                .as_deref()
                .is_some_and(|r| r.contains("retained snapshot")),
            inspected,
        ));
        expected.push((
            availability,
            "live",
            expected_second,
            available,
            if available { Some(123) } else { None },
            available,
            !unsupported,
        ));
        if available {
            assert_eq!(second.observed_at_ms, first.observed_at_ms);
        }
        if unsupported {
            assert_eq!(
                second, first,
                "Unsupported keeps its existing short-circuit"
            );
        }
        core.sessions.terminate(&chat).await.unwrap();
        let ended: ContextBreakdown = serde_json::from_value(
            client
                .call("GetChatContextBreakdown", json!({"chatId":chat}))
                .await
                .unwrap(),
        )
        .unwrap();
        actual.push((
            availability,
            "ended",
            ended.availability,
            ended.stale,
            ended.used_tokens,
            ended
                .reason
                .as_deref()
                .is_some_and(|r| r.contains("retained snapshot")),
            false,
        ));
        expected.push((
            availability,
            "ended",
            if available {
                ContextAvailability::Available
            } else {
                ContextAvailability::NotApplicable
            },
            available,
            if available { Some(123) } else { None },
            available,
            false,
        ));
    }
    core.shutdown().await;
    assert_eq!(
        actual, expected,
        "only an Available measurement can become a retained stale snapshot"
    );
}

#[tokio::test]
async fn context_rpc_preserves_protocol_reasons_and_keeps_other_errors_generic() {
    use zeron_harness::HarnessError;
    let (pending, mut native) = tokio::sync::mpsc::unbounded_channel();
    let registry = HarnessRegistry::new();
    registry.register(Arc::new(ControlledInspection {
        pending,
        starts: Arc::new(std::sync::atomic::AtomicUsize::new(0)),
    }));
    let dir = tempfile::tempdir().unwrap();
    let core = EngineCore::assemble(dir.path(), Arc::new(registry), HarnessId::Omp, None).unwrap();
    let request = RunRequest {
        prompt: "synthetic".into(),
        harness: Some(HarnessId::Omp),
        model: Some("selected-model".into()),
        reasoning: None,
        model_options: Default::default(),
        access: Default::default(),
        plan: false,
        cwd: dir.path().to_string_lossy().into_owned(),
        sandbox: SandboxLevel::WorkspaceWrite,
        auto_approve: false,
        enable_workers_mcp: false,
        workers_parent_chat_id: None,
        sessions: None,
        resume: None,
        attachments: vec![],
        worktree: None,
        mcp: None,
        extra_mcp: Default::default(),
        env: Default::default(),
    };
    let client = Arc::new(zeron_rpc::memory_client(core.rpc_service()));
    let mut outcomes = Vec::new();
    // Both protocol reasons must survive the serialized host boundary. Other
    // errors must still hide implementation-specific details; Unsupported stays distinct.
    for (index, (error, availability, reason)) in [
        (
            HarnessError::Protocol("Runtime configuration is not settled".into()),
            ContextAvailability::Error,
            "Runtime configuration is not settled",
        ),
        (
            HarnessError::Protocol("Context inspection is busy".into()),
            ContextAvailability::Error,
            "Context inspection is busy",
        ),
        (
            HarnessError::Io(std::io::Error::other("private transport diagnostic")),
            ContextAvailability::Error,
            "Context inspection failed or timed out",
        ),
        (
            HarnessError::Unsupported("Runtime update required".into()),
            ContextAvailability::Unsupported,
            "Runtime update required",
        ),
    ]
    .into_iter()
    .enumerate()
    {
        // Separate Chats keep prior-error retention and Unsupported caching
        // from changing the classification/reason under test.
        let chat_id = format!("error-{index}");
        core.workspace
            .create_chat(&chat_id, None, Some(&core.device_id), None, None)
            .unwrap();
        core.sessions
            .dispatch(&chat_id, HarnessId::Omp, request.clone(), None)
            .await
            .unwrap();
        tokio::time::timeout(Duration::from_secs(5), async {
            while core
                .sessions
                .session_status(&chat_id)
                .is_none_or(|s| s.status != SessionStatus::Idle)
            {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        let caller = client.clone();
        let requested_chat = chat_id.clone();
        let read = tokio::spawn(async move {
            caller
                .call("GetChatContextBreakdown", json!({"chatId":requested_chat}))
                .await
        });
        let waiter = tokio::time::timeout(Duration::from_secs(2), native.recv())
            .await
            .unwrap()
            .unwrap();
        waiter.reply.send(Err(error)).unwrap();
        let report: ContextBreakdown =
            serde_json::from_value(read.await.unwrap().unwrap()).unwrap();
        assert_eq!(report.chat_id, chat_id);
        assert_eq!(report.host_device_id, core.device_id);
        assert_eq!(report.used_tokens, None);
        outcomes.push((
            (availability, reason.to_string()),
            (report.availability, report.reason.unwrap()),
        ));
    }
    core.shutdown().await;
    for (expected, actual) in outcomes {
        assert_eq!(
            actual, expected,
            "native protocol reason must cross the context RPC unchanged"
        );
    }
}

#[tokio::test]
async fn context_rpc_rejects_late_model_and_runtime_results_without_caching_them() {
    let (pending, mut native) = tokio::sync::mpsc::unbounded_channel();
    let starts = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let registry = HarnessRegistry::new();
    registry.register(Arc::new(ControlledInspection {
        pending,
        starts: starts.clone(),
    }));
    let dir = tempfile::tempdir().unwrap();
    let core = EngineCore::assemble(dir.path(), Arc::new(registry), HarnessId::Omp, None).unwrap();
    core.workspace
        .create_chat("chat", None, Some(&core.device_id), None, None)
        .unwrap();
    let config = |model: &str| zeron_proto::ChatConfig {
        harness: HarnessId::Omp,
        model: Some(model.into()),
        reasoning: None,
        model_options: Default::default(),
        access: Default::default(),
        plan: Default::default(),
        sandbox: SandboxLevel::WorkspaceWrite,
    };
    let request = |model: &str| RunRequest {
        prompt: "synthetic".into(),
        harness: Some(HarnessId::Omp),
        model: Some(model.into()),
        reasoning: None,
        model_options: Default::default(),
        access: Default::default(),
        plan: Default::default(),
        cwd: dir.path().to_string_lossy().into_owned(),
        sandbox: SandboxLevel::WorkspaceWrite,
        auto_approve: false,
        enable_workers_mcp: false,
        workers_parent_chat_id: None,
        sessions: None,
        resume: None,
        attachments: vec![],
        worktree: None,
        mcp: None,
        extra_mcp: Default::default(),
        env: Default::default(),
    };
    core.workspace
        .set_chat_config("chat", &config("old"))
        .unwrap();
    core.sessions
        .dispatch("chat", HarnessId::Omp, request("old"), None)
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(5), async {
        while core
            .sessions
            .session_status("chat")
            .is_none_or(|s| s.status != SessionStatus::Idle)
        {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    let client = Arc::new(zeron_rpc::memory_client(core.rpc_service()));
    let caller = client.clone();
    let first = tokio::spawn(async move {
        caller
            .call("GetChatContextBreakdown", json!({"chatId":"chat"}))
            .await
    });
    let waiter = tokio::time::timeout(Duration::from_secs(2), native.recv())
        .await
        .unwrap()
        .unwrap();
    core.workspace
        .set_chat_config("chat", &config("new"))
        .unwrap();
    waiter
        .reply
        .send(Ok(ContextBreakdown {
            used_tokens: Some(999),
            ..Default::default()
        }))
        .unwrap();
    let rejected: ContextBreakdown = serde_json::from_value(first.await.unwrap().unwrap()).unwrap();
    assert_eq!(rejected.availability, ContextAvailability::NotApplicable);
    assert_eq!(rejected.used_tokens, None);
    // The selected model changed without a matching runtime: do not inspect the old process.
    let unavailable: ContextBreakdown = serde_json::from_value(
        client
            .call("GetChatContextBreakdown", json!({"chatId":"chat"}))
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(unavailable.used_tokens, None);
    assert!(native.try_recv().is_err());
    let mut other_settings = config("old");
    other_settings.reasoning = Some(zeron_proto::ReasoningLevel::High);
    core.workspace
        .set_chat_config("chat", &other_settings)
        .unwrap();
    let unavailable: ContextBreakdown = serde_json::from_value(
        client
            .call("GetChatContextBreakdown", json!({"chatId":"chat"}))
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(unavailable.availability, ContextAvailability::NotApplicable);
    assert!(
        native.try_recv().is_err(),
        "settings from another runtime must not be inspected"
    );
    // Keep model/reasoning/options/sandbox identical to the warm runtime.
    // A change to access alone or plan alone must reject it before inspection.
    let mut access_only = config("old");
    access_only.access = zeron_proto::AccessMode::Supervised;
    let mut plan_only = config("old");
    plan_only.plan = true;
    let mut outcomes = Vec::new();
    for (setting, selected) in [("access", access_only), ("plan", plan_only)] {
        core.workspace.set_chat_config("chat", &selected).unwrap();
        let read = client.call("GetChatContextBreakdown", json!({"chatId":"chat"}));
        tokio::pin!(read);
        let (reply, inspected) = tokio::select! {
            reply = &mut read => (reply, false),
            waiter = native.recv() => {
                // Let an incorrect implementation finish so both independent
                // settings are checked, without relying on a timeout failure.
                waiter.unwrap().reply.send(Ok(ContextBreakdown {
                    used_tokens: Some(777),
                    ..Default::default()
                })).unwrap();
                (read.await, true)
            }
        };
        let report: ContextBreakdown = serde_json::from_value(reply.unwrap()).unwrap();
        outcomes.push((setting, report.availability, report.used_tokens, inspected));
    }
    assert_eq!(
        outcomes,
        vec![
            ("access", ContextAvailability::NotApplicable, None, false),
            ("plan", ContextAvailability::NotApplicable, None, false),
        ],
        "access/plan-only changes must not inspect or cache the old warm runtime"
    );
    core.workspace
        .set_chat_config("chat", &config("old"))
        .unwrap();
    let caller = client.clone();
    let late = tokio::spawn(async move {
        caller
            .call("GetChatContextBreakdown", json!({"chatId":"chat"}))
            .await
    });
    let waiter = tokio::time::timeout(Duration::from_secs(2), native.recv())
        .await
        .unwrap()
        .unwrap();
    core.sessions.terminate("chat").await.unwrap();
    waiter
        .reply
        .send(Ok(ContextBreakdown {
            used_tokens: Some(888),
            ..Default::default()
        }))
        .unwrap();
    let rejected: ContextBreakdown = serde_json::from_value(late.await.unwrap().unwrap()).unwrap();
    assert_eq!(rejected.used_tokens, None);
    let no_cache: ContextBreakdown = serde_json::from_value(
        client
            .call("GetChatContextBreakdown", json!({"chatId":"chat"}))
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(no_cache.used_tokens, None);
    assert_eq!(starts.load(std::sync::atomic::Ordering::SeqCst), 1);
    core.sessions.shutdown().await;
}

#[tokio::test]
async fn context_reply_cannot_become_current_after_chat_deletion() {
    let (pending, mut native) = tokio::sync::mpsc::unbounded_channel();
    let registry = HarnessRegistry::new();
    registry.register(Arc::new(ControlledInspection {
        pending,
        starts: Arc::new(std::sync::atomic::AtomicUsize::new(0)),
    }));
    let dir = tempfile::tempdir().unwrap();
    let core = EngineCore::assemble(dir.path(), Arc::new(registry), HarnessId::Omp, None).unwrap();
    core.workspace
        .create_chat("chat", None, Some(&core.device_id), None, None)
        .unwrap();
    let request = RunRequest {
        prompt: "synthetic".into(),
        harness: Some(HarnessId::Omp),
        model: Some("selected-model".into()),
        reasoning: None,
        model_options: Default::default(),
        access: Default::default(),
        plan: Default::default(),
        cwd: dir.path().to_string_lossy().into_owned(),
        sandbox: SandboxLevel::WorkspaceWrite,
        auto_approve: false,
        enable_workers_mcp: false,
        workers_parent_chat_id: None,
        sessions: None,
        resume: None,
        attachments: vec![],
        worktree: None,
        mcp: None,
        extra_mcp: Default::default(),
        env: Default::default(),
    };
    core.sessions
        .dispatch("chat", HarnessId::Omp, request, None)
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(5), async {
        while core
            .sessions
            .session_status("chat")
            .is_none_or(|s| s.status != SessionStatus::Idle)
        {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    let client = Arc::new(zeron_rpc::memory_client(core.rpc_service()));
    let caller = client.clone();
    let initial = tokio::spawn(async move {
        caller
            .call("GetChatContextBreakdown", json!({"chatId":"chat"}))
            .await
    });
    native
        .recv()
        .await
        .unwrap()
        .reply
        .send(Ok(ContextBreakdown {
            schema_version: 1,
            used_tokens: Some(123),
            availability: ContextAvailability::Available,
            ..Default::default()
        }))
        .unwrap();
    let initial: ContextBreakdown =
        serde_json::from_value(initial.await.unwrap().unwrap()).unwrap();
    assert_eq!(
        initial.model, None,
        "a configured model is not native observation evidence"
    );
    let caller = client.clone();
    let invalid = tokio::spawn(async move {
        caller
            .call("GetChatContextBreakdown", json!({"chatId":"chat"}))
            .await
    });
    let failed_request = native.recv().await.unwrap();
    let caller = client.clone();
    let queued = tokio::spawn(async move {
        caller
            .call("GetChatContextBreakdown", json!({"chatId":"chat"}))
            .await
    });
    tokio::time::sleep(Duration::from_millis(10)).await;
    assert!(!queued.is_finished());
    failed_request
        .reply
        .send(Ok(ContextBreakdown::unavailable(
            ContextAvailability::Error,
            "Invalid native payload",
        )))
        .unwrap();
    let retained: ContextBreakdown =
        serde_json::from_value(invalid.await.unwrap().unwrap()).unwrap();
    let queued: ContextBreakdown = serde_json::from_value(
        tokio::time::timeout(Duration::from_millis(200), queued)
            .await
            .expect("queued readers must reuse the failed refresh outcome")
            .unwrap()
            .unwrap(),
    )
    .unwrap();
    assert_eq!(queued, retained);
    assert_eq!(retained.used_tokens, Some(123));
    assert_eq!(retained.observed_at_ms, initial.observed_at_ms);
    assert!(retained.stale);
    let caller = client.clone();
    let inspection = tokio::spawn(async move {
        caller
            .call("GetChatContextBreakdown", json!({"chatId":"chat"}))
            .await
    });
    let waiter = tokio::time::timeout(Duration::from_secs(2), native.recv())
        .await
        .unwrap()
        .unwrap();
    core.workspace.delete_chat("chat").unwrap();
    waiter
        .reply
        .send(Ok(ContextBreakdown {
            used_tokens: Some(777),
            ..Default::default()
        }))
        .unwrap();
    let rejected: ContextBreakdown =
        serde_json::from_value(inspection.await.unwrap().unwrap()).unwrap();
    assert_eq!(
        rejected.used_tokens, None,
        "deleting the selected Chat must reject an outstanding native result"
    );
    core.sessions.shutdown().await;
}
