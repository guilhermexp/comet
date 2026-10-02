//! What Settings → Projects shows for each project, UI-free: the registry's
//! projects (Spaces from every device) joined with this device's checkout
//! history, and which Worker sessions and Orchestrator chats belong to each.
//! Settings → Projects renders these and the Workers controller's
//! `list_projects` returns them, so both list the same ids for the same
//! project. Tickets are matched in [`crate::project_tickets`].

use std::collections::HashMap;

use zeron_proto::{Chat, Device, Space};

use crate::project_identity::IdentityRegistry;
use crate::project_ledger::{self, ProjectGroup, ProjectRow};
use crate::space_registry::{SpaceRef, space_refs};
use crate::{LocalWorkersClient, WorkerParentLink, WorkersError, WorkersSession};

/// One registry project with this device's checkouts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectEntry {
    pub space: SpaceRef,
    pub created_at_ms: i64,
    /// This device's checkout history linked to the project (empty for a
    /// project of another device).
    pub group: ProjectGroup,
    /// Latest real activity of its chats and checkouts, epoch ms.
    pub last_activity_ms: u64,
}

/// Every registry project with its local checkouts, most recent first, and
/// the checkout records linked to no project.
pub fn project_entries(
    spaces: &[Space],
    devices: &[Device],
    local_device_id: Option<&str>,
    chats: &[Chat],
    rows: &[ProjectRow],
) -> (Vec<ProjectEntry>, Vec<ProjectRow>) {
    let refs = space_refs(spaces, devices, local_device_id.unwrap_or_default());
    let local_projects: Vec<(String, String)> = refs
        .iter()
        .filter(|space| space.local)
        .map(|space| (space.id.clone(), space.name.clone()))
        .collect();
    let (groups, pending) = project_ledger::group_rows_by_project(&local_projects, rows);
    let mut groups: HashMap<String, ProjectGroup> = groups
        .into_iter()
        .map(|group| (group.id.clone(), group))
        .collect();
    let mut chat_activity: HashMap<&str, u64> = HashMap::new();
    for chat in chats {
        let Some(space_id) = chat.space_id.as_deref() else {
            continue;
        };
        let at = chat_last_activity_ms(chat);
        let entry = chat_activity.entry(space_id).or_insert(at);
        *entry = (*entry).max(at);
    }
    let mut entries: Vec<ProjectEntry> = refs
        .into_iter()
        .zip(spaces)
        .map(|(space, raw)| {
            let group = groups.remove(&space.id).unwrap_or_else(|| ProjectGroup {
                id: space.id.clone(),
                name: space.name.clone(),
                icon_path: None,
                added_at_unix_ms: 0,
                last_opened_at_unix_ms: 0,
                checkouts: Vec::new(),
            });
            let chats = chat_activity.get(space.id.as_str()).copied().unwrap_or(0);
            ProjectEntry {
                last_activity_ms: chats.max(group.last_opened_at_unix_ms),
                created_at_ms: raw.created_at.timestamp_millis(),
                space,
                group,
            }
        })
        .collect();
    entries.sort_by(|left, right| {
        right
            .last_activity_ms
            .cmp(&left.last_activity_ms)
            .then_with(|| left.space.name.cmp(&right.space.name))
            .then_with(|| left.space.id.cmp(&right.space.id))
    });
    (entries, pending)
}

/// This device's checkout history, decorated with the durable identity
/// (kind, branch, availability and the project link), plus that identity.
pub fn checkout_rows(
    client: &LocalWorkersClient,
) -> Result<(Vec<ProjectRow>, IdentityRegistry), WorkersError> {
    let rows = client.projects_with_ledger()?;
    let identity = client.project_identity_registry()?;
    Ok((
        project_ledger::decorate_with_identity(rows, &identity),
        identity,
    ))
}

/// The bootstrap's sessions plus every checkout's archived ones: the
/// bootstrap only carries a short preview of the archived. A checkout whose
/// archive cannot be read contributes none.
pub fn with_archived_sessions(
    client: &LocalWorkersClient,
    mut sessions: Vec<WorkersSession>,
    rows: &[ProjectRow],
) -> Vec<WorkersSession> {
    for project_id in rows.iter().filter_map(|row| row.project_id.as_deref()) {
        for session in client.archived_sessions(project_id).unwrap_or_default() {
            if !sessions.iter().any(|known| known.id == session.id) {
                sessions.push(session);
            }
        }
    }
    sessions
}

/// The project's Worker sessions with the checkout each ran in (principal or
/// worktree, live and archived), newest first. Worker sessions are
/// device-local, so a project of another device has none.
pub fn project_sessions<'a>(
    entry: &'a ProjectEntry,
    workers: &'a [WorkersSession],
) -> Vec<(&'a WorkersSession, &'a ProjectRow)> {
    if !entry.space.local {
        return Vec::new();
    }
    let mut sessions: Vec<(&WorkersSession, &ProjectRow)> = workers
        .iter()
        .filter_map(|session| {
            entry
                .group
                .checkouts
                .iter()
                .find(|row| {
                    row.project_id.as_deref() == Some(session.project_id.as_str())
                        || row.checkout_id.as_deref() == Some(session.project_id.as_str())
                })
                .map(|checkout| (session, checkout))
        })
        .collect();
    sessions.sort_by(|(left, _), (right, _)| {
        right
            .updated_at_unix_ms
            .cmp(&left.updated_at_unix_ms)
            .then_with(|| left.title.cmp(&right.title))
    });
    sessions
}

/// The agent a Worker runs: its provider, else its command's program name.
pub fn worker_provider(session: &WorkersSession) -> String {
    session
        .provider_id
        .clone()
        .or_else(|| {
            session
                .command
                .split_whitespace()
                .next()
                .map(|command| command.rsplit('/').next().unwrap_or(command).to_owned())
        })
        .unwrap_or_else(|| "worker".into())
}

/// Every chat of the project's Space, live and archived, newest first.
pub fn project_chats<'a>(entry: &ProjectEntry, chats: &'a [Chat]) -> Vec<&'a Chat> {
    let mut rows: Vec<&Chat> = chats
        .iter()
        .filter(|chat| chat.space_id.as_deref() == Some(entry.space.id.as_str()))
        .collect();
    rows.sort_by(|left, right| {
        chat_last_activity_ms(right)
            .cmp(&chat_last_activity_ms(left))
            .then_with(|| chat_title(left).cmp(&chat_title(right)))
    });
    rows
}

pub fn chat_title(chat: &Chat) -> String {
    chat.title
        .clone()
        .filter(|title| !title.trim().is_empty())
        .unwrap_or_else(|| "Untitled chat".into())
}

/// Last message, else creation, epoch ms.
pub fn chat_last_activity_ms(chat: &Chat) -> u64 {
    chat.last_message_at
        .unwrap_or(chat.created_at)
        .timestamp_millis()
        .max(0) as u64
}

/// Worker sessions the chat launched.
pub fn launched_workers(chat_id: &str, links: &[WorkerParentLink]) -> usize {
    links
        .iter()
        .filter(|link| link.parent_chat_id == chat_id)
        .count()
}
