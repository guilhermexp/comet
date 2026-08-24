use super::{ResourceSupport, WorkersResourceSnapshot, unix_time_ms};

pub(super) fn snapshot() -> WorkersResourceSnapshot {
    WorkersResourceSnapshot {
        support: ResourceSupport::Unsupported,
        sampled_at_unix_ms: unix_time_ms(),
        sessions: Vec::new(),
        error: Some("worker resource attribution is currently supported on macOS only".into()),
    }
}
