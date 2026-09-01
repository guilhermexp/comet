//! Typed Trajectory contracts and pure projections.
//!
//! Trajectory is a device-local read model for inspecting the execution chronology
//! and technical details of Chat runs. This module defines the shared wire and
//! projection contracts: stable record identity, lane classification (Input,
//! Model, Tools), status/error precedence, timing modes (recorded vs. sequence-only),
//! sanitized payload/result representations, hierarchical grouping, and idempotent
//! delta reconciliation.
//!
//! All logic in this module is pure: no I/O, no tokio, no database access.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::agent::{ToolCall, ToolDiff, ToolExecutionMeta};

// ---------------------------------------------------------------------------
// Stable Identity & References
// ---------------------------------------------------------------------------

/// Stable identifier for one Trajectory record.
///
/// Records are ordered by `(source_seq, sub_seq)` within a run.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrajectoryRecordId {
    pub run_id: String,
    pub source_seq: u64,
    pub sub_seq: u32,
}

impl TrajectoryRecordId {
    pub fn new(run_id: impl Into<String>, source_seq: u64, sub_seq: u32) -> Self {
        Self {
            run_id: run_id.into(),
            source_seq,
            sub_seq,
        }
    }

    /// String key for deterministic display and map lookup.
    pub fn key(&self) -> String {
        format!("{}:{}:{}", self.run_id, self.source_seq, self.sub_seq)
    }
}

/// Target field for explicit local-only raw reveal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TrajectoryRawField {
    Payload,
    Result,
}

/// Opaque source reference to the underlying local Run Journal entry.
///
/// This reference never carries raw payload or result text. It is used exclusively
/// by the local-only raw reveal RPC to locate the original journal event on the
/// capturing device after validating Chat ownership.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrajectoryRawRef {
    pub chat_id: String,
    pub source_seq: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_tool_use_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub call_id: Option<String>,
    pub field: TrajectoryRawField,
}

impl TrajectoryRawRef {
    pub fn new(
        chat_id: impl Into<String>,
        source_seq: u64,
        parent_tool_use_id: Option<String>,
        call_id: Option<String>,
        field: TrajectoryRawField,
    ) -> Self {
        Self {
            chat_id: chat_id.into(),
            source_seq,
            parent_tool_use_id,
            call_id,
            field,
        }
    }
}

// ---------------------------------------------------------------------------
// Lanes & Kinds
// ---------------------------------------------------------------------------

/// The three timeline overview lanes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TrajectoryLane {
    Input,
    Model,
    Tools,
}

impl TrajectoryLane {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Input => "input",
            Self::Model => "model",
            Self::Tools => "tools",
        }
    }
}

/// Semantic classification of a Trajectory record.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum TrajectoryRecordKind {
    SessionStarted,
    UserMessage,
    InputRequested,
    InputResolved,
    Steered,
    ContextUsage,
    AvailableCommands,
    AssistantMessage,
    Reasoning,
    WorkflowTask,
    ToolCall { tool_name: String },
    ToolResult { tool_name: String },
    ToolDiff { tool_name: String },
    Error,
    Done,
    Interrupted,
    Degraded,
    Custom { name: String },
}

impl TrajectoryRecordKind {
    /// Pure mapping from semantic kind to default timeline lane.
    pub fn default_lane(&self) -> TrajectoryLane {
        match self {
            Self::SessionStarted
            | Self::UserMessage
            | Self::InputRequested
            | Self::InputResolved
            | Self::Steered
            | Self::ContextUsage
            | Self::AvailableCommands => TrajectoryLane::Input,

            Self::AssistantMessage
            | Self::Reasoning
            | Self::WorkflowTask
            | Self::Done
            | Self::Interrupted
            | Self::Degraded
            | Self::Error => TrajectoryLane::Model,

            Self::ToolCall { .. } | Self::ToolResult { .. } | Self::ToolDiff { .. } => {
                TrajectoryLane::Tools
            }

            Self::Custom { .. } => TrajectoryLane::Model,
        }
    }
}

// ---------------------------------------------------------------------------
// Status & Error Precedence
// ---------------------------------------------------------------------------

/// Execution status of a record or run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TrajectoryStatus {
    Running,
    Completed,
    Error,
    Interrupted,
    Unsettled,
    Degraded,
}

impl TrajectoryStatus {
    pub fn is_error(&self) -> bool {
        matches!(self, Self::Error)
    }

    pub fn is_terminal(&self) -> bool {
        matches!(self, Self::Completed | Self::Error | Self::Interrupted)
    }
}

/// Compute effective status giving error state precedence without losing
/// the record's semantic lane or classification.
pub fn effective_status(base: TrajectoryStatus, is_error: bool) -> TrajectoryStatus {
    if is_error {
        TrajectoryStatus::Error
    } else {
        base
    }
}

// ---------------------------------------------------------------------------
// Timing Modes & Durations
// ---------------------------------------------------------------------------

/// Timing mode for a record or trajectory sequence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TrajectoryTimingMode {
    /// Equal-width geometry; no timestamps or durations were recorded.
    SequenceOnly,
    /// Measured timestamps and durations are available.
    Recorded,
}

/// Timing facts for one record or run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrajectoryTiming {
    pub mode: TrajectoryTimingMode,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub started_at: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ended_at: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ttft_ms: Option<u64>,
}

impl TrajectoryTiming {
    /// Create a sequence-only timing entry.
    pub fn sequence_only() -> Self {
        Self {
            mode: TrajectoryTimingMode::SequenceOnly,
            started_at: None,
            ended_at: None,
            duration_ms: None,
            ttft_ms: None,
        }
    }

    /// Create a recorded timing entry.
    pub fn recorded(
        started_at: Option<DateTime<Utc>>,
        ended_at: Option<DateTime<Utc>>,
        duration_ms: Option<u64>,
        ttft_ms: Option<u64>,
    ) -> Self {
        Self {
            mode: TrajectoryTimingMode::Recorded,
            started_at,
            ended_at,
            duration_ms,
            ttft_ms,
        }
    }

    /// Calculate duration from start and end times if not already set.
    pub fn effective_duration_ms(&self) -> Option<u64> {
        if self.mode == TrajectoryTimingMode::SequenceOnly {
            return None;
        }
        if let Some(d) = self.duration_ms {
            return Some(d);
        }
        if let (Some(start), Some(end)) = (self.started_at, self.ended_at) {
            let diff = end.signed_duration_since(start).num_milliseconds();
            if diff >= 0 {
                return Some(diff as u64);
            }
        }
        None
    }
}

/// Format duration in milliseconds into a concise readable string ("45ms", "1.2s", "3m 12s").
///
/// Returns `None` when timing is absent, sequence-only, or unavailable. Missing timing
/// must NEVER be formatted as "0ms" or an estimated value.
pub fn format_duration(timing: Option<&TrajectoryTiming>) -> Option<String> {
    let t = timing?;
    if t.mode == TrajectoryTimingMode::SequenceOnly {
        return None;
    }
    let ms = t.effective_duration_ms()?;
    Some(format_duration_ms(ms))
}

/// Helper for raw millisecond values.
pub fn format_duration_ms(ms: u64) -> String {
    if ms < 1_000 {
        format!("{}ms", ms)
    } else if ms < 60_000 {
        let secs = ms as f64 / 1000.0;
        format!("{:.2}s", secs)
    } else {
        let mins = ms / 60_000;
        let rem_secs = (ms % 60_000) / 1000;
        format!("{}m {}s", mins, rem_secs)
    }
}

/// Format duration or return a fixed unavailable placeholder ("—").
pub fn format_duration_or_unavailable(timing: Option<&TrajectoryTiming>) -> String {
    match format_duration(timing) {
        Some(d) => d,
        None => "—".to_string(),
    }
}

// ---------------------------------------------------------------------------
// Sanitized Payloads & Results
// ---------------------------------------------------------------------------

/// Sanitized payload preview for inspector view.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrajectoryPayloadPreview {
    pub summary: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sanitized_text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub schema_info: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub raw_ref: Option<TrajectoryRawRef>,
}

/// Sanitized result preview for inspector view.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrajectoryResultPreview {
    pub summary: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sanitized_text: Option<String>,
    pub is_error: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub raw_ref: Option<TrajectoryRawRef>,
}

/// Token usage metadata.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrajectoryUsage {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input_tokens: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_tokens: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub total_tokens: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_window: Option<u64>,
}

// ---------------------------------------------------------------------------
// Sanitization Budget Helpers
// ---------------------------------------------------------------------------

pub const DEFAULT_PREVIEW_BYTE_CAP: usize = 1_024;
pub const MAX_SUMMARY_LEN: usize = 256;

/// Bounded string truncation preserving UTF-8 boundaries.
pub fn truncate_preview(text: &str, byte_cap: usize) -> String {
    if text.len() <= byte_cap {
        return text.to_string();
    }
    // Find highest valid UTF-8 boundary within byte_cap
    let mut end = byte_cap;
    while end > 0 && !text.is_char_boundary(end) {
        end -= 1;
    }
    let truncated = &text[..end];
    format!("{}… (truncated)", truncated.trim_end())
}

/// Bounded summary truncation preserving UTF-8 boundaries.
pub fn truncate_summary(text: &str) -> String {
    if text.len() <= MAX_SUMMARY_LEN {
        return text.to_string();
    }
    let mut end = MAX_SUMMARY_LEN;
    while end > 0 && !text.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}…", &text[..end])
}

/// Sanitize prompt or message text for preview.
pub fn sanitize_prompt_preview(text: &str, byte_cap: usize) -> (String, Option<String>) {
    let single = text.split_whitespace().collect::<Vec<_>>().join(" ");
    let summary = if single.len() > MAX_SUMMARY_LEN {
        let mut end = MAX_SUMMARY_LEN;
        while end > 0 && !single.is_char_boundary(end) {
            end -= 1;
        }
        format!("{}…", &single[..end])
    } else {
        single
    };
    let preview = if text.len() > byte_cap {
        Some(truncate_preview(text, byte_cap))
    } else {
        Some(text.to_string())
    };
    (summary, preview)
}

/// Derive safe summary and bounded preview from a `ToolCall`.
pub fn sanitize_tool_call(
    call: &ToolCall,
    byte_cap: usize,
) -> (String, Option<String>, Option<String>) {
    match call {
        ToolCall::Exec { command } => {
            let summary = truncate_summary(&format!("$ {}", command));
            let bounded = truncate_preview(command, byte_cap);
            (summary, Some(bounded), Some("command: string".to_string()))
        }
        ToolCall::ReadFile { path } => {
            let summary = truncate_summary(&format!("Read {}", path));
            (
                summary,
                Some(path.clone()),
                Some("path: string".to_string()),
            )
        }
        ToolCall::WriteFile { path, content } => {
            let summary = truncate_summary(&format!("Write {}", path));
            let len = content.as_ref().map(|c| c.len()).unwrap_or(0);
            let bounded = format!("Path: {}\nBytes: {}", path, len);
            (
                summary,
                Some(bounded),
                Some("path: string, content: string".to_string()),
            )
        }
        ToolCall::EditFile { path, .. } => {
            let summary = truncate_summary(&format!("Edit {}", path));
            let bounded = format!("Path: {}", path);
            (
                summary,
                Some(bounded),
                Some("path: string, edits: string".to_string()),
            )
        }
        ToolCall::ApplyPatch { path } => {
            let summary = truncate_summary(&format!("Patch {}", path.as_deref().unwrap_or("")));
            (summary, path.clone(), Some("path: string".to_string()))
        }
        ToolCall::WebFetch { url, .. } => {
            let summary = truncate_summary(&format!("Fetch {}", url));
            (summary, Some(url.clone()), Some("url: string".to_string()))
        }
        ToolCall::WebSearch { query } => {
            let summary = truncate_summary(&format!("Search \"{}\"", query));
            (
                summary,
                Some(query.clone()),
                Some("query: string".to_string()),
            )
        }
        ToolCall::Search { pattern, path } => {
            let summary = truncate_summary(&format!("Search \"{}\"", pattern));
            let bounded = format!("Pattern: {}\nPath: {:?}", pattern, path);
            (
                summary,
                Some(bounded),
                Some("pattern: string, path: string".to_string()),
            )
        }
        ToolCall::Glob { pattern } => {
            let summary = truncate_summary(&format!("Glob {}", pattern));
            (
                summary,
                Some(pattern.clone()),
                Some("pattern: string".to_string()),
            )
        }
        ToolCall::Todo { items } => {
            let summary = truncate_summary(&format!("Todo ({} items)", items.len()));
            (summary, None, Some("items: array".to_string()))
        }
        ToolCall::Mcp {
            server,
            tool,
            input,
        } => {
            let summary = truncate_summary(&format!("MCP {}/{}", server, tool));
            let input_str = input.as_ref().map(|v| v.to_string()).unwrap_or_default();
            let bounded = truncate_preview(&input_str, byte_cap);
            (
                summary,
                Some(bounded),
                Some(format!("{}: {}", server, tool)),
            )
        }
        ToolCall::Unknown { name, input } => {
            let summary = truncate_summary(&format!("Tool {}", name));
            let input_str = input.as_ref().map(|v| v.to_string()).unwrap_or_default();
            let bounded = truncate_preview(&input_str, byte_cap);
            (summary, Some(bounded), Some(format!("tool: {}", name)))
        }
    }
}

/// Derive safe summary and preview from a tool result.
///
/// Invariant: Raw tool output is NEVER copied into the sanitized preview or summary.
/// Raw tool output is accessed exclusively via `TrajectoryRawRef` during explicit Raw Reveal.
pub fn sanitize_tool_result(
    output: Option<&str>,
    diff: Option<&ToolDiff>,
    execution: Option<&ToolExecutionMeta>,
    is_error: bool,
    _byte_cap: usize,
) -> (String, Option<String>, Option<i32>) {
    let exit_code = execution.and_then(|e| e.exit_code);
    let summary = if is_error {
        if let Some(code) = exit_code {
            format!("Failed (exit code {})", code)
        } else {
            "Failed".to_string()
        }
    } else if let Some(d) = diff {
        format!("Diff on {}", d.path)
    } else if let Some(code) = exit_code {
        format!("Completed (exit code {})", code)
    } else if let Some(out) = output {
        format!("Completed ({} bytes)", out.len())
    } else {
        "Completed".to_string()
    };

    let preview = if let Some(d) = diff {
        Some(format!(
            "Diff on {}:\n+{} lines, -{} lines",
            d.path,
            d.new_text.lines().count(),
            d.old_text
                .as_deref()
                .map(|s| s.lines().count())
                .unwrap_or(0)
        ))
    } else if is_error {
        Some(format!(
            "Tool execution failed{}",
            exit_code
                .map(|c| format!(" (exit code {})", c))
                .unwrap_or_default()
        ))
    } else {
        output.map(|out| {
            format!(
                "Output: {} bytes{}",
                out.len(),
                exit_code
                    .map(|c| format!(", exit code {}", c))
                    .unwrap_or_default()
            )
        })
    };

    (summary, preview, exit_code)
}

// ---------------------------------------------------------------------------
// Trajectory Record
// ---------------------------------------------------------------------------

/// A single captured technical record in the Trajectory read model.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrajectoryRecord {
    pub id: TrajectoryRecordId,
    pub chat_id: String,
    pub run_id: String,
    pub source_seq: u64,
    pub sub_seq: u32,
    pub lane: TrajectoryLane,
    pub kind: TrajectoryRecordKind,
    pub status: TrajectoryStatus,
    pub is_partial: bool,
    pub title: String,
    pub summary: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub turn_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub step_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub call_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_tool_use_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timing: Option<TrajectoryTiming>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub usage: Option<TrajectoryUsage>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub payload: Option<TrajectoryPayloadPreview>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<TrajectoryResultPreview>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error_message: Option<String>,
    #[serde(default)]
    pub is_degraded: bool,
}

impl TrajectoryRecord {
    pub fn key(&self) -> String {
        self.id.key()
    }
}

// ---------------------------------------------------------------------------
// Run, Turn & Step Grouping
// ---------------------------------------------------------------------------

/// A grouped Trajectory step (e.g. an assistant thought/tool sequence).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrajectoryStep {
    pub step_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub assistant_message_id: Option<String>,
    pub status: TrajectoryStatus,
    pub records: Vec<TrajectoryRecord>,
}

/// A grouped Trajectory turn (user input followed by assistant steps).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrajectoryTurn {
    pub turn_id: String,
    pub run_id: String,
    pub status: TrajectoryStatus,
    pub steps: Vec<TrajectoryStep>,
}

/// A grouped Trajectory run with clear boundaries.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrajectoryRun {
    pub run_id: String,
    pub chat_id: String,
    pub label: String,
    pub is_legacy: bool,
    pub status: TrajectoryStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timing: Option<TrajectoryTiming>,
    pub turns: Vec<TrajectoryTurn>,
}

/// Pure projection: group a flat slice of ordered records into runs, turns, and steps.
pub fn group_records(records: &[TrajectoryRecord]) -> Vec<TrajectoryRun> {
    let mut runs: Vec<TrajectoryRun> = Vec::new();

    for record in records {
        // Find or insert run
        let run_idx = if let Some(pos) = runs.iter().position(|r| r.run_id == record.run_id) {
            pos
        } else {
            let label = if record.run_id.starts_with("legacy") {
                "Legacy Run".to_string()
            } else {
                format!("Run {}", runs.len() + 1)
            };
            runs.push(TrajectoryRun {
                run_id: record.run_id.clone(),
                chat_id: record.chat_id.clone(),
                label,
                is_legacy: record.run_id.starts_with("legacy"),
                status: record.status,
                timing: record.timing.clone(),
                turns: Vec::new(),
            });
            runs.len() - 1
        };

        let run = &mut runs[run_idx];
        if record.status == TrajectoryStatus::Error {
            run.status = TrajectoryStatus::Error;
        } else if record.status == TrajectoryStatus::Interrupted
            && run.status != TrajectoryStatus::Error
        {
            run.status = TrajectoryStatus::Interrupted;
        }
        let turn_id = record
            .turn_id
            .clone()
            .unwrap_or_else(|| format!("{}:t0", record.run_id));
        let turn_idx = if let Some(pos) = run.turns.iter().position(|t| t.turn_id == turn_id) {
            pos
        } else {
            run.turns.push(TrajectoryTurn {
                turn_id: turn_id.clone(),
                run_id: record.run_id.clone(),
                status: record.status,
                steps: Vec::new(),
            });
            run.turns.len() - 1
        };

        let turn = &mut run.turns[turn_idx];
        if record.status == TrajectoryStatus::Error {
            turn.status = TrajectoryStatus::Error;
        } else if record.status == TrajectoryStatus::Interrupted
            && turn.status != TrajectoryStatus::Error
        {
            turn.status = TrajectoryStatus::Interrupted;
        }
        let step_id = record
            .step_id
            .clone()
            .unwrap_or_else(|| format!("{}:s0", turn_id));
        let step_idx = if let Some(pos) = turn.steps.iter().position(|s| s.step_id == step_id) {
            pos
        } else {
            turn.steps.push(TrajectoryStep {
                step_id: step_id.clone(),
                assistant_message_id: None,
                status: record.status,
                records: Vec::new(),
            });
            turn.steps.len() - 1
        };

        let step = &mut turn.steps[step_idx];
        if record.status == TrajectoryStatus::Error {
            step.status = TrajectoryStatus::Error;
        } else if record.status == TrajectoryStatus::Interrupted
            && step.status != TrajectoryStatus::Error
        {
            step.status = TrajectoryStatus::Interrupted;
        }
        step.records.push(record.clone());
    }

    runs
}

// ---------------------------------------------------------------------------
// Reconciliation, Partial-to-Final & Idempotent Deltas
// ---------------------------------------------------------------------------

/// Reconcile a single delta record into an existing ordered record list.
///
/// Replaces any existing record with the same `TrajectoryRecordId` (e.g. coalescing
/// streaming partials into final updates) or inserts in monotonic `(source_seq, sub_seq)` order.
pub fn reconcile_record(records: &mut Vec<TrajectoryRecord>, delta: TrajectoryRecord) {
    if let Some(pos) = records.iter().position(|r| r.id == delta.id) {
        records[pos] = delta;
    } else {
        let insert_pos = records
            .binary_search_by_key(&(delta.source_seq, delta.sub_seq), |r| {
                (r.source_seq, r.sub_seq)
            })
            .unwrap_or_else(|pos| pos);
        records.insert(insert_pos, delta);
    }
}

/// Apply a sequence of delta records idempotently.
pub fn apply_deltas(
    records: &mut Vec<TrajectoryRecord>,
    deltas: impl IntoIterator<Item = TrajectoryRecord>,
) {
    for delta in deltas {
        reconcile_record(records, delta);
    }
}

// ---------------------------------------------------------------------------
// Degraded Interval Marker
// ---------------------------------------------------------------------------

/// An explicitly recorded degraded interval where storage or migration gaps occurred.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrajectoryDegradedInterval {
    pub chat_id: String,
    pub run_id: String,
    pub from_seq: u64,
    pub to_seq: u64,
    pub reason: String,
    pub recorded_at: DateTime<Utc>,
}

// ---------------------------------------------------------------------------
// Unit Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn test_lane_classification() {
        // Scenario 1: Map system, user, context, assistant/model, tool, and subtool records to the correct lane.
        assert_eq!(
            TrajectoryRecordKind::SessionStarted.default_lane(),
            TrajectoryLane::Input
        );
        assert_eq!(
            TrajectoryRecordKind::UserMessage.default_lane(),
            TrajectoryLane::Input
        );
        assert_eq!(
            TrajectoryRecordKind::InputRequested.default_lane(),
            TrajectoryLane::Input
        );
        assert_eq!(
            TrajectoryRecordKind::InputResolved.default_lane(),
            TrajectoryLane::Input
        );
        assert_eq!(
            TrajectoryRecordKind::Steered.default_lane(),
            TrajectoryLane::Input
        );
        assert_eq!(
            TrajectoryRecordKind::ContextUsage.default_lane(),
            TrajectoryLane::Input
        );
        assert_eq!(
            TrajectoryRecordKind::AvailableCommands.default_lane(),
            TrajectoryLane::Input
        );

        assert_eq!(
            TrajectoryRecordKind::AssistantMessage.default_lane(),
            TrajectoryLane::Model
        );
        assert_eq!(
            TrajectoryRecordKind::Reasoning.default_lane(),
            TrajectoryLane::Model
        );
        assert_eq!(
            TrajectoryRecordKind::WorkflowTask.default_lane(),
            TrajectoryLane::Model
        );
        assert_eq!(
            TrajectoryRecordKind::Done.default_lane(),
            TrajectoryLane::Model
        );
        assert_eq!(
            TrajectoryRecordKind::Interrupted.default_lane(),
            TrajectoryLane::Model
        );
        assert_eq!(
            TrajectoryRecordKind::Degraded.default_lane(),
            TrajectoryLane::Model
        );

        assert_eq!(
            TrajectoryRecordKind::ToolCall {
                tool_name: "bash".into()
            }
            .default_lane(),
            TrajectoryLane::Tools
        );
        assert_eq!(
            TrajectoryRecordKind::ToolResult {
                tool_name: "bash".into()
            }
            .default_lane(),
            TrajectoryLane::Tools
        );
        assert_eq!(
            TrajectoryRecordKind::ToolDiff {
                tool_name: "edit".into()
            }
            .default_lane(),
            TrajectoryLane::Tools
        );
    }

    #[test]
    fn test_error_status_precedence() {
        // Scenario 2: Give error status precedence over the semantic color without losing the original kind.
        let status = effective_status(TrajectoryStatus::Completed, true);
        assert_eq!(status, TrajectoryStatus::Error);

        let status_ok = effective_status(TrajectoryStatus::Completed, false);
        assert_eq!(status_ok, TrajectoryStatus::Completed);

        let kind = TrajectoryRecordKind::ToolResult {
            tool_name: "bash".into(),
        };
        assert_eq!(kind.default_lane(), TrajectoryLane::Tools);
    }

    #[test]
    fn test_correlate_tool_call_and_result() {
        // Scenario 3: Correlate tool call and result by stable call identity across interleaved calls.
        let call_id_1 = "call-1";
        let call_id_2 = "call-2";

        let raw_ref_1 = TrajectoryRawRef::new(
            "chat-1",
            10,
            None,
            Some(call_id_1.to_string()),
            TrajectoryRawField::Payload,
        );
        let raw_ref_2 = TrajectoryRawRef::new(
            "chat-1",
            11,
            None,
            Some(call_id_2.to_string()),
            TrajectoryRawField::Payload,
        );
        let raw_res_1 = TrajectoryRawRef::new(
            "chat-1",
            12,
            None,
            Some(call_id_1.to_string()),
            TrajectoryRawField::Result,
        );

        assert_eq!(raw_ref_1.call_id.as_deref(), Some(call_id_1));
        assert_eq!(raw_res_1.call_id.as_deref(), Some(call_id_1));
        assert_ne!(raw_ref_2.call_id.as_deref(), Some(call_id_1));
    }

    #[test]
    fn test_multi_run_grouping_and_explicit_boundaries() {
        // Scenario 4: Preserve two runs for one Chat with an explicit boundary and independent sequence domains.
        let r1 = TrajectoryRecord {
            id: TrajectoryRecordId::new("run-1", 1, 0),
            chat_id: "chat-1".into(),
            run_id: "run-1".into(),
            source_seq: 1,
            sub_seq: 0,
            lane: TrajectoryLane::Input,
            kind: TrajectoryRecordKind::UserMessage,
            status: TrajectoryStatus::Completed,
            is_partial: false,
            title: "User".into(),
            summary: "Hello".into(),
            turn_id: Some("t1".into()),
            step_id: Some("s1".into()),
            call_id: None,
            parent_tool_use_id: None,
            timing: Some(TrajectoryTiming::sequence_only()),
            usage: None,
            payload: None,
            result: None,
            error_message: None,
            is_degraded: false,
        };

        let r2 = TrajectoryRecord {
            id: TrajectoryRecordId::new("run-2", 1, 0),
            chat_id: "chat-1".into(),
            run_id: "run-2".into(),
            source_seq: 1,
            sub_seq: 0,
            lane: TrajectoryLane::Input,
            kind: TrajectoryRecordKind::UserMessage,
            status: TrajectoryStatus::Completed,
            is_partial: false,
            title: "User".into(),
            summary: "Second run".into(),
            turn_id: Some("t2".into()),
            step_id: Some("s2".into()),
            call_id: None,
            parent_tool_use_id: None,
            timing: Some(TrajectoryTiming::sequence_only()),
            usage: None,
            payload: None,
            result: None,
            error_message: None,
            is_degraded: false,
        };

        let groups = group_records(&[r1, r2]);
        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0].run_id, "run-1");
        assert_eq!(groups[0].label, "Run 1");
        assert_eq!(groups[1].run_id, "run-2");
        assert_eq!(groups[1].label, "Run 2");
    }
    #[test]
    fn test_group_records_step_record_collection_and_content() {
        let r1 = TrajectoryRecord {
            id: TrajectoryRecordId::new("run-1", 1, 0),
            chat_id: "chat-1".into(),
            run_id: "run-1".into(),
            source_seq: 1,
            sub_seq: 0,
            lane: TrajectoryLane::Input,
            kind: TrajectoryRecordKind::UserMessage,
            status: TrajectoryStatus::Completed,
            is_partial: false,
            title: "User".into(),
            summary: "Hello".into(),
            turn_id: Some("t1".into()),
            step_id: Some("s1".into()),
            call_id: None,
            parent_tool_use_id: None,
            timing: Some(TrajectoryTiming::sequence_only()),
            usage: None,
            payload: None,
            result: None,
            error_message: None,
            is_degraded: false,
        };
        let r2 = TrajectoryRecord {
            id: TrajectoryRecordId::new("run-1", 2, 0),
            chat_id: "chat-1".into(),
            run_id: "run-1".into(),
            source_seq: 2,
            sub_seq: 0,
            lane: TrajectoryLane::Model,
            kind: TrajectoryRecordKind::AssistantMessage,
            status: TrajectoryStatus::Completed,
            is_partial: false,
            title: "Assistant".into(),
            summary: "Hi there".into(),
            turn_id: Some("t1".into()),
            step_id: Some("s1".into()),
            call_id: None,
            parent_tool_use_id: None,
            timing: Some(TrajectoryTiming::sequence_only()),
            usage: None,
            payload: None,
            result: None,
            error_message: None,
            is_degraded: false,
        };

        let groups = group_records(&[r1.clone(), r2.clone()]);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].turns.len(), 1);
        assert_eq!(groups[0].turns[0].steps.len(), 1);
        let step = &groups[0].turns[0].steps[0];
        assert_eq!(
            step.records.len(),
            2,
            "grouped step must collect all records"
        );
        assert_eq!(step.records[0].id, r1.id);
        assert_eq!(step.records[0].summary, "Hello");
        assert_eq!(step.records[1].id, r2.id);
        assert_eq!(step.records[1].summary, "Hi there");
    }

    #[test]
    fn test_timing_sequence_only_vs_recorded() {
        // Scenario 5: Project a record with no timestamp as sequence-only; formatting returns unavailable rather than 0 ms.
        let seq_timing = TrajectoryTiming::sequence_only();
        assert_eq!(format_duration(Some(&seq_timing)), None);
        assert_eq!(format_duration_or_unavailable(Some(&seq_timing)), "—");
        assert_eq!(format_duration_or_unavailable(None), "—");

        let rec_timing = TrajectoryTiming::recorded(None, None, Some(1500), None);
        assert_eq!(format_duration(Some(&rec_timing)), Some("1.50s".into()));
        assert_eq!(format_duration_or_unavailable(Some(&rec_timing)), "1.50s");

        let start = Utc.with_ymd_and_hms(2026, 9, 1, 12, 0, 0).unwrap();
        let end = Utc.with_ymd_and_hms(2026, 9, 1, 12, 0, 2).unwrap();
        let calc_timing = TrajectoryTiming::recorded(Some(start), Some(end), None, None);
        assert_eq!(format_duration(Some(&calc_timing)), Some("2.00s".into()));
        assert_eq!(format_duration_or_unavailable(Some(&calc_timing)), "2.00s");
    }

    #[test]
    fn test_reconcile_partial_to_final_record() {
        // Scenario 6: Reconcile a partial assistant record into its final record without duplicate ledger rows.
        let mut records = Vec::new();

        let partial = TrajectoryRecord {
            id: TrajectoryRecordId::new("run-1", 5, 0),
            chat_id: "chat-1".into(),
            run_id: "run-1".into(),
            source_seq: 5,
            sub_seq: 0,
            lane: TrajectoryLane::Model,
            kind: TrajectoryRecordKind::AssistantMessage,
            status: TrajectoryStatus::Running,
            is_partial: true,
            title: "Assistant".into(),
            summary: "Streaming partial...".into(),
            turn_id: Some("t1".into()),
            step_id: Some("s1".into()),
            call_id: None,
            parent_tool_use_id: None,
            timing: None,
            usage: None,
            payload: None,
            result: None,
            error_message: None,
            is_degraded: false,
        };

        reconcile_record(&mut records, partial);
        assert_eq!(records.len(), 1);
        assert!(records[0].is_partial);
        assert_eq!(records[0].summary, "Streaming partial...");

        let final_record = TrajectoryRecord {
            id: TrajectoryRecordId::new("run-1", 5, 0),
            chat_id: "chat-1".into(),
            run_id: "run-1".into(),
            source_seq: 5,
            sub_seq: 0,
            lane: TrajectoryLane::Model,
            kind: TrajectoryRecordKind::AssistantMessage,
            status: TrajectoryStatus::Completed,
            is_partial: false,
            title: "Assistant".into(),
            summary: "Completed final response.".into(),
            turn_id: Some("t1".into()),
            step_id: Some("s1".into()),
            call_id: None,
            parent_tool_use_id: None,
            timing: None,
            usage: None,
            payload: None,
            result: None,
            error_message: None,
            is_degraded: false,
        };

        reconcile_record(&mut records, final_record);
        assert_eq!(records.len(), 1);
        assert!(!records[0].is_partial);
        assert_eq!(records[0].summary, "Completed final response.");
    }

    #[test]
    fn test_idempotent_deltas() {
        // Scenario 7: Apply the same delta twice and keep one record.
        let mut records = Vec::new();
        let record = TrajectoryRecord {
            id: TrajectoryRecordId::new("run-1", 1, 0),
            chat_id: "chat-1".into(),
            run_id: "run-1".into(),
            source_seq: 1,
            sub_seq: 0,
            lane: TrajectoryLane::Input,
            kind: TrajectoryRecordKind::UserMessage,
            status: TrajectoryStatus::Completed,
            is_partial: false,
            title: "User".into(),
            summary: "Hello".into(),
            turn_id: None,
            step_id: None,
            call_id: None,
            parent_tool_use_id: None,
            timing: None,
            usage: None,
            payload: None,
            result: None,
            error_message: None,
            is_degraded: false,
        };

        apply_deltas(&mut records, vec![record.clone(), record]);
        assert_eq!(records.len(), 1);
    }

    #[test]
    fn test_sanitization_budgets() {
        // Scenario 8: Sanitize file writes, tool inputs, and tool results within the existing preview budgets while retaining schema/status metadata.
        let long_content = "a".repeat(5000);
        let write_call = ToolCall::WriteFile {
            path: "/path/to/sensitive/secret.env".into(),
            content: Some(long_content),
        };

        let (summary, preview, schema) = sanitize_tool_call(&write_call, 1024);
        assert_eq!(summary, "Write /path/to/sensitive/secret.env");
        assert!(preview.unwrap().contains("Bytes: 5000"));
        assert_eq!(schema.as_deref(), Some("path: string, content: string"));

        let secret_output = "Secret output: ghp_123456789012345678901234567890123456";
        let (res_sum, res_prev, exit) =
            sanitize_tool_result(Some(secret_output), None, None, false, 500);
        assert_eq!(res_sum, "Completed (55 bytes)");
        assert_eq!(res_prev, Some("Output: 55 bytes".to_string()));
        assert_eq!(exit, None);
        assert!(!res_sum.contains("ghp_"));
        assert!(!res_prev.as_ref().unwrap().contains("ghp_"));

        let (err_sum, err_prev, err_exit) = sanitize_tool_result(
            Some(secret_output),
            None,
            Some(&ToolExecutionMeta {
                exit_code: Some(1),
                duration_ms: None,
            }),
            true,
            500,
        );
        assert_eq!(err_sum, "Failed (exit code 1)");
        assert_eq!(
            err_prev,
            Some("Tool execution failed (exit code 1)".to_string())
        );
        assert_eq!(err_exit, Some(1));
        assert!(!err_sum.contains("ghp_"));
        assert!(!err_prev.as_ref().unwrap().contains("ghp_"));
    }

    #[test]
    fn test_multibyte_prompt_sanitization_boundary() {
        // Multibyte characters (e.g. 3-byte Japanese kanji, 4-byte emojis) crossing the 256-byte summary cap
        // must not panic with slice indexing errors.
        let kanji_text =
            "日本語テスト文字列で256バイトの境界線を越えるテストケースを作成します。".repeat(10);
        let (summary, preview) = sanitize_prompt_preview(&kanji_text, 1024);
        assert!(summary.ends_with('…'));
        assert!(summary.len() <= MAX_SUMMARY_LEN + '…'.len_utf8());
        assert!(preview.is_some());

        let emoji_text = "🚀🦀✨🔥🎉".repeat(30);
        let (emoji_summary, _) = sanitize_prompt_preview(&emoji_text, 500);
        assert!(emoji_summary.ends_with('…'));
    }
    #[test]
    fn test_multibyte_tool_call_summary_bounds_and_no_panic() {
        let long_cmd = "echo 🚀🦀✨🔥🎉".repeat(50);
        let exec_call = ToolCall::Exec { command: long_cmd };
        let (summary, _, _) = sanitize_tool_call(&exec_call, 1024);
        assert!(summary.ends_with('…'));
        assert!(summary.len() <= MAX_SUMMARY_LEN + '…'.len_utf8() + 4);

        let long_path = "日本語パスディレクトリ名/".repeat(30) + "ファイル.rs";
        let read_call = ToolCall::ReadFile { path: long_path };
        let (summary, _, _) = sanitize_tool_call(&read_call, 1024);
        assert!(summary.ends_with('…'));
        assert!(summary.len() <= MAX_SUMMARY_LEN + '…'.len_utf8() + 4);

        let long_query = "🔍検索クエリ".repeat(50);
        let search_call = ToolCall::WebSearch { query: long_query };
        let (summary, _, _) = sanitize_tool_call(&search_call, 1024);
        assert!(summary.ends_with('…'));
        assert!(summary.len() <= MAX_SUMMARY_LEN + '…'.len_utf8() + 4);
    }
}
