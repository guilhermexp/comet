//! Comet-facing adapter for the pinned Unpeel local worker runtime.

use std::collections::HashMap;
use std::sync::atomic::AtomicBool;

use serde::Deserialize;
use thiserror::Error;
use unpeel_core::controller_host::ControllerHostRuntime;
use unpeel_core::relay_crypto::TunnelRequest;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkersBootstrap {
    pub protocol: WorkersProtocol,
    pub projects: Vec<WorkersProject>,
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
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkersSession {
    pub id: String,
    pub project_id: String,
    pub title: String,
    pub state: String,
}

#[derive(Debug, Error)]
pub enum WorkersError {
    #[error("Unpeel bootstrap failed with status {status}: {message}")]
    Upstream { status: u16, message: String },
    #[error("Unpeel returned an invalid bootstrap response: {0}")]
    InvalidResponse(#[from] serde_json::Error),
}

#[derive(Debug, Default, Clone, Copy)]
pub struct LocalWorkersClient;

impl LocalWorkersClient {
    pub const fn new() -> Self {
        Self
    }

    pub fn bootstrap(&self) -> Result<WorkersBootstrap, WorkersError> {
        let runtime = ControllerHostRuntime::owner_transport("comet-local", None, None);
        let request = TunnelRequest {
            id: 1,
            method: "GET".to_owned(),
            path: "/mobile/bootstrap".to_owned(),
            query: Vec::new(),
            auth: None,
            content_type: None,
            body: Vec::new(),
        };
        let response = runtime.handle_tunnel("comet-workers", request, &AtomicBool::new(false));

        if response.status != 200 {
            let message = response
                .body
                .get("error")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("unknown upstream error")
                .to_owned();
            return Err(WorkersError::Upstream {
                status: response.status,
                message,
            });
        }

        let wire: BootstrapWire = serde_json::from_value(response.body)?;
        Ok(WorkersBootstrap {
            protocol: WorkersProtocol {
                major_version: wire.host_protocol.major_version,
                minor_version: wire.host_protocol.minor_version,
                capabilities: wire.host_protocol.capabilities,
            },
            projects: wire
                .projects
                .into_iter()
                .map(|project| WorkersProject {
                    id: project.id,
                    name: project.name,
                    path: project.path,
                })
                .collect(),
            sessions: wire
                .sessions
                .into_iter()
                .map(|session| WorkersSession {
                    id: session.id,
                    project_id: session.project_id,
                    title: session.title,
                    state: session.status,
                })
                .collect(),
        })
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BootstrapWire {
    host_protocol: ProtocolWire,
    #[serde(default)]
    projects: Vec<ProjectWire>,
    #[serde(default)]
    sessions: Vec<SessionWire>,
    #[serde(flatten)]
    _remaining: HashMap<String, serde_json::Value>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProtocolWire {
    major_version: u16,
    minor_version: u16,
    capabilities: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct ProjectWire {
    id: String,
    name: String,
    path: String,
}

#[derive(Debug, Deserialize)]
struct SessionWire {
    id: String,
    #[serde(rename = "projectID")]
    project_id: String,
    title: String,
    status: String,
}
