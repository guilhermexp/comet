//! Background work a Worker started and has not yet seen finish, read from the
//! provider's own transcript.
//!
//! Claude Code ends a turn (and fires `Stop`) while a `run_in_background` shell
//! or an async agent is still running, then opens a new turn on its own when
//! the task reports back. A `Stop` in that state is a pause, not the end of the
//! delegated task. The transcript records both edges: the launch as a tool
//! result whose `toolUseResult` carries `backgroundTaskId` (shell) or
//! `isAsync` + `agentId` (agent), and the finish as a queued
//! `<task-notification>` naming the same `<task-id>`. Providers that record
//! neither have no pending work here.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

use serde_json::Value;

/// Bytes of provider JSONL tail scanned. A task launched before this window is
/// not seen, which fails toward the previous behavior (completion allowed).
const SCAN_TAIL_BYTES: u64 = 2 * 1024 * 1024;

/// Ids of background tasks launched at or after `since_unix_ms` whose finish
/// notification is not in `lines`.
pub fn pending_background_task_ids<'a>(
    lines: impl IntoIterator<Item = &'a str>,
    since_unix_ms: u64,
) -> HashSet<String> {
    let mut pending = HashSet::new();
    for line in lines {
        for id in notified_task_ids(line) {
            pending.remove(id);
        }
        if !line.contains("\"toolUseResult\"") {
            continue;
        }
        let Ok(value) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        let Some(result) = value.get("toolUseResult") else {
            continue;
        };
        let id = result
            .get("backgroundTaskId")
            .and_then(Value::as_str)
            .or_else(|| {
                result
                    .get("isAsync")
                    .and_then(Value::as_bool)
                    .filter(|is_async| *is_async)
                    .and_then(|_| result.get("agentId"))
                    .and_then(Value::as_str)
            });
        let Some(id) = id.filter(|id| !id.is_empty()) else {
            continue;
        };
        let launched_at = value
            .get("timestamp")
            .and_then(Value::as_str)
            .and_then(|stamp| chrono::DateTime::parse_from_rfc3339(stamp).ok())
            .map(|stamp| stamp.timestamp_millis().max(0) as u64);
        if launched_at.is_none_or(|launched_at| launched_at >= since_unix_ms) {
            pending.insert(id.to_owned());
        }
    }
    pending
}

fn notified_task_ids(line: &str) -> impl Iterator<Item = &str> {
    const OPEN: &str = "<task-id>";
    const CLOSE: &str = "</task-id>";
    line.match_indices(OPEN).filter_map(move |(index, _)| {
        let rest = &line[index + OPEN.len()..];
        rest.find(CLOSE).map(|end| rest[..end].trim())
    })
}

/// How many background tasks the Worker's current runtime launched and has
/// not seen finish. Errors (no transcript, unreadable file) count as none:
/// this only ever withholds a completion, it never invents one.
pub fn pending_background_tasks(session_id: &str) -> usize {
    let Some(manifest) = unpeel_core::session_host::load_manifest(session_id) else {
        return 0;
    };
    let Ok(transcript) = unpeel_core::transcripts::resolve_provider_transcript(&manifest) else {
        return 0;
    };
    if transcript.path.extension().and_then(|ext| ext.to_str()) != Some("jsonl") {
        return 0;
    }
    // Tasks of an earlier runtime never report back to this one.
    let since = manifest.runtime_launched_at.unwrap_or(0);
    let Ok(len) = std::fs::metadata(&transcript.path).map(|metadata| metadata.len()) else {
        return 0;
    };
    type Cache = Mutex<HashMap<PathBuf, (u64, u64, usize)>>;
    static CACHE: OnceLock<Cache> = OnceLock::new();
    let cache = CACHE.get_or_init(Default::default);
    if let Some((cached_len, cached_since, count)) = cache
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .get(&transcript.path)
        .copied()
        && cached_len == len
        && cached_since == since
    {
        return count;
    }
    let Ok(read) =
        unpeel_core::transcripts::read_jsonl_lines_before(&transcript.path, None, SCAN_TAIL_BYTES)
    else {
        return 0;
    };
    let count =
        pending_background_task_ids(read.lines.iter().map(|(_, line)| line.as_str()), since).len();
    cache
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .insert(transcript.path, (len, since, count));
    count
}

#[cfg(test)]
mod tests {
    use super::pending_background_task_ids;

    const SHELL: &str = r#"{"type":"user","timestamp":"2026-10-14T20:36:15.908Z","toolUseResult":{"stdout":"","backgroundTaskId":"bv55tpwo5"}}"#;
    const AGENT: &str = r#"{"type":"user","timestamp":"2026-10-14T20:33:28.819Z","toolUseResult":{"isAsync":true,"status":"async_launched","agentId":"a1fe4f6baa2d3b59d"}}"#;
    const SHELL_DONE: &str = r#"{"type":"queue-operation","operation":"enqueue","timestamp":"2026-10-14T20:41:02.960Z","content":"<task-notification>\n<task-id>bv55tpwo5</task-id>\n<status>completed</status>"}"#;
    const AGENT_DONE: &str = r#"{"type":"queue-operation","operation":"enqueue","content":"<task-notification>\n<task-id>a1fe4f6baa2d3b59d</task-id>"}"#;
    const PLAIN: &str = r#"{"type":"user","timestamp":"2026-10-14T20:36:16.000Z","toolUseResult":{"stdout":"ok","interrupted":false}}"#;

    #[test]
    fn launched_tasks_stay_pending_until_their_notification() {
        let pending = pending_background_task_ids([AGENT, SHELL, PLAIN], 0);
        assert_eq!(pending.len(), 2);
        let pending = pending_background_task_ids([AGENT, SHELL, SHELL_DONE], 0);
        assert_eq!(
            pending.into_iter().collect::<Vec<_>>(),
            vec!["a1fe4f6baa2d3b59d"]
        );
        assert!(pending_background_task_ids([AGENT, SHELL, SHELL_DONE, AGENT_DONE], 0).is_empty());
    }

    #[test]
    fn tasks_from_before_the_current_runtime_are_ignored() {
        // 2026-10-14T20:35:00Z: the agent launched before, the shell after.
        let since = 1_792_010_100_000;
        let pending = pending_background_task_ids([AGENT, SHELL], since);
        assert_eq!(pending.into_iter().collect::<Vec<_>>(), vec!["bv55tpwo5"]);
    }
}
