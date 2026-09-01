use crate::session_host::HostedSessionManifest;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

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

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StoredSessionTelemetry {
    provider_session_id: String,
    provider_transcript_path: String,
    #[serde(flatten)]
    telemetry: SessionTelemetry,
}

#[derive(Debug)]
pub enum SessionTelemetryReadError {
    Rejected(String),
    Unavailable(String),
}

pub type ReadSessionTelemetry =
    fn(&HostedSessionManifest) -> Result<Option<SessionTelemetry>, SessionTelemetryReadError>;

#[derive(Clone, Debug, PartialEq, Eq)]
struct ProviderBinding {
    session_id: String,
    transcript_path: PathBuf,
}

fn marker_path(session_dir: &Path) -> std::path::PathBuf {
    session_dir.join(TELEMETRY_MARKER)
}

fn store_at(
    session_dir: &Path,
    binding: &ProviderBinding,
    telemetry: &SessionTelemetry,
) -> Result<(), String> {
    std::fs::create_dir_all(session_dir).map_err(|error| error.to_string())?;
    let temporary = session_dir.join(format!(
        ".session-telemetry.{}.{}.tmp",
        std::process::id(),
        uuid::Uuid::new_v4()
    ));
    let body = serde_json::to_vec(&StoredSessionTelemetry {
        provider_session_id: binding.session_id.clone(),
        provider_transcript_path: binding.transcript_path.to_string_lossy().into_owned(),
        telemetry: telemetry.clone(),
    })
    .map_err(|error| error.to_string())?;
    std::fs::write(&temporary, body).map_err(|error| error.to_string())?;
    if let Err(error) = std::fs::rename(&temporary, marker_path(session_dir)) {
        let _ = std::fs::remove_file(temporary);
        return Err(error.to_string());
    }
    Ok(())
}

fn load_at(session_dir: &Path, binding: Option<&ProviderBinding>) -> Option<SessionTelemetry> {
    let binding = binding?;
    let raw = std::fs::read(marker_path(session_dir)).ok()?;
    let stored = serde_json::from_slice::<StoredSessionTelemetry>(&raw).ok()?;
    (stored.provider_session_id == binding.session_id
        && Path::new(&stored.provider_transcript_path) == binding.transcript_path)
        .then_some(stored.telemetry)
}

fn refresh_at(
    session_dir: &Path,
    binding: Option<&ProviderBinding>,
    read: impl FnOnce() -> Result<Option<SessionTelemetry>, SessionTelemetryReadError>,
) -> Result<Option<SessionTelemetry>, String> {
    let telemetry = match read() {
        Ok(telemetry) => telemetry,
        Err(SessionTelemetryReadError::Rejected(error)) => {
            let _ = std::fs::remove_file(marker_path(session_dir));
            return Err(error);
        }
        Err(SessionTelemetryReadError::Unavailable(error)) => return Err(error),
    };
    match &telemetry {
        Some(telemetry) => store_at(
            session_dir,
            binding.ok_or_else(|| "Session telemetry has no provider binding".to_string())?,
            telemetry,
        )?,
        None => {
            let _ = std::fs::remove_file(marker_path(session_dir));
        }
    }
    Ok(telemetry)
}

fn raw_binding(manifest: &HostedSessionManifest) -> Option<(String, String)> {
    let (provider_session_id, provider_transcript_path) =
        crate::session_ops::provider_session_marker(&manifest.session.id);
    Some((
        provider_session_id.or_else(|| manifest.provider_session_id.clone())?,
        provider_transcript_path.or_else(|| manifest.provider_transcript_path.clone())?,
    ))
}

fn canonical_binding(raw: &(String, String)) -> Result<ProviderBinding, String> {
    Ok(ProviderBinding {
        session_id: raw.0.clone(),
        transcript_path: std::fs::canonicalize(&raw.1).map_err(|error| error.to_string())?,
    })
}

pub fn refresh(manifest: &HostedSessionManifest) -> Result<Option<SessionTelemetry>, String> {
    let session_dir = crate::session_host::session_dir(&manifest.session.id);
    let initial_raw_binding = raw_binding(manifest);
    let reader = crate::integrations::integration_for_command(&manifest.session.command)
        .and_then(|integration| integration.read_session_telemetry);
    let result = match reader {
        Some(read) => read(manifest),
        None => Ok(None),
    };
    let current_raw_binding = raw_binding(manifest);
    if current_raw_binding != initial_raw_binding {
        return Err("provider Session binding changed during telemetry refresh".into());
    }
    let binding = initial_raw_binding
        .as_ref()
        .map(canonical_binding)
        .transpose();
    let binding = match binding {
        Ok(binding) => binding,
        Err(error) => {
            if matches!(result, Err(SessionTelemetryReadError::Rejected(_))) {
                let _ = std::fs::remove_file(marker_path(&session_dir));
            }
            return Err(error);
        }
    };
    refresh_at(&session_dir, binding.as_ref(), || {
        let telemetry = match reader {
            Some(_) => result,
            None => Ok(None),
        };
        telemetry
    })
}

pub fn load(session_id: &str) -> Option<SessionTelemetry> {
    let (provider_session_id, provider_transcript_path) =
        crate::session_ops::provider_session_marker(session_id);
    let manifest = crate::session_host::load_manifest(session_id);
    let raw_binding = Some((
        provider_session_id.or_else(|| {
            manifest
                .as_ref()
                .and_then(|manifest| manifest.provider_session_id.clone())
        })?,
        provider_transcript_path.or_else(|| {
            manifest
                .as_ref()
                .and_then(|manifest| manifest.provider_transcript_path.clone())
        })?,
    ));
    let binding = raw_binding.and_then(|binding| canonical_binding(&binding).ok());
    load_at(
        &crate::session_host::session_dir(session_id),
        binding.as_ref(),
    )
}

pub fn invalidate(session_id: &str) -> Result<(), String> {
    match std::fs::remove_file(marker_path(&crate::session_host::session_dir(session_id))) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.to_string()),
    }
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

    fn binding(directory: &Path, session_id: &str) -> ProviderBinding {
        let transcript = directory.join(format!("{session_id}.jsonl"));
        std::fs::write(&transcript, "{}\n").expect("write transcript");
        ProviderBinding {
            session_id: session_id.into(),
            transcript_path: std::fs::canonicalize(transcript).expect("canonical transcript"),
        }
    }

    #[test]
    fn telemetry_marker_is_replaced_atomically() {
        let session_dir = tempfile::tempdir().expect("Worker Session directory");
        let binding = binding(session_dir.path(), "omp-A");
        store_at(session_dir.path(), &binding, &fixture()).expect("store telemetry");
        let replacement = SessionTelemetry {
            total_tokens: 84,
            models: vec![ModelTokenUsage {
                model: "provider/model:high".into(),
                total_tokens: 84,
                active: true,
            }],
        };

        store_at(session_dir.path(), &binding, &replacement).expect("replace telemetry");

        assert_eq!(
            load_at(session_dir.path(), Some(&binding)),
            Some(replacement)
        );
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
        let binding = binding(session_dir.path(), "omp-A");
        store_at(session_dir.path(), &binding, &original).expect("store telemetry");

        let result = refresh_at(session_dir.path(), Some(&binding), || {
            Err(SessionTelemetryReadError::Unavailable(
                "provider read failed".into(),
            ))
        });

        assert!(result.is_err());
        assert_eq!(load_at(session_dir.path(), Some(&binding)), Some(original));
    }

    #[test]
    fn telemetry_from_an_old_provider_binding_is_not_loaded() {
        let session_dir = tempfile::tempdir().expect("Worker Session directory");
        let old_binding = binding(session_dir.path(), "omp-A");
        let new_binding = binding(session_dir.path(), "omp-B");
        refresh_at(session_dir.path(), Some(&old_binding), || {
            Ok(Some(fixture()))
        })
        .expect("refresh telemetry");

        assert_eq!(load_at(session_dir.path(), Some(&new_binding)), None);
    }
}
