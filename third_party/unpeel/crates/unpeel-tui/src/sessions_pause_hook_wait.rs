use std::time::SystemTime;

use unpeel_core::session_host::HostedSessionManifest;

use super::super::{derive_status, Status};
use crate::activity::ActivityEngine;

#[test]
fn pause_hook_wait_settled_interrupt_reports_idle_without_confirming_stop() {
    let mut manifest: HostedSessionManifest = serde_json::from_value(serde_json::json!({
        "session": {
            "id": "codex-interrupt",
            "project_id": "project",
            "label": "Codex",
            "command": "codex"
        },
        "cwd": "/tmp",
        "state": "running",
        "pid": 123,
        "exit_code": null
    }))
    .expect("manifest decodes");
    let dir = std::env::temp_dir().join("unpeel-pause-hook-wait-no-seed");
    let mut engine = ActivityEngine::default();
    let now = SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(100);
    engine.apply_hook_event("codex-interrupt", "Start", None, now);

    assert_eq!(
        derive_status(&mut engine, &manifest, true, &dir, now),
        Status::Busy
    );
    manifest.turn_settled_by_interrupt = true;
    assert_eq!(
        derive_status(&mut engine, &manifest, true, &dir, now),
        Status::Idle
    );
    assert!(
        !engine.hook_confirmed_idle("codex-interrupt"),
        "interrupt must not confirm Stop"
    );
    manifest.turn_settled_by_interrupt = false;
    assert_eq!(
        derive_status(&mut engine, &manifest, true, &dir, now),
        Status::Busy,
        "clearing the interrupt evidence must rearm the live turn"
    );
}
