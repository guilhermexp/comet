//! Typed Comet adapter for the pinned Unpeel local worker runtime.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use base64::Engine as _;
use serde::Deserialize;
use serde_json::{Value, json};
use thiserror::Error;
use unpeel_core::controller_host::ControllerHostRuntime;
use unpeel_core::relay_crypto::TunnelRequest;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkersBootstrap {
    pub mac_name: String,
    pub protocol: WorkersProtocol,
    pub projects: Vec<WorkersProject>,
    pub presets: Vec<WorkersPreset>,
    pub sessions: Vec<WorkersSession>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkersProtocol {
    pub major_version: u16,
    pub minor_version: u16,
    pub capabilities: Vec<String>,
}

impl WorkersProtocol {
    pub fn supports(&self, capability: &str) -> bool {
        self.capabilities.iter().any(|value| value == capability)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkersProject {
    pub id: String,
    pub name: String,
    pub path: String,
    pub folder_id: Option<String>,
    pub parent_project_id: Option<String>,
    pub is_group: bool,
    pub worktree_branch: Option<String>,
    pub archived_session_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkersPreset {
    pub id: String,
    pub label: String,
    pub command: String,
    pub quick_launch: bool,
    pub is_default: bool,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct WorkersSessionCapabilities {
    pub restart: bool,
    pub resume_agent: bool,
    pub fork: bool,
    pub archive: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkersSession {
    pub id: String,
    pub project_id: String,
    pub title: String,
    pub command: String,
    pub state: String,
    pub activity: String,
    pub unread: bool,
    pub pinned: bool,
    pub archived: bool,
    pub provider_id: Option<String>,
    pub worktree_branch: Option<String>,
    pub created_at_unix_ms: u64,
    pub updated_at_unix_ms: u64,
    pub capabilities: WorkersSessionCapabilities,
}

impl WorkersSession {
    pub fn is_live(&self) -> bool {
        self.state == "running"
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkersOutput {
    pub offset: u64,
    pub next_offset: u64,
    pub data: Vec<u8>,
    pub truncated: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionAction {
    Stop,
    Restart,
    RestartAgent,
    ResumeAgent,
    Remove,
}

impl SessionAction {
    fn wire_name(self) -> &'static str {
        match self {
            Self::Stop => "stop",
            Self::Restart => "restart",
            Self::RestartAgent => "restart_agent",
            Self::ResumeAgent => "resume_agent",
            Self::Remove => "remove",
        }
    }
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct SessionOrganizationPatch {
    pub title: Option<String>,
    pub pinned: Option<bool>,
    pub archived: Option<bool>,
}

#[derive(Debug, Error)]
pub enum WorkersError {
    #[error("Unpeel request failed with status {status}: {message}")]
    Upstream { status: u16, message: String },
    #[error("Unpeel returned an invalid response: {0}")]
    InvalidResponse(#[from] serde_json::Error),
    #[error("Unpeel returned invalid terminal output: {0}")]
    InvalidOutput(#[from] base64::DecodeError),
}

#[derive(Debug, Clone)]
pub struct LocalWorkersClient {
    next_request_id: Arc<AtomicU64>,
}

impl Default for LocalWorkersClient {
    fn default() -> Self {
        Self::new()
    }
}

impl LocalWorkersClient {
    pub fn new() -> Self {
        Self {
            next_request_id: Arc::new(AtomicU64::new(1)),
        }
    }

    pub fn bootstrap(&self) -> Result<WorkersBootstrap, WorkersError> {
        let body = self.request("GET", "/mobile/bootstrap", Vec::new(), Value::Null)?;
        let wire: BootstrapWire = serde_json::from_value(body)?;
        Ok(WorkersBootstrap {
            mac_name: wire.mac_name,
            protocol: WorkersProtocol {
                major_version: wire.host_protocol.major_version,
                minor_version: wire.host_protocol.minor_version,
                capabilities: wire.host_protocol.capabilities,
            },
            projects: wire
                .projects
                .into_iter()
                .map(WorkersProject::from)
                .collect(),
            presets: wire.presets.into_iter().map(WorkersPreset::from).collect(),
            sessions: wire
                .sessions
                .into_iter()
                .map(WorkersSession::from)
                .collect(),
        })
    }

    pub fn read_output(
        &self,
        session_id: &str,
        offset: Option<u64>,
        wait_ms: u64,
    ) -> Result<WorkersOutput, WorkersError> {
        let mut query = vec![
            ("session_id".to_owned(), session_id.to_owned()),
            ("wait_ms".to_owned(), wait_ms.min(25_000).to_string()),
        ];
        if let Some(offset) = offset {
            query.push(("offset".to_owned(), offset.to_string()));
        }
        let wire: OutputWire =
            serde_json::from_value(self.request("GET", "/mobile/output", query, Value::Null)?)?;
        Ok(WorkersOutput {
            offset: wire.offset,
            next_offset: wire.next_offset,
            data: base64::engine::general_purpose::STANDARD.decode(wire.data_base64)?,
            truncated: wire.truncated,
        })
    }

    pub fn write(&self, session_id: &str, data: &str) -> Result<(), WorkersError> {
        self.mutate(
            "/mobile/write",
            json!({ "sessionID": session_id, "data": data }),
        )
    }

    pub fn resize(&self, session_id: &str, columns: u16, rows: u16) -> Result<(), WorkersError> {
        self.mutate(
            "/mobile/resize",
            json!({
                "sessionID": session_id,
                "columns": columns.clamp(2, 300),
                "rows": rows.clamp(2, 120),
            }),
        )
    }

    pub fn create_session(&self, project_id: &str, command: &str) -> Result<String, WorkersError> {
        let body = self.request(
            "POST",
            "/mobile/sessions",
            Vec::new(),
            json!({ "projectID": project_id, "command": command }),
        )?;
        let wire: CreatedSessionWire = serde_json::from_value(body)?;
        Ok(wire.session_id)
    }

    pub fn session_action(
        &self,
        session_id: &str,
        action: SessionAction,
    ) -> Result<(), WorkersError> {
        self.mutate(
            "/mobile/session-action",
            json!({ "sessionID": session_id, "action": action.wire_name() }),
        )
    }

    pub fn set_session_organization(
        &self,
        session_id: &str,
        patch: SessionOrganizationPatch,
    ) -> Result<(), WorkersError> {
        let mut body = serde_json::Map::new();
        body.insert("sessionID".into(), session_id.into());
        if let Some(title) = patch.title {
            body.insert("title".into(), title.into());
        }
        if let Some(pinned) = patch.pinned {
            body.insert("pinned".into(), pinned.into());
        }
        if let Some(archived) = patch.archived {
            body.insert("archived".into(), archived.into());
        }
        self.mutate("/mobile/session-organization", body.into())
    }

    pub fn archived_sessions(&self, project_id: &str) -> Result<Vec<WorkersSession>, WorkersError> {
        let body = self.request(
            "GET",
            "/mobile/archive",
            vec![("project_id".to_owned(), project_id.to_owned())],
            Value::Null,
        )?;
        let wire: ArchiveWire = serde_json::from_value(body)?;
        Ok(wire
            .sessions
            .into_iter()
            .map(WorkersSession::from)
            .collect())
    }

    fn mutate(&self, path: &str, body: Value) -> Result<(), WorkersError> {
        self.request("POST", path, Vec::new(), body).map(|_| ())
    }

    fn request(
        &self,
        method: &str,
        path: &str,
        query: Vec<(String, String)>,
        body: Value,
    ) -> Result<Value, WorkersError> {
        let id = self.next_request_id.fetch_add(1, Ordering::Relaxed);
        let runtime = ControllerHostRuntime::owner_transport("comet-local", None, None);
        let request = TunnelRequest {
            id,
            method: method.to_owned(),
            path: path.to_owned(),
            query,
            auth: None,
            content_type: (!body.is_null()).then(|| "application/json".to_owned()),
            body: if body.is_null() {
                Vec::new()
            } else {
                serde_json::to_vec(&body)?
            },
        };
        let response = runtime.handle_tunnel("comet-workers", request, &AtomicBool::new(false));
        if response.status == 200 {
            return Ok(response.body);
        }
        let message = response
            .body
            .get("error")
            .and_then(Value::as_str)
            .unwrap_or("unknown upstream error")
            .to_owned();
        Err(WorkersError::Upstream {
            status: response.status,
            message,
        })
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BootstrapWire {
    #[serde(default = "default_host_name")]
    mac_name: String,
    host_protocol: ProtocolWire,
    #[serde(default)]
    projects: Vec<ProjectWire>,
    #[serde(default)]
    presets: Vec<PresetWire>,
    #[serde(default)]
    sessions: Vec<SessionWire>,
}

fn default_host_name() -> String {
    "This Mac".to_owned()
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProtocolWire {
    major_version: u16,
    minor_version: u16,
    capabilities: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProjectWire {
    id: String,
    name: String,
    path: String,
    #[serde(default, rename = "folderID")]
    folder_id: Option<String>,
    #[serde(default, rename = "parentProjectID")]
    parent_project_id: Option<String>,
    #[serde(default)]
    is_group: bool,
    #[serde(default)]
    worktree_branch: Option<String>,
    #[serde(default)]
    archived_session_count: usize,
}

impl From<ProjectWire> for WorkersProject {
    fn from(value: ProjectWire) -> Self {
        Self {
            id: value.id,
            name: value.name,
            path: value.path,
            folder_id: value.folder_id,
            parent_project_id: value.parent_project_id,
            is_group: value.is_group,
            worktree_branch: value.worktree_branch,
            archived_session_count: value.archived_session_count,
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PresetWire {
    id: String,
    label: String,
    command: String,
    #[serde(default)]
    quick_launch: bool,
    #[serde(default)]
    is_default: bool,
}

impl From<PresetWire> for WorkersPreset {
    fn from(value: PresetWire) -> Self {
        Self {
            id: value.id,
            label: value.label,
            command: value.command,
            quick_launch: value.quick_launch,
            is_default: value.is_default,
        }
    }
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SessionCapabilitiesWire {
    #[serde(default)]
    restart: bool,
    #[serde(default)]
    resume_agent: bool,
    #[serde(default)]
    fork: bool,
    #[serde(default)]
    archive: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SessionWire {
    id: String,
    #[serde(rename = "projectID")]
    project_id: String,
    title: String,
    #[serde(default)]
    command: String,
    status: String,
    #[serde(default)]
    activity: String,
    #[serde(default)]
    unread: bool,
    #[serde(default)]
    pinned: bool,
    #[serde(default)]
    archived: bool,
    #[serde(default, rename = "providerID")]
    provider_id: Option<String>,
    #[serde(default)]
    worktree_branch: Option<String>,
    #[serde(default)]
    created_at_unix_ms: u64,
    #[serde(default)]
    updated_at_unix_ms: u64,
    #[serde(default)]
    capabilities: SessionCapabilitiesWire,
}

impl From<SessionWire> for WorkersSession {
    fn from(value: SessionWire) -> Self {
        Self {
            id: value.id,
            project_id: value.project_id,
            title: value.title,
            command: value.command,
            state: value.status,
            activity: value.activity,
            unread: value.unread,
            pinned: value.pinned,
            archived: value.archived,
            provider_id: value.provider_id,
            worktree_branch: value.worktree_branch,
            created_at_unix_ms: value.created_at_unix_ms,
            updated_at_unix_ms: value.updated_at_unix_ms,
            capabilities: WorkersSessionCapabilities {
                restart: value.capabilities.restart,
                resume_agent: value.capabilities.resume_agent,
                fork: value.capabilities.fork,
                archive: value.capabilities.archive,
            },
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct OutputWire {
    offset: u64,
    next_offset: u64,
    data_base64: String,
    truncated: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreatedSessionWire {
    #[serde(rename = "sessionID")]
    session_id: String,
}

#[derive(Debug, Deserialize)]
struct ArchiveWire {
    #[serde(default)]
    sessions: Vec<SessionWire>,
}
