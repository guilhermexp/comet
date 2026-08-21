#[cfg(test)]
mod tests {
    use std::fs;

    use chrono::{TimeZone, Utc};

    use super::summarize_codex_jsonl;

    #[test]
    fn codex_summary_uses_latest_lifetime_token_count() {
        let path = std::env::temp_dir().join(format!("zeron-usage-{}.jsonl", uuid::Uuid::new_v4()));
        fs::write(
            &path,
            concat!(
                "{\"timestamp\":\"2026-08-20T10:00:00Z\",\"type\":\"event_msg\",\"payload\":{\"type\":\"token_count\",\"info\":{\"total_token_usage\":{\"total_tokens\":120}}}}\n",
                "{\"timestamp\":\"2026-08-20T11:00:00Z\",\"type\":\"event_msg\",\"payload\":{\"type\":\"token_count\",\"info\":{\"total_token_usage\":{\"total_tokens\":450}}}}\n",
            ),
        )
        .unwrap();
        let summary = summarize_codex_jsonl(&path).unwrap();
        assert_eq!(summary.total_tokens, 450);
        assert_eq!(
            summary.timestamp,
            Utc.with_ymd_and_hms(2026, 8, 20, 11, 0, 0).unwrap()
        );
        let _ = fs::remove_file(path);
    }
}
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Duration, Utc};
use serde_json::Value;
use zeron_proto::AgentUsageLine;

const MAX_FILES: usize = 2_000;

#[derive(Debug, Clone, PartialEq)]
struct CodexSummary {
    timestamp: DateTime<Utc>,
    total_tokens: u64,
}

fn timestamp(value: &Value) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(value.as_str()?)
        .ok()
        .map(|value| value.with_timezone(&Utc))
}

fn number_at(value: &Value, paths: &[&[&str]]) -> u64 {
    paths
        .iter()
        .find_map(|path| {
            path.iter()
                .try_fold(value, |current, key| current.get(key))?
                .as_u64()
        })
        .unwrap_or(0)
}

fn summarize_codex_jsonl(path: &Path) -> Option<CodexSummary> {
    let raw = fs::read_to_string(path).ok()?;
    raw.lines().rev().find_map(|line| {
        let record: Value = serde_json::from_str(line).ok()?;
        let payload = record.get("payload")?;
        if record.get("type")?.as_str()? != "event_msg"
            || payload.get("type")?.as_str()? != "token_count"
        {
            return None;
        }
        Some(CodexSummary {
            timestamp: timestamp(
                record
                    .get("timestamp")
                    .or_else(|| payload.get("timestamp"))?,
            )?,
            total_tokens: number_at(
                payload,
                &[
                    &["info", "total_token_usage", "total_tokens"],
                    &["info", "totalTokenUsage", "totalTokens"],
                    &["total_token_usage", "total_tokens"],
                    &["totalTokens"],
                ],
            ),
        })
    })
}

fn recent_files(mut paths: Vec<PathBuf>) -> Vec<PathBuf> {
    paths.sort_by_key(|path| {
        std::cmp::Reverse(
            fs::metadata(path)
                .and_then(|metadata| metadata.modified())
                .ok(),
        )
    });
    paths.truncate(MAX_FILES);
    paths
}

fn usage_lines(
    samples: &[(DateTime<Utc>, u64, String)],
    now: DateTime<Utc>,
) -> Vec<AgentUsageLine> {
    [
        ("24h", Duration::days(1)),
        ("7d", Duration::days(7)),
        ("30d", Duration::days(30)),
    ]
    .into_iter()
    .map(|(label, duration)| {
        let cutoff = now - duration;
        let matching: Vec<_> = samples.iter().filter(|sample| sample.0 >= cutoff).collect();
        let tokens = matching.iter().map(|sample| sample.1).sum();
        let sessions = matching
            .iter()
            .map(|sample| sample.2.as_str())
            .collect::<HashSet<_>>()
            .len();
        AgentUsageLine {
            label: label.into(),
            value: format!("{} tokens", compact(tokens)),
            subtitle: (sessions > 0).then(|| {
                format!(
                    "{sessions} recent {}",
                    if sessions == 1 { "session" } else { "sessions" }
                )
            }),
        }
    })
    .collect()
}

fn compact(value: u64) -> String {
    if value < 1_000 {
        return value.to_string();
    }
    let (divisor, suffix) = if value >= 1_000_000_000 {
        (1_000_000_000.0, "B")
    } else if value >= 1_000_000 {
        (1_000_000.0, "M")
    } else {
        (1_000.0, "K")
    };
    let scaled = value as f64 / divisor;
    if value >= 1_000_000 || (scaled.fract().abs() < 0.05) {
        format!("{scaled:.0}{suffix}")
    } else {
        format!("{scaled:.1}{suffix}")
    }
}

pub(crate) fn codex_usage_lines(root: &Path, now: DateTime<Utc>) -> Vec<AgentUsageLine> {
    let mut files = Vec::new();
    for offset in 0..=30 {
        let day = (now - Duration::days(offset))
            .format("%Y/%m/%d")
            .to_string();
        if let Ok(entries) = fs::read_dir(root.join(day)) {
            files.extend(entries.flatten().map(|entry| entry.path()).filter(|path| {
                path.extension()
                    .is_some_and(|extension| extension == "jsonl")
            }));
        }
    }
    let samples = recent_files(files)
        .into_iter()
        .filter_map(|path| {
            summarize_codex_jsonl(&path).map(|s| {
                (
                    s.timestamp,
                    s.total_tokens,
                    path.to_string_lossy().into_owned(),
                )
            })
        })
        .collect::<Vec<_>>();
    usage_lines(&samples, now)
}

fn claude_tokens(usage: &Value) -> u64 {
    number_at(usage, &[&["total_tokens"]]).max(
        number_at(usage, &[&["input_tokens"]])
            + number_at(usage, &[&["cache_creation_input_tokens"]])
            + number_at(usage, &[&["cache_read_input_tokens"]])
            + number_at(usage, &[&["output_tokens"]]),
    )
}

pub(crate) fn claude_usage_lines(root: &Path, now: DateTime<Utc>) -> Vec<AgentUsageLine> {
    let mut files = Vec::new();
    if let Ok(projects) = fs::read_dir(root) {
        for project in projects.flatten().filter(|entry| entry.path().is_dir()) {
            if let Ok(entries) = fs::read_dir(project.path()) {
                files.extend(entries.flatten().map(|entry| entry.path()).filter(|path| {
                    path.extension()
                        .is_some_and(|extension| extension == "jsonl")
                }));
            }
        }
    }
    let mut samples = Vec::new();
    let mut seen = HashSet::new();
    for path in recent_files(files) {
        let Ok(raw) = fs::read_to_string(&path) else {
            continue;
        };
        for (index, line) in raw.lines().enumerate() {
            let Ok(record) = serde_json::from_str::<Value>(line) else {
                continue;
            };
            let usage = if record.get("type").and_then(Value::as_str) == Some("assistant") {
                record.get("message").and_then(|value| value.get("usage"))
            } else {
                record
                    .get("toolUseResult")
                    .and_then(|value| value.get("usage"))
            };
            let Some(usage) = usage else { continue };
            let Some(at) = record.get("timestamp").and_then(timestamp) else {
                continue;
            };
            let tokens = claude_tokens(usage);
            if tokens == 0 || at < now - Duration::days(30) {
                continue;
            }
            let session = record
                .get("sessionId")
                .and_then(Value::as_str)
                .map(str::to_owned)
                .unwrap_or_else(|| path.to_string_lossy().into_owned());
            let key = record
                .get("requestId")
                .or_else(|| record.get("uuid"))
                .and_then(Value::as_str)
                .map(str::to_owned)
                .unwrap_or_else(|| format!("{}:{index}", path.display()));
            if seen.insert(format!("{session}:{key}")) {
                samples.push((at, tokens, session));
            }
        }
    }
    usage_lines(&samples, now)
}
