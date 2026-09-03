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

pub const CURRENT_RAW_SOURCE_VERSION: u32 = 1;

fn default_raw_source_version() -> u32 {
    1
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
    #[serde(default = "default_raw_source_version")]
    pub source_version: u32,
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
            source_version: CURRENT_RAW_SOURCE_VERSION,
        }
    }

    pub fn with_version(mut self, version: u32) -> Self {
        self.source_version = version;
        self
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
    #[serde(other)]
    Unknown,
}

impl TrajectoryLane {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Input => "input",
            Self::Model => "model",
            Self::Tools => "tools",
            Self::Unknown => "unknown",
        }
    }
}

/// Semantic classification of a Trajectory record.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
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

#[derive(Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
enum KnownTrajectoryRecordKind {
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
}

#[derive(Deserialize)]
struct RawKindFallback {
    kind: String,
    #[serde(default)]
    name: Option<String>,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum TrajectoryRecordKindDe {
    Known(KnownTrajectoryRecordKind),
    Custom(RawKindFallback),
}

impl Serialize for TrajectoryRecordKind {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        match self {
            Self::SessionStarted => KnownTrajectoryRecordKind::SessionStarted.serialize(serializer),
            Self::UserMessage => KnownTrajectoryRecordKind::UserMessage.serialize(serializer),
            Self::InputRequested => KnownTrajectoryRecordKind::InputRequested.serialize(serializer),
            Self::InputResolved => KnownTrajectoryRecordKind::InputResolved.serialize(serializer),
            Self::Steered => KnownTrajectoryRecordKind::Steered.serialize(serializer),
            Self::ContextUsage => KnownTrajectoryRecordKind::ContextUsage.serialize(serializer),
            Self::AvailableCommands => {
                KnownTrajectoryRecordKind::AvailableCommands.serialize(serializer)
            }
            Self::AssistantMessage => {
                KnownTrajectoryRecordKind::AssistantMessage.serialize(serializer)
            }
            Self::Reasoning => KnownTrajectoryRecordKind::Reasoning.serialize(serializer),
            Self::WorkflowTask => KnownTrajectoryRecordKind::WorkflowTask.serialize(serializer),
            Self::ToolCall { tool_name } => KnownTrajectoryRecordKind::ToolCall {
                tool_name: tool_name.clone(),
            }
            .serialize(serializer),
            Self::ToolResult { tool_name } => KnownTrajectoryRecordKind::ToolResult {
                tool_name: tool_name.clone(),
            }
            .serialize(serializer),
            Self::ToolDiff { tool_name } => KnownTrajectoryRecordKind::ToolDiff {
                tool_name: tool_name.clone(),
            }
            .serialize(serializer),
            Self::Error => KnownTrajectoryRecordKind::Error.serialize(serializer),
            Self::Done => KnownTrajectoryRecordKind::Done.serialize(serializer),
            Self::Interrupted => KnownTrajectoryRecordKind::Interrupted.serialize(serializer),
            Self::Degraded => KnownTrajectoryRecordKind::Degraded.serialize(serializer),
            Self::Custom { name } => {
                #[derive(Serialize)]
                struct CustomKindSer<'a> {
                    kind: &'a str,
                }
                CustomKindSer {
                    kind: name.as_str(),
                }
                .serialize(serializer)
            }
        }
    }
}

impl<'de> Deserialize<'de> for TrajectoryRecordKind {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let helper = TrajectoryRecordKindDe::deserialize(deserializer)?;
        Ok(match helper {
            TrajectoryRecordKindDe::Known(known) => match known {
                KnownTrajectoryRecordKind::SessionStarted => Self::SessionStarted,
                KnownTrajectoryRecordKind::UserMessage => Self::UserMessage,
                KnownTrajectoryRecordKind::InputRequested => Self::InputRequested,
                KnownTrajectoryRecordKind::InputResolved => Self::InputResolved,
                KnownTrajectoryRecordKind::Steered => Self::Steered,
                KnownTrajectoryRecordKind::ContextUsage => Self::ContextUsage,
                KnownTrajectoryRecordKind::AvailableCommands => Self::AvailableCommands,
                KnownTrajectoryRecordKind::AssistantMessage => Self::AssistantMessage,
                KnownTrajectoryRecordKind::Reasoning => Self::Reasoning,
                KnownTrajectoryRecordKind::WorkflowTask => Self::WorkflowTask,
                KnownTrajectoryRecordKind::ToolCall { tool_name } => Self::ToolCall { tool_name },
                KnownTrajectoryRecordKind::ToolResult { tool_name } => {
                    Self::ToolResult { tool_name }
                }
                KnownTrajectoryRecordKind::ToolDiff { tool_name } => Self::ToolDiff { tool_name },
                KnownTrajectoryRecordKind::Error => Self::Error,
                KnownTrajectoryRecordKind::Done => Self::Done,
                KnownTrajectoryRecordKind::Interrupted => Self::Interrupted,
                KnownTrajectoryRecordKind::Degraded => Self::Degraded,
            },
            TrajectoryRecordKindDe::Custom(fallback) => {
                let name = if fallback.kind == "custom" && fallback.name.is_some() {
                    fallback.name.unwrap()
                } else {
                    fallback.kind
                };
                Self::Custom { name }
            }
        })
    }
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
    #[serde(other)]
    Unknown,
}

impl TrajectoryStatus {
    pub fn is_error(&self) -> bool {
        matches!(self, Self::Error)
    }

    pub fn is_terminal(&self) -> bool {
        matches!(self, Self::Completed | Self::Error | Self::Interrupted)
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
    let ms = timing?.effective_duration_ms()?;
    Some(format_duration_ms(ms))
}

/// Helper for raw millisecond values.
pub fn format_duration_ms(ms: u64) -> String {
    if ms < 1_000 {
        return format!("{ms}ms");
    }

    let tenths = (ms + 50) / 100;
    if tenths < 600 {
        if tenths.is_multiple_of(10) {
            format!("{}s", tenths / 10)
        } else {
            format!("{}.{}s", tenths / 10, tenths % 10)
        }
    } else {
        let rounded_secs = (ms + 500) / 1_000;
        format!("{}m {}s", rounded_secs / 60, rounded_secs % 60)
    }
}

/// Format duration or return a fixed unavailable placeholder ("—").
pub fn format_duration_or_unavailable(timing: Option<&TrajectoryTiming>) -> String {
    format_duration(timing).unwrap_or_else(|| "—".to_string())
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

const REDACTED: &str = "[REDACTED]";

fn is_secret_char(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-')
}

fn has_token_boundary(bytes: &[u8], start: usize) -> bool {
    start == 0 || !bytes[start - 1].is_ascii_alphanumeric()
}

fn starts_with_ignore_ascii_case(bytes: &[u8], start: usize, needle: &[u8]) -> bool {
    bytes
        .get(start..start.saturating_add(needle.len()))
        .is_some_and(|candidate| candidate.eq_ignore_ascii_case(needle))
}

fn secret_value_end(bytes: &[u8], start: usize, end_at_line: bool) -> usize {
    if let Some(quote @ (b'\'' | b'"')) = bytes.get(start).copied() {
        return bytes[start + 1..]
            .iter()
            .position(|byte| *byte == quote)
            .map_or(bytes.len(), |offset| start + offset + 2);
    }

    bytes[start..]
        .iter()
        .position(|byte| {
            if end_at_line {
                matches!(byte, b'\r' | b'\n' | b'\'' | b'"')
            } else {
                byte.is_ascii_whitespace() || matches!(byte, b'&' | b';' | b',' | b'\'' | b'"')
            }
        })
        .map_or(bytes.len(), |offset| start + offset)
}

fn skip_ascii_whitespace(bytes: &[u8], mut index: usize) -> usize {
    while bytes.get(index).is_some_and(u8::is_ascii_whitespace) {
        index += 1;
    }
    index
}

/// Range of a `<key><separator><value>` secret, or `None` when the shape does not match.
fn separated_secret_range(
    bytes: &[u8],
    start: usize,
    key: &[u8],
    separator: u8,
    end_at_line: bool,
) -> Option<(usize, usize)> {
    if !starts_with_ignore_ascii_case(bytes, start, key) {
        return None;
    }
    let mut separator_at = start + key.len();
    if bytes.get(separator_at).is_some_and(|quote| {
        matches!(quote, b'\'' | b'"')
            && start.checked_sub(1).and_then(|index| bytes.get(index)) == Some(quote)
    }) {
        separator_at += 1;
    }
    let separator_at = skip_ascii_whitespace(bytes, separator_at);
    if bytes.get(separator_at) != Some(&separator) {
        return None;
    }
    let value_start = skip_ascii_whitespace(bytes, separator_at + 1);
    if value_start >= bytes.len() {
        return None;
    }
    Some((
        value_start,
        secret_value_end(bytes, value_start, end_at_line),
    ))
}

fn labeled_secret_range(bytes: &[u8], start: usize) -> Option<(usize, usize)> {
    if !has_token_boundary(bytes, start) {
        return None;
    }

    if let Some(range) = separated_secret_range(bytes, start, b"authorization", b':', true) {
        return Some(range);
    }

    if starts_with_ignore_ascii_case(bytes, start, b"bearer") {
        let after_keyword = start + "bearer".len();
        if bytes
            .get(after_keyword)
            .is_some_and(u8::is_ascii_whitespace)
        {
            let value_start = skip_ascii_whitespace(bytes, after_keyword);
            if value_start < bytes.len() {
                return Some((value_start, secret_value_end(bytes, value_start, false)));
            }
        }
    }

    for key in [
        b"password".as_slice(),
        b"token",
        b"apikey",
        b"api_key",
        b"api-key",
    ] {
        for separator in [b'=', b':'] {
            if let Some(range) = separated_secret_range(bytes, start, key, separator, false) {
                return Some(range);
            }
        }
    }

    None
}

/// End of the run of secret-shaped characters starting at `from`.
fn secret_run_end(bytes: &[u8], from: usize) -> usize {
    from + bytes[from..]
        .iter()
        .take_while(|byte| is_secret_char(**byte))
        .count()
}

fn token_secret_end(bytes: &[u8], start: usize) -> Option<usize> {
    if !has_token_boundary(bytes, start) {
        return None;
    }

    let github_prefix = [b"ghp_".as_slice(), b"gho_", b"github_pat_", b"sk-"]
        .into_iter()
        .find(|prefix| bytes[start..].starts_with(prefix));
    if let Some(prefix) = github_prefix {
        let value_start = start + prefix.len();
        let end = secret_run_end(bytes, value_start);
        return (end > value_start).then_some(end);
    }

    let is_slack_prefix = bytes.get(start..start + 5).is_some_and(|candidate| {
        candidate.starts_with(b"xox")
            && matches!(candidate[3], b'a' | b'b' | b'p' | b'r' | b's')
            && candidate[4] == b'-'
    });
    if is_slack_prefix {
        let value_start = start + 5;
        let end = secret_run_end(bytes, value_start);
        return (end > value_start).then_some(end);
    }

    if bytes[start..].starts_with(b"AKIA") {
        let end = start
            + bytes[start..]
                .iter()
                .take_while(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit())
                .count();
        if end - start >= 20 {
            return Some(end);
        }
    }

    let opaque_end = secret_run_end(bytes, start);
    (opaque_end - start >= 32).then_some(opaque_end)
}

fn is_sensitive_query_key(key: &str) -> bool {
    const SENSITIVE_KEYS: &[&str] = &[
        "key",
        "sig",
        "signature",
        "secret",
        "code",
        "token",
        "apikey",
        "api_key",
        "api-key",
        "auth",
        "authorization",
        "access_token",
        "password",
        "pwd",
        "credential",
        "credentials",
        "session",
        "jwt",
        "private_key",
    ];
    SENSITIVE_KEYS
        .iter()
        .any(|sensitive| key.eq_ignore_ascii_case(sensitive))
}

fn sanitize_url_str(url_str: &str) -> String {
    let (scheme, rest) = if let Some(rest) = url_str.strip_prefix("https://") {
        ("https://", rest)
    } else if let Some(rest) = url_str.strip_prefix("http://") {
        ("http://", rest)
    } else if let Some(rest) = url_str.strip_prefix("wss://") {
        ("wss://", rest)
    } else if let Some(rest) = url_str.strip_prefix("ws://") {
        ("ws://", rest)
    } else if let Some(rest) = url_str.strip_prefix("ftp://") {
        ("ftp://", rest)
    } else {
        return url_str.to_string();
    };

    let (before_query, query_and_frag) = match rest.find('?') {
        Some(idx) => (&rest[..idx], Some(&rest[idx + 1..])),
        None => (rest, None),
    };

    let authority_end = before_query.find('/').unwrap_or(before_query.len());
    let authority = &before_query[..authority_end];
    let path_and_rest = &before_query[authority_end..];

    let sanitized_authority = if let Some(at_idx) = authority.find('@') {
        &authority[at_idx + 1..]
    } else {
        authority
    };

    let mut result = String::with_capacity(url_str.len());
    result.push_str(scheme);
    result.push_str(sanitized_authority);
    result.push_str(path_and_rest);

    if let Some(q_and_f) = query_and_frag {
        result.push('?');
        let (query, fragment) = match q_and_f.find('#') {
            Some(f_idx) => (&q_and_f[..f_idx], Some(&q_and_f[f_idx..])),
            None => (q_and_f, None),
        };

        let mut first = true;
        for param in query.split('&') {
            if !first {
                result.push('&');
            }
            first = false;

            if let Some((k, _v)) = param.split_once('=') {
                if is_sensitive_query_key(k) {
                    result.push_str(k);
                    result.push('=');
                    result.push_str(REDACTED);
                } else {
                    result.push_str(param);
                }
            } else if is_sensitive_query_key(param) {
                result.push_str(param);
                result.push('=');
                result.push_str(REDACTED);
            } else {
                result.push_str(param);
            }
        }

        if let Some(frag) = fragment {
            result.push('#');
            result.push_str(frag);
        }
    }

    result
}

fn redact_urls(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut result = String::with_capacity(text.len());
    let mut cursor = 0;
    let schemes = ["https://", "http://", "wss://", "ws://", "ftp://"];

    while cursor < bytes.len() {
        let matched_scheme = schemes.iter().find(|scheme| {
            text[cursor..].starts_with(*scheme)
                && (cursor == 0 || !bytes[cursor - 1].is_ascii_alphanumeric())
        });

        if let Some(scheme) = matched_scheme {
            let start = cursor;
            let mut end = start + scheme.len();
            while end < bytes.len() {
                let b = bytes[end];
                if b.is_ascii_whitespace() || matches!(b, b'"' | b'\'' | b'<' | b'>' | b'`') {
                    break;
                }
                end += 1;
            }

            while end > start + scheme.len()
                && matches!(bytes[end - 1], b'.' | b',' | b';' | b')' | b']')
            {
                end -= 1;
            }

            let url_candidate = &text[start..end];
            let sanitized = sanitize_url_str(url_candidate);
            result.push_str(&sanitized);
            cursor = end;
        } else {
            let ch = text[cursor..].chars().next().unwrap();
            result.push(ch);
            cursor += ch.len_utf8();
        }
    }

    result
}

fn redact_secrets(text: &str) -> String {
    let url_sanitized = redact_urls(text);
    let text = &url_sanitized;
    let bytes = text.as_bytes();
    let mut redacted = String::with_capacity(text.len());
    let mut copy_from = 0;
    let mut cursor = 0;

    while cursor < bytes.len() {
        let range = labeled_secret_range(bytes, cursor)
            .or_else(|| token_secret_end(bytes, cursor).map(|end| (cursor, end)));
        if let Some((secret_start, secret_end)) = range {
            redacted.push_str(&text[copy_from..secret_start]);
            redacted.push_str(REDACTED);
            cursor = secret_end;
            copy_from = secret_end;
        } else {
            cursor += text[cursor..].chars().next().unwrap().len_utf8();
        }
    }

    redacted.push_str(&text[copy_from..]);
    redacted
}

fn sanitized_summary(text: &str) -> String {
    truncate_summary(&redact_secrets(text))
}

fn sanitized_preview(text: &str, byte_cap: usize) -> String {
    truncate_preview(&redact_secrets(text), byte_cap)
}

fn tool_input_metadata(input: Option<&serde_json::Value>) -> String {
    let mut keys = input
        .and_then(serde_json::Value::as_object)
        .map(|object| object.keys().map(String::as_str).collect::<Vec<_>>())
        .unwrap_or_default();
    keys.sort_unstable();
    let names = if keys.is_empty() {
        "none".to_string()
    } else {
        keys.join(", ")
    };
    if let Some(value) = input {
        let byte_count = value.to_string().len();
        format!("args: {names} ({byte_count} bytes)")
    } else {
        format!("args: {names} (size unavailable)")
    }
}

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
    let redacted = redact_secrets(text);
    let single_line = redacted.split_whitespace().collect::<Vec<_>>().join(" ");
    (
        truncate_summary(&single_line),
        Some(truncate_preview(&redacted, byte_cap)),
    )
}

/// Derive safe summary and bounded preview from a `ToolCall`.
pub fn sanitize_tool_call(
    call: &ToolCall,
    byte_cap: usize,
) -> (String, Option<String>, Option<String>) {
    match call {
        ToolCall::Exec { command } => (
            sanitized_summary(&format!("$ {command}")),
            Some(sanitized_preview(command, byte_cap)),
            Some("command: string".to_string()),
        ),
        ToolCall::ReadFile { path } => (
            sanitized_summary(&format!("Read {path}")),
            Some(sanitized_preview(path, byte_cap)),
            Some("path: string".to_string()),
        ),
        ToolCall::WriteFile { path, content } => {
            let text = if let Some(content) = content.as_ref() {
                format!("Path: {path}\nBytes: {}", content.len())
            } else {
                format!("Path: {path}\nBytes: unavailable")
            };
            (
                sanitized_summary(&format!("Write {path}")),
                Some(sanitized_preview(&text, byte_cap)),
                Some("path: string, content: string".to_string()),
            )
        }
        ToolCall::EditFile { path, .. } => (
            sanitized_summary(&format!("Edit {path}")),
            Some(sanitized_preview(&format!("Path: {path}"), byte_cap)),
            Some("path: string, edits: string".to_string()),
        ),
        ToolCall::ApplyPatch { path } => (
            sanitized_summary(&format!("Patch {}", path.as_deref().unwrap_or(""))),
            path.as_deref()
                .map(|path| sanitized_preview(path, byte_cap)),
            Some("path: string".to_string()),
        ),
        ToolCall::WebFetch { url, .. } => (
            sanitized_summary(&format!("Fetch {url}")),
            Some(sanitized_preview(url, byte_cap)),
            Some("url: string".to_string()),
        ),
        ToolCall::WebSearch { query } => (
            sanitized_summary(&format!("Search \"{query}\"")),
            Some(sanitized_preview(query, byte_cap)),
            Some("query: string".to_string()),
        ),
        ToolCall::Search { pattern, path } => (
            sanitized_summary(&format!("Search \"{pattern}\"")),
            Some(sanitized_preview(
                &format!("Pattern: {pattern}\nPath: {path:?}"),
                byte_cap,
            )),
            Some("pattern: string, path: string".to_string()),
        ),
        ToolCall::Glob { pattern } => (
            sanitized_summary(&format!("Glob {pattern}")),
            Some(sanitized_preview(pattern, byte_cap)),
            Some("pattern: string".to_string()),
        ),
        ToolCall::Todo { items } => (
            sanitized_summary(&format!("Todo ({} items)", items.len())),
            None,
            Some("items: array".to_string()),
        ),
        ToolCall::Mcp {
            server,
            tool,
            input,
        } => (
            sanitized_summary(&format!("MCP {server}/{tool}")),
            Some(sanitized_preview(
                &tool_input_metadata(input.as_ref()),
                byte_cap,
            )),
            Some(redact_secrets(&format!("{server}: {tool}"))),
        ),
        ToolCall::Unknown { name, input } => (
            sanitized_summary(&format!("Tool {name}")),
            Some(sanitized_preview(
                &tool_input_metadata(input.as_ref()),
                byte_cap,
            )),
            Some(redact_secrets(&format!("tool: {name}"))),
        ),
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
    let exit_code = execution.and_then(|meta| meta.exit_code);

    let summary = if is_error {
        match exit_code {
            Some(code) => format!("Failed (exit code {code})"),
            None => "Failed".to_string(),
        }
    } else if let Some(diff) = diff {
        format!("Diff on {}", diff.path)
    } else if let Some(code) = exit_code {
        format!("Completed (exit code {code})")
    } else if let Some(output) = output {
        format!("Completed ({} bytes)", output.len())
    } else {
        "Completed".to_string()
    };

    let preview = if is_error {
        Some(format!(
            "Tool execution failed{}",
            exit_code
                .map(|code| format!(" (exit code {code})"))
                .unwrap_or_default()
        ))
    } else if let Some(diff) = diff {
        Some(format!(
            "Diff on {}:\n+{} lines, -{} lines",
            diff.path,
            diff.new_text.lines().count(),
            diff.old_text
                .as_deref()
                .map_or(0, |text| text.lines().count())
        ))
    } else {
        output.map(|output| {
            format!(
                "Output: {} bytes{}",
                output.len(),
                exit_code
                    .map(|code| format!(", exit code {code}"))
                    .unwrap_or_default()
            )
        })
    };
    (
        sanitized_summary(&summary),
        preview.map(|preview| sanitized_preview(&preview, _byte_cap)),
        exit_code,
    )
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

    /// Derived read-model status with a failed result taking precedence.
    pub fn effective_status(&self) -> TrajectoryStatus {
        if self.result.as_ref().is_some_and(|result| result.is_error) {
            TrajectoryStatus::Error
        } else {
            self.status
        }
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

fn status_precedence(status: TrajectoryStatus) -> u8 {
    match status {
        TrajectoryStatus::Error => 6,
        TrajectoryStatus::Interrupted => 5,
        TrajectoryStatus::Unsettled => 4,
        TrajectoryStatus::Running => 3,
        TrajectoryStatus::Degraded => 2,
        TrajectoryStatus::Completed => 1,
        TrajectoryStatus::Unknown => 0,
    }
}

fn fold_status(
    current: TrajectoryStatus,
    incoming: TrajectoryStatus,
    kind: &TrajectoryRecordKind,
) -> TrajectoryStatus {
    if matches!(kind, TrajectoryRecordKind::Done)
        && incoming == TrajectoryStatus::Completed
        && current == TrajectoryStatus::Running
    {
        return TrajectoryStatus::Completed;
    }

    if status_precedence(incoming) > status_precedence(current) {
        incoming
    } else {
        current
    }
}

fn aggregate_run_timing(run: &TrajectoryRun) -> Option<TrajectoryTiming> {
    if !run.status.is_terminal() {
        return None;
    }

    let mut first_start = None;
    let mut last_end = None;

    for record in run
        .turns
        .iter()
        .flat_map(|turn| &turn.steps)
        .flat_map(|step| &step.records)
    {
        let Some(timing) = &record.timing else {
            return None;
        };
        if timing.mode == TrajectoryTimingMode::SequenceOnly {
            return None;
        }
        if let Some(started) = timing.started_at {
            first_start =
                Some(first_start.map_or(started, |prev: DateTime<Utc>| prev.min(started)));
        }
        if let Some(ended) = timing.ended_at {
            last_end = Some(last_end.map_or(ended, |prev: DateTime<Utc>| prev.max(ended)));
        }
    }

    Some(TrajectoryTiming::recorded(
        Some(first_start?),
        Some(last_end?),
        None,
        None,
    ))
}

/// Pure projection: group a flat slice of ordered records into runs, turns, and steps.
pub fn group_records(records: &[TrajectoryRecord]) -> Vec<TrajectoryRun> {
    let mut runs: Vec<TrajectoryRun> = Vec::new();

    for record in records {
        // Find or insert run
        let run_idx = if let Some(pos) = runs.iter().position(|r| r.run_id == record.run_id) {
            pos
        } else {
            let is_legacy = record.run_id.starts_with("legacy");
            let label = if is_legacy {
                "Legacy Run".to_string()
            } else {
                let non_legacy_count = runs.iter().filter(|r| !r.is_legacy).count();
                format!("Run {}", non_legacy_count + 1)
            };
            runs.push(TrajectoryRun {
                run_id: record.run_id.clone(),
                chat_id: record.chat_id.clone(),
                label,
                is_legacy,
                status: record.effective_status(),
                timing: None,
                turns: Vec::new(),
            });
            runs.len() - 1
        };

        let run = &mut runs[run_idx];
        let effective_status = record.effective_status();
        run.status = fold_status(run.status, effective_status, &record.kind);
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
                status: effective_status,
                steps: Vec::new(),
            });
            run.turns.len() - 1
        };

        let turn = &mut run.turns[turn_idx];
        turn.status = fold_status(turn.status, effective_status, &record.kind);
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
                status: effective_status,
                records: Vec::new(),
            });
            turn.steps.len() - 1
        };

        let step = &mut turn.steps[step_idx];
        step.status = fold_status(step.status, effective_status, &record.kind);
        step.records.push(record.clone());
    }

    for run in &mut runs {
        run.timing = aggregate_run_timing(run);
    }

    runs
}

// ---------------------------------------------------------------------------
// Reconciliation, Partial-to-Final & Idempotent Deltas
// ---------------------------------------------------------------------------

/// Chronological key for a record within one Chat: `source_seq` is the Chat's
/// run-journal sequence, monotonic across runs, so it orders runs by arrival
/// rather than by the lexical accident of their ids.
pub fn stream_order_key(id: &TrajectoryRecordId) -> (u64, u32, &str) {
    (id.source_seq, id.sub_seq, id.run_id.as_str())
}

/// Reconcile a single delta record into an existing ordered record list.
///
/// `records` must already be sorted by [`stream_order_key`], the same
/// `(source_seq, sub_seq)` order the store serves snapshots in.
/// Replaces an existing partial with the same `TrajectoryRecordId` or inserts at
/// the sorted position. A re-delivered partial never replaces a stored final.
pub fn reconcile_record(records: &mut Vec<TrajectoryRecord>, delta: TrajectoryRecord) {
    match records.binary_search_by_key(&stream_order_key(&delta.id), |r| stream_order_key(&r.id)) {
        Ok(pos) => {
            if records[pos].is_partial || !delta.is_partial {
                records[pos] = delta;
            }
        }
        Err(pos) => records.insert(pos, delta),
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

    fn test_record(
        source_seq: u64,
        kind: TrajectoryRecordKind,
        status: TrajectoryStatus,
    ) -> TrajectoryRecord {
        TrajectoryRecord {
            id: TrajectoryRecordId::new("run-1", source_seq, 0),
            chat_id: "chat-1".into(),
            run_id: "run-1".into(),
            source_seq,
            sub_seq: 0,
            lane: kind.default_lane(),
            kind,
            status,
            is_partial: false,
            title: "Test".into(),
            summary: "Test record".into(),
            turn_id: Some("turn-1".into()),
            step_id: Some("step-1".into()),
            call_id: None,
            parent_tool_use_id: None,
            timing: None,
            usage: None,
            payload: None,
            result: None,
            error_message: None,
            is_degraded: false,
        }
    }

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
    fn result_error_overrides_completed_status_in_record_and_groups() {
        let mut record = test_record(
            1,
            TrajectoryRecordKind::ToolResult {
                tool_name: "bash".into(),
            },
            TrajectoryStatus::Completed,
        );
        record.result = Some(TrajectoryResultPreview {
            summary: "Failed".into(),
            sanitized_text: None,
            is_error: true,
            exit_code: Some(1),
            raw_ref: None,
        });

        assert_eq!(record.effective_status(), TrajectoryStatus::Error);
        let groups = group_records(&[record]);
        assert_eq!(groups[0].status, TrajectoryStatus::Error);
        assert_eq!(groups[0].turns[0].status, TrajectoryStatus::Error);
        assert_eq!(groups[0].turns[0].steps[0].status, TrajectoryStatus::Error);
    }

    #[test]
    fn interleaved_tool_calls_and_results_remain_correlated_when_grouped() {
        let mut call_1 = test_record(
            1,
            TrajectoryRecordKind::ToolCall {
                tool_name: "read".into(),
            },
            TrajectoryStatus::Completed,
        );
        call_1.call_id = Some("call-1".into());
        let mut call_2 = test_record(
            2,
            TrajectoryRecordKind::ToolCall {
                tool_name: "exec".into(),
            },
            TrajectoryStatus::Completed,
        );
        call_2.call_id = Some("call-2".into());
        let mut result_1 = test_record(
            3,
            TrajectoryRecordKind::ToolResult {
                tool_name: "read".into(),
            },
            TrajectoryStatus::Completed,
        );
        result_1.call_id = Some("call-1".into());
        let mut result_2 = test_record(
            4,
            TrajectoryRecordKind::ToolResult {
                tool_name: "exec".into(),
            },
            TrajectoryStatus::Completed,
        );
        result_2.call_id = Some("call-2".into());

        let groups = group_records(&[call_1, call_2, result_1, result_2]);
        let records = &groups[0].turns[0].steps[0].records;
        assert_eq!(
            records
                .iter()
                .map(|record| record.call_id.as_deref())
                .collect::<Vec<_>>(),
            [
                Some("call-1"),
                Some("call-2"),
                Some("call-1"),
                Some("call-2")
            ]
        );
        assert!(matches!(
            records[0].kind,
            TrajectoryRecordKind::ToolCall { .. }
        ));
        assert!(matches!(
            records[2].kind,
            TrajectoryRecordKind::ToolResult { .. }
        ));
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
    fn done_record_completes_a_run_that_started_running() {
        let running = test_record(
            1,
            TrajectoryRecordKind::AssistantMessage,
            TrajectoryStatus::Running,
        );
        let done = test_record(2, TrajectoryRecordKind::Done, TrajectoryStatus::Completed);

        let groups = group_records(&[running, done]);
        assert_eq!(groups[0].status, TrajectoryStatus::Completed);
        assert_eq!(groups[0].turns[0].status, TrajectoryStatus::Completed);
        assert_eq!(
            groups[0].turns[0].steps[0].status,
            TrajectoryStatus::Completed
        );
    }

    #[test]
    fn group_status_fold_obeys_full_precedence() {
        let precedence_pairs = [
            (TrajectoryStatus::Completed, TrajectoryStatus::Degraded),
            (TrajectoryStatus::Degraded, TrajectoryStatus::Running),
            (TrajectoryStatus::Running, TrajectoryStatus::Unsettled),
            (TrajectoryStatus::Unsettled, TrajectoryStatus::Interrupted),
            (TrajectoryStatus::Interrupted, TrajectoryStatus::Error),
        ];

        for (lower, higher) in precedence_pairs {
            for statuses in [[lower, higher], [higher, lower]] {
                let records = [
                    test_record(1, TrajectoryRecordKind::AssistantMessage, statuses[0]),
                    test_record(2, TrajectoryRecordKind::AssistantMessage, statuses[1]),
                ];
                let groups = group_records(&records);
                assert_eq!(groups[0].status, higher);
                assert_eq!(groups[0].turns[0].status, higher);
                assert_eq!(groups[0].turns[0].steps[0].status, higher);
            }
        }
    }

    #[test]
    fn sequence_only_member_prevents_group_recorded_duration() {
        let start = Utc.with_ymd_and_hms(2026, 9, 1, 12, 0, 0).unwrap();
        let end = Utc.with_ymd_and_hms(2026, 9, 1, 12, 0, 2).unwrap();
        let mut first = test_record(
            1,
            TrajectoryRecordKind::AssistantMessage,
            TrajectoryStatus::Completed,
        );
        first.timing = Some(TrajectoryTiming::recorded(
            Some(start),
            Some(end),
            Some(2_000),
            None,
        ));
        let mut second = test_record(2, TrajectoryRecordKind::Done, TrajectoryStatus::Completed);
        second.timing = Some(TrajectoryTiming::sequence_only());

        assert_eq!(group_records(&[first, second])[0].timing, None);
    }

    #[test]
    fn group_timing_uses_first_start_and_last_end_only() {
        let start = Utc.with_ymd_and_hms(2026, 9, 1, 12, 0, 0).unwrap();
        let end = Utc.with_ymd_and_hms(2026, 9, 1, 12, 0, 3).unwrap();
        let mut first = test_record(
            1,
            TrajectoryRecordKind::AssistantMessage,
            TrajectoryStatus::Completed,
        );
        first.timing = Some(TrajectoryTiming::recorded(Some(start), None, None, None));
        let mut last = test_record(2, TrajectoryRecordKind::Done, TrajectoryStatus::Completed);
        last.timing = Some(TrajectoryTiming::recorded(None, Some(end), None, None));

        let timing = group_records(&[first, last])[0].timing.clone().unwrap();
        assert_eq!(timing.started_at, Some(start));
        assert_eq!(timing.ended_at, Some(end));
        assert_eq!(timing.duration_ms, None);
        assert_eq!(timing.effective_duration_ms(), Some(3_000));
    }

    #[test]
    fn test_timing_sequence_only_vs_recorded() {
        // Scenario 5: Project a record with no timestamp as sequence-only; formatting returns unavailable rather than 0 ms.
        let seq_timing = TrajectoryTiming::sequence_only();
        assert_eq!(format_duration(Some(&seq_timing)), None);
        assert_eq!(format_duration_or_unavailable(Some(&seq_timing)), "—");
        assert_eq!(format_duration_or_unavailable(None), "—");

        let rec_timing = TrajectoryTiming::recorded(None, None, Some(1500), None);
        assert_eq!(format_duration(Some(&rec_timing)), Some("1.5s".into()));
        assert_eq!(format_duration_or_unavailable(Some(&rec_timing)), "1.5s");

        let start = Utc.with_ymd_and_hms(2026, 9, 1, 12, 0, 0).unwrap();
        let end = Utc.with_ymd_and_hms(2026, 9, 1, 12, 0, 2).unwrap();
        let calc_timing = TrajectoryTiming::recorded(Some(start), Some(end), None, None);
        assert_eq!(format_duration(Some(&calc_timing)), Some("2s".into()));
        assert_eq!(format_duration_or_unavailable(Some(&calc_timing)), "2s");
        assert_eq!(format_duration_ms(59_999), "1m 0s");
        assert_eq!(format_duration_ms(192_000), "3m 12s");
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
    fn late_partial_cannot_clobber_an_existing_final_record() {
        let mut final_record = test_record(
            1,
            TrajectoryRecordKind::AssistantMessage,
            TrajectoryStatus::Completed,
        );
        final_record.summary = "Final".into();

        let mut late_partial = final_record.clone();
        late_partial.status = TrajectoryStatus::Running;
        late_partial.is_partial = true;
        late_partial.summary = "Late partial".into();

        let mut records = vec![final_record];
        apply_deltas(&mut records, [late_partial]);

        assert_eq!(records.len(), 1);
        assert!(!records[0].is_partial);
        assert_eq!(records[0].summary, "Final");
        assert_eq!(records[0].status, TrajectoryStatus::Completed);
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
    fn sanitization_redacts_secret_shapes_and_hides_mcp_inputs() {
        let secret_shapes = [
            "ghp_short",
            "gho_short",
            "github_pat_short",
            "sk-short",
            "xoxb-short",
            "AKIA1234567890ABCDEF",
            "Bearer short",
            "Authorization: short",
            "password=short",
            "token=short",
            "api_key=short",
            "api-key=short",
            "apikey=short",
            "abcdefghijklmnopqrstuvwxyzABCDEF",
        ];
        for secret in secret_shapes {
            let call = ToolCall::Exec {
                command: format!("echo {secret}"),
            };
            let (summary, preview, _) = sanitize_tool_call(&call, 1_024);
            assert!(!summary.contains(secret), "summary leaked {secret}");
            assert!(
                !preview.as_deref().unwrap().contains(secret),
                "preview leaked {secret}"
            );
        }

        let authorization = ToolCall::Exec {
            command: "curl -H 'Authorization: Basic short-credential'".into(),
        };
        let (summary, preview, _) = sanitize_tool_call(&authorization, 1_024);
        assert!(!summary.contains("short-credential"));
        assert!(!preview.as_deref().unwrap().contains("short-credential"));

        for (assignment, secret) in [
            ("curl --password=hunter2", "hunter2"),
            ("run --token=short-token", "short-token"),
            ("curl -H 'X-Api-Key: abc123'", "abc123"),
            (r#"send '{"password":"json-secret"}'"#, "json-secret"),
        ] {
            let call = ToolCall::Exec {
                command: assignment.into(),
            };
            let (summary, preview, _) = sanitize_tool_call(&call, 1_024);
            assert!(!summary.contains(secret), "summary leaked {secret}");
            assert!(
                !preview.as_deref().unwrap().contains(secret),
                "preview leaked {secret}"
            );
        }

        let prompt = "Use ghp_prompt-secret with Authorization: Bearer sk-prompt-secret";
        let (summary, preview) = sanitize_prompt_preview(prompt, 1_024);
        assert!(!summary.contains("ghp_prompt-secret"));
        assert!(!summary.contains("sk-prompt-secret"));
        assert!(!preview.as_deref().unwrap().contains("ghp_prompt-secret"));
        assert!(!preview.as_deref().unwrap().contains("sk-prompt-secret"));

        let mcp_input = serde_json::json!({
            "token": "ghp_mcp-secret",
            "path": "/tmp/input",
            "recursive": true
        });
        let input_bytes = mcp_input.to_string().len();
        let mcp = ToolCall::Mcp {
            server: "filesystem".into(),
            tool: "read".into(),
            input: Some(mcp_input),
        };
        let (summary, preview, schema) = sanitize_tool_call(&mcp, 1_024);
        assert!(!summary.contains("ghp_mcp-secret"));
        assert_eq!(
            preview.as_deref(),
            Some(format!("args: path, recursive, token ({input_bytes} bytes)").as_str())
        );
        assert!(!preview.as_deref().unwrap().contains("ghp_mcp-secret"));
        assert!(!schema.as_deref().unwrap().contains("ghp_mcp-secret"));

        let unknown = ToolCall::Unknown {
            name: "custom".into(),
            input: Some(serde_json::json!({
                "authorization": "Bearer sk-unknown-secret"
            })),
        };
        let (summary, preview, schema) = sanitize_tool_call(&unknown, 1_024);
        assert!(!summary.contains("sk-unknown-secret"));
        assert!(!preview.as_deref().unwrap().contains("sk-unknown-secret"));
        assert!(!schema.as_deref().unwrap().contains("sk-unknown-secret"));
        assert!(
            preview
                .as_deref()
                .unwrap()
                .starts_with("args: authorization (")
        );
    }

    #[test]
    fn trajectory_wire_values_are_forward_tolerant_and_camel_case() {
        let kind = serde_json::to_value(TrajectoryRecordKind::ToolCall {
            tool_name: "bash".into(),
        })
        .unwrap();
        assert_eq!(kind["toolName"], "bash");
        assert!(kind.get("tool_name").is_none());

        assert_eq!(
            serde_json::from_str::<TrajectoryLane>("\"futureLane\"").unwrap(),
            TrajectoryLane::Unknown
        );
        assert_eq!(
            serde_json::from_str::<TrajectoryStatus>("\"futureStatus\"").unwrap(),
            TrajectoryStatus::Unknown
        );
        assert_eq!(
            serde_json::to_string(&TrajectoryLane::Tools).unwrap(),
            "\"tools\""
        );
        assert_eq!(
            serde_json::to_string(&TrajectoryStatus::Completed).unwrap(),
            "\"completed\""
        );

        let raw_ref: TrajectoryRawRef = serde_json::from_value(serde_json::json!({
            "chatId": "chat-1",
            "sourceSeq": 1,
            "field": "payload"
        }))
        .unwrap();
        assert_eq!(raw_ref.source_version, 1);
    }

    #[test]
    fn test_opaque_token_straddling_preview_cap_is_redacted_before_truncation() {
        // An opaque 40-char token that starts 14 bytes before the 1024-byte
        // cap: truncating first would leave a 14-char prefix that the >=32
        // opaque rule no longer matches, leaking part of the secret.
        let token = "Zq7Lm9Xk3Vb2Nc8Pw5Rt1Yh6Uj4Gf0Dd9Ss2Aa7E";
        assert_eq!(token.len(), 40);
        let filler = "a ".repeat(505);
        assert_eq!(filler.len(), 1010);
        let text = format!("{filler}{token} tail");
        let (summary, preview) = sanitize_prompt_preview(&text, 1024);
        let preview = preview.unwrap();
        assert!(preview.ends_with("(truncated)"), "cap not applied: {preview:?}");
        assert!(!preview.contains(&token[..14]), "preview leaked {preview:?}");
        assert!(!summary.contains(&token[..14]), "summary leaked {summary:?}");
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

    #[test]
    fn test_trajectory_timing_non_terminal_run_with_settled_tools_produces_no_duration() {
        let start = Utc.with_ymd_and_hms(2026, 9, 1, 12, 0, 0).unwrap();
        let end = Utc.with_ymd_and_hms(2026, 9, 1, 12, 0, 2).unwrap();

        let mut call = test_record(
            1,
            TrajectoryRecordKind::ToolCall {
                tool_name: "bash".into(),
            },
            TrajectoryStatus::Running,
        );
        call.timing = Some(TrajectoryTiming::recorded(Some(start), None, None, None));

        let mut result = test_record(
            2,
            TrajectoryRecordKind::ToolResult {
                tool_name: "bash".into(),
            },
            TrajectoryStatus::Running,
        );
        result.timing = Some(TrajectoryTiming::recorded(None, Some(end), None, None));

        let groups = group_records(&[call, result]);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].status, TrajectoryStatus::Running);
        assert_eq!(
            groups[0].timing, None,
            "non-terminal run must NOT produce an aggregate duration even if a tool settled"
        );
    }

    #[test]
    fn test_trajectory_timing_missing_timing_degrades_to_sequence_only_instead_of_stronger_claim() {
        let start = Utc.with_ymd_and_hms(2026, 9, 1, 12, 0, 0).unwrap();
        let mid = Utc.with_ymd_and_hms(2026, 9, 1, 12, 0, 1).unwrap();
        let end = Utc.with_ymd_and_hms(2026, 9, 1, 12, 0, 3).unwrap();

        let mut r1 = test_record(
            1,
            TrajectoryRecordKind::AssistantMessage,
            TrajectoryStatus::Completed,
        );
        r1.timing = Some(TrajectoryTiming::recorded(
            Some(start),
            Some(mid),
            None,
            None,
        ));

        let mut r2 = test_record(
            2,
            TrajectoryRecordKind::Reasoning,
            TrajectoryStatus::Completed,
        );
        r2.timing = None; // Missing timing must degrade aggregate, not be silently skipped

        let mut r3 = test_record(3, TrajectoryRecordKind::Done, TrajectoryStatus::Completed);
        r3.timing = Some(TrajectoryTiming::recorded(Some(mid), Some(end), None, None));

        let groups = group_records(&[r1, r2, r3]);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].status, TrajectoryStatus::Completed);
        assert_eq!(
            groups[0].timing, None,
            "a member record with timing == None must degrade run timing to None"
        );
    }

    #[test]
    fn test_trajectory_timing_aggregate_uses_max_ended_at() {
        let t0 = Utc.with_ymd_and_hms(2026, 9, 1, 12, 0, 0).unwrap();
        let t1 = Utc.with_ymd_and_hms(2026, 9, 1, 12, 0, 1).unwrap();
        let t2 = Utc.with_ymd_and_hms(2026, 9, 1, 12, 0, 2).unwrap();
        let t3 = Utc.with_ymd_and_hms(2026, 9, 1, 12, 0, 5).unwrap();

        let mut r1 = test_record(
            1,
            TrajectoryRecordKind::AssistantMessage,
            TrajectoryStatus::Completed,
        );
        r1.timing = Some(TrajectoryTiming::recorded(Some(t0), Some(t1), None, None));

        let mut r2_tool_long = test_record(
            2,
            TrajectoryRecordKind::ToolResult {
                tool_name: "background_job".into(),
            },
            TrajectoryStatus::Completed,
        );
        r2_tool_long.timing = Some(TrajectoryTiming::recorded(Some(t1), Some(t3), None, None));

        let mut r3_done = test_record(3, TrajectoryRecordKind::Done, TrajectoryStatus::Completed);
        r3_done.timing = Some(TrajectoryTiming::recorded(Some(t2), Some(t2), None, None));

        let groups = group_records(&[r1, r2_tool_long, r3_done]);
        let timing = groups[0].timing.as_ref().expect("timing present");
        assert_eq!(timing.started_at, Some(t0));
        assert_eq!(
            timing.ended_at,
            Some(t3),
            "aggregate ended_at must be the MAX ended_at among records, not the last seen"
        );
        assert_eq!(timing.effective_duration_ms(), Some(5_000));
    }

    #[test]
    fn test_trajectory_record_kind_unknown_kind_deserializes_to_custom() {
        // Unknown kind from a newer build degrades gracefully to Custom { name }
        let json_from_newer_build = serde_json::json!({
            "kind": "deepSearchTask",
            "searchQuery": "quantum computing",
            "subAgents": 3
        });
        let deserialized: TrajectoryRecordKind =
            serde_json::from_value(json_from_newer_build).expect("deserialization must succeed");
        assert_eq!(
            deserialized,
            TrajectoryRecordKind::Custom {
                name: "deepSearchTask".to_string()
            }
        );
        assert_eq!(deserialized.default_lane(), TrajectoryLane::Model);

        // Round-trip of Custom variant
        let custom = TrajectoryRecordKind::Custom {
            name: "futureProtocolKind".to_string(),
        };
        let serialized = serde_json::to_string(&custom).expect("serialize custom");
        assert_eq!(serialized, "{\"kind\":\"futureProtocolKind\"}");
        let roundtrip: TrajectoryRecordKind =
            serde_json::from_str(&serialized).expect("deserialize roundtrip");
        assert_eq!(roundtrip, custom);

        // Explicit { "kind": "custom", "name": "foo" } format also supported
        let explicit_custom = serde_json::json!({
            "kind": "custom",
            "name": "legacyFormatKind"
        });
        let from_explicit: TrajectoryRecordKind =
            serde_json::from_value(explicit_custom).expect("deserialize explicit custom");
        assert_eq!(
            from_explicit,
            TrajectoryRecordKind::Custom {
                name: "legacyFormatKind".to_string()
            }
        );
    }

    #[test]
    fn test_trajectory_reconcile_shuffled_multirun_batch_insertion_and_idempotence() {
        let r1_1 = test_record(
            1,
            TrajectoryRecordKind::UserMessage,
            TrajectoryStatus::Completed,
        );
        let mut r1_2 = test_record(
            2,
            TrajectoryRecordKind::AssistantMessage,
            TrajectoryStatus::Completed,
        );
        r1_2.id = TrajectoryRecordId::new("run-1", 2, 0);
        r1_2.source_seq = 2;
        let mut r1_5 = test_record(5, TrajectoryRecordKind::Done, TrajectoryStatus::Completed);
        r1_5.id = TrajectoryRecordId::new("run-1", 5, 0);
        r1_5.source_seq = 5;

        let mut r2_1 = test_record(
            1,
            TrajectoryRecordKind::UserMessage,
            TrajectoryStatus::Completed,
        );
        r2_1.id = TrajectoryRecordId::new("run-2", 1, 0);
        r2_1.run_id = "run-2".into();
        r2_1.source_seq = 1;
        let mut r2_2 = test_record(
            2,
            TrajectoryRecordKind::AssistantMessage,
            TrajectoryStatus::Completed,
        );
        r2_2.id = TrajectoryRecordId::new("run-2", 2, 0);
        r2_2.run_id = "run-2".into();
        r2_2.source_seq = 2;
        let mut r2_5 = test_record(5, TrajectoryRecordKind::Done, TrajectoryStatus::Completed);
        r2_5.id = TrajectoryRecordId::new("run-2", 5, 0);
        r2_5.run_id = "run-2".into();
        r2_5.source_seq = 5;

        let expected_ordered_ids = vec![
            TrajectoryRecordId::new("run-1", 1, 0),
            TrajectoryRecordId::new("run-2", 1, 0),
            TrajectoryRecordId::new("run-1", 2, 0),
            TrajectoryRecordId::new("run-2", 2, 0),
            TrajectoryRecordId::new("run-1", 5, 0),
            TrajectoryRecordId::new("run-2", 5, 0),
        ];

        // Shuffled batch interleaving runs and sequences
        let shuffled = vec![
            r2_5.clone(),
            r1_2.clone(),
            r2_1.clone(),
            r1_5.clone(),
            r2_2.clone(),
            r1_1.clone(),
        ];

        let mut records = Vec::new();

        // First pass: insertion branch for all items
        apply_deltas(&mut records, shuffled.clone());
        let pass1_ids: Vec<_> = records.iter().map(|r| r.id.clone()).collect();
        assert_eq!(
            pass1_ids, expected_ordered_ids,
            "first pass must insert records in (source_seq, sub_seq, run_id) stream order"
        );

        // Second pass: idempotence check
        apply_deltas(&mut records, shuffled);
        let pass2_ids: Vec<_> = records.iter().map(|r| r.id.clone()).collect();
        assert_eq!(
            pass2_ids, expected_ordered_ids,
            "second pass must be strictly idempotent with no duplicates or reordering"
        );
    }

    #[test]
    fn test_trajectory_group_records_legacy_and_native_run_numbering() {
        let mut r_leg = test_record(
            1,
            TrajectoryRecordKind::UserMessage,
            TrajectoryStatus::Completed,
        );
        r_leg.id = TrajectoryRecordId::new("legacy-session-001", 1, 0);
        r_leg.run_id = "legacy-session-001".into();

        let mut r1 = test_record(
            1,
            TrajectoryRecordKind::UserMessage,
            TrajectoryStatus::Completed,
        );
        r1.id = TrajectoryRecordId::new("run-native-1", 1, 0);
        r1.run_id = "run-native-1".into();

        let mut r2 = test_record(
            1,
            TrajectoryRecordKind::UserMessage,
            TrajectoryStatus::Completed,
        );
        r2.id = TrajectoryRecordId::new("run-native-2", 1, 0);
        r2.run_id = "run-native-2".into();

        let groups = group_records(&[r_leg, r1, r2]);
        assert_eq!(groups.len(), 3);
        assert_eq!(groups[0].label, "Legacy Run");
        assert_eq!(groups[0].is_legacy, true);
        assert_eq!(groups[1].label, "Run 1");
        assert_eq!(groups[1].is_legacy, false);
        assert_eq!(groups[2].label, "Run 2");
        assert_eq!(groups[2].is_legacy, false);
    }

    #[test]
    fn test_trajectory_sanitize_unavailable_sizes_do_not_fabricate_zero_bytes() {
        let write_none = ToolCall::WriteFile {
            path: "/path/to/file.txt".into(),
            content: None,
        };
        let (summary, preview, _) = sanitize_tool_call(&write_none, 1024);
        assert_eq!(summary, "Write /path/to/file.txt");
        let prev = preview.expect("preview present");
        assert!(prev.contains("Bytes: unavailable"));
        assert!(!prev.contains("Bytes: 0"));

        let write_empty = ToolCall::WriteFile {
            path: "/path/to/empty.txt".into(),
            content: Some(String::new()),
        };
        let (_, preview_empty, _) = sanitize_tool_call(&write_empty, 1024);
        assert!(preview_empty.expect("preview present").contains("Bytes: 0"));

        let mcp_none = ToolCall::Mcp {
            server: "git".into(),
            tool: "status".into(),
            input: None,
        };
        let (_, mcp_prev, _) = sanitize_tool_call(&mcp_none, 1024);
        assert_eq!(mcp_prev.as_deref(), Some("args: none (size unavailable)"));

        let unknown_none = ToolCall::Unknown {
            name: "custom_op".into(),
            input: None,
        };
        let (_, unk_prev, _) = sanitize_tool_call(&unknown_none, 1024);
        assert_eq!(unk_prev.as_deref(), Some("args: none (size unavailable)"));
    }

    #[test]
    fn test_trajectory_sanitize_url_userinfo_and_query_credentials_redacted() {
        let web_fetch = ToolCall::WebFetch {
            url: "https://user:s3cret@host.com/p?key=abc123&sig=deadbeef&page=1".into(),
            prompt: None,
        };
        let (summary, preview, _) = sanitize_tool_call(&web_fetch, 1024);
        assert!(!summary.contains("user:s3cret"));
        assert!(!summary.contains("abc123"));
        assert!(!summary.contains("deadbeef"));
        assert!(summary.contains("https://host.com/p?key=[REDACTED]&sig=[REDACTED]&page=1"));

        let prev = preview.expect("preview present");
        assert!(!prev.contains("user:s3cret"));
        assert!(!prev.contains("abc123"));
        assert!(!prev.contains("deadbeef"));
        assert!(prev.contains("https://host.com/p?key=[REDACTED]&sig=[REDACTED]&page=1"));

        let exec_cmd = ToolCall::Exec {
            command: "curl -X GET 'https://admin:pwd999@api.example.com/v1/auth?token=secrettoken&code=456&mode=fast'".into(),
        };
        let (exec_sum, exec_prev, _) = sanitize_tool_call(&exec_cmd, 1024);
        assert!(!exec_sum.contains("admin:pwd999"));
        assert!(!exec_sum.contains("secrettoken"));
        assert!(!exec_sum.contains("456"));
        assert!(exec_sum.contains("token=[REDACTED]"));
        assert!(exec_sum.contains("code=[REDACTED]"));
        assert!(exec_sum.contains("mode=fast"));

        let exec_p = exec_prev.expect("preview present");
        assert!(!exec_p.contains("admin:pwd999"));
        assert!(!exec_p.contains("secrettoken"));
        assert!(!exec_p.contains("456"));
    }

    #[test]
    fn test_trajectory_sanitize_tool_result_error_precedence_over_diff() {
        let diff = ToolDiff {
            path: "src/main.rs".into(),
            old_text: Some("fn old() {}".into()),
            new_text: "fn new() {}".into(),
        };
        let meta = ToolExecutionMeta {
            exit_code: Some(1),
            duration_ms: None,
        };

        // When is_error = true, error summary and preview must take precedence over diff
        let (summary, preview, exit_code) = sanitize_tool_result(
            Some("compilation failed"),
            Some(&diff),
            Some(&meta),
            true,
            500,
        );

        assert_eq!(summary, "Failed (exit code 1)");
        assert_eq!(
            preview.as_deref(),
            Some("Tool execution failed (exit code 1)")
        );
        assert_eq!(exit_code, Some(1));
        assert!(!preview.as_deref().unwrap().contains("Diff on src/main.rs"));

        // When is_error = false, diff is displayed normally
        let (ok_sum, ok_prev, ok_exit) =
            sanitize_tool_result(Some("ok"), Some(&diff), Some(&meta), false, 500);
        assert_eq!(ok_sum, "Diff on src/main.rs");
        assert!(ok_prev.as_deref().unwrap().contains("Diff on src/main.rs"));
        assert_eq!(ok_exit, Some(1));
    }
}
