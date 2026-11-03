use std::{path::Path, process::Command, sync::Arc, time::Duration};

use serde_json::json;
use zeron_doc::{SessionDoc, SessionMessageEntry};
use zeron_engine::{EngineCore, EngineProfile, HarnessRegistry};
use zeron_proto::HarnessId;
use zeron_sync::DocsStore;

fn child(test: &str, root: &Path) {
    let output = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", test, "--nocapture"])
        .env("WORKED_PROJECTS_FIXTURE", root)
        .env("UNPEEL_HOME", root.join("unpeel"))
        .env("HOME", root)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn worked_projects_cold_unselected_chats() {
    let Ok(root) = std::env::var("WORKED_PROJECTS_FIXTURE") else {
        let dir = tempfile::tempdir().unwrap();
        child("worked_projects_cold_unselected_chats", dir.path());
        return;
    };
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        let root = Path::new(&root);
        let home = root.join("unpeel");
        std::fs::create_dir_all(&home).unwrap();
        let a = root.join("comet");
        let a_worktree = root.join("other-worktree");
        let b = root.join("bops");
        let own = root.join("own");
        for path in [&a, &a_worktree, &b, &own] { std::fs::create_dir_all(path).unwrap(); }
        std::fs::write(home.join("app-state.json"), serde_json::to_vec(&json!({
            "comet_project_identity": {"version":1,"repositories":[],"checkouts":[
                {"projectID":"checkout-a","path":a,"spaceID":"space-comet"},
                {"projectID":"checkout-a-worktree","path":a_worktree,"spaceID":"space-comet"},
                {"projectID":"checkout-b","path":b,"spaceID":"space-bops"}
            ],"suppressedProjectIds":[]}
        })).unwrap()).unwrap();
        let profile = EngineProfile::development(&root.join("data"), "test-org", "test-user");
        let registry = Arc::new(HarnessRegistry::new());
        let core = EngineCore::assemble_with_profile(profile.clone(), registry.clone(), HarnessId::Mock, None).unwrap();
        let device = core.device_id.clone();
        core.workspace.create_space("space-comet", &device, a.to_str().unwrap(), Some("comet".into()), false).unwrap();
        core.workspace.create_space("space-bops", &device, b.to_str().unwrap(), Some("bops".into()), false).unwrap();
        for (id, paths) in [
            ("chat-a", vec![a.join("a.rs"), a_worktree.join("b.rs")]),
            ("chat-b", vec![b.join("a.rs")]),
            ("chat-c", vec![a.join("one.rs"), b.join("two.rs"), a_worktree.join("three.rs")]),
        ] {
            core.workspace.create_chat(id, None, Some(&device), None, Some(own.to_string_lossy().into_owned())).unwrap();
            core.workspace.set_chat_branch(id, "feature/shared").unwrap();
            let doc = SessionDoc::init(id).unwrap();
            let parts: Vec<_> = paths.iter().enumerate().map(|(i,path)| json!({"kind":"tool","id":format!("tool-{i}"),"call":{"kind":"readFile","path":path}})).collect();
            let entry: SessionMessageEntry = serde_json::from_value(json!({"id":"entry","role":"assistant","createdAt":1,"deviceId":device,"parts":parts})).unwrap();
            doc.push_message(&entry).unwrap();
            DocsStore::open(profile.store_root()).unwrap().save_snapshot_with_cursor(id, &doc.export_snapshot().unwrap(), 0, 2).unwrap();
        }
        let mut before = core.workspace.read_chats().unwrap();
        zeron_proto::view::sort_chats(&mut before);
        let before: Vec<_> = before.into_iter().map(|chat| chat.id).collect();
        core.shutdown().await;
        drop(core);
        let core = EngineCore::assemble_with_profile(profile, registry, HarnessId::Mock, None).unwrap();
        tokio::time::timeout(Duration::from_secs(10), async {
            loop {
                let a = core.workspace.chat("chat-a").unwrap().unwrap().worked_projects;
                let b = core.workspace.chat("chat-b").unwrap().unwrap().worked_projects;
                let c = core.workspace.chat("chat-c").unwrap().unwrap().worked_projects;
                if let (Some(a), Some(b), Some(c)) = (a, b, c) {
                    if a.project_ids == ["space-comet"] && b.project_ids == ["space-bops"]
                        && c.project_ids == ["space-comet", "space-bops"]
                    {
                        break;
                    }
                }
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        }).await.unwrap_or_else(|_| panic!("cold summaries were not produced: chats={:?}, spaces={:?}, links={:?}", core.workspace.read_chats(), core.workspace.read_spaces(), zeron_workers_unpeel::space_links::linked_checkouts_at(&home.join("app-state.json"))));
        let mut after = core.workspace.read_chats().unwrap();
        zeron_proto::view::sort_chats(&mut after);
        assert_eq!(after.into_iter().map(|chat| chat.id).collect::<Vec<_>>(), before);
        assert_eq!(core.doc_host.sync_resources()["openDocuments"], 0);
        assert_eq!(core.doc_host.sync_resources()["retainedBy"]["connections"], 0);
        core.shutdown().await;
    });
}

#[test]
fn worked_projects_causal_reassociation_and_host_boundary() {
    let Ok(root) = std::env::var("WORKED_PROJECTS_FIXTURE") else {
        let dir = tempfile::tempdir().unwrap();
        child(
            "worked_projects_causal_reassociation_and_host_boundary",
            dir.path(),
        );
        return;
    };
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        let root = Path::new(&root);
        let home = root.join("unpeel");
        std::fs::create_dir_all(&home).unwrap();
        let a = root.join("alpha");
        let b = root.join("beta");
        let c = root.join("gamma");
        let own = root.join("own");
        for path in [&a, &b, &c, &own] {
            std::fs::create_dir_all(path).unwrap();
        }
        let profile = EngineProfile::development(&root.join("data"), "test-org", "test-user");
        let registry = Arc::new(HarnessRegistry::new());
        let core = EngineCore::assemble_with_profile(
            profile.clone(), registry.clone(), HarnessId::Mock, None,
        ).unwrap();
        let device = core.device_id.clone();
        for (id, path) in [("space-a", &a), ("space-b", &b), ("space-c", &c)] {
            core.workspace.create_space(id, &device, path.to_str().unwrap(), None, false).unwrap();
        }
        core.workspace.create_chat("cold", None, Some(&device), None, Some(own.to_string_lossy().into_owned())).unwrap();
        core.workspace.create_chat("partial", None, Some(&device), None, Some(own.to_string_lossy().into_owned())).unwrap();
        core.workspace.create_chat("foreign", None, Some("different-host"), None, None).unwrap();
        let doc = SessionDoc::init("cold").unwrap();
        let entry = |id: &str, path: &Path| -> SessionMessageEntry {
            serde_json::from_value(json!({
                "id": id, "role": "assistant", "createdAt": 1, "deviceId": device,
                "parts": [{"kind":"tool","id":id,"call":{"kind":"readFile","path":path}}]
            })).unwrap()
        };
        doc.push_message(&entry("a", &a.join("one.rs"))).unwrap();
        let older = doc.export_snapshot().unwrap();
        doc.push_message(&entry("b", &b.join("two.rs"))).unwrap();
        let newer = doc.export_snapshot().unwrap();
        let witness = doc.doc().state_vv().encode();
        DocsStore::open(profile.store_root()).unwrap()
            .save_snapshot_with_cursor("cold", &older, 0, 2).unwrap();
        let partial = SessionDoc::init("partial").unwrap();
        partial.push_message(&entry("partial-a", &a.join("one.rs"))).unwrap();
        let store = DocsStore::open(profile.store_root()).unwrap();
        store.save_snapshot_with_cursor("partial", &partial.export_snapshot().unwrap(), 0, 2).unwrap();
        partial.push_message(&entry("partial-b", &b.join("two.rs"))).unwrap();
        let partial_witness = partial.doc().state_vv().encode();
        let missing_dependency = partial.doc().oplog_vv();
        partial.push_message(&entry("partial-c", &b.join("three.rs"))).unwrap();
        let dependent = partial.doc().export(loro::ExportMode::updates(&missing_dependency)).unwrap();
        store.enqueue_chat_update("partial", "dependent", &dependent).unwrap();
        core.shutdown().await;
        core.workspace.set_chat_worked_projects("cold", &zeron_proto::ChatWorkedProjects {
            project_ids: vec!["space-a".into(), "space-b".into()],
            complete: true,
            history_version: Some(witness.clone()),
        }).unwrap();
        core.workspace.set_chat_worked_projects("partial", &zeron_proto::ChatWorkedProjects {
            project_ids: vec!["space-a".into(), "space-b".into()],
            complete: true,
            history_version: Some(partial_witness.clone()),
        }).unwrap();
        let before = core.workspace.chat("cold").unwrap().unwrap();
        core.workspace.flush();
        drop(core);

        let write_catalog = |second: &str| {
            std::fs::write(home.join("app-state.json"), serde_json::to_vec(&json!({
                "comet_project_identity": {"version":1,"repositories":[],"checkouts":[
                    {"projectID":"checkout-a","path":a,"spaceID":"space-a"},
                    {"projectID":"checkout-b","path":b,"spaceID":second}
                ],"suppressedProjectIds":[]}
            })).unwrap()).unwrap();
        };
        write_catalog("space-b");
        let core = EngineCore::assemble_with_profile(
            profile.clone(), registry, HarnessId::Mock, None,
        ).unwrap();
        tokio::time::timeout(Duration::from_secs(10), async {
            loop {
                let current = core.workspace.chat("cold").unwrap().unwrap().worked_projects.unwrap();
                if !current.complete {
                    assert_eq!(current.project_ids, ["space-a", "space-b"]);
                    assert_eq!(current.history_version, Some(witness.clone()));
                    break;
                }
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        }).await.expect("older cold snapshot was not reconciled conservatively");
        tokio::time::timeout(Duration::from_secs(10), async {
            loop {
                let current = core.workspace.chat("partial").unwrap().unwrap().worked_projects.unwrap();
                if !current.complete {
                    assert_eq!(current.project_ids, ["space-a", "space-b"]);
                    assert_eq!(current.history_version, Some(partial_witness.clone()));
                    break;
                }
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        }).await.expect("pending history with a missing dependency must remain incomplete");
        assert!(core.worked_projects.ensure(&["cold".into(), "foreign".into()]).is_err());

        DocsStore::open(profile.store_root()).unwrap()
            .save_snapshot_with_cursor("cold", &newer, 0, 2).unwrap();
        write_catalog("space-c");
        tokio::time::timeout(Duration::from_secs(10), async {
            loop {
                let current = core.workspace.chat("cold").unwrap().unwrap().worked_projects.unwrap();
                if current.complete && current.project_ids == ["space-a", "space-c"] {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        }).await.unwrap_or_else(|_| panic!("causally covering snapshot did not apply catalog reassociation: chat={:?}, spaces={:?}", core.workspace.chat("cold"), core.workspace.read_spaces()));
        std::fs::remove_file(home.join("app-state.json")).unwrap();
        tokio::time::timeout(Duration::from_secs(10), async {
            loop {
                let current = core.workspace.chat("cold").unwrap().unwrap().worked_projects.unwrap();
                if !current.complete {
                    assert_eq!(current.project_ids, ["space-a", "space-c"]);
                    break;
                }
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        }).await.expect("unavailable catalog must mark known results incomplete");
        write_catalog("space-c");
        tokio::time::timeout(Duration::from_secs(10), async {
            while !core.workspace.chat("cold").unwrap().unwrap().worked_projects.unwrap().complete {
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        }).await.expect("catalog recovery");
        DocsStore::open(profile.store_root()).unwrap()
            .save_snapshot_with_cursor("cold", b"corrupt snapshot", 0, 2).unwrap();
        core.worked_projects.ensure(&["cold".into()]).unwrap();
        tokio::time::timeout(Duration::from_secs(10), async {
            loop {
                let current = core.workspace.chat("cold").unwrap().unwrap().worked_projects.unwrap();
                if !current.complete {
                    assert_eq!(current.project_ids, ["space-a", "space-c"]);
                    break;
                }
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        }).await.expect("unreadable history must mark known results incomplete");
        let mut after = core.workspace.chat("cold").unwrap().unwrap();
        after.worked_projects = before.worked_projects.clone();
        assert_eq!(after, before, "summary publication must not change adjacent Chat fields");
        core.shutdown().await;
        assert!(core.worked_projects.ensure(&["cold".into()]).is_err());
    });
}

#[test]
fn worked_projects_salvaged_tool_does_not_erase_known_project() {
    let Ok(root) = std::env::var("WORKED_PROJECTS_FIXTURE") else {
        let dir = tempfile::tempdir().unwrap();
        child(
            "worked_projects_salvaged_tool_does_not_erase_known_project",
            dir.path(),
        );
        return;
    };
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        let root = Path::new(&root);
        let home = root.join("unpeel");
        std::fs::create_dir_all(&home).unwrap();
        let project = root.join("project");
        let own = root.join("own");
        std::fs::create_dir_all(&project).unwrap();
        std::fs::create_dir_all(&own).unwrap();
        std::fs::write(home.join("app-state.json"), serde_json::to_vec(&json!({
            "comet_project_identity":{"version":1,"repositories":[],"checkouts":[
                {"projectID":"checkout","path":project,"spaceID":"project"}
            ],"suppressedProjectIds":[]}
        })).unwrap()).unwrap();
        let profile = EngineProfile::development(&root.join("data"), "test-org", "test-user");
        let registry = Arc::new(HarnessRegistry::new());
        let core = EngineCore::assemble_with_profile(profile.clone(), registry.clone(), HarnessId::Mock, None).unwrap();
        let device = core.device_id.clone();
        core.workspace.create_space("project", &device, project.to_str().unwrap(), None, false).unwrap();
        core.workspace.create_chat("damaged", None, Some(&device), None, Some(own.to_string_lossy().into_owned())).unwrap();
        core.workspace.set_chat_archived("damaged", true).unwrap();
        let doc = SessionDoc::init("damaged").unwrap();
        let entry: SessionMessageEntry = serde_json::from_value(json!({
            "id":"entry","role":"assistant","createdAt":1,"deviceId":device,
            "parts":[{"kind":"tool","id":"tool","call":{"kind":"readFile","path":project.join("file")}}]
        })).unwrap();
        doc.push_message(&entry).unwrap();
        let prior = doc.doc().state_vv().encode();
        core.workspace.set_chat_worked_projects("damaged", &zeron_proto::ChatWorkedProjects {
            project_ids: vec!["project".into()], complete: true, history_version: Some(prior),
        }).unwrap();
        let messages = doc.doc().get_list("messages");
        messages.delete(0, 1).unwrap();
        messages.push(loro::LoroValue::from(json!({
            "id":"entry","role":"assistant","createdAt":1,"deviceId":device,
            "parts":[{"kind":"tool","id":"tool","call":{"kind":"readFile"}}]
        }))).unwrap();
        doc.doc().commit();
        DocsStore::open(profile.store_root()).unwrap()
            .save_snapshot_with_cursor("damaged", &doc.export_snapshot().unwrap(), 0, 2).unwrap();
        core.shutdown().await;
        drop(core);
        let core = EngineCore::assemble_with_profile(profile, registry, HarnessId::Mock, None).unwrap();
        core.worked_projects.ensure(&["damaged".into()]).unwrap();
        tokio::time::timeout(Duration::from_secs(10), async {
            loop {
                let current = core.workspace.chat("damaged").unwrap().unwrap().worked_projects.unwrap();
                if !current.complete || current.project_ids.is_empty() {
                    assert_eq!(current.project_ids, ["project"], "salvaged tool is incomplete, not a confirmed empty history");
                    assert!(!current.complete);
                    break;
                }
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        }).await.expect("damaged history observation");
        core.shutdown().await;
    });
}

#[test]
fn worked_projects_archived_demand_is_scoped() {
    let Ok(root) = std::env::var("WORKED_PROJECTS_FIXTURE") else {
        let dir = tempfile::tempdir().unwrap();
        child("worked_projects_archived_demand_is_scoped", dir.path());
        return;
    };
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        let root = Path::new(&root);
        let home = root.join("unpeel");
        std::fs::create_dir_all(&home).unwrap();
        let project = root.join("project");
        let own = root.join("own");
        std::fs::create_dir_all(&project).unwrap();
        std::fs::create_dir_all(&own).unwrap();
        let profile = EngineProfile::development(&root.join("data"), "test-org", "test-user");
        let registry = Arc::new(HarnessRegistry::new());
        let core = EngineCore::assemble_with_profile(profile.clone(), registry.clone(), HarnessId::Mock, None).unwrap();
        let device = core.device_id.clone();
        core.workspace.create_space("project", &device, project.to_str().unwrap(), None, false).unwrap();
        for id in ["active", "displayed", "hidden"] {
            core.workspace.create_chat(id, None, Some(&device), None, Some(own.to_string_lossy().into_owned())).unwrap();
            if id != "active" {
                core.workspace.set_chat_archived(id, true).unwrap();
            }
            let doc = SessionDoc::init(id).unwrap();
            let entry: SessionMessageEntry = serde_json::from_value(json!({
                "id":"entry","role":"assistant","createdAt":1,"deviceId":device,
                "parts":[{"kind":"tool","id":"tool","call":{"kind":"readFile","path":project.join("file")}}]
            })).unwrap();
            doc.push_message(&entry).unwrap();
            DocsStore::open(profile.store_root()).unwrap()
                .save_snapshot_with_cursor(id, &doc.export_snapshot().unwrap(), 0, 2).unwrap();
        }
        core.shutdown().await;
        drop(core);
        std::fs::write(home.join("app-state.json"), serde_json::to_vec(&json!({
            "comet_project_identity":{"version":1,"repositories":[],"checkouts":[
                {"projectID":"checkout","path":project,"spaceID":"project"}
            ],"suppressedProjectIds":[]}
        })).unwrap()).unwrap();
        let core = EngineCore::assemble_with_profile(profile, registry, HarnessId::Mock, None).unwrap();
        tokio::time::timeout(Duration::from_secs(10), async {
            while core.workspace.chat("active").unwrap().unwrap().worked_projects.is_none() {
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        }).await.expect("active backfill");
        assert!(core.workspace.chat("displayed").unwrap().unwrap().worked_projects.is_none());
        assert!(core.workspace.chat("hidden").unwrap().unwrap().worked_projects.is_none());
        assert!(core.worked_projects.ensure(&["displayed".into(), "missing".into()]).is_err());
        core.worked_projects.ensure(&["displayed".into(), "displayed".into()]).unwrap();
        tokio::time::timeout(Duration::from_secs(10), async {
            loop {
                if core.workspace.chat("displayed").unwrap().unwrap().worked_projects
                    .is_some_and(|summary| summary.project_ids == ["project"]) {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        }).await.expect("displayed archived backfill");
        assert!(core.workspace.chat("hidden").unwrap().unwrap().worked_projects.is_none());
        assert_eq!(core.doc_host.sync_resources()["openDocuments"], 0);
        assert_eq!(core.doc_host.sync_resources()["retainedBy"]["connections"], 0);
        core.shutdown().await;
    });
}
