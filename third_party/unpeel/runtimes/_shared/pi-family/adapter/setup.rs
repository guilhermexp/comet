use crate::app_paths::unpeel_home;
use crate::hook_assets::{
    NOTIFY_HOOK_SCRIPT, notify_hook_script_path, write_executable_script, write_file_atomic,
};
use std::ops::Range;
use std::path::PathBuf;

const LIFECYCLE_EXTENSION: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../runtimes/_shared/pi-family/assets/lifecycle-extension.js"
));

pub(crate) fn lifecycle_extension_path() -> PathBuf {
    unpeel_home()
        .join("hooks")
        .join(if super::FILTER_NESTED_OMP_SUBAGENTS {
            "omp-lifecycle-extension.js"
        } else {
            "pi-family-lifecycle-extension.js"
        })
}

/// Append `--extension <lifecycle extension>` exactly once.
///
/// Every pi-family CLI (`pi`, `omp`, `prime-agent`) takes `-e/--extension` and
/// runs the same extension API (`agent_start`/`agent_end`/`ui_prompt_*`), so
/// the Start/Stop/Attention transport is identical for all three; only the
/// alias gate differs, and that stays with each runtime's own adapter.
pub(crate) fn with_lifecycle_extension(command: &str) -> String {
    let trimmed = command.trim();
    let path = lifecycle_extension_path();
    let raw_path = path.to_string_lossy();
    let quoted_path = crate::integrations::shared::shell_quote(&raw_path);
    if has_extension_argument(trimmed, raw_path.as_ref()) {
        return trimmed.to_string();
    }
    // Older Worker sessions already have the shared path persisted in their
    // command. Replace it for OMP so future launches get the OMP-only path
    // classifier while remaining idempotent for Pi and Prime.
    if super::FILTER_NESTED_OMP_SUBAGENTS {
        let legacy_path = unpeel_home()
            .join("hooks")
            .join("pi-family-lifecycle-extension.js");
        let legacy_raw = legacy_path.to_string_lossy();
        if let Some(updated) = replace_extension_argument(
            trimmed,
            legacy_raw.as_ref(),
            raw_path.as_ref(),
            &quoted_path,
        ) {
            return updated;
        }
    }
    append_extension_argument(trimmed, &quoted_path)
}

fn append_extension_argument(command: &str, quoted_path: &str) -> String {
    if let Some(delimiter) = shell_tokens(command)
        .into_iter()
        .find(|token| token.value == "--")
    {
        let (before, after) = command.split_at(delimiter.raw.start);
        format!("{before}--extension {quoted_path} {after}")
    } else {
        format!("{command} --extension {quoted_path}")
    }
}

#[derive(Debug)]
struct ShellToken {
    raw: Range<usize>,
    value: String,
}

fn shell_tokens(command: &str) -> Vec<ShellToken> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut start = None;
    let mut in_single = false;
    let mut in_double = false;
    let mut escaped = false;
    for (index, ch) in command.char_indices() {
        if escaped {
            current.push(ch);
            escaped = false;
            continue;
        }
        match ch {
            '\\' if !in_single => {
                start.get_or_insert(index);
                escaped = true;
            }
            '\'' if !in_double => {
                start.get_or_insert(index);
                in_single = !in_single;
            }
            '"' if !in_single => {
                start.get_or_insert(index);
                in_double = !in_double;
            }
            ch if ch.is_whitespace() && !in_single && !in_double => {
                if let Some(start) = start.take() {
                    tokens.push(ShellToken {
                        raw: start..index,
                        value: std::mem::take(&mut current),
                    });
                }
            }
            _ => {
                start.get_or_insert(index);
                current.push(ch);
            }
        }
    }
    if let Some(start) = start {
        tokens.push(ShellToken {
            raw: start..command.len(),
            value: current,
        });
    }
    tokens
}

fn extension_argument_value<'a>(tokens: &'a [ShellToken], index: usize) -> Option<&'a str> {
    let token = tokens.get(index)?;
    match token.value.as_str() {
        "--extension" | "-e" => tokens.get(index + 1).map(|value| value.value.as_str()),
        _ => token
            .value
            .strip_prefix("--extension=")
            .or_else(|| token.value.strip_prefix("-e="))
            .or_else(|| token.value.strip_prefix("-e")),
    }
}

fn has_extension_argument(command: &str, expected_path: &str) -> bool {
    let tokens = shell_tokens(command);
    for (index, token) in tokens.iter().enumerate() {
        if token.value == "--" {
            break;
        }
        let matches = matches!(token.value.as_str(), "--extension" | "-e")
            .then(|| extension_argument_value(&tokens, index))
            .flatten()
            == Some(expected_path)
            || token
                .value
                .strip_prefix("--extension=")
                .or_else(|| token.value.strip_prefix("-e="))
                .or_else(|| token.value.strip_prefix("-e"))
                == Some(expected_path);
        if matches {
            return true;
        }
    }
    false
}

fn replace_extension_argument(
    command: &str,
    old_path: &str,
    new_path: &str,
    quoted_new_path: &str,
) -> Option<String> {
    let tokens = shell_tokens(command);
    let mut replacement = None;
    for (index, token) in tokens.iter().enumerate() {
        if token.value == "--" {
            break;
        }
        if matches!(token.value.as_str(), "--extension" | "-e") {
            let Some(value) = tokens.get(index + 1) else {
                continue;
            };
            if value.value == old_path {
                replacement = Some((value.raw.clone(), quoted_new_path.to_owned()));
                break;
            }
            continue;
        }
        let matched = token
            .value
            .strip_prefix("--extension=")
            .or_else(|| token.value.strip_prefix("-e="))
            .or_else(|| token.value.strip_prefix("-e"))
            == Some(old_path);
        if matched {
            let flag = if token.value.starts_with("--extension=") {
                "--extension="
            } else if token.value.starts_with("-e=") {
                "-e="
            } else {
                "-e"
            };
            replacement = Some((token.raw.clone(), format!("{flag}{quoted_new_path}")));
            break;
        }
    }
    let (range, value) = replacement?;
    let mut updated = command.to_owned();
    updated.replace_range(range, &value);
    Some(updated)
}

fn render_lifecycle_extension(
    notify_path: &std::path::Path,
    filter_nested_omp_subagents: bool,
) -> Result<String, String> {
    let notify_path_json = serde_json::to_string(&notify_path.to_string_lossy())
        .map_err(|error| format!("Failed to encode lifecycle hook path: {error}"))?;
    Ok(LIFECYCLE_EXTENSION
        .replace("{{NOTIFY_PATH_JSON}}", &notify_path_json)
        .replace(
            "{{FILTER_NESTED_OMP_SUBAGENTS_JSON}}",
            if filter_nested_omp_subagents {
                "true"
            } else {
                "false"
            },
        ))
}

pub(crate) fn install_lifecycle_extension() -> Result<(), String> {
    let notify_path = notify_hook_script_path();
    write_executable_script(&notify_path, NOTIFY_HOOK_SCRIPT, "shared notify transport")?;
    let extension = render_lifecycle_extension(&notify_path, super::FILTER_NESTED_OMP_SUBAGENTS)?;
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
            render_lifecycle_extension(&notify_path, false).expect("render lifecycle extension"),
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
        let payload: serde_json::Value =
            serde_json::from_slice(&std::fs::read(capture_path).expect("captured hook payload"))
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

    #[test]
    fn migrates_only_the_exact_extension_argument_value() {
        let old_path = "/tmp/unpeel/hooks/pi-family-lifecycle-extension.js";
        let new_path = "/tmp/unpeel/hooks/omp-lifecycle-extension.js";
        let quoted_old = crate::integrations::shared::shell_quote(old_path);
        let quoted_new = crate::integrations::shared::shell_quote(new_path);
        let prompt = crate::integrations::shared::shell_quote(&format!("inspect {old_path}"));
        let command = format!("omp --extension {quoted_old} --prompt {prompt}");

        let migrated = replace_extension_argument(&command, old_path, new_path, &quoted_new)
            .expect("replace the exact extension option");
        assert!(has_extension_argument(&migrated, new_path));
        assert!(migrated.contains(&format!("--prompt {prompt}")));
        assert!(!has_extension_argument(&migrated, old_path));

        let equals = format!("omp --extension={quoted_old} --prompt {prompt}");
        let migrated_equals = replace_extension_argument(&equals, old_path, new_path, &quoted_new)
            .expect("replace an equals-form extension option");
        assert!(has_extension_argument(&migrated_equals, new_path));
        assert!(migrated_equals.contains(&format!("--prompt {prompt}")));

        let backup = format!("omp --extension {old_path}.backup --prompt {prompt}");
        assert!(replace_extension_argument(&backup, old_path, new_path, &quoted_new).is_none());
        assert!(!has_extension_argument(
            &format!("omp --prompt {prompt}"),
            old_path
        ));

        let positional = format!("omp --prompt inspect -- --extension {quoted_old}");
        assert!(replace_extension_argument(&positional, old_path, new_path, &quoted_new).is_none());
        assert!(!has_extension_argument(&positional, old_path));
        let appended = with_lifecycle_extension(&positional);
        let current_path = lifecycle_extension_path();
        let quoted_current =
            crate::integrations::shared::shell_quote(&current_path.to_string_lossy());
        assert!(appended.contains(&format!(
            "--extension {quoted_current} -- --extension {quoted_old}"
        )));
        assert!(appended.ends_with(&format!("--extension {quoted_old}")));
    }

    #[test]
    fn lifecycle_extension_ignores_nested_omp_agent_events_and_keeps_primary_rebinding() {
        let directory = tempfile::tempdir().expect("temporary extension harness");
        let capture_path = directory.path().join("payloads.jsonl");
        let notify_path = directory.path().join("notify.sh");
        std::fs::write(
            &notify_path,
            "#!/bin/bash\nprintf '%s\\n' \"$1\" >> \"$CAPTURE_PATH\"\n",
        )
        .expect("write capture notifier");
        let default_sessions = directory.path().join("omp/agent/sessions/-project");
        let primary_file = default_sessions.join("2026-10-08T12-00-00-000Z_primary.jsonl");
        let nested_file = default_sessions
            .join("2026-10-08T12-00-00-000Z_primary")
            .join("PgliteSqlCheck.jsonl");
        let explicit_main_file = nested_file.clone();
        let unflushed_nested_file = nested_file.with_file_name("NotYetFlushed.jsonl");
        let resumed_file = default_sessions.join("2026-10-08T12-30-00-000Z_resumed.jsonl");
        std::fs::create_dir_all(nested_file.parent().expect("nested transcript directory"))
            .expect("create OMP session artifacts directory");
        std::fs::write(
            &primary_file,
            "{\"type\":\"session\",\"id\":\"opus-primary\"}\n",
        )
        .expect("write primary OMP transcript");
        std::fs::write(
            &nested_file,
            "{\"type\":\"session\",\"id\":\"gemini-child\"}\n",
        )
        .expect("write nested OMP transcript");
        #[cfg(unix)]
        let nested_alias_file = {
            use std::os::unix::fs::symlink;

            let alias_dir = directory.path().join("transcript-alias");
            std::fs::create_dir_all(&alias_dir).expect("create transcript alias directory");
            let alias_file = alias_dir.join("PgliteSqlCheck.jsonl");
            symlink(&nested_file, &alias_file).expect("create nested transcript alias");
            alias_file
        };
        #[cfg(not(unix))]
        let nested_alias_file = nested_file.clone();
        let explicit_root = directory.path().join("explicit-session-dir");
        let explicit_primary = explicit_root
            .join("-project")
            .join("2026-10-08T13-00-00-000Z_explicit.jsonl");
        std::fs::create_dir_all(explicit_primary.parent().expect("explicit OMP cwd bucket"))
            .expect("create explicit OMP session directory");
        std::fs::write(
            &explicit_primary,
            "{\"type\":\"session\",\"id\":\"opus-explicit\"}\n",
        )
        .expect("write explicit primary OMP transcript");
        std::fs::write(
            &resumed_file,
            "{\"type\":\"session\",\"id\":\"opus-resumed\"}\n",
        )
        .expect("write resumed primary OMP transcript");
        let extension_path = directory.path().join("lifecycle-extension.mjs");
        std::fs::write(
            &extension_path,
            render_lifecycle_extension(&notify_path, true).expect("render lifecycle extension"),
        )
        .expect("write lifecycle extension");
        let harness_path = directory.path().join("harness.mjs");
        std::fs::write(
            &harness_path,
            format!(
                r#"import register from {extension_path};
const handlers = new Map();
register({{ on(name, handler) {{ handlers.set(name, handler); }} }});
const nested = {{
  sessionManager: {{
    getSessionId() {{ return "gemini-child"; }},
    getSessionFile() {{ return {nested_file}; }},
  }},
}};
const nestedAlias = {{
  sessionManager: {{
    getSessionId() {{ return "gemini-child"; }},
    getSessionFile() {{ return {nested_alias_file}; }},
  }},
}};
const unflushedNested = {{
  sessionManager: {{
    getSessionId() {{ return "unflushed-child"; }},
    getSessionFile() {{ return {unflushed_nested_file}; }},
  }},
}};
const primary = {{
  sessionManager: {{
    getSessionId() {{ return "opus-primary"; }},
    getSessionFile() {{ return {primary_file}; }},
  }},
}};
const explicitPrimary = {{
  sessionManager: {{
    getSessionId() {{ return "opus-explicit"; }},
    getSessionFile() {{ return {explicit_primary}; }},
  }},
}};
const reboundPrimary = {{
  sessionManager: {{
    getSessionId() {{ return "opus-resumed"; }},
    getSessionFile() {{ return {resumed_file}; }},
  }},
}};
const inMemorySubagent = {{
  agent: {{ kind: "sub", id: "PgliteSqlCheck", name: "PgliteSqlCheck", depth: 0, parentId: "Main" }},
  sessionManager: {{ getSessionId() {{ return "memory-child"; }}, getSessionFile() {{ return null; }} }},
}};
const explicitMain = {{
  agent: {{ kind: "main", id: "Main", name: "Main", depth: 0 }},
  sessionManager: {{
    getSessionId() {{ return "opus-explicit-main"; }},
    getSessionFile() {{ return {explicit_main_file}; }},
  }},
}};
await handlers.get("agent_start")({{}}, nested);
await handlers.get("agent_end")({{}}, nestedAlias);
await handlers.get("agent_start")({{}}, unflushedNested);
await handlers.get("ui_prompt_start")({{ kind: "custom", reason: "ui_prompt" }}, nested);
await handlers.get("ui_prompt_end")({{}}, nested);
await handlers.get("agent_start")({{}}, inMemorySubagent);
await handlers.get("agent_start")({{}}, explicitMain);
await handlers.get("agent_start")({{}}, primary);
await handlers.get("ui_prompt_start")({{ kind: "custom", reason: "ui_prompt" }}, explicitPrimary);
await handlers.get("ui_prompt_end")({{}}, explicitPrimary);
await handlers.get("agent_end")({{}}, primary);
await handlers.get("agent_start")({{}}, reboundPrimary);
await handlers.get("agent_end")({{}}, reboundPrimary);
"#,
                extension_path = serde_json::to_string(&extension_path.to_string_lossy()).unwrap(),
                nested_file = serde_json::to_string(&nested_file.to_string_lossy()).unwrap(),
                nested_alias_file =
                    serde_json::to_string(&nested_alias_file.to_string_lossy()).unwrap(),
                unflushed_nested_file =
                    serde_json::to_string(&unflushed_nested_file.to_string_lossy()).unwrap(),
                primary_file = serde_json::to_string(&primary_file.to_string_lossy()).unwrap(),
                explicit_primary =
                    serde_json::to_string(&explicit_primary.to_string_lossy()).unwrap(),
                explicit_main_file =
                    serde_json::to_string(&explicit_main_file.to_string_lossy()).unwrap(),
                resumed_file = serde_json::to_string(&resumed_file.to_string_lossy()).unwrap(),
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
        let payloads: Vec<serde_json::Value> = std::fs::read_to_string(capture_path)
            .expect("captured hook payloads")
            .lines()
            .map(|line| serde_json::from_str(line).expect("valid captured hook JSON"))
            .collect();
        assert_eq!(
            payloads
                .iter()
                .map(|payload| payload["hook_event_name"].as_str().unwrap())
                .collect::<Vec<_>>(),
            [
                "Start",
                "Start",
                "PermissionRequest",
                "UserPromptSubmit",
                "Stop",
                "Start",
                "Stop"
            ]
        );
        assert_eq!(
            payloads
                .iter()
                .map(|payload| payload["session_id"].as_str().unwrap())
                .collect::<Vec<_>>(),
            [
                "opus-explicit-main",
                "opus-primary",
                "opus-explicit",
                "opus-explicit",
                "opus-primary",
                "opus-resumed",
                "opus-resumed"
            ]
        );
    }

    #[test]
    fn pi_family_extension_keeps_lifecycle_for_unrelated_nested_jsonl_paths() {
        let directory = tempfile::tempdir().expect("temporary extension harness");
        let capture_path = directory.path().join("payloads.jsonl");
        let notify_path = directory.path().join("notify.sh");
        std::fs::write(
            &notify_path,
            "#!/bin/bash\nprintf '%s\\n' \"$1\" >> \"$CAPTURE_PATH\"\n",
        )
        .expect("write capture notifier");
        let parent_transcript = directory.path().join("primary.jsonl");
        let child_path = directory.path().join("primary/child.jsonl");
        std::fs::create_dir_all(child_path.parent().expect("child transcript directory"))
            .expect("create nested transcript directory");
        std::fs::write(&parent_transcript, "parent")
            .expect("write unrelated neighboring transcript");
        std::fs::write(&child_path, "child").expect("write nested-looking Pi transcript");
        let extension_path = directory.path().join("pi-lifecycle-extension.mjs");
        std::fs::write(
            &extension_path,
            render_lifecycle_extension(&notify_path, false).expect("render Pi extension"),
        )
        .expect("write Pi extension");
        let harness_path = directory.path().join("harness.mjs");
        std::fs::write(
            &harness_path,
            format!(
                r#"import register from {};
const handlers = new Map();
register({{ on(name, handler) {{ handlers.set(name, handler); }} }});
const context = {{ sessionManager: {{
  getSessionId() {{ return "pi-primary"; }},
  getSessionFile() {{ return {}; }},
}} }};
await handlers.get("agent_end")({{}}, context);
"#,
                serde_json::to_string(&extension_path.to_string_lossy()).unwrap(),
                serde_json::to_string(&child_path.to_string_lossy()).unwrap(),
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
        let payloads: Vec<serde_json::Value> = std::fs::read_to_string(capture_path)
            .expect("captured hook payloads")
            .lines()
            .map(|line| serde_json::from_str(line).expect("valid captured hook JSON"))
            .collect();
        assert_eq!(payloads.len(), 1);
        assert_eq!(payloads[0]["hook_event_name"], "Stop");
        assert_eq!(payloads[0]["session_id"], "pi-primary");
    }

    #[test]
    fn ui_prompt_start_posts_permission_request_without_stop() {
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
            render_lifecycle_extension(&notify_path, false).expect("render lifecycle extension"),
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
await handlers.get("ui_prompt_start")({{ kind: "custom", reason: "ui_prompt" }}, context);
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
        let payload: serde_json::Value =
            serde_json::from_slice(&std::fs::read(capture_path).expect("captured hook payload"))
                .expect("valid captured hook JSON");
        assert_eq!(payload["hook_event_name"], "PermissionRequest");
        assert_eq!(payload["tool_name"], "AskUserQuestion");
        assert_ne!(payload["hook_event_name"], "Stop");
        assert_eq!(payload["session_id"], "omp-provider-1");
    }
}
