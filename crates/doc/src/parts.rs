//! Message parts: the event fold, the render-only privacy policy, and continuation splitting.
//!
//! Ports of `packages/control/src/parts.ts` (fold) and
//! `packages/session-doc/src/{render-parts,messages}.ts`.

use serde::{Deserialize, Serialize};

use zeron_proto::{
    AgentEvent, ToolCall, ToolDiff, ToolExecutionMeta, UserInputQuestion, WorkflowProgressNode,
    WorkflowTaskUpdate, WorkflowUsage,
};

use crate::constants::MSG_INLINE_MAX;

/// Char cap for the tool-output SUMMARY persisted into the doc: first
/// non-empty line, nothing more. The per-part 4KB cap (c951c3e) bounded each
/// part but not the session — chat 1b65e93d measured 917KB (85%) of a 1MB doc
/// in capped outputs across 426 tool parts (docs/chat2-sync.md). Full outputs
/// live in the R2 sidecar behind `output_ref`; t3code ships an 84-char
/// summary, so 160 is generous.
pub const TOOL_OUTPUT_SUMMARY_MAX: usize = 160;

/// The doc-resident form of a tool output (docs/chat2-sync.md A1; the R2
/// sidecar is PARKED as of 2026-08-10, so this IS the whole record in the
/// doc — the full text survives only in the host's local run journal):
///
/// - Markdown code fences are stripped first — ACP harnesses fence every
///   output, so the fence is transport wrapping, never content (pre-fix,
///   every summary read "```console…").
/// - Outputs that fit [`TOOL_OUTPUT_SUMMARY_MAX`] chars ride whole — a
///   summary of a two-line output is more UI than the output.
/// - Bigger outputs keep the first non-empty line, capped, with a `…`
///   marker meaning "there was more".
///
/// `None` for blank output.
pub fn summarize_tool_output(text: &str) -> Option<String> {
    let kept: Vec<&str> = text
        .lines()
        .filter(|l| !l.trim_start().starts_with("```"))
        .collect();
    let stripped = kept.join("\n");
    let stripped = stripped.trim();
    if stripped.is_empty() {
        return None;
    }
    if stripped.chars().count() <= TOOL_OUTPUT_SUMMARY_MAX {
        return Some(stripped.to_owned());
    }
    // Too big to inline: first non-empty line, capped. There is always more
    // than the summary here (whole output exceeded the budget), so the
    // marker is unconditional.
    let line = stripped
        .lines()
        .find(|l| !l.trim().is_empty())
        .unwrap_or(stripped)
        .trim_end();
    let mut chars = 0usize;
    let mut end = line.len();
    for (i, _) in line.char_indices() {
        if chars == TOOL_OUTPUT_SUMMARY_MAX {
            end = i;
            break;
        }
        chars += 1;
    }
    let mut out = line[..end].to_owned();
    out.push('…');
    Some(out)
}

/// Per-file diff stats persisted in place of inline diff text (t3's shape).
/// The inline diff was the bigger bomb than outputs — 32KB/edit, unexercised
/// only because the claude harness emits none. Full diff text lives in the
/// sidecar behind `diff_ref`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolDiffStat {
    pub path: String,
    pub additions: u64,
    pub deletions: u64,
}

/// Line-level add/delete counts for one file's diff.
pub fn diff_stat(diff: &ToolDiff) -> ToolDiffStat {
    let (additions, deletions) = match &diff.old_text {
        None => (diff.new_text.lines().count() as u64, 0),
        Some(old) => {
            let text_diff = similar::TextDiff::from_lines(old.as_str(), diff.new_text.as_str());
            let mut additions = 0u64;
            let mut deletions = 0u64;
            for change in text_diff.iter_all_changes() {
                match change.tag() {
                    similar::ChangeTag::Insert => additions += 1,
                    similar::ChangeTag::Delete => deletions += 1,
                    similar::ChangeTag::Equal => {}
                }
            }
            (additions, deletions)
        }
    };
    ToolDiffStat {
        path: diff.path.clone(),
        additions,
        deletions,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MessageStatus {
    Streaming,
    Complete,
    Aborted,
}

/// Lifecycle of a spawned subagent, carried on its spawn chip.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SubagentStatus {
    Running,
    Done,
    Failed,
}

/// One rendered part of an assistant message.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum MessagePart {
    Text {
        id: String,
        text: String,
    },
    Reasoning {
        id: String,
        text: String,
        #[serde(default)]
        completed: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        duration_ms: Option<u64>,
    },
    #[serde(rename_all = "camelCase")]
    Tool {
        id: String,
        call: ToolCall,
        #[serde(default)]
        is_error: bool,
        /// True once a ToolResult arrived.
        #[serde(default)]
        resolved: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        execution: Option<ToolExecutionMeta>,
        /// One-line tool output summary ([`summarize_tool_output`]). Old
        /// entries (pre-strip) still carry up to 4KB here; old app versions
        /// render this field either way, so the strip is invisible to them.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        output: Option<String>,
        /// Inline file diff — written by pre-strip app versions only; new
        /// folds persist [`Self::Tool::diff_stats`] + `diff_ref` instead.
        /// Kept so old docs render their diffs.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        diff: Option<ToolDiff>,
        /// Sidecar key (`{chatId}/{partId}`) of the full output — additive;
        /// stamped by [`apply_sidecar_refs`] (the fold is chat-agnostic).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        output_ref: Option<String>,
        /// Full-output byte length, so the UI can say "Show full output (12 KB)".
        #[serde(default, skip_serializing_if = "Option::is_none")]
        output_bytes: Option<u64>,
        /// Sidecar key (`{chatId}/{partId}.diff`) of the full diff JSON.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        diff_ref: Option<String>,
        /// Per-file diff stats (additive replacement for inline `diff`).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        diff_stats: Option<Vec<ToolDiffStat>>,
        /// The SUBAGENT doc id this spawn chip indexes (additive; stamped by
        /// the engine like the sidecar refs — the fold is chat-agnostic).
        /// The chip IS the index: the client learns the doc/blob id from it,
        /// there is no listing endpoint.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        subagent_ref: Option<String>,
        /// Subagent lifecycle, DISTINCT from `resolved`: under the eager-done
        /// policy the spawn tool's own result lands while the subagent still
        /// runs. `running` → the ref is a live doc (watch it); `done`/
        /// `failed` → frozen (blob first, doc as fallback).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        subagent_status: Option<SubagentStatus>,
        /// One-line live tail of the subagent's latest output, folded from
        /// its tagged text deltas (capped; display-only).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        subagent_tail: Option<String>,
    },
    #[serde(rename_all = "camelCase")]
    Input {
        id: String,
        request_id: String,
        questions: Vec<UserInputQuestion>,
        #[serde(default)]
        resolved: bool,
    },
    Error {
        id: String,
        message: String,
    },
    /// Durable provider activity used by the chat Workers projection. This
    /// part is persisted and synchronized, but transcript rendering filters it.
    WorkflowTask {
        id: String,
        task: WorkflowTaskUpdate,
    },
}

impl MessagePart {
    pub fn id(&self) -> &str {
        match self {
            MessagePart::Text { id, .. }
            | MessagePart::Reasoning { id, .. }
            | MessagePart::Tool { id, .. }
            | MessagePart::Input { id, .. }
            | MessagePart::Error { id, .. }
            | MessagePart::WorkflowTask { id, .. } => id,
        }
    }

    pub fn byte_len(&self) -> usize {
        match self {
            MessagePart::Text { text, .. } => text.len(),
            MessagePart::Reasoning { text, .. } => text.len(),
            MessagePart::Tool {
                call,
                execution,
                output,
                diff,
                diff_stats,
                ..
            } => {
                serde_json::to_vec(call).map_or(0, |v| v.len())
                    + execution
                        .as_ref()
                        .map_or(0, |m| serde_json::to_vec(m).map_or(0, |v| v.len()))
                    + output.as_ref().map_or(0, String::len)
                    + diff
                        .as_ref()
                        .map_or(0, |d| serde_json::to_vec(d).map_or(0, |v| v.len()))
                    + diff_stats
                        .as_ref()
                        .map_or(0, |s| serde_json::to_vec(s).map_or(0, |v| v.len()))
            }
            MessagePart::Input { questions, .. } => {
                serde_json::to_vec(questions).map_or(0, |v| v.len())
            }
            MessagePart::Error { message, .. } => message.len(),
            MessagePart::WorkflowTask { task, .. } => {
                serde_json::to_vec(task).map_or(0, |value| value.len())
            }
        }
    }
}

fn last_transcript_part_mut(parts: &mut [MessagePart]) -> Option<&mut MessagePart> {
    parts
        .iter_mut()
        .rev()
        .find(|part| !matches!(part, MessagePart::WorkflowTask { .. }))
}

fn complete_trailing_reasoning(parts: &mut [MessagePart]) {
    if let Some(MessagePart::Reasoning { completed, .. }) = last_transcript_part_mut(parts) {
        *completed = true;
    }
}

fn replace_non_empty(slot: &mut Option<String>, incoming: &Option<String>) {
    if incoming
        .as_ref()
        .is_some_and(|value| !value.trim().is_empty())
    {
        *slot = incoming.clone();
    }
}

fn merge_workflow_usage(slot: &mut Option<WorkflowUsage>, incoming: Option<WorkflowUsage>) {
    let Some(incoming) = incoming else {
        return;
    };
    match slot {
        Some(current) => {
            if incoming.total_tokens.is_some() {
                current.total_tokens = incoming.total_tokens;
            }
            if incoming.tool_uses.is_some() {
                current.tool_uses = incoming.tool_uses;
            }
            if incoming.duration_ms.is_some() {
                current.duration_ms = incoming.duration_ms;
            }
        }
        None => *slot = Some(incoming),
    }
}

fn same_progress_node(left: &WorkflowProgressNode, right: &WorkflowProgressNode) -> bool {
    match (left, right) {
        (
            WorkflowProgressNode::Phase { index: left, .. },
            WorkflowProgressNode::Phase { index: right, .. },
        ) => left == right,
        (
            WorkflowProgressNode::Agent {
                index: left_index,
                phase_index: left_phase,
                agent_id: left_id,
                ..
            },
            WorkflowProgressNode::Agent {
                index: right_index,
                phase_index: right_phase,
                agent_id: right_id,
                ..
            },
        ) => match (left_id.as_deref(), right_id.as_deref()) {
            (Some(left), Some(right)) if !left.is_empty() && !right.is_empty() => left == right,
            _ => left_phase == right_phase && left_index == right_index,
        },
        _ => false,
    }
}

fn merge_progress_node(current: &mut WorkflowProgressNode, incoming: &WorkflowProgressNode) {
    match (current, incoming) {
        (
            WorkflowProgressNode::Phase { title, .. },
            WorkflowProgressNode::Phase {
                title: incoming_title,
                ..
            },
        ) => {
            if !incoming_title.trim().is_empty() {
                *title = incoming_title.clone();
            }
        }
        (
            WorkflowProgressNode::Agent {
                index,
                label,
                phase_index,
                phase_title,
                agent_id,
                model,
                state,
                prompt_preview,
                ..
            },
            WorkflowProgressNode::Agent {
                index: incoming_index,
                label: incoming_label,
                phase_index: incoming_phase_index,
                phase_title: incoming_phase_title,
                agent_id: incoming_agent_id,
                model: incoming_model,
                state: incoming_state,
                prompt_preview: incoming_prompt_preview,
                ..
            },
        ) => {
            *index = *incoming_index;
            *phase_index = *incoming_phase_index;
            if !incoming_label.trim().is_empty() {
                *label = incoming_label.clone();
            }
            replace_non_empty(phase_title, incoming_phase_title);
            replace_non_empty(agent_id, incoming_agent_id);
            replace_non_empty(model, incoming_model);
            replace_non_empty(state, incoming_state);
            replace_non_empty(prompt_preview, incoming_prompt_preview);
        }
        _ => {}
    }
}

fn merge_workflow_task(current: &mut WorkflowTaskUpdate, incoming: &WorkflowTaskUpdate) {
    current.status = incoming.status;
    replace_non_empty(&mut current.workflow_name, &incoming.workflow_name);
    replace_non_empty(&mut current.description, &incoming.description);
    merge_workflow_usage(&mut current.usage, incoming.usage);
    if incoming.agent_count.is_some() {
        current.agent_count = incoming.agent_count;
    }
    replace_non_empty(&mut current.task_type, &incoming.task_type);
    replace_non_empty(&mut current.subagent_type, &incoming.subagent_type);

    for incoming_node in &incoming.progress {
        if let Some(current_node) = current
            .progress
            .iter_mut()
            .find(|node| same_progress_node(node, incoming_node))
        {
            merge_progress_node(current_node, incoming_node);
        } else {
            current.progress.push(incoming_node.clone());
        }
    }
}

/// Fold one agent event into a parts accumulator, in place.
///
/// In place because the fold runs once per streamed event: rebuilding the
/// accumulator each time made long turns O(n²) in allocations.
///
/// Semantics from zeron `foldEventIntoParts`:
/// - `SessionStarted` / `Steered` reset the accumulator (turn boundary — makes replay safe).
/// - `TextDelta` appends to the trailing text part, or starts a new one if the trail is not text
///   (a tool call in between breaks the text block).
/// - `ToolCall` appends, or refreshes in place when the id already exists (SDK retry idempotence).
/// - `ToolResult` marks the matching tool part resolved / errored in place.
/// - `InputRequested` appends an input part; `InputResolved` marks it resolved.
/// - `Error` and `Done{error}` become visible error parts.
pub fn fold_event_into_parts(out: &mut Vec<MessagePart>, event: &AgentEvent) {
    let closes_reasoning = match event {
        AgentEvent::TextDelta { text } | AgentEvent::Error { message: text } => !text.is_empty(),
        AgentEvent::ToolCall { .. }
        | AgentEvent::InputRequested { .. }
        | AgentEvent::Done { .. } => true,
        _ => false,
    };
    if closes_reasoning {
        complete_trailing_reasoning(out);
    }
    match event {
        AgentEvent::SessionStarted { .. } | AgentEvent::Steered { .. } => {
            out.clear();
        }
        AgentEvent::TextDelta { text } => {
            if let Some(MessagePart::Text { text: tail, .. }) = last_transcript_part_mut(out) {
                tail.push_str(text);
            } else {
                let id = format!("t{}", out.len());
                out.push(MessagePart::Text {
                    id,
                    text: text.clone(),
                });
            }
        }
        AgentEvent::ReasoningDelta { text } => {
            if text.is_empty() {
                return;
            }
            if let Some(MessagePart::Reasoning {
                text: tail,
                completed: false,
                ..
            }) = last_transcript_part_mut(out)
            {
                tail.push_str(text);
            } else {
                let id = format!("r{}", out.len());
                out.push(MessagePart::Reasoning {
                    id,
                    text: text.clone(),
                    completed: false,
                    duration_ms: None,
                });
            }
        }
        AgentEvent::ToolCall { id, call } => {
            if let Some(existing) = out.iter_mut().find_map(|p| match p {
                MessagePart::Tool {
                    id: pid, call: c, ..
                } if pid == id => Some(c),
                _ => None,
            }) {
                *existing = call.clone();
            } else {
                out.push(MessagePart::Tool {
                    id: id.clone(),
                    call: call.clone(),
                    is_error: false,
                    resolved: false,
                    execution: None,
                    output: None,
                    diff: None,
                    output_ref: None,
                    output_bytes: None,
                    diff_ref: None,
                    diff_stats: None,
                    subagent_ref: None,
                    subagent_status: None,
                    subagent_tail: None,
                });
            }
        }
        AgentEvent::ToolResult {
            id,
            is_error,
            output,
            diff,
            execution,
        } => {
            for p in out.iter_mut() {
                if let MessagePart::Tool {
                    id: pid,
                    is_error: e,
                    resolved,
                    execution: execution_slot,
                    output: out_slot,
                    diff: diff_slot,
                    output_bytes,
                    diff_stats,
                    ..
                } = p
                    && pid == id
                {
                    *e = *is_error;
                    *resolved = true;
                    *execution_slot = *execution;
                    // Keep only the bounded summary in the replicated doc so
                    // an expanded card explains what the tool returned. Full
                    // output remains journal-only while the R2 sidecar is
                    // parked; inline diff text is still reduced to stats.
                    *out_slot = output.as_deref().and_then(summarize_tool_output);
                    *output_bytes = None;
                    *diff_slot = None;
                    *diff_stats = diff.as_ref().map(|d| vec![diff_stat(d)]);
                }
            }
        }
        AgentEvent::InputRequested {
            request_id,
            questions,
        } => {
            let id = format!("in-{request_id}");
            if !out.iter().any(|p| p.id() == id) {
                out.push(MessagePart::Input {
                    id,
                    request_id: request_id.clone(),
                    questions: questions.clone(),
                    resolved: false,
                });
            }
        }
        AgentEvent::InputResolved { request_id } => {
            for p in out.iter_mut() {
                if let MessagePart::Input {
                    request_id: rid,
                    resolved,
                    ..
                } = p
                    && rid == request_id
                {
                    *resolved = true;
                }
            }
        }
        AgentEvent::Error { message } => {
            let id = format!("e{}", out.len());
            out.push(MessagePart::Error {
                id,
                message: message.clone(),
            });
        }
        AgentEvent::Done { error, .. } => {
            if let Some(message) = error {
                let id = format!("e{}", out.len());
                out.push(MessagePart::Error {
                    id,
                    message: message.clone(),
                });
            }
        }
        AgentEvent::WorkflowTask { task } => {
            if let Some(current) = out.iter_mut().find_map(|part| match part {
                MessagePart::WorkflowTask { task: current, .. }
                    if current.task_id == task.task_id =>
                {
                    Some(current)
                }
                _ => None,
            }) {
                merge_workflow_task(current, task);
            } else {
                out.push(MessagePart::WorkflowTask {
                    id: format!("workflow-{}", task.task_id),
                    task: task.clone(),
                });
            }
        }
        // Subagent-attributed CONTENT belongs to the subagent's own doc (the
        // engine routes it there); the parent doc keeps only the spawn chip —
        // which these events refresh in place: LIFECYCLE ONLY. A live tail
        // was tried and rejected: rewriting the chip per delta batch grew
        // the parent doc's oplog for the whole subagent run and rendered as
        // distracting mid-stream fragments (user call, 2026-08-18). The
        // `subagent_tail` field stays in the schema for docs that carry it.
        AgentEvent::Subagent {
            parent_tool_use_id,
            event,
        } => {
            let status = match event.as_ref() {
                AgentEvent::Done { status, .. } => Some(match status {
                    zeron_proto::DoneStatus::Errored => SubagentStatus::Failed,
                    _ => SubagentStatus::Done,
                }),
                // A steer RESURRECTS a settled chip — it announces more work
                // (claude: a queued SendMessage relaunches the agent), so
                // this is the one event allowed past the no-regress guard.
                AgentEvent::UserMessage { .. } => Some(SubagentStatus::Running),
                _ => None,
            };
            for p in out.iter_mut() {
                if let MessagePart::Tool {
                    id,
                    subagent_status,
                    ..
                } = p
                    && id == parent_tool_use_id
                {
                    match status {
                        Some(s) => *subagent_status = Some(s),
                        // Any tagged traffic proves the subagent is live;
                        // never regress a terminal state.
                        None if !matches!(
                            subagent_status,
                            Some(SubagentStatus::Done) | Some(SubagentStatus::Failed)
                        ) =>
                        {
                            *subagent_status = Some(SubagentStatus::Running);
                        }
                        None => {}
                    }
                }
            }
        }
        // AvailableCommands feeds the engine's per-harness command cache, not
        // the transcript. UserMessage becomes its own doc ENTRY (the engine's
        // subagent sink writes it), never a part of the assistant message.
        AgentEvent::AssistantMessageCompleted { .. }
        | AgentEvent::Usage { .. }
        | AgentEvent::AvailableCommands { .. }
        | AgentEvent::UserMessage { .. } => {}
    }
}


/// Stamp sidecar keys onto resolved tool parts that have sidecar content.
///
/// Separate from the fold because the fold is chat-agnostic and pure; the
/// caller (who knows the chat id) runs this right after each fold step, before
/// the parts hit the doc. Idempotent. Key shape `{chatId}/{partId}` (+
/// `.diff`) matches the edge's `/blob/{chatId}/{partId}` route.
pub fn apply_sidecar_refs(chat_id: &str, parts: &mut [MessagePart]) {
    for part in parts.iter_mut() {
        if let MessagePart::Tool {
            id,
            resolved: true,
            output_ref,
            output_bytes,
            diff_ref,
            diff_stats,
            ..
        } = part
        {
            if output_ref.is_none() && output_bytes.is_some() {
                *output_ref = Some(format!("{chat_id}/{id}"));
            }
            if diff_ref.is_none() && diff_stats.is_some() {
                *diff_ref = Some(format!("{chat_id}/{id}.diff"));
            }
        }
    }
}

/// What a [`AgentEvent::ToolResult`] owes the sidecar: the full output text
/// and/or the full diff (as JSON), keyed by part id. `None` when the event
/// carries nothing worth uploading.
#[derive(Debug, Clone, PartialEq)]
pub struct SidecarPayload {
    pub part_id: String,
    pub output: Option<String>,
    pub diff: Option<ToolDiff>,
}

pub fn sidecar_payload(event: &AgentEvent) -> Option<SidecarPayload> {
    let AgentEvent::ToolResult {
        id, output, diff, ..
    } = event
    else {
        return None;
    };
    let output = output.clone().filter(|o| !o.trim().is_empty());
    if output.is_none() && diff.is_none() {
        return None;
    }
    Some(SidecarPayload {
        part_id: id.clone(),
        output,
        diff: diff.clone(),
    })
}

/// Render-only privacy policy — strip heavy/sensitive tool inputs before a call enters the doc.
///
/// Keeps: command / path / pattern / url / query / todo items / server+tool names.
/// Drops: WriteFile content, EditFile old/new strings, WebFetch prompt, Mcp/Unknown input.
/// Full inputs remain only in the host's local run journal. Idempotent.
pub fn sanitize_tool_call(call: &ToolCall) -> ToolCall {
    match call {
        ToolCall::WriteFile { path, .. } => ToolCall::WriteFile {
            path: path.clone(),
            content: None,
        },
        ToolCall::EditFile { path, .. } => ToolCall::EditFile {
            path: path.clone(),
            old_string: None,
            new_string: None,
        },
        ToolCall::WebFetch { url, .. } => ToolCall::WebFetch {
            url: url.clone(),
            prompt: None,
        },
        ToolCall::Mcp { server, tool, .. } => ToolCall::Mcp {
            server: server.clone(),
            tool: tool.clone(),
            input: None,
        },
        ToolCall::Unknown { name, .. } => ToolCall::Unknown {
            name: name.clone(),
            input: None,
        },
        other => other.clone(),
    }
}

/// Deterministic continuation id: `"{root}#c{n}"`.
pub fn continuation_id(root: &str, index: usize) -> String {
    format!("{root}#c{index}")
}

/// Split an oversized parts list into chunks each under `MSG_INLINE_MAX` bytes.
///
/// Splitting happens at part boundaries; an oversized text part is itself chunked at char
/// boundaries. Returns one Vec per resulting entry — the first keeps the root id, the rest are
/// continuations (`continuation_id(root, i)`), matching `splitMessageEntry` in zeron.
pub fn split_parts(parts: &[MessagePart]) -> Vec<Vec<MessagePart>> {
    let mut chunks: Vec<Vec<MessagePart>> = vec![Vec::new()];
    let mut current_bytes = 0usize;

    let push_part = |chunks: &mut Vec<Vec<MessagePart>>, current: &mut usize, part: MessagePart| {
        let len = part.byte_len();
        if *current > 0 && *current + len > MSG_INLINE_MAX {
            chunks.push(Vec::new());
            *current = 0;
        }
        *current += len;
        chunks.last_mut().unwrap().push(part);
    };

    for part in parts {
        match part {
            MessagePart::Text { id, text } if text.len() > MSG_INLINE_MAX => {
                // Chunk oversized text at char boundaries.
                let mut start = 0usize;
                let mut piece = 0usize;
                while start < text.len() {
                    let mut end = (start + MSG_INLINE_MAX).min(text.len());
                    while end < text.len() && !text.is_char_boundary(end) {
                        end -= 1;
                    }
                    // Guard: ensure forward progress on pathological boundaries.
                    if end <= start {
                        end = text.len();
                    }
                    let sub = MessagePart::Text {
                        id: if piece == 0 {
                            id.clone()
                        } else {
                            format!("{id}~{piece}")
                        },
                        text: text[start..end].to_string(),
                    };
                    push_part(&mut chunks, &mut current_bytes, sub);
                    start = end;
                    piece += 1;
                }
            }
            MessagePart::Reasoning {
                id,
                text,
                completed,
                duration_ms,
            } if text.len() > MSG_INLINE_MAX => {
                let mut start = 0usize;
                let mut piece = 0usize;
                while start < text.len() {
                    let mut end = (start + MSG_INLINE_MAX).min(text.len());
                    while end < text.len() && !text.is_char_boundary(end) {
                        end -= 1;
                    }
                    if end <= start {
                        end = text.len();
                    }
                    let is_last = end == text.len();
                    let sub = MessagePart::Reasoning {
                        id: if piece == 0 {
                            id.clone()
                        } else {
                            format!("{id}~{piece}")
                        },
                        text: text[start..end].to_string(),
                        completed: is_last && *completed,
                        duration_ms: is_last.then_some(*duration_ms).flatten(),
                    };
                    push_part(&mut chunks, &mut current_bytes, sub);
                    start = end;
                    piece += 1;
                }
            }
            other => push_part(&mut chunks, &mut current_bytes, other.clone()),
        }
    }
    chunks
}

/// Render-time inverse of splitting: concatenate continuation entries' parts in list order.
pub fn join_continuations(entries: Vec<Vec<MessagePart>>) -> Vec<MessagePart> {
    entries.into_iter().flatten().collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use zeron_proto::{
        WorkflowProgressNode, WorkflowTaskStatus, WorkflowTaskUpdate, WorkflowUsage,
    };

    fn text_delta(s: &str) -> AgentEvent {
        AgentEvent::TextDelta { text: s.into() }
    }

    #[test]
    fn workflow_updates_preserve_richer_fields() {
        let mut parts = Vec::new();
        fold_event_into_parts(
            &mut parts,
            &AgentEvent::WorkflowTask {
                task: WorkflowTaskUpdate {
                    task_id: "wf-1".into(),
                    status: WorkflowTaskStatus::Running,
                    workflow_name: Some("Audit".into()),
                    description: Some("Review repository".into()),
                    usage: Some(WorkflowUsage {
                        total_tokens: Some(1_200),
                        tool_uses: Some(4),
                        duration_ms: None,
                    }),
                    progress: vec![
                        WorkflowProgressNode::Phase {
                            index: 0,
                            title: "Review".into(),
                        },
                        WorkflowProgressNode::Agent {
                            index: 0,
                            label: "Reviewer".into(),
                            phase_index: 0,
                            phase_title: Some("Review".into()),
                            agent_id: Some("agent-1".into()),
                            model: Some("sonnet".into()),
                            state: Some("running".into()),
                            prompt_preview: Some("Inspect the repository".into()),
                        },
                    ],
                    agent_count: Some(1),
                    task_type: Some("local_workflow".into()),
                    subagent_type: None,
                },
            },
        );
        fold_event_into_parts(
            &mut parts,
            &AgentEvent::WorkflowTask {
                task: WorkflowTaskUpdate {
                    task_id: "wf-1".into(),
                    status: WorkflowTaskStatus::Completed,
                    workflow_name: None,
                    description: None,
                    usage: Some(WorkflowUsage {
                        total_tokens: None,
                        tool_uses: None,
                        duration_ms: Some(2_500),
                    }),
                    progress: vec![WorkflowProgressNode::Agent {
                        index: 1,
                        label: "   ".into(),
                        phase_index: 1,
                        phase_title: None,
                        agent_id: Some("agent-1".into()),
                        model: None,
                        state: Some("completed".into()),
                        prompt_preview: None,
                    }],
                    agent_count: None,
                    task_type: None,
                    subagent_type: None,
                },
            },
        );

        let MessagePart::WorkflowTask { task, .. } = &parts[0] else {
            panic!("expected workflow activity part")
        };
        assert_eq!(parts.len(), 1);
        assert_eq!(task.status, WorkflowTaskStatus::Completed);
        assert_eq!(task.workflow_name.as_deref(), Some("Audit"));
        assert_eq!(task.description.as_deref(), Some("Review repository"));
        assert_eq!(
            task.usage,
            Some(WorkflowUsage {
                total_tokens: Some(1_200),
                tool_uses: Some(4),
                duration_ms: Some(2_500),
            })
        );
        assert_eq!(task.progress.len(), 2);
        assert!(matches!(
            &task.progress[1],
            WorkflowProgressNode::Agent {
                index,
                label,
                phase_index,
                model,
                state,
                prompt_preview,
                ..
            }
                if label == "Reviewer"
                    && *index == 1
                    && *phase_index == 1
                    && model.as_deref() == Some("sonnet")
                    && state.as_deref() == Some("completed")
                    && prompt_preview.as_deref() == Some("Inspect the repository")
        ));
    }

    #[test]
    fn workflow_activity_does_not_split_reasoning() {
        let mut parts = Vec::new();
        fold_event_into_parts(
            &mut parts,
            &AgentEvent::ReasoningDelta {
                text: "Reviewing".into(),
            },
        );
        fold_event_into_parts(
            &mut parts,
            &AgentEvent::WorkflowTask {
                task: WorkflowTaskUpdate {
                    task_id: "wf-1".into(),
                    status: WorkflowTaskStatus::Running,
                    workflow_name: Some("Audit".into()),
                    description: None,
                    usage: None,
                    progress: Vec::new(),
                    agent_count: None,
                    task_type: Some("local_workflow".into()),
                    subagent_type: None,
                },
            },
        );
        fold_event_into_parts(
            &mut parts,
            &AgentEvent::ReasoningDelta {
                text: " repository".into(),
            },
        );
        fold_event_into_parts(
            &mut parts,
            &AgentEvent::ToolCall {
                id: "tool-1".into(),
                call: ToolCall::Exec {
                    command: "git status".into(),
                },
            },
        );

        assert_eq!(
            parts
                .iter()
                .filter(|part| matches!(part, MessagePart::Reasoning { .. }))
                .count(),
            1
        );
        assert!(matches!(
            &parts[0],
            MessagePart::Reasoning { text, completed: true, .. }
                if text == "Reviewing repository"
        ));
    }

    #[test]
    fn text_deltas_merge_until_broken_by_tool() {
        let mut parts = Vec::new();
        fold_event_into_parts(&mut parts, &text_delta("Hello "));
        fold_event_into_parts(&mut parts, &text_delta("world"));
        assert_eq!(parts.len(), 1);
        fold_event_into_parts(
            &mut parts,
            &AgentEvent::ToolCall {
                id: "tool-1".into(),
                call: ToolCall::Exec {
                    command: "ls".into(),
                },
            },
        );
        fold_event_into_parts(&mut parts, &text_delta("after"));
        assert_eq!(parts.len(), 3);
        match &parts[2] {
            MessagePart::Text { text, .. } => assert_eq!(text, "after"),
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn reasoning_deltas_append_and_close_before_the_next_visible_part() {
        let mut parts = Vec::new();
        fold_event_into_parts(
            &mut parts,
            &AgentEvent::ReasoningDelta { text: "one".into() },
        );
        fold_event_into_parts(
            &mut parts,
            &AgentEvent::ReasoningDelta {
                text: " two".into(),
            },
        );
        fold_event_into_parts(
            &mut parts,
            &AgentEvent::ToolCall {
                id: "c1".into(),
                call: ToolCall::Exec {
                    command: "pwd".into(),
                },
            },
        );
        assert!(matches!(
            &parts[0],
            MessagePart::Reasoning {
                text,
                completed: true,
                ..
            } if text == "one two"
        ));
        assert!(matches!(&parts[1], MessagePart::Tool { .. }));
    }

    #[test]
    fn tool_result_does_not_split_concurrent_reasoning() {
        let mut parts = Vec::new();
        fold_event_into_parts(
            &mut parts,
            &AgentEvent::ToolCall {
                id: "c1".into(),
                call: ToolCall::Exec {
                    command: "sleep 1".into(),
                },
            },
        );
        fold_event_into_parts(
            &mut parts,
            &AgentEvent::ReasoningDelta {
                text: "waiting".into(),
            },
        );
        fold_event_into_parts(
            &mut parts,
            &AgentEvent::ToolResult {
                id: "c1".into(),
                is_error: false,
                output: None,
                diff: None,
                execution: None,
            },
        );
        fold_event_into_parts(
            &mut parts,
            &AgentEvent::ReasoningDelta {
                text: " done".into(),
            },
        );

        assert!(matches!(
            &parts[1],
            MessagePart::Reasoning {
                text,
                completed: false,
                ..
            } if text == "waiting done"
        ));
        assert_eq!(
            parts
                .iter()
                .filter(|part| matches!(part, MessagePart::Reasoning { .. }))
                .count(),
            1
        );
    }

    #[test]
    fn empty_reasoning_is_a_heartbeat_and_done_closes_a_live_trace() {
        let mut parts = Vec::new();
        fold_event_into_parts(
            &mut parts,
            &AgentEvent::ReasoningDelta {
                text: String::new(),
            },
        );
        assert!(parts.is_empty());
        fold_event_into_parts(
            &mut parts,
            &AgentEvent::ReasoningDelta {
                text: "checking".into(),
            },
        );
        fold_event_into_parts(
            &mut parts,
            &AgentEvent::Done {
                status: zeron_proto::DoneStatus::Completed,
                result: None,
                error: None,
                session_id: None,
            },
        );
        assert!(matches!(
            &parts[0],
            MessagePart::Reasoning {
                completed: true,
                ..
            }
        ));
    }

    #[test]
    fn session_started_resets_accumulator() {
        let mut parts = Vec::new();
        fold_event_into_parts(&mut parts, &text_delta("junk"));
        fold_event_into_parts(
            &mut parts,
            &AgentEvent::SessionStarted {
                harness: zeron_proto::HarnessId::Mock,
                model: "m".into(),
                tools: vec![],
                cwd: "/".into(),
                session_id: "s".into(),
                assistant_message_id: "a".into(),
            },
        );
        assert!(parts.is_empty());
    }

    #[test]
    fn tool_call_refresh_is_idempotent() {
        let call = AgentEvent::ToolCall {
            id: "t".into(),
            call: ToolCall::Exec {
                command: "ls".into(),
            },
        };
        let mut once = Vec::new();
        fold_event_into_parts(&mut once, &call);
        let mut twice = once.clone();
        fold_event_into_parts(&mut twice, &call);
        assert_eq!(once, twice);
    }

    #[test]
    fn tool_result_marks_resolution() {
        let mut parts = Vec::new();
        fold_event_into_parts(
            &mut parts,
            &AgentEvent::ToolCall {
                id: "t".into(),
                call: ToolCall::Exec {
                    command: "ls".into(),
                },
            },
        );
        fold_event_into_parts(
            &mut parts,
            &AgentEvent::ToolResult {
                id: "t".into(),
                is_error: true,
                output: None,
                diff: None,
                execution: None,
            },
        );
        match &parts[0] {
            MessagePart::Tool {
                is_error, resolved, ..
            } => {
                assert!(*is_error);
                assert!(*resolved);
            }
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn tool_result_preserves_optional_execution_metadata() {
        let mut parts = Vec::new();
        fold_event_into_parts(
            &mut parts,
            &AgentEvent::ToolCall {
                id: "exec".into(),
                call: ToolCall::Exec {
                    command: "cargo test".into(),
                },
            },
        );
        let metadata = zeron_proto::ToolExecutionMeta {
            exit_code: Some(0),
            duration_ms: Some(1_250),
        };
        fold_event_into_parts(
            &mut parts,
            &AgentEvent::ToolResult {
                id: "exec".into(),
                is_error: false,
                output: Some("ok".into()),
                diff: None,
                execution: Some(metadata),
            },
        );
        assert!(matches!(
            &parts[0],
            MessagePart::Tool {
                execution: Some(actual),
                ..
            } if *actual == metadata
        ));
    }

    #[test]
    fn tool_result_keeps_bounded_output_for_the_transcript_card() {
        let mut parts = Vec::new();
        fold_event_into_parts(
            &mut parts,
            &AgentEvent::ToolCall {
                id: "worker-call".into(),
                call: ToolCall::Mcp {
                    server: "comet-workers".into(),
                    tool: "launch_worker".into(),
                    input: None,
                },
            },
        );
        fold_event_into_parts(
            &mut parts,
            &AgentEvent::ToolResult {
                id: "worker-call".into(),
                is_error: false,
                output: Some(
                    r#"{"session_id":"worker-1","launched":true,"briefing_submitted":true}"#.into(),
                ),
                diff: None,
                execution: None,
            },
        );

        assert!(matches!(
            &parts[0],
            MessagePart::Tool {
                resolved: true,
                output: Some(output),
                ..
            } if output.contains("worker-1") && output.contains("launched")
        ));
    }

    #[test]
    fn sanitize_strips_heavy_inputs_and_is_idempotent() {
        let call = ToolCall::WriteFile {
            path: "/x".into(),
            content: Some("secret".into()),
        };
        let clean = sanitize_tool_call(&call);
        assert_eq!(
            clean,
            ToolCall::WriteFile {
                path: "/x".into(),
                content: None
            }
        );
        assert_eq!(sanitize_tool_call(&clean), clean);
    }

    #[test]
    fn split_and_join_round_trip() {
        let big = "x".repeat(MSG_INLINE_MAX * 2 + 100);
        let parts = vec![
            MessagePart::Text {
                id: "t0".into(),
                text: big.clone(),
            },
            MessagePart::Tool {
                id: "tool-1".into(),
                call: ToolCall::Exec {
                    command: "ls".into(),
                },
                is_error: false,
                resolved: true,
                execution: None,
                output: None,
                diff: None,
                output_ref: None,
                output_bytes: None,
                diff_ref: None,
                diff_stats: None,
                subagent_ref: None,
                subagent_status: None,
                subagent_tail: None,
            },
        ];
        let chunks = split_parts(&parts);
        assert!(
            chunks.len() >= 3,
            "expected >=3 chunks, got {}",
            chunks.len()
        );
        for chunk in &chunks {
            let bytes: usize = chunk.iter().map(|p| p.byte_len()).sum();
            assert!(bytes <= MSG_INLINE_MAX, "chunk over cap: {bytes}");
        }
        let joined = join_continuations(chunks);
        let text: String = joined
            .iter()
            .filter_map(|p| match p {
                MessagePart::Text { text, .. } => Some(text.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(text, big);
        assert!(matches!(joined.last().unwrap(), MessagePart::Tool { .. }));
    }

    #[test]
    fn continuation_ids_are_deterministic() {
        assert_eq!(continuation_id("m1", 1), "m1#c1");
    }

    // ── A1 strip (docs/chat2-sync.md) ───────────────────────────────────────

    #[test]
    fn summarize_inlines_small_outputs_and_marks_big_cuts() {
        assert_eq!(summarize_tool_output(""), None);
        assert_eq!(summarize_tool_output("  \n\t\n"), None);
        assert_eq!(summarize_tool_output("one line"), Some("one line".into()));
        // Small multi-line outputs ride whole — no summary, no "…".
        assert_eq!(
            summarize_tool_output("\n\nfirst real\nsecond"),
            Some("first real\nsecond".into())
        );
        assert_eq!(
            summarize_tool_output("only line\n\n  \n"),
            Some("only line".into())
        );
        // Markdown fences are transport wrapping, never content: stripped
        // even when they'd otherwise be the first line, and a fence-only
        // output is blank.
        assert_eq!(
            summarize_tool_output("```console\nreal content\n```"),
            Some("real content".into())
        );
        assert_eq!(summarize_tool_output("```\n```"), None);
        // Big outputs: first non-empty (post-fence) line + unconditional "…".
        let big = format!("```console\nhead line\n{}\n```", "x".repeat(300));
        assert_eq!(summarize_tool_output(&big), Some("head line…".into()));
        let long = "x".repeat(TOOL_OUTPUT_SUMMARY_MAX + 40);
        let summary = summarize_tool_output(&long).unwrap();
        assert_eq!(summary.chars().count(), TOOL_OUTPUT_SUMMARY_MAX + 1);
        assert!(summary.ends_with('…'));
        // Char-boundary safety on multibyte input.
        let wide = "é".repeat(TOOL_OUTPUT_SUMMARY_MAX + 5);
        let summary = summarize_tool_output(&wide).unwrap();
        assert_eq!(summary.chars().count(), TOOL_OUTPUT_SUMMARY_MAX + 1);
    }

    #[test]
    fn diff_stat_counts_line_changes() {
        let stat = diff_stat(&ToolDiff {
            path: "/w/a.rs".into(),
            old_text: Some("a\nb\nc\n".into()),
            new_text: "a\nB\nc\nd\n".into(),
        });
        assert_eq!(stat.path, "/w/a.rs");
        assert_eq!(stat.additions, 2); // B + d
        assert_eq!(stat.deletions, 1); // b
        // New file: every line is an addition.
        let stat = diff_stat(&ToolDiff {
            path: "/w/new.rs".into(),
            old_text: None,
            new_text: "one\ntwo\n".into(),
        });
        assert_eq!((stat.additions, stat.deletions), (2, 0));
    }

    #[test]
    fn fold_strips_output_to_summary_and_diff_to_stats() {
        let mut parts = Vec::new();
        fold_event_into_parts(
            &mut parts,
            &AgentEvent::ToolCall {
                id: "t".into(),
                call: ToolCall::Exec {
                    command: "cargo test".into(),
                },
            },
        );
        let full = "running 42 tests\n".repeat(300); // ~5KB, was 4KB inline pre-strip
        fold_event_into_parts(
            &mut parts,
            &AgentEvent::ToolResult {
                id: "t".into(),
                is_error: false,
                output: Some(full.clone()),
                diff: Some(ToolDiff {
                    path: "/w/a.rs".into(),
                    old_text: Some("a\n".into()),
                    new_text: "b\n".into(),
                }),
                execution: None,
            },
        );
        match &parts[0] {
            MessagePart::Tool {
                output,
                output_bytes,
                diff,
                diff_stats,
                ..
            } => {
                // The bounded output summary explains the result in the
                // expanded card; full output stays journal-only. Diff text
                // still stays out of the doc while stats survive.
                assert_eq!(output.as_deref(), Some("running 42 tests…"));
                assert_eq!(*output_bytes, None);
                assert!(diff.is_none(), "inline diff text must not enter the doc");
                let stats = diff_stats.as_ref().unwrap();
                assert_eq!(stats.len(), 1);
                assert_eq!((stats[0].additions, stats[0].deletions), (1, 1));
            }
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn sidecar_refs_stamp_once_and_only_where_content_exists() {
        let mut parts = Vec::new();
        fold_event_into_parts(
            &mut parts,
            &AgentEvent::ToolCall {
                id: "t1".into(),
                call: ToolCall::Exec {
                    command: "ls".into(),
                },
            },
        );
        // Unresolved: no refs yet.
        apply_sidecar_refs("chat-9", &mut parts);
        assert!(matches!(
            &parts[0],
            MessagePart::Tool {
                output_ref: None,
                diff_ref: None,
                ..
            }
        ));
        fold_event_into_parts(
            &mut parts,
            &AgentEvent::ToolResult {
                id: "t1".into(),
                is_error: false,
                output: Some("hello".into()),
                diff: Some(ToolDiff {
                    path: "/w/a".into(),
                    old_text: None,
                    new_text: "x\n".into(),
                }),
                execution: None,
            },
        );
        apply_sidecar_refs("chat-9", &mut parts);
        apply_sidecar_refs("chat-9", &mut parts); // idempotent
        match &parts[0] {
            MessagePart::Tool {
                output_ref,
                diff_ref,
                ..
            } => {
                // One-liner fold: outputs never reach the doc, so there is
                // no output content to key even after resolution; diff
                // STATS exist, so the diff ref still stamps.
                assert_eq!(output_ref.as_deref(), None);
                assert_eq!(diff_ref.as_deref(), Some("chat-9/t1.diff"));
            }
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn subagent_events_refresh_the_spawn_chip_in_place() {
        use zeron_proto::DoneStatus;
        let mut parts = Vec::new();
        fold_event_into_parts(
            &mut parts,
            &AgentEvent::ToolCall {
                id: "toolu_sub".into(),
                call: ToolCall::Unknown {
                    name: "Agent".into(),
                    input: None,
                },
            },
        );
        // Tagged traffic marks the chip running — and ONLY that: the tail
        // stays untouched (per-delta chip rewrites polluted the parent doc).
        fold_event_into_parts(
            &mut parts,
            &AgentEvent::Subagent {
                parent_tool_use_id: "toolu_sub".into(),
                event: Box::new(AgentEvent::TextDelta {
                    text: "scanning\nfound 3 issues".into(),
                }),
            },
        );
        match &parts[0] {
            MessagePart::Tool {
                subagent_status,
                subagent_tail,
                ..
            } => {
                assert_eq!(*subagent_status, Some(SubagentStatus::Running));
                assert_eq!(*subagent_tail, None);
            }
            other => panic!("{other:?}"),
        }
        // A tagged Done is terminal; later traffic must not regress it.
        fold_event_into_parts(
            &mut parts,
            &AgentEvent::Subagent {
                parent_tool_use_id: "toolu_sub".into(),
                event: Box::new(AgentEvent::Done {
                    status: DoneStatus::Completed,
                    result: None,
                    error: None,
                    session_id: None,
                }),
            },
        );
        fold_event_into_parts(
            &mut parts,
            &AgentEvent::Subagent {
                parent_tool_use_id: "toolu_sub".into(),
                event: Box::new(AgentEvent::TextDelta {
                    text: "late flush".into(),
                }),
            },
        );
        match &parts[0] {
            MessagePart::Tool {
                subagent_status, ..
            } => assert_eq!(*subagent_status, Some(SubagentStatus::Done)),
            other => panic!("{other:?}"),
        }
        // Content never leaked into the parent parts.
        assert_eq!(parts.len(), 1);
    }

    #[test]
    fn sidecar_payload_carries_full_texts() {
        assert_eq!(
            sidecar_payload(&AgentEvent::TextDelta { text: "x".into() }),
            None
        );
        assert_eq!(
            sidecar_payload(&AgentEvent::ToolResult {
                id: "t".into(),
                is_error: false,
                output: Some("   \n".into()),
                diff: None,
                execution: None,
            }),
            None,
            "blank output uploads nothing"
        );
        let payload = sidecar_payload(&AgentEvent::ToolResult {
            id: "t".into(),
            is_error: true,
            output: Some("full output".into()),
            diff: None,
            execution: None,
        })
        .unwrap();
        assert_eq!(payload.part_id, "t");
        assert_eq!(payload.output.as_deref(), Some("full output"));
    }
}
