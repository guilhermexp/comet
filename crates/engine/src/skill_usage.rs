//! Reconstructible, host-local projection over retained Project Chats.
use std::collections::BTreeMap;
use zeron_doc::{MessagePart, MessageRole, SessionMessageEntry};
use zeron_proto::{Chat, Space, skill_usage::*};

#[derive(Debug, Default)]
pub(crate) struct ChatSkillUsage {
    pub counts: BTreeMap<String, u64>,
    pub complete: bool,
}

pub(crate) fn chat_skill_usage(
    chat: &Chat,
    space: &Space,
    entries: &[SessionMessageEntry],
    catalog: &[SkillUsageCatalogEntry],
) -> ChatSkillUsage {
    let mut result = ChatSkillUsage {
        complete: true,
        ..Default::default()
    };
    if chat.space_id.as_deref() != Some(space.id.as_str()) {
        return result;
    }
    let boundary = format!("fork:{}", chat.id);
    let parts: Vec<_> = entries
        .iter()
        .flat_map(|entry| entry.parts.iter().map(move |part| (entry, part)))
        .collect();
    let own = parts
        .iter()
        .position(|(_, part)| matches!(part, MessagePart::Fork { id, .. } if id == &boundary));
    if own.is_none()
        && parts
            .iter()
            .any(|(_, part)| matches!(part, MessagePart::Fork { .. }))
    {
        result.complete = false;
        return result;
    }
    let mut calls = BTreeMap::new();
    for (entry, part) in parts.into_iter().skip(own.map_or(0, |index| index + 1)) {
        if entry.role != MessageRole::Assistant {
            continue;
        }
        if let MessagePart::Tool { id, call, .. } = part {
            let mut root = entry;
            let mut seen = std::collections::HashSet::new();
            while let Some(parent) = root.continuation_of.as_deref() {
                if !seen.insert(root.id.as_str()) {
                    result.complete = false;
                    break;
                }
                match entries.iter().find(|e| e.id == parent) {
                    Some(parent_entry) => root = parent_entry,
                    None => break,
                }
            }
            let root_id = root.continuation_of.as_deref().unwrap_or(&root.id);
            let identity = skill_invocation_reference(call).and_then(|reference| {
                match_skill_reference(
                    reference,
                    catalog,
                    chat.cwd.as_deref().unwrap_or(&space.path),
                    &space.path,
                )
            });
            calls.insert((root_id, id), identity);
        }
    }
    for path in calls.into_values().flatten() {
        *result.counts.entry(path).or_default() += 1;
    }
    result
}

use crate::{doc_host::DocHost, sessions::SessionsEngine, workspace_host::WorkspaceHost};
use futures::{StreamExt, stream::BoxStream};
use tokio::sync::watch;
use zeron_proto::{Session, SessionStatus};

struct UsageWatch {
    host: DocHost,
    workspace: WorkspaceHost,
    registry_complete: bool,
    space: Space,
    catalog: Vec<SkillUsageCatalogEntry>,
    chats: watch::Receiver<Vec<Chat>>,
    sessions: watch::Receiver<Vec<Session>>,
    spaces: watch::Receiver<Vec<Space>>,
    prior_sessions: Vec<Session>,
    history: tokio::sync::broadcast::Receiver<String>,
    history_dirty: std::collections::HashSet<String>,
    contributions: BTreeMap<String, (Chat, ChatSkillUsage)>,
    first: bool,
    last: Option<ProjectSkillUsageSnapshot>,
}

impl UsageWatch {
    fn drain_history(&mut self) {
        loop {
            match self.history.try_recv() {
                Ok(id) => {
                    self.history_dirty.insert(id);
                }
                Err(tokio::sync::broadcast::error::TryRecvError::Lagged(_)) => {
                    self.history_dirty
                        .extend(self.contributions.keys().cloned());
                }
                Err(_) => break,
            }
        }
    }

    fn history_ready(&self) -> bool {
        self.history_dirty.iter().any(|id| {
            !self
                .prior_sessions
                .iter()
                .any(|s| &s.chat_id == id && s.status == SessionStatus::Working)
        })
    }

    fn snapshot(&self, coverage: SkillUsageCoverage) -> ProjectSkillUsageSnapshot {
        let mut totals: BTreeMap<String, u64> = self
            .catalog
            .iter()
            .filter_map(|entry| entry.relative_path.clone().map(|path| (path, 0)))
            .collect();
        let mut counted = 0;
        for (_, contribution) in self.contributions.values() {
            counted += usize::from(contribution.complete);
            for (path, count) in &contribution.counts {
                *totals.entry(path.clone()).or_default() += count;
            }
        }
        ProjectSkillUsageSnapshot {
            project_id: self.space.id.clone(),
            host_device_id: self.space.device_id.clone(),
            counts: totals
                .into_iter()
                .map(|(relative_path, count)| ProjectSkillUsageCount {
                    relative_path,
                    count,
                })
                .collect(),
            coverage,
            chats_total: self.contributions.len(),
            chats_counted: counted,
        }
    }
    async fn reconcile(&mut self) {
        self.chats.borrow_and_update();
        self.spaces.borrow_and_update();
        let sessions = self.sessions.borrow_and_update().clone();
        // Registry watch publication is asynchronous. Read the current registry
        // after attaching receivers rather than treating an initial empty watch
        // value as proof that the Project or its Chats do not exist.
        let workspace = self.workspace.clone();
        let project = self.space.id.clone();
        let registry = tokio::task::spawn_blocking(move || {
            (workspace.read_chats(), workspace.space(&project))
        })
        .await;
        let Ok((Ok(current), Ok(new_space))) = registry else {
            self.registry_complete = false;
            return;
        };
        self.registry_complete = true;
        let root_changed = new_space.as_ref().is_none_or(|space| {
            space.path != self.space.path || space.device_id != self.space.device_id
        });
        if let Some(space) = new_space {
            self.space = space;
        } else {
            self.space.device_id.clear();
            self.contributions.clear();
            return;
        }
        let dirty: std::collections::HashSet<_> = sessions
            .iter()
            .filter(|session| {
                let previous = self
                    .prior_sessions
                    .iter()
                    .find(|old| old.chat_id == session.chat_id);
                previous.is_none_or(|old| {
                    old.status != session.status
                        || old.started_at != session.started_at
                        || old.last_completed_turn != session.last_completed_turn
                }) || session.status != SessionStatus::Working && previous != Some(*session)
            })
            .map(|session| session.chat_id.clone())
            .chain(
                self.prior_sessions
                    .iter()
                    .filter(|old| !sessions.iter().any(|s| s.chat_id == old.chat_id))
                    .map(|s| s.chat_id.clone()),
            )
            .collect();
        self.drain_history();
        // Local streaming snapshots can be persisted during a turn. Coalesce
        // those revisions until its existing Session completion notification.
        let working: std::collections::HashSet<_> = sessions
            .iter()
            .filter(|s| s.status == SessionStatus::Working)
            .map(|s| s.chat_id.clone())
            .collect();
        let mut dirty = dirty;
        self.history_dirty.retain(|id| {
            if working.contains(id) {
                true
            } else {
                dirty.insert(id.clone());
                false
            }
        });
        self.prior_sessions = sessions;
        let eligible: Vec<_> = current
            .into_iter()
            .filter(|chat| chat.space_id.as_deref() == Some(self.space.id.as_str()))
            .collect();
        self.contributions
            .retain(|id, _| eligible.iter().any(|chat| &chat.id == id));
        self.history_dirty
            .retain(|id| eligible.iter().any(|chat| &chat.id == id));
        for chat in eligible {
            if !root_changed
                && !dirty.contains(&chat.id)
                && self
                    .contributions
                    .get(&chat.id)
                    .is_some_and(|(old, _)| old == &chat)
            {
                continue;
            }
            let host = self.host.clone();
            let space = self.space.clone();
            let catalog = self.catalog.clone();
            let for_read = chat.clone();
            let contribution = if chat.device_id != self.space.device_id
                || self.space.device_id != self.host.device_id()
            {
                ChatSkillUsage::default()
            } else {
                tokio::task::spawn_blocking(move || {
                    let Ok(Some((doc, history_complete))) =
                        host.read_local_doc_with_coverage(&for_read.id)
                    else {
                        return ChatSkillUsage::default();
                    };
                    let before = doc.doc().state_vv();
                    let Ok(entries) = doc.read_entries() else {
                        return ChatSkillUsage::default();
                    };
                    let mut usage = chat_skill_usage(&for_read, &space, &entries, &catalog);
                    usage.complete &= history_complete
                        && doc.chat_id().as_deref() == Some(for_read.id.as_str())
                        && entries.len() == doc.doc().get_list("messages").len()
                        && before == doc.doc().state_vv();
                    usage
                })
                .await
                .unwrap_or_default()
            };
            self.contributions
                .insert(chat.id.clone(), (chat, contribution));
        }
    }
}

/// Receivers are attached before backfill. Dropping the stream drops all watches;
/// at most the single already-started blocking read can finish after cancellation.
pub(crate) fn watch_project_skill_usage(
    host: DocHost,
    workspace: WorkspaceHost,
    sessions: &SessionsEngine,
    space: Space,
    catalog: Vec<SkillUsageCatalogEntry>,
) -> BoxStream<'static, serde_json::Value> {
    let chats = workspace.watch_chats();
    let sessions = sessions.watch_sessions();
    let spaces = workspace.watch_spaces();
    let history = host.watch_history_changes();
    let state = UsageWatch {
        host,
        workspace,
        registry_complete: true,
        space,
        catalog: normalize_skill_catalog(catalog),
        chats,
        sessions,
        spaces,
        prior_sessions: Vec::new(),
        history,
        history_dirty: Default::default(),
        contributions: BTreeMap::new(),
        first: true,
        last: None,
    };
    futures::stream::unfold(state, |mut state| async move {
        if state.first {
            state.first = false;
            return Some((
                serde_json::to_value(state.snapshot(SkillUsageCoverage::Loading)).ok()?,
                state,
            ));
        }
        loop {
            state.drain_history();
            let history_ready = state.history_ready();
            if state.last.is_some()
                && !history_ready
                && !state.chats.has_changed().unwrap_or(false)
                && !state.sessions.has_changed().unwrap_or(false)
                && !state.spaces.has_changed().unwrap_or(false)
            {
                tokio::select! {
                    event = state.history.recv() => {
                        match event {
                            Ok(id) => {
                                let working = state.prior_sessions.iter().any(|s| s.chat_id == id && s.status == SessionStatus::Working);
                                state.history_dirty.insert(id);
                                if working { continue; }
                            }
                            Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => { state.history_dirty.extend(state.contributions.keys().cloned()); }
                            Err(_) => return None,
                        }
                    },
                    changed = state.chats.changed() => { changed.ok()?; },
                    changed = state.sessions.changed() => { changed.ok()?; },
                    changed = state.spaces.changed() => { changed.ok()?; },
                }
            }
            state.reconcile().await;
            state.drain_history();
            let history_ready = state.history_ready();
            // Reconcile queued updates before publishing: an older scan never
            // overwrites a newer contribution, including tools-only completions.
            if history_ready || state.chats.has_changed().unwrap_or(false)
                || state.sessions.has_changed().unwrap_or(false)
                || state.spaces.has_changed().unwrap_or(false)
            {
                continue;
            }
            let coverage = if state.space.device_id != state.host.device_id() {
                SkillUsageCoverage::Unavailable
            } else if state.registry_complete
                && state.contributions.values().all(|(_, c)| c.complete)
            {
                SkillUsageCoverage::Complete
            } else {
                SkillUsageCoverage::Partial
            };
            let snapshot = state.snapshot(coverage);
            if state.last.as_ref() == Some(&snapshot) {
                continue;
            }
            let value = serde_json::to_value(&snapshot).ok()?;
            state.last = Some(snapshot);
            return Some((value, state));
        }
    })
    .boxed()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    fn space() -> Space {
        serde_json::from_value(
            json!({"id":"p","deviceId":"host","path":"/main","createdAt":"2026-01-01T00:00:00Z"}),
        )
        .unwrap()
    }
    fn chat(id: &str) -> Chat {
        serde_json::from_value(json!({"id":id,"deviceId":"host","spaceId":"p","cwd":"/tree","archived":true,"createdAt":"2026-01-01T00:00:00Z"})).unwrap()
    }
    fn catalog() -> Vec<SkillUsageCatalogEntry> {
        vec![SkillUsageCatalogEntry {
            name: "a".into(),
            relative_path: Some(".agents/skills/a/SKILL.md".into()),
        }]
    }
    fn entry(id: &str, calls: &[&str]) -> SessionMessageEntry {
        serde_json::from_value(json!({"id":id,"role":"assistant","createdAt":1,"deviceId":"host","parts":calls.iter().map(|id| json!({"kind":"tool","id":id,"call":{"kind":"readFile","path":"/tree/.agents/skills/a/SKILL.md"}})).collect::<Vec<_>>()})).unwrap()
    }
    fn fork(id: &str) -> SessionMessageEntry {
        serde_json::from_value(json!({"id":"seam","role":"system","createdAt":2,"deviceId":"host","parts":[{"kind":"fork","id":format!("fork:{id}"),"sourceChatId":"parent","sourceTitle":"parent"}]})).unwrap()
    }
    fn count(chat: &Chat, entries: &[SessionMessageEntry]) -> u64 {
        chat_skill_usage(chat, &space(), entries, &catalog())
            .counts
            .values()
            .sum()
    }
    #[test]
    fn skill_usage_aggregates_archived_and_isolates_exact_project() {
        assert_eq!(
            count(&chat("a"), &[entry("e", &["1", "2"])])
                + count(&chat("b"), &[entry("e", &["1", "2", "3"])]),
            5
        );
        let mut other = chat("other");
        other.space_id = Some("other".into());
        assert_eq!(count(&other, &[entry("e", &["1"])]), 0);
        other.space_id = None;
        assert_eq!(count(&other, &[entry("e", &["1"])]), 0);
    }
    #[test]
    fn skill_usage_replay_continuations_and_user_calls() {
        let initial = entry("e", &["1"]);
        let mut finished = initial.clone();
        let MessagePart::Tool { resolved, .. } = &mut finished.parts[0] else {
            unreachable!()
        };
        *resolved = true;
        let mut failed = finished.clone();
        let MessagePart::Tool { is_error, .. } = &mut failed.parts[0] else {
            unreachable!()
        };
        *is_error = true;
        let mut continuation = entry("next", &["1", "2"]);
        continuation.continuation_of = Some("e".into());
        assert_eq!(
            count(
                &chat("a"),
                &[
                    initial.clone(),
                    finished,
                    failed,
                    initial,
                    continuation.clone(),
                ]
            ),
            2
        );
        continuation.role = MessageRole::User;
        assert_eq!(count(&chat("a"), &[continuation]), 0);
        let mut prose = SessionMessageEntry {
            parts: vec![MessagePart::Text {
                id: "mention".into(),
                text: "Please use skill://a (zeron-invoke:a)".into(),
            }],
            ..entry("prose", &[])
        };
        assert_eq!(count(&chat("a"), &[prose.clone()]), 0);
        prose.role = MessageRole::User;
        assert_eq!(count(&chat("a"), &[prose]), 0);
        assert_eq!(
            count(
                &chat("a"),
                &[entry("e", &["1"]), entry("different", &["1"])]
            ),
            2
        );
    }
    #[test]
    fn skill_usage_own_fork_boundary_and_missing_boundary() {
        let mut child = chat("child");
        child.parent_chat_id = Some("parent".into());
        assert_eq!(
            count(
                &child,
                &[
                    entry("inherited", &["1", "2"]),
                    fork("parent"),
                    entry("also-inherited", &["3"]),
                    fork("child"),
                    entry("own", &["4"])
                ]
            ),
            1
        );
        assert!(
            !chat_skill_usage(
                &child,
                &space(),
                &[entry("inherited", &["1"]), fork("parent")],
                &catalog()
            )
            .complete
        );
        assert_eq!(count(&child, &[entry("own", &["1"])]), 1);
    }
    #[tokio::test]
    async fn skill_usage_coverage_does_not_change_existing_local_read_contract() {
        let dir = tempfile::tempdir().unwrap();
        let core = crate::EngineCore::assemble_with_identity(
            dir.path(),
            std::sync::Arc::new(crate::HarnessRegistry::new()),
            zeron_proto::HarnessId::Mock,
            None,
            "dev-org",
            "dev-user",
        )
        .unwrap();
        let store = zeron_sync::DocsStore::open(dir.path().join("orgs/dev-org/dev-user")).unwrap();
        let doc = zeron_doc::SessionDoc::init("a").unwrap();
        store
            .save_snapshot_with_cursor("a", &doc.export_snapshot().unwrap(), 0, 2)
            .unwrap();
        doc.push_message(&entry("missing-dependency", &["1"]))
            .unwrap();
        let vv = doc.doc().oplog_vv();
        doc.push_message(&entry("dependent", &["2"])).unwrap();
        store
            .enqueue_chat_update(
                "a",
                "dependent",
                &doc.doc().export(loro::ExportMode::updates(&vv)).unwrap(),
            )
            .unwrap();
        // Existing replay callers may inspect available local rows. Coverage
        // metadata must not turn their previous successful read into an error.
        assert!(core.doc_host.read_local_doc("a").unwrap().is_some());
        core.shutdown().await;
    }
    #[tokio::test]
    async fn skill_usage_review_f5_cached_replay_survives_bad_outbox() {
        let dir = tempfile::tempdir().unwrap();
        let core = crate::EngineCore::assemble_with_identity(
            dir.path(),
            std::sync::Arc::new(crate::HarnessRegistry::new()),
            zeron_proto::HarnessId::Mock,
            None,
            "dev-org",
            "dev-user",
        )
        .unwrap();
        core.workspace
            .create_space("p", &core.device_id, "/main", None, false)
            .unwrap();
        core.workspace
            .create_chat("child", Some("p"), None, None, Some("/tree".into()))
            .unwrap();
        let child = core.doc_host.open_local("child").unwrap();
        child
            .doc()
            .push_message(&entry("available", &["1"]))
            .unwrap();
        let store = zeron_sync::DocsStore::open(dir.path().join("orgs/dev-org/dev-user")).unwrap();
        store
            .enqueue_chat_update("child", "malformed", b"not a loro update")
            .unwrap();
        assert!(core.doc_host.read_local_doc_with_coverage("child").is_err());
        let cached = core
            .doc_host
            .read_local_doc("child")
            .expect("cached replay must not validate outbox")
            .unwrap();
        assert!(
            cached
                .read_entries()
                .unwrap()
                .iter()
                .any(|e| e.id == "available")
        );
        assert!(std::sync::Arc::ptr_eq(&cached, &child.doc_arc()));
        core.shutdown().await;
    }
}
