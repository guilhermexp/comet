//! Settings → Projects presentation over the shared project readers: the
//! search predicate, the rename mutation and the Sessions tab rows. Which
//! projects exist, which checkouts, Worker sessions and chats belong to each
//! live in `zeron_workers_unpeel::project_activity`, which the Workers
//! controller's `list_projects` also reads. A project is a Space; a Workers
//! checkout without a project is only ever association-pending.

use zeron_proto::Chat;
use zeron_workers_unpeel::project_activity::{self, ProjectEntry};
use zeron_workers_unpeel::project_ledger::{self, CheckoutAvailability, ProjectRow};
use zeron_workers_unpeel::{WorkerParentLink, WorkersSession};

/// Search over the project name, its device name and path, and every
/// checkout's name, branch or path. `Some(None)`: the project itself matched;
/// `Some(Some(row))`: this checkout is the match to focus.
pub(crate) fn entry_match<'a>(
    entry: &'a ProjectEntry,
    query: &str,
) -> Option<Option<&'a ProjectRow>> {
    let query = query.trim().to_lowercase();
    if query.is_empty()
        || entry.space.name.to_lowercase().contains(&query)
        || entry.space.path.to_lowercase().contains(&query)
        || entry
            .space
            .device_name
            .as_deref()
            .is_some_and(|name| name.to_lowercase().contains(&query))
    {
        return Some(None);
    }
    project_ledger::group_matches_query(&entry.group, &query).map(Some)
}

/// `Mutate renameSpace` for a project, local or remote: the name is the
/// Space's, so the rename reaches every device and listing.
pub fn rename_project_params(space_id: &str, name: &str) -> serde_json::Value {
    serde_json::json!({ "op": "renameSpace", "spaceId": space_id, "name": name })
}

// ── Sessions tab ────────────────────────────────────────────────────────────

/// One Worker session of the project, from any of its checkouts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SessionKind {
    pub session_id: String,
    /// Branch, or "Principal" for the project's own folder.
    pub checkout: String,
    pub live: bool,
    pub archived: bool,
    pub checkout_available: bool,
    /// `(chat id, title)` of the Orchestrator chat that launched it.
    pub parent_chat: Option<(String, String)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SessionRow {
    pub kind: SessionKind,
    pub title: String,
    pub runtime: String,
    /// The agent's mark (Claude, OMP, Codex…), as in the Workers sidebar.
    pub runtime_icon: &'static str,
    /// The model the Worker ran on: the active one, else the first reported.
    pub model: Option<String>,
    pub status: String,
    pub last_activity_ms: u64,
}

/// What activating a row opens in the side panel. A Worker row opens that
/// Worker's own terminal, never the Orchestrator chat that launched it;
/// chats open from the Orchestrator sessions tab. Nothing here restarts a
/// Worker: stopped, archived or checkout-less sessions replay read-only.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum SessionPanelTarget {
    /// The chat-opened Worker surface, attached to the running session.
    Worker { session_id: String },
    /// The same surface over the session's recorded output, never restarted.
    WorkerReplay { session_id: String },
    /// An Orchestrator chat transcript, read-only, with a way to the chat.
    Chat { chat_id: String },
}

/// One Orchestrator chat of the project, for the Orchestrator sessions tab.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ChatSessionRow {
    pub chat_id: String,
    pub title: String,
    pub archived: bool,
    /// Worker sessions this chat launched.
    pub workers: usize,
    pub last_activity_ms: u64,
}

/// The Orchestrator sessions tab of one project: every chat of its Space,
/// live and archived, newest first.
pub(crate) fn chat_rows(
    entry: &ProjectEntry,
    chats: &[Chat],
    links: &[WorkerParentLink],
) -> Vec<ChatSessionRow> {
    project_activity::project_chats(entry, chats)
        .into_iter()
        .map(|chat| ChatSessionRow {
            chat_id: chat.id.clone(),
            title: project_activity::chat_title(chat),
            archived: chat.archived,
            workers: project_activity::launched_workers(&chat.id, links),
            last_activity_ms: project_activity::chat_last_activity_ms(chat),
        })
        .collect()
}

pub(crate) fn activation(row: &SessionRow) -> SessionPanelTarget {
    let worker = &row.kind;
    if worker.live && !worker.archived && worker.checkout_available {
        SessionPanelTarget::Worker {
            session_id: worker.session_id.clone(),
        }
    } else {
        SessionPanelTarget::WorkerReplay {
            session_id: worker.session_id.clone(),
        }
    }
}

/// The Sessions tab of one project: every Worker session of any of its
/// checkouts (principal and worktrees, live and archived), newest first —
/// the Workers counterpart of Settings → Archived sessions. Worker sessions
/// are device-local, so a project of another device has none here.
pub(crate) fn session_rows(
    entry: &ProjectEntry,
    chats: &[Chat],
    workers: &[WorkersSession],
    links: &[WorkerParentLink],
) -> Vec<SessionRow> {
    project_activity::project_sessions(entry, workers)
        .into_iter()
        .map(|(session, checkout)| {
            let principal = checkout.checkout_kind == Some(project_ledger::CheckoutKind::Primary);
            let parent_chat = links
                .iter()
                .find(|link| link.worker_session_id == session.id)
                .map(|link| {
                    let title = chats
                        .iter()
                        .find(|chat| chat.id == link.parent_chat_id)
                        .and_then(|chat| chat.title.clone())
                        .unwrap_or_else(|| "Orchestrator chat".into());
                    (link.parent_chat_id.clone(), title)
                });
            SessionRow {
                kind: SessionKind {
                    session_id: session.id.clone(),
                    checkout: if principal {
                        "Principal".into()
                    } else {
                        checkout
                            .display_branch()
                            .unwrap_or(checkout.name.as_str())
                            .to_owned()
                    },
                    live: session.is_live(),
                    archived: session.archived,
                    checkout_available: checkout.checkout_availability
                        != Some(CheckoutAvailability::Missing),
                    parent_chat,
                },
                title: session.title.clone(),
                runtime: project_activity::worker_provider(session),
                runtime_icon: crate::workers::presentation::runtime_icon_path(
                    session.provider_id.as_deref(),
                    Some(session.command.as_str()),
                ),
                model: session
                    .model_usage
                    .iter()
                    .find(|usage| usage.active)
                    .or_else(|| session.model_usage.first())
                    .map(|usage| usage.model.clone()),
                status: if session.archived {
                    "Archived".into()
                } else if session.is_live() {
                    session.activity.clone()
                } else {
                    session.state.clone()
                },
                last_activity_ms: session.updated_at_unix_ms,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};
    use zeron_proto::{Device, Space};
    use zeron_workers_unpeel::project_activity::project_entries;
    use zeron_workers_unpeel::project_ledger::{AssociationState, CheckoutKind, CheckoutOwnership};

    fn space(id: &str, device: &str, path: &str, created_ms: i64) -> Space {
        Space {
            id: id.into(),
            device_id: device.into(),
            path: path.into(),
            name: None,
            git_detected: true,
            git_checked_at: None,
            checkout_id: None,
            created_at: Utc.timestamp_millis_opt(created_ms).unwrap(),
        }
    }

    fn device(id: &str, name: &str) -> Device {
        Device {
            id: id.into(),
            name: name.into(),
            platform: "macos".into(),
            last_seen_at: None,
            created_at: None,
            version: None,
            cursor_sdk_version: None,
            capabilities: Vec::new(),
        }
    }

    fn chat(id: &str, space_id: &str, at_ms: i64, archived: bool) -> Chat {
        Chat {
            id: id.into(),
            device_id: "dev-local".into(),
            title: Some(format!("chat {id}")),
            archived,
            cwd: None,
            branch: None,
            checkout_id: None,
            source_context: None,
            config: None,
            last_message_preview: None,
            last_message_at: Some(Utc.timestamp_millis_opt(at_ms).unwrap()),
            created_at: Utc.timestamp_millis_opt(1).unwrap(),
            harness_session_id: None,
            harness_session_cwd: None,
            parent_chat_id: None,
            space_id: Some(space_id.into()),
            last_seen_at: None,
            room_gen: None,
            origin_chat_id: None,
        }
    }

    fn checkout(
        space_id: Option<&str>,
        id: &str,
        path: &str,
        kind: CheckoutKind,
        availability: CheckoutAvailability,
        branch: &str,
        opened_ms: u64,
    ) -> ProjectRow {
        ProjectRow {
            project_id: Some(id.into()),
            path: path.into(),
            name: path.rsplit('/').next().unwrap().into(),
            added_at_unix_ms: 1,
            last_opened_at_unix_ms: opened_ms,
            icon_path: None,
            repository_id: Some("repo".into()),
            checkout_id: Some(id.into()),
            checkout_kind: Some(kind),
            checkout_ownership: Some(CheckoutOwnership::External),
            checkout_availability: Some(availability),
            current_branch: Some(branch.into()),
            last_known_branch: None,
            association: if space_id.is_some() {
                AssociationState::Known
            } else {
                AssociationState::Pending
            },
            archived: false,
            space_id: space_id.map(str::to_owned),
        }
    }

    fn worker(id: &str, checkout_id: &str, state: &str, archived: bool, at: u64) -> WorkersSession {
        WorkersSession {
            id: id.to_owned(),
            project_id: checkout_id.to_owned(),
            title: format!("worker {id}"),
            command: "/usr/local/bin/omp --session-dir x".to_owned(),
            state: state.to_owned(),
            activity: "idle".to_owned(),
            unread: false,
            pinned: false,
            archived,
            provider_id: None,
            active_runtime_id: None,
            runtime_launch_pending: false,
            runtime_generation: 1,
            notify_when_done: false,
            terminal_background_hex: None,
            worktree_branch: None,
            created_at_unix_ms: 1,
            updated_at_unix_ms: at,
            idle_since_unix_ms: None,
            idle_confirmed_by_hook: false,
            resumable_conversation: false,
            total_tokens: None,
            model_usage: Vec::new(),
            capabilities: zeron_workers_unpeel::WorkersSessionCapabilities::default(),
        }
    }

    /// The owner's registry: local JK with worktrees, `orchestrator`, and a
    /// remote project; plus one unlinked Workers checkout.
    fn fixture() -> (Vec<ProjectEntry>, Vec<ProjectRow>) {
        let spaces = vec![
            space("space-jk", "dev-local", "/p/JK Distribuição", 10),
            space("space-orch", "dev-local", "/Users/me/orchestrator", 20),
            space(
                "space-craft",
                "dev-mini",
                "/Users/mini/craft-agents-oss",
                30,
            ),
        ];
        let devices = vec![
            device("dev-local", "MacBook"),
            device("dev-mini", "Mac mini"),
        ];
        let rows = vec![
            checkout(
                Some("space-jk"),
                "comet-jk",
                "/p/JK Distribuição",
                CheckoutKind::Primary,
                CheckoutAvailability::Available,
                "main",
                500,
            ),
            checkout(
                Some("space-jk"),
                "comet-sec-cron",
                "/p/.worktrees-jk/sec-cron",
                CheckoutKind::Linked,
                CheckoutAvailability::Available,
                "sec/cron",
                400,
            ),
            checkout(
                Some("space-jk"),
                "comet-sec-old",
                "/p/.worktrees-jk/sec-old",
                CheckoutKind::Linked,
                CheckoutAvailability::Missing,
                "sec/old",
                300,
            ),
            checkout(
                None,
                "comet-stray",
                "/tmp/stray",
                CheckoutKind::Unresolved,
                CheckoutAvailability::ProbeFailed,
                "x",
                900,
            ),
        ];
        let chats = vec![chat("c-orch", "space-orch", 800, false)];
        project_entries(&spaces, &devices, Some("dev-local"), &chats, &rows)
    }

    #[test]
    fn every_registry_project_is_listed_once_with_its_device_newest_first() {
        let (entries, pending) = fixture();
        let ids: Vec<&str> = entries.iter().map(|e| e.space.id.as_str()).collect();
        assert_eq!(ids, vec!["space-orch", "space-jk", "space-craft"]);
        let craft = &entries[2];
        assert_eq!(craft.space.device_name.as_deref(), Some("Mac mini"));
        assert!(!craft.space.local);
        assert!(
            craft.group.checkouts.is_empty(),
            "remote projects carry no checkouts"
        );
        let jk = &entries[1];
        assert_eq!(jk.space.name, "JK Distribuição");
        assert_eq!(jk.group.checkouts.len(), 3);
        assert_eq!(jk.group.available_checkouts().count(), 2);
        assert_eq!(jk.group.historical_checkouts().count(), 1);
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].project_id.as_deref(), Some("comet-stray"));
        // No project row exists for a Workers registration without a project.
        assert!(entries.iter().all(|e| e.space.id != "comet-stray"));
    }

    #[test]
    fn an_empty_registry_lists_no_projects() {
        let (entries, pending) = project_entries(&[], &[], Some("dev-local"), &[], &[]);
        assert!(entries.is_empty() && pending.is_empty());
    }

    #[test]
    fn search_matches_name_device_path_and_checkouts_case_insensitively() {
        let (entries, _) = fixture();
        let by_id = |id: &str| entries.iter().find(|e| e.space.id == id).unwrap();
        assert_eq!(entry_match(by_id("space-craft"), "MAC MINI"), Some(None));
        assert_eq!(entry_match(by_id("space-orch"), "orchestr"), Some(None));
        let child = entry_match(by_id("space-jk"), "SEC/CRON").unwrap().unwrap();
        assert_eq!(child.path, "/p/.worktrees-jk/sec-cron");
        let historical = entry_match(by_id("space-jk"), "sec-old").unwrap().unwrap();
        assert_eq!(
            historical.checkout_availability,
            Some(CheckoutAvailability::Missing)
        );
        assert!(
            entries
                .iter()
                .all(|e| entry_match(e, "nothing-matches").is_none())
        );
    }

    #[test]
    fn sessions_list_every_checkout_worker_newest_first() {
        let (entries, _) = fixture();
        let jk = entries.iter().find(|e| e.space.id == "space-jk").unwrap();
        let chats = vec![chat("c-jk", "space-jk", 700, false)];
        let workers = vec![
            worker("w-main", "comet-jk", "running", false, 800),
            worker("w-cron", "comet-sec-cron", "exited", true, 600),
            worker("w-gone", "comet-sec-old", "exited", false, 200),
            worker("w-elsewhere", "comet-other", "running", false, 950),
        ];
        let links = vec![WorkerParentLink {
            worker_session_id: "w-main".into(),
            parent_chat_id: "c-jk".into(),
            registered_at_unix_ms: 1,
        }];
        let rows = session_rows(jk, &chats, &workers, &links);
        let titles: Vec<&str> = rows.iter().map(|row| row.title.as_str()).collect();
        assert_eq!(
            titles,
            vec!["worker w-main", "worker w-cron", "worker w-gone"]
        );
        assert_eq!(rows[0].kind.checkout, "Principal");
        assert_eq!(
            rows[0].kind.parent_chat,
            Some(("c-jk".to_owned(), "chat c-jk".to_owned()))
        );
        assert_eq!(rows[1].kind.checkout, "sec/cron");
        assert_eq!(rows[1].status, "Archived");
        assert_eq!(rows[0].runtime, "omp");
        assert_eq!(rows[0].runtime_icon, crate::icons::WORKER_OMP);
        assert_eq!(rows[0].model, None);
    }

    #[test]
    fn orchestrator_rows_list_the_project_chats_with_their_workers() {
        let (entries, _) = fixture();
        let jk = entries.iter().find(|e| e.space.id == "space-jk").unwrap();
        let chats = vec![
            chat("c-old", "space-jk", 100, true),
            chat("c-new", "space-jk", 900, false),
            chat("c-other", "space-other", 950, false),
        ];
        let links = vec![WorkerParentLink {
            worker_session_id: "w-main".into(),
            parent_chat_id: "c-new".into(),
            registered_at_unix_ms: 1,
        }];
        let rows = chat_rows(jk, &chats, &links);
        let ids: Vec<&str> = rows.iter().map(|row| row.chat_id.as_str()).collect();
        assert_eq!(ids, vec!["c-new", "c-old"]);
        assert_eq!(rows[0].workers, 1);
        assert!(rows[1].archived);
    }

    #[test]
    fn session_rows_carry_the_agent_mark_and_the_active_model() {
        let (entries, _) = fixture();
        let jk = entries.iter().find(|e| e.space.id == "space-jk").unwrap();
        let mut claude = worker("w-claude", "comet-jk", "running", false, 800);
        claude.command = "claude --resume abc".into();
        claude.model_usage = vec![
            zeron_workers_unpeel::WorkersModelTokenUsage {
                model: "claude-sonnet".into(),
                total_tokens: 10,
                active: false,
            },
            zeron_workers_unpeel::WorkersModelTokenUsage {
                model: "claude-opus".into(),
                total_tokens: 20,
                active: true,
            },
        ];
        let rows = session_rows(jk, &[], &[claude], &[]);
        assert_eq!(rows[0].runtime_icon, crate::icons::WORKER_CLAUDE);
        assert_eq!(rows[0].model.as_deref(), Some("claude-opus"));
    }

    #[test]
    fn activating_rows_never_restarts_a_worker() {
        let (entries, _) = fixture();
        let jk = entries.iter().find(|e| e.space.id == "space-jk").unwrap();
        let workers = vec![
            worker("w-live", "comet-jk", "running", false, 3),
            worker("w-archived", "comet-sec-cron", "exited", true, 2),
            worker("w-missing", "comet-sec-old", "running", false, 1),
        ];
        let rows = session_rows(jk, &[], &workers, &[]);
        let targets: Vec<SessionPanelTarget> = rows.iter().map(activation).collect();
        assert_eq!(
            targets,
            vec![
                SessionPanelTarget::Worker {
                    session_id: "w-live".into()
                },
                SessionPanelTarget::WorkerReplay {
                    session_id: "w-archived".into()
                },
                // A checkout that disappeared still lists and replays.
                SessionPanelTarget::WorkerReplay {
                    session_id: "w-missing".into()
                },
            ]
        );
    }

    #[test]
    fn a_remote_project_has_no_worker_sessions_here() {
        let (entries, _) = fixture();
        let craft = entries
            .iter()
            .find(|e| e.space.id == "space-craft")
            .unwrap();
        let workers = vec![worker("w-main", "comet-jk", "running", false, 800)];
        assert!(session_rows(craft, &[], &workers, &[]).is_empty());
    }
}
