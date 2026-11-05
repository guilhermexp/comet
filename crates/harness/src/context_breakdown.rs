//! Normalize only native accounting metadata; prompt and file bodies never cross this seam.
use serde_json::Value;
use zeron_proto::*;

fn text(value: &Value, key: &str) -> Option<String> {
    value
        .get(key)?
        .as_str()
        .map(|s| s.chars().filter(|c| !c.is_control()).take(512).collect())
}
fn list(value: Option<&Value>) -> ContextItemList {
    let Some(rows) = value.and_then(Value::as_array) else {
        return ContextItemList::default();
    };
    let mut result = ContextItemList {
        availability: ContextAvailability::Available,
        ..Default::default()
    };
    for row in rows.iter().take(512) {
        let Some(name) = text(row, "name").or_else(|| text(row, "path")) else {
            result.availability = ContextAvailability::Partial;
            continue;
        };
        result.items.push(ContextItem {
            name,
            detail: text(row, "serverName")
                .or_else(|| text(row, "server"))
                .or_else(|| text(row, "type")),
            tokens: row.get("tokens").and_then(Value::as_u64),
            deferred: row
                .get("isDeferred")
                .or_else(|| row.get("deferred"))
                .and_then(Value::as_bool)
                .unwrap_or(false),
            loaded: row.get("isLoaded").and_then(Value::as_bool),
            origin: text(row, "origin"),
            state: text(row, "state"),
            path: text(row, "path"),
            tool_name: text(row, "toolName"),
            reason: text(row, "reason"),
        });
    }
    if rows.len() > 512 {
        result.availability = ContextAvailability::Partial;
        result.reason = Some("Native list truncated at 512 items".into());
    }
    result
}

fn section(value: &Value) -> ContextItemList {
    if !value.get("items").is_some_and(Value::is_array) {
        return ContextItemList::default();
    }
    let mut result = list(value.get("items"));
    let declared = match value["availability"].as_str() {
        Some("available") => ContextAvailability::Available,
        Some("partial") => ContextAvailability::Partial,
        Some("not-applicable") => ContextAvailability::NotApplicable,
        _ => ContextAvailability::Unsupported,
    };
    if declared != ContextAvailability::Available
        || result.availability != ContextAvailability::Partial
    {
        result.availability = declared;
    }
    result.reason = text(value, "reason").or(result.reason);
    for item in &mut result.items {
        item.deferred = item.state.as_deref() == Some("last-request-deferred");
    }
    result
}

fn section_total(list: &ContextItemList) -> Option<u64> {
    matches!(list.availability, ContextAvailability::Available).then(|| {
        list.items
            .iter()
            .try_fold(0u64, |total, item| total.checked_add(item.tokens?))
    })?
}

fn omp_details(value: &Value) -> ContextNativeMetadata {
    let identity = &value["identity"];
    let details = &value["details"];
    let deferred = &details["deferred"];
    let last = &deferred["lastRequest"];
    ContextNativeMetadata {
        identity: ContextNativeIdentity {
            session_id: text(identity, "sessionId"),
            provider: text(&identity["model"], "provider"),
            model_id: text(&identity["model"], "id"),
            context_window: identity["contextWindow"].as_u64(),
            compaction_epoch: identity["compactionEpoch"].as_u64(),
            context_usage_revision: identity["contextUsageRevision"].as_u64(),
            prompt_digest: text(identity, "promptDigest"),
            message_count: identity["messageCount"].as_u64(),
        },
        captured_at_ms: value["snapshot"]["capturedAtMs"].as_i64(),
        has_local_changes: value["snapshot"]["hasLocalChanges"].as_bool(),
        threshold_tokens: value["total"]["thresholdTokens"].as_u64(),
        system_context: ContextAttribution {
            sources: section(&details["systemContext"]),
            boundary_approx: details["systemContext"]["boundaryApprox"].as_bool(),
            attributed_tokens: details["systemContext"]["attributedTokens"].as_u64(),
            unattributed_tokens: details["systemContext"]["unattributedTokens"].as_i64(),
        },
        context_files: section(&details["contextFiles"]),
        memory: section(&details["memory"]),
        mcp_instructions: section(&details["mcpInstructions"]),
        deferred: ContextDeferred {
            schemas_in_context: deferred["schemasInContext"].as_bool(),
            xdev: section(&deferred["xdev"]),
            last_request: ContextHistoricalDeferred {
                tools: section(last),
                message_index: last["messageIndex"].as_u64(),
                transcript_index: last["transcriptIndex"].as_u64(),
                timestamp: last["timestamp"].as_i64(),
            },
            active_estimate_reason: text(deferred, "activeEstimateReason"),
        },
        tokenizer_encoding: text(&details["tokenizer"], "encoding"),
        tokenizer_boundary_approx: details["tokenizer"]["boundaryApprox"].as_bool(),
        unavailable: value["unavailable"]
            .as_array()
            .into_iter()
            .flatten()
            .take(128)
            .filter_map(|row| {
                Some(ContextUnavailableReason {
                    section: text(row, "section")?,
                    reason: text(row, "reason")?,
                })
            })
            .collect(),
    }
}
fn category(
    id: &str,
    label: &str,
    kind: ContextCategoryKind,
    tokens: Option<u64>,
    basis: &str,
) -> ContextCategory {
    let bounded = |s: &str| {
        s.chars()
            .filter(|c| !c.is_control())
            .take(512)
            .collect::<String>()
    };
    ContextCategory {
        id: bounded(id),
        label: bounded(label),
        kind,
        tokens,
        basis: bounded(basis),
        parent_id: None,
        reason: None,
    }
}

pub fn omp(value: &Value) -> ContextBreakdown {
    if value.get("schemaVersion").and_then(Value::as_u64) != Some(1) {
        return ContextBreakdown::unavailable(
            ContextAvailability::Unsupported,
            "OMP context inspection requires schema version 1",
        );
    }
    let identity = &value["identity"];
    let Some(raw_model) = identity.get("model") else {
        return ContextBreakdown::unavailable(
            ContextAvailability::Error,
            "Invalid OMP context identity",
        );
    };
    let model = (!raw_model.is_null()).then_some(raw_model);
    if model.is_some_and(|m| {
        m.get("provider").and_then(Value::as_str).is_none()
            || m.get("id").and_then(Value::as_str).is_none()
    }) {
        return ContextBreakdown::unavailable(
            ContextAvailability::Error,
            "Invalid OMP context model identity",
        );
    }
    let mut report = ContextBreakdown {
        schema_version: 1,
        source: "omp-native-v1".into(),
        native_session_id: text(identity, "sessionId"),
        model: model.and_then(|m| Some(format!("{}/{}", text(m, "provider")?, text(m, "id")?))),
        measured_at_ms: value["snapshot"]["lastProviderAtMs"].as_i64(),
        context_epoch: identity["compactionEpoch"].as_u64(),
        context_revision: identity
            .get("contextUsageRevision")
            .and_then(Value::as_u64)
            .map(|v| v.to_string()),
        used_tokens: value["total"]["usedTokens"].as_u64(),
        used_basis: text(&value["snapshot"], "usedBasis"),
        streaming: value["snapshot"]["streaming"].as_bool(),
        pending_request: value["snapshot"]["pendingRequest"].as_bool(),
        capacity: value["total"]["contextWindow"].as_u64(),
        availability: ContextAvailability::Available,
        native: Some(omp_details(value)),
        ..Default::default()
    };
    if model.is_none() {
        report.availability = ContextAvailability::NotApplicable;
        report.reason = Some("No model selected in the native session".into());
        return report;
    }
    let labels = [
        ("systemPrompt", "System prompt"),
        ("systemTools", "System tools"),
        ("systemContext", "System context"),
        ("skills", "Skills"),
        ("messages", "Messages"),
    ];
    for (id, label) in labels {
        let cell = &value["categories"][id];
        let tokens = cell["tokens"].as_u64();
        let basis = cell["basis"].as_str().unwrap_or("unavailable");
        let mut row = category(id, label, ContextCategoryKind::Used, tokens, basis);
        row.reason = text(cell, "reason");
        if tokens.is_none() {
            report.availability = ContextAvailability::Partial;
        }
        report.categories.push(row);
    }
    if let Some(cells) = value["categories"].as_object() {
        for (id, cell) in cells.iter().take(512) {
            if labels.iter().any(|(known, _)| known == id) {
                continue;
            }
            report.categories.push(category(
                id,
                id,
                ContextCategoryKind::Used,
                cell["tokens"].as_u64(),
                cell["basis"].as_str().unwrap_or("unavailable"),
            ));
        }
    }
    report.categories.push(category(
        "autocompact_buffer",
        "Autocompact buffer",
        ContextCategoryKind::Buffer,
        value["total"]["reserveTokens"].as_u64(),
        "native-reserve",
    ));
    report.categories.push(category(
        "free_space",
        "Free space",
        ContextCategoryKind::Free,
        value["total"]["freeTokens"].as_u64(),
        "native-budget",
    ));
    let native = report.native.as_ref().expect("OMP metadata initialized");
    report.mcp_tools = section(&value["details"]["mcpTools"]);
    report.memory_files = native.context_files.clone();
    report
        .memory_files
        .items
        .extend(native.memory.items.iter().cloned());
    if native.memory.availability != ContextAvailability::Available
        || native.context_files.availability != ContextAvailability::Available
    {
        report.memory_files.availability = if report.memory_files.items.is_empty() {
            ContextAvailability::Unsupported
        } else {
            ContextAvailability::Partial
        };
        report.memory_files.reason = native
            .context_files
            .reason
            .clone()
            .or_else(|| native.memory.reason.clone());
    }
    if report.memory_files.items.len() > 512 {
        report.memory_files.items.truncate(512);
        report.memory_files.availability = ContextAvailability::Partial;
        report.memory_files.reason =
            Some("Native memory/context list truncated at 512 items".into());
    }
    for (id, label, list, parent) in [
        (
            "memory_files",
            "Memory / context files",
            &report.memory_files,
            "systemContext",
        ),
        (
            "mcp_instructions",
            "MCP server instructions",
            &native.mcp_instructions,
            "systemContext",
        ),
    ] {
        let mut row = category(
            id,
            label,
            ContextCategoryKind::Used,
            section_total(list),
            "tokenizer-estimate; nested attribution",
        );
        row.parent_id = Some(parent.into());
        row.reason = list.reason.clone();
        report.categories.push(row);
    }
    for (id, label, list) in [
        (
            "discoverable_tools",
            "Discoverable tools",
            &native.deferred.xdev,
        ),
        (
            "last_request_deferred",
            "Deferred tools · last request",
            &native.deferred.last_request.tools,
        ),
    ] {
        let mut row = category(
            id,
            label,
            ContextCategoryKind::Deferred,
            section_total(list),
            "native metadata; not current active budget",
        );
        row.reason = list.reason.clone();
        report.categories.push(row);
    }
    let mut active_mcp = report.mcp_tools.clone();
    active_mcp
        .items
        .retain(|item| matches!(item.state.as_deref(), Some("active" | "omitted-by-policy")));
    let mut row = category(
        "active_mcp_tools",
        "Active MCP schemas",
        ContextCategoryKind::Used,
        section_total(&active_mcp),
        "native declared roster estimate; provider activation unknown",
    );
    row.parent_id = Some("systemTools".into());
    row.reason = native.deferred.active_estimate_reason.clone();
    report.categories.push(row);
    report.availability = ContextAvailability::Partial;
    report.reason = Some("Native total; category/fragment estimates. Deferred request metadata is historical, not current activation.".into());
    report
}

pub fn claude_summary(value: &Value) -> ContextBreakdown {
    let Some(rows) = value.get("categories").and_then(Value::as_array) else {
        return ContextBreakdown::unavailable(
            ContextAvailability::Error,
            "Invalid Claude summary response",
        );
    };
    let mut report = ContextBreakdown {
        schema_version: 1,
        source: "claude-summary".into(),
        model: text(value, "model"),
        used_tokens: value["totalTokens"].as_u64(),
        used_basis: Some("summary-estimate".into()),
        capacity: value["maxTokens"].as_u64(),
        availability: ContextAvailability::Available,
        mcp_tools: list(value.get("mcpTools")),
        memory_files: list(value.get("memoryFiles")),
        ..Default::default()
    };
    // The installed summary control deliberately redacts memoryFiles to [].
    // Empty here is unavailable attribution, not a measured absence of files.
    if report.memory_files.items.is_empty() {
        report.memory_files.availability = ContextAvailability::Unsupported;
        report.memory_files.reason = Some("Claude summary omits memory-file metadata".into());
    }
    for row in rows.iter().take(512) {
        let Some(id) = text(row, "id") else {
            report.availability = ContextAvailability::Partial;
            continue;
        };
        let kind = match row["kind"].as_str() {
            Some("buffer") => ContextCategoryKind::Buffer,
            Some("free") => ContextCategoryKind::Free,
            Some("deferred") => ContextCategoryKind::Deferred,
            _ if row["isDeferred"].as_bool() == Some(true) => ContextCategoryKind::Deferred,
            _ => ContextCategoryKind::Used,
        };
        report.categories.push(category(
            &id,
            &text(row, "name").unwrap_or(id.clone()),
            kind,
            row["tokens"].as_u64(),
            "summary-estimate",
        ));
    }
    if report.used_tokens.is_none()
        || report.capacity.is_none()
        || report.categories.iter().any(|r| r.tokens.is_none())
        || report.mcp_tools.availability != ContextAvailability::Available
        || report.memory_files.availability != ContextAvailability::Available
        || rows.len() > 512
    {
        report.availability = ContextAvailability::Partial;
    }
    // apiUsage:null is absence, and receipt time is never provider time.
    report.measured_at_ms = None;
    report.reason = Some("Approximate summary; provider measurement time unavailable".into());
    report
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn claude_summary_preserves_estimates_null_api_and_unknown_categories() {
        let value = json!({"totalTokens":828300,"maxTokens":1000000,"model":"test", "apiUsage":null,
            "categories":[{"id":"messages","name":"Messages","tokens":828300,"kind":"used"},
            {"id":"future_native_category","name":"Future native category","tokens":12,"kind":"used"},
            {"id":"deferred_tools","name":"Deferred tools","tokens":4200,"kind":"deferred"}],
            "memoryFiles":[],"mcpTools":[]});
        let report = claude_summary(&value);
        assert_eq!(report.used_tokens, Some(828300));
        assert_eq!(report.measured_at_ms, None);
        assert_eq!(report.categories.len(), 3);
        assert_eq!(report.categories[2].percentage(report.capacity), None);
        assert_eq!(
            report.memory_files.availability,
            ContextAvailability::Unsupported
        );
        assert!(report.memory_files.items.is_empty());
        assert!(
            report
                .categories
                .iter()
                .all(|r| r.basis == "summary-estimate")
        );
        let missing = claude_summary(&json!({"categories":[]}));
        assert_eq!(missing.used_tokens, None);
        assert_ne!(
            missing.memory_files.availability,
            ContextAvailability::Available
        );
    }
    #[test]
    fn omp_rejects_unknown_schema_and_no_model_is_not_zero() {
        let unsupported = omp(&json!({"schemaVersion":99}));
        assert_eq!(unsupported.availability, ContextAvailability::Unsupported);
        let no_model = omp(
            &json!({"schemaVersion":1,"identity":{"sessionId":"s","model":null},"total":{"usedTokens":null,"contextWindow":null},"categories":{}}),
        );
        assert_eq!(no_model.availability, ContextAvailability::NotApplicable);
        assert_eq!(no_model.used_tokens, None);
    }
    #[test]
    fn claude_native_lists_preserve_loading_unknown_tokens_and_bounds() {
        let value = json!({"categories":[],"totalTokens":0,"maxTokens":1000,
            "mcpTools":[{"name":"tool","serverName":"server","isLoaded":false},
                {"name":"deferred","tokens":0,"isDeferred":true,"isLoaded":true}],
            "memoryFiles":[{"path":"/synthetic/CLAUDE.md","type":"Project","tokens":0}]});
        let report = claude_summary(&value);
        assert_eq!(report.mcp_tools.items[0].loaded, Some(false));
        assert!(!report.mcp_tools.items[0].deferred);
        assert_eq!(report.mcp_tools.items[0].tokens, None);
        assert!(report.mcp_tools.items[1].deferred);
        assert_eq!(report.mcp_tools.items[1].tokens, Some(0));
        assert_eq!(
            report.memory_files.availability,
            ContextAvailability::Available
        );
        assert_eq!(
            report.memory_files.items[0].detail.as_deref(),
            Some("Project")
        );
        assert_eq!(report.memory_files.items[0].tokens, Some(0));
        let rows = vec![json!({"name":"x".repeat(1000),"tokens":null}); 513];
        let bounded = list(Some(&json!(rows)));
        assert_eq!(bounded.items.len(), 512);
        assert_eq!(bounded.items[0].name.len(), 512);
        assert_eq!(bounded.availability, ContextAvailability::Partial);
    }
    #[test]
    fn coordinated_omp_details_preserve_identity_nested_residual_and_historical_deferred() {
        let value = json!({"schemaVersion":1,
            "identity":{"sessionId":"s","model":{"provider":"fixture","id":"fixture"},"contextWindow":1000,"compactionEpoch":2,"contextUsageRevision":3,"promptDigest":"synthetic-digest","messageCount":8},
            "snapshot":{"capturedAtMs":500,"lastProviderAtMs":400,"usedBasis":"provider-anchor","hasLocalChanges":null,"streaming":false,"pendingRequest":false},
            "total":{"usedTokens":300,"contextWindow":1000,"reserveTokens":100,"thresholdTokens":900,"freeTokens":600},
            "categories":{"systemPrompt":{"tokens":100,"basis":"tokenizer-estimate"},"systemTools":{"tokens":0,"basis":"tokenizer-estimate"},"systemContext":{"tokens":100,"basis":"tokenizer-estimate"},"skills":{"tokens":0,"basis":"tokenizer-estimate"},"messages":{"tokens":100,"basis":"provider-anchor-residual"}},
            "details":{"systemContext":{"availability":"available","items":[],"boundaryApprox":true,"attributedTokens":120,"unattributedTokens":-20},
            "contextFiles":{"availability":"available","items":[{"name":"CLAUDE.md","path":"/synthetic/CLAUDE.md","origin":"context-file","state":"active","tokens":100}]},
            "memory":{"availability":"available","items":[{"name":"Memory","origin":"memory","state":"active","tokens":20}]},
            "mcpInstructions":{"availability":"available","items":[]},
            "mcpTools":{"availability":"available","items":[{"name":"tool","serverName":"server","origin":"mcp","state":"omitted-by-policy","tokens":0}]},
            "deferred":{"schemasInContext":false,"xdev":{"availability":"available","items":[{"name":"discover","origin":"xdev","state":"discoverable","tokens":null}]},"lastRequest":{"availability":"available","reason":"historical-request-only","messageIndex":2,"transcriptIndex":7,"timestamp":400,"items":[{"name":"old-tool","origin":"provider-defer-loading","state":"last-request-deferred","tokens":null}]},"activeEstimateReason":"native-estimate-counts-declared-roster-not-provider-defer-activation"},"tokenizer":{"encoding":null,"boundaryApprox":true}},"unavailable":[]});
        let report = omp(&value);
        assert_eq!(report.memory_files.items.len(), 2);
        assert_eq!(report.mcp_tools.items[0].tokens, Some(0));
        let native = &serde_json::to_value(&report).unwrap()["native"];
        assert_eq!(native["identity"]["messageCount"], 8);
        assert_eq!(native["identity"]["promptDigest"], "synthetic-digest");
        assert!(native["hasLocalChanges"].is_null());
        assert_eq!(native["systemContext"]["unattributedTokens"], -20);
        assert_eq!(native["deferred"]["lastRequest"]["messageIndex"], 2);
        assert_eq!(native["deferred"]["lastRequest"]["transcriptIndex"], 7);
        assert_eq!(native["deferred"]["schemasInContext"], false);
        assert_eq!(
            native["deferred"]["activeEstimateReason"],
            value["details"]["deferred"]["activeEstimateReason"]
        );
        assert!(
            report
                .categories
                .iter()
                .filter(|c| c.kind == ContextCategoryKind::Deferred)
                .all(|c| c.percentage(report.capacity).is_none())
        );
        for version in [json!(true), json!(1.0), json!(2), Value::Null] {
            let mut invalid = value.clone();
            invalid["schemaVersion"] = version;
            assert_eq!(omp(&invalid).availability, ContextAvailability::Unsupported);
        }
    }
}
