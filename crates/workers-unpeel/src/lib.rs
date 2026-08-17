//! Typed Comet adapter for the pinned Unpeel local worker runtime.

use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use base64::Engine as _;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use thiserror::Error;
use unpeel_core::controller_host::ControllerHostRuntime;
use unpeel_core::relay_crypto::TunnelRequest;
use unpeel_core::session_host;

pub fn is_session_host_mode(args: &[String]) -> bool {
    session_host_launch_args(args).is_some()
}

pub fn session_host_launch_args(args: &[String]) -> Option<&[String]> {
    if args.first().map(String::as_str) == Some(session_host::SESSION_HOST_ARG) {
        Some(&args[1..])
    } else {
        None
    }
}

/// The `unpeel-host <launch-file>` compatibility entrypoint used by
/// `session_ops::spawn_session` before it re-execs the detached
/// `__session_host__` process.
pub fn session_host_launcher_path(args: &[String]) -> Option<&Path> {
    let [path] = args else {
        return None;
    };
    let path = Path::new(path);
    (path.file_name().and_then(|name| name.to_str()) == Some("launch.json")).then_some(path)
}

/// Let the Comet executable serve as Unpeel's detached local PTY host.
/// Returns `true` after a host invocation has run, `false` for normal Comet CLI/UI arguments.
pub fn run_session_host_mode_if_requested() -> Result<bool, String> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    if let Some(host_args) = session_host_launch_args(&args) {
        session_host::run_from_args(host_args)?;
        return Ok(true);
    }
    if let Some(launch_file) = session_host_launcher_path(&args) {
        session_host::spawn_host_process_from_launch_file(launch_file)?;
        return Ok(true);
    }
    Ok(false)
}

/// Make the current Comet executable the Unpeel launcher when no explicit
/// host override was supplied. This keeps packaged and development builds
/// self-contained: the launcher-file mode above re-execs the same binary in
/// `__session_host__` mode.
pub fn configure_self_as_session_host_launcher() -> Result<(), String> {
    if std::env::var_os("UNPEEL_HOST_CMD").is_some() {
        return Ok(());
    }
    let executable = std::env::current_exe()
        .map_err(|error| format!("Failed to locate Comet session host: {error}"))?;
    // SAFETY: callers invoke this at process startup, before GPUI, Tokio, or
    // any application worker threads exist. Children inherit this value.
    unsafe { std::env::set_var("UNPEEL_HOST_CMD", executable) };
    Ok(())
}

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
    pub cli_id: Option<String>,
    pub enabled: bool,
    pub quick_launch: bool,
    pub is_default: bool,
    pub tint_color_hex: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum InitialTextSubmitMode {
    #[serde(rename = "pasteOnly")]
    PasteOnly,
    #[serde(rename = "pasteAndSubmit")]
    PasteAndSubmit,
    #[serde(rename = "raw")]
    Raw,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkersLaunchRequest {
    pub project_id: String,
    pub preset_id: Option<String>,
    pub command: Option<String>,
    pub worktree_path: Option<String>,
    pub worktree_branch: Option<String>,
    pub initial_text: Option<String>,
    pub initial_text_submit_mode: Option<InitialTextSubmitMode>,
}

impl WorkersLaunchRequest {
    pub fn terminal(project_id: impl Into<String>) -> Self {
        Self::command(project_id, "")
    }

    pub fn preset(project_id: impl Into<String>, preset_id: impl Into<String>) -> Self {
        Self {
            project_id: project_id.into(),
            preset_id: Some(preset_id.into()),
            command: None,
            worktree_path: None,
            worktree_branch: None,
            initial_text: None,
            initial_text_submit_mode: None,
        }
    }

    pub fn command(project_id: impl Into<String>, command: impl Into<String>) -> Self {
        Self {
            project_id: project_id.into(),
            preset_id: None,
            command: Some(command.into()),
            worktree_path: None,
            worktree_branch: None,
            initial_text: None,
            initial_text_submit_mode: None,
        }
    }

    pub fn with_worktree(mut self, path: impl Into<String>, branch: impl Into<String>) -> Self {
        self.worktree_path = Some(path.into());
        self.worktree_branch = Some(branch.into());
        self
    }

    pub fn with_optional_worktree(
        self,
        path: Option<impl Into<String>>,
        branch: Option<impl Into<String>>,
    ) -> Self {
        match (path, branch) {
            (Some(path), Some(branch)) => self.with_worktree(path, branch),
            _ => self,
        }
    }

    pub fn with_initial_text(
        mut self,
        text: impl Into<String>,
        mode: InitialTextSubmitMode,
    ) -> Self {
        self.initial_text = Some(text.into());
        self.initial_text_submit_mode = Some(mode);
        self
    }

    pub fn wire_body(&self) -> Value {
        let mut body = serde_json::Map::new();
        body.insert("projectID".into(), self.project_id.clone().into());
        if let Some(value) = &self.preset_id {
            body.insert("presetID".into(), value.clone().into());
        }
        if let Some(value) = &self.command {
            body.insert("command".into(), value.clone().into());
        }
        if let Some(value) = &self.worktree_path {
            body.insert("worktreePath".into(), value.clone().into());
        }
        if let Some(value) = &self.worktree_branch {
            body.insert("worktreeBranch".into(), value.clone().into());
        }
        if let Some(value) = &self.initial_text {
            body.insert("initialText".into(), value.clone().into());
        }
        if let Some(value) = self.initial_text_submit_mode {
            body.insert(
                "initialTextSubmitMode".into(),
                serde_json::to_value(value).expect("submit mode serializes"),
            );
        }
        body.into()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkersRuntime {
    pub cli_id: String,
    pub label: String,
    pub icon: String,
    pub tint_color_hex: Option<String>,
    pub spinner_tint_color_hex: Option<String>,
    pub supports_quick_launch: bool,
    pub installed: bool,
    pub installed_path: Option<String>,
    pub install_command: Option<String>,
    pub official_url: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkersPresetSetting {
    pub id: String,
    pub label: String,
    pub command: String,
    pub project_id: Option<String>,
    pub cli_id: Option<String>,
    pub enabled: bool,
    pub quick_launch: bool,
    pub is_default: bool,
    pub installed: bool,
    pub supports_quick_launch: bool,
    pub risky: bool,
    pub icon: String,
    pub tint_color_hex: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkersTranscriptSettings {
    #[serde(default = "default_true")]
    pub include_user: bool,
    #[serde(default = "default_true")]
    pub include_assistant: bool,
    #[serde(default)]
    pub include_reasoning: bool,
    #[serde(default)]
    pub include_tools: bool,
    #[serde(default = "default_true")]
    pub include_file_changes: bool,
    #[serde(default = "default_true")]
    pub include_plan_updates: bool,
    #[serde(default = "default_true")]
    pub include_session_info: bool,
    #[serde(default = "default_transcript_max_entries")]
    pub max_entries: usize,
}

fn default_true() -> bool {
    true
}

fn default_transcript_max_entries() -> usize {
    20
}

impl Default for WorkersTranscriptSettings {
    fn default() -> Self {
        Self {
            include_user: true,
            include_assistant: true,
            include_reasoning: false,
            include_tools: false,
            include_file_changes: true,
            include_plan_updates: true,
            include_session_info: true,
            max_entries: 20,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkersNotificationSettings {
    #[serde(default = "default_true")]
    pub menu_attention_detection: bool,
    #[serde(default = "default_true")]
    pub desktop_notifications: bool,
    #[serde(default = "default_true")]
    pub sound_enabled: bool,
    #[serde(default = "default_true")]
    pub background_only: bool,
}

impl Default for WorkersNotificationSettings {
    fn default() -> Self {
        Self {
            menu_attention_detection: true,
            desktop_notifications: true,
            sound_enabled: true,
            background_only: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkersSettingsSnapshot {
    pub presets: Vec<WorkersPresetSetting>,
    pub runtimes: Vec<WorkersRuntime>,
    pub transcripts: WorkersTranscriptSettings,
    pub notifications: WorkersNotificationSettings,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct PresetPatch {
    pub label: Option<String>,
    pub command: Option<String>,
    pub enabled: Option<bool>,
    pub quick_launch: Option<bool>,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct WorkersSessionCapabilities {
    pub restart: bool,
    pub resume_agent: bool,
    pub fork: bool,
    pub archive: bool,
    pub append_system_context: bool,
    pub notify_when_done: bool,
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
    pub active_runtime_id: Option<String>,
    pub runtime_launch_pending: bool,
    pub notify_when_done: bool,
    pub terminal_background_hex: Option<String>,
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
    #[error("Unpeel state operation failed: {0}")]
    State(String),
    #[error("Invalid project directory {path}: {message}")]
    InvalidProject { path: String, message: String },
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

    pub fn mark_read(&self, session_id: &str) -> Result<(), WorkersError> {
        self.mutate("/mobile/mark-read", json!({ "sessionID": session_id }))
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

    pub fn launch_session(&self, launch: &WorkersLaunchRequest) -> Result<String, WorkersError> {
        let body = self.request("POST", "/mobile/sessions", Vec::new(), launch.wire_body())?;
        let wire: CreatedSessionWire = serde_json::from_value(body)?;
        Ok(wire.session_id)
    }

    pub fn create_session(&self, project_id: &str, command: &str) -> Result<String, WorkersError> {
        self.launch_session(&WorkersLaunchRequest::command(project_id, command))
    }

    pub fn settings(&self) -> Result<WorkersSettingsSnapshot, WorkersError> {
        let raw = unpeel_core::app_state::load().map_err(WorkersError::State)?;
        let presets = raw
            .get("presets")
            .cloned()
            .map(serde_json::from_value::<Vec<unpeel_core::state::Preset>>)
            .transpose()?
            .unwrap_or_default();
        let transcripts = raw
            .get("transcript_settings")
            .cloned()
            .map(serde_json::from_value::<WorkersTranscriptSettings>)
            .transpose()?
            .unwrap_or_default();
        let notifications = raw
            .get("comet_workers_notifications")
            .cloned()
            .map(serde_json::from_value::<WorkersNotificationSettings>)
            .transpose()?
            .unwrap_or_default();
        let runtimes = runtime_catalog_snapshot();
        let presets = preset_settings(presets, &runtimes);
        Ok(WorkersSettingsSnapshot {
            presets,
            runtimes,
            transcripts,
            notifications,
        })
    }

    pub fn add_preset(&self, label: &str, command: &str) -> Result<String, WorkersError> {
        let label = label.trim();
        let command = command.trim();
        if label.is_empty() || command.is_empty() {
            return Err(WorkersError::State(
                "preset label and command are required".into(),
            ));
        }
        let id = format!("comet-{}", uuid::Uuid::new_v4().simple());
        edit_presets(|presets| {
            presets.push(unpeel_core::state::Preset {
                id: id.clone(),
                label: label.to_owned(),
                command: command.to_owned(),
                project_id: None,
                enabled: true,
                quick_launch: false,
            });
            Ok(())
        })?;
        Ok(id)
    }

    pub fn update_preset(&self, id: &str, patch: PresetPatch) -> Result<(), WorkersError> {
        edit_presets(|presets| {
            let preset = presets
                .iter_mut()
                .find(|preset| preset.id == id)
                .ok_or_else(|| format!("unknown preset id: {id}"))?;
            if let Some(label) = patch.label {
                let label = label.trim();
                if label.is_empty() {
                    return Err("preset label is required".into());
                }
                preset.label = label.to_owned();
            }
            if let Some(command) = patch.command {
                let command = command.trim();
                if command.is_empty() {
                    return Err("preset command is required".into());
                }
                preset.command = command.to_owned();
            }
            if let Some(enabled) = patch.enabled {
                preset.enabled = enabled;
            }
            if let Some(quick_launch) = patch.quick_launch {
                preset.quick_launch =
                    unpeel_core::state::sanitize_preset_quick_launch(&preset.command, quick_launch);
            }
            Ok(())
        })
    }

    pub fn delete_preset(&self, id: &str) -> Result<(), WorkersError> {
        edit_presets(|presets| {
            let before = presets.len();
            presets.retain(|preset| preset.id != id);
            if presets.len() == before {
                return Err(format!("unknown preset id: {id}"));
            }
            Ok(())
        })
    }

    pub fn move_preset(&self, id: &str, target_index: usize) -> Result<(), WorkersError> {
        edit_presets(|presets| {
            let from = presets
                .iter()
                .position(|preset| preset.id == id)
                .ok_or_else(|| format!("unknown preset id: {id}"))?;
            let preset = presets.remove(from);
            let target = target_index.min(presets.len());
            presets.insert(target, preset);
            Ok(())
        })
    }

    pub fn set_transcript_settings(
        &self,
        settings: WorkersTranscriptSettings,
    ) -> Result<(), WorkersError> {
        if !matches!(settings.max_entries, 0 | 20 | 50 | 100) {
            return Err(WorkersError::State(
                "transcript max_entries must be 0, 20, 50, or 100".into(),
            ));
        }
        unpeel_core::app_state::edit(|state| {
            state.insert(
                "transcript_settings".into(),
                serde_json::to_value(settings).map_err(|error| error.to_string())?,
            );
            Ok(())
        })
        .map_err(WorkersError::State)
    }

    pub fn set_notification_settings(
        &self,
        settings: WorkersNotificationSettings,
    ) -> Result<(), WorkersError> {
        unpeel_core::app_state::edit(|state| {
            state.insert(
                "comet_workers_notifications".into(),
                serde_json::to_value(settings).map_err(|error| error.to_string())?,
            );
            Ok(())
        })
        .map_err(WorkersError::State)
    }

    pub fn add_project(&self, path: &Path) -> Result<String, WorkersError> {
        let canonical =
            std::fs::canonicalize(path).map_err(|error| WorkersError::InvalidProject {
                path: path.display().to_string(),
                message: error.to_string(),
            })?;
        if !canonical.is_dir() {
            return Err(WorkersError::InvalidProject {
                path: canonical.display().to_string(),
                message: "not a directory".into(),
            });
        }
        let canonical_string = canonical.to_string_lossy().to_string();
        let name = canonical
            .file_name()
            .and_then(|value| value.to_str())
            .filter(|value| !value.is_empty())
            .unwrap_or("Project")
            .to_owned();
        unpeel_core::app_state::edit(|state| {
            let projects_value = state
                .entry("projects")
                .or_insert_with(|| Value::Array(Vec::new()));
            let projects = projects_value
                .as_array_mut()
                .ok_or_else(|| "projects must be an array".to_string())?;
            if let Some(id) = projects.iter().find_map(|project| {
                (project.get("path").and_then(Value::as_str) == Some(canonical_string.as_str()))
                    .then(|| project.get("id")?.as_str().map(str::to_owned))
                    .flatten()
            }) {
                return Ok(id);
            }
            let id = format!("comet-{}", uuid::Uuid::new_v4().simple());
            projects.push(json!({
                "id": id,
                "name": name,
                "path": canonical_string,
                "workspace_id": "personal",
                "sort_order": projects.len() as u32,
            }));
            Ok(id)
        })
        .map_err(WorkersError::State)
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
    #[serde(default, rename = "cliID")]
    cli_id: Option<String>,
    #[serde(default = "default_true")]
    enabled: bool,
    #[serde(default)]
    quick_launch: bool,
    #[serde(default)]
    is_default: bool,
    #[serde(default)]
    tint_color_hex: Option<String>,
}

impl From<PresetWire> for WorkersPreset {
    fn from(value: PresetWire) -> Self {
        Self {
            id: value.id,
            label: value.label,
            command: value.command,
            cli_id: value.cli_id,
            enabled: value.enabled,
            quick_launch: value.quick_launch,
            is_default: value.is_default,
            tint_color_hex: value.tint_color_hex,
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
    #[serde(default)]
    append_system_context: bool,
    #[serde(default)]
    notify_when_done: bool,
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
    #[serde(default, rename = "activeRuntimeID")]
    active_runtime_id: Option<String>,
    #[serde(default)]
    runtime_launch_pending: bool,
    #[serde(default)]
    notify_when_done: bool,
    #[serde(default)]
    terminal_background_hex: Option<String>,
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
            active_runtime_id: value.active_runtime_id,
            runtime_launch_pending: value.runtime_launch_pending,
            notify_when_done: value.notify_when_done,
            terminal_background_hex: value.terminal_background_hex,
            worktree_branch: value.worktree_branch,
            created_at_unix_ms: value.created_at_unix_ms,
            updated_at_unix_ms: value.updated_at_unix_ms,
            capabilities: WorkersSessionCapabilities {
                restart: value.capabilities.restart,
                resume_agent: value.capabilities.resume_agent,
                fork: value.capabilities.fork,
                archive: value.capabilities.archive,
                append_system_context: value.capabilities.append_system_context,
                notify_when_done: value.capabilities.notify_when_done,
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

fn edit_presets(
    mutate: impl FnOnce(&mut Vec<unpeel_core::state::Preset>) -> Result<(), String>,
) -> Result<(), WorkersError> {
    unpeel_core::app_state::edit(|state| {
        let mut presets = state
            .get("presets")
            .cloned()
            .map(serde_json::from_value::<Vec<unpeel_core::state::Preset>>)
            .transpose()
            .map_err(|error| error.to_string())?
            .unwrap_or_default();
        mutate(&mut presets)?;
        state.insert(
            "presets".into(),
            serde_json::to_value(presets).map_err(|error| error.to_string())?,
        );
        Ok(())
    })
    .map_err(WorkersError::State)
}

fn runtime_catalog_snapshot() -> Vec<WorkersRuntime> {
    let search_dirs = unpeel_core::setup::search_dirs();
    unpeel_core::runtime_catalog::builtin_runtime_catalog()
        .current_platform_descriptors()
        .map(|runtime| {
            let installed_path = runtime
                .detection
                .command_aliases
                .iter()
                .find_map(|alias| unpeel_core::setup::find_command_path(alias, &search_dirs));
            WorkersRuntime {
                cli_id: runtime.legacy_slug.clone(),
                label: runtime.label.clone(),
                icon: runtime.display.icon.clone(),
                tint_color_hex: runtime.display.tint.clone(),
                spinner_tint_color_hex: runtime.display.spinner_tint.clone(),
                supports_quick_launch: runtime.supports_quick_launch,
                installed: installed_path.is_some(),
                installed_path,
                install_command: runtime
                    .install
                    .as_ref()
                    .and_then(|install| install.command.clone()),
                official_url: runtime
                    .install
                    .as_ref()
                    .map(|install| install.official_url.clone()),
            }
        })
        .collect()
}

fn preset_settings(
    presets: Vec<unpeel_core::state::Preset>,
    runtimes: &[WorkersRuntime],
) -> Vec<WorkersPresetSetting> {
    let catalog = unpeel_core::runtime_catalog::builtin_runtime_catalog();
    let mut default_cli_ids = std::collections::HashSet::new();
    presets
        .into_iter()
        .map(|preset| {
            let head = unpeel_core::integrations::command_head(&preset.command);
            let runtime = catalog.by_command_alias_for_current_platform(head);
            let cli_id = runtime.map(|runtime| runtime.legacy_slug.clone());
            let runtime_snapshot = cli_id
                .as_deref()
                .and_then(|id| runtimes.iter().find(|runtime| runtime.cli_id == id));
            let is_default = preset.enabled
                && cli_id
                    .as_ref()
                    .is_some_and(|id| default_cli_ids.insert(id.clone()));
            WorkersPresetSetting {
                id: preset.id,
                label: preset.label,
                command: preset.command.clone(),
                project_id: preset.project_id,
                cli_id,
                enabled: preset.enabled,
                quick_launch: preset.quick_launch,
                is_default,
                installed: runtime_snapshot.map_or(true, |runtime| runtime.installed),
                supports_quick_launch: runtime_snapshot
                    .is_some_and(|runtime| runtime.supports_quick_launch),
                risky: command_is_risky(&preset.command),
                icon: runtime_snapshot
                    .map(|runtime| runtime.icon.clone())
                    .unwrap_or_else(|| "terminal".into()),
                tint_color_hex: runtime_snapshot.and_then(|runtime| runtime.tint_color_hex.clone()),
            }
        })
        .collect()
}

fn command_is_risky(command: &str) -> bool {
    command.split_whitespace().any(|argument| {
        argument == "--yolo"
            || argument == "--force"
            || argument == "-f"
            || argument.starts_with("--dangerously")
    })
}
