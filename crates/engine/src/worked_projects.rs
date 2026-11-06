//! Host-owned, cold-history projection of worked project identities.
use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::Duration,
};

use loro::VersionVector;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use zeron_doc::{MessagePart, MessageRole};
use zeron_proto::{Chat, ChatWorkedProjects, Space};
use zeron_workers_unpeel::space_links::{linked_checkouts_at, workers_state_path};

use crate::{
    EngineError,
    doc_host::DocHost,
    fs_watch::{FsWatch, FsWatchHub},
    sessions::SessionsEngine,
    workspace_host::WorkspaceHost,
};

type Catalog = Vec<(String, String)>;

#[derive(Clone)]
pub struct WorkedProjectsSync {
    inner: Arc<Inner>,
}
struct Inner {
    workspace: WorkspaceHost,
    device_id: String,
    demand: mpsc::UnboundedSender<Vec<String>>,
    cancel: CancellationToken,
    task: Mutex<Option<tokio::task::JoinHandle<()>>>,
}

impl WorkedProjectsSync {
    pub fn start(
        host: DocHost,
        workspace: WorkspaceHost,
        sessions: &SessionsEngine,
        device_id: &str,
    ) -> Self {
        let (demand, demand_rx) = mpsc::unbounded_channel();
        let cancel = CancellationToken::new();
        let inner = Arc::new(Inner {
            workspace: workspace.clone(),
            device_id: device_id.into(),
            demand,
            cancel: cancel.clone(),
            task: Mutex::new(None),
        });
        // Install every receiver before taking the first registry snapshot.
        let chats = workspace.watch_chats();
        let spaces = workspace.watch_spaces();
        let session_watch = sessions.watch_sessions();
        let history = host.watch_history_changes();
        let task = tokio::spawn(supervise(
            host,
            workspace,
            device_id.into(),
            chats,
            spaces,
            session_watch,
            history,
            demand_rx,
            cancel,
        ));
        *inner.task.lock().unwrap_or_else(|e| e.into_inner()) = Some(task);
        Self { inner }
    }

    pub fn ensure(&self, chat_ids: &[String]) -> Result<(), EngineError> {
        let mut unique = HashSet::new();
        for id in chat_ids {
            let chat = self
                .inner
                .workspace
                .chat(id)?
                .ok_or_else(|| EngineError::Other(format!("unknown chat: {id}")))?;
            if chat.device_id != self.inner.device_id {
                return Err(EngineError::Other(format!(
                    "chat is not hosted by this device: {id}"
                )));
            }
            unique.insert(id.clone());
        }
        self.inner
            .demand
            .send(unique.into_iter().collect())
            .map_err(|_| EngineError::Other("worked project service is stopped".into()))
    }

    pub async fn shutdown(&self) {
        self.inner.cancel.cancel();
        let task = self
            .inner
            .task
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .take();
        if let Some(task) = task {
            let _ = task.await;
        }
    }
}

fn catalog(path: &Path, spaces: &[Space], device: &str) -> Result<Catalog, String> {
    if !path.exists() {
        return Err("checkout catalog is unavailable".into());
    }
    let owned: HashSet<_> = spaces
        .iter()
        .filter(|s| s.device_id == device)
        .map(|s| s.id.as_str())
        .collect();
    let mut pairs: Catalog = linked_checkouts_at(path)?
        .into_iter()
        .filter(|(_, id)| owned.contains(id.as_str()))
        .map(|(path, id)| (path.to_string_lossy().into_owned(), id))
        .collect();
    pairs.sort();
    pairs.dedup();
    Ok(pairs)
}

struct Observation {
    ids: Vec<String>,
    complete: bool,
    version: Option<VersionVector>,
}
fn observe(host: &DocHost, chat: &Chat, catalog: &Catalog) -> Option<Observation> {
    let (doc, coverage) = host.read_local_doc_with_coverage(&chat.id).ok()??;
    let before = doc.doc().state_vv();
    let (entries, decoded) = doc.read_entries_for_attribution().ok()?;
    let calls = entries
        .iter()
        .filter(|entry| entry.role == MessageRole::Assistant)
        .flat_map(|entry| entry.parts.iter())
        .filter_map(|part| match part {
            MessagePart::Tool { call, .. } => Some(call),
            _ => None,
        });
    let roots: Vec<_> = catalog.iter().map(|(path, _)| path.as_str()).collect();
    let cwd = chat.cwd.as_deref().unwrap_or("");
    let home = std::env::var_os("HOME").map(PathBuf::from);
    let indices = zeron_proto::worked_projects::worked_root_indices(
        calls,
        &roots,
        Path::new(cwd),
        home.as_deref(),
    );
    let mut seen = HashSet::new();
    let ids = indices
        .into_iter()
        .filter_map(|i| {
            let id = &catalog[i].1;
            seen.insert(id.clone()).then(|| id.clone())
        })
        .collect();
    let stable = before == doc.doc().state_vv()
        && doc.chat_id().as_deref() == Some(chat.id.as_str())
        && entries.len() == doc.doc().get_list("messages").len();
    let version = (stable && !before.is_empty()).then_some(before);
    Some(Observation {
        ids,
        complete: coverage && stable && decoded,
        version,
    })
}

// A non-canonical varint prefix cannot begin a Loro VersionVector. It records
// that some retained IDs lack provenance while keeping all known causal state.
const UNKNOWN_EVIDENCE_PREFIX: [u8; 12] = [0xff; 12];

fn decode_evidence(bytes: Option<&[u8]>) -> (Option<VersionVector>, bool) {
    let Some(bytes) = bytes else {
        return (None, true);
    };
    let (bytes, unknown) = match bytes.strip_prefix(&UNKNOWN_EVIDENCE_PREFIX) {
        Some(bytes) => (bytes, true),
        None => (bytes, false),
    };
    let version = VersionVector::decode(bytes)
        .ok()
        .filter(|vv| !vv.is_empty());
    let unknown = unknown || version.is_none();
    (version, unknown)
}

fn encode_evidence(version: VersionVector, unknown: bool) -> Vec<u8> {
    let encoded = version.encode();
    if !unknown {
        return encoded;
    }
    let mut bytes = Vec::with_capacity(UNKNOWN_EVIDENCE_PREFIX.len() + encoded.len());
    bytes.extend_from_slice(&UNKNOWN_EVIDENCE_PREFIX);
    bytes.extend_from_slice(&encoded);
    bytes
}

fn combine(
    previous: Option<&ChatWorkedProjects>,
    observed: Option<Observation>,
) -> Option<ChatWorkedProjects> {
    let Some(observed) = observed else {
        return previous.map(|old| {
            let mut old = old.clone();
            old.complete = false;
            old
        });
    };
    let Some(old) =
        previous.filter(|old| !old.project_ids.is_empty() || old.history_version.is_some())
    else {
        return Some(ChatWorkedProjects {
            project_ids: observed.ids,
            complete: observed.complete && observed.version.is_some(),
            history_version: observed.version.map(|v| v.encode()),
        });
    };
    let (mut prior, mut unknown) = decode_evidence(old.history_version.as_deref());
    if observed.complete
        && !unknown
        && prior
            .as_ref()
            .zip(observed.version.as_ref())
            .is_some_and(|(prior, current)| current.includes_vv(prior))
    {
        return Some(ChatWorkedProjects {
            project_ids: observed.ids,
            complete: true,
            history_version: observed.version.map(|v| v.encode()),
        });
    }
    let mut ids = old.project_ids.clone();
    for id in observed.ids {
        if !ids.contains(&id) {
            ids.push(id);
        }
    }
    let introduced = ids.len() != old.project_ids.len();
    match observed.version {
        Some(current) => match &mut prior {
            Some(prior) => prior.merge(&current),
            None => prior = Some(current),
        },
        None if introduced => unknown = true,
        None => {}
    }
    let version = match prior {
        Some(prior) => Some(encode_evidence(prior, unknown)),
        None if !introduced => old.history_version.clone(),
        None => None,
    };
    Some(ChatWorkedProjects {
        project_ids: ids,
        complete: false,
        history_version: version,
    })
}

// Leave room for the registry op envelope; a causal witness must never be
// truncated just to fit. If possible, downgrade the retained summary.
fn fit_registry_budget(
    previous: Option<&ChatWorkedProjects>,
    summary: ChatWorkedProjects,
) -> Option<ChatWorkedProjects> {
    let fits = |value: &ChatWorkedProjects| {
        serde_json::to_vec(value).is_ok_and(|bytes| bytes.len() <= 12 * 1024)
    };
    if fits(&summary) {
        return Some(summary);
    }
    let mut retained = previous?.clone();
    retained.complete = false;
    fits(&retained).then_some(retained)
}

async fn cancellable_read<T: Send + 'static>(
    cancel: &CancellationToken,
    read: impl FnOnce() -> T + Send + 'static,
) -> Option<T> {
    tokio::select! {
        _ = cancel.cancelled() => None,
        result = tokio::time::timeout(Duration::from_secs(30), tokio::task::spawn_blocking(read)) => {
            result.ok()?.ok()
        }
    }
}

fn session_changed_for(
    sessions: &tokio::sync::watch::Receiver<Vec<zeron_proto::Session>>,
    previous: &[zeron_proto::Session],
    chat_id: &str,
) -> bool {
    sessions.has_changed().unwrap_or(false) && {
        let current = sessions.borrow();
        current.iter().find(|s| s.chat_id == chat_id)
            != previous.iter().find(|s| s.chat_id == chat_id)
    }
}

fn drain_history_for(
    history: &mut tokio::sync::broadcast::Receiver<String>,
    working: &HashSet<String>,
    pending: &mut HashSet<String>,
    dirty: &mut HashSet<String>,
    chat_id: &str,
) -> (bool, bool) {
    let mut changed = false;
    let mut lagged = false;
    loop {
        match history.try_recv() {
            Ok(id) => {
                changed |= id == chat_id;
                if working.contains(&id) {
                    pending.insert(id);
                } else {
                    dirty.insert(id);
                }
            }
            Err(tokio::sync::broadcast::error::TryRecvError::Lagged(_)) => {
                lagged = true;
                changed = true;
            }
            Err(_) => break,
        }
    }
    (changed, lagged)
}

#[allow(clippy::too_many_arguments)]
async fn supervise(
    host: DocHost,
    workspace: WorkspaceHost,
    device: String,
    mut chats: tokio::sync::watch::Receiver<Vec<Chat>>,
    mut spaces: tokio::sync::watch::Receiver<Vec<Space>>,
    mut sessions: tokio::sync::watch::Receiver<Vec<zeron_proto::Session>>,
    mut history: tokio::sync::broadcast::Receiver<String>,
    mut demand: mpsc::UnboundedReceiver<Vec<String>>,
    cancel: CancellationToken,
) {
    let state_path = workers_state_path();
    let (kick, mut kicks) = mpsc::unbounded_channel();
    let mut watch: Option<FsWatch> = None;
    let mut timer = tokio::time::interval(Duration::from_secs(120));
    timer.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    let mut requested = HashSet::new();
    let mut dirty = HashSet::new();
    let mut pending_history: HashSet<String> = HashSet::new();
    let mut prior_sessions: Vec<zeron_proto::Session> = Vec::new();
    let mut contexts: HashMap<String, (String, Option<String>, bool, Option<String>)> =
        HashMap::new();
    let mut previous_catalog: Option<Catalog> = None;
    let mut force_all = true;
    loop {
        if cancel.is_cancelled() {
            break;
        }
        chats.borrow_and_update();
        spaces.borrow_and_update();
        let current_sessions = sessions.borrow_and_update().clone();
        let working: HashSet<_> = current_sessions
            .iter()
            .filter(|session| session.status == zeron_proto::SessionStatus::Working)
            .map(|session| session.chat_id.clone())
            .collect();
        for session in &current_sessions {
            let previous = prior_sessions
                .iter()
                .find(|old| old.chat_id == session.chat_id);
            if session.status != zeron_proto::SessionStatus::Working
                && previous.is_none_or(|old| {
                    old.status != session.status
                        || old.started_at != session.started_at
                        || old.last_completed_turn != session.last_completed_turn
                })
            {
                dirty.insert(session.chat_id.clone());
            }
        }
        for old in &prior_sessions {
            if !current_sessions
                .iter()
                .any(|session| session.chat_id == old.chat_id)
            {
                dirty.insert(old.chat_id.clone());
            }
        }
        pending_history.retain(|id| {
            if working.contains(id) {
                true
            } else {
                dirty.insert(id.clone());
                false
            }
        });
        prior_sessions = current_sessions;
        if watch.as_ref().is_none_or(|w| !w.is_live()) {
            let parent = state_path.parent().unwrap_or(Path::new(".")).to_path_buf();
            let tx = kick.clone();
            watch = cancellable_read(&cancel, move || {
                FsWatchHub::global().watch(
                    &parent,
                    notify::RecursiveMode::NonRecursive,
                    move |_| {
                        let _ = tx.send(());
                    },
                )
            })
            .await;
        }
        if cancel.is_cancelled() {
            break;
        }
        let ws = workspace.clone();
        let path = state_path.clone();
        let host_id = device.clone();
        let snapshot = cancellable_read(&cancel, move || {
            let chats = ws.read_chats().ok()?;
            let catalog = ws
                .read_spaces()
                .map_err(|error| error.to_string())
                .and_then(|spaces| catalog(&path, &spaces, &host_id));
            Some((chats, catalog))
        })
        .await
        .flatten();
        if let Some((rows, Err(_))) = &snapshot {
            for chat in rows.iter().filter(|chat| {
                chat.device_id == device && (!chat.archived || requested.contains(&chat.id))
            }) {
                if cancel.is_cancelled() {
                    break;
                }
                if let Some(mut summary) = chat.worked_projects.clone()
                    && summary.complete
                {
                    summary.complete = false;
                    let _ = workspace.publish_worked_projects_if_current(chat, &summary);
                }
            }
        }
        let snapshot =
            snapshot.and_then(|(rows, catalog)| catalog.ok().map(|catalog| (rows, catalog)));
        if let Some((rows, current_catalog)) = snapshot {
            if previous_catalog.as_ref() != Some(&current_catalog) {
                force_all = true;
            }
            previous_catalog = Some(current_catalog.clone());
            let mut next_contexts = HashMap::new();
            for chat in &rows {
                if chat.device_id != device {
                    continue;
                }
                let context = (
                    chat.device_id.clone(),
                    chat.cwd.clone(),
                    chat.archived,
                    chat.space_id.clone(),
                );
                if contexts.get(&chat.id) != Some(&context) {
                    dirty.insert(chat.id.clone());
                }
                next_contexts.insert(chat.id.clone(), context);
                if force_all && (!chat.archived || requested.contains(&chat.id)) {
                    dirty.insert(chat.id.clone());
                }
            }
            contexts = next_contexts;
            force_all = false;
            let mut pending: Vec<_> = dirty.drain().collect();
            pending.sort();
            for id in pending {
                if cancel.is_cancelled() {
                    break;
                }
                let Some(chat) = rows
                    .iter()
                    .find(|chat| {
                        chat.id == id
                            && chat.device_id == device
                            && (!chat.archived || requested.contains(&chat.id))
                    })
                    .cloned()
                else {
                    continue;
                };
                let reader = host.clone();
                let chat_for_read = chat.clone();
                let roots = current_catalog.clone();
                let observed =
                    cancellable_read(&cancel, move || observe(&reader, &chat_for_read, &roots))
                        .await
                        .flatten();
                if cancel.is_cancelled() {
                    break;
                }
                let (changed, lagged) = drain_history_for(
                    &mut history,
                    &working,
                    &mut pending_history,
                    &mut dirty,
                    &id,
                );
                force_all |= lagged;
                if changed {
                    if working.contains(&id) {
                        pending_history.insert(id);
                    } else {
                        dirty.insert(id);
                    }
                    continue;
                }
                // Publication compares this Chat atomically. The global Chat
                // watch also carries our own unrelated summary writes, which
                // must not restart every remaining cold read in the batch.
                if spaces.has_changed().unwrap_or(false)
                    || session_changed_for(&sessions, &prior_sessions, &id)
                {
                    dirty.insert(id);
                    continue;
                }
                let ws = workspace.clone();
                let path = state_path.clone();
                let host_id = device.clone();
                let checked = cancellable_read(&cancel, move || {
                    ws.read_spaces()
                        .map_err(|error| error.to_string())
                        .and_then(|spaces| catalog(&path, &spaces, &host_id))
                })
                .await;
                if cancel.is_cancelled() {
                    break;
                }
                if !matches!(&checked, Some(Ok(current)) if current == &current_catalog) {
                    dirty.insert(id);
                    force_all = true;
                    continue;
                }
                let (changed, lagged) = drain_history_for(
                    &mut history,
                    &working,
                    &mut pending_history,
                    &mut dirty,
                    &id,
                );
                force_all |= lagged;
                if changed {
                    if working.contains(&id) {
                        pending_history.insert(id);
                    } else {
                        dirty.insert(id);
                    }
                    continue;
                }
                if spaces.has_changed().unwrap_or(false)
                    || session_changed_for(&sessions, &prior_sessions, &id)
                {
                    dirty.insert(id);
                    continue;
                }
                let Some(summary) = combine(chat.worked_projects.as_ref(), observed) else {
                    continue;
                };
                let Some(summary) = fit_registry_budget(chat.worked_projects.as_ref(), summary)
                else {
                    tracing::warn!(chat = %chat.id, "worked-project summary exceeds registry budget");
                    continue;
                };
                if chat.worked_projects.as_ref() == Some(&summary) {
                    continue;
                }
                if cancel.is_cancelled() {
                    break;
                }
                if !workspace
                    .publish_worked_projects_if_current(&chat, &summary)
                    .unwrap_or(false)
                {
                    dirty.insert(id);
                }
            }
        } else {
            // Catalog/read failure is unknown, not an empty authoritative catalog.
            // Retry on the next real signal or repair tick instead of spinning.
            dirty.clear();
            previous_catalog = None;
            force_all = false;
        }
        if !dirty.is_empty() || force_all {
            continue;
        }
        loop {
            tokio::select! {
                _ = cancel.cancelled() => break,
                changed = chats.changed() => { if changed.is_err() { return; } break; },
                changed = spaces.changed() => { if changed.is_err() { return; } force_all = true; break; },
                changed = sessions.changed() => { if changed.is_err() { return; } break; },
                event = history.recv() => match event {
                    Ok(id) if working.contains(&id) => { pending_history.insert(id); },
                    Ok(id) => { dirty.insert(id); break; },
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => { force_all = true; break; },
                    Err(_) => return,
                },
                Some(ids) = demand.recv() => { for id in ids { requested.insert(id.clone()); dirty.insert(id); } break; },
                Some(_) = kicks.recv() => break,
                _ = timer.tick() => { force_all = true; break; },
            }
        }
        chats.borrow_and_update();
        spaces.borrow_and_update();
        sessions.borrow_and_update();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn worked_projects_shutdown_does_not_wait_for_blocked_read() {
        let cancel = CancellationToken::new();
        let (started_tx, started_rx) = tokio::sync::oneshot::channel();
        let (release_tx, release_rx) = std::sync::mpsc::channel();
        let read_cancel = cancel.clone();
        let task = tokio::spawn(async move {
            cancellable_read(&read_cancel, move || {
                let _ = started_tx.send(());
                let _ = release_rx.recv();
                42
            })
            .await
        });
        started_rx.await.unwrap();
        cancel.cancel();
        let result = tokio::time::timeout(Duration::from_secs(1), task).await;
        let _ = release_tx.send(());
        assert!(
            result
                .expect("shutdown waited for a blocked read")
                .unwrap()
                .is_none()
        );
    }

    fn doc_with(value: &str) -> loro::LoroDoc {
        let doc = loro::LoroDoc::new();
        doc.get_map("test").insert("value", value).unwrap();
        doc.commit();
        doc
    }

    #[test]
    fn worked_projects_empty_unversioned_summary_does_not_taint_new_observation() {
        let old = ChatWorkedProjects {
            project_ids: vec![],
            complete: false,
            history_version: None,
        };
        let doc = doc_with("observed");
        let result = combine(
            Some(&old),
            Some(Observation {
                ids: vec!["project".into()],
                complete: true,
                version: Some(doc.state_vv()),
            }),
        )
        .unwrap();
        assert_eq!(result.project_ids, ["project"]);
        assert!(result.complete);
        let prior = doc_with("prior").state_vv();
        let versioned_empty = ChatWorkedProjects {
            project_ids: vec![],
            complete: true,
            history_version: Some(prior.encode()),
        };
        let incomparable = doc_with("incomparable").state_vv();
        let retained = combine(
            Some(&versioned_empty),
            Some(Observation {
                ids: vec!["other".into()],
                complete: true,
                version: Some(incomparable),
            }),
        )
        .unwrap();
        assert!(
            !retained.complete,
            "a versioned empty summary still carries causal evidence"
        );
        assert!(
            VersionVector::decode(retained.history_version.as_ref().unwrap())
                .unwrap()
                .includes_vv(&prior)
        );
    }

    #[test]
    fn worked_projects_other_session_updates_do_not_invalidate_a_cold_read() {
        let session = |id: &str| zeron_proto::Session {
            background_tasks: Default::default(),

            chat_id: id.into(),
            device_id: "host".into(),
            status: zeron_proto::SessionStatus::Idle,
            last_completed_turn: None,
            started_at: None,
            updated_at: chrono::Utc::now(),
            last_turn_outcome: None,
            terminal_activity: None,
            running_subagents: 0,
            context_usage: None,
            error: None,
            turn_stats: None,
        };
        let prior = vec![session("a"), session("b")];
        let (tx, rx) = tokio::sync::watch::channel(prior.clone());
        let mut updated = prior.clone();
        updated[1].status = zeron_proto::SessionStatus::Working;
        tx.send_replace(updated.clone());
        assert!(!session_changed_for(&rx, &prior, "a"));
        assert!(session_changed_for(&rx, &prior, "b"));
        updated[0].status = zeron_proto::SessionStatus::Working;
        tx.send_replace(updated);
        assert!(session_changed_for(&rx, &prior, "a"));
    }

    #[test]
    fn worked_projects_history_invalidation_targets_the_changed_chat() {
        let (tx, mut history) = tokio::sync::broadcast::channel(4);
        let mut pending = HashSet::new();
        let mut dirty = HashSet::new();
        let working = HashSet::from(["working".to_string()]);
        tx.send("other".to_string()).unwrap();
        tx.send("working".to_string()).unwrap();
        let (changed, lagged) =
            drain_history_for(&mut history, &working, &mut pending, &mut dirty, "target");
        assert!(!changed && !lagged);
        assert!(dirty.contains("other"));
        assert!(pending.contains("working"));
        tx.send("target".to_string()).unwrap();
        let (changed, lagged) =
            drain_history_for(&mut history, &working, &mut pending, &mut dirty, "target");
        assert!(changed && !lagged);
        for i in 0..8 {
            tx.send(format!("lag-{i}")).unwrap();
        }
        let (changed, lagged) =
            drain_history_for(&mut history, &working, &mut pending, &mut dirty, "target");
        assert!(
            changed && lagged,
            "lag invalidates the observation conservatively"
        );
    }

    #[test]
    fn worked_projects_oversized_result_keeps_prior_ids_but_marks_them_incomplete() {
        let old = ChatWorkedProjects {
            project_ids: vec!["known".into()],
            complete: true,
            history_version: Some(doc_with("known").state_vv().encode()),
        };
        let oversized = ChatWorkedProjects {
            project_ids: vec!["x".repeat(12 * 1024)],
            complete: true,
            history_version: old.history_version.clone(),
        };
        let retained = fit_registry_budget(Some(&old), oversized).unwrap();
        assert_eq!(retained.project_ids, old.project_ids);
        assert_eq!(retained.history_version, old.history_version);
        assert!(!retained.complete);
    }

    #[test]
    fn worked_projects_older_cold_history_cannot_erase_newer_evidence() {
        let doc = doc_with("a");
        let older = doc.state_vv();
        doc.get_map("test").insert("another", "b").unwrap();
        doc.commit();
        let newer = doc.state_vv();
        let previous = ChatWorkedProjects {
            project_ids: vec!["space-a".into(), "space-b".into()],
            complete: true,
            history_version: Some(newer.encode()),
        };
        let partial = combine(
            Some(&previous),
            Some(Observation {
                ids: vec!["space-a".into()],
                complete: true,
                version: Some(older),
            }),
        )
        .unwrap();
        assert_eq!(partial.project_ids, ["space-a", "space-b"]);
        assert!(!partial.complete);
        let witness = VersionVector::decode(partial.history_version.as_ref().unwrap()).unwrap();
        assert!(witness.includes_vv(&newer));
        let reassociated = combine(
            Some(&partial),
            Some(Observation {
                ids: vec!["space-c".into()],
                complete: true,
                version: Some(newer),
            }),
        )
        .unwrap();
        assert_eq!(reassociated.project_ids, ["space-c"]);
        assert!(reassociated.complete);
    }

    #[test]
    fn worked_projects_unstable_new_contact_retains_prior_causal_evidence() {
        assert!(VersionVector::decode(&UNKNOWN_EVIDENCE_PREFIX).is_err());
        let previous_version = doc_with("prior").state_vv();
        let previous = ChatWorkedProjects {
            project_ids: vec!["space-a".into()],
            complete: true,
            history_version: Some(previous_version.encode()),
        };
        let uncertain = combine(
            Some(&previous),
            Some(Observation {
                ids: vec!["space-b".into()],
                complete: false,
                version: None,
            }),
        )
        .unwrap();
        assert_eq!(uncertain.project_ids, ["space-a", "space-b"]);
        assert!(!uncertain.complete);
        let (known, unknown) = decode_evidence(uncertain.history_version.as_deref());
        assert!(unknown);
        assert!(known.unwrap().includes_vv(&previous_version));
        let additional = doc_with("additional").state_vv();
        let extended = combine(
            Some(&uncertain),
            Some(Observation {
                ids: vec!["space-c".into()],
                complete: false,
                version: Some(additional.clone()),
            }),
        )
        .unwrap();
        let (known, unknown) = decode_evidence(extended.history_version.as_deref());
        let known = known.unwrap();
        assert!(unknown && known.includes_vv(&previous_version) && known.includes_vv(&additional));
        let later = combine(
            Some(&extended),
            Some(Observation {
                ids: vec!["space-a".into()],
                complete: true,
                version: Some(previous_version),
            }),
        )
        .unwrap();
        assert_eq!(later.project_ids, ["space-a", "space-b", "space-c"]);
        assert!(
            !later.complete,
            "unknown provenance cannot authorize deletion"
        );
    }

    #[test]
    fn worked_projects_partial_versions_merge_without_inventing_authority() {
        let a = doc_with("a").state_vv();
        let b = doc_with("b").state_vv();
        let previous = ChatWorkedProjects {
            project_ids: vec!["space-a".into()],
            complete: true,
            history_version: Some(a.encode()),
        };
        let joined = combine(
            Some(&previous),
            Some(Observation {
                ids: vec!["space-b".into()],
                complete: false,
                version: Some(b.clone()),
            }),
        )
        .unwrap();
        assert_eq!(joined.project_ids, ["space-a", "space-b"]);
        assert!(!joined.complete);
        let witness = VersionVector::decode(joined.history_version.as_ref().unwrap()).unwrap();
        assert!(witness.includes_vv(&a));
        assert!(witness.includes_vv(&b));

        let invalid = ChatWorkedProjects {
            history_version: Some(vec![255]),
            ..previous
        };
        let retained = combine(
            Some(&invalid),
            Some(Observation {
                ids: vec!["space-b".into()],
                complete: true,
                version: Some(b.clone()),
            }),
        )
        .unwrap();
        assert_eq!(retained.project_ids, ["space-a", "space-b"]);
        assert!(!retained.complete);
        let (known, unknown) = decode_evidence(retained.history_version.as_deref());
        assert!(unknown);
        assert!(known.unwrap().includes_vv(&b));
        let later = combine(
            Some(&retained),
            Some(Observation {
                ids: vec!["space-b".into()],
                complete: true,
                version: Some(b),
            }),
        )
        .unwrap();
        assert_eq!(later.project_ids, ["space-a", "space-b"]);
        assert!(!later.complete);
    }

    #[tokio::test]
    async fn worked_projects_stale_read_cannot_overwrite_new_summary_or_context() {
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(zeron_sync::DocsStore::open(dir.path()).unwrap());
        let workspace = WorkspaceHost::open(
            store,
            crate::workspace_host::WorkspaceHostConfig {
                device_id: "host".into(),
                device_name: "Host".into(),
                platform: "macos".into(),
                org_id: "org".into(),
                user_id: "user".into(),
                edge: None,
                local_only: true,
            },
        )
        .unwrap();
        workspace
            .create_chat("chat", None, Some("host"), None, Some("/old".into()))
            .unwrap();
        let stale = workspace.chat("chat").unwrap().unwrap();
        let newer = ChatWorkedProjects {
            project_ids: vec!["newer".into()],
            complete: true,
            history_version: Some(doc_with("newer").state_vv().encode()),
        };
        workspace.set_chat_worked_projects("chat", &newer).unwrap();
        let old_result = ChatWorkedProjects {
            project_ids: vec!["older".into()],
            complete: true,
            history_version: None,
        };
        assert!(
            !workspace
                .publish_worked_projects_if_current(&stale, &old_result)
                .unwrap()
        );
        let current = workspace.chat("chat").unwrap().unwrap();
        assert_eq!(current.worked_projects.as_ref(), Some(&newer));
        workspace.set_chat_cwd("chat", "/new").unwrap();
        assert!(
            !workspace
                .publish_worked_projects_if_current(&current, &old_result)
                .unwrap()
        );
        let current = workspace.chat("chat").unwrap().unwrap();
        assert_eq!(current.cwd.as_deref(), Some("/new"));
        assert_eq!(current.worked_projects.as_ref(), Some(&newer));
        let failed_read = combine(Some(&newer), None).unwrap();
        assert_eq!(failed_read.project_ids, newer.project_ids);
        assert_eq!(failed_read.history_version, newer.history_version);
        assert!(!failed_read.complete);
        workspace.shutdown();
    }
}
