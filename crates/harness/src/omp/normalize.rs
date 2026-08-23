use std::collections::HashMap;

use serde_json::{Value, json};
use zeron_proto::{
    AgentEvent, DoneStatus, HarnessId, SlashCommand, TodoItem, ToolCall, ToolDiff,
    ToolExecutionMeta, WorkflowProgressNode, WorkflowTaskStatus, WorkflowTaskUpdate, WorkflowUsage,
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
    index: u32,
    description: Option<String>,
}

pub struct OmpNormalizer {
    cwd: String,
    model: String,
    subagents: HashMap<String, SubagentContext>,
    todos: Vec<TodoItem>,
    todo_previous: HashMap<String, Vec<TodoItem>>,
}

impl OmpNormalizer {
    pub fn new(cwd: impl Into<String>, model: impl Into<String>) -> Self {
        Self {
            cwd: cwd.into(),
            model: model.into(),
            subagents: HashMap::new(),
            todos: Vec::new(),
            todo_previous: HashMap::new(),
        }
    }

    pub fn push(&mut self, frame: Value) -> Vec<AgentEvent> {
        match frame.get("type").and_then(Value::as_str) {
            Some("message_update") => self.message_update(&frame),
            Some("tool_execution_start") => self.tool_start(&frame).into_iter().collect(),
            Some("tool_execution_end") => self.tool_end(&frame),
            Some("available_commands_update") => available_commands(&frame)
                .map(|commands| AgentEvent::AvailableCommands { commands })
                .into_iter()
                .collect(),
            Some("subagent_lifecycle") => self.subagent_lifecycle(&frame),
            Some("subagent_progress") => self.subagent_progress(&frame),
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

    fn tool_start(&mut self, frame: &Value) -> Option<AgentEvent> {
        let id = frame.get("toolCallId")?.as_str()?;
        let name = frame.get("toolName")?.as_str()?;
        if name != "todo" {
            return tool_start(frame);
        }

        let input = frame.get("args").unwrap_or(&Value::Null);
        self.todo_previous.insert(id.to_owned(), self.todos.clone());
        if let Some(items) = todo_items_from_input(input) {
            self.todos = items;
        }
        Some(AgentEvent::ToolCall {
            id: id.to_owned(),
            call: ToolCall::Todo {
                items: self.todos.clone(),
            },
        })
    }

    fn tool_end(&mut self, frame: &Value) -> Vec<AgentEvent> {
        let Some(result) = tool_end(frame) else {
            return Vec::new();
        };
        if frame.get("toolName").and_then(Value::as_str) != Some("todo") {
            return vec![result];
        }

        let Some(id) = frame.get("toolCallId").and_then(Value::as_str) else {
            return vec![result];
        };
        let previous = self.todo_previous.remove(id);
        if let Some(items) = frame
            .get("result")
            .and_then(|value| value.get("details"))
            .and_then(|value| value.get("phases"))
            .and_then(todo_items_from_phases)
        {
            self.todos = items;
        } else if frame.get("isError").and_then(Value::as_bool) == Some(true)
            && let Some(previous) = previous
        {
            self.todos = previous;
        }
        vec![
            AgentEvent::ToolCall {
                id: id.to_owned(),
                call: ToolCall::Todo {
                    items: self.todos.clone(),
                },
            },
            result,
        ]
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
            let previous = self.subagents.get(id).cloned();
            let Some(parent_tool_use_id) = payload
                .get("parentToolCallId")
                .and_then(Value::as_str)
                .filter(|parent| !parent.is_empty())
                .map(str::to_owned)
                .or_else(|| {
                    previous
                        .as_ref()
                        .map(|context| context.parent_tool_use_id.clone())
                })
            else {
                return Vec::new();
            };
            let session_id = payload
                .get("sessionFile")
                .and_then(Value::as_str)
                .filter(|session| !session.is_empty())
                .map(str::to_owned)
                .or_else(|| previous.as_ref().map(|context| context.session_id.clone()))
                .unwrap_or_else(|| id.to_owned());
            let agent = payload
                .get("agent")
                .and_then(Value::as_str)
                .filter(|agent| !agent.is_empty())
                .map(str::to_owned)
                .or_else(|| previous.as_ref().map(|context| context.agent.clone()))
                .unwrap_or_else(|| "task".to_owned());
            let index = u32_value(payload.get("index"))
                .or_else(|| previous.as_ref().map(|context| context.index))
                .unwrap_or(0);
            let description = first_string(payload, &["description", "assignment", "task"])
                .or_else(|| {
                    previous
                        .as_ref()
                        .and_then(|context| context.description.clone())
                });
            self.subagents.insert(
                id.to_owned(),
                SubagentContext {
                    parent_tool_use_id: parent_tool_use_id.clone(),
                    session_id: session_id.clone(),
                    agent: agent.clone(),
                    index,
                    description: description.clone(),
                },
            );
            // Fan-out routing: one `task` tool call spawns N subagents, so the
            // bare parent id cannot key docs/chips — each subagent gets a
            // compound id and its own synthetic spawn chip (first sight only).
            let compound = compound_parent_id(&parent_tool_use_id, id);
            let mut events = Vec::with_capacity(3);
            if previous.is_none() {
                events.push(spawn_chip(&compound, &agent, description.as_deref()));
            }
            if description.is_some() {
                events.push(subagent_workflow_update(
                    id,
                    WorkflowTaskStatus::Running,
                    description,
                    None,
                    Vec::new(),
                    &agent,
                ));
            }
            events.push(AgentEvent::Subagent {
                parent_tool_use_id: compound,
                event: Box::new(AgentEvent::SessionStarted {
                    harness: HarnessId::Omp,
                    model: self.model.clone(),
                    tools: Vec::new(),
                    cwd: self.cwd.clone(),
                    session_id,
                    assistant_message_id: format!("omp-subagent-{id}"),
                }),
            });
            return events;
        }
        if matches!(
            status,
            "completed" | "failed" | "errored" | "cancelled" | "aborted"
        ) && let Some(context) = self.subagents.remove(id)
        {
            let failed = matches!(status, "failed" | "errored");
            let interrupted = matches!(status, "cancelled" | "aborted");
            let workflow_status = if failed {
                WorkflowTaskStatus::Failed
            } else if interrupted {
                WorkflowTaskStatus::Cancelled
            } else {
                WorkflowTaskStatus::Completed
            };
            let error = failed.then(|| {
                payload
                    .get("error")
                    .and_then(Value::as_str)
                    .map(|error| truncate(&sanitize_diagnostic(error), 1_024))
                    .unwrap_or_else(|| format!("OMP {} subagent failed", context.agent))
            });
            let compound = compound_parent_id(&context.parent_tool_use_id, id);
            let mut events = Vec::with_capacity(3);
            if context.description.is_some() {
                events.push(subagent_workflow_update(
                    id,
                    workflow_status,
                    context.description,
                    None,
                    Vec::new(),
                    &context.agent,
                ));
            }
            // Resolve the synthetic spawn chip so it never spins forever.
            events.push(AgentEvent::ToolResult {
                id: compound.clone(),
                is_error: failed,
                output: Some(error.clone().unwrap_or_else(|| status.to_owned())),
                diff: None,
                execution: None,
            });
            events.push(AgentEvent::Subagent {
                parent_tool_use_id: compound,
                event: Box::new(AgentEvent::Done {
                    status: if failed {
                        DoneStatus::Errored
                    } else if interrupted {
                        DoneStatus::Interrupted
                    } else {
                        DoneStatus::Completed
                    },
                    result: None,
                    error,
                    session_id: Some(context.session_id),
                }),
            });
            return events;
        }
        Vec::new()
    }

    fn subagent_progress(&mut self, frame: &Value) -> Vec<AgentEvent> {
        let Some(payload) = frame.get("payload").and_then(Value::as_object) else {
            return Vec::new();
        };
        let payload = Value::Object(payload.clone());
        let Some(progress) = payload.get("progress").and_then(Value::as_object) else {
            return Vec::new();
        };
        let progress = Value::Object(progress.clone());
        let Some(id) = progress
            .get("id")
            .and_then(Value::as_str)
            .filter(|id| !id.is_empty())
        else {
            return Vec::new();
        };
        let Some(status) = progress
            .get("status")
            .and_then(Value::as_str)
            .and_then(workflow_task_status)
        else {
            return Vec::new();
        };
        let previous = self.subagents.get(id).cloned();
        let Some(parent_tool_use_id) = payload
            .get("parentToolCallId")
            .and_then(Value::as_str)
            .filter(|parent| !parent.is_empty())
            .map(str::to_owned)
            .or_else(|| {
                previous
                    .as_ref()
                    .map(|context| context.parent_tool_use_id.clone())
            })
        else {
            return Vec::new();
        };
        let agent = first_string(&payload, &["agent"])
            .or_else(|| first_string(&progress, &["agent"]))
            .or_else(|| previous.as_ref().map(|context| context.agent.clone()))
            .unwrap_or_else(|| "task".to_owned());
        let index = u32_value(progress.get("index"))
            .or_else(|| u32_value(payload.get("index")))
            .or_else(|| previous.as_ref().map(|context| context.index))
            .unwrap_or(0);
        let description = first_string(&payload, &["assignment", "task", "description"])
            .or_else(|| first_string(&progress, &["assignment", "task", "description"]))
            .or_else(|| {
                previous
                    .as_ref()
                    .and_then(|context| context.description.clone())
            })
            .unwrap_or_else(|| agent.clone());
        let session_id = previous
            .as_ref()
            .map(|context| context.session_id.clone())
            .unwrap_or_else(|| id.to_owned());
        self.subagents.insert(
            id.to_owned(),
            SubagentContext {
                parent_tool_use_id: parent_tool_use_id.clone(),
                session_id,
                agent: agent.clone(),
                index,
                description: Some(description.clone()),
            },
        );
        // A progress frame can be the subagent's first sight: open its chip
        // here too, or a fast run that settles before any lifecycle frame
        // would leave the parent transcript chipless.
        let mut events = Vec::with_capacity(2);
        if previous.is_none() {
            events.push(spawn_chip(
                &compound_parent_id(&parent_tool_use_id, id),
                &agent,
                Some(&description),
            ));
        }
        let usage = WorkflowUsage {
            total_tokens: progress.get("tokens").and_then(Value::as_u64),
            tool_uses: progress.get("toolCount").and_then(Value::as_u64),
            duration_ms: progress.get("durationMs").and_then(Value::as_u64),
        };
        let usage = (usage.total_tokens.is_some()
            || usage.tool_uses.is_some()
            || usage.duration_ms.is_some())
        .then_some(usage);
        let node = WorkflowProgressNode::Agent {
            index,
            label: description.clone(),
            phase_index: 0,
            phase_title: Some("OMP subagents".to_owned()),
            agent_id: Some(id.to_owned()),
            model: first_string(&progress, &["resolvedModel"]),
            state: Some(workflow_status_name(status).to_owned()),
            prompt_preview: Some(description.clone()),
        };
        events.push(subagent_workflow_update(
            id,
            status,
            Some(description),
            usage,
            vec![node],
            &agent,
        ));
        events
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
                parent_tool_use_id: compound_parent_id(&context.parent_tool_use_id, id),
                event: Box::new(event),
            })
            .into_iter()
            .collect()
    }
}

/// Per-subagent routing id: OMP's `task` tool fans out N subagents under ONE
/// tool call, so the bare parent id cannot key a doc/chip per subagent (every
/// subagent would collide on `chat--sub--{toolUseId}` — one doc, one chip,
/// one UI row). The compound id gives each subagent its own spawn chip and
/// transcript doc, the 1-chip-per-subagent invariant other harnesses keep.
fn compound_parent_id(parent: &str, id: &str) -> String {
    format!("{parent}--{id}")
}

/// The synthetic spawn chip folded into the parent transcript for a fanned-out
/// subagent: named after the task (claude-driver parity), so the chip — and
/// the tab it opens — says what the subagent does.
fn spawn_chip(compound_id: &str, agent: &str, description: Option<&str>) -> AgentEvent {
    let label = description
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .unwrap_or(agent);
    let name = truncate(&label.split_whitespace().collect::<Vec<_>>().join(" "), 96);
    AgentEvent::ToolCall {
        id: compound_id.to_owned(),
        call: ToolCall::Unknown {
            name: format!("Agent: {name}"),
            input: description.map(|text| json!({ "description": text })),
        },
    }
}

fn workflow_task_status(status: &str) -> Option<WorkflowTaskStatus> {
    match status {
        "started" | "pending" | "running" => Some(WorkflowTaskStatus::Running),
        "completed" => Some(WorkflowTaskStatus::Completed),
        "failed" | "errored" => Some(WorkflowTaskStatus::Failed),
        "aborted" | "cancelled" => Some(WorkflowTaskStatus::Cancelled),
        _ => None,
    }
}

fn workflow_status_name(status: WorkflowTaskStatus) -> &'static str {
    match status {
        WorkflowTaskStatus::Running => "running",
        WorkflowTaskStatus::Completed => "completed",
        WorkflowTaskStatus::Failed => "failed",
        WorkflowTaskStatus::Cancelled => "cancelled",
    }
}

fn first_string(value: &Value, keys: &[&str]) -> Option<String> {
    keys.iter()
        .filter_map(|key| value.get(key).and_then(Value::as_str))
        .find(|value| !value.trim().is_empty())
        .map(str::to_owned)
}

fn u32_value(value: Option<&Value>) -> Option<u32> {
    value
        .and_then(Value::as_u64)
        .and_then(|value| u32::try_from(value).ok())
}

fn subagent_workflow_update(
    id: &str,
    status: WorkflowTaskStatus,
    description: Option<String>,
    usage: Option<WorkflowUsage>,
    progress: Vec<WorkflowProgressNode>,
    agent: &str,
) -> AgentEvent {
    AgentEvent::WorkflowTask {
        task: WorkflowTaskUpdate {
            task_id: id.to_owned(),
            status,
            workflow_name: None,
            description,
            usage,
            progress,
            agent_count: Some(1),
            task_type: Some("subagent".to_owned()),
            subagent_type: Some(agent.to_owned()),
        },
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

fn todo_items_from_input(input: &Value) -> Option<Vec<TodoItem>> {
    if input.get("op").and_then(Value::as_str) != Some("init") {
        return None;
    }
    let items = if let Some(phases) = input.get("list").and_then(Value::as_array) {
        phases
            .iter()
            .filter_map(|phase| phase.get("items").and_then(Value::as_array))
            .flatten()
            .filter_map(Value::as_str)
            .collect::<Vec<_>>()
    } else {
        input
            .get("items")
            .and_then(Value::as_array)?
            .iter()
            .filter_map(Value::as_str)
            .collect::<Vec<_>>()
    };
    Some(
        items
            .into_iter()
            .map(|text| TodoItem {
                text: text.to_owned(),
                done: false,
            })
            .collect(),
    )
}

fn todo_items_from_phases(phases: &Value) -> Option<Vec<TodoItem>> {
    Some(
        phases
            .as_array()?
            .iter()
            .filter_map(|phase| phase.get("tasks").and_then(Value::as_array))
            .flatten()
            .filter_map(|task| {
                let text = task.get("content").and_then(Value::as_str)?;
                let done = matches!(
                    task.get("status").and_then(Value::as_str),
                    Some("completed" | "abandoned")
                );
                Some(TodoItem {
                    text: text.to_owned(),
                    done,
                })
            })
            .collect(),
    )
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use zeron_proto::{WorkflowProgressNode, WorkflowTaskStatus, WorkflowUsage};

    #[test]
    fn omp_subagent_progress_emits_workflow_activity() {
        let mut normalizer = OmpNormalizer::new("/repo", "openai-codex/gpt-5.6-sol");
        let started = normalizer.push(json!({
            "type": "subagent_lifecycle",
            "payload": {
                "id": "sub-1",
                "index": 0,
                "agent": "scout",
                "description": "Inspect authentication paths.",
                "status": "started",
                "parentToolCallId": "call_task|fc_parent",
                "sessionFile": "/tmp/sub-1.jsonl"
            }
        }));
        // First sight opens the synthetic spawn chip under the compound id.
        assert!(matches!(
            &started[0],
            AgentEvent::ToolCall { id, call: ToolCall::Unknown { name, .. } }
                if id == "call_task|fc_parent--sub-1" && name.starts_with("Agent: ")
        ));
        assert!(started.iter().any(|event| matches!(
            event,
            AgentEvent::WorkflowTask { task }
                if task.task_id == "sub-1"
                    && task.status == WorkflowTaskStatus::Running
                    && task.task_type.as_deref() == Some("subagent")
                    && task.subagent_type.as_deref() == Some("scout")
        )));
        assert!(started.iter().any(|event| matches!(
            event,
            AgentEvent::Subagent { parent_tool_use_id, event }
                if parent_tool_use_id == "call_task|fc_parent--sub-1"
                    && matches!(event.as_ref(), AgentEvent::SessionStarted { .. })
        )));

        let progress = normalizer.push(json!({
            "type": "subagent_progress",
            "payload": {
                "parentToolCallId": "call_task|fc_parent",
                "agent": "scout",
                "progress": {
                    "id": "sub-1",
                    "index": 0,
                    "status": "running",
                    "task": "Inspect authentication paths.",
                    "toolCount": 2,
                    "tokens": 120,
                    "durationMs": 1000,
                    "resolvedModel": "openai-codex/gpt-5.6-sol:high"
                }
            }
        }));
        assert!(matches!(
            &progress[..],
            [AgentEvent::WorkflowTask { task }]
                if task.task_id == "sub-1"
                    && task.status == WorkflowTaskStatus::Running
                    && task.description.as_deref() == Some("Inspect authentication paths.")
                    && task.usage == Some(WorkflowUsage {
                        total_tokens: Some(120),
                        tool_uses: Some(2),
                        duration_ms: Some(1_000),
                    })
                    && matches!(task.progress.as_slice(), [WorkflowProgressNode::Agent { agent_id, model, .. }]
                        if agent_id.as_deref() == Some("sub-1")
                            && model.as_deref() == Some("openai-codex/gpt-5.6-sol:high"))
                    && task.task_type.as_deref() == Some("subagent")
                    && task.subagent_type.as_deref() == Some("scout")
        ));

        let nested = normalizer.push(json!({
            "type": "subagent_event",
            "payload": {
                "id": "sub-1",
                "event": {
                    "type": "message_update",
                    "assistantMessageEvent": { "type": "text_delta", "delta": "found auth" }
                }
            }
        }));
        assert!(matches!(
            &nested[..],
            [AgentEvent::Subagent { parent_tool_use_id, event }]
                if parent_tool_use_id == "call_task|fc_parent--sub-1"
                    && matches!(event.as_ref(), AgentEvent::TextDelta { text } if text == "found auth")
        ));

        let completed = normalizer.push(json!({
            "type": "subagent_lifecycle",
            "payload": { "id": "sub-1", "status": "completed" }
        }));
        assert!(completed.iter().any(|event| matches!(
            event,
            AgentEvent::WorkflowTask { task }
                if task.task_id == "sub-1" && task.status == WorkflowTaskStatus::Completed
        )));
        assert!(completed.iter().any(|event| matches!(
            event,
            AgentEvent::ToolResult { id, is_error: false, .. }
                if id == "call_task|fc_parent--sub-1"
        )));
        assert!(completed.iter().any(|event| matches!(
            event,
            AgentEvent::Subagent { parent_tool_use_id, event }
                if parent_tool_use_id == "call_task|fc_parent--sub-1"
                    && matches!(event.as_ref(), AgentEvent::Done { status: DoneStatus::Completed, .. })
        )));
    }

    #[test]
    fn omp_batch_subagents_get_distinct_chips_and_routing() {
        // OMP's task tool fans out N subagents under ONE tool call: each must
        // get its own synthetic chip and compound routing id, or every
        // subagent collides on `chat--sub--{toolUseId}` (one doc, one UI row).
        let mut normalizer = OmpNormalizer::new("/repo", "openai-codex/gpt-5.6-sol");
        let spawn = |id: &str| {
            json!({
                "type": "subagent_lifecycle",
                "payload": {
                    "id": id,
                    "agent": "task",
                    "description": format!("Research {id}"),
                    "status": "started",
                    "parentToolCallId": "tool_batch",
                    "sessionFile": format!("/tmp/{id}.jsonl")
                }
            })
        };
        let parent_of = |events: &[AgentEvent]| {
            events
                .iter()
                .find_map(|event| match event {
                    AgentEvent::Subagent {
                        parent_tool_use_id, ..
                    } => Some(parent_tool_use_id.clone()),
                    _ => None,
                })
                .expect("tagged event")
        };
        let alpha = normalizer.push(spawn("Alpha"));
        let beta = normalizer.push(spawn("Beta"));
        assert!(matches!(&alpha[0], AgentEvent::ToolCall { id, .. } if id == "tool_batch--Alpha"));
        assert!(matches!(&beta[0], AgentEvent::ToolCall { id, .. } if id == "tool_batch--Beta"));
        assert_eq!(parent_of(&alpha), "tool_batch--Alpha");
        assert_eq!(parent_of(&beta), "tool_batch--Beta");
        assert_eq!(normalizer.active_subagents(), 2);

        let done = normalizer.push(json!({
            "type": "subagent_lifecycle",
            "payload": { "id": "Alpha", "status": "completed" }
        }));
        assert_eq!(parent_of(&done), "tool_batch--Alpha");
        assert!(done.iter().any(|event| matches!(
            event,
            AgentEvent::ToolResult { id, .. } if id == "tool_batch--Alpha"
        )));
        // Beta still runs: the turn must not complete yet.
        assert_eq!(
            normalizer.classify_agent_end(&json!({ "type": "agent_end", "messages": [] })),
            AgentEndDisposition::Continue
        );
        assert_eq!(normalizer.active_subagents(), 1);
    }

    #[test]
    fn omp_malformed_progress_is_ignored_without_losing_subagent_context() {
        let mut normalizer = OmpNormalizer::new("/repo", "openai-codex/gpt-5.6-sol");
        normalizer.push(json!({
            "type": "subagent_lifecycle",
            "payload": {
                "id": "sub-1",
                "agent": "scout",
                "status": "running",
                "parentToolCallId": "task-1"
            }
        }));
        assert!(
            normalizer
                .push(json!({ "type": "subagent_progress", "payload": { "progress": "bad" } }))
                .is_empty()
        );
        let nested = normalizer.push(json!({
            "type": "subagent_event",
            "payload": {
                "id": "sub-1",
                "event": {
                    "type": "message_update",
                    "assistantMessageEvent": { "type": "text_delta", "delta": "still alive" }
                }
            }
        }));
        assert!(matches!(&nested[..], [AgentEvent::Subagent { .. }]));
    }

    #[test]
    fn omp_todo_init_normalizes_phased_items() {
        let mut normalizer = OmpNormalizer::new("/repo", "kimi-code/k3");
        let events = normalizer.push(json!({
            "type": "tool_execution_start",
            "toolCallId": "todo-1",
            "toolName": "todo",
            "args": {
                "op": "init",
                "list": [{
                    "phase": "Work",
                    "items": ["Inspect state", "Run gates"]
                }]
            }
        }));

        assert!(matches!(
            &events[..],
            [AgentEvent::ToolCall {
                id,
                call: ToolCall::Todo { items },
            }] if id == "todo-1"
                && items.len() == 2
                && items[0].text == "Inspect state"
                && !items[0].done
                && items[1].text == "Run gates"
                && !items[1].done
        ));
    }

    #[test]
    fn omp_todo_init_normalizes_flat_items() {
        let mut normalizer = OmpNormalizer::new("/repo", "kimi-code/k3");
        let events = normalizer.push(json!({
            "type": "tool_execution_start",
            "toolCallId": "todo-2",
            "toolName": "todo",
            "args": {
                "op": "init",
                "items": ["Inspect state", "Run gates"]
            }
        }));

        assert!(matches!(
            &events[..],
            [AgentEvent::ToolCall {
                call: ToolCall::Todo { items },
                ..
            }] if items.iter().map(|item| item.text.as_str()).collect::<Vec<_>>()
                == ["Inspect state", "Run gates"]
        ));
    }

    #[test]
    fn omp_todo_invalid_init_still_uses_shared_todo_call() {
        let mut normalizer = OmpNormalizer::new("/repo", "kimi-code/k3");
        let events = normalizer.push(json!({
            "type": "tool_execution_start",
            "toolCallId": "todo-3",
            "toolName": "todo",
            "args": { "op": "init", "task": "" }
        }));

        assert!(matches!(
            &events[..],
            [AgentEvent::ToolCall {
                call: ToolCall::Todo { items },
                ..
            }] if items.is_empty()
        ));
    }

    #[test]
    fn omp_todo_append_start_preserves_previous_snapshot() {
        let mut normalizer = OmpNormalizer::new("/repo", "kimi-code/k3");
        normalizer.push(json!({
            "type": "tool_execution_start",
            "toolCallId": "todo-init",
            "toolName": "todo",
            "args": { "op": "init", "items": ["Inspect state", "Run gates"] }
        }));

        let events = normalizer.push(json!({
            "type": "tool_execution_start",
            "toolCallId": "todo-append",
            "toolName": "todo",
            "args": {
                "op": "append",
                "phase": "Work",
                "items": ["Write report"]
            }
        }));

        assert!(matches!(
            &events[..],
            [AgentEvent::ToolCall {
                call: ToolCall::Todo { items },
                ..
            }] if items.iter().map(|item| item.text.as_str()).collect::<Vec<_>>()
                == ["Inspect state", "Run gates"]
        ));
    }

    #[test]
    fn omp_todo_result_reconciles_authoritative_snapshot() {
        let mut normalizer = OmpNormalizer::new("/repo", "kimi-code/k3");
        normalizer.push(json!({
            "type": "tool_execution_start",
            "toolCallId": "todo-4",
            "toolName": "todo",
            "args": { "op": "init", "items": ["Inspect state", "Run gates"] }
        }));

        let events = normalizer.push(json!({
            "type": "tool_execution_end",
            "toolCallId": "todo-4",
            "toolName": "todo",
            "isError": false,
            "result": {
                "content": [{ "type": "text", "text": "1/2 done" }],
                "details": {
                    "phases": [{
                        "name": "Work",
                        "tasks": [
                            { "content": "Inspect state", "status": "completed" },
                            { "content": "Run gates", "status": "in_progress" }
                        ]
                    }]
                }
            }
        }));

        assert!(matches!(
            &events[..],
            [
                AgentEvent::ToolCall {
                    id,
                    call: ToolCall::Todo { items },
                },
                AgentEvent::ToolResult {
                    id: result_id,
                    is_error: false,
                    ..
                }
            ] if id == "todo-4"
                && result_id == "todo-4"
                && items.len() == 2
                && items[0].text == "Inspect state"
                && items[0].done
                && items[1].text == "Run gates"
                && !items[1].done
        ));
    }

    #[test]
    fn omp_todo_result_preserves_snapshot_and_error_output() {
        let mut normalizer = OmpNormalizer::new("/repo", "kimi-code/k3");
        normalizer.push(json!({
            "type": "tool_execution_start",
            "toolCallId": "todo-5",
            "toolName": "todo",
            "args": { "op": "init", "items": ["Inspect state", "Run gates"] }
        }));
        normalizer.push(json!({
            "type": "tool_execution_end",
            "toolCallId": "todo-5",
            "toolName": "todo",
            "isError": false,
            "result": {
                "details": {
                    "phases": [{
                        "name": "Work",
                        "tasks": [
                            { "content": "Inspect state", "status": "in_progress" },
                            { "content": "Run gates", "status": "pending" }
                        ]
                    }]
                }
            }
        }));
        normalizer.push(json!({
            "type": "tool_execution_start",
            "toolCallId": "todo-failed",
            "toolName": "todo",
            "args": { "op": "init", "task": "" }
        }));

        let events = normalizer.push(json!({
            "type": "tool_execution_end",
            "toolCallId": "todo-failed",
            "toolName": "todo",
            "isError": true,
            "result": {
                "content": [{
                    "type": "text",
                    "text": "Errors: Missing list for init operation"
                }]
            }
        }));

        assert!(matches!(
            &events[..],
            [
                AgentEvent::ToolCall {
                    call: ToolCall::Todo { items },
                    ..
                },
                AgentEvent::ToolResult {
                    is_error: true,
                    output: Some(output),
                    ..
                }
            ] if items.iter().map(|item| item.text.as_str()).collect::<Vec<_>>()
                    == ["Inspect state", "Run gates"]
                && output.contains("Missing list for init operation")
        ));
    }

    #[test]
    fn omp_todo_rejected_init_restores_previous_snapshot() {
        let mut normalizer = OmpNormalizer::new("/repo", "kimi-code/k3");
        normalizer.push(json!({
            "type": "tool_execution_start",
            "toolCallId": "todo-initial",
            "toolName": "todo",
            "args": { "op": "init", "items": ["Inspect state", "Run gates"] }
        }));
        normalizer.push(json!({
            "type": "tool_execution_end",
            "toolCallId": "todo-initial",
            "toolName": "todo",
            "isError": false,
            "result": {
                "details": {
                    "phases": [{
                        "name": "Work",
                        "tasks": [
                            { "content": "Inspect state", "status": "in_progress" },
                            { "content": "Run gates", "status": "pending" }
                        ]
                    }]
                }
            }
        }));

        normalizer.push(json!({
            "type": "tool_execution_start",
            "toolCallId": "todo-rejected",
            "toolName": "todo",
            "args": {
                "op": "init",
                "items": ["Duplicate task", "Duplicate task"]
            }
        }));
        let events = normalizer.push(json!({
            "type": "tool_execution_end",
            "toolCallId": "todo-rejected",
            "toolName": "todo",
            "isError": true,
            "result": {
                "content": [{ "type": "text", "text": "Duplicate task" }]
            }
        }));

        assert!(matches!(
            &events[..],
            [
                AgentEvent::ToolCall {
                    call: ToolCall::Todo { items },
                    ..
                },
                AgentEvent::ToolResult { is_error: true, .. }
            ] if items.iter().map(|item| item.text.as_str()).collect::<Vec<_>>()
                == ["Inspect state", "Run gates"]
        ));
    }
}
