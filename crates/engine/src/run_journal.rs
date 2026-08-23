//! Per-session on-disk event journal (port of zeron's `run-journal.ts`, JSONL-shaped).
//!
//! One append-only JSONL file per chat under `{data_dir}/journals/{chat_id}.jsonl`; each
//! line is `{"seq": n, "event": AgentEvent}` with a monotonically increasing `seq`. The
//! journal is the durable replay source for live streams (`Subscribe` = replay then tail
//! the broadcast hub) and the crash-recovery gauge: a journal whose LAST event is not
//! `Done` belongs to a run that died mid-stream — boot recovery stamps its doc entry
//! `aborted` and closes the journal with a synthetic `Done`.
//!
//! Bounded-window compaction is deferred (whole file kept for now, per M2 scope); a torn
//! trailing line from a crash mid-write is tolerated everywhere.

use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard, PoisonError};

use serde::{Deserialize, Serialize};

use zeron_proto::{AgentEvent, FileToolInputSnapshot, ToolCall, ToolDiff};

#[derive(Debug, thiserror::Error)]
pub enum JournalError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct JournalLine {
    seq: u64,
    event: AgentEvent,
}

struct ChatJournal {
    file: File,
    next_seq: u64,
    /// True when the file ends without a newline (torn write) — the next append
    /// starts with one so the torn line stays isolated.
    needs_newline: bool,
}

/// Append-only JSONL journal store, one file per chat.
pub struct RunJournal {
    dir: PathBuf,
    open_files: Mutex<HashMap<String, ChatJournal>>,
}

impl RunJournal {
    pub fn open(dir: impl AsRef<Path>) -> Result<Self, JournalError> {
        let dir = dir.as_ref().to_path_buf();
        std::fs::create_dir_all(&dir)?;
        Ok(Self {
            dir,
            open_files: Mutex::new(HashMap::new()),
        })
    }

    fn lock(&self) -> MutexGuard<'_, HashMap<String, ChatJournal>> {
        self.open_files
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }

    fn path_for(&self, chat_id: &str) -> PathBuf {
        self.dir.join(format!("{}.jsonl", sanitize_id(chat_id)))
    }

    fn attempts_path(&self, chat_id: &str) -> PathBuf {
        self.dir.join(format!("{}.resume", sanitize_id(chat_id)))
    }

    /// Auto-resume revival budget (zeron `resumeAttempt`/`MAX_AUTO_RESUME`):
    /// persisted beside the journal so a run that CRASHES THE ENGINE cannot
    /// revive itself in an infinite boot loop.
    pub fn resume_attempts(&self, chat_id: &str) -> u32 {
        std::fs::read_to_string(self.attempts_path(chat_id))
            .ok()
            .and_then(|s| s.trim().parse().ok())
            .unwrap_or(0)
    }

    pub fn note_resume_attempt(&self, chat_id: &str) -> u32 {
        let next = self.resume_attempts(chat_id) + 1;
        if let Err(err) = std::fs::write(self.attempts_path(chat_id), next.to_string()) {
            tracing::warn!(chat = %chat_id, error = %err, "resume-attempt ledger write failed");
        }
        next
    }

    /// A cleanly completed turn resets the budget — only consecutive
    /// crash-revive-crash cycles exhaust it.
    pub fn clear_resume_attempts(&self, chat_id: &str) {
        let _ = std::fs::remove_file(self.attempts_path(chat_id));
    }

    /// Append one event; returns its journal seq.
    pub fn append(&self, chat_id: &str, event: &AgentEvent) -> Result<u64, JournalError> {
        let mut files = self.lock();
        if !files.contains_key(chat_id) {
            // Bound the open-fd set: entries were never removed, so every chat
            // ever run held a descriptor for the process lifetime. Dropping is
            // safe — the next append reopens and rescans the tail. The cap
            // comfortably exceeds concurrent runs, so eviction stays rare.
            const OPEN_FILE_CAP: usize = 16;
            if files.len() >= OPEN_FILE_CAP {
                files.clear();
            }
            let path = self.path_for(chat_id);
            let (next_seq, needs_newline) = scan_tail(&path)?;
            let file = OpenOptions::new().create(true).append(true).open(&path)?;
            files.insert(
                chat_id.to_string(),
                ChatJournal {
                    file,
                    next_seq,
                    needs_newline,
                },
            );
        }
        // Entry guaranteed present; avoid unwrap in a library path regardless.
        let Some(journal) = files.get_mut(chat_id) else {
            return Err(JournalError::Io(std::io::Error::other(
                "journal entry vanished under lock",
            )));
        };
        let seq = journal.next_seq;
        let line = serde_json::to_string(&JournalLine {
            seq,
            event: event.clone(),
        })?;
        let mut buf = Vec::with_capacity(line.len() + 2);
        if journal.needs_newline {
            buf.push(b'\n');
        }
        buf.extend_from_slice(line.as_bytes());
        buf.push(b'\n');
        journal.file.write_all(&buf)?;
        journal.file.flush()?;
        journal.needs_newline = false;
        journal.next_seq = seq + 1;
        Ok(seq)
    }

    /// Events with `seq > after_seq`, in order. A cursor ahead of the last issued seq is
    /// from a previous era (file replaced) — falls back to a full replay, mirroring zeron.
    pub fn replay(
        &self,
        chat_id: &str,
        after_seq: u64,
    ) -> Result<Vec<(u64, AgentEvent)>, JournalError> {
        let path = self.path_for(chat_id);
        if !path.exists() {
            return Ok(Vec::new());
        }
        let all = read_lines(&path)?;
        let last_seq = all.last().map(|(seq, _)| *seq).unwrap_or(0);
        let from = if after_seq > last_seq { 0 } else { after_seq };
        Ok(all.into_iter().filter(|(seq, _)| *seq > from).collect())
    }

    /// The last event in a chat's journal, if any (ignores a torn tail line).
    pub fn last_event(&self, chat_id: &str) -> Result<Option<(u64, AgentEvent)>, JournalError> {
        let path = self.path_for(chat_id);
        if !path.exists() {
            return Ok(None);
        }
        Ok(read_lines(&path)?.into_iter().next_back())
    }

    /// Return only the historical input fields needed to render a Write/Edit
    /// card. The synchronized doc never carries these bodies; this lookup is
    /// local to the device that owns the append-only journal.
    pub fn file_tool_input(
        &self,
        chat_id: &str,
        tool_call_id: &str,
        max_bytes: usize,
    ) -> Result<Option<FileToolInputSnapshot>, JournalError> {
        self.file_tool_input_scoped(chat_id, tool_call_id, None, max_bytes)
    }

    pub fn file_tool_input_scoped(
        &self,
        chat_id: &str,
        tool_call_id: &str,
        parent_tool_use_id: Option<&str>,
        max_bytes: usize,
    ) -> Result<Option<FileToolInputSnapshot>, JournalError> {
        let events = self.replay(chat_id, 0)?;
        let mut authoritative_diff: Option<ToolDiff> = None;
        let mut path_only_fallback: Option<FileToolInputSnapshot> = None;
        for (_, event) in events.into_iter().rev() {
            let event = match (parent_tool_use_id, event) {
                (None, AgentEvent::Subagent { .. }) => continue,
                (None, event) => event,
                (Some(scope), event) => match event_in_subagent_scope(event, scope) {
                    Some(event) => event,
                    None => continue,
                },
            };
            match event {
                AgentEvent::ToolResult {
                    id,
                    diff: Some(diff),
                    ..
                } if id == tool_call_id && authoritative_diff.is_none() => {
                    authoritative_diff = Some(diff);
                }
                AgentEvent::ToolCall { id, call } if id == tool_call_id => {
                    let (mut snapshot, has_complete_body) = match call {
                        ToolCall::WriteFile { path, content } => {
                            let has_complete_body = content.is_some();
                            (
                                FileToolInputSnapshot {
                                    path,
                                    content,
                                    old_string: None,
                                    new_string: None,
                                    truncated: false,
                                },
                                has_complete_body,
                            )
                        }
                        ToolCall::EditFile {
                            path,
                            old_string,
                            new_string,
                        } => {
                            let has_complete_body = old_string.is_some() && new_string.is_some();
                            (
                                FileToolInputSnapshot {
                                    path,
                                    content: None,
                                    old_string,
                                    new_string,
                                    truncated: false,
                                },
                                has_complete_body,
                            )
                        }
                        _ => return Ok(None),
                    };
                    if let Some(diff) = authoritative_diff.take() {
                        snapshot.path = diff.path;
                        snapshot.content = None;
                        snapshot.old_string = diff.old_text;
                        snapshot.new_string = Some(diff.new_text);
                        cap_file_snapshot_serialized(&mut snapshot, max_bytes);
                        return Ok(Some(snapshot));
                    }
                    if has_complete_body {
                        cap_file_snapshot_serialized(&mut snapshot, max_bytes);
                        return Ok(Some(snapshot));
                    }
                    path_only_fallback.get_or_insert(snapshot);
                }
                _ => {}
            }
        }
        if let Some(mut snapshot) = path_only_fallback {
            cap_file_snapshot_serialized(&mut snapshot, max_bytes);
            Ok(Some(snapshot))
        } else {
            Ok(None)
        }
    }

    /// Crash-recovery scan: chat ids whose journal's last event is NOT a `Done` — their
    /// runs died mid-stream and need recovery (stamp `aborted`, close the journal).
    pub fn stale_sessions(&self) -> Result<Vec<String>, JournalError> {
        let mut stale = Vec::new();
        for entry in std::fs::read_dir(&self.dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("jsonl") {
                continue;
            }
            let Some(chat_id) = path.file_stem().and_then(|s| s.to_str()) else {
                continue;
            };
            let last = read_lines(&path)?.into_iter().next_back();
            match last {
                Some((_, AgentEvent::Done { .. })) | None => {}
                Some(_) => stale.push(chat_id.to_string()),
            }
        }
        stale.sort();
        Ok(stale)
    }

    /// Remove a chat's journal file entirely (tests / future compaction).
    pub fn discard(&self, chat_id: &str) -> Result<(), JournalError> {
        self.lock().remove(chat_id);
        let path = self.path_for(chat_id);
        if path.exists() {
            std::fs::remove_file(path)?;
        }
        Ok(())
    }
}

fn event_in_subagent_scope(event: AgentEvent, scope: &str) -> Option<AgentEvent> {
    match event {
        AgentEvent::Subagent {
            parent_tool_use_id,
            event,
        } if parent_tool_use_id == scope => Some(*event),
        AgentEvent::Subagent { event, .. } => event_in_subagent_scope(*event, scope),
        _ => None,
    }
}

fn truncate_utf8_bytes(value: &mut String, max_bytes: usize) -> bool {
    if value.len() <= max_bytes {
        return false;
    }
    let mut end = max_bytes.min(value.len());
    while end > 0 && !value.is_char_boundary(end) {
        end -= 1;
    }
    value.truncate(end);
    true
}

fn cap_file_snapshot(snapshot: &mut FileToolInputSnapshot, max_bytes: usize) {
    if let Some(content) = snapshot.content.as_mut() {
        snapshot.truncated |= truncate_utf8_bytes(content, max_bytes);
        return;
    }

    match (snapshot.old_string.as_mut(), snapshot.new_string.as_mut()) {
        (Some(old), Some(new)) => {
            let total = old.len().saturating_add(new.len());
            if total <= max_bytes {
                return;
            }
            let old_budget = if total == 0 {
                0
            } else {
                ((max_bytes as u128 * old.len() as u128) / total as u128) as usize
            };
            let new_budget = max_bytes.saturating_sub(old_budget);
            snapshot.truncated |= truncate_utf8_bytes(old, old_budget);
            snapshot.truncated |= truncate_utf8_bytes(new, new_budget);
        }
        (Some(old), None) => snapshot.truncated |= truncate_utf8_bytes(old, max_bytes),
        (None, Some(new)) => snapshot.truncated |= truncate_utf8_bytes(new, max_bytes),
        (None, None) => {}
    }
}

fn cap_file_snapshot_serialized(snapshot: &mut FileToolInputSnapshot, max_bytes: usize) {
    cap_file_snapshot(snapshot, max_bytes);
    loop {
        let serialized = serde_json::to_vec(snapshot).map_or(usize::MAX, |value| value.len());
        if serialized <= max_bytes {
            break;
        }
        let overflow = serialized.saturating_sub(max_bytes);
        // JSON escaping expands one input byte by at most six bytes (`\u00XX`).
        // Remove the minimum safe raw chunk and remeasure the actual envelope.
        let reduction = overflow.div_ceil(6).max(1);
        let lengths = [
            snapshot.path.len(),
            snapshot.content.as_ref().map_or(0, String::len),
            snapshot.old_string.as_ref().map_or(0, String::len),
            snapshot.new_string.as_ref().map_or(0, String::len),
        ];
        let Some((field, length)) = lengths
            .into_iter()
            .enumerate()
            .max_by_key(|(_, length)| *length)
        else {
            break;
        };
        if length == 0 {
            break;
        }
        let budget = length.saturating_sub(reduction);
        let truncated = match field {
            0 => truncate_utf8_bytes(&mut snapshot.path, budget),
            1 => snapshot
                .content
                .as_mut()
                .is_some_and(|value| truncate_utf8_bytes(value, budget)),
            2 => snapshot
                .old_string
                .as_mut()
                .is_some_and(|value| truncate_utf8_bytes(value, budget)),
            _ => snapshot
                .new_string
                .as_mut()
                .is_some_and(|value| truncate_utf8_bytes(value, budget)),
        };
        snapshot.truncated |= truncated;
        if !truncated {
            break;
        }
    }
}

/// Parse every valid line; malformed lines (torn tail writes) are skipped.
fn read_lines(path: &Path) -> Result<Vec<(u64, AgentEvent)>, JournalError> {
    let file = match File::open(path) {
        Ok(f) => f,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(e.into()),
    };
    let mut out = Vec::new();
    for line in BufReader::new(file).lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        match serde_json::from_str::<JournalLine>(&line) {
            Ok(parsed) => out.push((parsed.seq, parsed.event)),
            Err(err) => {
                tracing::warn!(path = %path.display(), error = %err, "journal: skipping malformed line");
            }
        }
    }
    Ok(out)
}

/// Next seq (last valid seq + 1, starting at 1) and whether the file ends mid-line.
fn scan_tail(path: &Path) -> Result<(u64, bool), JournalError> {
    let bytes = match std::fs::read(path) {
        Ok(b) => b,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok((1, false)),
        Err(e) => return Err(e.into()),
    };
    let needs_newline = bytes.last().is_some_and(|b| *b != b'\n');
    let next_seq = read_lines(path)?
        .last()
        .map(|(seq, _)| seq + 1)
        .unwrap_or(1);
    Ok((next_seq, needs_newline))
}

/// Journal (`.jsonl`) and resume-budget (`.resume`) paths for `chat_id` under an
/// arbitrary journals directory — profile import copies these files between
/// profiles without opening a `RunJournal`.
pub fn journal_paths(dir: &Path, chat_id: &str) -> (PathBuf, PathBuf) {
    let stem = sanitize_id(chat_id);
    (
        dir.join(format!("{stem}.jsonl")),
        dir.join(format!("{stem}.resume")),
    )
}

/// Chat ids become file names; anything outside a conservative set is replaced so a
/// hostile id cannot traverse paths. (Ids are uuids in practice.)
fn sanitize_id(chat_id: &str) -> String {
    chat_id
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '-' | '_') {
                c
            } else {
                '_'
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use zeron_proto::DoneStatus;

    fn text(s: &str) -> AgentEvent {
        AgentEvent::TextDelta { text: s.into() }
    }

    fn done() -> AgentEvent {
        AgentEvent::Done {
            status: DoneStatus::Completed,
            result: None,
            error: None,
            session_id: None,
        }
    }

    #[test]
    fn appends_are_monotonic_and_replayable() {
        let dir = tempfile::tempdir().unwrap();
        let journal = RunJournal::open(dir.path()).unwrap();
        assert_eq!(journal.append("chat-1", &text("a")).unwrap(), 1);
        assert_eq!(journal.append("chat-1", &text("b")).unwrap(), 2);
        assert_eq!(journal.append("chat-1", &done()).unwrap(), 3);

        let all = journal.replay("chat-1", 0).unwrap();
        assert_eq!(all.len(), 3);
        let after = journal.replay("chat-1", 2).unwrap();
        assert_eq!(after.len(), 1);
        assert!(matches!(after[0].1, AgentEvent::Done { .. }));
        // Era fallback: cursor ahead of last seq replays everything.
        assert_eq!(journal.replay("chat-1", 99).unwrap().len(), 3);
    }

    #[test]
    fn seq_continues_across_reopen() {
        let dir = tempfile::tempdir().unwrap();
        {
            let journal = RunJournal::open(dir.path()).unwrap();
            journal.append("chat-1", &text("a")).unwrap();
        }
        let journal = RunJournal::open(dir.path()).unwrap();
        assert_eq!(journal.append("chat-1", &text("b")).unwrap(), 2);
    }

    #[test]
    fn stale_scan_flags_journals_without_terminal_done() {
        let dir = tempfile::tempdir().unwrap();
        let journal = RunJournal::open(dir.path()).unwrap();
        journal.append("dead", &text("partial")).unwrap();
        journal.append("clean", &text("full")).unwrap();
        journal.append("clean", &done()).unwrap();
        assert_eq!(journal.stale_sessions().unwrap(), vec!["dead".to_string()]);
        // Closing the stale journal with a Done clears the flag.
        journal.append("dead", &done()).unwrap();
        assert!(journal.stale_sessions().unwrap().is_empty());
    }

    #[test]
    fn torn_tail_line_is_tolerated() {
        let dir = tempfile::tempdir().unwrap();
        {
            let journal = RunJournal::open(dir.path()).unwrap();
            journal.append("chat-1", &text("a")).unwrap();
        }
        // Simulate a crash mid-write: garbage with no trailing newline.
        let path = dir.path().join("chat-1.jsonl");
        let mut f = OpenOptions::new().append(true).open(&path).unwrap();
        f.write_all(b"{\"seq\":2,\"event\":{\"type\":\"textD")
            .unwrap();
        drop(f);

        let journal = RunJournal::open(dir.path()).unwrap();
        assert_eq!(journal.replay("chat-1", 0).unwrap().len(), 1);
        assert_eq!(journal.append("chat-1", &text("b")).unwrap(), 2);
        let all = journal.replay("chat-1", 0).unwrap();
        assert_eq!(all.len(), 2);
        assert_eq!(all[1].0, 2);
    }

    #[test]
    fn file_tool_input_returns_newest_progressive_write_snapshot() {
        let dir = tempfile::tempdir().unwrap();
        let journal = RunJournal::open(dir.path()).unwrap();
        for content in ["first", "first\nsecond"] {
            journal
                .append(
                    "chat",
                    &AgentEvent::ToolCall {
                        id: "write-1".into(),
                        call: zeron_proto::ToolCall::WriteFile {
                            path: "notes/new.txt".into(),
                            content: Some(content.into()),
                        },
                    },
                )
                .unwrap();
        }
        journal.append("chat", &done()).unwrap();

        let snapshot = journal
            .file_tool_input("chat", "write-1", 1_048_576)
            .unwrap()
            .unwrap();
        assert_eq!(snapshot.path, "notes/new.txt");
        assert_eq!(snapshot.content.as_deref(), Some("first\nsecond"));
        assert!(!snapshot.truncated);
    }

    #[test]
    fn file_tool_input_keeps_richer_progressive_body_after_path_only_finalize() {
        let dir = tempfile::tempdir().unwrap();
        let journal = RunJournal::open(dir.path()).unwrap();
        for content in [Some("first\nsecond"), None] {
            journal
                .append(
                    "chat",
                    &AgentEvent::ToolCall {
                        id: "write-1".into(),
                        call: zeron_proto::ToolCall::WriteFile {
                            path: "notes/new.txt".into(),
                            content: content.map(str::to_owned),
                        },
                    },
                )
                .unwrap();
        }

        let snapshot = journal
            .file_tool_input("chat", "write-1", 1_048_576)
            .unwrap()
            .unwrap();
        assert_eq!(snapshot.content.as_deref(), Some("first\nsecond"));
    }

    #[test]
    fn file_tool_input_supports_edit_and_authoritative_result_diff() {
        let dir = tempfile::tempdir().unwrap();
        let journal = RunJournal::open(dir.path()).unwrap();
        journal
            .append(
                "chat",
                &AgentEvent::ToolCall {
                    id: "edit-1".into(),
                    call: zeron_proto::ToolCall::EditFile {
                        path: "src/main.rs".into(),
                        old_string: Some("before".into()),
                        new_string: Some("speculative".into()),
                    },
                },
            )
            .unwrap();
        journal
            .append(
                "chat",
                &AgentEvent::ToolResult {
                    id: "edit-1".into(),
                    is_error: false,
                    output: None,
                    diff: Some(zeron_proto::ToolDiff {
                        path: "src/main.rs".into(),
                        old_text: Some("before".into()),
                        new_text: "authoritative".into(),
                    }),
                    execution: None,
                },
            )
            .unwrap();

        let snapshot = journal
            .file_tool_input("chat", "edit-1", 1_048_576)
            .unwrap()
            .unwrap();
        assert_eq!(snapshot.content, None);
        assert_eq!(snapshot.old_string.as_deref(), Some("before"));
        assert_eq!(snapshot.new_string.as_deref(), Some("authoritative"));
    }

    #[test]
    fn file_tool_input_rejects_non_file_calls_and_missing_ids() {
        let dir = tempfile::tempdir().unwrap();
        let journal = RunJournal::open(dir.path()).unwrap();
        journal
            .append(
                "chat",
                &AgentEvent::ToolCall {
                    id: "exec-1".into(),
                    call: zeron_proto::ToolCall::Exec {
                        command: "cat secret".into(),
                    },
                },
            )
            .unwrap();
        assert_eq!(
            journal
                .file_tool_input("chat", "exec-1", 1_048_576)
                .unwrap(),
            None
        );
        assert_eq!(
            journal
                .file_tool_input("chat", "missing", 1_048_576)
                .unwrap(),
            None
        );
    }

    #[test]
    fn file_tool_input_scopes_nested_subagent_tool_ids_by_parent_spawn() {
        let dir = tempfile::tempdir().unwrap();
        let journal = RunJournal::open(dir.path()).unwrap();
        for (parent, content) in [("spawn-a", "alpha"), ("spawn-b", "beta")] {
            journal
                .append(
                    "chat",
                    &AgentEvent::Subagent {
                        parent_tool_use_id: parent.into(),
                        event: Box::new(AgentEvent::ToolCall {
                            id: "write-1".into(),
                            call: zeron_proto::ToolCall::WriteFile {
                                path: "notes/new.txt".into(),
                                content: Some(content.into()),
                            },
                        }),
                    },
                )
                .unwrap();
        }
        journal
            .append(
                "chat",
                &AgentEvent::Subagent {
                    parent_tool_use_id: "spawn-root".into(),
                    event: Box::new(AgentEvent::Subagent {
                        parent_tool_use_id: "spawn-nested".into(),
                        event: Box::new(AgentEvent::ToolCall {
                            id: "write-1".into(),
                            call: zeron_proto::ToolCall::WriteFile {
                                path: "notes/new.txt".into(),
                                content: Some("gamma".into()),
                            },
                        }),
                    }),
                },
            )
            .unwrap();

        let snapshot = journal
            .file_tool_input_scoped("chat", "write-1", Some("spawn-a"), 1_048_576)
            .unwrap()
            .unwrap();
        assert_eq!(snapshot.content.as_deref(), Some("alpha"));
        let nested = journal
            .file_tool_input_scoped("chat", "write-1", Some("spawn-nested"), 1_048_576)
            .unwrap()
            .unwrap();
        assert_eq!(nested.content.as_deref(), Some("gamma"));
        assert_eq!(
            journal
                .file_tool_input("chat", "write-1", 1_048_576)
                .unwrap(),
            None,
            "unscoped parent transcript lookup must not leak nested input"
        );
    }

    #[test]
    fn file_tool_input_caps_utf8_content_without_splitting_a_scalar() {
        let dir = tempfile::tempdir().unwrap();
        let journal = RunJournal::open(dir.path()).unwrap();
        journal
            .append(
                "chat",
                &AgentEvent::ToolCall {
                    id: "write-1".into(),
                    call: zeron_proto::ToolCall::WriteFile {
                        path: "wide.txt".into(),
                        content: Some("é".repeat(100)),
                    },
                },
            )
            .unwrap();

        let snapshot = journal
            .file_tool_input("chat", "write-1", 128)
            .unwrap()
            .unwrap();
        let content = snapshot.content.unwrap();
        assert!(snapshot.truncated);
        assert!(!content.is_empty());
        assert!(content.len() <= 128);
        assert!(content.is_char_boundary(content.len()));
    }

    #[test]
    fn file_tool_input_enforces_the_one_mib_response_budget() {
        let dir = tempfile::tempdir().unwrap();
        let journal = RunJournal::open(dir.path()).unwrap();
        journal
            .append(
                "chat",
                &AgentEvent::ToolCall {
                    id: "write-large".into(),
                    call: zeron_proto::ToolCall::WriteFile {
                        path: "large.txt".into(),
                        content: Some("é".repeat(600_000)),
                    },
                },
            )
            .unwrap();

        let snapshot = journal
            .file_tool_input("chat", "write-large", 1_048_576)
            .unwrap()
            .unwrap();
        let content = snapshot.content.unwrap();
        assert!(snapshot.truncated);
        assert!(content.len() <= 1_048_576);
        assert!(content.is_char_boundary(content.len()));
    }

    #[test]
    fn file_tool_input_caps_the_serialized_snapshot_not_only_raw_body_bytes() {
        let dir = tempfile::tempdir().unwrap();
        let journal = RunJournal::open(dir.path()).unwrap();
        journal
            .append(
                "chat",
                &AgentEvent::ToolCall {
                    id: "write-escaped".into(),
                    call: zeron_proto::ToolCall::WriteFile {
                        path: "escaped.txt".into(),
                        content: Some("\u{0001}".repeat(1_000)),
                    },
                },
            )
            .unwrap();

        let snapshot = journal
            .file_tool_input("chat", "write-escaped", 1_024)
            .unwrap()
            .unwrap();
        assert!(snapshot.truncated);
        assert!(serde_json::to_vec(&snapshot).unwrap().len() <= 1_024);
    }
}
