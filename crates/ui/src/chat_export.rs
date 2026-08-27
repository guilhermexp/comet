use serde::Serialize;
use zeron_doc::{MessagePart, MessageRole, SessionMessageEntry};
use zeron_proto::{ToolCall, view::tool_chip_content};

const MAX_FILENAME_TITLE_CHARS: usize = 100;
const MAX_FILENAME_BYTES: usize = 255;
const HEAVY_OUTPUT_BYTE_THRESHOLD: u64 = 16 * 1024;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ChatMetadata {
    pub(crate) id: String,
    pub(crate) title: String,
    pub(crate) branch: String,
    pub(crate) cwd: String,
    pub(crate) exported_at: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum ArtifactKind {
    FileWrite,
    HeavyOutput,
    Subagent,
}

impl ArtifactKind {
    fn markdown_label(self) -> &'static str {
        match self {
            Self::FileWrite => "File write",
            Self::HeavyOutput => "Heavy output",
            Self::Subagent => "Subagent",
        }
    }

    fn text_label(self) -> &'static str {
        match self {
            Self::FileWrite => "file-write",
            Self::HeavyOutput => "heavy-output",
            Self::Subagent => "subagent",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Artifact {
    pub(crate) kind: ArtifactKind,
    pub(crate) message_ix: usize,
    pub(crate) part_ix: usize,
    pub(crate) tool: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) output_bytes: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) subagent_ref: Option<String>,
}

#[derive(Clone, Debug)]
pub(crate) struct ExportDoc {
    pub(crate) chat: ChatMetadata,
    pub(crate) messages: Vec<SessionMessageEntry>,
    pub(crate) artifacts: Vec<Artifact>,
}

impl ExportDoc {
    pub(crate) fn from_transcript(chat: ChatMetadata, entries: &[SessionMessageEntry]) -> Self {
        let mut messages = Vec::with_capacity(entries.len());
        let mut artifacts = Vec::new();

        for (message_ix, message) in entries.iter().enumerate() {
            for (part_ix, part) in message.parts.iter().enumerate() {
                let MessagePart::Tool {
                    call,
                    output_bytes,
                    diff_stats,
                    file_preview,
                    subagent_ref,
                    ..
                } = part
                else {
                    continue;
                };
                let (tool, _) = tool_chip_content(call);

                let mut file_paths = Vec::new();
                if let Some(stats) = diff_stats {
                    for stat in stats {
                        if !file_paths.contains(&stat.path) {
                            file_paths.push(stat.path.clone());
                        }
                    }
                }
                if file_preview.is_some()
                    && let Some(path) = file_write_path(call)
                    && !file_paths.iter().any(|candidate| candidate == path)
                {
                    file_paths.push(path.to_owned());
                }
                for path in file_paths {
                    artifacts.push(Artifact {
                        kind: ArtifactKind::FileWrite,
                        message_ix,
                        part_ix,
                        tool: tool.to_owned(),
                        path: Some(path),
                        output_bytes: None,
                        subagent_ref: None,
                    });
                }

                if let Some(subagent_ref) = subagent_ref {
                    artifacts.push(Artifact {
                        kind: ArtifactKind::Subagent,
                        message_ix,
                        part_ix,
                        tool: tool.to_owned(),
                        path: None,
                        output_bytes: None,
                        subagent_ref: Some(subagent_ref.clone()),
                    });
                }

                if output_bytes.is_some_and(|bytes| bytes > HEAVY_OUTPUT_BYTE_THRESHOLD) {
                    artifacts.push(Artifact {
                        kind: ArtifactKind::HeavyOutput,
                        message_ix,
                        part_ix,
                        tool: tool.to_owned(),
                        path: None,
                        output_bytes: *output_bytes,
                        subagent_ref: None,
                    });
                }
            }
            messages.push(message.clone());
        }

        Self {
            chat,
            messages,
            artifacts,
        }
    }
}

fn file_write_path(call: &ToolCall) -> Option<&str> {
    match call {
        ToolCall::WriteFile { path, .. } | ToolCall::EditFile { path, .. } => Some(path),
        ToolCall::ApplyPatch { path } => path.as_deref(),
        _ => None,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ExportFormat {
    Markdown,
    Text,
    Json,
}

impl ExportFormat {
    fn extension(self) -> &'static str {
        match self {
            Self::Markdown => "md",
            Self::Text => "txt",
            Self::Json => "json",
        }
    }
}

pub(crate) fn render_markdown(doc: &ExportDoc) -> String {
    let mut rendered = format!(
        "# {}\n\n**Exported:** {}\n**Project:** {}\n**Branch:** {}\n\n---\n\n## Artifacts\n\n",
        doc.chat.title, doc.chat.exported_at, doc.chat.cwd, doc.chat.branch
    );
    if doc.artifacts.is_empty() {
        rendered.push_str("_None._\n");
    } else {
        for artifact in &doc.artifacts {
            rendered.push_str(&render_markdown_artifact(artifact));
        }
    }

    for message in &doc.messages {
        let author = match message.role {
            MessageRole::User => "You",
            MessageRole::Assistant => "Assistant",
            MessageRole::System => "System",
        };
        rendered.push_str(&format!("\n### **{author}**\n\n"));
        for part in &message.parts {
            match part {
                MessagePart::Text { text, .. } => {
                    rendered.push_str(text);
                    rendered.push('\n');
                }
                MessagePart::Tool { call, .. } => {
                    rendered.push_str(&render_markdown_tool(call));
                }
                MessagePart::Reasoning { .. }
                | MessagePart::Input { .. }
                | MessagePart::Error { .. }
                | MessagePart::WorkflowTask { .. } => {}
            }
        }
    }

    rendered
}

fn render_markdown_artifact(artifact: &Artifact) -> String {
    let mut rendered = format!(
        "- **{}** `{}`",
        artifact.kind.markdown_label(),
        artifact.tool
    );
    if let Some(path) = &artifact.path {
        rendered.push_str(&format!(" `{path}`"));
    }
    if let Some(subagent_ref) = &artifact.subagent_ref {
        rendered.push_str(&format!(" `{subagent_ref}`"));
    }
    if let Some(output_bytes) = artifact.output_bytes {
        rendered.push_str(&format!(" · {output_bytes} bytes"));
    }
    rendered.push_str(&format!(
        " · message {}, part {}\n",
        artifact.message_ix, artifact.part_ix
    ));
    rendered
}

fn longest_backtick_run(text: &str) -> usize {
    let mut longest = 0;
    let mut current = 0;
    for ch in text.chars() {
        if ch == '`' {
            current += 1;
            longest = longest.max(current);
        } else {
            current = 0;
        }
    }
    longest
}

fn render_markdown_tool(call: &ToolCall) -> String {
    match call {
        ToolCall::Exec { command } => {
            let fence = "`".repeat(longest_backtick_run(command).max(2) + 1);
            format!("{fence}bash\n{command}\n{fence}\n\n")
        }
        ToolCall::WriteFile { path, .. } | ToolCall::EditFile { path, .. } => {
            format!("> Modified: `{path}`\n\n")
        }
        ToolCall::ReadFile { path } => format!("> Read: `{path}`\n\n"),
        other => {
            let (label, _) = tool_chip_content(other);
            format!("> *Used {label} tool*\n\n")
        }
    }
}

pub(crate) fn render_text(doc: &ExportDoc) -> String {
    let mut rendered = format!(
        "{}\nExported: {}\nProject: {}\nBranch: {}\n\n---\n\nARTIFACTS:\n",
        doc.chat.title, doc.chat.exported_at, doc.chat.cwd, doc.chat.branch
    );
    if doc.artifacts.is_empty() {
        rendered.push_str("(none)\n");
    } else {
        for artifact in &doc.artifacts {
            rendered.push_str(&render_text_artifact(artifact));
        }
    }

    for message in &doc.messages {
        let author = match message.role {
            MessageRole::User => "You",
            MessageRole::Assistant => "Assistant",
            MessageRole::System => "System",
        };
        rendered.push_str(&format!("\n{author}:\n"));
        for part in &message.parts {
            match part {
                MessagePart::Text { text, .. } => {
                    rendered.push_str(text);
                    rendered.push('\n');
                }
                MessagePart::Tool { call, .. } => {
                    let (label, _) = tool_chip_content(call);
                    rendered.push_str(&format!("[used {label} tool]\n"));
                }
                MessagePart::Reasoning { .. }
                | MessagePart::Input { .. }
                | MessagePart::Error { .. }
                | MessagePart::WorkflowTask { .. } => {}
            }
        }
    }

    rendered
}

fn render_text_artifact(artifact: &Artifact) -> String {
    let mut rendered = format!("- {} {}", artifact.kind.text_label(), artifact.tool);
    if let Some(path) = &artifact.path {
        rendered.push_str(&format!(" {path}"));
    }
    if let Some(subagent_ref) = &artifact.subagent_ref {
        rendered.push_str(&format!(" {subagent_ref}"));
    }
    if let Some(output_bytes) = artifact.output_bytes {
        rendered.push_str(&format!(" ({output_bytes} bytes)"));
    }
    rendered.push_str(&format!(
        " @ message {}, part {}\n",
        artifact.message_ix, artifact.part_ix
    ));
    rendered
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct JsonExport<'a> {
    exported_at: &'a str,
    chat: JsonChat<'a>,
    artifact_index: &'a [Artifact],
    messages: &'a [SessionMessageEntry],
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct JsonChat<'a> {
    id: &'a str,
    title: &'a str,
    branch: &'a str,
    cwd: &'a str,
}

pub(crate) fn render_json(doc: &ExportDoc) -> Result<String, serde_json::Error> {
    serde_json::to_string_pretty(&JsonExport {
        exported_at: &doc.chat.exported_at,
        chat: JsonChat {
            id: &doc.chat.id,
            title: &doc.chat.title,
            branch: &doc.chat.branch,
            cwd: &doc.chat.cwd,
        },
        artifact_index: &doc.artifacts,
        messages: &doc.messages,
    })
}

fn sanitize_filename_component(raw: &str) -> String {
    let mut sanitized = String::new();
    let mut previous_was_separator = false;

    for ch in raw.chars() {
        let invalid = ch.is_control()
            || ch.is_whitespace()
            || matches!(ch, '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*');
        if invalid {
            if !sanitized.is_empty() && !previous_was_separator {
                sanitized.push('_');
            }
            previous_was_separator = true;
        } else {
            sanitized.push(ch);
            previous_was_separator = false;
        }
    }

    sanitized
}

pub(crate) fn build_filename(title: &str, chat_id: &str, format: ExportFormat) -> String {
    let sanitized = sanitize_filename_component(title);

    let id_prefix = sanitize_filename_component(chat_id)
        .trim_matches('_')
        .chars()
        .take(8)
        .collect::<String>();
    let id_prefix = if id_prefix.is_empty() {
        "id".to_owned()
    } else {
        id_prefix
    };
    let suffix = format!("-{id_prefix}.{}", format.extension());
    let max_title_bytes = MAX_FILENAME_BYTES.saturating_sub(suffix.len());
    let mut capped = String::new();
    for ch in sanitized
        .trim_matches('_')
        .chars()
        .take(MAX_FILENAME_TITLE_CHARS)
    {
        if capped.len() + ch.len_utf8() > max_title_bytes {
            break;
        }
        capped.push(ch);
    }
    let title = if capped.is_empty() { "chat" } else { &capped };

    format!("{title}{suffix}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use zeron_doc::{
        FileChangeKind, FileChangePreview, MessagePart, MessageRole, SessionMessageEntry,
        ToolDiffStat,
    };
    use zeron_proto::ToolCall;

    fn message(id: &str, role: MessageRole, text: &str) -> SessionMessageEntry {
        SessionMessageEntry {
            id: id.to_owned(),
            role,
            parts: vec![MessagePart::Text {
                id: format!("{id}-text"),
                text: text.to_owned(),
            }],
            created_at: 1_725_000_000_000,
            device_id: "device-1".to_owned(),
            status: None,
            duration_ms: None,
            continuation_of: None,
        }
    }

    fn metadata(title: &str) -> ChatMetadata {
        ChatMetadata {
            id: "chat-0123456789".to_owned(),
            title: title.to_owned(),
            branch: "change/export".to_owned(),
            cwd: "/work/zeron".to_owned(),
            exported_at: "2026-08-26T12:34:56Z".to_owned(),
        }
    }

    fn tool_part(
        id: &str,
        call: ToolCall,
        output: Option<&str>,
        output_ref: Option<&str>,
        output_bytes: Option<u64>,
        diff_stats: Option<Vec<ToolDiffStat>>,
        file_preview: Option<FileChangePreview>,
        subagent_ref: Option<&str>,
    ) -> MessagePart {
        MessagePart::Tool {
            id: id.to_owned(),
            call,
            is_error: false,
            resolved: true,
            execution: None,
            output: output.map(str::to_owned),
            diff: None,
            output_ref: output_ref.map(str::to_owned),
            output_bytes,
            diff_ref: None,
            diff_stats,
            file_preview,
            subagent_ref: subagent_ref.map(str::to_owned),
            subagent_status: None,
            subagent_tail: None,
        }
    }

    #[test]
    fn export_doc_keeps_message_order_and_has_no_artifacts_for_text_only_chat() {
        let entries = vec![
            message("m-user", MessageRole::User, "First"),
            message("m-assistant", MessageRole::Assistant, "Second"),
        ];

        let doc = ExportDoc::from_transcript(metadata("A Chat"), &entries);

        assert_eq!(doc.messages.len(), 2);
        assert_eq!(doc.messages[0].id, "m-user");
        assert_eq!(doc.messages[1].id, "m-assistant");
        assert!(doc.artifacts.is_empty());
    }

    #[test]
    fn markdown_keeps_the_chat_spine_and_explicit_empty_artifact_section() {
        let entries = vec![message("m-user", MessageRole::User, "Hello")];
        let doc = ExportDoc::from_transcript(metadata("A Chat"), &entries);

        let markdown = render_markdown(&doc);

        assert!(markdown.starts_with("# A Chat\n"));
        assert!(markdown.contains("**Exported:** 2026-08-26T12:34:56Z\n"));
        assert!(markdown.contains("**Project:** /work/zeron\n"));
        assert!(markdown.contains("**Branch:** change/export\n"));
        assert!(markdown.contains("## Artifacts\n\n_None._\n"));
        assert!(markdown.contains("### **You**\n\nHello\n"));
    }

    #[test]
    fn filename_falls_back_sanitizes_and_caps_the_title() {
        assert_eq!(
            build_filename("", "chat-0123456789", ExportFormat::Markdown),
            "chat-chat-012.md"
        );
        assert_eq!(
            build_filename(" <>:\"/\\|?* ", "abcdef012345", ExportFormat::Text),
            "chat-abcdef01.txt"
        );

        let long_title = "a".repeat(120);
        let filename = build_filename(&long_title, "1234567890", ExportFormat::Json);
        assert_eq!(filename, format!("{}-12345678.json", "a".repeat(100)));
    }

    #[test]
    fn filename_replaces_control_characters_and_collapses_separator_runs() {
        assert_eq!(
            build_filename(
                "\0 hello\u{001f}\tworld ",
                "abcdef012345",
                ExportFormat::Markdown,
            ),
            "hello_world-abcdef01.md"
        );
    }

    #[test]
    fn filename_keeps_the_complete_name_within_common_filesystem_byte_limits() {
        let filename = build_filename(&"😀".repeat(120), "abcdef012345", ExportFormat::Markdown);

        assert!(filename.len() <= 255);
        assert!(filename.ends_with("-abcdef01.md"));
    }

    #[test]
    fn filename_sanitizes_the_chat_id_like_the_title() {
        assert_eq!(
            build_filename("notes", "../../etc/pw", ExportFormat::Markdown),
            "notes-.._.._et.md"
        );
        assert_eq!(
            build_filename("notes", " \t\n", ExportFormat::Markdown),
            "notes-id.md"
        );
    }

    #[test]
    fn markdown_exec_fence_survives_a_command_carrying_triple_backticks() {
        let command = "cat <<'EOF'\n```rust\nfn main() {}\n```\nEOF";
        let mut assistant = message("m-assistant", MessageRole::Assistant, "");
        assistant.parts = vec![tool_part(
            "exec",
            ToolCall::Exec {
                command: command.to_owned(),
            },
            None,
            None,
            None,
            None,
            None,
            None,
        )];
        let after = message("m-user", MessageRole::User, "still readable");

        let markdown = render_markdown(&ExportDoc::from_transcript(
            metadata("Fences"),
            &[assistant, after],
        ));

        assert!(
            markdown.contains(&format!("````bash\n{command}\n````")),
            "exec fence must outgrow the longest backtick run in the command: {markdown}"
        );
        let body = markdown.split("````bash").nth(1).expect("exec block");
        let closing = body.split_once("\n````").expect("closing fence").1;
        assert!(
            closing.contains("### **You**\n\nstill readable"),
            "content after the exec block must stay outside the fence: {closing}"
        );
    }

    #[test]
    fn artifact_index_records_files_subagents_and_heavy_outputs_by_ordinal() {
        let write = tool_part(
            "write",
            ToolCall::WriteFile {
                path: "src/export.rs".to_owned(),
                content: None,
            },
            None,
            None,
            None,
            Some(vec![ToolDiffStat {
                path: "src/export.rs".to_owned(),
                additions: 12,
                deletions: 0,
            }]),
            Some(FileChangePreview {
                kind: FileChangeKind::Write,
                lines: Vec::new(),
                total_lines: 12,
                additions: 12,
                deletions: 0,
                truncated_before: 0,
            }),
            None,
        );
        let agent = tool_part(
            "agent",
            ToolCall::Unknown {
                name: "Agent: inspect".to_owned(),
                input: None,
            },
            None,
            None,
            None,
            None,
            None,
            Some("subagent-doc-1"),
        );
        let heavy = tool_part(
            "read",
            ToolCall::ReadFile {
                path: "report.txt".to_owned(),
            },
            Some("one-line summary that must not render"),
            Some("chat/sidecar-output"),
            Some(16 * 1024 + 1),
            None,
            None,
            None,
        );
        let mut entry = message("m-assistant", MessageRole::Assistant, "Done");
        entry.parts = vec![write, agent, heavy];

        let doc = ExportDoc::from_transcript(metadata("Artifacts"), &[entry]);

        assert_eq!(doc.artifacts.len(), 3);
        assert_eq!(doc.artifacts[0].kind, ArtifactKind::FileWrite);
        assert_eq!(doc.artifacts[0].path.as_deref(), Some("src/export.rs"));
        assert_eq!(
            (doc.artifacts[0].message_ix, doc.artifacts[0].part_ix),
            (0, 0)
        );
        assert_eq!(doc.artifacts[1].kind, ArtifactKind::Subagent);
        assert_eq!(
            doc.artifacts[1].subagent_ref.as_deref(),
            Some("subagent-doc-1")
        );
        assert_eq!(
            (doc.artifacts[1].message_ix, doc.artifacts[1].part_ix),
            (0, 1)
        );
        assert_eq!(doc.artifacts[2].kind, ArtifactKind::HeavyOutput);
        assert_eq!(doc.artifacts[2].output_bytes, Some(16 * 1024 + 1));
        assert_eq!(
            (doc.artifacts[2].message_ix, doc.artifacts[2].part_ix),
            (0, 2)
        );

        let markdown = render_markdown(&doc);
        assert!(markdown.contains("src/export.rs"));
        assert!(markdown.contains("subagent-doc-1"));
        assert!(markdown.contains("16385 bytes"));
        assert!(!markdown.contains("chat/sidecar-output"));
        assert!(!markdown.contains("one-line summary that must not render"));
    }

    #[test]
    fn markdown_shapes_tool_variants_without_verbose_or_stripped_payloads() {
        let tools = vec![
            tool_part(
                "exec",
                ToolCall::Exec {
                    command: "cargo test -p zeron-ui".to_owned(),
                },
                Some("verbose exec output"),
                None,
                None,
                None,
                None,
                None,
            ),
            tool_part(
                "edit",
                ToolCall::EditFile {
                    path: "src/lib.rs".to_owned(),
                    old_string: None,
                    new_string: None,
                },
                Some("verbose edit output"),
                None,
                None,
                None,
                None,
                None,
            ),
            tool_part(
                "write",
                ToolCall::WriteFile {
                    path: "src/new.rs".to_owned(),
                    content: None,
                },
                Some("verbose write output"),
                None,
                None,
                None,
                None,
                None,
            ),
            tool_part(
                "read",
                ToolCall::ReadFile {
                    path: "README.md".to_owned(),
                },
                None,
                None,
                None,
                None,
                None,
                None,
            ),
            tool_part(
                "unknown",
                ToolCall::Unknown {
                    name: "custom-secret-tool".to_owned(),
                    input: None,
                },
                None,
                None,
                None,
                None,
                None,
                None,
            ),
        ];
        let mut entry = message("m-assistant", MessageRole::Assistant, "");
        entry.parts = tools;

        let markdown = render_markdown(&ExportDoc::from_transcript(metadata("Tools"), &[entry]));

        assert!(markdown.contains("```bash\ncargo test -p zeron-ui\n```"));
        assert!(markdown.contains("> Modified: `src/lib.rs`"));
        assert!(markdown.contains("> Modified: `src/new.rs`"));
        assert!(markdown.contains("> Read: `README.md`"));
        assert!(markdown.contains("> *Used Tool tool*"));
        assert!(!markdown.contains("verbose exec output"));
        assert!(!markdown.contains("verbose edit output"));
        assert!(!markdown.contains("verbose write output"));
    }

    #[test]
    fn all_formats_cover_the_same_messages_and_artifacts_in_order() {
        let write = tool_part(
            "write",
            ToolCall::WriteFile {
                path: "src/shared.rs".to_owned(),
                content: None,
            },
            None,
            None,
            None,
            Some(vec![ToolDiffStat {
                path: "src/shared.rs".to_owned(),
                additions: 3,
                deletions: 1,
            }]),
            None,
            None,
        );
        let agent = tool_part(
            "agent",
            ToolCall::Unknown {
                name: "Agent: verify".to_owned(),
                input: None,
            },
            Some("bounded summary"),
            Some("chat/subagent-sidecar"),
            None,
            None,
            None,
            Some("subagent-doc-shared"),
        );
        let mut first = message("message-first", MessageRole::User, "FIRST MESSAGE");
        first.parts.push(write);
        let mut second = message("message-second", MessageRole::Assistant, "SECOND MESSAGE");
        second.parts.push(agent);
        let doc = ExportDoc::from_transcript(metadata("Shared"), &[first, second]);

        let markdown = render_markdown(&doc);
        let text = render_text(&doc);
        let json = render_json(&doc).expect("the export document is JSON serializable");
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

        let message_counts = [
            markdown.matches("### **").count(),
            text.lines()
                .filter(|line| matches!(*line, "You:" | "Assistant:" | "System:"))
                .count(),
            parsed["messages"].as_array().unwrap().len(),
        ];
        assert_eq!(message_counts, [2, 2, 2]);

        assert!(markdown.contains(
            "## Artifacts\n\n\
- **File write** `Write` `src/shared.rs` · message 0, part 1\n\
- **Subagent** `Agent` `subagent-doc-shared` · message 1, part 1\n"
        ));
        assert!(text.contains(
            "ARTIFACTS:\n\
- file-write Write src/shared.rs @ message 0, part 1\n\
- subagent Agent subagent-doc-shared @ message 1, part 1\n"
        ));

        for rendered in [&markdown, &text, &json] {
            assert!(rendered.contains("src/shared.rs"));
            assert!(rendered.contains("subagent-doc-shared"));
            assert!(
                rendered.find("FIRST MESSAGE").unwrap() < rendered.find("SECOND MESSAGE").unwrap()
            );
        }

        assert_eq!(parsed["exportedAt"], "2026-08-26T12:34:56Z");
        assert_eq!(parsed["chat"]["id"], "chat-0123456789");
        assert_eq!(parsed["artifactIndex"].as_array().unwrap().len(), 2);
        assert_eq!(parsed["artifactIndex"][0]["kind"], "fileWrite");
        assert_eq!(parsed["artifactIndex"][0]["path"], "src/shared.rs");
        assert_eq!(parsed["artifactIndex"][0]["messageIx"], 0);
        assert_eq!(parsed["artifactIndex"][0]["partIx"], 1);
        assert_eq!(parsed["artifactIndex"][1]["kind"], "subagent");
        assert_eq!(
            parsed["artifactIndex"][1]["subagentRef"],
            "subagent-doc-shared"
        );
        assert_eq!(parsed["artifactIndex"][1]["messageIx"], 1);
        assert_eq!(parsed["artifactIndex"][1]["partIx"], 1);
        assert_eq!(parsed["messages"][1]["parts"][1]["call"]["kind"], "unknown");
        assert_eq!(
            parsed["messages"][1]["parts"][1]["outputRef"],
            "chat/subagent-sidecar"
        );
        assert!(
            json.lines()
                .any(|line| line.starts_with("  \"exportedAt\""))
        );
    }
}
