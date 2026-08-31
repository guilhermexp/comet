use crate::session_host::HostedSessionManifest;
use serde::{Deserialize, Serialize};
use std::path::Path;

const TELEMETRY_MARKER: &str = "session-telemetry.json";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelTokenUsage {
    pub model: String,
    pub total_tokens: u64,
    pub active: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionTelemetry {
    pub total_tokens: u64,
    pub models: Vec<ModelTokenUsage>,
}

pub type ReadSessionTelemetry =
    fn(&HostedSessionManifest) -> Result<Option<SessionTelemetry>, String>;

fn marker_path(session_dir: &Path) -> std::path::PathBuf {
    session_dir.join(TELEMETRY_MARKER)
}

fn store_at(session_dir: &Path, telemetry: &SessionTelemetry) -> Result<(), String> {
    std::fs::create_dir_all(session_dir).map_err(|error| error.to_string())?;
    let temporary = session_dir.join(format!(
        ".session-telemetry.{}.{}.tmp",
        std::process::id(),
        uuid::Uuid::new_v4()
    ));
    let body = serde_json::to_vec(telemetry).map_err(|error| error.to_string())?;
    std::fs::write(&temporary, body).map_err(|error| error.to_string())?;
    if let Err(error) = std::fs::rename(&temporary, marker_path(session_dir)) {
        let _ = std::fs::remove_file(temporary);
        return Err(error.to_string());
    }
    Ok(())
}

fn load_at(session_dir: &Path) -> Option<SessionTelemetry> {
    let raw = std::fs::read(marker_path(session_dir)).ok()?;
    serde_json::from_slice(&raw).ok()
}

fn refresh_at(
    session_dir: &Path,
    read: impl FnOnce() -> Result<Option<SessionTelemetry>, String>,
) -> Result<Option<SessionTelemetry>, String> {
    let telemetry = read()?;
    match &telemetry {
        Some(telemetry) => store_at(session_dir, telemetry)?,
        None => {
            let _ = std::fs::remove_file(marker_path(session_dir));
        }
    }
    Ok(telemetry)
}

pub fn refresh(manifest: &HostedSessionManifest) -> Result<Option<SessionTelemetry>, String> {
    let session_dir = crate::session_host::session_dir(&manifest.session.id);
    let reader = crate::integrations::integration_for_command(&manifest.session.command)
        .and_then(|integration| integration.read_session_telemetry);
    refresh_at(&session_dir, || match reader {
        Some(read) => read(manifest),
        None => Ok(None),
    })
}

pub fn load(session_id: &str) -> Option<SessionTelemetry> {
    load_at(&crate::session_host::session_dir(session_id))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> SessionTelemetry {
        SessionTelemetry {
            total_tokens: 42,
            models: vec![ModelTokenUsage {
                model: "provider/model:high".into(),
                total_tokens: 42,
                active: true,
            }],
        }
    }

    #[test]
    fn telemetry_marker_is_replaced_atomically() {
        let session_dir = tempfile::tempdir().expect("Worker Session directory");
        store_at(session_dir.path(), &fixture()).expect("store telemetry");
        let replacement = SessionTelemetry {
            total_tokens: 84,
            models: vec![ModelTokenUsage {
                model: "provider/model:high".into(),
                total_tokens: 84,
                active: true,
            }],
        };

        store_at(session_dir.path(), &replacement).expect("replace telemetry");

        assert_eq!(load_at(session_dir.path()), Some(replacement));
        assert!(std::fs::read_dir(session_dir.path())
            .expect("read Session directory")
            .all(|entry| !entry
                .expect("Session entry")
                .file_name()
                .to_string_lossy()
                .ends_with(".tmp")));
    }

    #[test]
    fn failed_refresh_preserves_the_last_valid_marker() {
        let session_dir = tempfile::tempdir().expect("Worker Session directory");
        let original = fixture();
        store_at(session_dir.path(), &original).expect("store telemetry");

        let result = refresh_at(session_dir.path(), || Err("provider read failed".into()));

        assert!(result.is_err());
        assert_eq!(load_at(session_dir.path()), Some(original));
    }
}
