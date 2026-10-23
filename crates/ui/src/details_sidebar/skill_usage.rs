//! Scoped Project skill totals, independent of the selected Chat.
use zeron_proto::skill_usage::*;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct UsageKey {
    pub project: String,
    pub host: String,
    pub catalog: Vec<SkillUsageCatalogEntry>,
}
#[derive(Clone)]
pub(super) struct UsageModel {
    pub key: Option<UsageKey>,
    pub generation: u64,
    pub snapshot: Option<ProjectSkillUsageSnapshot>,
    pub coverage: SkillUsageCoverage,
}
impl Default for UsageModel {
    fn default() -> Self {
        Self {
            key: None,
            generation: 0,
            snapshot: None,
            coverage: SkillUsageCoverage::Unavailable,
        }
    }
}
impl UsageModel {
    pub fn select(&mut self, mut key: Option<UsageKey>) -> bool {
        if self.key == key {
            return false;
        }
        if let Some(key) = &mut key {
            key.catalog = normalize_skill_catalog(std::mem::take(&mut key.catalog));
        }
        if self.key == key {
            return false;
        }
        self.key = key;
        self.generation = self.generation.wrapping_add(1);
        self.snapshot = None;
        self.coverage = if self.key.is_some() {
            SkillUsageCoverage::Loading
        } else {
            SkillUsageCoverage::Unavailable
        };
        true
    }
    pub fn receive(
        &mut self,
        generation: u64,
        snapshot: Option<ProjectSkillUsageSnapshot>,
    ) -> bool {
        let Some(key) = self.key.as_ref().filter(|_| generation == self.generation) else {
            return false;
        };
        if snapshot
            .as_ref()
            .is_some_and(|s| s.project_id != key.project || s.host_device_id != key.host)
        {
            return false;
        }
        let coverage = snapshot
            .as_ref()
            .map_or(SkillUsageCoverage::Unavailable, |s| s.coverage);
        let changed = self.snapshot != snapshot || self.coverage != coverage;
        self.coverage = coverage;
        self.snapshot = snapshot;
        changed
    }
    pub fn presentation(&self, path: &str) -> (String, String) {
        match self.coverage {
            SkillUsageCoverage::Complete => {
                if let Some(count) = self
                    .snapshot
                    .as_ref()
                    .and_then(|s| s.counts.iter().find(|c| c.relative_path == path))
                {
                    return (
                        format!("x{}", count.count),
                        format!(
                            "{} recorded invocations in this project. Includes failed attempts; only identifiable skill activity is counted.",
                            count.count
                        ),
                    );
                }
                (
                    "—".into(),
                    "Project skill history is unavailable on the host device.".into(),
                )
            }
            SkillUsageCoverage::Loading => ("…".into(), "Loading project skill history…".into()),
            SkillUsageCoverage::Partial => (
                "—".into(),
                "Project skill history is incomplete. A total cannot be confirmed.".into(),
            ),
            SkillUsageCoverage::Unavailable => (
                "—".into(),
                "Project skill history is unavailable on the host device.".into(),
            ),
        }
    }
}

use super::*;
impl DetailsSidebar {
    pub(super) fn hide_project_skill_usage(&mut self) {
        self.project_skill_usage_watch = None;
        self.project_skill_usage_connection = None;
        self.project_skill_usage_available = None;
        self.project_skill_usage.select(None);
    }
    pub(super) fn ensure_project_skill_usage(&mut self, cx: &mut Context<Self>) {
        let state = self.app_state.read(cx);
        let context = self.sidebar.context();
        let space = context
            .and_then(|context| context.chat_id.as_ref())
            .and_then(|id| state.chats.iter().find(|chat| &chat.id == id))
            .and_then(|chat| chat.space_id.as_ref())
            .and_then(|id| state.spaces.iter().find(|space| &space.id == id));
        let Some(space) = space else {
            self.hide_project_skill_usage();
            return;
        };
        let project = space.id.clone();
        let host = space.device_id.clone();
        let engine = state.engine().cloned();
        let available = engine.is_some()
            && state.device_online(&host, chrono::Utc::now())
            && (state.local_device_id.as_deref() == Some(host.as_str())
                || !matches!(
                    state.connectivity.state,
                    zeron_proto::ConnectivityState::Offline
                        | zeron_proto::ConnectivityState::Reconnecting
                ));
        let catalog = self
            .context_sources
            .as_ref()
            .filter(|sources| {
                context.is_some_and(|c| sources.key.ends_with(&format!(":{}", c.key)))
            })
            .map(|s| s.catalog.clone());
        let Some(catalog) = catalog else {
            if self
                .project_skill_usage
                .key
                .as_ref()
                .is_none_or(|key| key.project != project || key.host != host)
            {
                self.hide_project_skill_usage();
            }
            return;
        };
        let key = UsageKey {
            project,
            host,
            catalog,
        };
        let connection_changed = match (&self.project_skill_usage_connection, &engine) {
            (Some(old), Some(new)) => !old.same_connection(new),
            (None, None) => false,
            _ => true,
        };
        let availability_changed = self
            .project_skill_usage_available
            .is_some_and(|old| old != available);
        if connection_changed || availability_changed {
            self.hide_project_skill_usage();
        }
        self.project_skill_usage_available = Some(available);
        let scope_changed = self.project_skill_usage.select(Some(key.clone()));
        if !scope_changed && !connection_changed && !availability_changed {
            return;
        }
        self.project_skill_usage_watch = None;
        let generation = self.project_skill_usage.generation;
        self.project_skill_usage_connection = engine.clone();
        if !available {
            self.project_skill_usage.receive(generation, None);
            return;
        }
        let Some(engine) = engine else {
            self.project_skill_usage.receive(generation, None);
            return;
        };
        self.project_skill_usage_connection = Some(engine.clone());
        self.project_skill_usage_watch = Some(cx.spawn(async move |this, cx| {
            let params = WatchProjectSkillUsageParams {
                project_id: key.project,
                catalog: key.catalog,
                target_device_id: Some(key.host),
            };
            let params = serde_json::to_value(params).unwrap();
            let mut retry_seconds = 1;
            loop {
                let result = engine
                    .client()
                    .subscribe_checked(methods::WATCH_PROJECT_SKILL_USAGE, params.clone())
                    .await;
                let unsupported = matches!(
                    &result,
                    Err(zeron_rpc::RpcError::UnknownMethod(_)
                        | zeron_rpc::RpcError::BadParams(_)
                        | zeron_rpc::RpcError::Capability(_))
                );
                if let Ok(mut rx) = result {
                    while let Some(value) = rx.recv().await {
                        let snapshot = serde_json::from_value(value).ok();
                        if this
                            .update(cx, |this, cx| {
                                if this.project_skill_usage.receive(generation, snapshot) {
                                    cx.notify();
                                }
                            })
                            .is_err()
                        {
                            return;
                        }
                    }
                }
                let current = this
                    .update(cx, |this, cx| {
                        if this.project_skill_usage.generation != generation {
                            return false;
                        }
                        if this.project_skill_usage.receive(generation, None) {
                            cx.notify();
                        }
                        true
                    })
                    .unwrap_or(false);
                if !current || unsupported {
                    return;
                }
                // A forwarded stream can end during a short host outage while
                // presence remains online. Recovery belongs to this scoped task,
                // never to redraw; dropping it cancels both timer and RPC.
                cx.background_executor()
                    .timer(std::time::Duration::from_secs(retry_seconds))
                    .await;
                retry_seconds = (retry_seconds * 2).min(30);
            }
        }));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn key(project: &str) -> UsageKey {
        UsageKey {
            project: project.into(),
            host: "host".into(),
            catalog: vec![SkillUsageCatalogEntry {
                name: "a".into(),
                relative_path: Some("a/SKILL.md".into()),
            }],
        }
    }
    fn snapshot(
        project: &str,
        count: u64,
        coverage: SkillUsageCoverage,
    ) -> ProjectSkillUsageSnapshot {
        ProjectSkillUsageSnapshot {
            project_id: project.into(),
            host_device_id: "host".into(),
            counts: vec![ProjectSkillUsageCount {
                relative_path: "a/SKILL.md".into(),
                count,
            }],
            coverage,
            chats_total: 1,
            chats_counted: 1,
        }
    }
    #[test]
    fn project_skill_usage_coverage_labels_and_failure_clears_confirmed_total() {
        let mut model = UsageModel::default();
        model.select(Some(key("a")));
        assert_eq!(model.presentation("a/SKILL.md").0, "…");
        for count in [0, 12, 123456] {
            assert!(model.receive(
                model.generation,
                Some(snapshot("a", count, SkillUsageCoverage::Complete))
            ));
            assert_eq!(model.presentation("a/SKILL.md").0, format!("x{count}"));
            assert!(
                model
                    .presentation("a/SKILL.md")
                    .1
                    .contains("recorded invocations")
            );
        }
        for coverage in [SkillUsageCoverage::Partial, SkillUsageCoverage::Unavailable] {
            model.receive(model.generation, Some(snapshot("a", 12, coverage)));
            assert_eq!(model.presentation("a/SKILL.md").0, "—");
        }
        model.receive(
            model.generation,
            Some(snapshot("a", 12, SkillUsageCoverage::Complete)),
        );
        model.receive(model.generation, None);
        assert_eq!(model.presentation("a/SKILL.md").0, "—");
    }
    #[test]
    fn project_skill_usage_scope_generation_and_equivalent_catalog() {
        let mut model = UsageModel::default();
        assert!(model.select(Some(key("a"))));
        let old = model.generation;
        assert!(!model.select(Some(key("a"))));
        assert_eq!(model.generation, old);
        assert!(model.select(Some(key("b"))));
        assert!(!model.receive(old, Some(snapshot("a", 12, SkillUsageCoverage::Complete))));
        assert_eq!(model.presentation("a/SKILL.md").0, "…");
        assert!(!model.receive(
            model.generation,
            Some(snapshot("a", 12, SkillUsageCoverage::Complete))
        ));
        let mut changed = key("b");
        changed.host = "other".into();
        assert!(model.select(Some(changed.clone())));
        changed.catalog.push(SkillUsageCatalogEntry {
            name: "external".into(),
            relative_path: None,
        });
        assert!(model.select(Some(changed)));
        assert!(model.select(None));
        assert!(model.key.is_none());
        assert!(!model.receive(old, Some(snapshot("a", 12, SkillUsageCoverage::Complete))));
    }
}

#[cfg(test)]
mod lifecycle_tests {
    use super::*;
    use gpui::{AppContext, Modifiers, Render, Window};
    use serde_json::json;
    #[gpui::test]
    fn project_skill_usage_review_f1_rpc_lifecycle_and_compact_rows(cx: &mut gpui::TestAppContext) {
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
        let state = cx.new(|_| AppState::new());
        state.update(cx,|state,_| {
            state.set_test_engine(crate::state::EngineHandle::from_test_client(zeron_rpc::RpcClient::new(out,inbound)));state.local_device_id=Some("local".into());
            state.spaces=vec![serde_json::from_value(json!({"id":"p","deviceId":"project-host","path":"/main","createdAt":"2026-01-01T00:00:00Z"})).unwrap()];
            state.chats=vec![serde_json::from_value(json!({"id":"chat","spaceId":"p","deviceId":"chat-host","cwd":"/tree","archived":false,"createdAt":"2026-01-01T00:00:00Z"})).unwrap()];
            state.connectivity.state=zeron_proto::ConnectivityState::Connected;
            state.devices=vec![serde_json::from_value(json!({"id":"project-host","name":"host","platform":"linux","lastSeenAt":chrono::Utc::now()})).unwrap()];
        });
        let workers = cx.new(|cx| WorkersModel::detached(state.clone(), cx));
        let composer = cx.new(|cx| Composer::new(state.clone(), cx));
        let pickers = composer.read_with(cx, |c, _| c.pickers().clone());
        let sidebar = cx.new(|cx| {
            DetailsSidebar::new(
                state.clone(),
                workers,
                DetailsSidebarPreferences::default(),
                pickers,
                composer,
                cx,
            )
        });
        let catalog: Vec<_> = (0..8)
            .map(|i| SkillUsageCatalogEntry {
                name: format!("skill-{i}"),
                relative_path: Some(format!(".agents/skills/{i}/SKILL.md")),
            })
            .collect();
        sidebar.update(cx,|sidebar,cx| {
            sidebar.sidebar.set_context(Some(DetailsContext{key:"fixture".into(),cwd:"/tree".into(),branch:None,chat_id:Some("chat".into()),target_device_id:Some("chat-host".into()),mode:crate::details_sidebar::context::DetailsMode::Orchestrator}));
            sidebar.context_sources=Some(super::super::session_card::ContextSources{key:"Mock:fixture".into(),skills:catalog.iter().map(|e|super::super::session_card::ProjectSkill{name:format!("{} with a very long skill name that must truncate before the invocation counter",e.name).into(),relative_path:e.relative_path.clone().unwrap()}).collect(),catalog:catalog.clone()});
            sidebar.context_sources_expanded=true;sidebar.checkout_not_git=true;sidebar.ensure_project_skill_usage(cx);
        });
        struct Probe {
            sidebar: Entity<DetailsSidebar>,
            width: f32,
            _observe: Subscription,
            _events: Subscription,
        }
        impl Render for Probe {
            fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
                let theme = Theme::of(cx).clone();
                let card = self.sidebar.update(cx, |s, cx| {
                    let context = s.sidebar.context().unwrap().clone();
                    s.render_session_card(
                        &context,
                        div().into_any_element(),
                        false,
                        None,
                        &theme,
                        cx,
                    )
                });
                div().w(px(self.width)).h(px(1000.0)).child(card)
            }
        }
        let opened = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let opened_events = opened.clone();
        let (probe, window) = cx.add_window_view(|_, cx| {
            let observe = cx.observe(&sidebar, |_, _, cx| cx.notify());
            let events = cx.subscribe(&sidebar, move |_: &mut Probe, _, event, cx| {
                if let DetailsSidebarEvent::OpenFile { relative_path, .. } = event {
                    opened_events.borrow_mut().push(relative_path.clone());
                    cx.notify();
                }
            });
            Probe {
                sidebar: sidebar.clone(),
                width: 300.0,
                _observe: observe,
                _events: events,
            }
        });
        let mut watches = Vec::new();
        let mut cancellations = Vec::new();
        for _ in 0..12 {
            window.update(|w, cx| w.draw(cx).clear());
            window.run_until_parked();
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
                if request.method.as_deref() == Some("WatchProjectSkillUsage") {
                    assert_eq!(request.params["projectId"], "p");
                    assert_eq!(request.params["targetDeviceId"], "project-host");
                    watches.push(request.id);
                    reply.item = Some(
                        serde_json::to_value(ProjectSkillUsageSnapshot {
                            project_id: "p".into(),
                            host_device_id: "project-host".into(),
                            counts: catalog
                                .iter()
                                .enumerate()
                                .map(|(i, e)| ProjectSkillUsageCount {
                                    relative_path: e.relative_path.clone().unwrap(),
                                    count: match i {
                                        0 => 0,
                                        1 => 12,
                                        _ => 123456,
                                    },
                                })
                                .collect(),
                            coverage: SkillUsageCoverage::Complete,
                            chats_total: 2,
                            chats_counted: 2,
                        })
                        .unwrap(),
                    );
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
        assert_eq!(
            watches.len(),
            1,
            "one scoped Project subscription across renders"
        );
        sidebar.read_with(window, |s, _| {
            assert_eq!(
                s.project_skill_usage
                    .presentation(".agents/skills/0/SKILL.md")
                    .0,
                "x0"
            );
            assert_eq!(
                s.project_skill_usage
                    .presentation(".agents/skills/1/SKILL.md")
                    .0,
                "x12"
            );
        });
        for (width, dark) in [(300.0, true), (500.0, true), (300.0, false), (500.0, false)] {
            window.update(|_, cx| cx.set_global(if dark { Theme::dark() } else { Theme::light() }));
            probe.update(window, |p, cx| {
                p.width = width;
                cx.notify();
            });
            window.update(|w, cx| w.draw(cx).clear());
            window.run_until_parked();
            assert_eq!(
                f32::from(
                    window
                        .debug_bounds("project-skills-viewport")
                        .unwrap()
                        .size
                        .height
                ),
                172.0,
                "seven complete rows plus incumbent bottom padding"
            );
            for i in 0..3 {
                let row = window
                    .debug_bounds(
                        [
                            "project-skill-row-0",
                            "project-skill-row-1",
                            "project-skill-row-2",
                        ][i],
                    )
                    .expect("skill row");
                let label = window
                    .debug_bounds(
                        [
                            "project-skill-name-0",
                            "project-skill-name-1",
                            "project-skill-name-2",
                        ][i],
                    )
                    .expect("name");
                let counter = window
                    .debug_bounds(
                        [
                            "project-skill-count-0",
                            "project-skill-count-1",
                            "project-skill-count-2",
                        ][i],
                    )
                    .expect("counter");
                assert_eq!(f32::from(row.size.height), 24.0);
                assert!(label.right() <= counter.left());
                assert!(counter.right() <= row.right());
            }
        }
        let row = window.debug_bounds("project-skill-row-0").unwrap();
        window.simulate_click(row.center(), Modifiers::default());
        assert_eq!(opened.borrow().as_slice(), [".agents/skills/0/SKILL.md"]);
        sidebar.update(window, |s, cx| {
            s.context_sources_scroll
                .scroll_to_item(7, gpui::ScrollStrategy::Bottom);
            cx.notify();
        });
        for _ in 0..2 {
            window.update(|w, cx| w.draw(cx).clear());
            window.run_until_parked();
        }
        let last = window
            .debug_bounds("project-skill-row-7")
            .expect("eighth skill reachable through bounded scrolling");
        assert!(
            last.bottom()
                <= window
                    .debug_bounds("project-skills-viewport")
                    .unwrap()
                    .bottom()
        );
        window.simulate_click(last.center(), Modifiers::default());
        assert_eq!(opened.borrow().last().unwrap(), ".agents/skills/7/SKILL.md");
        let header = window.debug_bounds("context-sources-header").unwrap();
        window.simulate_click(header.center(), Modifiers::default());
        window.update(|w, cx| w.draw(cx).clear());
        assert!(window.debug_bounds("project-skills-viewport").is_none());
        window.simulate_click(header.center(), Modifiers::default());
        window.update(|w, cx| w.draw(cx).clear());
        assert!(window.debug_bounds("project-skills-viewport").is_some());
        let generation = sidebar.read_with(window, |s, _| s.project_skill_usage.generation);
        state.update(window, |state, _| {
            let mut chat = state.chats[0].clone();
            chat.id = "other-chat".into();
            state.chats.push(chat);
        });
        sidebar.update(window, |s, cx| {
            let mut context = s.sidebar.context().unwrap().clone();
            context.key = "other-context".into();
            context.chat_id = Some("other-chat".into());
            s.sidebar.set_context(Some(context));
            // The new Chat's ListSkills is still pending: retain the same
            // Project subscription, then reuse it for an equivalent catalog.
            s.ensure_project_skill_usage(cx);
            s.context_sources.as_mut().unwrap().key = "Mock:other-context".into();
            s.context_sources.as_mut().unwrap().catalog.reverse();
            s.ensure_project_skill_usage(cx);
        });
        window.run_until_parked();
        runtime.block_on(async {
            tokio::task::yield_now().await;
        });
        assert_eq!(
            sidebar.read_with(window, |s, _| s.project_skill_usage.generation),
            generation,
            "another Chat and equivalent catalog reuse the Project stream"
        );
        while let Ok(request) = requests.try_recv() {
            let request: zeron_rpc::ClientFrame = serde_json::from_str(&request).unwrap();
            assert!(request.method.as_deref() != Some("WatchProjectSkillUsage"));
        }
        // A remote EOF must clear the confirmed total but not retry on every draw.
        replies
            .try_send(
                serde_json::to_string(&zeron_rpc::ServerFrame {
                    id: watches[0],
                    done: true,
                    ..Default::default()
                })
                .unwrap(),
            )
            .unwrap();
        for _ in 0..6 {
            runtime.block_on(async {
                tokio::task::yield_now().await;
            });
            window.run_until_parked();
            sidebar.update(window, |s, cx| s.ensure_project_skill_usage(cx));
        }
        assert_eq!(
            sidebar.read_with(window, |s, _| s.project_skill_usage.coverage),
            SkillUsageCoverage::Unavailable
        );
        while let Ok(request) = requests.try_recv() {
            let request: zeron_rpc::ClientFrame = serde_json::from_str(&request).unwrap();
            assert_ne!(
                request.method.as_deref(),
                Some("WatchProjectSkillUsage"),
                "EOF alone must not loop retries"
            );
        }
        // The actual short outage: presence/global connectivity never went down.
        window
            .executor()
            .advance_clock(std::time::Duration::from_secs(1));
        for _ in 0..12 {
            sidebar.update(window, |s, cx| s.ensure_project_skill_usage(cx));
            window.run_until_parked();
            runtime.block_on(async {
                tokio::task::yield_now().await;
            });
            while let Ok(request) = requests.try_recv() {
                let request: zeron_rpc::ClientFrame = serde_json::from_str(&request).unwrap();
                if request.cancel {
                    cancellations.push(request.id);
                    continue;
                }
                if request.method.as_deref() == Some("WatchProjectSkillUsage") {
                    watches.push(request.id);
                    replies.try_send(serde_json::to_string(&zeron_rpc::ServerFrame{id:request.id,item:Some(json!({"projectId":"p","hostDeviceId":"project-host","counts":[{"relativePath":".agents/skills/0/SKILL.md","count":43}],"coverage":"complete","chatsTotal":2,"chatsCounted":2})),..Default::default()}).unwrap()).unwrap();
                }
            }
        }
        state.read_with(window, |s, _| {
            assert!(s.device_online("project-host", chrono::Utc::now()));
            assert_eq!(
                s.connectivity.state,
                zeron_proto::ConnectivityState::Connected
            );
        });
        assert_eq!(
            watches.len(),
            2,
            "short EOF must recover while presence stays online"
        );
        assert_eq!(
            sidebar.read_with(window, |s, _| s
                .project_skill_usage
                .presentation(".agents/skills/0/SKILL.md")
                .0),
            "x43"
        );
        for phase in 0..3 {
            let initial_offline = phase == 1;
            if initial_offline {
                sidebar.update(window, |s, _| s.hide_project_skill_usage());
            }
            let before = watches.len();
            for available in [false, true] {
                state.update(window, |s,_| {
                    s.devices=vec![serde_json::from_value(json!({"id":"project-host","name":"host","platform":"linux","lastSeenAt":if available || phase == 2 {chrono::Utc::now()} else {chrono::Utc::now()-chrono::Duration::days(1)}})).unwrap()];
                    if phase == 2 {
                        s.connectivity.state = if available { zeron_proto::ConnectivityState::Connected } else { zeron_proto::ConnectivityState::Reconnecting };
                    }
                });
                for _ in 0..12 {
                    sidebar.update(window, |s, cx| s.ensure_project_skill_usage(cx));
                    window.run_until_parked();
                    runtime.block_on(async {
                        tokio::task::yield_now().await;
                    });
                    while let Ok(request) = requests.try_recv() {
                        let request: zeron_rpc::ClientFrame =
                            serde_json::from_str(&request).unwrap();
                        if request.cancel {
                            cancellations.push(request.id);
                            continue;
                        }
                        if request.method.as_deref() == Some("WatchProjectSkillUsage") {
                            assert!(available, "offline host must not create a watch");
                            watches.push(request.id);
                            replies.try_send(serde_json::to_string(&zeron_rpc::ServerFrame { id:request.id,item:Some(json!({"projectId":"p","hostDeviceId":"project-host","counts":[{"relativePath":".agents/skills/0/SKILL.md","count":42}],"coverage":"complete","chatsTotal":2,"chatsCounted":2})),..Default::default() }).unwrap()).unwrap();
                        }
                    }
                }
            }
            assert_eq!(
                watches.len(),
                before + 1,
                "host recovery must rearm exactly once in the same scope"
            );
            assert_eq!(
                sidebar.read_with(window, |s, _| s
                    .project_skill_usage
                    .presentation(".agents/skills/0/SKILL.md")
                    .0),
                "x42"
            );
        }
        // Terminate the healthy watch, then change catalog while recovery sleeps.
        replies
            .try_send(
                serde_json::to_string(&zeron_rpc::ServerFrame {
                    id: *watches.last().unwrap(),
                    done: true,
                    ..Default::default()
                })
                .unwrap(),
            )
            .unwrap();
        for _ in 0..6 {
            runtime.block_on(async {
                tokio::task::yield_now().await;
            });
            window.run_until_parked();
        }
        for (name, error, permanent) in [
            (
                "unsupported",
                "unknown method: WatchProjectSkillUsage",
                true,
            ),
            ("retrying", "temporarily unavailable", false),
        ] {
            let before = watches.len();
            sidebar.update(window, |s, cx| {
                s.context_sources
                    .as_mut()
                    .unwrap()
                    .catalog
                    .push(SkillUsageCatalogEntry {
                        name: name.into(),
                        relative_path: None,
                    });
                s.ensure_project_skill_usage(cx);
            });
            for _ in 0..12 {
                window.run_until_parked();
                runtime.block_on(async {
                    tokio::task::yield_now().await;
                });
                while let Ok(request) = requests.try_recv() {
                    let request: zeron_rpc::ClientFrame = serde_json::from_str(&request).unwrap();
                    if request.cancel {
                        cancellations.push(request.id);
                        continue;
                    }
                    if request.method.as_deref() == Some("WatchProjectSkillUsage") {
                        watches.push(request.id);
                        replies
                            .try_send(
                                serde_json::to_string(&zeron_rpc::ServerFrame {
                                    id: request.id,
                                    err: Some(error.into()),
                                    ..Default::default()
                                })
                                .unwrap(),
                            )
                            .unwrap();
                    }
                }
            }
            assert_eq!(watches.len(), before + 1);
            assert_eq!(
                sidebar.read_with(window, |s, _| s.project_skill_usage.coverage),
                SkillUsageCoverage::Unavailable
            );
            if permanent {
                window
                    .executor()
                    .advance_clock(std::time::Duration::from_secs(60));
                for _ in 0..12 {
                    sidebar.update(window, |s, cx| s.ensure_project_skill_usage(cx));
                    window.run_until_parked();
                    runtime.block_on(async {
                        tokio::task::yield_now().await;
                    });
                }
                while let Ok(request) = requests.try_recv() {
                    let request: zeron_rpc::ClientFrame = serde_json::from_str(&request).unwrap();
                    assert_ne!(
                        request.method.as_deref(),
                        Some("WatchProjectSkillUsage"),
                        "unsupported host and canceled old scope must not retry"
                    );
                }
            }
        }
        sidebar.update(window, |s, cx| s.hide_worked_project_diffs(cx));
        window.run_until_parked();
        runtime.block_on(async {
            tokio::task::yield_now().await;
        });
        while let Ok(request) = requests.try_recv() {
            let request: zeron_rpc::ClientFrame = serde_json::from_str(&request).unwrap();
            if request.cancel {
                cancellations.push(request.id);
            }
        }
        assert!(
            cancellations.contains(&watches[1]),
            "scope/availability changes cancel the previously healthy RPC"
        );
        window
            .executor()
            .advance_clock(std::time::Duration::from_secs(60));
        for _ in 0..6 {
            window.run_until_parked();
            runtime.block_on(async {
                tokio::task::yield_now().await;
            });
        }
        while let Ok(request) = requests.try_recv() {
            let request: zeron_rpc::ClientFrame = serde_json::from_str(&request).unwrap();
            assert_ne!(
                request.method.as_deref(),
                Some("WatchProjectSkillUsage"),
                "hide must cancel delayed recovery"
            );
        }
    }
}
