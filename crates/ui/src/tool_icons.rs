use std::path::Path;

use gpui::SharedString;
use serde_json::Value;
use zeron_proto::ToolCall;

use crate::details_sidebar::files_view::material_icon_path;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ToolIconDescriptor {
    Material(SharedString),
    Solar(&'static str),
}

impl ToolIconDescriptor {
    pub(crate) fn material_image(&self) -> Option<std::sync::Arc<gpui::Image>> {
        match self {
            Self::Material(path) => crate::icons::material_file_icon_image(path.as_ref()),
            Self::Solar(_) => None,
        }
    }
}

const COMMAND_ICONS: &[(&str, &str)] = &[
    ("bash", "console"),
    ("bun", "bun"),
    ("bunx", "bun"),
    ("cargo", "rust"),
    ("chrome", "chrome"),
    ("chromium", "chrome"),
    ("deno", "deno"),
    ("docker", "docker"),
    ("docker-compose", "docker"),
    ("fish", "console"),
    ("gh", "git"),
    ("git", "git"),
    ("go", "go"),
    ("gofmt", "go"),
    ("google-chrome", "chrome"),
    ("java", "java"),
    ("javac", "java"),
    ("node", "nodejs"),
    ("nodejs", "nodejs"),
    ("npm", "npm"),
    ("npx", "npm"),
    ("playwright", "playwright"),
    ("pnpm", "pnpm"),
    ("pnpx", "pnpm"),
    ("poetry", "poetry"),
    ("pytest", "python"),
    ("ruby", "ruby"),
    ("rustc", "rust"),
    ("sh", "console"),
    ("swift", "swift"),
    ("terraform", "terraform"),
    ("uv", "uv"),
    ("yarn", "yarn"),
    ("yarnpkg", "yarn"),
    ("zsh", "console"),
];

fn material(name: &str) -> ToolIconDescriptor {
    ToolIconDescriptor::Material(format!("file-icons/{name}.svg").into())
}

fn file(path: &str) -> ToolIconDescriptor {
    let name = Path::new(path)
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .unwrap_or(path);
    ToolIconDescriptor::Material(material_icon_path(name, false, false))
}

fn split_shell_commands(command: &str) -> Vec<&str> {
    let mut segments = Vec::new();
    let mut start = 0;
    let mut quote = None;
    let mut escaped = false;
    let mut chars = command.char_indices().peekable();

    while let Some((index, character)) = chars.next() {
        if escaped {
            escaped = false;
            continue;
        }
        if character == '\\' {
            escaped = true;
            continue;
        }
        if let Some(active_quote) = quote {
            if character == active_quote {
                quote = None;
            }
            continue;
        }
        if matches!(character, '\'' | '"') {
            quote = Some(character);
            continue;
        }

        let separator_len = match character {
            ';' | '\n' => Some(character.len_utf8()),
            '|' => {
                if chars.peek().is_some_and(|(_, next)| *next == '|') {
                    chars.next();
                    Some(2)
                } else {
                    Some(1)
                }
            }
            '&' if chars.peek().is_some_and(|(_, next)| *next == '&') => {
                chars.next();
                Some(2)
            }
            _ => None,
        };
        if let Some(separator_len) = separator_len {
            segments.push(&command[start..index]);
            start = index + separator_len;
        }
    }
    segments.push(&command[start..]);
    segments
}

fn normalized_executable(token: &str) -> String {
    let unquoted = token.trim_matches(|character| matches!(character, '\'' | '"'));
    Path::new(unquoted)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default()
        .trim_end_matches(".exe")
        .to_ascii_lowercase()
}

fn is_environment_assignment(token: &str) -> bool {
    token.split_once('=').is_some_and(|(name, _)| {
        let mut chars = name.chars();
        chars
            .next()
            .is_some_and(|first| first == '_' || first.is_ascii_alphabetic())
            && chars.all(|character| character == '_' || character.is_ascii_alphanumeric())
    })
}

fn is_versioned_python(executable: &str) -> bool {
    ["python", "pip"].iter().any(|prefix| {
        executable.strip_prefix(prefix).is_some_and(|suffix| {
            !suffix.is_empty()
                && suffix.chars().any(|character| character.is_ascii_digit())
                && suffix
                    .chars()
                    .all(|character| character.is_ascii_digit() || character == '.')
        })
    })
}

fn command_icon(command: &str) -> &'static str {
    for segment in split_shell_commands(command) {
        for token in segment.split_whitespace() {
            if token.is_empty()
                || token.starts_with('-')
                || is_environment_assignment(token)
                || matches!(
                    token,
                    "command" | "env" | "exec" | "nohup" | "sudo" | "time"
                )
            {
                continue;
            }

            let executable = normalized_executable(token);
            if executable == "python" || executable == "pip" || is_versioned_python(&executable) {
                return "python";
            }
            if let Some((_, icon)) = COMMAND_ICONS
                .iter()
                .find(|(candidate, _)| *candidate == executable)
            {
                return icon;
            }
            break;
        }
    }
    "console"
}

fn value_string<'a>(input: Option<&'a Value>, keys: &[&str]) -> &'a str {
    keys.iter()
        .find_map(|key| input?.get(*key).and_then(Value::as_str))
        .unwrap_or_default()
}

fn semantic_tool_icon(name: &str, input: Option<&Value>) -> ToolIconDescriptor {
    let lower = name.trim_start_matches("tool-").to_ascii_lowercase();
    let language = value_string(input, &["language"]).to_ascii_lowercase();
    let path = value_string(input, &["file_path", "notebook_path", "path"]);
    let command = value_string(input, &["command"]);

    if lower.contains("worktree") {
        return ToolIconDescriptor::Solar(crate::icons::GIT_BRANCH);
    }
    if lower.contains("browser") || lower == "webfetch" {
        return material("chrome");
    }
    if matches!(lower.as_str(), "read" | "write" | "edit") && !path.is_empty() {
        return file(path);
    }
    if matches!(lower.as_str(), "bash" | "run" | "exec") && !command.is_empty() {
        return material(command_icon(command));
    }
    if lower == "eval" {
        return material(if matches!(language.as_str(), "py" | "python") {
            "python"
        } else {
            "javascript"
        });
    }
    if lower.contains("grep") || lower.contains("search") {
        return material("search");
    }
    if lower.contains("glob") {
        return material("folder");
    }
    if lower.contains("todo") || lower.starts_with("task") || lower.contains("plan") {
        return material("todo");
    }
    if lower.contains("skill") {
        return material("skill");
    }
    if lower.contains("question") || lower.contains("prompt") {
        return material("prompt");
    }
    if lower.contains("terminal") || lower.contains("shell") {
        return material("console");
    }
    if lower.contains("agent") || lower == "workers" {
        return material("robot");
    }
    material("settings")
}

pub(crate) fn tool_icon_descriptor(call: &ToolCall) -> ToolIconDescriptor {
    match call {
        ToolCall::Exec { command } => material(command_icon(command)),
        ToolCall::ReadFile { path }
        | ToolCall::WriteFile { path, .. }
        | ToolCall::EditFile { path, .. } => file(path),
        ToolCall::ApplyPatch { path: Some(path) } => file(path),
        ToolCall::ApplyPatch { path: None } => ToolIconDescriptor::Solar(crate::icons::PEN),
        ToolCall::Search { .. } | ToolCall::WebSearch { .. } => material("search"),
        ToolCall::Glob { .. } => material("folder"),
        ToolCall::WebFetch { .. } => material("chrome"),
        ToolCall::Todo { .. } => material("todo"),
        ToolCall::Mcp { tool, input, .. }
        | ToolCall::Unknown {
            name: tool, input, ..
        } => semantic_tool_icon(tool, input.as_ref()),
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use zeron_proto::ToolCall;

    use super::{ToolIconDescriptor, tool_icon_descriptor};

    fn material(name: &str) -> ToolIconDescriptor {
        ToolIconDescriptor::Material(format!("file-icons/{name}.svg").into())
    }

    #[test]
    fn compound_commands_use_the_first_recognized_executable() {
        for (command, expected) in [
            ("cd app && git log --oneline | head -10", "git"),
            ("env DEBUG=1 python3 -m pytest; git status", "python"),
            ("bash scripts/check.sh; git status", "console"),
            ("'/opt/homebrew/bin/node' script.mjs", "nodejs"),
            ("printf 'git | python' && cargo test", "rust"),
        ] {
            assert_eq!(
                tool_icon_descriptor(&ToolCall::Exec {
                    command: command.into(),
                }),
                material(expected),
                "{command}",
            );
        }
    }

    #[test]
    fn versioned_python_and_command_families_use_catalog_assets() {
        for (command, expected) in [
            ("python3.13 -m pytest", "python"),
            ("pip3 install httpx", "python"),
            ("pnpx playwright test", "pnpm"),
            ("docker compose up", "docker"),
            ("swift test", "swift"),
            ("uv run pytest", "uv"),
        ] {
            assert_eq!(
                tool_icon_descriptor(&ToolCall::Exec {
                    command: command.into(),
                }),
                material(expected),
            );
        }
    }

    #[test]
    fn file_calls_delegate_to_the_existing_material_manifest() {
        assert_eq!(
            tool_icon_descriptor(&ToolCall::ReadFile {
                path: "/repo/package.json".into(),
            }),
            material("nodejs"),
        );
        assert_eq!(
            tool_icon_descriptor(&ToolCall::EditFile {
                path: "/repo/scripts/check.py".into(),
                old_string: None,
                new_string: None,
            }),
            material("python"),
        );
        assert_eq!(
            tool_icon_descriptor(&ToolCall::WriteFile {
                path: "/repo/src/main.rs".into(),
                content: None,
            }),
            material("rust"),
        );
    }

    #[test]
    fn runtime_native_tools_get_semantic_icons() {
        for (name, input, expected) in [
            ("grep", json!({"pattern": "needle"}), "search"),
            ("glob", json!({"pattern": "**/*.rs"}), "folder"),
            ("todo", json!({"items": []}), "todo"),
            ("eval", json!({"language": "python"}), "python"),
            ("browser_tool", json!({"command": "snapshot"}), "chrome"),
            ("Skill", json!({"path": "brainstorming"}), "skill"),
        ] {
            assert_eq!(
                tool_icon_descriptor(&ToolCall::Unknown {
                    name: name.into(),
                    input: Some(input),
                }),
                material(expected),
                "{name}",
            );
        }
    }

    #[test]
    fn semantic_variants_and_unknown_fallback_remain_explicit() {
        assert_eq!(
            tool_icon_descriptor(&ToolCall::WebFetch {
                url: "https://example.com".into(),
                prompt: None,
            }),
            material("chrome"),
        );
        assert_eq!(
            tool_icon_descriptor(&ToolCall::Todo { items: Vec::new() }),
            material("todo"),
        );
        assert_eq!(
            tool_icon_descriptor(&ToolCall::Unknown {
                name: "not-a-real-tool".into(),
                input: None,
            }),
            material("settings"),
        );
        assert_eq!(
            tool_icon_descriptor(&ToolCall::ApplyPatch { path: None }),
            ToolIconDescriptor::Solar(crate::icons::PEN),
        );
    }

    #[test]
    fn every_material_descriptor_used_by_representative_calls_is_embedded() {
        let calls = [
            ToolCall::Exec {
                command: "git status".into(),
            },
            ToolCall::Exec {
                command: "python3 -m pytest".into(),
            },
            ToolCall::ReadFile {
                path: "package.json".into(),
            },
            ToolCall::Unknown {
                name: "browser_tool".into(),
                input: None,
            },
        ];
        for call in calls {
            let descriptor = tool_icon_descriptor(&call);
            let ToolIconDescriptor::Material(path) = &descriptor else {
                panic!("representative call did not resolve to Material: {call:?}");
            };
            assert!(
                descriptor.material_image().is_some(),
                "missing embedded asset {path}",
            );
        }
    }
}
