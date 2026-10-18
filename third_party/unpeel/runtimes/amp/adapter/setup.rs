use crate::hook_assets::{
    ensure_project_exclude_entry, notify_hook_script_path, write_executable_script,
    write_project_file_no_symlinks, NOTIFY_HOOK_SCRIPT,
};
use std::path::Path;

pub(crate) const AMP_PLUGIN_SCRIPT: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../runtimes/amp/assets/hooks/plugin.js"
));

pub fn install_amp_plugin() -> Result<(), String> {
    let notify_path = notify_hook_script_path();
    write_executable_script(&notify_path, NOTIFY_HOOK_SCRIPT, "notify hook script")?;
    Ok(())
}
pub fn prepare_amp_project_plugin(cwd: &str) -> Result<(), String> {
    let plugin = AMP_PLUGIN_SCRIPT.replace(
        "{{NOTIFY_PATH}}",
        notify_hook_script_path().to_string_lossy().as_ref(),
    );
    write_project_file_no_symlinks(
        Path::new(cwd),
        Path::new(".amp/plugins/unpeel-notify.js"),
        plugin.as_bytes(),
    )?;
    ensure_project_exclude_entry(cwd, ".amp/plugins/unpeel-notify.js");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;

    #[test]
    fn amp_plugin_stamps_runtime_id_and_omits_raw_prompts() {
        let directory = tempfile::tempdir().expect("temporary amp test directory");
        let capture_path = directory.path().join("payloads.jsonl");
        let notify_path = directory.path().join("notify.sh");
        std::fs::write(
            &notify_path,
            "#!/bin/bash\nprintf '%s\\n' \"$1\" >> \"$CAPTURE_PATH\"\n",
        )
        .expect("write capture notifier");
        let plugin_code =
            AMP_PLUGIN_SCRIPT.replace("{{NOTIFY_PATH}}", notify_path.to_string_lossy().as_ref());
        let plugin_path = directory.path().join("unpeel-notify.mjs");
        std::fs::write(&plugin_path, plugin_code).expect("write rendered amp plugin");

        let harness_path = directory.path().join("harness.mjs");
        std::fs::write(
            &harness_path,
            format!(
                r#"import register from {};
const handlers = new Map();
const amp = {{
  on(name, handler) {{ handlers.set(name, handler); }},
}};
register(amp);
await handlers.get("agent.start")({{ threadID: "thread-abc", message: "sensitive secret prompt" }});
await handlers.get("agent.end")({{ threadID: "thread-abc", status: "done" }});
"#,
                serde_json::to_string(&plugin_path.to_string_lossy()).unwrap()
            ),
        )
        .expect("write amp test harness");

        let output = Command::new("bun")
            .arg("run")
            .arg(&harness_path)
            .env("CAPTURE_PATH", &capture_path)
            .output()
            .expect("run amp plugin with bun");
        assert!(
            output.status.success(),
            "bun failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );

        let payloads: Vec<serde_json::Value> = std::fs::read_to_string(capture_path)
            .expect("captured hook payloads")
            .lines()
            .map(|line| serde_json::from_str(line).expect("valid captured hook JSON"))
            .collect();
        assert_eq!(payloads.len(), 2);
        assert_eq!(payloads[0]["hook_event_name"], "Start");
        assert_eq!(payloads[0]["unpeel_runtime_id"], "amp");
        assert_eq!(payloads[0]["session_id"], "thread-abc");
        assert!(payloads[0].get("prompt_text").is_none());
        assert!(payloads[0].get("tool_name").is_none());

        assert_eq!(payloads[1]["hook_event_name"], "Stop");
        assert_eq!(payloads[1]["unpeel_runtime_id"], "amp");
        assert_eq!(payloads[1]["session_id"], "thread-abc");
        assert!(payloads[1].get("tool_name").is_none());
    }

    #[test]
    fn amp_plugin_handles_ask_user_choice_and_filters_unrelated_tools() {
        let directory = tempfile::tempdir().expect("temporary amp test directory");
        let capture_path = directory.path().join("payloads.jsonl");
        let notify_path = directory.path().join("notify.sh");
        std::fs::write(
            &notify_path,
            "#!/bin/bash\nprintf '%s\\n' \"$1\" >> \"$CAPTURE_PATH\"\n",
        )
        .expect("write capture notifier");
        let plugin_code =
            AMP_PLUGIN_SCRIPT.replace("{{NOTIFY_PATH}}", notify_path.to_string_lossy().as_ref());
        let plugin_path = directory.path().join("unpeel-notify.mjs");
        std::fs::write(&plugin_path, plugin_code).expect("write rendered amp plugin");

        let harness_path = directory.path().join("harness.mjs");
        std::fs::write(
            &harness_path,
            format!(
                r#"import register from {};
const handlers = new Map();
const amp = {{
  on(name, handler) {{ handlers.set(name, handler); }},
}};
register(amp);

// 1. Start root thread
await handlers.get("agent.start")({{ thread: {{ id: "thread-1" }} }});

// 2. Unrelated tool bash -> MUST NOT emit attention
const r1 = await handlers.get("tool.call")({{ thread: {{ id: "thread-1" }}, tool: "bash", toolUseID: "t-1", input: {{ command: "ls" }} }});
if (r1?.action !== "allow") throw new Error("expected allow");
await handlers.get("tool.result")({{ thread: {{ id: "thread-1" }}, tool: "bash", toolUseID: "t-1", status: "done" }});

// 3. Different thread activity -> MUST be suppressed
await handlers.get("tool.call")({{ thread: {{ id: "thread-other" }}, tool: "ask_user_choice", toolUseID: "other-q" }});

// 4. Real question on thread-1 -> PermissionRequest
await handlers.get("tool.call")({{ thread: {{ id: "thread-1" }}, tool: "ask_user_choice", toolUseID: "q-1", input: {{}} }});

// 5. Duplicate / unknown toolUseID result -> MUST NOT emit UserPromptSubmit
await handlers.get("tool.result")({{ thread: {{ id: "thread-1" }}, tool: "ask_user_choice", toolUseID: "unknown-q", status: "done" }});

// 6. Matching question result on thread-1 -> UserPromptSubmit
await handlers.get("tool.result")({{ thread: {{ id: "thread-1" }}, tool: "ask_user_choice", toolUseID: "q-1", status: "done" }});

// 7. Turn finishes -> Stop
await handlers.get("agent.end")({{ thread: {{ id: "thread-1" }}, status: "done" }});
"#,
                serde_json::to_string(&plugin_path.to_string_lossy()).unwrap()
            ),
        )
        .expect("write amp test harness");

        let output = Command::new("bun")
            .arg("run")
            .arg(&harness_path)
            .env("CAPTURE_PATH", &capture_path)
            .output()
            .expect("run amp plugin with bun");
        assert!(
            output.status.success(),
            "bun failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );

        let payloads: Vec<serde_json::Value> = std::fs::read_to_string(capture_path)
            .expect("captured hook payloads")
            .lines()
            .map(|line| serde_json::from_str(line).expect("valid captured hook JSON"))
            .collect();
        assert_eq!(
            payloads.len(),
            4,
            "unrelated tools and foreign threads suppressed"
        );
        for payload in &payloads {
            assert_eq!(payload["unpeel_runtime_id"], "amp");
            assert_eq!(payload["session_id"], "thread-1");
        }
        assert_eq!(payloads[0]["hook_event_name"], "Start");
        assert_eq!(payloads[1]["hook_event_name"], "PermissionRequest");
        assert_eq!(payloads[1]["tool_name"], "ask_user_choice");
        assert_eq!(payloads[2]["hook_event_name"], "UserPromptSubmit");
        assert_eq!(payloads[3]["hook_event_name"], "Stop");
    }
}
