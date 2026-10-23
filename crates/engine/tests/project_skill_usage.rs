//! Real engine/RPC, isolated registry and cold snapshots; never launches a provider.
use serde_json::{Value, json};
use std::{path::Path, sync::Arc, time::Duration};
use zeron_doc::{SessionDoc, SessionMessageEntry};
use zeron_engine::{EngineCore, HarnessRegistry};
use zeron_proto::{HarnessId, skill_usage::*};
use zeron_rpc::RpcClient;
use zeron_sync::DocsStore;

fn core(dir: &Path) -> EngineCore {
    EngineCore::assemble(dir, Arc::new(HarnessRegistry::new()), HarnessId::Mock, None).unwrap()
}
fn store(dir: &Path) -> DocsStore {
    DocsStore::open(dir.join("orgs/dev-org/dev-user")).unwrap()
}
fn entry(id: &str, n: usize) -> SessionMessageEntry {
    serde_json::from_value(json!({"id":id,"role":"assistant","createdAt":1,"deviceId":"host","parts":(0..n).map(|i| json!({"kind":"tool","id":format!("tool-{i}"),"call":{"kind":"readFile","path":"skill://a"}})).collect::<Vec<_>>()})).unwrap()
}
fn params(project: &str) -> Value {
    json!({"projectId":project,"catalog":[{"name":"a","relativePath":".agents/skills/a/SKILL.md"}]})
}
fn seed(
    core: &EngineCore,
    store: &DocsStore,
    id: &str,
    project: &str,
    n: usize,
    epoch: u32,
) -> SessionDoc {
    core.workspace
        .create_chat(id, Some(project), None, None, Some("/tree".into()))
        .unwrap();
    let doc = SessionDoc::init(id).unwrap();
    doc.push_message(&entry("e", n)).unwrap();
    store
        .save_snapshot_with_cursor(id, &doc.export_snapshot().unwrap(), 0, epoch)
        .unwrap();
    doc
}
async fn frame(rx: &mut zeron_rpc::RpcSubscription) -> ProjectSkillUsageSnapshot {
    serde_json::from_value(
        tokio::time::timeout(Duration::from_secs(10), rx.recv())
            .await
            .expect("frame timeout")
            .expect("stream ended"),
    )
    .unwrap()
}
async fn settled(rx: &mut zeron_rpc::RpcSubscription) -> ProjectSkillUsageSnapshot {
    loop {
        let f = frame(rx).await;
        if f.coverage != SkillUsageCoverage::Loading {
            return f;
        }
    }
}
fn setup(core: &EngineCore) {
    for (id, path) in [("p", "/main"), ("q", "/other")] {
        core.workspace
            .create_space(id, core.workspace.device_id(), path, None, false)
            .unwrap();
    }
}
#[tokio::test]
async fn project_skill_usage_cold_outbox_restart_replay_and_missing_history() {
    let dir = tempfile::tempdir().unwrap();
    for pass in 0..2 {
        let core = core(dir.path());
        setup(&core);
        let store = store(dir.path());
        if pass == 0 {
            let parent = seed(&core, &store, "a", "p", 2, 0);
            let child = SessionDoc::init("a--sub--agent").unwrap();
            child.push_message(&entry("nested-use", 99)).unwrap();
            store
                .save_snapshot("a--sub--agent", &child.export_snapshot().unwrap())
                .unwrap();
            let spawn: SessionMessageEntry = serde_json::from_value(json!({"id":"spawn","role":"assistant","createdAt":1,"deviceId":"host","parts":[{"kind":"tool","id":"agent","call":{"kind":"unknown","name":"Agent","input":{"name":"a"}},"subagentRef":"a--sub--agent"}]})).unwrap();
            parent.push_message(&spawn).unwrap();
            store
                .save_snapshot("a", &parent.export_snapshot().unwrap())
                .unwrap();
            let doc = seed(&core, &store, "b", "p", 2, 2);
            let vv = doc.doc().oplog_vv();
            doc.push_message(&entry("outbox", 1)).unwrap();
            store
                .enqueue_chat_update(
                    "b",
                    "batch",
                    &doc.doc().export(loro::ExportMode::updates(&vv)).unwrap(),
                )
                .unwrap();
            core.workspace.set_chat_archived("b", true).unwrap();
            seed(&core, &store, "outside", "q", 8, 0);
        }
        let before = core.doc_host.sync_resources();
        let client = zeron_rpc::memory_client(core.rpc_service());
        let mut rx = client
            .subscribe_scoped("WatchProjectSkillUsage", params("p"))
            .await
            .unwrap();
        let f = settled(&mut rx).await;
        assert_eq!(f.coverage, SkillUsageCoverage::Complete);
        assert_eq!(f.counts[0].count, 5);
        assert_eq!(f.chats_counted, 2);
        let after = core.doc_host.sync_resources();
        for key in ["openDocuments", "documentLoads"] {
            assert_eq!(before[key], after[key], "cold scan {key}");
        }
        drop(rx);
        if pass == 1 {
            core.workspace
                .create_chat("missing", Some("p"), None, None, Some("/tree".into()))
                .unwrap();
            let mut rx = client
                .subscribe_scoped("WatchProjectSkillUsage", params("p"))
                .await
                .unwrap();
            assert_eq!(settled(&mut rx).await.coverage, SkillUsageCoverage::Partial);
        }
        core.shutdown().await;
    }
}
#[tokio::test]
async fn project_skill_usage_updates_nonselected_chat_and_purge() {
    let dir = tempfile::tempdir().unwrap();
    let core = core(dir.path());
    setup(&core);
    let store = store(dir.path());
    seed(&core, &store, "a", "p", 2, 0);
    let doc = seed(&core, &store, "b", "p", 3, 0);
    let client = zeron_rpc::memory_client(core.rpc_service());
    let mut rx = client
        .subscribe_scoped("WatchProjectSkillUsage", params("p"))
        .await
        .unwrap();
    assert_eq!(settled(&mut rx).await.counts[0].count, 5);
    seed(&core, &store, "unrelated", "q", 99, 0);
    core.workspace
        .note_message("unrelated", "completed elsewhere");
    assert!(
        tokio::time::timeout(Duration::from_millis(100), rx.recv())
            .await
            .is_err(),
        "another Project must not publish a changed total"
    );
    doc.push_message(&entry("new", 1)).unwrap();
    store
        .save_snapshot("b", &doc.export_snapshot().unwrap())
        .unwrap();
    core.workspace.note_message("b", "turn ended");
    assert_eq!(settled(&mut rx).await.counts[0].count, 6);
    core.workspace.delete_chat("b").unwrap();
    assert_eq!(settled(&mut rx).await.counts[0].count, 2);
    drop(rx);
    core.shutdown().await;
}
#[tokio::test]
async fn project_skill_usage_rejects_wrong_host_and_marks_foreign_chat_partial() {
    let dir = tempfile::tempdir().unwrap();
    let core = core(dir.path());
    setup(&core);
    let store = store(dir.path());
    seed(&core, &store, "foreign", "p", 2, 0);
    core.workspace.set_chat_host("foreign", "other").unwrap();
    core.workspace
        .create_space("remote", "other", "/remote", None, false)
        .unwrap();
    let client: RpcClient = zeron_rpc::memory_client(core.rpc_service());
    assert!(
        client
            .subscribe_checked("WatchProjectSkillUsage", params("remote"))
            .await
            .is_err()
    );
    let mut rx = client
        .subscribe_scoped("WatchProjectSkillUsage", params("p"))
        .await
        .unwrap();
    assert_eq!(settled(&mut rx).await.coverage, SkillUsageCoverage::Partial);
    drop(rx);
    core.shutdown().await;
}

#[tokio::test]
async fn project_skill_usage_malformed_and_causal_outbox_are_partial() {
    let dir = tempfile::tempdir().unwrap();
    let core = core(dir.path());
    setup(&core);
    let store = store(dir.path());
    let doc = seed(&core, &store, "broken", "p", 0, 0);
    doc.doc()
        .get_list("messages")
        .push(loro::LoroValue::String("not an entry".into()))
        .unwrap();
    doc.doc().commit();
    store
        .save_snapshot("broken", &doc.export_snapshot().unwrap())
        .unwrap();
    let client = zeron_rpc::memory_client(core.rpc_service());
    let mut rx = client
        .subscribe_scoped("WatchProjectSkillUsage", params("p"))
        .await
        .unwrap();
    assert_eq!(settled(&mut rx).await.coverage, SkillUsageCoverage::Partial);
    drop(rx);
    core.workspace.delete_chat("broken").unwrap();
    let doc = seed(&core, &store, "causal", "p", 0, 2);
    doc.push_message(&entry("missing-dependency", 1)).unwrap();
    let vv = doc.doc().oplog_vv();
    doc.push_message(&entry("dependent", 1)).unwrap();
    store
        .enqueue_chat_update(
            "causal",
            "dependent",
            &doc.doc().export(loro::ExportMode::updates(&vv)).unwrap(),
        )
        .unwrap();
    let mut rx = client
        .subscribe_scoped("WatchProjectSkillUsage", params("p"))
        .await
        .unwrap();
    assert_eq!(settled(&mut rx).await.coverage, SkillUsageCoverage::Partial);
    drop(rx);
    store
        .save_snapshot("causal", b"unreadable snapshot")
        .unwrap();
    let mut unreadable = client
        .subscribe_scoped("WatchProjectSkillUsage", params("p"))
        .await
        .unwrap();
    assert_eq!(
        settled(&mut unreadable).await.coverage,
        SkillUsageCoverage::Partial
    );
    drop(unreadable);
    core.shutdown().await;
}

#[tokio::test]
async fn project_skill_usage_tools_only_turn_refreshes_without_text() {
    use zeron_proto::{AgentEvent, DoneStatus, RunRequest, ToolCall};
    let dir = tempfile::tempdir().unwrap();
    let registry = HarnessRegistry::new();
    registry.register(Arc::new(zeron_harness::mock::MockHarness {
        script: vec![
            AgentEvent::ToolCall {
                id: "fresh".into(),
                call: ToolCall::ReadFile {
                    path: "skill://a".into(),
                },
            },
            AgentEvent::Done {
                status: DoneStatus::Completed,
                result: None,
                error: None,
                session_id: None,
            },
        ],
    }));
    let core = EngineCore::assemble(dir.path(), Arc::new(registry), HarnessId::Mock, None).unwrap();
    setup(&core);
    let store = store(dir.path());
    seed(&core, &store, "a", "p", 2, 0);
    seed(&core, &store, "b", "p", 3, 0);
    let client = zeron_rpc::memory_client(core.rpc_service());
    let mut rx = client
        .subscribe_scoped("WatchProjectSkillUsage", params("p"))
        .await
        .unwrap();
    assert_eq!(settled(&mut rx).await.counts[0].count, 5);
    let request: RunRequest=serde_json::from_value(json!({"prompt":"","cwd":dir.path(),"harness":"mock","sandbox":"workspace-write","autoApprove":false,"attachments":[],"modelOptions":{}})).unwrap();
    core.sessions
        .dispatch("b", HarnessId::Mock, request, None)
        .await
        .unwrap();
    loop {
        let f = settled(&mut rx).await;
        if f.counts[0].count == 6 {
            break;
        }
    }
    drop(rx);
    core.shutdown().await;
}

#[test]
fn project_skill_usage_update_during_backfill_is_not_lost() {
    use futures::{StreamExt, poll};
    use zeron_rpc::{RpcReply, RpcService};
    let runtime = tokio::runtime::Builder::new_current_thread()
        .max_blocking_threads(1)
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(async {
        let dir = tempfile::tempdir().unwrap();
        let core = core(dir.path());
        setup(&core);
        let store = store(dir.path());
        seed(&core, &store, "a", "p", 2, 0);
        seed(&core, &store, "b", "p", 3, 0);
        let live = core.doc_host.open_local("b").unwrap();
        let RpcReply::Stream(mut stream) = core
            .rpc_service()
            .handle("WatchProjectSkillUsage", params("p"))
            .await
            .unwrap()
        else {
            panic!("stream")
        };
        let loading: ProjectSkillUsageSnapshot =
            serde_json::from_value(stream.next().await.unwrap()).unwrap();
        assert_eq!(loading.coverage, SkillUsageCoverage::Loading);
        // Occupy the sole blocking worker so the initial backfill is in flight
        // but cannot complete until after the update and its registry wake.
        let (started, ready) = tokio::sync::oneshot::channel();
        let (release, wait) = std::sync::mpsc::channel();
        let blocker = tokio::task::spawn_blocking(move || {
            started.send(()).unwrap();
            wait.recv().unwrap();
        });
        ready.await.unwrap();
        let mut pending = Box::pin(stream.next());
        assert!(poll!(pending.as_mut()).is_pending());
        live.doc()
            .push_message(&entry("during-backfill", 1))
            .unwrap();
        core.workspace
            .note_message("b", "turn complete during scan");
        release.send(()).unwrap();
        blocker.await.unwrap();
        let frame: ProjectSkillUsageSnapshot = serde_json::from_value(
            tokio::time::timeout(Duration::from_secs(10), pending)
                .await
                .unwrap()
                .unwrap(),
        )
        .unwrap();
        assert_eq!(frame.coverage, SkillUsageCoverage::Complete);
        assert_eq!(frame.counts[0].count, 6);
        drop(stream);
        core.shutdown().await;
    });
}
