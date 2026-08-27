use std::collections::HashMap;

use zeron_doc::{MessagePart, MessageStatus};
use zeron_proto::ToolCall;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TurnStepsPlan {
    pub split_before_part: usize,
    pub summary: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ActivityBucket {
    Agent,
    Skill,
    Read,
    Search,
    Edit,
    Command,
    Wait,
    Message,
    Terminal,
    Capture,
    Tool,
}

pub fn plan_turn_steps(
    parts: &[MessagePart],
    status: Option<MessageStatus>,
) -> Option<TurnStepsPlan> {
    // Um turno em voo fica inteiro aberto. Dobrar o prefixo já assentado
    // enquanto o agente trabalha escondia exatamente a saida que a run esta
    // produzindo: sobrava so a chamada corrente, uma por vez. Dobrar e a
    // affordance do turno TERMINADO, logo abaixo.
    if status == Some(MessageStatus::Streaming) {
        return None;
    }

    let last_tool = parts
        .iter()
        .rposition(|part| matches!(part, MessagePart::Tool { .. }))?;
    let last_text = parts.iter().rposition(
        |part| matches!(part, MessagePart::Text { text, .. } if !text.trim().is_empty()),
    )?;
    if last_text <= last_tool {
        return None;
    }
    let prefix = &parts[..last_text];
    if prefix.iter().any(is_unsettled_part) {
        return None;
    }

    Some(TurnStepsPlan {
        split_before_part: last_text,
        summary: turn_summary(prefix),
    })
}

fn turn_summary(parts: &[MessagePart]) -> String {
    let breakdown = activity_breakdown(parts);
    if !breakdown.is_empty() {
        return breakdown;
    }

    let steps = parts.iter().filter(|part| is_visible_part(part)).count();
    format!("{steps} {}", if steps == 1 { "step" } else { "steps" })
}

fn is_visible_part(part: &MessagePart) -> bool {
    match part {
        MessagePart::Text { text, .. }
        | MessagePart::Reasoning { text, .. }
        | MessagePart::Error { message: text, .. } => !text.trim().is_empty(),
        MessagePart::Tool { .. } | MessagePart::Input { .. } => true,
        MessagePart::WorkflowTask { .. } => false,
    }
}

fn is_unsettled_part(part: &MessagePart) -> bool {
    match part {
        MessagePart::Input { resolved, .. } => !resolved,
        MessagePart::Tool { .. }
        | MessagePart::Reasoning { .. }
        | MessagePart::Text { .. }
        | MessagePart::Error { .. }
        | MessagePart::WorkflowTask { .. } => false,
    }
}

pub fn activity_breakdown(parts: &[MessagePart]) -> String {
    let mut counts = HashMap::<ActivityBucket, usize>::new();
    for call in parts.iter().filter_map(|part| match part {
        MessagePart::Tool { call, .. } => Some(call),
        _ => None,
    }) {
        *counts.entry(activity_bucket(call)).or_default() += 1;
    }

    ActivityBucket::ORDER
        .into_iter()
        .filter_map(|bucket| {
            let count = counts.get(&bucket).copied()?;
            let label = if count == 1 {
                bucket.singular()
            } else {
                bucket.plural()
            };
            Some(format!("{count} {label}"))
        })
        .collect::<Vec<_>>()
        .join(", ")
}

fn activity_bucket(call: &ToolCall) -> ActivityBucket {
    match call {
        ToolCall::ReadFile { .. } | ToolCall::Search { .. } | ToolCall::Glob { .. } => {
            ActivityBucket::Read
        }
        ToolCall::WebSearch { .. } | ToolCall::WebFetch { .. } => ActivityBucket::Search,
        ToolCall::WriteFile { .. } | ToolCall::EditFile { .. } | ToolCall::ApplyPatch { .. } => {
            ActivityBucket::Edit
        }
        ToolCall::Exec { .. } => ActivityBucket::Command,
        ToolCall::Todo { .. } => ActivityBucket::Tool,
        ToolCall::Mcp { tool, input, .. } => named_activity_bucket(tool, input.as_ref()),
        ToolCall::Unknown { name, input } => named_activity_bucket(name, input.as_ref()),
    }
}

fn named_activity_bucket(name: &str, input: Option<&serde_json::Value>) -> ActivityBucket {
    let normalized = name
        .trim()
        .rsplit("__")
        .next()
        .unwrap_or(name)
        .to_ascii_lowercase();

    if normalized == "hub" {
        return match input
            .and_then(|value| value.get("op"))
            .and_then(serde_json::Value::as_str)
            .map(str::to_ascii_lowercase)
            .as_deref()
        {
            Some("wait") => ActivityBucket::Wait,
            Some("send") => ActivityBucket::Message,
            Some("start" | "restart" | "stop") => ActivityBucket::Command,
            _ => ActivityBucket::Tool,
        };
    }

    match normalized.as_str() {
        "agent" | "task" | "create_agent" | "spawn_agent" | "spawn_subagent" => {
            ActivityBucket::Agent
        }
        name if name.starts_with("agent:") || name.starts_with("task:") => ActivityBucket::Agent,
        "skill" => ActivityBucket::Skill,
        "read" | "grep" | "glob" | "search" => ActivityBucket::Read,
        "websearch" | "web_search" | "webfetch" | "web_fetch" => ActivityBucket::Search,
        "edit" | "write" | "multiedit" | "notebookedit" | "apply_patch" => ActivityBucket::Edit,
        "bash" | "exec" | "eval" | "create_terminal" => ActivityBucket::Command,
        "wait_for_agent" => ActivityBucket::Wait,
        "send_agent_prompt" | "send_terminal_keys" => ActivityBucket::Message,
        "list_terminals" => ActivityBucket::Terminal,
        "capture_terminal" => ActivityBucket::Capture,
        _ => ActivityBucket::Tool,
    }
}

impl ActivityBucket {
    const ORDER: [Self; 11] = [
        Self::Agent,
        Self::Skill,
        Self::Read,
        Self::Search,
        Self::Edit,
        Self::Command,
        Self::Wait,
        Self::Message,
        Self::Terminal,
        Self::Capture,
        Self::Tool,
    ];

    const fn singular(self) -> &'static str {
        match self {
            Self::Agent => "agent",
            Self::Skill => "skill",
            Self::Read => "read",
            Self::Search => "search",
            Self::Edit => "edit",
            Self::Command => "command",
            Self::Wait => "wait",
            Self::Message => "message",
            Self::Terminal => "terminal",
            Self::Capture => "capture",
            Self::Tool => "tool",
        }
    }

    const fn plural(self) -> &'static str {
        match self {
            Self::Agent => "agents",
            Self::Skill => "skills",
            Self::Read => "reads",
            Self::Search => "searches",
            Self::Edit => "edits",
            Self::Command => "commands",
            Self::Wait => "waits",
            Self::Message => "messages",
            Self::Terminal => "terminals",
            Self::Capture => "captures",
            Self::Tool => "tools",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use zeron_doc::SubagentStatus;

    fn text(id: &str, value: &str) -> MessagePart {
        MessagePart::Text {
            id: id.into(),
            text: value.into(),
        }
    }

    fn tool(id: &str, call: ToolCall, resolved: bool) -> MessagePart {
        MessagePart::Tool {
            id: id.into(),
            call,
            is_error: false,
            resolved,
            execution: None,
            output: None,
            diff: None,
            output_ref: None,
            output_bytes: None,
            diff_ref: None,
            diff_stats: None,
            file_preview: None,
            subagent_ref: None,
            subagent_status: None,
            subagent_tail: None,
        }
    }

    fn read(path: &str) -> ToolCall {
        ToolCall::ReadFile { path: path.into() }
    }

    fn exec(command: &str) -> ToolCall {
        ToolCall::Exec {
            command: command.into(),
        }
    }

    fn reasoning(id: &str, completed: bool) -> MessagePart {
        MessagePart::Reasoning {
            id: id.into(),
            text: "Considering the next step".into(),
            completed,
            duration_ms: None,
        }
    }

    fn input(id: &str, resolved: bool) -> MessagePart {
        MessagePart::Input {
            id: id.into(),
            request_id: format!("request-{id}"),
            questions: Vec::new(),
            resolved,
        }
    }

    fn running_subagent(id: &str) -> MessagePart {
        let mut part = tool(
            id,
            ToolCall::Unknown {
                name: "Agent: reviewer".into(),
                input: None,
            },
            true,
        );
        let MessagePart::Tool {
            subagent_ref,
            subagent_status,
            ..
        } = &mut part
        else {
            unreachable!()
        };
        *subagent_ref = Some("subagent-doc".into());
        *subagent_status = Some(SubagentStatus::Running);
        part
    }

    fn unknown(name: &str, input: Option<serde_json::Value>) -> ToolCall {
        ToolCall::Unknown {
            name: name.into(),
            input,
        }
    }

    fn mcp(tool: &str) -> ToolCall {
        ToolCall::Mcp {
            server: "orchestrator".into(),
            tool: tool.into(),
            input: None,
        }
    }

    #[test]
    fn settled_turn_folds_everything_before_text_after_the_last_tool() {
        let parts = vec![
            text("narration", "Inspecting"),
            tool("read", read("src/lib.rs"), true),
            text("answer", "The issue is fixed."),
        ];

        let plan = plan_turn_steps(&parts, Some(MessageStatus::Complete)).unwrap();
        assert_eq!(plan.split_before_part, 2);
        assert_eq!(plan.summary, "1 read");
    }

    #[test]
    fn settled_turn_without_text_after_its_last_tool_stays_unwrapped() {
        let parts = vec![
            text("narration", "Inspecting"),
            tool("read", read("src/lib.rs"), true),
        ];

        assert_eq!(plan_turn_steps(&parts, Some(MessageStatus::Complete)), None);
    }

    #[test]
    fn settled_turn_ignores_whitespace_after_the_last_tool() {
        let parts = vec![
            text("narration", "Inspecting"),
            tool("read", read("src/lib.rs"), true),
            text("empty", "  \n"),
        ];

        assert_eq!(plan_turn_steps(&parts, Some(MessageStatus::Aborted)), None);
    }

    #[test]
    fn settled_turn_uses_the_last_non_empty_text_after_the_last_tool() {
        let parts = vec![
            tool("read", read("src/lib.rs"), true),
            text("first-answer", "First answer"),
            text("empty", "  "),
            text("last-answer", "Final answer"),
        ];

        let plan = plan_turn_steps(&parts, Some(MessageStatus::Complete)).unwrap();
        assert_eq!(plan.split_before_part, 3);
    }

    #[test]
    fn settled_turn_never_folds_a_pending_input() {
        for status in [MessageStatus::Complete, MessageStatus::Aborted] {
            let parts = vec![
                tool("read", read("src/lib.rs"), true),
                input("active-input", false),
                text("answer", "Final answer"),
            ];

            assert_eq!(
                plan_turn_steps(&parts, Some(status)),
                None,
                "settled {status:?} must keep {} visible",
                parts[1].id()
            );
        }
    }

    #[test]
    fn terminated_turn_folds_activity_the_dead_run_left_unresolved() {
        for status in [MessageStatus::Complete, MessageStatus::Aborted] {
            let stalled_cases = vec![
                tool("active-tool", exec("cargo check"), false),
                reasoning("active-reasoning", false),
                running_subagent("active-subagent"),
            ];

            for stalled in stalled_cases {
                let parts = vec![
                    tool("read", read("src/lib.rs"), true),
                    stalled,
                    text("answer", "Final answer"),
                ];
                let id = parts[1].id().to_owned();

                let plan = plan_turn_steps(&parts, Some(status)).unwrap_or_else(|| {
                    panic!("terminated {status:?} must fold {id} into the steps chip")
                });
                assert_eq!(plan.split_before_part, 2);
            }
        }
    }

    #[test]
    fn streaming_turn_never_folds_anything() {
        // Toda forma que antes virava StreamingPrefix: tool em voo, texto
        // corrente, reasoning ativo, input pendente, subagente rodando.
        let shapes = [
            vec![
                tool("old", exec("cargo check"), true),
                tool("first", exec("cargo test -p zeron-ui"), false),
                tool("second", read("Cargo.toml"), false),
            ],
            vec![
                tool("read", read("src/lib.rs"), true),
                text("latest", "Preparing the result"),
            ],
            vec![
                tool("read", read("src/lib.rs"), true),
                reasoning("thinking", false),
                tool("later", exec("cargo check"), true),
            ],
            vec![
                tool("read", read("src/lib.rs"), true),
                input("question", false),
                tool("later", exec("cargo check"), true),
            ],
            vec![
                tool("read", read("src/lib.rs"), true),
                running_subagent("agent"),
                tool("later", exec("cargo check"), true),
            ],
        ];

        for parts in shapes {
            assert_eq!(
                plan_turn_steps(&parts, Some(MessageStatus::Streaming)),
                None,
                "streaming turn must stay fully expanded"
            );
        }
    }

    #[test]
    fn activity_breakdown_uses_canonical_buckets_in_fixed_order() {
        let calls = vec![
            unknown("Agent: reviewer", None),
            unknown("Skill", None),
            read("src/lib.rs"),
            ToolCall::Search {
                pattern: "TurnSteps".into(),
                path: None,
            },
            ToolCall::WebSearch {
                query: "GPUI virtualization".into(),
            },
            ToolCall::WriteFile {
                path: "notes.md".into(),
                content: None,
            },
            ToolCall::EditFile {
                path: "src/lib.rs".into(),
                old_string: None,
                new_string: None,
            },
            exec("cargo check"),
            unknown("hub", Some(serde_json::json!({"op": "wait"}))),
            mcp("wait_for_agent"),
            mcp("send_agent_prompt"),
            mcp("list_terminals"),
            mcp("capture_terminal"),
            ToolCall::Todo { items: Vec::new() },
            mcp("list_pending_permissions"),
        ];
        let parts = calls
            .into_iter()
            .enumerate()
            .map(|(index, call)| tool(&format!("tool-{index}"), call, true))
            .collect::<Vec<_>>();

        assert_eq!(
            activity_breakdown(&parts),
            "1 agent, 1 skill, 2 reads, 1 search, 2 edits, 1 command, 2 waits, 1 message, 1 terminal, 1 capture, 2 tools"
        );
    }

    #[test]
    fn activity_breakdown_merges_builtin_and_mcp_agent_calls() {
        let parts = vec![
            tool("native", unknown("Agent", None), true),
            tool("mcp", mcp("create_agent"), true),
        ];

        assert_eq!(activity_breakdown(&parts), "2 agents");
        assert_eq!(activity_breakdown(&parts).matches("agent").count(), 1);
    }

    #[test]
    fn activity_breakdown_classifies_each_hub_operation() {
        let parts = ["start", "restart", "stop", "wait", "send", "inspect"]
            .into_iter()
            .enumerate()
            .map(|(index, op)| {
                tool(
                    &format!("hub-{index}"),
                    unknown("hub", Some(serde_json::json!({"op": op}))),
                    true,
                )
            })
            .collect::<Vec<_>>();

        assert_eq!(
            activity_breakdown(&parts),
            "3 commands, 1 wait, 1 message, 1 tool"
        );
    }

    #[test]
    fn activity_breakdown_classifies_native_named_tools() {
        let parts = vec![
            tool("task", unknown("task", None), true),
            tool("skill", unknown("skill", None), true),
            tool("eval", unknown("eval", None), true),
        ];

        assert_eq!(activity_breakdown(&parts), "1 agent, 1 skill, 1 command");
    }

    #[test]
    fn activity_breakdown_classifies_acp_task_description_as_agent() {
        let parts = vec![tool(
            "task",
            unknown("Task: Inspect authentication paths", None),
            true,
        )];

        assert_eq!(activity_breakdown(&parts), "1 agent");
    }

    #[test]
    fn activity_breakdown_returns_empty_without_categorizable_tools() {
        let parts = vec![
            text("narration", "Inspecting"),
            reasoning("reasoning", true),
        ];

        assert_eq!(activity_breakdown(&parts), "");
    }

    #[test]
    fn summary_falls_back_to_visible_step_count_without_tools() {
        assert_eq!(turn_summary(&[reasoning("done", true)]), "1 step");
        assert_eq!(
            turn_summary(&[reasoning("a", true), reasoning("b", true)]),
            "2 steps"
        );
    }

    #[test]
    fn remaining_builtin_and_mcp_variants_follow_the_same_buckets() {
        let parts = vec![
            tool(
                "glob",
                ToolCall::Glob {
                    pattern: "**/*.rs".into(),
                },
                true,
            ),
            tool(
                "fetch",
                ToolCall::WebFetch {
                    url: "https://example.com".into(),
                    prompt: None,
                },
                true,
            ),
            tool("patch", ToolCall::ApplyPatch { path: None }, true),
            tool("terminal", mcp("create_terminal"), true),
            tool("keys", mcp("send_terminal_keys"), true),
        ];

        assert_eq!(
            activity_breakdown(&parts),
            "1 read, 1 search, 1 edit, 1 command, 1 message"
        );
    }
}
