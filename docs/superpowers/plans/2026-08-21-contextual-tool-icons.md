# Contextual Tool Icons Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox syntax for tracking.

**Goal:** Render the most specific existing Material or Solar icon for every shared transcript ToolCall without changing any runtime adapter or persisted protocol.

**Architecture:** Add a presentation-only tool_icons module that converts the existing ToolCall enum into a ToolIconDescriptor. The transcript renderer consumes the descriptor and either paints an original-color embedded Material image or the current theme-tinted Solar SVG inside the unchanged chip tile.

**Tech Stack:** Rust 2024, GPUI, zeron-proto::ToolCall, embedded Material Icon Theme assets, existing Solar SVG assets, Cargo tests.

## Global Constraints

- Apply the resolver in the shared renderer so OMP, Claude Code, Codex, Pi, ACP, and future harnesses receive the same behavior.
- Do not change harness adapters, engine lifecycle, protocol variants, persisted documents, tool labels, grouping, output, or disclosure behavior.
- Reuse the bundled Material Icon Theme and Solar assets; add no dependency and create no artwork.
- Preserve the existing 18 px tile, 12 px glyph footprint, spacing, hover, running, failure, and expanded states.
- Use TDD: observe every resolver test fail before adding its implementation.
- Context7 library /websites/rs_gpui confirms StyledImage::object_fit and ObjectFit::Contain; use that API together with the repository's existing img(Arc<Image>) pattern.
- Run focused tests during edits and the complete workspace check once at the end.
- Keep all commits local; do not push, release, or promote to main.

---

## File Structure

- Create crates/ui/src/tool_icons.rs: shell parsing, semantic tool classification, file-icon resolution, and ToolIconDescriptor.
- Modify crates/ui/src/lib.rs: register the new UI module.
- Modify crates/ui/src/transcript.rs: render Material and Solar descriptors in the existing icon tile.
- Reuse crates/ui/src/details_sidebar/files_view.rs: canonical filename-to-Material-icon resolver.
- Reuse crates/ui/src/icons.rs: embedded Material images and Solar SVG elements.

### Task 1: Shared contextual icon resolver

**Files:**
- Create: crates/ui/src/tool_icons.rs
- Modify: crates/ui/src/lib.rs
- Test: crates/ui/src/tool_icons.rs

**Interfaces:**
- Consumes: zeron_proto::ToolCall, crate::details_sidebar::files_view::material_icon_path, and Solar paths from crate::icons.
- Produces: pub(crate) enum ToolIconDescriptor { Material(SharedString), Solar(&'static str) } and pub(crate) fn tool_icon_descriptor(call: &ToolCall) -> ToolIconDescriptor.

- [ ] **Step 1: Register the module and write the failing behavioral tests**

Add pub mod tool_icons; beside the other UI modules in crates/ui/src/lib.rs. Create crates/ui/src/tool_icons.rs initially with only these tests so the first test run fails because the production contract does not exist:

~~~rust
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
                tool_icon_descriptor(&ToolCall::Exec { command: command.into() }),
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
                tool_icon_descriptor(&ToolCall::Exec { command: command.into() }),
                material(expected),
            );
        }
    }

    #[test]
    fn file_calls_delegate_to_the_existing_material_manifest() {
        assert_eq!(
            tool_icon_descriptor(&ToolCall::ReadFile { path: "/repo/package.json".into() }),
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
}
~~~

- [ ] **Step 2: Run the focused test and verify RED**

Run:

~~~bash
cargo test -p zeron-ui tool_icons::tests --no-default-features
~~~

Expected: compilation fails with unresolved imports for ToolIconDescriptor and tool_icon_descriptor. This proves the tests target the missing shared resolver.

- [ ] **Step 3: Implement the descriptor, shell parser, and semantic mappings**

Add the production contract above the tests in crates/ui/src/tool_icons.rs. Keep helpers private and port the Orchestrator.dev parsing behavior rather than introducing a shell dependency:

~~~rust
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
        ToolCall::Mcp { tool, input, .. } | ToolCall::Unknown { name: tool, input } => {
            semantic_tool_icon(tool, input.as_ref())
        }
    }
}
~~~

Add these complete parsing helpers and command table. They scan byte-safe char boundaries, ignore separators inside quotes, skip wrappers and environment assignments, normalize executable basenames, and return the first recognized command family:

~~~rust
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
                || token
                    .split_once('=')
                    .is_some_and(|(name, _)| {
                        let mut chars = name.chars();
                        chars.next().is_some_and(|first| first == '_' || first.is_ascii_alphabetic())
                            && chars.all(|character| character == '_' || character.is_ascii_alphanumeric())
                    })
                || matches!(token, "command" | "env" | "exec" | "nohup" | "sudo" | "time")
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
~~~

Implement semantic_tool_icon case-insensitively with these ordered rules:

~~~rust
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
~~~

- [ ] **Step 4: Run the resolver tests and verify GREEN**

Run:

~~~bash
cargo test -p zeron-ui tool_icons::tests --no-default-features
~~~

Expected: all contextual resolver tests pass.

- [ ] **Step 5: Run formatting and focused neighboring tests**

Run:

~~~bash
cargo fmt --all -- --check
cargo test -p zeron-ui details_sidebar::files_view::tests --no-default-features
~~~

Expected: formatting and the canonical Material filename resolver tests pass.

- [ ] **Step 6: Commit the resolver**

~~~bash
git add crates/ui/src/lib.rs crates/ui/src/tool_icons.rs
git commit -m "feat: resolve contextual tool icons"
~~~

### Task 2: Render Material descriptors in transcript chips

**Files:**
- Modify: crates/ui/src/transcript.rs
- Test: crates/ui/src/tool_icons.rs

**Interfaces:**
- Consumes: crate::tool_icons::{ToolIconDescriptor, tool_icon_descriptor} from Task 1 and crate::icons::material_file_icon_image.
- Produces: every transcript tool chip renders a fixed-size Material image or Solar SVG selected by the shared descriptor.

- [ ] **Step 1: Reuse the verified GPUI image pattern**

Context7 library /websites/rs_gpui documents StyledImage::object_fit(ObjectFit::Contain), and the repository already compiles img(Arc<Image>) in details_sidebar/view.rs and file_preview/view.rs. Use exactly that pattern; no dependency or API exploration is required during implementation.

- [ ] **Step 2: Add the failing asset-availability regression**

Append this test to crates/ui/src/tool_icons.rs:

~~~rust
#[test]
fn every_material_descriptor_used_by_representative_calls_is_embedded() {
    let calls = [
        ToolCall::Exec { command: "git status".into() },
        ToolCall::Exec { command: "python3 -m pytest".into() },
        ToolCall::ReadFile { path: "package.json".into() },
        ToolCall::Unknown { name: "browser_tool".into(), input: None },
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
~~~

Run the focused test. Expected: compilation fails because ToolIconDescriptor has no material_image method. This is the rendering/asset seam the transcript will consume.

- [ ] **Step 3: Replace the raw Solar path renderer with descriptor rendering**

Implement the asset seam in crates/ui/src/tool_icons.rs:

~~~rust
impl ToolIconDescriptor {
    pub(crate) fn material_image(&self) -> Option<std::sync::Arc<gpui::Image>> {
        match self {
            Self::Material(path) => crate::icons::material_file_icon_image(path.as_ref()),
            Self::Solar(_) => None,
        }
    }
}
~~~

Remove tool_icon_path from crates/ui/src/transcript.rs. Add this presentation helper beside the existing chip renderer:

~~~rust
fn tool_icon(call: &ToolCall, theme: &Theme) -> AnyElement {
    let descriptor = crate::tool_icons::tool_icon_descriptor(call);
    match &descriptor {
        crate::tool_icons::ToolIconDescriptor::Material(path) => {
            let image = descriptor
                .material_image()
                .expect("resolved tool icon is embedded");
            img(image)
                .size(px(12.0))
                .object_fit(ObjectFit::Contain)
                .flex_none()
                .into_any_element()
        }
        crate::tool_icons::ToolIconDescriptor::Solar(path) => crate::icons::icon(*path)
            .size(px(12.0))
            .text_color(theme.text_muted)
            .into_any_element(),
    }
}
~~~

Replace only the icon tile child:

~~~rust
.child(tool_icon(&tool.call, theme))
~~~

Do not alter the tile dimensions, label/detail text, status tint, or surrounding layout.

- [ ] **Step 4: Verify the regression and transcript tests pass**

Run:

~~~bash
cargo test -p zeron-ui tool_icons::tests --no-default-features
cargo test -p zeron-ui transcript::tests --no-default-features
~~~

Expected: all resolver, embedded-asset, and transcript tests pass.

- [ ] **Step 5: Run the Impeccable mechanical detector once**

Run exactly once after the UI edit:

~~~bash
node /Users/guilhermevarela/.agents/skills/impeccable/scripts/detect.mjs --json crates/ui/src/transcript.rs crates/ui/src/tool_icons.rs
~~~

Expected: no actionable design-system drift. Fix only findings caused by this change and do not rerun the detector.

- [ ] **Step 6: Commit the renderer integration**

~~~bash
git add crates/ui/src/transcript.rs crates/ui/src/tool_icons.rs
git commit -m "feat: render contextual tool icons"
~~~

### Task 3: Integration and visual gate

**Files:**
- Verify only: all files changed in Tasks 1-2

**Interfaces:**
- Consumes: committed resolver and transcript renderer.
- Produces: current build/test evidence and a visually inspected dev app; no new behavior unless one bounded correction is required.

- [ ] **Step 1: Run source hygiene gates**

~~~bash
cargo fmt --all -- --check
git diff --check HEAD~2..HEAD
git status --short
~~~

Expected: formatting and whitespace checks pass; only intentional committed changes exist.

- [ ] **Step 2: Run focused UI and runtime regression gates**

~~~bash
cargo test -p zeron-ui tool_icons::tests --no-default-features
cargo test -p zeron-ui transcript::tests --no-default-features
cargo test -p zeron-harness --test omp_rpc
cargo test -p zeron-harness --test claude ask_user_question_round_trips_through_the_control_channel -- --exact
cargo test -p zeron-harness --test codex approvals_round_trip_as_input_requests -- --exact
~~~

Expected: every focused test passes; the OMP real smoke remains ignored unless explicitly selected.

- [ ] **Step 3: Run the complete compilation gate once**

~~~bash
cargo check --workspace
cargo build -p zeron
~~~

Expected: both commands succeed. Existing warnings may remain, but this change introduces no new warning.

- [ ] **Step 4: Restart the dev app from the feature branch**

Stop only the target/debug/zeron process launched from this worktree, then run:

~~~bash
RUST_LOG=warn cargo run -p zeron
~~~

Expected: the app starts from feat/native-omp-rpc-runtime and the OMP rail remains available.

- [ ] **Step 5: Inspect one representative transcript in a bounded visual pass**

Confirm in the running app:

- Git, Python, Node, browser, and console commands use their colored catalog assets when present.
- Read/Edit/Write use filename-specific assets.
- Grep, Glob, Todo, Eval, agent, and browser calls no longer use the generic widget when their semantics are known.
- Unknown calls retain a stable fallback.
- Icon tiles remain 18 px, aligned, unclipped, and coherent in success, failure, collapsed, and expanded states.

If one defect is found, make one grouped correction, rerun only its focused tests, rebuild, and perform one final confirmation. Stop after the second visual pass.

- [ ] **Step 6: Request code review and record the final state**

Use the requesting-code-review skill against the feature range beginning at 0a987c3. Address Critical/Important findings with focused tests, then confirm:

~~~bash
git status --short --branch
git log -4 --oneline
pgrep -fl 'target/debug/zeron' || true
~~~

Expected: clean feature branch, local commits only, and the dev process active if visual inspection left it running.
