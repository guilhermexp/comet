use std::collections::HashMap;

use serde_json::Value;
use zeron_proto::{
    AgentEvent, DoneStatus, HarnessId, SlashCommand, ToolCall, ToolDiff, ToolExecutionMeta,
};

use super::protocol::sanitize_diagnostic;

const MAX_TOOL_OUTPUT_BYTES: usize = 64 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentEndDisposition {
    Continue,
    Complete,
    Error(String),
}

#[derive(Debug, Clone)]
struct SubagentContext {
    parent_tool_use_id: String,
    session_id: String,
    agent: String,
}

pub struct OmpNormalizer {
    cwd: String,
    model: String,
    subagents: HashMap<String, SubagentContext>,
}

impl OmpNormalizer {
    pub fn new(cwd: impl Into<String>, model: impl Into<String>) -> Self {
        Self {
            cwd: cwd.into(),
            model: model.into(),
            subagents: HashMap::new(),
        }
    }

    pub fn push(&mut self, frame: Value) -> Vec<AgentEvent> {
        match frame.get("type").and_then(Value::as_str) {
            Some("message_update") => self.message_update(&frame),
            Some("tool_execution_start") => tool_start(&frame).into_iter().collect(),
            Some("tool_execution_end") => tool_end(&frame).into_iter().collect(),
            Some("available_commands_update") => available_commands(&frame)
                .map(|commands| AgentEvent::AvailableCommands { commands })
                .into_iter()
                .collect(),
            Some("subagent_lifecycle") => self.subagent_lifecycle(&frame),
            Some("subagent_event") => self.subagent_event(&frame),
            Some("notice") if frame.get("level").and_then(Value::as_str) == Some("error") => frame
                .get("message")
                .and_then(Value::as_str)
                .filter(|message| !message.is_empty())
                .map(|message| AgentEvent::Error {
                    message: truncate(&sanitize_diagnostic(message), 1_024),
                })
                .into_iter()
                .collect(),
            _ => Vec::new(),
        }
    }

    pub fn active_subagents(&self) -> usize {
        self.subagents.len()
    }

    pub fn classify_agent_end(&mut self, frame: &Value) -> AgentEndDisposition {
        if frame.get("isTerminal").and_then(Value::as_bool) == Some(false)
            || !self.subagents.is_empty()
        {
            return AgentEndDisposition::Continue;
        }
        let failure = frame
            .get("messages")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .rev()
            .find(|message| message.get("role").and_then(Value::as_str) == Some("assistant"))
            .filter(|message| message.get("stopReason").and_then(Value::as_str) == Some("error"))
            .and_then(|message| message.get("errorMessage").and_then(Value::as_str))
            .filter(|message| !message.is_empty())
            .map(|message| truncate(&sanitize_diagnostic(message), 1_024));
        failure
            .map(AgentEndDisposition::Error)
            .unwrap_or(AgentEndDisposition::Complete)
    }

    fn message_update(&self, frame: &Value) -> Vec<AgentEvent> {
        let Some(event) = frame.get("assistantMessageEvent") else {
            return Vec::new();
        };
        let delta = event
            .get("delta")
            .and_then(Value::as_str)
            .unwrap_or_default();
        match event.get("type").and_then(Value::as_str) {
            Some("text_delta") if !delta.is_empty() => vec![AgentEvent::TextDelta {
                text: delta.to_owned(),
            }],
            Some("thinking_delta") if !delta.is_empty() => vec![AgentEvent::ReasoningDelta {
                text: delta.to_owned(),
            }],
            Some("toolcall_end") => event_tool_call(event).into_iter().collect(),
            _ => Vec::new(),
        }
    }

    fn subagent_lifecycle(&mut self, frame: &Value) -> Vec<AgentEvent> {
        let Some(payload) = frame.get("payload") else {
            return Vec::new();
        };
        let Some(id) = payload
            .get("id")
            .and_then(Value::as_str)
            .filter(|id| !id.is_empty())
        else {
            return Vec::new();
        };
        let status = payload
            .get("status")
            .and_then(Value::as_str)
            .unwrap_or_default();
        if matches!(status, "running" | "started" | "pending") {
            let Some(parent_tool_use_id) = payload
                .get("parentToolCallId")
                .and_then(Value::as_str)
                .filter(|parent| !parent.is_empty())
            else {
                return Vec::new();
            };
            let session_id = payload
                .get("sessionFile")
                .and_then(Value::as_str)
                .filter(|session| !session.is_empty())
                .unwrap_or(id)
                .to_owned();
            let agent = payload
                .get("agent")
                .and_then(Value::as_str)
                .filter(|agent| !agent.is_empty())
                .unwrap_or("task")
                .to_owned();
            self.subagents.insert(
                id.to_owned(),
                SubagentContext {
                    parent_tool_use_id: parent_tool_use_id.to_owned(),
                    session_id: session_id.clone(),
                    agent,
                },
            );
            return vec![AgentEvent::Subagent {
                parent_tool_use_id: parent_tool_use_id.to_owned(),
                event: Box::new(AgentEvent::SessionStarted {
                    harness: HarnessId::Omp,
                    model: self.model.clone(),
                    tools: Vec::new(),
                    cwd: self.cwd.clone(),
                    session_id,
                    assistant_message_id: format!("omp-subagent-{id}"),
                }),
            }];
        }
        if matches!(
            status,
            "completed" | "failed" | "errored" | "cancelled" | "aborted"
        ) && let Some(context) = self.subagents.remove(id)
        {
            let failed = matches!(status, "failed" | "errored");
            let interrupted = matches!(status, "cancelled" | "aborted");
            return vec![AgentEvent::Subagent {
                parent_tool_use_id: context.parent_tool_use_id,
                event: Box::new(AgentEvent::Done {
                    status: if failed {
                        DoneStatus::Errored
                    } else if interrupted {
                        DoneStatus::Interrupted
                    } else {
                        DoneStatus::Completed
                    },
                    result: None,
                    error: failed.then(|| {
                        payload
                            .get("error")
                            .and_then(Value::as_str)
                            .map(|error| truncate(&sanitize_diagnostic(error), 1_024))
                            .unwrap_or_else(|| format!("OMP {} subagent failed", context.agent))
                    }),
                    session_id: Some(context.session_id),
                }),
            }];
        }
        Vec::new()
    }

    fn subagent_event(&mut self, frame: &Value) -> Vec<AgentEvent> {
        let Some(payload) = frame.get("payload") else {
            return Vec::new();
        };
        let Some(id) = payload.get("id").and_then(Value::as_str) else {
            return Vec::new();
        };
        let Some(context) = self.subagents.get(id).cloned() else {
            return Vec::new();
        };
        let Some(event) = payload.get("event") else {
            return Vec::new();
        };
        nested_event(event)
            .map(|event| AgentEvent::Subagent {
                parent_tool_use_id: context.parent_tool_use_id,
                event: Box::new(event),
            })
            .into_iter()
            .collect()
    }
}

fn nested_event(frame: &Value) -> Option<AgentEvent> {
    match frame.get("type").and_then(Value::as_str) {
        Some("message_update") => {
            let event = frame.get("assistantMessageEvent")?;
            let delta = event.get("delta")?.as_str()?.to_owned();
            match event.get("type")?.as_str()? {
                "text_delta" => Some(AgentEvent::TextDelta { text: delta }),
                "thinking_delta" => Some(AgentEvent::ReasoningDelta { text: delta }),
                _ => None,
            }
        }
        Some("tool_execution_start") => tool_start(frame),
        Some("tool_execution_end") => tool_end(frame),
        _ => None,
    }
}

fn event_tool_call(event: &Value) -> Option<AgentEvent> {
    let call = event.get("toolCall").unwrap_or(event);
    let id = call
        .get("id")
        .or_else(|| call.get("toolCallId"))?
        .as_str()?;
    let name = call
        .get("name")
        .or_else(|| call.get("toolName"))?
        .as_str()?;
    let input = call
        .get("arguments")
        .or_else(|| call.get("args"))
        .cloned()
        .unwrap_or(Value::Null);
    Some(AgentEvent::ToolCall {
        id: id.to_owned(),
        call: normalize_tool(name, &input),
    })
}

fn tool_start(frame: &Value) -> Option<AgentEvent> {
    let id = frame.get("toolCallId")?.as_str()?;
    let name = frame.get("toolName")?.as_str()?;
    let input = frame.get("args").cloned().unwrap_or(Value::Null);
    Some(AgentEvent::ToolCall {
        id: id.to_owned(),
        call: normalize_tool(name, &input),
    })
}

fn tool_end(frame: &Value) -> Option<AgentEvent> {
    let id = frame.get("toolCallId")?.as_str()?;
    let result = frame.get("result");
    Some(AgentEvent::ToolResult {
        id: id.to_owned(),
        is_error: frame.get("isError").and_then(Value::as_bool) == Some(true),
        output: result.and_then(tool_output),
        diff: result.and_then(tool_diff),
        execution: result.and_then(execution_meta),
    })
}

fn execution_meta(result: &Value) -> Option<ToolExecutionMeta> {
    let details = result.get("details").unwrap_or(result);
    let exit_code = details
        .get("exitCode")
        .or_else(|| details.get("exit_code"))
        .and_then(Value::as_i64)
        .and_then(|code| i32::try_from(code).ok());
    let duration_ms = details
        .get("durationMs")
        .or_else(|| details.get("duration_ms"))
        .and_then(Value::as_u64);
    (exit_code.is_some() || duration_ms.is_some()).then_some(ToolExecutionMeta {
        exit_code,
        duration_ms,
    })
}

fn tool_diff(result: &Value) -> Option<ToolDiff> {
    let path = result.get("path")?.as_str()?.trim();
    if path.is_empty() {
        return None;
    }
    Some(ToolDiff {
        path: truncate(path, 4_096),
        old_text: result
            .get("oldText")
            .and_then(Value::as_str)
            .map(|text| truncate(text, MAX_TOOL_OUTPUT_BYTES)),
        new_text: result
            .get("newText")
            .and_then(Value::as_str)
            .map(|text| truncate(text, MAX_TOOL_OUTPUT_BYTES))?,
    })
}

fn normalize_tool(name: &str, input: &Value) -> ToolCall {
    match name {
        "bash" => ToolCall::Exec {
            command: string_value(input, "command"),
        },
        "read" => ToolCall::ReadFile {
            path: string_value(input, "path"),
        },
        "write" => ToolCall::WriteFile {
            path: string_value(input, "path"),
            content: optional_string(input, "content"),
        },
        "edit" => ToolCall::EditFile {
            path: string_value(input, "path"),
            old_string: optional_string(input, "oldText"),
            new_string: optional_string(input, "newText"),
        },
        "workers" => ToolCall::Mcp {
            server: "comet-workers".into(),
            tool: "workers".into(),
            input: Some(input.clone()),
        },
        other => ToolCall::Unknown {
            name: other.to_owned(),
            input: Some(input.clone()),
        },
    }
}

fn available_commands(frame: &Value) -> Option<Vec<SlashCommand>> {
    let rows = frame.get("commands")?.as_array()?;
    Some(
        rows.iter()
            .filter_map(|row| {
                let name = row.get("name")?.as_str()?.trim();
                if name.is_empty() {
                    return None;
                }
                Some(SlashCommand {
                    name: name.to_owned(),
                    description: row
                        .get("description")
                        .and_then(Value::as_str)
                        .map(|value| truncate(value, 1_024))
                        .unwrap_or_default(),
                    input_hint: row
                        .pointer("/input/hint")
                        .and_then(Value::as_str)
                        .map(|value| truncate(value, 240)),
                })
            })
            .take(1_000)
            .collect(),
    )
}

fn tool_output(result: &Value) -> Option<String> {
    if let Some(text) = result.as_str() {
        return Some(truncate(text, MAX_TOOL_OUTPUT_BYTES));
    }
    if let Some(content) = result.get("content").and_then(Value::as_array) {
        let joined = content
            .iter()
            .filter_map(|item| {
                (item.get("type").and_then(Value::as_str) == Some("text"))
                    .then(|| item.get("text").and_then(Value::as_str))
                    .flatten()
            })
            .collect::<Vec<_>>()
            .join("\n");
        if !joined.is_empty() {
            return Some(truncate(&joined, MAX_TOOL_OUTPUT_BYTES));
        }
    }
    serde_json::to_string(result)
        .ok()
        .filter(|text| text != "null")
        .map(|text| truncate(&text, MAX_TOOL_OUTPUT_BYTES))
}

fn string_value(value: &Value, key: &str) -> String {
    value
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned()
}

fn optional_string(value: &Value, key: &str) -> Option<String> {
    value.get(key).and_then(Value::as_str).map(str::to_owned)
}

fn truncate(value: &str, max_bytes: usize) -> String {
    if value.len() <= max_bytes {
        return value.to_owned();
    }
    let mut end = max_bytes;
    while end > 0 && !value.is_char_boundary(end) {
        end -= 1;
    }
    value[..end].to_owned()
}
