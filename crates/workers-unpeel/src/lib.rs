//! Typed Comet adapter for the pinned Unpeel local worker runtime.

mod activity_bridge;
mod controller_mcp;
mod hook_migration;
mod parent_notifications;
pub mod resources;
mod session_event_journal;
pub mod workspace_trust;

pub use controller_mcp::CONTROLLER_MCP_ARG;
#[doc(hidden)]
pub use hook_migration::remove_legacy_hook_root_at;
pub use parent_notifications::{
    WorkerCompletionEvidence, WorkerParentLink, WorkerParentNotification,
    WorkerParentNotificationKind, ack_worker_parent_notification,
    ack_worker_parent_notification_at, activate_worker_parent_task, activate_worker_parent_task_at,
    begin_worker_parent_task, begin_worker_parent_task_at, build_worker_parent_notification_prompt,
    cancel_worker_parent_task, cancel_worker_parent_task_at, confirm_worker_parent_task_submission,
    pending_worker_parent_notifications, pending_worker_parent_notifications_at,
    pending_worker_parent_notifications_with_evidence_at, prepare_worker_parent_task,
    prepare_worker_parent_task_at, register_worker_parent, register_worker_parent_at,
    worker_parent_links, worker_parent_links_at,
};

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use base64::Engine as _;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use thiserror::Error;
use unicode_width::UnicodeWidthChar as _;
use unpeel_core::activity_log::{ActivityLogEntry, ActivityLogKind, ActivityLogStore};
use unpeel_core::controller_host::ControllerHostRuntime;
use unpeel_core::relay_crypto::TunnelRequest;
use unpeel_core::terminal_viewport::{TerminalViewportSnapshot, TerminalViewportStyleRun};
use unpeel_core::{browser_mcp, computer_mcp, mcp_gate, mcp_host, session_host};

pub fn is_session_host_mode(args: &[String]) -> bool {
    session_host_launch_args(args).is_some() || is_internal_host_mode(args)
}

fn is_internal_host_mode(args: &[String]) -> bool {
    let Some(argument) = args.first().map(String::as_str) else {
        return false;
    };
    matches!(
        argument,
        controller_mcp::CONTROLLER_MCP_ARG
            | mcp_host::MCP_HOST_ARG
            | browser_mcp::BROWSER_MCP_ARG
            | mcp_gate::MCP_GATE_ARG
            | browser_mcp::BROWSER_CLEANUP_ARG
            | computer_mcp::COMPUTER_CLEANUP_ARG
            | session_host::COMPACT_OUTPUT_JOURNALS_ARG
    ) || unpeel_core::integrations::legacy_mcp_gate_kind(argument).is_some()
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
    let mut args = std::env::args().skip(1).collect::<Vec<_>>();
    if args.first().map(String::as_str) == Some(controller_mcp::CONTROLLER_MCP_ARG) {
        if args.len() != 1 {
            return Err(format!(
                "usage: zeron {}",
                controller_mcp::CONTROLLER_MCP_ARG
            ));
        }
        ensure_controller_mcp_host_launcher()?;
        controller_mcp::run_stdio()?;
        return Ok(true);
    }
    if args.first().map(String::as_str) == Some(session_host::COMPACT_OUTPUT_JOURNALS_ARG) {
        if args.len() != 1 {
            return Err(format!(
                "usage: zeron {}",
                session_host::COMPACT_OUTPUT_JOURNALS_ARG
            ));
        }
        let summary = session_host::compact_exited_output_journals()?;
        println!(
            "scanned={} compacted={} logical_bytes_evicted={}",
            summary.scanned, summary.compacted, summary.logical_bytes_evicted
        );
        return Ok(true);
    }
    if args.first().map(String::as_str) == Some(mcp_host::MCP_HOST_ARG) {
        mcp_host::run_stdio()?;
        return Ok(true);
    }
    if args.first().map(String::as_str) == Some(browser_mcp::BROWSER_MCP_ARG) {
        browser_mcp::run_stdio()?;
        return Ok(true);
    }
    if args.first().map(String::as_str) == Some(mcp_gate::MCP_GATE_ARG) {
        args.remove(0);
        mcp_gate::run_stdio(args.first().map(String::as_str).unwrap_or_default())?;
        return Ok(true);
    }
    if let Some(kind) = args
        .first()
        .and_then(|argument| unpeel_core::integrations::legacy_mcp_gate_kind(argument))
    {
        mcp_gate::run_stdio(kind)?;
        return Ok(true);
    }
    if args.first().map(String::as_str) == Some(browser_mcp::BROWSER_CLEANUP_ARG) {
        args.remove(0);
        browser_mcp::run_cleanup(&args)?;
        return Ok(true);
    }
    if args.first().map(String::as_str) == Some(computer_mcp::COMPUTER_CLEANUP_ARG) {
        args.remove(0);
        computer_mcp::run_cleanup(&args)?;
        return Ok(true);
    }
    if let Some(host_args) = session_host_launch_args(&args) {
        let _journal = session_event_journal::install_for_session_host(host_args)?;
        session_host::run_from_args(host_args)?;
        return Ok(true);
    }
    if let Some(launch_file) = session_host_launcher_path(&args) {
        session_host::spawn_host_process_from_launch_file(launch_file)?;
        return Ok(true);
    }
    Ok(false)
}

#[doc(hidden)]
pub fn controller_mcp_handle_request(request: Value) -> Option<Value> {
    controller_mcp::handle_request(request)
}

#[doc(hidden)]
pub fn controller_mcp_parse_launch(request: Value) -> Result<WorkersLaunchRequest, String> {
    controller_mcp::parse_launch(request)
}

#[doc(hidden)]
pub fn controller_mcp_parse_launch_briefing(
    request: Value,
) -> Result<(WorkersLaunchRequest, Option<String>), String> {
    controller_mcp::parse_launch_briefing(request)
}

#[doc(hidden)]
pub fn controller_mcp_encode_keys(keys: &[String]) -> Result<String, String> {
    controller_mcp::encode_keys(keys)
}

#[doc(hidden)]
pub fn controller_mcp_clean_output(text: &str, max_bytes: usize) -> String {
    controller_mcp::clean_output(text, max_bytes)
}

#[doc(hidden)]
pub fn controller_mcp_choose_semantic_output(
    raw: &str,
    screen_rows: Option<Vec<String>>,
    max_bytes: usize,
) -> String {
    controller_mcp::choose_semantic_output(raw, screen_rows, max_bytes)
}

#[doc(hidden)]
pub fn controller_mcp_consume_authority_marker() -> Result<(), String> {
    controller_mcp::consume_authority_marker()
}

#[doc(hidden)]
pub fn controller_mcp_take_parent_chat_id() -> Option<String> {
    controller_mcp::take_parent_chat_id()
}

#[doc(hidden)]
pub fn controller_mcp_startup_prompt_response(screen: &str) -> Option<String> {
    controller_mcp::startup_prompt_response(screen)
}

#[doc(hidden)]
pub fn controller_mcp_tracks_task_episode(parent_chat_id: Option<&str>, submit: bool) -> bool {
    controller_mcp::tracks_task_episode(parent_chat_id, submit)
}

#[doc(hidden)]
pub fn controller_mcp_is_briefing_screen_ready(
    runtime: &str,
    screen: &str,
    stable_for_ms: u64,
) -> bool {
    controller_mcp::is_briefing_screen_ready(runtime, screen, stable_for_ms)
}

#[doc(hidden)]
pub fn controller_mcp_sanitize_text(text: &str) -> String {
    controller_mcp::sanitize_text(text)
}

#[doc(hidden)]
pub fn controller_mcp_archive_guard(session: &WorkersSession) -> Result<(), String> {
    controller_mcp::archive_guard(session)
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

#[doc(hidden)]
pub fn ensure_controller_mcp_host_launcher() -> Result<(), String> {
    configure_self_as_session_host_launcher()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkersBootstrap {
    pub mac_name: String,
    pub protocol: WorkersProtocol,
    pub projects: Vec<WorkersProject>,
    pub presets: Vec<WorkersPreset>,
    pub sessions: Vec<WorkersSession>,
    pub activity_log: Vec<WorkersActivityLogEntry>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkersActivityLogKind {
    Started,
    NeedsInput,
    Finished,
    Exited,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkersActivityLogEntry {
    pub id: String,
    pub session_id: String,
    pub kind: WorkersActivityLogKind,
    pub at_unix_ms: u64,
    pub title: String,
    pub command: String,
    pub project_id: String,
    pub project_name: String,
}

impl From<ActivityLogEntry> for WorkersActivityLogEntry {
    fn from(value: ActivityLogEntry) -> Self {
        Self {
            id: value.id,
            session_id: value.session_id,
            kind: match value.kind {
                ActivityLogKind::Started => WorkersActivityLogKind::Started,
                ActivityLogKind::NeedsInput => WorkersActivityLogKind::NeedsInput,
                ActivityLogKind::Finished => WorkersActivityLogKind::Finished,
                ActivityLogKind::Exited => WorkersActivityLogKind::Exited,
            },
            at_unix_ms: value.at,
            title: value.title,
            command: value.command,
            project_id: value.project_id,
            project_name: value.project_name,
        }
    }
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
    pub git_branch: Option<String>,
    pub archived_session_count: usize,
    pub folder_color_id: Option<String>,
    pub session_sort: WorkersSessionSort,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum WorkersSessionSort {
    #[default]
    Custom,
    RecentlyUpdated,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkersCreateWorktreeRequest {
    pub project_id: String,
    pub branch: String,
    pub name: Option<String>,
    pub base_ref: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkersCreateGroupRequest {
    pub parent_project_id: String,
    pub name: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WorkersProjectOrganizationPatch {
    pub display_name: Option<String>,
    pub folder_color_id: Option<Option<String>>,
    pub session_sort: Option<WorkersSessionSort>,
    pub sort_order: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkersWorktreeResult {
    pub project_id: String,
    pub path: String,
    pub branch: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkersWorktreeLaunchResult {
    pub project_id: String,
    pub session_id: String,
    pub path: String,
    pub branch: String,
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct WorkersAppearanceSettings {
    #[serde(default)]
    pub show_session_gallery: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkersResourceSettings {
    #[serde(default = "default_true")]
    pub monitoring_enabled: bool,
    #[serde(default = "default_resource_warning_gib")]
    pub per_worker_warning_gib: u16,
    #[serde(default = "default_resource_critical_gib")]
    pub per_worker_critical_gib: u16,
    #[serde(default = "default_true")]
    pub notifications_enabled: bool,
    #[serde(default)]
    pub hibernation_enabled: bool,
    #[serde(default = "default_hibernate_idle_minutes")]
    pub hibernate_after_idle_minutes: u16,
    #[serde(default = "default_max_live_idle_workers")]
    pub max_live_idle_workers: u16,
}

const fn default_resource_warning_gib() -> u16 {
    4
}

const fn default_resource_critical_gib() -> u16 {
    8
}

const fn default_hibernate_idle_minutes() -> u16 {
    15
}

const fn default_max_live_idle_workers() -> u16 {
    12
}

impl Default for WorkersResourceSettings {
    fn default() -> Self {
        Self {
            monitoring_enabled: true,
            per_worker_warning_gib: default_resource_warning_gib(),
            per_worker_critical_gib: default_resource_critical_gib(),
            notifications_enabled: true,
            hibernation_enabled: false,
            hibernate_after_idle_minutes: default_hibernate_idle_minutes(),
            max_live_idle_workers: default_max_live_idle_workers(),
        }
    }
}

impl WorkersResourceSettings {
    fn validated(mut self) -> Result<Self, WorkersError> {
        if self.per_worker_warning_gib == 0 {
            return Err(WorkersError::State(
                "resource warning threshold must be at least 1 GiB".into(),
            ));
        }
        if self.per_worker_critical_gib < self.per_worker_warning_gib {
            return Err(WorkersError::State(
                "resource critical threshold must be at least the warning threshold".into(),
            ));
        }
        self.per_worker_warning_gib = self.per_worker_warning_gib.min(1_024);
        self.per_worker_critical_gib = self.per_worker_critical_gib.clamp(1, 1_024);
        self.hibernate_after_idle_minutes = self.hibernate_after_idle_minutes.clamp(1, 10_080);
        self.max_live_idle_workers = self.max_live_idle_workers.clamp(1, 256);
        Ok(self)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkersArtifact {
    pub kind: String,
    pub name: String,
    pub path: PathBuf,
    pub size: u64,
    pub modified_at_unix_ms: u64,
    pub is_image: bool,
}

pub fn is_image_artifact_name(name: &str) -> bool {
    Path::new(name)
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            matches!(
                extension.to_ascii_lowercase().as_str(),
                "png" | "jpg" | "jpeg" | "gif" | "webp"
            )
        })
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
    pub appearance: WorkersAppearanceSettings,
    pub resources: WorkersResourceSettings,
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
    pub runtime_generation: u64,
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkersViewport {
    pub output_offset: u64,
    pub cols: u16,
    pub rows: u16,
    pub ansi: Vec<u8>,
    pub input_modes: WorkersViewportInputModes,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct WorkersViewportInputModes {
    pub known: bool,
    pub mouse_reporting: bool,
    pub mouse_button_motion: bool,
    pub mouse_any_motion: bool,
    pub alternate_screen: bool,
    pub mouse_alternate_scroll: bool,
    pub application_cursor: bool,
}

fn viewport_input_modes(snapshot: &TerminalViewportSnapshot) -> WorkersViewportInputModes {
    WorkersViewportInputModes {
        known: snapshot.input_modes_known,
        mouse_reporting: snapshot.mouse_reporting,
        mouse_button_motion: snapshot.mouse_button_motion,
        mouse_any_motion: snapshot.mouse_any_motion,
        alternate_screen: snapshot.alternate_screen,
        mouse_alternate_scroll: snapshot.mouse_alternate_scroll,
        application_cursor: snapshot.application_cursor,
    }
}

fn color_sgr(color: &str, foreground: bool) -> Option<String> {
    let channel = if foreground { 38 } else { 48 };
    if let Some(index) = color.strip_prefix("ansi:") {
        let index = index.parse::<u8>().ok()?;
        return Some(format!("{channel};5;{index}"));
    }
    if let Some(index) = color.strip_prefix("ansi256:") {
        let index = index.parse::<u8>().ok()?;
        return Some(format!("{channel};5;{index}"));
    }
    let rgb = color.strip_prefix("rgb:")?;
    let mut channels = rgb.split(',').map(str::parse::<u8>);
    let red = channels.next()?.ok()?;
    let green = channels.next()?.ok()?;
    let blue = channels.next()?.ok()?;
    channels
        .next()
        .is_none()
        .then(|| format!("{channel};2;{red};{green};{blue}"))
}

fn style_sgr(style: Option<&TerminalViewportStyleRun>) -> String {
    let Some(style) = style else {
        return "\x1b[0m".into();
    };
    let mut attributes = vec!["0".to_owned()];
    if style.bold {
        attributes.push("1".into());
    }
    if style.inverse {
        attributes.push("7".into());
    }
    if let Some(fg) = style.fg.as_deref().and_then(|color| color_sgr(color, true)) {
        attributes.push(fg);
    }
    if let Some(bg) = style
        .bg
        .as_deref()
        .and_then(|color| color_sgr(color, false))
    {
        attributes.push(bg);
    }
    format!("\x1b[{}m", attributes.join(";"))
}

fn viewport_snapshot_to_ansi(snapshot: &TerminalViewportSnapshot) -> Vec<u8> {
    let mut ansi = Vec::new();
    if snapshot.alternate_screen {
        ansi.extend_from_slice(b"\x1b[?1049h");
    }
    ansi.extend_from_slice(b"\x1b[2J\x1b[H\x1b[?7l");

    for (row_index, row) in snapshot.viewport_rows.iter().enumerate() {
        ansi.extend_from_slice(format!("\x1b[{};1H", row_index + 1).as_bytes());
        let mut column = 0_usize;
        let mut active_style = None;
        for character in row.text.chars() {
            let style_index = row.styles.iter().position(|style| {
                let start = usize::from(style.start);
                let end = start.saturating_add(usize::from(style.len));
                column >= start && column < end
            });
            if active_style != style_index {
                ansi.extend_from_slice(
                    style_sgr(style_index.map(|index| &row.styles[index])).as_bytes(),
                );
                active_style = style_index;
            }
            let mut encoded = [0_u8; 4];
            ansi.extend_from_slice(character.encode_utf8(&mut encoded).as_bytes());
            column = column.saturating_add(character.width().unwrap_or(0));
        }
        if active_style.is_some() {
            ansi.extend_from_slice(b"\x1b[0m");
        }
    }

    ansi.extend_from_slice(b"\x1b[?7h");
    ansi.extend_from_slice(if snapshot.application_cursor {
        b"\x1b[?1h"
    } else {
        b"\x1b[?1l"
    });
    let cursor_row = snapshot.cursor_row.min(snapshot.rows.saturating_sub(1)) + 1;
    let cursor_col = snapshot.cursor_col.min(snapshot.cols.saturating_sub(1)) + 1;
    ansi.extend_from_slice(format!("\x1b[{cursor_row};{cursor_col}H").as_bytes());
    ansi
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionAction {
    Stop,
    Restart,
    RestartAgent,
    ResumeAgent,
    Remove,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkersSessionCommand {
    Stop,
    RestartSession,
    RestartAgent,
    ResumeAgent,
    Fork,
    ClearAttention,
    AppendSystemContext { text: String },
    SetNotifyWhenDone { enabled: bool },
    Archive,
    Restore,
    RestoreAndResume,
    Remove,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkersTranscriptRange {
    Last20,
    Last50,
    WholeConversation,
}

impl WorkersTranscriptRange {
    pub const fn entries(self) -> usize {
        match self {
            Self::Last20 => 20,
            Self::Last50 => 50,
            Self::WholeConversation => 0,
        }
    }
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
    pub notify_when_done: Option<bool>,
    pub project_id: Option<Option<String>>,
}

#[derive(Debug, Error)]
pub enum WorkersError {
    #[error("Unpeel request failed with status {status}: {message}")]
    Upstream { status: u16, message: String },
    #[error("Unpeel returned an invalid response: {0}")]
    InvalidResponse(#[from] serde_json::Error),
    #[error("Unpeel returned invalid terminal output: {0}")]
    InvalidOutput(#[from] base64::DecodeError),
    #[error("Unpeel protocol error: {0}")]
    Protocol(String),
    #[error("Unpeel state operation failed: {0}")]
    State(String),
    #[error("Invalid project directory {path}: {message}")]
    InvalidProject { path: String, message: String },
}

#[derive(Clone)]
pub struct LocalWorkersClient {
    next_request_id: Arc<AtomicU64>,
    activity: Arc<activity_bridge::ActivityBridge>,
    last_displayed_grid: Arc<AtomicU64>,
    resource_sampler: Arc<Mutex<resources::ResourceSampler>>,
}

fn shared_displayed_grid() -> Arc<AtomicU64> {
    static GRID: std::sync::OnceLock<Arc<AtomicU64>> = std::sync::OnceLock::new();
    GRID.get_or_init(|| Arc::new(AtomicU64::new(0))).clone()
}

fn shared_resource_sampler() -> Arc<Mutex<resources::ResourceSampler>> {
    static SAMPLER: std::sync::OnceLock<Arc<Mutex<resources::ResourceSampler>>> =
        std::sync::OnceLock::new();
    SAMPLER
        .get_or_init(|| Arc::new(Mutex::new(resources::ResourceSampler::default())))
        .clone()
}

impl std::fmt::Debug for LocalWorkersClient {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("LocalWorkersClient")
            .field("hook_port", &self.activity.hook_port())
            .field("remembered_grid", &self.remembered_grid())
            .finish_non_exhaustive()
    }
}

impl Default for LocalWorkersClient {
    fn default() -> Self {
        Self::new()
    }
}

impl LocalWorkersClient {
    pub fn new() -> Self {
        if hook_migration::is_comet_application_process()
            && let Err(error) = hook_migration::ensure_managed_hook_migration()
        {
            unpeel_core::hook_assets::append_trace_log_line(&format!(
                "Comet managed hook migration is incomplete: {error}"
            ));
        }
        Self {
            next_request_id: Arc::new(AtomicU64::new(1)),
            activity: activity_bridge::shared_activity_bridge(),
            last_displayed_grid: shared_displayed_grid(),
            resource_sampler: shared_resource_sampler(),
        }
    }

    /// Monotonic signal updated as soon as a lifecycle hook is accepted.
    /// The UI can refresh immediately without inferring activity from time.
    pub fn activity_epoch(&self) -> u64 {
        self.activity.change_epoch()
    }

    pub fn resource_snapshot(
        &self,
        include_processes: bool,
    ) -> Result<resources::WorkersResourceSnapshot, WorkersError> {
        self.resource_sampler
            .lock()
            .map_err(|_| WorkersError::State("worker resource sampler lock was poisoned".into()))
            .map(|mut sampler| sampler.sample(include_processes))
    }

    /// Keep the most recently painted terminal grid available to the model's
    /// independent client. Full-screen TUIs must receive these dimensions at
    /// PTY creation time; a corrective resize after first paint is too late
    /// for clients such as Kimi Code.
    pub fn remember_grid(&self, columns: u16, rows: u16) {
        let columns = columns.clamp(2, 300) as u64;
        let rows = rows.clamp(2, 120) as u64;
        self.last_displayed_grid
            .store((columns << 16) | rows, Ordering::Relaxed);
    }

    fn remembered_grid(&self) -> Option<(u16, u16)> {
        let packed = self.last_displayed_grid.load(Ordering::Relaxed);
        (packed != 0).then_some(((packed >> 16) as u16, (packed & 0xffff) as u16))
    }

    pub fn bootstrap(&self) -> Result<WorkersBootstrap, WorkersError> {
        let body = self.request("GET", "/mobile/bootstrap", Vec::new(), Value::Null)?;
        let wire: BootstrapWire = serde_json::from_value(body)?;
        let mut bootstrap = WorkersBootstrap {
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
            activity_log: ActivityLogStore::load_default()
                .map(|store| {
                    store
                        .entries()
                        .iter()
                        .cloned()
                        .map(WorkersActivityLogEntry::from)
                        .collect()
                })
                .unwrap_or_default(),
        };
        apply_project_organization_overlay(&mut bootstrap.projects);
        apply_runtime_capabilities(&mut bootstrap.sessions);
        apply_notify_when_done_overlay(&mut bootstrap.sessions);
        self.activity.enrich(&mut bootstrap.sessions);
        Ok(bootstrap)
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

    pub fn read_viewport(
        &self,
        session_id: &str,
        columns: u16,
        rows: u16,
    ) -> Result<WorkersViewport, WorkersError> {
        let snapshot = unpeel_core::terminal_viewport::read_terminal_viewport_snapshot(
            session_id.to_owned(),
            columns.clamp(2, 300),
            rows.clamp(2, 120),
            None,
            Some(0),
            Some(rows.clamp(2, 120)),
        )
        .map_err(WorkersError::Protocol)?;
        let ansi = viewport_snapshot_to_ansi(&snapshot);
        Ok(WorkersViewport {
            output_offset: snapshot.output_offset,
            cols: snapshot.cols,
            rows: snapshot.rows,
            ansi,
            input_modes: viewport_input_modes(&snapshot),
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
        let bootstrap = self.bootstrap()?;
        let launch = workspace_trust::prepare_launch_workspace_trust(
            launch,
            &bootstrap.projects,
            &bootstrap.presets,
        )?;
        let mut launch_body = launch.wire_body();
        if let (Some(body), Some((columns, rows))) =
            (launch_body.as_object_mut(), self.remembered_grid())
        {
            body.insert("initialColumns".into(), json!(columns));
            body.insert("initialRows".into(), json!(rows));
        }
        let body = self.request("POST", "/mobile/sessions", Vec::new(), launch_body)?;
        let wire: CreatedSessionWire = serde_json::from_value(body)?;
        Ok(wire.session_id)
    }

    pub fn create_session(&self, project_id: &str, command: &str) -> Result<String, WorkersError> {
        self.launch_session(&WorkersLaunchRequest::command(project_id, command))
    }

    pub fn settings(&self) -> Result<WorkersSettingsSnapshot, WorkersError> {
        let raw = migrate_comet_workers_presets()?;
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
        let appearance = raw
            .get("comet_workers_appearance")
            .cloned()
            .map(serde_json::from_value::<WorkersAppearanceSettings>)
            .transpose()?
            .unwrap_or_default();
        let resources = raw
            .get("comet_workers_resources")
            .cloned()
            .map(serde_json::from_value::<WorkersResourceSettings>)
            .transpose()?
            .unwrap_or_default()
            .validated()
            .unwrap_or_default();
        let runtimes = runtime_catalog_snapshot();
        let presets = preset_settings(presets, &runtimes);
        Ok(WorkersSettingsSnapshot {
            presets,
            runtimes,
            transcripts,
            notifications,
            appearance,
            resources,
        })
    }

    pub fn session_artifacts(&self, session_id: &str) -> Vec<WorkersArtifact> {
        unpeel_core::session_artifacts::list(session_id)
            .into_iter()
            .filter_map(|artifact| {
                let path = unpeel_core::session_artifacts::kind_dir(session_id, &artifact.kind)?
                    .join(&artifact.name);
                Some(WorkersArtifact {
                    is_image: is_image_artifact_name(&artifact.name),
                    kind: artifact.kind,
                    name: artifact.name,
                    path,
                    size: artifact.size,
                    modified_at_unix_ms: artifact.modified_at_unix_ms,
                })
            })
            .collect()
    }

    pub fn session_artifact_dir(
        &self,
        session_id: &str,
        kind: &str,
    ) -> Result<PathBuf, WorkersError> {
        unpeel_core::session_artifacts::kind_dir(session_id, kind)
            .ok_or_else(|| WorkersError::State("invalid session artifact path".into()))
    }

    pub fn delete_session_artifact(
        &self,
        session_id: &str,
        kind: &str,
        name: &str,
    ) -> Result<(), WorkersError> {
        unpeel_core::session_artifacts::delete(session_id, kind, name).map_err(WorkersError::State)
    }

    /// Install one of Unpeel's pinned built-in runtimes with its catalog-owned
    /// command. The caller only supplies the runtime identity; executable
    /// shell text is never accepted from the renderer.
    pub fn install_runtime(&self, cli_id: &str) -> Result<WorkersRuntime, WorkersError> {
        if let Some(runtime) = runtime_catalog_snapshot()
            .into_iter()
            .find(|runtime| runtime.cli_id == cli_id && runtime.installed)
        {
            return Ok(runtime);
        }

        let install = trusted_runtime_install(cli_id).ok_or_else(|| {
            WorkersError::State(format!(
                "No trusted install command is available for {cli_id}"
            ))
        })?;
        let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".to_owned());
        let output = Command::new(&shell)
            .args(["-l", "-c", &install.command])
            .stdin(Stdio::null())
            .output()
            .map_err(|error| WorkersError::State(format!("Failed to launch {shell}: {error}")))?;
        let combined = format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        if !output.status.success() {
            let status = output.status.code().unwrap_or(-1);
            return Err(WorkersError::State(install_failure_message(
                status, &combined,
            )));
        }

        runtime_catalog_snapshot()
            .into_iter()
            .find(|runtime| runtime.cli_id == install.cli_id && runtime.installed)
            .ok_or_else(|| {
                WorkersError::State(format!(
                    "Installed, but {} was not found on your PATH",
                    install.cli_id
                ))
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

    pub fn set_appearance_settings(
        &self,
        settings: WorkersAppearanceSettings,
    ) -> Result<(), WorkersError> {
        unpeel_core::app_state::edit(|state| {
            state.insert(
                "comet_workers_appearance".into(),
                serde_json::to_value(settings).map_err(|error| error.to_string())?,
            );
            Ok(())
        })
        .map_err(WorkersError::State)
    }

    pub fn set_resource_settings(
        &self,
        settings: WorkersResourceSettings,
    ) -> Result<(), WorkersError> {
        let settings = settings.validated()?;
        unpeel_core::app_state::edit(|state| {
            state.insert(
                "comet_workers_resources".into(),
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

    pub fn create_group(&self, request: WorkersCreateGroupRequest) -> Result<String, WorkersError> {
        let name = request.name.trim();
        if name.is_empty() {
            return Err(WorkersError::State("group name is required".into()));
        }
        let parent_project_id = request.parent_project_id;
        unpeel_core::app_state::edit(|state| {
            let projects = state
                .get_mut("projects")
                .and_then(Value::as_array_mut)
                .ok_or_else(|| "projects must be an array".to_string())?;
            let parent = projects
                .iter()
                .find(|project| {
                    project.get("id").and_then(Value::as_str) == Some(&parent_project_id)
                })
                .ok_or_else(|| format!("unknown parent project id: {parent_project_id}"))?;
            let parent_path = parent
                .get("path")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned();
            let id = format!("comet-group-{}", uuid::Uuid::new_v4().simple());
            projects.push(json!({
                "id": id,
                "name": name,
                "path": parent_path,
                "workspace_id": "personal",
                "sort_order": projects.len() as u32,
                "parent_project_id": parent_project_id,
                "is_folder": true,
            }));
            Ok(id)
        })
        .map_err(WorkersError::State)
    }

    pub fn create_worktree(
        &self,
        request: WorkersCreateWorktreeRequest,
    ) -> Result<WorkersWorktreeResult, WorkersError> {
        let branch = request.branch.trim();
        if branch.is_empty() {
            return Err(WorkersError::State("worktree branch is required".into()));
        }
        let state = unpeel_core::app_state::load().map_err(WorkersError::State)?;
        let projects = state
            .get("projects")
            .and_then(Value::as_array)
            .ok_or_else(|| WorkersError::State("projects must be an array".into()))?;
        let parent = projects
            .iter()
            .find(|project| project.get("id").and_then(Value::as_str) == Some(&request.project_id))
            .ok_or_else(|| {
                WorkersError::State(format!("unknown parent project id: {}", request.project_id))
            })?;
        let parent_path = parent
            .get("path")
            .and_then(Value::as_str)
            .ok_or_else(|| WorkersError::State("parent project path is missing".into()))?;
        let worktree =
            unpeel_core::worktrees::create(parent_path, branch, request.base_ref.as_deref())
                .map_err(WorkersError::State)?;
        let project_id = format!("comet-worktree-{}", uuid::Uuid::new_v4().simple());
        let display_name = request
            .name
            .as_deref()
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .unwrap_or(branch)
            .to_owned();
        let path = worktree.path.clone();
        let register = unpeel_core::app_state::edit(|state| {
            let projects = state
                .get_mut("projects")
                .and_then(Value::as_array_mut)
                .ok_or_else(|| "projects must be an array".to_string())?;
            projects.push(json!({
                "id": project_id,
                "name": display_name,
                "path": path,
                "workspace_id": "personal",
                "sort_order": projects.len() as u32,
                "parent_project_id": request.project_id,
                "is_folder": true,
                "worktree_branch": branch,
            }));
            Ok(())
        });
        if let Err(error) = register {
            let _ = unpeel_core::worktrees::remove(&worktree.path, true);
            return Err(WorkersError::State(error));
        }
        Ok(WorkersWorktreeResult {
            project_id,
            path: worktree.path,
            branch: branch.to_owned(),
        })
    }

    pub fn create_worktree_and_launch(
        &self,
        request: WorkersCreateWorktreeRequest,
        mut launch: WorkersLaunchRequest,
    ) -> Result<WorkersWorktreeLaunchResult, WorkersError> {
        let worktree = self.create_worktree(request)?;
        launch.project_id = worktree.project_id.clone();
        launch.worktree_path = Some(worktree.path.clone());
        launch.worktree_branch = Some(worktree.branch.clone());
        match self.launch_session(&launch) {
            Ok(session_id) => Ok(WorkersWorktreeLaunchResult {
                project_id: worktree.project_id,
                session_id,
                path: worktree.path,
                branch: worktree.branch,
            }),
            Err(error) => {
                let _ = self.remove_worktree(&worktree.project_id, true);
                Err(error)
            }
        }
    }

    pub fn set_project_organization(
        &self,
        project_id: &str,
        patch: WorkersProjectOrganizationPatch,
    ) -> Result<(), WorkersError> {
        let state = unpeel_core::app_state::load().map_err(WorkersError::State)?;
        let projects = state
            .get("projects")
            .and_then(Value::as_array)
            .ok_or_else(|| WorkersError::State("projects must be an array".into()))?;
        let project = projects
            .iter()
            .find(|project| project.get("id").and_then(Value::as_str) == Some(project_id))
            .ok_or_else(|| WorkersError::State(format!("unknown project id: {project_id}")))?;
        let parent_id = project
            .get("parent_project_id")
            .and_then(Value::as_str)
            .map(str::to_owned);

        if let Some(display_name) = patch.display_name {
            let display_name = display_name.trim();
            if display_name.is_empty() {
                return Err(WorkersError::State("project name is required".into()));
            }
            let is_worktree = project
                .get("worktree_branch")
                .and_then(Value::as_str)
                .is_some();
            if is_worktree {
                rename_project_record(project_id, display_name)?;
            } else {
                unpeel_core::session_ops::rename_group_project(project_id, display_name)
                    .map_err(WorkersError::State)?;
            }
        }
        if let Some(folder_color_id) = patch.folder_color_id {
            if parent_id.is_some() {
                return Err(WorkersError::State(
                    "only main projects can have a folder color".into(),
                ));
            }
            set_project_folder_color(project_id, folder_color_id.as_deref())?;
        }
        if let Some(session_sort) = patch.session_sort {
            unpeel_core::session_ops::set_session_date_sorted(
                project_id,
                session_sort == WorkersSessionSort::RecentlyUpdated,
            )
            .map_err(WorkersError::State)?;
        }
        if let Some(sort_order) = patch.sort_order {
            let all_ids: Vec<String> = projects
                .iter()
                .filter_map(|project| project.get("id").and_then(Value::as_str).map(str::to_owned))
                .collect();
            let mut sibling_ids: Vec<String> = projects
                .iter()
                .filter(|candidate| {
                    candidate.get("parent_project_id").and_then(Value::as_str)
                        == parent_id.as_deref()
                })
                .filter_map(|candidate| {
                    candidate
                        .get("id")
                        .and_then(Value::as_str)
                        .map(str::to_owned)
                })
                .collect();
            let from = sibling_ids
                .iter()
                .position(|id| id == project_id)
                .ok_or_else(|| WorkersError::State("project is not reorderable".into()))?;
            let id = sibling_ids.remove(from);
            let target = sort_order.min(sibling_ids.len());
            sibling_ids.insert(target, id);
            unpeel_core::session_ops::set_project_sibling_order(&sibling_ids, &all_ids)
                .map_err(WorkersError::State)?;
        }
        Ok(())
    }

    pub fn set_session_order(
        &self,
        project_id: &str,
        session_ids: &[String],
    ) -> Result<(), WorkersError> {
        unpeel_core::session_ops::set_session_order(project_id, session_ids)
            .map_err(WorkersError::State)
    }

    pub fn rename_worktree(&self, project_id: &str, name: &str) -> Result<(), WorkersError> {
        let name = name.trim();
        if name.is_empty() {
            return Err(WorkersError::State("worktree name is required".into()));
        }
        rename_project_record(project_id, name)
    }

    pub fn remove_worktree(&self, project_id: &str, force: bool) -> Result<(), WorkersError> {
        let state = unpeel_core::app_state::load().map_err(WorkersError::State)?;
        let project = state
            .get("projects")
            .and_then(Value::as_array)
            .and_then(|projects| {
                projects
                    .iter()
                    .find(|project| project.get("id").and_then(Value::as_str) == Some(project_id))
            })
            .ok_or_else(|| WorkersError::State(format!("unknown worktree id: {project_id}")))?;
        if project
            .get("worktree_branch")
            .and_then(Value::as_str)
            .is_none()
        {
            return Err(WorkersError::State("project is not a worktree".into()));
        }
        let path = project
            .get("path")
            .and_then(Value::as_str)
            .ok_or_else(|| WorkersError::State("worktree path is missing".into()))?
            .to_owned();
        unpeel_core::worktrees::remove(&path, force).map_err(WorkersError::State)?;
        let removed = project_tree_ids(project_id)?;
        self.remove_sessions_in_projects(&removed)?;
        remove_project_tree(project_id)
    }

    pub fn remove_group(&self, project_id: &str) -> Result<(), WorkersError> {
        let state = unpeel_core::app_state::load().map_err(WorkersError::State)?;
        let project = state
            .get("projects")
            .and_then(Value::as_array)
            .and_then(|projects| {
                projects
                    .iter()
                    .find(|project| project.get("id").and_then(Value::as_str) == Some(project_id))
            })
            .ok_or_else(|| WorkersError::State(format!("unknown group id: {project_id}")))?;
        let is_group = project.get("is_folder").and_then(Value::as_bool) == Some(true)
            && project
                .get("worktree_branch")
                .and_then(Value::as_str)
                .is_none()
            && project
                .get("parent_project_id")
                .and_then(Value::as_str)
                .is_some();
        if !is_group {
            return Err(WorkersError::State("project is not a group".into()));
        }
        let parent_id = project
            .get("parent_project_id")
            .and_then(Value::as_str)
            .ok_or_else(|| WorkersError::State("group parent is missing".into()))?
            .to_owned();
        for session in self
            .bootstrap()?
            .sessions
            .into_iter()
            .filter(|session| session.project_id == project_id)
        {
            unpeel_core::session_ops::set_pinned(&session.id, false)
                .map_err(WorkersError::State)?;
            unpeel_core::session_ops::set_project_override(&session.id, &parent_id)
                .map_err(WorkersError::State)?;
            unpeel_core::session_ops::archive_session(&session.id).map_err(WorkersError::State)?;
        }
        remove_project_record(project_id)
    }

    pub fn remove_project(&self, project_id: &str) -> Result<(), WorkersError> {
        let removed = project_tree_ids(project_id)?;
        self.remove_sessions_in_projects(&removed)?;
        remove_project_tree(project_id)
    }

    fn remove_sessions_in_projects(
        &self,
        project_ids: &std::collections::HashSet<String>,
    ) -> Result<(), WorkersError> {
        for session in self
            .bootstrap()?
            .sessions
            .into_iter()
            .filter(|session| project_ids.contains(&session.project_id))
        {
            unpeel_core::session_ops::remove_session(&session.id).map_err(WorkersError::State)?;
        }
        Ok(())
    }

    pub fn reveal_project(&self, path: &str) -> Result<(), WorkersError> {
        let status = std::process::Command::new("open")
            .arg("-R")
            .arg(path)
            .status()
            .map_err(|error| WorkersError::State(error.to_string()))?;
        status
            .success()
            .then_some(())
            .ok_or_else(|| WorkersError::State("Finder could not reveal the project".into()))
    }

    pub fn open_project_in_editor(&self, path: &str) -> Result<(), WorkersError> {
        let editor = unpeel_core::app_state::load()
            .ok()
            .and_then(|state| state.get("code_editor")?.as_str().map(str::to_owned))
            .unwrap_or_else(|| "code".into());
        if std::process::Command::new(&editor)
            .arg(path)
            .spawn()
            .is_ok()
        {
            return Ok(());
        }
        let status = std::process::Command::new("open")
            .arg(path)
            .status()
            .map_err(|error| WorkersError::State(error.to_string()))?;
        status
            .success()
            .then_some(())
            .ok_or_else(|| WorkersError::State(format!("could not open project in {editor}")))
    }

    pub fn open_project_with_application(
        &self,
        path: &str,
        bundle_ids: Vec<String>,
        app_names: Vec<String>,
    ) -> Result<(), WorkersError> {
        for bundle_id in bundle_ids {
            if std::process::Command::new("/usr/bin/open")
                .args(["-b", bundle_id.as_str(), path])
                .status()
                .is_ok_and(|status| status.success())
            {
                return Ok(());
            }
        }
        for app_name in app_names {
            if std::process::Command::new("/usr/bin/open")
                .args(["-a", app_name.as_str(), path])
                .status()
                .is_ok_and(|status| status.success())
            {
                return Ok(());
            }
        }
        Err(WorkersError::State(
            "the selected workspace application is unavailable".into(),
        ))
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

    pub fn session_command(
        &self,
        session: &WorkersSession,
        command: WorkersSessionCommand,
    ) -> Result<Option<String>, WorkersError> {
        let session_id = session.id.as_str();
        match command {
            WorkersSessionCommand::Stop => {
                self.session_action(session_id, SessionAction::Stop)?;
                Ok(None)
            }
            WorkersSessionCommand::RestartSession => {
                self.session_action(session_id, SessionAction::Restart)?;
                Ok(None)
            }
            WorkersSessionCommand::RestartAgent => {
                self.session_action(session_id, SessionAction::RestartAgent)?;
                Ok(None)
            }
            WorkersSessionCommand::ResumeAgent => {
                self.session_action(session_id, SessionAction::ResumeAgent)?;
                Ok(None)
            }
            WorkersSessionCommand::Fork => self.fork_session(session).map(Some),
            WorkersSessionCommand::ClearAttention => {
                self.clear_attention(session_id)?;
                Ok(None)
            }
            WorkersSessionCommand::AppendSystemContext { text } => {
                let text = text.trim();
                if text.is_empty() {
                    return Err(WorkersError::State(
                        "system context must not be blank".into(),
                    ));
                }
                self.append_system_context(session_id, Some(text))?;
                Ok(None)
            }
            WorkersSessionCommand::SetNotifyWhenDone { enabled } => {
                self.set_session_organization(
                    session_id,
                    SessionOrganizationPatch {
                        notify_when_done: Some(enabled),
                        ..Default::default()
                    },
                )?;
                Ok(None)
            }
            WorkersSessionCommand::Archive => {
                if session.is_live() {
                    self.session_action(session_id, SessionAction::Stop)?;
                }
                self.set_session_organization(
                    session_id,
                    SessionOrganizationPatch {
                        archived: Some(true),
                        ..Default::default()
                    },
                )?;
                Ok(None)
            }
            WorkersSessionCommand::Restore => {
                self.set_session_organization(
                    session_id,
                    SessionOrganizationPatch {
                        archived: Some(false),
                        ..Default::default()
                    },
                )?;
                Ok(None)
            }
            WorkersSessionCommand::RestoreAndResume => {
                self.set_session_organization(
                    session_id,
                    SessionOrganizationPatch {
                        archived: Some(false),
                        ..Default::default()
                    },
                )?;
                let resume = if session.capabilities.resume_agent {
                    SessionAction::ResumeAgent
                } else {
                    SessionAction::Restart
                };
                if let Err(error) = self.session_action(session_id, resume) {
                    let _ = self.set_session_organization(
                        session_id,
                        SessionOrganizationPatch {
                            archived: Some(true),
                            ..Default::default()
                        },
                    );
                    return Err(error);
                }
                Ok(None)
            }
            WorkersSessionCommand::Remove => {
                self.session_action(session_id, SessionAction::Remove)?;
                Ok(None)
            }
        }
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
        if let Some(notify_when_done) = patch.notify_when_done {
            set_notify_when_done_overlay(session_id, notify_when_done)?;
        }
        if let Some(project_id) = patch.project_id {
            self.move_session(session_id, project_id.as_deref())?;
        }
        if body.len() == 1 {
            return Ok(());
        }
        self.mutate("/mobile/session-organization", body.into())
    }

    pub fn clear_attention(&self, session_id: &str) -> Result<(), WorkersError> {
        if unpeel_core::session_host::load_manifest(session_id).is_none() {
            return Err(WorkersError::State(format!("unknown session {session_id}")));
        }
        self.activity.clear_attention(session_id);
        Ok(())
    }

    pub fn move_session(
        &self,
        session_id: &str,
        project_id: Option<&str>,
    ) -> Result<(), WorkersError> {
        match project_id {
            Some(project_id) => {
                unpeel_core::session_ops::set_project_override(session_id, project_id)
            }
            None => unpeel_core::session_ops::clear_project_override(session_id),
        }
        .map_err(WorkersError::State)
    }

    pub fn append_system_context(
        &self,
        session_id: &str,
        context: Option<&str>,
    ) -> Result<(), WorkersError> {
        unpeel_core::session_ops::set_appended_context(session_id, context)
            .map_err(WorkersError::State)
    }

    pub fn fork_session(&self, session: &WorkersSession) -> Result<String, WorkersError> {
        let manifest = unpeel_core::session_host::load_manifest(&session.id)
            .ok_or_else(|| WorkersError::State(format!("unknown session {}", session.id)))?;
        if !manifest.has_been_written_to {
            return Err(WorkersError::State(
                "A session must receive input before it can be forked".into(),
            ));
        }
        let (provider_session_id, _) =
            unpeel_core::session_ops::provider_session_marker(&session.id);
        let command = unpeel_core::resume::forked(
            &manifest.session.command,
            provider_session_id
                .as_deref()
                .or(manifest.provider_session_id.as_deref()),
        )
        .ok_or_else(|| WorkersError::State("This worker does not support fork".into()))?;
        let mut request = WorkersLaunchRequest::command(session.project_id.clone(), command);
        if let Some(branch) = session.worktree_branch.clone() {
            request = request.with_worktree(manifest.cwd, branch);
        }
        self.launch_session(&request)
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

    pub fn transcript_markdown(
        &self,
        session_id: &str,
        entries: Option<usize>,
    ) -> Result<String, WorkersError> {
        let mut query = vec![("session_id".to_owned(), session_id.to_owned())];
        if let Some(entries) = entries {
            query.push(("entries".to_owned(), entries.to_string()));
        }
        let body = self.request("GET", "/mobile/transcript-markdown", query, Value::Null)?;
        body.get("markdown")
            .and_then(Value::as_str)
            .map(str::to_owned)
            .ok_or_else(|| WorkersError::Protocol("transcript response missing markdown".into()))
    }

    pub fn transcript_markdown_range(
        &self,
        session_id: &str,
        range: WorkersTranscriptRange,
    ) -> Result<String, WorkersError> {
        self.transcript_markdown(session_id, Some(range.entries()))
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
        let runtime =
            ControllerHostRuntime::owner_transport("comet-local", None, self.activity.hook_port());
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
    git_branch: Option<String>,
    #[serde(default)]
    archived_session_count: usize,
    #[serde(default)]
    date_sorted: bool,
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
            git_branch: value.git_branch,
            archived_session_count: value.archived_session_count,
            folder_color_id: None,
            session_sort: if value.date_sorted {
                WorkersSessionSort::RecentlyUpdated
            } else {
                WorkersSessionSort::Custom
            },
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
            runtime_generation: 0,
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

const NOTIFY_WHEN_DONE_OVERLAY_KEY: &str = "comet_workers_notify_when_done";
const PROJECT_FOLDER_COLORS_KEY: &str = "comet_workers_project_folder_colors";

fn apply_project_organization_overlay(projects: &mut [WorkersProject]) {
    let Ok(state) = unpeel_core::app_state::load() else {
        return;
    };
    let colors = state
        .get(PROJECT_FOLDER_COLORS_KEY)
        .and_then(Value::as_object);
    for project in projects {
        project.folder_color_id = colors
            .and_then(|colors| colors.get(&project.id))
            .and_then(Value::as_str)
            .map(str::to_owned);
        if unpeel_core::session_ops::session_date_sorted(&project.id) {
            project.session_sort = WorkersSessionSort::RecentlyUpdated;
        }
    }
}

fn set_project_folder_color(project_id: &str, color_id: Option<&str>) -> Result<(), WorkersError> {
    const COLORS: &[&str] = &[
        "sky", "blue", "violet", "rose", "amber", "moss", "teal", "graphite",
    ];
    if color_id.is_some_and(|color| !COLORS.contains(&color)) {
        return Err(WorkersError::State(format!(
            "unknown folder color: {}",
            color_id.unwrap_or_default()
        )));
    }
    unpeel_core::app_state::edit(|state| {
        let values = state
            .entry(PROJECT_FOLDER_COLORS_KEY)
            .or_insert_with(|| Value::Object(serde_json::Map::new()));
        let values = values
            .as_object_mut()
            .ok_or_else(|| format!("{PROJECT_FOLDER_COLORS_KEY} must be an object"))?;
        match color_id {
            Some(color_id) => {
                values.insert(project_id.to_owned(), Value::String(color_id.to_owned()));
            }
            None => {
                values.remove(project_id);
            }
        }
        Ok(())
    })
    .map_err(WorkersError::State)
}

fn rename_project_record(project_id: &str, name: &str) -> Result<(), WorkersError> {
    unpeel_core::app_state::edit(|state| {
        let projects = state
            .get_mut("projects")
            .and_then(Value::as_array_mut)
            .ok_or_else(|| "projects must be an array".to_string())?;
        let project = projects
            .iter_mut()
            .find(|project| project.get("id").and_then(Value::as_str) == Some(project_id))
            .ok_or_else(|| format!("unknown project id: {project_id}"))?;
        project["name"] = Value::String(name.to_owned());
        Ok(())
    })
    .map_err(WorkersError::State)
}

fn remove_project_record(project_id: &str) -> Result<(), WorkersError> {
    unpeel_core::app_state::edit(|state| {
        let projects = state
            .get_mut("projects")
            .and_then(Value::as_array_mut)
            .ok_or_else(|| "projects must be an array".to_string())?;
        let before = projects.len();
        projects.retain(|project| project.get("id").and_then(Value::as_str) != Some(project_id));
        if projects.len() == before {
            return Err(format!("unknown project id: {project_id}"));
        }
        if let Some(colors) = state
            .get_mut(PROJECT_FOLDER_COLORS_KEY)
            .and_then(Value::as_object_mut)
        {
            colors.remove(project_id);
        }
        if let Some(modes) = state
            .get_mut("session_sort_modes")
            .and_then(Value::as_object_mut)
        {
            modes.remove(project_id);
        }
        Ok(())
    })
    .map_err(WorkersError::State)
}

fn remove_project_tree(project_id: &str) -> Result<(), WorkersError> {
    unpeel_core::app_state::edit(|state| {
        let projects = state
            .get_mut("projects")
            .and_then(Value::as_array_mut)
            .ok_or_else(|| "projects must be an array".to_string())?;
        if !projects
            .iter()
            .any(|project| project.get("id").and_then(Value::as_str) == Some(project_id))
        {
            return Err(format!("unknown project id: {project_id}"));
        }
        let mut removed = std::collections::HashSet::from([project_id.to_owned()]);
        loop {
            let before = removed.len();
            for project in projects.iter() {
                if project
                    .get("parent_project_id")
                    .and_then(Value::as_str)
                    .is_some_and(|parent| removed.contains(parent))
                {
                    if let Some(id) = project.get("id").and_then(Value::as_str) {
                        removed.insert(id.to_owned());
                    }
                }
            }
            if removed.len() == before {
                break;
            }
        }
        projects.retain(|project| {
            project
                .get("id")
                .and_then(Value::as_str)
                .is_none_or(|id| !removed.contains(id))
        });
        for key in [PROJECT_FOLDER_COLORS_KEY, "session_sort_modes"] {
            if let Some(map) = state.get_mut(key).and_then(Value::as_object_mut) {
                map.retain(|id, _| !removed.contains(id));
            }
        }
        Ok(())
    })
    .map_err(WorkersError::State)
}

fn project_tree_ids(project_id: &str) -> Result<std::collections::HashSet<String>, WorkersError> {
    let state = unpeel_core::app_state::load().map_err(WorkersError::State)?;
    let projects = state
        .get("projects")
        .and_then(Value::as_array)
        .ok_or_else(|| WorkersError::State("projects must be an array".into()))?;
    if !projects
        .iter()
        .any(|project| project.get("id").and_then(Value::as_str) == Some(project_id))
    {
        return Err(WorkersError::State(format!(
            "unknown project id: {project_id}"
        )));
    }
    let mut removed = std::collections::HashSet::from([project_id.to_owned()]);
    loop {
        let before = removed.len();
        for project in projects {
            if project
                .get("parent_project_id")
                .and_then(Value::as_str)
                .is_some_and(|parent| removed.contains(parent))
            {
                if let Some(id) = project.get("id").and_then(Value::as_str) {
                    removed.insert(id.to_owned());
                }
            }
        }
        if removed.len() == before {
            return Ok(removed);
        }
    }
}

fn apply_notify_when_done_overlay(sessions: &mut [WorkersSession]) {
    let Ok(state) = unpeel_core::app_state::load() else {
        return;
    };
    let Some(values) = state
        .get(NOTIFY_WHEN_DONE_OVERLAY_KEY)
        .and_then(Value::as_object)
    else {
        return;
    };
    for session in sessions {
        if let Some(enabled) = values.get(&session.id).and_then(Value::as_bool) {
            session.notify_when_done = enabled;
        }
    }
}

fn apply_runtime_capabilities(sessions: &mut [WorkersSession]) {
    use unpeel_core::runtime_catalog::RuntimeCapability;

    let catalog = unpeel_core::runtime_catalog::builtin_runtime_catalog();
    for session in sessions {
        let command = unpeel_core::integrations::command_head(&session.command);
        let Some(runtime) = catalog.by_command_alias_for_current_platform(command) else {
            continue;
        };
        session.capabilities.notify_when_done = runtime
            .capabilities
            .contains(&RuntimeCapability::NotifyWhenDone);
    }
}

fn set_notify_when_done_overlay(session_id: &str, enabled: bool) -> Result<(), WorkersError> {
    unpeel_core::app_state::edit(|state| {
        let values = state
            .entry(NOTIFY_WHEN_DONE_OVERLAY_KEY)
            .or_insert_with(|| Value::Object(serde_json::Map::new()));
        let values = values
            .as_object_mut()
            .ok_or_else(|| format!("{NOTIFY_WHEN_DONE_OVERLAY_KEY} must be an object"))?;
        if enabled {
            values.insert(session_id.to_owned(), Value::Bool(true));
        } else {
            values.remove(session_id);
        }
        Ok(())
    })
    .map_err(WorkersError::State)
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

const COMET_WORKERS_PRESET_CATALOG_VERSION_KEY: &str = "comet_workers_preset_catalog_version";
const COMET_WORKERS_PRESET_CATALOG_VERSION: u64 = 1;
const COMET_WORKERS_PRESET_V1_IDS: [&str; 2] = ["omp", "prime-agent"];

fn migrate_comet_workers_presets() -> Result<Value, WorkersError> {
    let raw = unpeel_core::app_state::load().map_err(WorkersError::State)?;
    let current_version = raw
        .get(COMET_WORKERS_PRESET_CATALOG_VERSION_KEY)
        .and_then(Value::as_u64)
        .unwrap_or_default();
    if current_version >= COMET_WORKERS_PRESET_CATALOG_VERSION {
        return Ok(raw);
    }

    let has_presets = raw
        .get("presets")
        .and_then(Value::as_array)
        .is_some_and(|presets| !presets.is_empty());
    let native_presets_were_seeded = raw
        .get("native_preset_overlay_migrated")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    if !has_presets && !native_presets_were_seeded {
        return Ok(raw);
    }

    unpeel_core::app_state::edit(|state| {
        let current_version = state
            .get(COMET_WORKERS_PRESET_CATALOG_VERSION_KEY)
            .and_then(Value::as_u64)
            .unwrap_or_default();
        if current_version >= COMET_WORKERS_PRESET_CATALOG_VERSION {
            return Ok(());
        }

        let mut presets = state
            .get("presets")
            .cloned()
            .map(serde_json::from_value::<Vec<unpeel_core::state::Preset>>)
            .transpose()
            .map_err(|error| error.to_string())?
            .unwrap_or_default();
        for builtin in unpeel_core::state::builtin_global_presets()
            .into_iter()
            .filter(|preset| COMET_WORKERS_PRESET_V1_IDS.contains(&preset.id.as_str()))
        {
            if presets.iter().all(|preset| preset.id != builtin.id) {
                presets.push(builtin);
            }
        }
        state.insert(
            "presets".into(),
            serde_json::to_value(presets).map_err(|error| error.to_string())?,
        );
        state.insert(
            COMET_WORKERS_PRESET_CATALOG_VERSION_KEY.into(),
            Value::from(COMET_WORKERS_PRESET_CATALOG_VERSION),
        );
        Ok(())
    })
    .map_err(WorkersError::State)?;

    unpeel_core::app_state::load().map_err(WorkersError::State)
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

#[derive(Debug, Clone, PartialEq, Eq)]
struct TrustedRuntimeInstall {
    cli_id: String,
    command: String,
}

fn trusted_runtime_install(cli_id: &str) -> Option<TrustedRuntimeInstall> {
    let catalog = unpeel_core::runtime_catalog::builtin_runtime_catalog();
    let runtime = catalog.by_legacy_slug_for_current_platform(cli_id)?;
    let command = runtime.install.as_ref()?.command.as_ref()?.trim();
    if command.is_empty() {
        return None;
    }
    Some(TrustedRuntimeInstall {
        cli_id: runtime.legacy_slug.clone(),
        command: command.to_owned(),
    })
}

fn install_failure_message(status: i32, output: &str) -> String {
    let lines = output
        .lines()
        .map(str::trim)
        .filter(|line| {
            !line.is_empty() && !line.contains("A complete log of this run can be found in")
        })
        .collect::<Vec<_>>();
    let start = lines.len().saturating_sub(5);
    let tail = lines[start..].join("\n");
    if tail.is_empty() {
        format!("Install failed (exit {status})")
    } else {
        tail
    }
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

#[cfg(test)]
mod host_mode_tests {
    use unpeel_core::{browser_mcp, computer_mcp, mcp_gate, mcp_host, session_host};

    use super::is_session_host_mode;

    #[test]
    fn every_detached_unpeel_helper_is_routed_before_the_app_starts() {
        for argument in [
            session_host::SESSION_HOST_ARG,
            session_host::COMPACT_OUTPUT_JOURNALS_ARG,
            mcp_host::MCP_HOST_ARG,
            browser_mcp::BROWSER_MCP_ARG,
            mcp_gate::MCP_GATE_ARG,
            browser_mcp::BROWSER_CLEANUP_ARG,
            computer_mcp::COMPUTER_CLEANUP_ARG,
        ] {
            assert!(
                is_session_host_mode(&[argument.to_owned()]),
                "{argument} must not fall through into the desktop app"
            );
        }
        assert!(!is_session_host_mode(&["--help".to_owned()]));
    }
}

#[cfg(test)]
mod runtime_capability_tests {
    use super::{WorkersSession, WorkersSessionCapabilities, apply_runtime_capabilities};

    fn session(command: &str) -> WorkersSession {
        WorkersSession {
            id: command.to_owned(),
            project_id: "project".to_owned(),
            title: command.to_owned(),
            command: command.to_owned(),
            state: "running".to_owned(),
            activity: "idle".to_owned(),
            unread: false,
            pinned: false,
            archived: false,
            provider_id: None,
            active_runtime_id: None,
            runtime_launch_pending: false,
            runtime_generation: 0,
            notify_when_done: false,
            terminal_background_hex: None,
            worktree_branch: None,
            created_at_unix_ms: 0,
            updated_at_unix_ms: 0,
            capabilities: WorkersSessionCapabilities::default(),
        }
    }

    #[test]
    fn completion_notifications_follow_the_pinned_runtime_catalog() {
        let mut sessions = vec![
            session("claude"),
            session("pi"),
            session("omp"),
            session("prime-agent"),
        ];

        apply_runtime_capabilities(&mut sessions);

        assert!(sessions[0].capabilities.notify_when_done);
        assert!(!sessions[1].capabilities.notify_when_done);
        assert!(sessions[2].capabilities.notify_when_done);
        assert!(sessions[3].capabilities.notify_when_done);
    }
}

#[cfg(test)]
mod activity_log_tests {
    use unpeel_core::activity_log::{ActivityLogEntry, ActivityLogKind};

    use super::{WorkersActivityLogEntry, WorkersActivityLogKind};

    #[test]
    fn bootstrap_activity_log_preserves_upstream_history_fields() {
        let dto = WorkersActivityLogEntry::from(ActivityLogEntry {
            id: "event-1".to_owned(),
            session_id: "session-1".to_owned(),
            kind: ActivityLogKind::Finished,
            at: 1_234,
            title: "Ship it".to_owned(),
            command: "claude".to_owned(),
            project_id: "project-1".to_owned(),
            project_name: "Project One".to_owned(),
        });
        assert_eq!(dto.id, "event-1");
        assert_eq!(dto.session_id, "session-1");
        assert_eq!(dto.kind, WorkersActivityLogKind::Finished);
        assert_eq!(dto.at_unix_ms, 1_234);
        assert_eq!(dto.title, "Ship it");
        assert_eq!(dto.command, "claude");
        assert_eq!(dto.project_id, "project-1");
        assert_eq!(dto.project_name, "Project One");
    }
}

#[cfg(test)]
mod terminal_viewport_tests {
    use unpeel_core::terminal_viewport::{
        TerminalViewportRow, TerminalViewportSnapshot, TerminalViewportStyleRun,
    };

    use super::{WorkersViewportInputModes, viewport_input_modes, viewport_snapshot_to_ansi};

    fn snapshot(rows: Vec<TerminalViewportRow>) -> TerminalViewportSnapshot {
        TerminalViewportSnapshot {
            cols: 40,
            rows: rows.len() as u16,
            output_offset: 123,
            truncated: false,
            cursor_row: 1,
            cursor_col: 2,
            scrollback_rows: 0,
            viewport_start_row: 0,
            scroll_offset_rows: 0,
            input_modes_known: true,
            mouse_reporting: false,
            mouse_button_motion: false,
            mouse_any_motion: false,
            alternate_screen: true,
            mouse_alternate_scroll: false,
            application_cursor: true,
            viewport_rows: rows,
        }
    }

    #[test]
    fn ghostty_viewport_keeps_the_first_visible_row_at_the_top() {
        let ansi = viewport_snapshot_to_ansi(&snapshot(vec![
            TerminalViewportRow {
                text: "Welcome to Kimi Code CLI!".into(),
                styles: Vec::new(),
                wrapped: false,
            },
            TerminalViewportRow {
                text: "Directory: ~/project".into(),
                styles: Vec::new(),
                wrapped: false,
            },
        ]));

        let welcome = ansi
            .windows(b"Welcome to Kimi Code CLI!".len())
            .position(|window| window == b"Welcome to Kimi Code CLI!")
            .expect("welcome row must be present");
        let directory = ansi
            .windows(b"Directory: ~/project".len())
            .position(|window| window == b"Directory: ~/project")
            .expect("directory row must be present");

        assert!(ansi.starts_with(b"\x1b[?1049h\x1b[2J\x1b[H\x1b[?7l\x1b[1;1H"));
        assert!(welcome < directory);
        assert!(ansi.ends_with(b"\x1b[?7h\x1b[?1h\x1b[2;3H"));
    }

    #[test]
    fn ghostty_input_modes_are_forwarded_without_ansi_inference() {
        let snapshot = snapshot(Vec::new());

        assert_eq!(
            viewport_input_modes(&snapshot),
            WorkersViewportInputModes {
                known: true,
                mouse_reporting: false,
                mouse_button_motion: false,
                mouse_any_motion: false,
                alternate_screen: true,
                mouse_alternate_scroll: false,
                application_cursor: true,
            }
        );
    }

    #[test]
    fn ghostty_cell_style_runs_are_preserved_in_the_rebuilt_viewport() {
        let ansi = viewport_snapshot_to_ansi(&snapshot(vec![TerminalViewportRow {
            text: "A界B".into(),
            styles: vec![TerminalViewportStyleRun {
                start: 1,
                len: 2,
                fg: Some("rgb:1,2,3".into()),
                bg: Some("ansi256:42".into()),
                bold: true,
                inverse: false,
            }],
            wrapped: false,
        }]));

        let ansi = String::from_utf8(ansi).expect("viewport ANSI must remain valid UTF-8");
        assert!(ansi.contains("\x1b[0;1;38;2;1;2;3;48;5;42m界"));
    }
}

#[cfg(test)]
mod launch_grid_tests {
    use super::LocalWorkersClient;

    #[test]
    fn last_displayed_grid_is_shared_by_independent_clients() {
        let terminal_client = LocalWorkersClient::new();
        let model_client = LocalWorkersClient::new();

        terminal_client.remember_grid(224, 48);

        assert_eq!(model_client.remembered_grid(), Some((224, 48)));
    }
}

#[cfg(test)]
mod runtime_install_tests {
    use super::{install_failure_message, trusted_runtime_install};

    #[test]
    fn install_failure_uses_the_last_five_meaningful_lines() {
        let output = "first\nsecond\nthird\nfourth\nfifth\nsixth\n\
            A complete log of this run can be found in /tmp/npm.log\n";

        assert_eq!(
            install_failure_message(17, output),
            "second\nthird\nfourth\nfifth\nsixth"
        );
    }

    #[test]
    fn install_failure_falls_back_to_the_exit_status() {
        assert_eq!(
            install_failure_message(9, "\n\n"),
            "Install failed (exit 9)"
        );
    }

    #[test]
    fn runtime_install_commands_only_come_from_the_pinned_catalog() {
        let runtime = trusted_runtime_install("claude").expect("Claude is installable");
        assert_eq!(runtime.cli_id, "claude");
        assert!(!runtime.command.trim().is_empty());
        assert!(trusted_runtime_install("unknown-provider").is_none());
    }
}

#[cfg(test)]
mod session_gallery_tests {
    use super::{WorkersAppearanceSettings, is_image_artifact_name};

    #[test]
    fn session_gallery_is_disabled_by_default() {
        assert!(!WorkersAppearanceSettings::default().show_session_gallery);
    }

    #[test]
    fn gallery_recognizes_unpeels_supported_image_extensions_case_insensitively() {
        for name in [
            "shot.png",
            "photo.JPG",
            "image.jpeg",
            "anim.GIF",
            "capture.webp",
        ] {
            assert!(is_image_artifact_name(name), "expected image: {name}");
        }
        assert!(!is_image_artifact_name("transcript.txt"));
        assert!(!is_image_artifact_name("no-extension"));
    }
}
