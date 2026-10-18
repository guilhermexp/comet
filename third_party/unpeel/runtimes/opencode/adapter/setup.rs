use crate::app_paths::app_hooks_root;
use crate::hook_assets::{notify_hook_script_path, write_executable_script, NOTIFY_HOOK_SCRIPT};
use std::fs;
use std::path::PathBuf;

pub(crate) const OPENCODE_PLUGIN_SCRIPT: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../runtimes/opencode/assets/hooks/plugin.js"
));

pub fn install_opencode_plugin() -> Result<(), String> {
    let notify_path = notify_hook_script_path();
    write_executable_script(&notify_path, NOTIFY_HOOK_SCRIPT, "notify hook script")?;

    let plugin_dir = opencode_plugin_dir();
    fs::create_dir_all(&plugin_dir).map_err(|e| {
        format!(
            "Failed to create OpenCode plugin dir {}: {e}",
            plugin_dir.display()
        )
    })?;
    let plugin =
        OPENCODE_PLUGIN_SCRIPT.replace("{{NOTIFY_PATH}}", notify_path.to_string_lossy().as_ref());
    fs::write(opencode_plugin_path(), plugin)
        .map_err(|e| format!("Failed to write OpenCode plugin: {e}"))?;
    Ok(())
}
pub fn opencode_config_dir() -> PathBuf {
    app_hooks_root().join("opencode")
}
pub(crate) fn opencode_plugin_dir() -> PathBuf {
    opencode_config_dir().join("plugin")
}

pub(crate) fn opencode_plugin_path() -> PathBuf {
    opencode_plugin_dir().join("comet-notify.js")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;

    #[test]
    fn opencode_plugin_stamps_runtime_id_and_maps_permission_events() {
        let directory = tempfile::tempdir().expect("temporary opencode test directory");
        let capture_path = directory.path().join("payloads.jsonl");
        let notify_path = directory.path().join("notify.sh");
        std::fs::write(
            &notify_path,
            "#!/bin/bash\nprintf '%s\\n' \"$1\" >> \"$CAPTURE_PATH\"\n",
        )
        .expect("write capture notifier");
        let plugin_code = OPENCODE_PLUGIN_SCRIPT
            .replace("{{NOTIFY_PATH}}", notify_path.to_string_lossy().as_ref());
        let plugin_path = directory.path().join("comet-notify.mjs");
        std::fs::write(&plugin_path, plugin_code).expect("write rendered opencode plugin");

        let harness_path = directory.path().join("harness.mjs");
        std::fs::write(
            &harness_path,
            format!(
                r#"import {{ spawn }} from "node:child_process";
import {{ UnpeelNotifyPlugin }} from {};

const mockDollar = async (strings, ...values) => {{
  const script = values[0];
  const payload = values[1];
  return new Promise((resolve) => {{
    const child = spawn("bash", [script, payload], {{ stdio: "ignore" }});
    child.once("exit", resolve);
    child.once("error", resolve);
  }});
}};

const client = {{
  session: {{
    list: async () => ({{ data: [{{ id: "root-1", parentID: null }}] }}),
  }},
}};

const plugin = await UnpeelNotifyPlugin({{ $: mockDollar, client }});
await plugin.event({{
  event: {{
    type: "session.status",
    properties: {{ sessionID: "root-1", status: {{ type: "busy" }} }},
  }},
}});
await plugin.event({{
  event: {{
    type: "permission.asked",
    properties: {{
      id: "per-1",
      sessionID: "root-1",
      permission: "bash_exec",
      patterns: [],
      metadata: {{}},
      always: [],
    }},
  }},
}});
await plugin.event({{
  event: {{
    type: "permission.replied",
    properties: {{ sessionID: "root-1", requestID: "per-1", reply: "once" }},
  }},
}});
await plugin.event({{
  event: {{
    type: "session.status",
    properties: {{ sessionID: "root-1", status: {{ type: "idle" }} }},
  }},
}});
"#,
                serde_json::to_string(&plugin_path.to_string_lossy()).unwrap()
            ),
        )
        .expect("write opencode test harness");

        let output = Command::new("bun")
            .arg("run")
            .arg(&harness_path)
            .env("CAPTURE_PATH", &capture_path)
            .env("UNPEEL_SESSION_ID", "test-session-id")
            .output()
            .expect("run opencode plugin with bun");
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
        assert_eq!(payloads.len(), 4);
        assert_eq!(payloads[0]["hook_event_name"], "Start");
        assert_eq!(payloads[0]["unpeel_runtime_id"], "opencode");
        assert_eq!(payloads[0]["session_id"], "root-1");

        assert_eq!(payloads[1]["hook_event_name"], "PermissionRequest");
        assert_eq!(payloads[1]["unpeel_runtime_id"], "opencode");
        assert_eq!(payloads[1]["tool_name"], "bash_exec");
        assert_eq!(payloads[1]["session_id"], "root-1");

        assert_eq!(payloads[2]["hook_event_name"], "UserPromptSubmit");
        assert_eq!(payloads[2]["unpeel_runtime_id"], "opencode");
        assert_eq!(payloads[2]["session_id"], "root-1");

        assert_eq!(payloads[3]["hook_event_name"], "Stop");
        assert_eq!(payloads[3]["unpeel_runtime_id"], "opencode");
        assert_eq!(payloads[3]["session_id"], "root-1");
    }

    #[test]
    fn opencode_plugin_handles_question_events_and_cancellation_interleaving() {
        let directory = tempfile::tempdir().expect("temporary opencode test directory");
        let capture_path = directory.path().join("payloads.jsonl");
        let notify_path = directory.path().join("notify.sh");
        std::fs::write(
            &notify_path,
            "#!/bin/bash\nprintf '%s\\n' \"$1\" >> \"$CAPTURE_PATH\"\n",
        )
        .expect("write capture notifier");
        let plugin_code = OPENCODE_PLUGIN_SCRIPT
            .replace("{{NOTIFY_PATH}}", notify_path.to_string_lossy().as_ref());
        let plugin_path = directory.path().join("comet-notify.mjs");
        std::fs::write(&plugin_path, plugin_code).expect("write rendered opencode plugin");

        let harness_path = directory.path().join("harness.mjs");
        std::fs::write(
            &harness_path,
            format!(
                r#"import {{ spawn }} from "node:child_process";
import {{ UnpeelNotifyPlugin }} from {};

const mockDollar = async (strings, ...values) => {{
  const script = values[0];
  const payload = values[1];
  return new Promise((resolve) => {{
    const child = spawn("bash", [script, payload], {{ stdio: "ignore" }});
    child.once("exit", resolve);
    child.once("error", resolve);
  }});
}};

const client = {{
  session: {{
    list: async () => ({{
      data: [
        {{ id: "root-1", parentID: null }},
        {{ id: "child-sub", parentID: "root-1" }},
        {{ id: "foreign-root", parentID: null }},
      ],
    }}),
  }},
}};

const plugin = await UnpeelNotifyPlugin({{ $: mockDollar, client }});

// 1. Root starts busy -> Start
await plugin.event({{
  event: {{
    type: "session.status",
    properties: {{ sessionID: "root-1", status: {{ type: "busy" }} }},
  }},
}});

// 2. Interleaved child session activity -> MUST be suppressed
await plugin.event({{
  event: {{
    type: "session.status",
    properties: {{ sessionID: "child-sub", status: {{ type: "busy" }} }},
  }},
}});
await plugin.event({{
  event: {{
    type: "question.asked",
    properties: {{ sessionID: "child-sub" }},
  }},
}});

// 2b. Interleaved foreign root session activity -> MUST be rejected because rootSessionID is root-1
await plugin.event({{
  event: {{
    type: "session.status",
    properties: {{ sessionID: "foreign-root", status: {{ type: "busy" }} }},
  }},
}});
await plugin.event({{
  event: {{
    type: "question.v2.asked",
    properties: {{ sessionID: "foreign-root", id: "foreign-q" }},
  }},
}});
await plugin.event({{
  event: {{
    type: "permission.v2.replied",
    properties: {{ sessionID: "foreign-root", requestID: "foreign-perm" }},
  }},
}});

// 3. Root permission v2 asked with action: "write_file" -> PermissionRequest
await plugin.event({{
  event: {{
    type: "permission.v2.asked",
    properties: {{
      id: "perm-1",
      sessionID: "root-1",
      action: "write_file",
    }},
  }},
}});

// 4. Root question v2 asked while permission is pending -> PermissionRequest
await plugin.event({{
  event: {{
    type: "question.v2.asked",
    properties: {{
      id: "q-1",
      sessionID: "root-1",
    }},
  }},
}});

// 5. Permission perm-1 replied, but q-1 is STILL pending -> MUST NOT emit UserPromptSubmit yet
await plugin.event({{
  event: {{
    type: "permission.v2.replied",
    properties: {{
      requestID: "perm-1",
      sessionID: "root-1",
    }},
  }},
}});

// 6. Question q-1 rejected/cancelled -> now both pending sets empty -> emits UserPromptSubmit
await plugin.event({{
  event: {{
    type: "question.v2.rejected",
    properties: {{
      requestID: "q-1",
      sessionID: "root-1",
    }},
  }},
}});

// 6b. Duplicate reply for already-resolved q-1 -> MUST NOT emit second UserPromptSubmit
await plugin.event({{
  event: {{
    type: "question.v2.rejected",
    properties: {{
      requestID: "q-1",
      sessionID: "root-1",
    }},
  }},
}});

// 6c. Unknown ID reply -> MUST NOT emit UserPromptSubmit
await plugin.event({{
  event: {{
    type: "permission.v2.replied",
    properties: {{
      requestID: "nonexistent-id",
      sessionID: "root-1",
    }},
  }},
}});

// 7. Interleaved child session idle -> MUST be suppressed
await plugin.event({{
  event: {{
    type: "session.status",
    properties: {{ sessionID: "child-sub", status: {{ type: "idle" }} }},
  }},
}});

// 8. Root session idle -> Stop
await plugin.event({{
  event: {{
    type: "session.status",
    properties: {{ sessionID: "root-1", status: {{ type: "idle" }} }},
  }},
}});
"#,
                serde_json::to_string(&plugin_path.to_string_lossy()).unwrap()
            ),
        )
        .expect("write opencode test harness");

        let output = Command::new("bun")
            .arg("run")
            .arg(&harness_path)
            .env("CAPTURE_PATH", &capture_path)
            .env("UNPEEL_SESSION_ID", "test-session-id")
            .output()
            .expect("run opencode plugin with bun");
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
            5,
            "permission reply did not prematurely clear pending question"
        );
        for payload in &payloads {
            assert_eq!(payload["unpeel_runtime_id"], "opencode");
            assert_eq!(payload["session_id"], "root-1");
        }

        assert_eq!(payloads[0]["hook_event_name"], "Start");
        assert_eq!(payloads[1]["hook_event_name"], "PermissionRequest");
        assert_eq!(payloads[1]["tool_name"], "write_file");
        assert_eq!(payloads[2]["hook_event_name"], "PermissionRequest");
        assert_eq!(payloads[2]["tool_name"], "question");
        assert_eq!(payloads[3]["hook_event_name"], "UserPromptSubmit");
        assert_eq!(payloads[4]["hook_event_name"], "Stop");
    }
}
