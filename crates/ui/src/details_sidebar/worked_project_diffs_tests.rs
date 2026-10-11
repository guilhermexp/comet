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
fn worked_project_rows_show_only_their_own_diff(cx: &mut gpui::TestAppContext) {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let _guard = runtime.enter();
    let data = tempfile::tempdir().unwrap();
    cx.update(|cx| {
        gpui_base::init(cx);
        crate::settings::init(crate::settings::UiSettings::default(), data.path(), cx);
        cx.set_global(Theme::dark());
    });
    let (out, mut requests) = tokio::sync::mpsc::channel::<String>(64);
    let (replies, inbound) = tokio::sync::mpsc::channel::<String>(64);
    let server = runtime.spawn(async move {
        while let Some(request) = requests.recv().await {
            let request: zeron_rpc::ClientFrame = serde_json::from_str(&request).unwrap();
            if request.cancel { continue; }
            let mut reply = zeron_rpc::ServerFrame { id: request.id, ..Default::default() };
            if request.method.as_deref() == Some(methods::WATCH_CHECKOUT_DIFFS) {
                let diff = |cwd: &str, additions, deletions| serde_json::json!({
                    "checkoutId": cwd, "deviceId": "local", "cwd": cwd,
                    "patch": "", "files": [{"path":"file.txt", "status":"modified", "additions":additions, "deletions":deletions}],
                    "additions": additions, "deletions": deletions, "truncated": false,
                    "checksum": cwd, "updatedAt": "2026-10-11T00:00:00Z"
                });
                reply.item = Some(serde_json::json!([
                    diff("/tmp/worked-first", 17, 4), diff("/tmp/not-worked", 999, 888)
                ]));
            } else { reply.ok = Some(serde_json::Value::Null); }
            if replies.send(serde_json::to_string(&reply).unwrap()).await.is_err() { break; }
        }
    });
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
                make_project("first", "First project", "/tmp/worked-first"),
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
    let (_, window) = cx.add_window_view(|_, cx| {
        let observe = cx.observe(&sidebar, |_, _, cx| cx.notify());
        Probe {
            sidebar: sidebar.clone(),
            _observe: observe,
        }
    });
    for _ in 0..20 {
        window.update(|window, cx| window.draw(cx).clear());
        window.run_until_parked();
        std::thread::sleep(std::time::Duration::from_millis(5));
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
    assert!(window.debug_bounds("worked-project-additions-0-17").is_some());
    assert!(window.debug_bounds("worked-project-deletions-0-4").is_some());
    assert!(window.debug_bounds("worked-project-additions-0-999").is_none());
    server.abort();
}
