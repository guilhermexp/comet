use super::*;
use crate::details_sidebar::context::DetailsMode;
use gpui::{AppContext, Render, Window};
use zeron_doc::{MessagePart, MessageRole, SessionMessageEntry};
use zeron_proto::ToolCall;
use zeron_workers_unpeel::{WorkersProject, WorkersSessionSort};

fn make_project(id: &str, name: &str, path: &str) -> WorkersProject {
    WorkersProject {
        id: id.to_string(),
        name: name.to_string(),
        path: path.to_string(),
        folder_id: None,
        parent_project_id: None,
        is_group: false,
        worktree_branch: None,
        git_branch: None,
        archived_session_count: 0,
        folder_color_id: None,
        session_sort: WorkersSessionSort::Custom,
        repository_id: None,
        repository_name: None,
        repository_path: None,
        checkout_kind: None,
        checkout_ownership: None,
        checkout_availability: None,
        checkout_archived: false,
        checkout_detached: false,
        space_id: None,
    }
}

fn assistant_entry(calls: Vec<ToolCall>) -> SessionMessageEntry {
    SessionMessageEntry {
        id: "msg-1".to_string(),
        role: MessageRole::Assistant,
        parts: calls
            .into_iter()
            .enumerate()
            .map(|(idx, call)| MessagePart::Tool {
                id: format!("tool-{idx}"),
                call,
                diff: None,
                output: None,
                output_ref: None,
                output_bytes: None,
                diff_ref: None,
                diff_stats: None,
                file_preview: None,
                subagent_ref: None,
                subagent_status: None,
                subagent_tail: None,
                subagent_end: None,
                execution: None,
                resolved: true,
                is_error: false,
            })
            .collect(),
        created_at: 0,
        device_id: "dev-1".to_string(),
        status: None,
        duration_ms: None,
        continuation_of: None,
    }
}

#[gpui::test]
fn worked_project_diff_updates_and_visibility_cancel_the_local_stream(
    cx: &mut gpui::TestAppContext,
) {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let _guard = runtime.enter();
    let data = tempfile::tempdir().unwrap();
    cx.update(|cx| {
        gpui_base::init(cx);
        crate::settings::init(crate::settings::UiSettings::default(), data.path(), cx);
        cx.set_global(Theme::dark());
    });
    let (out, mut requests) = tokio::sync::mpsc::channel::<String>(64);
    let (replies, inbound) = tokio::sync::mpsc::channel::<String>(64);
    let mut watches = Vec::new();
    let mut cancellations = Vec::new();
    let state = cx.new(|_| AppState::new());
    state.update(cx, |state, _| {
        state.set_test_engine(crate::state::EngineHandle::from_test_client(
            zeron_rpc::RpcClient::new(out, inbound),
        ));
        state.local_device_id = Some("local".into());
        state.transcript = vec![assistant_entry(vec![
            ToolCall::ReadFile {
                path: "/tmp/worked-first/file.txt".into(),
            },
            ToolCall::ReadFile {
                path: "/tmp/worked-second/file.txt".into(),
            },
        ])];
    });
    let workers = cx.new(|cx| WorkersModel::detached(state.clone(), cx));
    workers.update(cx, |workers, _| {
        workers.snapshot = Some(zeron_workers_unpeel::WorkersBootstrap {
            mac_name: "fixture".into(),
            protocol: zeron_workers_unpeel::WorkersProtocol {
                major_version: 1,
                minor_version: 0,
                capabilities: vec![],
            },
            projects: vec![
                { let mut project = make_project("first", "Very long Worker worktree project name that must truncate before its counter", "/tmp/worked-first"); project.worktree_branch = Some("worker-branch".into()); project },
                make_project("second", "Second project", "/tmp/worked-second"),
            ],
            presets: vec![],
            sessions: vec![],
            activity_log: vec![],
        });
    });
    let composer = cx.new(|cx| Composer::new(state.clone(), cx));
    let pickers = composer.read_with(cx, |composer, _| composer.pickers().clone());
    let sidebar = cx.new(|cx| {
        DetailsSidebar::new(
            state,
            workers,
            DetailsSidebarPreferences::default(),
            pickers,
            composer,
            cx,
        )
    });
    sidebar.update(cx, |sidebar, _| {
        sidebar.sidebar.set_context(Some(DetailsContext {
            key: "fixture".into(),
            cwd: "/tmp/orchestrator".into(),
            branch: Some("main".into()),
            chat_id: None,
            target_device_id: Some("remote-chat-host".into()),
            mode: DetailsMode::Orchestrator,
        }));
    });
    struct Probe {
        sidebar: Entity<DetailsSidebar>,
        _observe: Subscription,
    }
    impl Render for Probe {
        fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            let theme = Theme::of(cx).clone();
            let body = self
                .sidebar
                .update(cx, |sidebar, cx| sidebar.render_details(&theme, cx));
            div().w(px(360.0)).h(px(900.0)).child(body)
        }
    }
    sidebar.update(cx, |sidebar, _| {
        sidebar.sidebar.toggle_projects_worked_collapsed()
    });
    let (_, window) = cx.add_window_view(|_, cx| {
        let observe = cx.observe(&sidebar, |_, _, cx| cx.notify());
        Probe {
            sidebar: sidebar.clone(),
            _observe: observe,
        }
    });
    window.update(|window, cx| window.draw(cx).clear());
    window.run_until_parked();
    while let Ok(request) = requests.try_recv() {
        let request: zeron_rpc::ClientFrame = serde_json::from_str(&request).unwrap();
        assert!(
            request.method.as_deref() != Some(methods::WATCH_CHECKOUT_DIFFS)
                || request.params.get("cwds").is_none(),
            "initially collapsed section opens no Worked Project stream"
        );
        if !request.cancel {
            replies
                .try_send(
                    serde_json::to_string(&zeron_rpc::ServerFrame {
                        id: request.id,
                        ok: Some(serde_json::Value::Null),
                        ..Default::default()
                    })
                    .unwrap(),
                )
                .unwrap();
        }
    }
    runtime.block_on(async {
        tokio::task::yield_now().await;
    });
    sidebar.update(window, |sidebar, cx| {
        sidebar.sidebar.toggle_projects_worked_collapsed();
        cx.notify();
    });
    for _ in 0..20 {
        window.update(|window, cx| window.draw(cx).clear());
        window.run_until_parked();
        // Drive RPC and deliver server frames on the test's own thread.
        runtime.block_on(async {
            tokio::task::yield_now().await;
        });
        while let Ok(request) = requests.try_recv() {
            let request: zeron_rpc::ClientFrame = serde_json::from_str(&request).unwrap();
            if request.cancel {
                cancellations.push(request.id);
                continue;
            }
            let mut reply = zeron_rpc::ServerFrame {
                id: request.id,
                ..Default::default()
            };
            if request.method.as_deref() == Some(methods::WATCH_CHECKOUT_DIFFS) {
                if request.params.get("cwds").is_some() {
                    assert!(request.params.get("targetDeviceId").is_none());
                    assert_eq!(
                        request.params["cwds"],
                        serde_json::json!(["/tmp/worked-first", "/tmp/worked-second"])
                    );
                    watches.push(request.id);
                }
                let diff = |cwd: &str, additions, deletions| {
                    serde_json::json!({
                        "checkoutId": cwd, "deviceId": "local", "cwd": cwd,
                        "patch": "", "files": [{"path":"file.txt", "status":"modified", "additions":additions, "deletions":deletions}],
                        "additions": additions, "deletions": deletions, "truncated": false,
                        "checksum": cwd, "updatedAt": "2026-10-11T00:00:00Z"
                    })
                };
                reply.item = Some(serde_json::json!([
                    diff("/tmp/worked-first", 17, 4),
                    diff("/tmp/not-worked", 999, 888)
                ]));
            } else {
                reply.ok = Some(serde_json::Value::Null);
            }
            replies
                .try_send(serde_json::to_string(&reply).unwrap())
                .unwrap();
        }
        runtime.block_on(async {
            tokio::task::yield_now().await;
        });
    }
    let totals = window
        .debug_bounds("worked-project-diff-0")
        .expect("first project's +17 / −4 must appear");
    let row = window
        .debug_bounds("worked-project-row-0")
        .expect("first project row");
    assert!(totals.right() <= row.right());
    assert!(
        totals.left() > row.center().x,
        "totals must occupy the right of the row"
    );
    assert!(
        window.debug_bounds("worked-project-diff-1").is_none(),
        "missing diff must never fall back to another checkout"
    );
    assert!(window.debug_bounds("worked-project-diff-2").is_none());
    assert!(
        window
            .debug_bounds("worked-project-additions-0-17")
            .is_some()
    );
    assert!(
        window
            .debug_bounds("worked-project-deletions-0-4")
            .is_some()
    );
    assert!(
        window
            .debug_bounds("worked-project-additions-0-999")
            .is_none()
    );
    assert_eq!(watches.len(), 1, "repeated renders retain one local stream");
    let id = watches[0];
    let frame = |additions, files: serde_json::Value| {
        serde_json::to_string(&zeron_rpc::ServerFrame {
        id, item: Some(serde_json::json!({
            "checkoutId":"/tmp/worked-first", "deviceId":"local", "cwd":"/tmp/worked-first",
            "patch":"", "files":files, "additions":additions, "deletions":0,
            "truncated":false, "checksum":format!("updated-{additions}"), "updatedAt":"2026-10-11T00:00:01Z"
        })), ..Default::default()
    }).unwrap()
    };
    replies.try_send(frame(23, serde_json::json!([{"path":"file.txt", "status":"modified", "additions":23, "deletions":0}]))).unwrap();
    runtime.block_on(async {
        tokio::task::yield_now().await;
    });
    window.run_until_parked();
    window.update(|window, cx| window.draw(cx).clear());
    assert!(
        window
            .debug_bounds("worked-project-additions-0-23")
            .is_some(),
        "incremental diff updates the worktree row"
    );
    replies.try_send(frame(0, serde_json::json!([]))).unwrap();
    runtime.block_on(async {
        tokio::task::yield_now().await;
    });
    window.run_until_parked();
    window.update(|window, cx| window.draw(cx).clear());
    assert!(
        window.debug_bounds("worked-project-diff-0").is_none(),
        "clean checkout hides totals"
    );
    // Empty snapshot includes the non-Git/unavailable case without substituting
    // another checkout, and clears any prior counter.
    replies
        .try_send(
            serde_json::to_string(&zeron_rpc::ServerFrame {
                id,
                item: Some(serde_json::json!([])),
                ..Default::default()
            })
            .unwrap(),
        )
        .unwrap();
    runtime.block_on(async {
        tokio::task::yield_now().await;
    });
    window.run_until_parked();
    sidebar.update(window, |sidebar, cx| {
        sidebar.sidebar.toggle_projects_worked_collapsed();
        cx.notify();
    });
    window.update(|window, cx| window.draw(cx).clear());
    window.run_until_parked();
    while let Ok(request) = requests.try_recv() {
        let request: zeron_rpc::ClientFrame = serde_json::from_str(&request).unwrap();
        if request.cancel {
            cancellations.push(request.id);
        } else {
            assert!(
                request.method.as_deref() != Some(methods::WATCH_CHECKOUT_DIFFS)
                    || request.params.get("cwds").is_none(),
                "collapsed section must not subscribe"
            );
        }
    }
    assert!(
        cancellations.contains(&id),
        "collapse immediately cancels even a silent stream"
    );
    sidebar.read_with(window, |sidebar, _| {
        assert!(sidebar.worked_diff_watch.is_none());
        assert!(sidebar.worked_diffs.is_empty());
    });
    // Hidden Details uses the same cancellation boundary as section collapse.
    let paths = vec!["/tmp/worked-first".to_string()];
    sidebar.update(window, |sidebar, cx| {
        sidebar.ensure_worked_project_diff_watch(paths.clone(), cx);
    });
    window.run_until_parked();
    runtime.block_on(async {
        tokio::task::yield_now().await;
    });
    let request: zeron_rpc::ClientFrame =
        serde_json::from_str(&requests.try_recv().unwrap()).unwrap();
    assert_eq!(request.params["cwds"], serde_json::json!(paths));
    sidebar.update(window, |sidebar, cx| sidebar.hide_worked_project_diffs(cx));
    window.run_until_parked();
    let cancel: zeron_rpc::ClientFrame =
        serde_json::from_str(&requests.try_recv().unwrap()).unwrap();
    assert!(cancel.cancel);
    assert_eq!(cancel.id, request.id);

    // Showing again creates a fresh subscription. Ending it clears visible
    // totals and the key, allowing the same paths to subscribe again.
    sidebar.update(window, |sidebar, cx| {
        sidebar.ensure_worked_project_diff_watch(paths.clone(), cx);
        sidebar.worked_diffs.insert(
            paths[0].clone(),
            session_card::SessionDiffTotals {
                files: 1,
                additions: 7,
                deletions: 2,
            },
        );
    });
    window.run_until_parked();
    runtime.block_on(async {
        tokio::task::yield_now().await;
    });
    let request: zeron_rpc::ClientFrame =
        serde_json::from_str(&requests.try_recv().unwrap()).unwrap();
    replies
        .try_send(
            serde_json::to_string(&zeron_rpc::ServerFrame {
                id: request.id,
                done: true,
                ..Default::default()
            })
            .unwrap(),
        )
        .unwrap();
    runtime.block_on(async {
        tokio::task::yield_now().await;
    });
    window.run_until_parked();
    sidebar.read_with(window, |sidebar, _| {
        assert!(
            sidebar.worked_diff_key.is_none(),
            "ended stream must permit retry"
        );
        assert!(
            sidebar.worked_diffs.is_empty(),
            "ended stream removes stale totals"
        );
    });
    sidebar.update(window, |sidebar, cx| {
        sidebar.ensure_worked_project_diff_watch(paths.clone(), cx)
    });
    window.run_until_parked();
    runtime.block_on(async {
        tokio::task::yield_now().await;
    });
    let retry: zeron_rpc::ClientFrame =
        serde_json::from_str(&requests.try_recv().unwrap()).unwrap();
    assert_ne!(retry.id, request.id);
    // Connection EOF also ends subscriptions; RpcClient does not replay them.
    drop(replies);
    runtime.block_on(async {
        tokio::task::yield_now().await;
    });
    window.run_until_parked();
    sidebar.read_with(window, |sidebar, _| {
        assert!(sidebar.worked_diff_key.is_none())
    });

    // A closed outbound channel makes subscribe_scoped fail immediately.
    // Observe notifications directly: empty failures must not request redraws.
    drop(requests);
    let notifications = std::rc::Rc::new(std::cell::Cell::new(0));
    let observed = notifications.clone();
    let _observe = sidebar.update(window, |_, cx| {
        cx.observe(&sidebar, move |_, _, cx| {
            observed.set(observed.get() + 1);
            let _ = cx;
        })
    });
    for _ in 0..3 {
        sidebar.update(window, |sidebar, cx| {
            sidebar.ensure_worked_project_diff_watch(paths.clone(), cx)
        });
        window.run_until_parked();
        runtime.block_on(async {
            tokio::task::yield_now().await;
        });
        window.run_until_parked();
        sidebar.read_with(window, |sidebar, _| {
            assert!(
                sidebar.worked_diff_key.is_none(),
                "failed subscription must permit retry"
            );
            assert!(sidebar.worked_diffs.is_empty());
        });
    }
    assert_eq!(
        notifications.get(),
        0,
        "empty failures must not cause a redraw loop"
    );
}
