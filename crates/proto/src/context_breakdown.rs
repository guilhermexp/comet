//! Ephemeral host-local context inspection. Never stored in a synced document.
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetChatContextBreakdownParams {
    pub chat_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_device_id: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum ContextAvailability {
    Available,
    Partial,
    #[default]
    Unsupported,
    NotApplicable,
    Error,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ContextCategoryKind {
    Used,
    Buffer,
    Free,
    Deferred,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContextCategory {
    pub id: String,
    pub label: String,
    pub kind: ContextCategoryKind,
    pub tokens: Option<u64>,
    pub basis: String,
    /// Nested attribution is included in the parent's budget, never additive.
    pub parent_id: Option<String>,
    pub reason: Option<String>,
}

impl ContextCategory {
    pub fn percentage(&self, capacity: Option<u64>) -> Option<f64> {
        if self.kind == ContextCategoryKind::Deferred {
            return None;
        }
        Some(self.tokens? as f64 / capacity.filter(|n| *n > 0)? as f64 * 100.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default, rename_all = "camelCase")]
pub struct ContextItem {
    pub name: String,
    pub detail: Option<String>,
    pub tokens: Option<u64>,
    pub deferred: bool,
    /// Native loading state is independent of provider deferred accounting.
    #[serde(default)]
    pub loaded: Option<bool>,
    pub origin: Option<String>,
    pub state: Option<String>,
    pub path: Option<String>,
    pub tool_name: Option<String>,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default, rename_all = "camelCase")]
pub struct ContextNativeIdentity {
    pub session_id: Option<String>,
    pub provider: Option<String>,
    pub model_id: Option<String>,
    pub context_window: Option<u64>,
    pub compaction_epoch: Option<u64>,
    pub context_usage_revision: Option<u64>,
    pub prompt_digest: Option<String>,
    pub message_count: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default, rename_all = "camelCase")]
pub struct ContextAttribution {
    pub sources: ContextItemList,
    pub boundary_approx: Option<bool>,
    pub attributed_tokens: Option<u64>,
    /// Signed residual: independent fragment tokenization has BPE boundaries.
    pub unattributed_tokens: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default, rename_all = "camelCase")]
pub struct ContextHistoricalDeferred {
    pub tools: ContextItemList,
    pub message_index: Option<u64>,
    pub transcript_index: Option<u64>,
    pub timestamp: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default, rename_all = "camelCase")]
pub struct ContextDeferred {
    pub schemas_in_context: Option<bool>,
    pub xdev: ContextItemList,
    pub last_request: ContextHistoricalDeferred,
    pub active_estimate_reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default, rename_all = "camelCase")]
pub struct ContextNativeMetadata {
    pub identity: ContextNativeIdentity,
    pub captured_at_ms: Option<i64>,
    /// None means unknown, never false inferred from equal token totals.
    pub has_local_changes: Option<bool>,
    pub threshold_tokens: Option<u64>,
    pub system_context: ContextAttribution,
    pub context_files: ContextItemList,
    pub memory: ContextItemList,
    pub mcp_instructions: ContextItemList,
    pub deferred: ContextDeferred,
    pub tokenizer_encoding: Option<String>,
    pub tokenizer_boundary_approx: Option<bool>,
    pub unavailable: Vec<ContextUnavailableReason>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContextUnavailableReason {
    pub section: String,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ContextItemList {
    pub availability: ContextAvailability,
    pub reason: Option<String>,
    pub items: Vec<ContextItem>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ContextBreakdown {
    pub schema_version: u32,
    pub host_device_id: String,
    pub chat_id: String,
    /// Engine run identity, distinct from the native session id across respawns.
    pub runtime_id: String,
    pub native_session_id: Option<String>,
    pub model: Option<String>,
    pub source: String,
    pub observed_at_ms: i64,
    pub measured_at_ms: Option<i64>,
    pub context_epoch: Option<u64>,
    pub context_revision: Option<String>,
    pub availability: ContextAvailability,
    pub stale: bool,
    pub reason: Option<String>,
    pub used_tokens: Option<u64>,
    pub used_basis: Option<String>,
    #[serde(default)]
    pub streaming: Option<bool>,
    #[serde(default)]
    pub pending_request: Option<bool>,
    pub capacity: Option<u64>,
    pub categories: Vec<ContextCategory>,
    pub mcp_tools: ContextItemList,
    pub memory_files: ContextItemList,
    #[serde(default)]
    pub native: Option<ContextNativeMetadata>,
}

impl ContextBreakdown {
    pub fn unavailable(availability: ContextAvailability, reason: impl Into<String>) -> Self {
        Self {
            schema_version: 1,
            availability,
            reason: Some(reason.into()),
            ..Self::default()
        }
    }

    /// Bounded fractions for the segmented bar. Text retains over-capacity counts.
    pub fn segments(&self) -> Vec<(usize, f32)> {
        let Some(capacity) = self.capacity.filter(|n| *n > 0) else {
            return Vec::new();
        };
        let mut remaining = 1.0f64;
        self.categories
            .iter()
            .enumerate()
            .filter_map(|(index, row)| {
                if row.parent_id.is_some() || row.kind == ContextCategoryKind::Deferred {
                    return None;
                }
                let fraction = (row.tokens? as f64 / capacity as f64).min(remaining);
                remaining = (remaining - fraction).max(0.0);
                (fraction > 0.0).then_some((index, fraction as f32))
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn budget_excludes_deferred_and_nested_attribution_and_bounds_geometry() {
        let row = |id: &str, kind, tokens, parent: Option<&str>| ContextCategory {
            id: id.into(),
            label: id.into(),
            kind,
            tokens,
            basis: "estimate".into(),
            parent_id: parent.map(str::to_owned),
            reason: None,
        };
        let mut report = ContextBreakdown {
            capacity: Some(1_000_000),
            used_tokens: Some(828_300),
            categories: vec![
                row("messages", ContextCategoryKind::Used, Some(828_300), None),
                row(
                    "memory",
                    ContextCategoryKind::Used,
                    Some(100),
                    Some("messages"),
                ),
                row("reserve", ContextCategoryKind::Buffer, Some(33_000), None),
                row("free", ContextCategoryKind::Free, Some(138_700), None),
                row(
                    "deferred",
                    ContextCategoryKind::Deferred,
                    Some(900_000),
                    None,
                ),
            ],
            ..Default::default()
        };
        assert_eq!(
            report
                .segments()
                .iter()
                .map(|(i, _)| *i)
                .collect::<Vec<_>>(),
            vec![0, 2, 3]
        );
        assert_eq!(report.categories[4].percentage(report.capacity), None);
        assert_eq!(
            report.categories[0].percentage(report.capacity),
            Some(82.83)
        );
        report.capacity = Some(0);
        assert!(report.segments().is_empty());
        assert_eq!(report.categories[0].percentage(report.capacity), None);
        report.capacity = Some(100);
        assert_eq!(report.segments(), vec![(0, 1.0)]);
        assert_eq!(report.used_tokens, Some(828_300));
    }
}
