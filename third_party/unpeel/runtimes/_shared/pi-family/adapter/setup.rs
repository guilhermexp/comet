use crate::app_paths::unpeel_home;
use crate::hook_assets::{
    notify_hook_script_path, write_executable_script, write_file_atomic, NOTIFY_HOOK_SCRIPT,
};
use std::path::PathBuf;

const LIFECYCLE_EXTENSION: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../runtimes/_shared/pi-family/assets/lifecycle-extension.js"
));

pub(crate) fn lifecycle_extension_path() -> PathBuf {
    unpeel_home()
        .join("hooks")
        .join("pi-family-lifecycle-extension.js")
}

/// Append `--extension <lifecycle extension>` exactly once.
///
/// Every pi-family CLI (`pi`, `omp`, `prime-agent`) takes `-e/--extension` and
/// runs the same extension API (`agent_start`/`agent_end`), so the Start/Stop
/// transport is identical for all three; only the alias gate differs, and that
/// stays with each runtime's own adapter.
pub(crate) fn with_lifecycle_extension(command: &str) -> String {
    let trimmed = command.trim();
    let path = lifecycle_extension_path();
    let raw_path = path.to_string_lossy();
    let quoted_path = crate::integrations::shared::shell_quote(&raw_path);
    if trimmed.contains(raw_path.as_ref()) || trimmed.contains(&quoted_path) {
        return trimmed.to_string();
    }
    format!("{trimmed} --extension {quoted_path}")
}

fn render_lifecycle_extension(notify_path: &std::path::Path) -> Result<String, String> {
    let notify_path_json = serde_json::to_string(&notify_path.to_string_lossy())
        .map_err(|error| format!("Failed to encode lifecycle hook path: {error}"))?;
    Ok(LIFECYCLE_EXTENSION.replace("{{NOTIFY_PATH_JSON}}", &notify_path_json))
}

pub(crate) fn install_lifecycle_extension() -> Result<(), String> {
    let notify_path = notify_hook_script_path();
    write_executable_script(
        &notify_path,
        NOTIFY_HOOK_SCRIPT,
        "shared notify transport",
    )?;
    let extension = render_lifecycle_extension(&notify_path)?;
    write_file_atomic(
        &lifecycle_extension_path(),
        &extension,
        "Pi-family lifecycle extension",
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;

    #[test]
    fn lifecycle_extension_reports_provider_session_identity() {
        let directory = tempfile::tempdir().expect("temporary extension harness");
        let capture_path = directory.path().join("payload.json");
        let notify_path = directory.path().join("notify.sh");
        std::fs::write(
            &notify_path,
            "#!/bin/bash\nprintf '%s' \"$1\" > \"$CAPTURE_PATH\"\n",
        )
        .expect("write capture notifier");
        let extension_path = directory.path().join("lifecycle-extension.mjs");
        std::fs::write(
            &extension_path,
            render_lifecycle_extension(&notify_path).expect("render lifecycle extension"),
        )
        .expect("write lifecycle extension");
        let harness_path = directory.path().join("harness.mjs");
        std::fs::write(
            &harness_path,
            format!(
                r#"import register from {};
const handlers = new Map();
register({{ on(name, handler) {{ handlers.set(name, handler); }} }});
const context = {{ sessionManager: {{
  getSessionId() {{ return "omp-provider-1"; }},
  getSessionFile() {{ return "/trusted/omp-provider-1.jsonl"; }},
}} }};
await handlers.get("agent_end")({{}}, context);
"#,
                serde_json::to_string(&extension_path.to_string_lossy()).unwrap()
            ),
        )
        .expect("write extension harness");

        let output = Command::new("bun")
            .arg("run")
            .arg(&harness_path)
            .env("CAPTURE_PATH", &capture_path)
            .output()
            .expect("run lifecycle extension with bun");
        assert!(
            output.status.success(),
            "bun failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let payload: serde_json::Value = serde_json::from_slice(
            &std::fs::read(capture_path).expect("captured hook payload"),
        )
        .expect("valid captured hook JSON");
        assert_eq!(
            payload,
            serde_json::json!({
                "hook_event_name": "Stop",
                "session_id": "omp-provider-1",
                "provider_transcript_path": "/trusted/omp-provider-1.jsonl"
            })
        );
    }
}
