//! Device-local discovery metadata and the browser's live preview list.
use serde::{Deserialize, Serialize};

pub const PREVIEW_PROXY_PORT: u16 = 7331;

/// Stamped by the engine into the environment of every process it launches:
/// [`chat_origin`] for agent runs, [`terminal_origin`] for its terminals
/// (project actions run in one). Children inherit it, through `nohup`,
/// daemonizing and an engine restart alike, so preview discovery lists
/// exactly the servers Zeron started and knows which chat started them. One
/// variable, so the nearest Zeron launch always overwrites an outer one.
pub const ORIGIN_ENV: &str = "ZERON_ORIGIN";

pub fn chat_origin(chat_id: &str) -> String {
    format!("chat:{chat_id}")
}

pub fn terminal_origin(terminal_id: &str) -> String {
    format!("terminal:{terminal_id}")
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PreviewService {
    /// Persisted identities; neither includes a listening port or process ID.
    pub id: String,
    pub project_id: String,
    pub project_name: String,
    pub project_cwd: String,
    pub device_id: String,
    pub device_name: String,
    pub hostname: String,
    pub name: String,
    pub port: u16,
    pub pid: u32,
    pub cwd: String,
    /// Process creation time, milliseconds since the Unix epoch.
    pub started_at: u64,
    pub zeron_owned: bool,
    /// The chat whose agent or terminal started it (see [`ORIGIN_ENV`]).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chat_id: Option<String>,
}

impl PreviewService {
    pub fn url(&self, proxy_port: u16) -> String {
        format!("http://{}:{proxy_port}", self.hostname)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PreviewSnapshot {
    pub services: Vec<PreviewService>,
    pub proxy_port: u16,
    pub error: Option<String>,
    #[serde(default)]
    pub project_name: Option<String>,
    #[serde(default)]
    pub remote: bool,
}

impl Default for PreviewSnapshot {
    fn default() -> Self {
        Self {
            services: Vec::new(),
            proxy_port: PREVIEW_PROXY_PORT,
            error: None,
            project_name: None,
            remote: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WatchPreviewsParams {
    pub chat_id: String,
}

/// A TCP listener running inside one of the requested project roots on the
/// host device — the Details "Servers" list. Unlike [`PreviewService`] this
/// is not limited to HTTP or to the chat's own project.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DevServer {
    pub pid: u32,
    /// Process creation time, milliseconds since the Unix epoch.
    pub started_at: u64,
    pub port: u16,
    /// The listener process's working directory.
    pub cwd: String,
    /// The requested root that owns `cwd`.
    pub root: String,
    /// Command line, for the row's label.
    pub command: String,
    /// Dev-server phrases from the launch chain (`npm run dev`, `next dev`),
    /// matched against the chat's own commands to tell what its agent started.
    #[serde(default)]
    pub signatures: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListDevServersParams {
    pub cwds: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StopDevServerParams {
    pub pid: u32,
    pub started_at: u64,
    pub root: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StopDevServerReply {
    /// Every process signalled: the listener plus its in-project launchers.
    pub stopped: Vec<u32>,
}
