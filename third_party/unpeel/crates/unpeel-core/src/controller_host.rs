//! Disk-backed Host adapter for transport-neutral Controller requests.
//!
//! The native app and the TUI can enrich the shared router with in-process UI
//! state. An on-demand SSH gateway has neither frontend, so this adapter builds
//! the authoritative subset from `~/.unpeel`: app state, session manifests,
//! markers, output logs, and control sockets. Platform-only capabilities such
//! as pairing and approval prompts are deliberately not advertised here.

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use base64::Engine;
use serde_json::{json, Value};

use crate::controller_api::{
    ControllerEffects, ControllerPrincipal, ControllerRequest, ControllerResponse,
    HostBootstrapContext, HostCreateContext, HostCreatePreset, HostCreateProject, HostRouteContext,
};
use crate::controller_protocol::HostProtocolDescriptor;
use crate::relay_crypto::TunnelRequest;
use crate::session_host::{self, HostedSessionManifest, HostedSessionState, SessionHostCommand};

const OUTPUT_WAIT_MAX_MS: u64 = 25_000;
const OUTPUT_WAIT_POLL_MS: u64 = 20;
/// Base64 and the JSON envelope must still fit the common Relay/SSH response
/// budget. Controllers can keep paging with `nextOffset`.
// Output bytes are base64 inside the route JSON, then that JSON is base64
// again inside the transport envelope. Keep enough room for both expansions
// plus envelope metadata under the shared 512 KiB plaintext ceiling.
const OUTPUT_MAX_BYTES: usize = 256 * 1024;
const MAX_SESSION_ID_BYTES: usize = 128;

#[derive(Clone)]
pub struct ControllerHostRuntime {
    principal: ControllerPrincipal,
    hook_port: Option<u16>,
}

impl ControllerHostRuntime {
    pub fn owner_transport(
        transport: impl Into<String>,
        subject: Option<String>,
        hook_port: Option<u16>,
    ) -> Self {
        Self {
            principal: ControllerPrincipal::OwnerTransport {
                transport: transport.into(),
                subject,
            },
            hook_port,
        }
    }

    /// Translate the common Relay/SSH wire request into a Host-authenticated
    /// semantic request. The wire cannot choose its own principal.
    pub fn handle_tunnel(
        &self,
        namespace: &str,
        request: TunnelRequest,
        cancelled: &AtomicBool,
    ) -> ControllerResponse {
        if !request.path.starts_with("/mobile/") || request.path == "/mobile/pair" {
            return response(request.id, 404, json!({ "error": "not found" }));
        }
        if !matches!(request.method.as_str(), "GET" | "POST") {
            return response(request.id, 405, json!({ "error": "method not allowed" }));
        }

        let (body, body_base64) = if request.body.is_empty() {
            (Value::Null, None)
        } else {
            match serde_json::from_slice(&request.body) {
                Ok(value) => (value, None),
                Err(_) => (
                    Value::Null,
                    Some(base64::engine::general_purpose::STANDARD.encode(&request.body)),
                ),
            }
        };
        let request_id = format!("{namespace}:{}", request.id);
        let semantic = ControllerRequest {
            id: Some(request_id),
            method: request.method,
            path: request.path,
            query: request.query.into_iter().collect(),
            body,
            content_type: request.content_type,
            body_base64,
            principal: self.principal.clone(),
        };
        self.handle(&semantic, cancelled)
    }

    fn handle(&self, request: &ControllerRequest, cancelled: &AtomicBool) -> ControllerResponse {
        let needs_catalog = matches!(
            (request.method.as_str(), request.path.as_str()),
            ("GET", "/mobile/bootstrap")
                | ("GET", "/mobile/archive")
                | ("POST", "/mobile/sessions")
                | ("POST", "/mobile/project-organization")
        );
        let catalog = if needs_catalog {
            match DiskCatalog::capture() {
                Ok(catalog) => Some(catalog),
                Err(message) => {
                    return ControllerResponse {
                        id: request.id.clone(),
                        status: 500,
                        body: json!({ "error": message }),
                    };
                }
            }
        } else {
            None
        };
        let route_context = catalog.as_ref().map(DiskCatalog::route_context);
        let create_context = catalog
            .as_ref()
            .map(|catalog| catalog.create_context(self.hook_port));
        let effects = ControllerEffects::new(Arc::new({
            let hook_port = self.hook_port;
            move |request| {
                crate::controller_api::execute_headless_session_action(request, hook_port)
            }
        }));

        if let Some(response) = crate::controller_api::route_with_effects(
            request,
            route_context.as_ref(),
            create_context.as_ref(),
            Some(&effects),
        ) {
            return response;
        }

        let (status, body) = match (request.method.as_str(), request.path.as_str()) {
            ("GET", "/mobile/output") => output(request, cancelled),
            ("POST", "/mobile/session-organization") => organization(request),
            ("POST", "/mobile/project-organization") => {
                let projects = catalog
                    .as_ref()
                    .and_then(|catalog| catalog.bootstrap.get("projects"))
                    .and_then(Value::as_array)
                    .cloned()
                    .unwrap_or_default();
                // No color writer: a bare disk gateway has no UserDefaults
                // access, so folder colors stay a frontend-adapter verb.
                project_organization_response(&request.body, &projects, None)
            }
            ("POST", "/mobile/resize-desktop") => resize_desktop(request),
            // Approval queues live inside the native app or TUI. This
            // disk-only adapter must not invent an empty queue or accept a
            // no-op answer.
            ("POST", "/mobile/approvals/answer") => (
                501,
                json!({ "error": "approval answers require a frontend Host adapter" }),
            ),
            _ => (404, json!({ "error": "not found" })),
        };
        ControllerResponse {
            id: request.id.clone(),
            status,
            body,
        }
    }
}

fn response(id: u64, status: u16, body: Value) -> ControllerResponse {
    ControllerResponse {
        id: Some(id.to_string()),
        status,
        body,
    }
}

#[derive(Clone)]
struct ProjectRecord {
    id: String,
    name: String,
    path: String,
    parent_id: Option<String>,
    sort_order: u64,
    is_folder: bool,
    worktree_branch: Option<String>,
}

struct DiskCatalog {
    bootstrap: Value,
    archives: HashMap<String, Vec<Value>>,
    projects: Vec<HostCreateProject>,
    presets: Vec<HostCreatePreset>,
}

impl DiskCatalog {
    fn capture() -> Result<Self, String> {
        let state = crate::app_state::load()?;
        let date_sorted_projects: HashSet<String> = state
            .get("session_sort_modes")
            .and_then(Value::as_object)
            .into_iter()
            .flat_map(|modes| modes.iter())
            .filter_map(|(id, mode)| (mode.as_str() == Some("date")).then_some(id.clone()))
            .collect();
        let mut projects = project_records(&state);
        projects.sort_by(|left, right| {
            left.sort_order
                .cmp(&right.sort_order)
                .then_with(|| left.id.cmp(&right.id))
        });
        // A drag persisted by ANY frontend lands in the shared
        // project-order.json and outranks the file's sort_order — the same
        // precedence every sidebar gives it. The bootstrap must advertise
        // the order the Host displays, or Controllers scramble it.
        let shared_order = crate::session_ops::project_order();
        if !shared_order.is_empty() {
            projects.sort_by_key(|record| {
                shared_order
                    .iter()
                    .position(|id| *id == record.id)
                    .unwrap_or(usize::MAX)
            });
        }
        let manifests = session_host::list_manifests();
        let pinned = pinned_session_ids(&state);
        let activity = activity_state();
        let known_ids: HashSet<String> =
            projects.iter().map(|project| project.id.clone()).collect();
        let folder_ids: HashSet<String> = projects
            .iter()
            .filter(|project| project.is_folder && project.parent_id.is_none())
            .map(|project| project.id.clone())
            .collect();

        let folders: Vec<Value> = projects
            .iter()
            .filter(|project| folder_ids.contains(&project.id))
            .map(|project| json!({ "id": project.id, "name": project.name }))
            .collect();
        let mut wire_projects = Vec::new();
        let mut create_projects = Vec::new();
        for (display_rank, project) in projects
            .iter()
            .filter(|project| !folder_ids.contains(&project.id))
            .enumerate()
        {
            let archived_count = manifests
                .iter()
                .filter(|manifest| {
                    effective_project_id(manifest, &known_ids) == project.id
                        && crate::session_ops::archived_marker(&manifest.session.id).is_some()
                })
                .count();
            // sortOrder is the DISPLAY rank (array order and field agree),
            // never the raw file value a shared-order drag may contradict.
            let mut value = json!({
                "id": project.id,
                "name": project.name,
                "path": project.path,
                "sortOrder": display_rank,
                "mcpBlocked": false,
                "archivedSessionCount": archived_count,
            });
            if let Some(object) = value.as_object_mut() {
                if let Some(parent) = project.parent_id.as_deref() {
                    if folder_ids.contains(parent) {
                        object.insert("folderID".into(), parent.into());
                    } else {
                        object.insert("parentProjectID".into(), parent.into());
                    }
                }
                if let Some(branch) = project.worktree_branch.as_deref() {
                    object.insert("worktreeBranch".into(), branch.into());
                }
                if project.is_folder && project.parent_id.is_some() {
                    object.insert("isGroup".into(), true.into());
                }
                if date_sorted_projects.contains(&project.id) {
                    object.insert("dateSorted".into(), true.into());
                }
            }
            wire_projects.push(value);
            create_projects.push(HostCreateProject {
                id: project.id.clone(),
                path: project.path.clone(),
                is_folder: project.is_folder && project.parent_id.is_some(),
                worktree_path: project
                    .worktree_branch
                    .as_ref()
                    .map(|_| project.path.clone()),
                worktree_branch: project.worktree_branch.clone(),
            });
        }

        let (wire_presets, create_presets) = presets(&state);
        let mut sessions = Vec::new();
        let mut archives: HashMap<String, Vec<Value>> = wire_projects
            .iter()
            .filter_map(|project| project.get("id")?.as_str().map(str::to_owned))
            .map(|id| (id, Vec::new()))
            .collect();
        let mut manifests = manifests;
        manifests.sort_by(|left, right| {
            right
                .session
                .created_at
                .cmp(&left.session.created_at)
                .then_with(|| left.session.id.cmp(&right.session.id))
        });
        for manifest in &manifests {
            let project_id = effective_project_id(manifest, &known_ids);
            let archived = crate::session_ops::archived_marker(&manifest.session.id).is_some();
            let summary = session_summary(
                manifest,
                &project_id,
                pinned.contains(&manifest.session.id),
                archived,
                activity.get(&manifest.session.id),
            );
            if archived {
                archives
                    .entry(project_id.clone())
                    .or_default()
                    .push(summary.clone());
            }
            // The bootstrap includes the same five-row stopped/archived
            // preview as the Host sidebar. Pins still win over archive.
            sessions.push(summary);
        }
        let session_orders: HashMap<String, Vec<String>> = wire_projects
            .iter()
            .filter_map(|project| project.get("id")?.as_str())
            .map(|id| (id.to_owned(), crate::session_ops::session_order(id)))
            .collect();
        sort_wire_sessions(&mut sessions, &wire_projects, &session_orders);
        retain_wire_sidebar_window(&mut sessions);

        Ok(Self {
            bootstrap: json!({
                "macName": hostname_short(),
                "folders": folders,
                "projects": wire_projects,
                "presets": wire_presets,
                "sessions": sessions,
            }),
            archives,
            projects: create_projects,
            presets: create_presets,
        })
    }

    fn route_context(&self) -> HostRouteContext {
        let mut bootstrap = HostBootstrapContext::headless(self.bootstrap.clone());
        bootstrap.host_id = std::fs::read_to_string(
            crate::app_paths::unpeel_home()
                .join("mobile")
                .join("mac-id"),
        )
        .ok()
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty());
        // These require a frontend-owned in-memory service and are not
        // available merely because the disk-backed gateway is running.
        bootstrap.protocol = disk_protocol();
        HostRouteContext {
            bootstrap: Some(bootstrap),
            archived_sessions_by_project: self.archives.clone(),
        }
    }

    fn create_context(&self, hook_port: Option<u16>) -> HostCreateContext {
        HostCreateContext::new(
            self.projects.clone(),
            self.presets.clone(),
            Arc::new(move |request| {
                crate::controller_api::execute_headless_session_create(request, hook_port)
            }),
        )
    }
}

/// Shared Host semantics for `POST /mobile/project-organization`
/// (`project.organization.set`) over a wire-project catalog: rename a group,
/// set a main project's folder color (only when the adapter can persist one —
/// `color_writer` is `None` on a bare disk gateway), flip a group's session
/// sort, and `sortOrder` — move the project to that index among its
/// same-parent siblings in the advertised display order. Persistence goes
/// through the shared choke points (`app_state::edit`,
/// `session_ops::set_project_sibling_order` — flock + state-bus announce).
/// Every field is type-checked and every unsupported field rejected before
/// anything applies, so a compound patch can never half-apply behind a
/// 400/404/501. Used by the TUI's `/mobile` server and the disk gateway so a
/// Controller sees one behavior whichever transport carried the patch.
#[allow(clippy::type_complexity)]
pub fn project_organization_response(
    body: &Value,
    wire_projects: &[Value],
    color_writer: Option<&dyn Fn(&str, Option<&str>) -> Result<(), String>>,
) -> (u16, Value) {
    let error = |message: &str| json!({ "error": message });
    let Some(project_id) = body
        .get("projectID")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return (400, error("invalid project id"));
    };
    let display_name = match body.get("displayName") {
        None | Some(Value::Null) => None,
        Some(Value::String(value)) => {
            let value = value.trim();
            (!value.is_empty()).then(|| value.to_owned())
        }
        Some(_) => return (400, error("displayName must be a string")),
    };
    let color_id = match body.get("colorID") {
        None | Some(Value::Null) => None,
        Some(Value::String(value)) => Some(value.clone()),
        Some(_) => return (400, error("colorID must be a string")),
    };
    let date_sorted = match body.get("dateSorted") {
        None | Some(Value::Null) => None,
        Some(Value::Bool(value)) => Some(*value),
        Some(_) => return (400, error("dateSorted must be a boolean")),
    };
    let sort_order = match body.get("sortOrder") {
        None | Some(Value::Null) => None,
        Some(value) => match value.as_i64() {
            Some(index) if index >= 0 => Some(index as usize),
            _ => return (400, error("sortOrder must be a non-negative integer")),
        },
    };
    let folder_move_requested = match body.get("folderID") {
        None | Some(Value::Null) => false,
        Some(Value::String(_)) => true,
        Some(_) => return (400, error("folderID must be a string")),
    };

    let Some(target) = wire_projects
        .iter()
        .find(|project| project.get("id").and_then(Value::as_str) == Some(project_id))
    else {
        return (404, error("unknown project"));
    };
    // Unsupported operations are rejected after the project resolves and
    // before anything applies (native resource ordering).
    if folder_move_requested {
        return (
            501,
            error("moving a project between folders is not supported by this Host"),
        );
    }
    let parent_id = target.get("parentProjectID").and_then(Value::as_str);
    let is_group = target
        .get("isGroup")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    if display_name.is_some() && !is_group {
        return (400, error("Only groups can be renamed remotely"));
    }
    if let Some(color) = color_id.as_deref() {
        // Folder color is a MAIN-project verb — groups and worktrees stay
        // neutral (same rule as the desktop and TUI menus).
        if parent_id.is_some() {
            return (400, error("Only main projects can be colored"));
        }
        const FOLDER_COLORS: [&str; 8] = [
            "sky", "blue", "violet", "rose", "amber", "moss", "teal", "graphite",
        ];
        if !color.is_empty() && !FOLDER_COLORS.contains(&color) {
            return (400, error(&format!("Unknown folder color: {color}")));
        }
        if color_writer.is_none() {
            return (501, error("folder colors are not supported by this Host"));
        }
    }
    if display_name.is_none() && color_id.is_none() && date_sorted.is_none() && sort_order.is_none()
    {
        // Match the native DTO: an empty patch, explicit nulls, and a name
        // that trims to empty are successful no-ops.
        return (200, json!({ "ok": true }));
    }

    // Apply. Once any field lands, a later failure is effect-unknown;
    // Controllers must refresh Host state before deciding whether to retry.
    // The group/non-group split was already answered from the wire catalog
    // above, so a failure here is broken shared state or IO, not validation.
    if let Some(name) = display_name {
        if let Err(e) = crate::session_ops::rename_group_project(project_id, &name) {
            return (
                500,
                error(&format!("organization rename preflight failed: {e}")),
            );
        }
    }
    if let (Some(color), Some(write_color)) = (color_id.as_deref(), color_writer) {
        let color = (!color.is_empty()).then_some(color);
        if let Err(e) = write_color(project_id, color) {
            return (
                500,
                error(&format!(
                    "organization update effect unknown; refresh Host state: {e}"
                )),
            );
        }
    }
    if let Some(date_sorted) = date_sorted {
        if let Err(e) = crate::session_ops::set_session_date_sorted(project_id, date_sorted) {
            return (
                500,
                error(&format!(
                    "organization update effect unknown; refresh Host state: {e}"
                )),
            );
        }
    }
    if let Some(index) = sort_order {
        let ids_where = |filter: &dyn Fn(&Value) -> bool| -> Vec<String> {
            wire_projects
                .iter()
                .filter(|project| filter(project))
                .filter_map(|project| project.get("id").and_then(Value::as_str).map(str::to_owned))
                .collect()
        };
        let sibling_ids = ids_where(&|project| {
            project.get("parentProjectID").and_then(Value::as_str) == parent_id
        });
        let mut ordered = sibling_ids.clone();
        if let Some(from) = ordered.iter().position(|id| id == project_id) {
            let id = ordered.remove(from);
            let to = index.min(ordered.len());
            ordered.insert(to, id);
        }
        if ordered != sibling_ids {
            let all_ids = ids_where(&|_| true);
            if let Err(e) = crate::session_ops::set_project_sibling_order(&ordered, &all_ids) {
                return (
                    500,
                    error(&format!(
                        "organization update effect unknown; refresh Host state: {e}"
                    )),
                );
            }
        }
    }
    (200, json!({ "ok": true }))
}

fn disk_protocol() -> HostProtocolDescriptor {
    let mut protocol = HostProtocolDescriptor::headless_v1();
    protocol.capabilities.retain(|capability| {
        !matches!(
            capability.as_str(),
            "approval.answer" | "approval.list" | "pairing.create" | "session.output.subscribe"
        )
    });
    protocol
}

fn project_records(state: &Value) -> Vec<ProjectRecord> {
    state
        .get("projects")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|value| {
            Some(ProjectRecord {
                id: string_field(value, &["id"])?.to_owned(),
                name: string_field(value, &["name"])
                    .unwrap_or("Project")
                    .to_owned(),
                path: string_field(value, &["path"])
                    .unwrap_or_default()
                    .to_owned(),
                parent_id: string_field(value, &["parent_project_id", "parentProjectID"])
                    .map(str::to_owned),
                sort_order: integer_field(value, &["sort_order", "sortOrder"]).unwrap_or(0),
                is_folder: bool_field(value, &["is_folder", "isFolder"]).unwrap_or(false),
                worktree_branch: string_field(value, &["worktree_branch", "worktreeBranch"])
                    .map(str::to_owned),
            })
        })
        .collect()
}

fn presets(state: &Value) -> (Vec<Value>, Vec<HostCreatePreset>) {
    let mut wire = Vec::new();
    let mut create = Vec::new();
    for value in state
        .get("presets")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let Some(id) = string_field(value, &["id"]) else {
            continue;
        };
        let Some(command) = string_field(value, &["command"]) else {
            continue;
        };
        let enabled = bool_field(value, &["enabled"]).unwrap_or(true);
        if !enabled {
            continue;
        }
        let label = string_field(value, &["label"]).unwrap_or(command);
        let project_id = string_field(value, &["project_id", "projectID"]).map(str::to_owned);
        wire.push(json!({
            "id": id,
            "label": label,
            "command": command,
            "enabled": true,
            "quickLaunch": bool_field(value, &["quick_launch", "quickLaunch"])
                .unwrap_or(false),
            "isDefault": false,
        }));
        create.push(HostCreatePreset {
            id: id.to_owned(),
            command: command.to_owned(),
            enabled: true,
            project_id,
        });
    }
    (wire, create)
}

fn pinned_session_ids(state: &Value) -> HashSet<String> {
    state
        .get("pinned_sessions")
        .or_else(|| state.get("pinnedSessions"))
        .and_then(Value::as_object)
        .into_iter()
        .flat_map(|projects| projects.values())
        .filter_map(Value::as_array)
        .flatten()
        .filter_map(|entry| match entry {
            Value::String(id) => Some(id.clone()),
            Value::Object(object) => object
                .get("session_id")
                .or_else(|| object.get("sessionID"))
                .and_then(Value::as_str)
                .map(str::to_owned)
                .or_else(|| {
                    object
                        .get("key")
                        .and_then(Value::as_str)
                        .and_then(|key| key.strip_prefix("session:"))
                        .map(str::to_owned)
                }),
            _ => None,
        })
        .collect()
}

fn activity_state() -> HashMap<String, Value> {
    std::fs::read(crate::app_paths::activity_state_path())
        .ok()
        .and_then(|raw| serde_json::from_slice::<Value>(&raw).ok())
        .and_then(|value| value.get("sessions").and_then(Value::as_object).cloned())
        .map(|object| object.into_iter().collect())
        .unwrap_or_default()
}

fn effective_project_id(manifest: &HostedSessionManifest, known: &HashSet<String>) -> String {
    crate::session_ops::project_override_marker(&manifest.session.id)
        .filter(|project_id| known.contains(project_id))
        .unwrap_or_else(|| manifest.session.project_id.clone())
}

fn session_summary(
    manifest: &HostedSessionManifest,
    project_id: &str,
    pinned: bool,
    archived: bool,
    activity: Option<&Value>,
) -> Value {
    let running = manifest.state == HostedSessionState::Running;
    let updated_at = crate::session_ops::latest_lifecycle_ms(
        &manifest.session.id,
        &manifest.session.command,
        manifest.session.created_at,
        (!running).then_some(manifest.updated_at),
    )
    .max(crate::session_ops::archive_stamp(&manifest.session.id).unwrap_or(0));
    let claimed_unread = activity
        .and_then(|value| value.get("unread"))
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let head = crate::integrations::command_head(&manifest.session.command);
    let unread = claimed_unread
        && match crate::session_ops::read_marker(&manifest.session.id) {
            Some(read_at) => crate::session_ops::last_activity_ms(
                &manifest.session.id,
                &manifest.session.command,
            )
            .is_some_and(|settled_at| settled_at > read_at),
            None => true,
        };
    let persisted_activity = activity
        .and_then(|value| value.get("activity_status"))
        .or_else(|| activity.and_then(|value| value.get("activityStatus")))
        .and_then(Value::as_str);
    let activity_name = if !running {
        if unread {
            "done"
        } else {
            "idle"
        }
    } else if manifest.menu_prompt_active {
        "blocked"
    } else {
        match persisted_activity {
            Some("starting") => "starting",
            Some("working") => "working",
            Some("blocked") => "blocked",
            _ if unread => "done",
            _ => "idle",
        }
    };
    let resumable = manifest.session.command.trim().is_empty()
        || crate::resume::can_resume(&manifest.session.command);
    let active_runtime_id = running
        .then(|| session_host::active_runtime_id(manifest))
        .flatten();
    let resume_agent = running
        && !manifest.runtime_launch_pending
        && manifest.host_protocol_version.unwrap_or(0)
            >= session_host::SESSION_HOST_RESUME_AGENT_PROTOCOL_VERSION
        && crate::resume::can_resume_agent(&manifest.session.command, active_runtime_id);
    let mut value = json!({
        "id": manifest.session.id,
        "projectID": project_id,
        "title": crate::session_ops::title_marker(&manifest.session.id)
            .unwrap_or_else(|| manifest.session.label.clone()),
        "command": manifest.session.command,
        "createdAtUnixMs": manifest.session.created_at,
        "updatedAtUnixMs": updated_at,
        "status": if running { "running" } else { "exited" },
        "activity": activity_name,
        "unread": unread,
        "pinned": pinned,
        "notifyWhenDone": false,
        "runtimeLaunchPending": manifest.runtime_launch_pending,
        "capabilities": {
            // `restart` is the legacy terminal-replacing Resume operation.
            // A live Session offers shell-only Resume Agent after its managed
            // runtime has exited; active/passively observed jobs offer neither.
            "restart": !running && resumable,
            "resumeAgent": resume_agent,
            "fork": crate::resume::can_fork(&manifest.session.command),
            "appendSystemContext": crate::provider_context::supports(&manifest.session.command),
            "notifyWhenDone": false,
            "archive": resumable,
        },
        "archived": archived,
    });
    // A retained final observation is useful in Host diagnostics, but it is
    // never advertised as the currently active runtime after the PTY exits.
    if let Some(runtime_id) = active_runtime_id {
        value["activeRuntimeID"] = runtime_id.into();
    }
    if let Some(provider) = provider_id(head) {
        value["providerID"] = provider.into();
    }
    value
}

/// Put each Host project's wire sessions in the order its sidebar advertises.
/// Pins remain their explicit first section. Live rows come next (Recent
/// lifecycle order in date mode, manual order in custom mode), followed by
/// stopped/archived rows newest-first. Grouping the flat wire array by project
/// is harmless to clients and makes its filtered order unambiguous.
fn sort_wire_sessions(
    sessions: &mut [Value],
    projects: &[Value],
    session_orders: &HashMap<String, Vec<String>>,
) {
    let project_rank: HashMap<&str, usize> = projects
        .iter()
        .enumerate()
        .filter_map(|(rank, project)| Some((project.get("id")?.as_str()?, rank)))
        .collect();
    let date_sorted: HashSet<&str> = projects
        .iter()
        .filter(|project| {
            project
                .get("dateSorted")
                .and_then(Value::as_bool)
                .unwrap_or(false)
        })
        .filter_map(|project| project.get("id")?.as_str())
        .collect();
    fn string<'a>(value: &'a Value, field: &str) -> &'a str {
        value.get(field).and_then(Value::as_str).unwrap_or_default()
    }
    fn number(value: &Value, field: &str) -> u64 {
        value.get(field).and_then(Value::as_u64).unwrap_or(0)
    }
    sessions.sort_by(|left, right| {
        let left_project = string(left, "projectID");
        let right_project = string(right, "projectID");
        project_rank
            .get(left_project)
            .copied()
            .unwrap_or(usize::MAX)
            .cmp(
                &project_rank
                    .get(right_project)
                    .copied()
                    .unwrap_or(usize::MAX),
            )
            .then_with(|| left_project.cmp(right_project))
            .then_with(|| {
                let left_pinned = left.get("pinned").and_then(Value::as_bool).unwrap_or(false);
                let right_pinned = right
                    .get("pinned")
                    .and_then(Value::as_bool)
                    .unwrap_or(false);
                let pinned_order = right_pinned.cmp(&left_pinned);
                if pinned_order != std::cmp::Ordering::Equal {
                    return pinned_order;
                }
                let order = session_orders.get(left_project);
                let manual_key = |value: &Value| match order
                    .and_then(|ids| ids.iter().position(|id| id == string(value, "id")))
                {
                    Some(rank) => (1, rank, std::cmp::Reverse(number(value, "createdAtUnixMs"))),
                    None => (0, 0, std::cmp::Reverse(number(value, "createdAtUnixMs"))),
                };
                if left_pinned {
                    return manual_key(left)
                        .cmp(&manual_key(right))
                        .then_with(|| string(left, "id").cmp(string(right, "id")));
                }

                let running = |value: &Value| string(value, "status") == "running";
                let left_running = running(left);
                let right_running = running(right);
                if left_running != right_running {
                    return right_running.cmp(&left_running);
                }
                if !left_running {
                    return number(right, "updatedAtUnixMs")
                        .cmp(&number(left, "updatedAtUnixMs"))
                        .then_with(|| string(left, "id").cmp(string(right, "id")));
                }
                if !date_sorted.contains(left_project) {
                    return manual_key(left)
                        .cmp(&manual_key(right))
                        .then_with(|| string(left, "id").cmp(string(right, "id")));
                }
                let working =
                    |value: &Value| matches!(string(value, "activity"), "starting" | "working");
                working(right)
                    .cmp(&working(left))
                    .then_with(|| {
                        number(right, "updatedAtUnixMs").cmp(&number(left, "updatedAtUnixMs"))
                    })
                    .then_with(|| string(left, "id").cmp(string(right, "id")))
            })
    });
}

/// Keep every pin and live row, then at most five stopped/archived preview
/// rows per project. Unread stopped rows stay visible past that window,
/// matching the native and TUI keep-visible contract. `sort_wire_sessions`
/// has already made the first five the newest lifecycle events.
fn retain_wire_sidebar_window(sessions: &mut Vec<Value>) {
    const STOPPED_WINDOW: usize = 5;
    let mut stopped_by_project: HashMap<String, usize> = HashMap::new();
    sessions.retain(|session| {
        let pinned = session
            .get("pinned")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let running = session.get("status").and_then(Value::as_str) == Some("running");
        let unread = session
            .get("unread")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        if pinned || running {
            return true;
        }
        let project = session
            .get("projectID")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned();
        let count = stopped_by_project.entry(project).or_default();
        let keep = *count < STOPPED_WINDOW || unread;
        *count += 1;
        keep
    });
}

fn provider_id(head: &str) -> Option<&'static str> {
    crate::runtime_catalog::builtin_runtime_catalog()
        .by_command_alias_for_current_platform(head)
        .map(|runtime| runtime.legacy_slug.as_str())
}

fn string_field<'a>(value: &'a Value, names: &[&str]) -> Option<&'a str> {
    names
        .iter()
        .find_map(|name| value.get(*name).and_then(Value::as_str))
}

fn integer_field(value: &Value, names: &[&str]) -> Option<u64> {
    names
        .iter()
        .find_map(|name| value.get(*name).and_then(Value::as_u64))
}

fn bool_field(value: &Value, names: &[&str]) -> Option<bool> {
    names
        .iter()
        .find_map(|name| value.get(*name).and_then(Value::as_bool))
}

fn hostname_short() -> String {
    let mut buffer = [0u8; 256];
    let rc = unsafe { libc::gethostname(buffer.as_mut_ptr().cast(), buffer.len()) };
    if rc != 0 {
        return "Host".into();
    }
    let name = buffer.split(|byte| *byte == 0).next().unwrap_or_default();
    String::from_utf8_lossy(name)
        .trim_end_matches(".local")
        .to_owned()
}

fn query_session_id(request: &ControllerRequest) -> Option<&str> {
    request
        .query
        .get("session_id")
        .or_else(|| request.query.get("sessionID"))
        .map(String::as_str)
        .filter(|value| safe_session_id(value))
}

fn body_session_id(request: &ControllerRequest) -> Option<&str> {
    request
        .body
        .get("sessionID")
        .or_else(|| request.body.get("session_id"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| safe_session_id(value))
}

fn safe_session_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_SESSION_ID_BYTES
        && !value.contains('/')
        && !value.contains('\\')
        && !value.contains("..")
}

fn output(request: &ControllerRequest, cancelled: &AtomicBool) -> (u16, Value) {
    let Some(session_id) = query_session_id(request) else {
        return (400, json!({ "error": "invalid session id" }));
    };
    let offset = request
        .query
        .get("offset")
        .and_then(|value| value.parse::<u64>().ok());
    let limit = request
        .query
        .get("limit")
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(OUTPUT_MAX_BYTES)
        .clamp(1, OUTPUT_MAX_BYTES);
    let wait_ms = request
        .query
        .get("wait_ms")
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(0)
        .min(OUTPUT_WAIT_MAX_MS);
    if let Some(offset) = offset {
        let deadline = Instant::now() + Duration::from_millis(wait_ms);
        while wait_ms > 0
            && !cancelled.load(Ordering::Relaxed)
            && Instant::now() < deadline
            && std::fs::metadata(session_host::output_path(session_id))
                .map(|metadata| metadata.len())
                .unwrap_or(0)
                == offset
        {
            std::thread::sleep(Duration::from_millis(OUTPUT_WAIT_POLL_MS));
        }
    }
    let chunk = match session_host::read_output_chunk(session_id, offset, Some(limit), Some(limit))
    {
        Ok(chunk) => chunk,
        Err(message) => return (500, json!({ "error": message })),
    };
    let start = chunk.next_offset.saturating_sub(chunk.data.len() as u64);
    (
        200,
        json!({
            "sessionID": session_id,
            "offset": start,
            "nextOffset": chunk.next_offset,
            "dataBase64": base64::engine::general_purpose::STANDARD.encode(chunk.data),
            "truncated": offset.map_or(start > 0, |requested| requested != start),
            "capturedAtUnixMs": crate::state::current_timestamp_ms(),
        }),
    )
}

fn organization(request: &ControllerRequest) -> (u16, Value) {
    let Some(session_id) = body_session_id(request) else {
        return (400, json!({ "error": "invalid session id" }));
    };
    if session_host::load_manifest(session_id).is_none() {
        return (404, json!({ "error": "unknown session" }));
    }
    let title = match request.body.get("title") {
        None | Some(Value::Null) => None,
        Some(Value::String(value)) => {
            let value = value.trim();
            (!value.is_empty()).then_some(value)
        }
        Some(_) => return (400, json!({ "error": "title must be a string" })),
    };
    let pinned = match request.body.get("pinned") {
        None | Some(Value::Null) => None,
        Some(Value::Bool(value)) => Some(*value),
        Some(_) => return (400, json!({ "error": "pinned must be a boolean" })),
    };
    let archived = match request.body.get("archived") {
        None | Some(Value::Null) => None,
        Some(Value::Bool(value)) => Some(*value),
        Some(_) => return (400, json!({ "error": "archived must be a boolean" })),
    };
    if request
        .body
        .get("notifyWhenDone")
        .is_some_and(|value| !value.is_null())
    {
        if !request.body["notifyWhenDone"].is_boolean() {
            return (400, json!({ "error": "notifyWhenDone must be a boolean" }));
        }
        return (
            501,
            json!({ "error": "notifyWhenDone is not supported by this Host" }),
        );
    }
    if let Some(value) = pinned {
        if let Err(message) = crate::session_ops::set_pinned(session_id, value) {
            return (500, json!({ "error": message }));
        }
    }
    if let Some(value) = title {
        if let Err(message) = crate::session_ops::set_title(session_id, value) {
            return (500, json!({ "error": message }));
        }
    }
    let result = match archived {
        Some(true) => crate::session_ops::archive_session(session_id),
        Some(false) => crate::session_ops::restore_session(session_id),
        None => Ok(()),
    };
    match result {
        Ok(()) => (200, json!({ "ok": true })),
        Err(message) => (500, json!({ "error": message })),
    }
}

fn resize_desktop(request: &ControllerRequest) -> (u16, Value) {
    let Some(session_id) = body_session_id(request) else {
        return (400, json!({ "error": "invalid session id" }));
    };
    if request
        .body
        .get("clear")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        return (200, json!({ "ok": true }));
    }
    let cols = request
        .body
        .get("columns")
        .and_then(Value::as_u64)
        .unwrap_or(0)
        .clamp(2, 300) as u16;
    let rows = request
        .body
        .get("rows")
        .and_then(Value::as_u64)
        .unwrap_or(0)
        .clamp(2, 120) as u16;
    match session_host::send_command(session_id, &SessionHostCommand::Resize { cols, rows }) {
        Ok(()) => (200, json!({ "ok": true })),
        Err(_) => (404, json!({ "error": "session host unavailable" })),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manifest_with_runtime(state: HostedSessionState, command: &str) -> HostedSessionManifest {
        HostedSessionManifest {
            session: crate::state::SessionInfo {
                id: "__active_runtime_wire_test__".into(),
                project_id: "project-1".into(),
                label: "Shell".into(),
                custom_title: false,
                command: command.into(),
                created_at: 1,
                tag_id: None,
                worktree_path: None,
                worktree_branch: None,
                parent_session_id: None,
                spawned_by: None,
                role: None,
                task: None,
            },
            cwd: "/tmp".into(),
            state,
            pid: None,
            pid_started_at: None,
            exit_code: None,
            host_build_id: None,
            host_protocol_version: Some(session_host::SESSION_HOST_RESUME_AGENT_PROTOCOL_VERSION),
            has_been_written_to: true,
            provider_session_id: None,
            provider_transcript_path: None,
            managed_storage_path: None,
            resume_failure_markers: Vec::new(),
            runtime: Some(crate::session_host::HostedSessionRuntime {
                current_observation: Some(crate::runtime_observer::ActiveRuntimeObservation {
                    runtime_id: "claude".into(),
                    pid: 42,
                    pid_started_at: Some(1),
                    process_group_id: 42,
                    process_name: "claude".into(),
                    argv: Some(vec!["claude".into()]),
                }),
            }),
            runtime_launch_generation: 1,
            runtime_launch_pending: false,
            runtime_launched_at: Some(1),
            runtime_launch_output_offset: 0,
            mcp_enabled: None,
            browser_mcp_enabled: None,
            computer_mcp_enabled: None,
            mcp_client_registered: false,
            browser_client_registered: false,
            computer_client_registered: false,
            menu_prompt_active: false,
            screen_changed_at: None,
            detected_local_urls: Vec::new(),
            heartbeat_at: 1,
            updated_at: 1,
        }
    }

    #[test]
    fn disk_protocol_does_not_claim_in_process_services() {
        let protocol = disk_protocol();
        assert!(!protocol.supports("approval.answer"));
        assert!(!protocol.supports("approval.list"));
        assert!(!protocol.supports("pairing.create"));
        assert!(!protocol.supports("session.output.subscribe"));
        assert!(protocol.supports("host.bootstrap"));
        assert!(protocol.supports("session.create"));
        assert!(protocol.supports("session.output.read"));
    }

    #[test]
    fn session_summary_advertises_live_runtime_without_rewriting_launch_provider() {
        let manifest = manifest_with_runtime(HostedSessionState::Running, "codex");
        let summary = session_summary(&manifest, "project-1", false, false, None);
        assert_eq!(summary["activeRuntimeID"], "claude");
        assert_eq!(summary["providerID"], "codex");
        assert!(summary["capabilities"].get("restartAgent").is_none());
        assert_eq!(summary["capabilities"]["resumeAgent"], false);
        assert_eq!(summary["capabilities"]["restart"], false);

        let managed = manifest_with_runtime(HostedSessionState::Running, "claude");
        let summary = session_summary(&managed, "project-1", false, false, None);
        assert!(summary["capabilities"].get("restartAgent").is_none());
        assert_eq!(summary["capabilities"]["resumeAgent"], false);
        assert_eq!(summary["capabilities"]["restart"], false);

        let mut returned_to_shell = managed.clone();
        returned_to_shell.runtime = None;
        let summary = session_summary(&returned_to_shell, "project-1", false, false, None);
        assert_eq!(summary["capabilities"]["resumeAgent"], true);
        assert_eq!(summary["runtimeLaunchPending"], false);

        let mut launch_pending = returned_to_shell.clone();
        launch_pending.runtime_launch_pending = true;
        let summary = session_summary(&launch_pending, "project-1", false, false, None);
        assert_eq!(summary["runtimeLaunchPending"], true);
        assert_eq!(summary["capabilities"]["resumeAgent"], false);

        let mut old_host = managed.clone();
        old_host.host_protocol_version =
            Some(session_host::SESSION_HOST_RESTART_AGENT_PROTOCOL_VERSION);
        old_host.runtime = None;
        let summary = session_summary(&old_host, "project-1", false, false, None);
        assert!(summary["capabilities"].get("restartAgent").is_none());
        assert_eq!(summary["capabilities"]["resumeAgent"], false);
        assert_eq!(summary["capabilities"]["restart"], false);

        let blank = manifest_with_runtime(HostedSessionState::Running, "");
        let summary = session_summary(&blank, "project-1", false, false, None);
        assert_eq!(summary["activeRuntimeID"], "claude");
        assert!(summary.get("providerID").is_none());
        assert!(summary["capabilities"].get("restartAgent").is_none());
        assert_eq!(summary["capabilities"]["resumeAgent"], false);
        assert_eq!(summary["capabilities"]["restart"], false);
    }

    #[test]
    fn session_summary_does_not_advertise_an_exited_runtime_observation() {
        let manifest = manifest_with_runtime(HostedSessionState::Exited, "");
        let summary = session_summary(&manifest, "project-1", false, false, None);
        assert!(summary.get("activeRuntimeID").is_none());
        assert!(summary["capabilities"].get("restartAgent").is_none());
        assert_eq!(summary["capabilities"]["resumeAgent"], false);
        assert_eq!(summary["capabilities"]["restart"], true);
    }

    #[test]
    fn wire_date_sort_uses_working_then_lifecycle_while_custom_stays_manual() {
        let projects = vec![
            json!({ "id": "recent", "dateSorted": true }),
            json!({ "id": "custom" }),
        ];
        let session = |id: &str,
                       project: &str,
                       pinned: bool,
                       status: &str,
                       activity: &str,
                       created: u64,
                       updated: u64| {
            json!({
                "id": id,
                "projectID": project,
                "pinned": pinned,
                "status": status,
                "activity": activity,
                "createdAtUnixMs": created,
                "updatedAtUnixMs": updated,
            })
        };
        let mut sessions = vec![
            session(
                "recent-idle-live",
                "recent",
                false,
                "running",
                "idle",
                1,
                20,
            ),
            session(
                "custom-working-old",
                "custom",
                false,
                "running",
                "working",
                1,
                90,
            ),
            session("recent-exited", "recent", false, "exited", "idle", 1, 90),
            session("recent-z", "recent", false, "exited", "idle", 1, 30),
            session("recent-busy", "recent", false, "running", "working", 1, 10),
            session("custom-new", "custom", false, "running", "idle", 100, 100),
            session("recent-a", "recent", false, "exited", "idle", 1, 30),
            session("recent-pinned", "recent", true, "exited", "idle", 1, 999),
        ];

        let orders = HashMap::from([(
            "custom".to_owned(),
            vec!["custom-working-old".to_owned(), "custom-new".to_owned()],
        )]);
        sort_wire_sessions(&mut sessions, &projects, &orders);
        let ids: Vec<&str> = sessions
            .iter()
            .filter_map(|session| session.get("id")?.as_str())
            .collect();
        assert_eq!(
            ids,
            [
                "recent-pinned",
                "recent-busy",
                "recent-idle-live",
                "recent-exited",
                "recent-a",
                "recent-z",
                "custom-working-old",
                "custom-new",
            ]
        );
    }

    #[test]
    fn wire_sidebar_keeps_only_five_stopped_rows_per_project() {
        let mut sessions = vec![json!({
            "id": "live",
            "projectID": "project",
            "status": "running",
            "pinned": false,
            "updatedAtUnixMs": 1,
        })];
        for updated in 10..17 {
            sessions.push(json!({
                "id": format!("archived-{updated}"),
                "projectID": "project",
                "status": "exited",
                "archived": true,
                "pinned": false,
                "updatedAtUnixMs": updated,
            }));
        }
        sessions.push(json!({
            "id": "unread-old",
            "projectID": "project",
            "status": "exited",
            "archived": true,
            "unread": true,
            "pinned": false,
            "updatedAtUnixMs": 2,
        }));
        sort_wire_sessions(
            &mut sessions,
            &[json!({ "id": "project", "dateSorted": true })],
            &HashMap::new(),
        );
        retain_wire_sidebar_window(&mut sessions);

        assert_eq!(
            sessions
                .iter()
                .filter_map(|session| session.get("id")?.as_str())
                .collect::<Vec<_>>(),
            [
                "live",
                "archived-16",
                "archived-15",
                "archived-14",
                "archived-13",
                "archived-12",
                "unread-old",
            ]
        );

        let mut newest_unread = vec![json!({
            "id": "unread-newest",
            "projectID": "project",
            "status": "exited",
            "unread": true,
            "pinned": false,
            "updatedAtUnixMs": 100,
        })];
        for updated in 10..15 {
            newest_unread.push(json!({
                "id": format!("read-{updated}"),
                "projectID": "project",
                "status": "exited",
                "unread": false,
                "pinned": false,
                "updatedAtUnixMs": updated,
            }));
        }
        sort_wire_sessions(
            &mut newest_unread,
            &[json!({ "id": "project", "dateSorted": true })],
            &HashMap::new(),
        );
        retain_wire_sidebar_window(&mut newest_unread);
        assert_eq!(
            newest_unread.len(),
            5,
            "unread rows within the window count toward it"
        );
    }

    /// Validation and no-op paths only — nothing here may touch shared
    /// files, so the fixture stays safe in a parallel test run. The real
    /// write path is proven end to end by the TUI PTY suite (`remote_host`).
    #[test]
    fn project_organization_validates_before_any_shared_write() {
        let projects = vec![
            json!({ "id": "p1", "name": "One", "path": "/tmp/one", "sortOrder": 0 }),
            json!({
                "id": "g1", "name": "Backlog", "path": "/tmp/one",
                "parentProjectID": "p1", "isGroup": true, "sortOrder": 1
            }),
        ];
        let case = |body: Value| project_organization_response(&body, &projects, None).0;

        assert_eq!(case(json!({ "sortOrder": 0 })), 400, "missing project id");
        assert_eq!(
            case(json!({ "projectID": "nope", "sortOrder": 0 })),
            404,
            "unknown project"
        );
        assert_eq!(
            case(json!({ "projectID": "p1" })),
            200,
            "empty patch is a successful no-op"
        );
        assert_eq!(
            case(json!({ "projectID": "p1", "sortOrder": "first" })),
            400,
            "malformed sortOrder"
        );
        assert_eq!(
            case(json!({ "projectID": "p1", "sortOrder": -1 })),
            400,
            "negative sortOrder"
        );
        assert_eq!(
            case(json!({ "projectID": "p1", "sortOrder": 0 })),
            200,
            "single-sibling move to its own slot is a no-op (no write)"
        );
        assert_eq!(
            case(json!({ "projectID": "p1", "folderID": "f1" })),
            501,
            "legacy folder moves are rejected, never silently ignored"
        );
        assert_eq!(
            case(json!({ "projectID": "p1", "displayName": "New name" })),
            400,
            "only groups rename remotely"
        );
        assert_eq!(
            case(json!({ "projectID": "g1", "colorID": "sky" })),
            400,
            "colors are a main-project verb"
        );
        assert_eq!(
            case(json!({ "projectID": "p1", "colorID": "plaid" })),
            400,
            "unknown color id"
        );
        assert_eq!(
            case(json!({ "projectID": "p1", "colorID": "sky" })),
            501,
            "no color writer means colors are honestly unsupported"
        );
        assert_eq!(
            case(json!({ "projectID": "p1", "displayName": "   " })),
            200,
            "whitespace-only rename trims to a no-op"
        );
    }

    #[test]
    fn wire_principal_is_always_replaced_by_the_host() {
        let runtime = ControllerHostRuntime::owner_transport("ssh", Some("uid:501".into()), None);
        assert_eq!(
            runtime.principal,
            ControllerPrincipal::OwnerTransport {
                transport: "ssh".into(),
                subject: Some("uid:501".into()),
            }
        );
    }
}
