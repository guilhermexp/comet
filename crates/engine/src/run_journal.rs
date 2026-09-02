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
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard, PoisonError};

use serde::{Deserialize, Serialize};

use zeron_proto::{AgentEvent, FileToolInputSnapshot, ToolCall, ToolDiff};
use zeron_rpc::{TrajectoryRawField, TrajectoryRawRevealResult, TrajectoryUnavailableReason};

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

#[derive(Debug, Default)]
struct ReverseScanStats {
    lines_scanned: usize,
    max_buffer_bytes: usize,
    oversized_line: bool,
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

    pub fn path_for(&self, chat_id: &str) -> PathBuf {
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
        Ok(self
            .file_tool_input_scoped_impl(chat_id, tool_call_id, parent_tool_use_id, max_bytes)?
            .0)
    }

    /// Locate and extract a raw field from the local Run Journal entry.
    ///
    /// Validates `source_seq`, unwrap subagents if `parent_tool_use_id` is provided,
    /// checks matching `call_id`, coalesces streaming delta sequences for assistant/reasoning,
    /// and safely extracts the requested `Payload` or `Result`.
    pub fn raw_reveal(
        &self,
        chat_id: &str,
        source_seq: u64,
        parent_tool_use_id: Option<&str>,
        call_id: Option<&str>,
        field: TrajectoryRawField,
    ) -> Result<TrajectoryRawRevealResult, JournalError> {
        // B3: Refuse resolution if the chat_id is not canonically representable in journal storage.
        // Non-injective sanitization (e.g. "a/b" -> "a_b") aliasing a shadow Chat's journal must
        // never disclose raw journal content from another Chat.
        if sanitize_id(chat_id) != chat_id {
            return Ok(TrajectoryRawRevealResult::unavailable(
                field,
                TrajectoryUnavailableReason::NotFound,
                Some("Chat ID is not canonically representable in journal storage".into()),
            ));
        }

        let path = self.path_for(chat_id);
        if !path.exists() {
            return Ok(TrajectoryRawRevealResult::unavailable(
                field,
                TrajectoryUnavailableReason::NotFound,
                Some("Journal file not found".into()),
            ));
        }

        let file = match std::fs::File::open(&path) {
            Ok(f) => f,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return Ok(TrajectoryRawRevealResult::unavailable(
                    field,
                    TrajectoryUnavailableReason::NotFound,
                    Some("Journal file not found".into()),
                ));
            }
            Err(e) => return Err(e.into()),
        };

        let mut reader = std::io::BufReader::new(file);
        let mut line_buf = Vec::new();
        let mut had_corrupt_line = false;
        let mut lines_scanned = 0usize;
        let mut remaining_bytes_budget = MAX_RAW_REVEAL_SCAN_BYTES;

        loop {
            lines_scanned += 1;
            if lines_scanned > MAX_RAW_REVEAL_SCAN_LINES {
                return Ok(TrajectoryRawRevealResult::unavailable(
                    field,
                    TrajectoryUnavailableReason::SourceOversized,
                    Some("Journal scan line budget exceeded".into()),
                ));
            }

            match read_bounded_line(
                &mut reader,
                &mut line_buf,
                MAX_REVERSE_SCAN_LINE_BYTES,
                &mut remaining_bytes_budget,
            )? {
                BoundedLineRead::BudgetExceeded | BoundedLineRead::Oversized => {
                    return Ok(TrajectoryRawRevealResult::unavailable(
                        field,
                        TrajectoryUnavailableReason::SourceOversized,
                        Some("Journal source line or scan budget exceeds limits".into()),
                    ));
                }
                BoundedLineRead::Eof => {
                    break;
                }
                BoundedLineRead::Line => {}
            }

            if line_buf.iter().all(u8::is_ascii_whitespace) {
                continue;
            }

            let parsed: JournalLine = match serde_json::from_slice(&line_buf) {
                Ok(p) => p,
                Err(err) => {
                    tracing::warn!(
                        path = %path.display(),
                        error = %err,
                        "journal: raw_reveal encountered malformed line"
                    );
                    had_corrupt_line = true;
                    continue;
                }
            };

            if parsed.seq == source_seq {
                let event = match (parent_tool_use_id, parsed.event) {
                    (None, AgentEvent::Subagent { .. }) => {
                        return Ok(TrajectoryRawRevealResult::unavailable(
                            field,
                            TrajectoryUnavailableReason::MismatchedReference,
                            Some(
                                "Event is a nested subagent event but no parentToolUseId provided"
                                    .into(),
                            ),
                        ));
                    }
                    (None, event) => event,
                    (Some(scope), event) => match event_in_subagent_scope(event, scope) {
                        Some(event) => event,
                        None => {
                            return Ok(TrajectoryRawRevealResult::unavailable(
                                field,
                                TrajectoryUnavailableReason::MismatchedReference,
                                Some("Subagent parentToolUseId scope not found in event".into()),
                            ));
                        }
                    },
                };

                match field {
                    TrajectoryRawField::Payload => {
                        match event {
                            AgentEvent::TextDelta { text } => {
                                let mut accumulated = text;
                                let mut expected_seq = source_seq + 1;
                                loop {
                                    lines_scanned += 1;
                                    if lines_scanned > MAX_RAW_REVEAL_SCAN_LINES {
                                        return Ok(TrajectoryRawRevealResult::unavailable(
                                        field,
                                        TrajectoryUnavailableReason::SourceOversized,
                                        Some("Journal scan line budget exceeded during coalescing".into()),
                                    ));
                                    }

                                    match read_bounded_line(
                                        &mut reader,
                                        &mut line_buf,
                                        MAX_REVERSE_SCAN_LINE_BYTES,
                                        &mut remaining_bytes_budget,
                                    )? {
                                        BoundedLineRead::BudgetExceeded
                                        | BoundedLineRead::Oversized => {
                                            return Ok(TrajectoryRawRevealResult::unavailable(
                                            field,
                                            TrajectoryUnavailableReason::SourceOversized,
                                            Some("Continuation line exceeds maximum line size or budget".into()),
                                        ));
                                        }
                                        BoundedLineRead::Eof => {
                                            break;
                                        }
                                        BoundedLineRead::Line => {}
                                    }

                                    if line_buf.iter().all(u8::is_ascii_whitespace) {
                                        continue;
                                    }

                                    let next_parsed: JournalLine =
                                        match serde_json::from_slice(&line_buf) {
                                            Ok(p) => p,
                                            Err(_) => {
                                                return Ok(TrajectoryRawRevealResult::unavailable(
                                                    field,
                                                    TrajectoryUnavailableReason::SourceCorrupt,
                                                    Some(
                                                        "Malformed continuation line in journal"
                                                            .into(),
                                                    ),
                                                ));
                                            }
                                        };

                                    if next_parsed.seq != expected_seq {
                                        return Ok(TrajectoryRawRevealResult::unavailable(
                                            field,
                                            TrajectoryUnavailableReason::SourceCorrupt,
                                            Some(format!(
                                                "Sequence gap in text delta continuation: expected {expected_seq}, got {}",
                                                next_parsed.seq
                                            )),
                                        ));
                                    }

                                    let next_event = match (parent_tool_use_id, next_parsed.event) {
                                        (None, AgentEvent::Subagent { .. }) => break,
                                        (None, event) => event,
                                        (Some(scope), event) => {
                                            match event_in_subagent_scope(event, scope) {
                                                Some(ev) => ev,
                                                None => break,
                                            }
                                        }
                                    };
                                    if let AgentEvent::TextDelta { text: next_text } = next_event {
                                        accumulated.push_str(&next_text);
                                        expected_seq += 1;
                                        if accumulated.len() >= MAX_RAW_REVEAL_BYTES {
                                            break;
                                        }
                                    } else {
                                        // Legitimate completion of TextDelta sequence
                                        break;
                                    }
                                }
                                return Ok(TrajectoryRawRevealResult::available(
                                    TrajectoryRawField::Payload,
                                    cap_reveal_text(accumulated),
                                ));
                            }
                            AgentEvent::ReasoningDelta { text } => {
                                let mut accumulated = text;
                                let mut expected_seq = source_seq + 1;
                                loop {
                                    lines_scanned += 1;
                                    if lines_scanned > MAX_RAW_REVEAL_SCAN_LINES {
                                        return Ok(TrajectoryRawRevealResult::unavailable(
                                        field,
                                        TrajectoryUnavailableReason::SourceOversized,
                                        Some("Journal scan line budget exceeded during coalescing".into()),
                                    ));
                                    }

                                    match read_bounded_line(
                                        &mut reader,
                                        &mut line_buf,
                                        MAX_REVERSE_SCAN_LINE_BYTES,
                                        &mut remaining_bytes_budget,
                                    )? {
                                        BoundedLineRead::BudgetExceeded
                                        | BoundedLineRead::Oversized => {
                                            return Ok(TrajectoryRawRevealResult::unavailable(
                                            field,
                                            TrajectoryUnavailableReason::SourceOversized,
                                            Some("Continuation line exceeds maximum line size or budget".into()),
                                        ));
                                        }
                                        BoundedLineRead::Eof => {
                                            break;
                                        }
                                        BoundedLineRead::Line => {}
                                    }

                                    if line_buf.iter().all(u8::is_ascii_whitespace) {
                                        continue;
                                    }

                                    let next_parsed: JournalLine =
                                        match serde_json::from_slice(&line_buf) {
                                            Ok(p) => p,
                                            Err(_) => {
                                                return Ok(TrajectoryRawRevealResult::unavailable(
                                                    field,
                                                    TrajectoryUnavailableReason::SourceCorrupt,
                                                    Some(
                                                        "Malformed continuation line in journal"
                                                            .into(),
                                                    ),
                                                ));
                                            }
                                        };

                                    if next_parsed.seq != expected_seq {
                                        return Ok(TrajectoryRawRevealResult::unavailable(
                                            field,
                                            TrajectoryUnavailableReason::SourceCorrupt,
                                            Some(format!(
                                                "Sequence gap in reasoning delta continuation: expected {expected_seq}, got {}",
                                                next_parsed.seq
                                            )),
                                        ));
                                    }

                                    let next_event = match (parent_tool_use_id, next_parsed.event) {
                                        (None, AgentEvent::Subagent { .. }) => break,
                                        (None, event) => event,
                                        (Some(scope), event) => {
                                            match event_in_subagent_scope(event, scope) {
                                                Some(ev) => ev,
                                                None => break,
                                            }
                                        }
                                    };
                                    if let AgentEvent::ReasoningDelta { text: next_text } =
                                        next_event
                                    {
                                        accumulated.push_str(&next_text);
                                        expected_seq += 1;
                                        if accumulated.len() >= MAX_RAW_REVEAL_BYTES {
                                            break;
                                        }
                                    } else {
                                        // Legitimate completion of ReasoningDelta sequence
                                        break;
                                    }
                                }
                                return Ok(TrajectoryRawRevealResult::available(
                                    TrajectoryRawField::Payload,
                                    cap_reveal_text(accumulated),
                                ));
                            }
                            _ => {
                                return Ok(extract_raw_payload(&event, call_id));
                            }
                        }
                    }
                    TrajectoryRawField::Result => {
                        return Ok(extract_raw_result(&event, call_id));
                    }
                }
            }

            if parsed.seq > source_seq {
                return Ok(scan_missed_sequence(field, had_corrupt_line));
            }
        }

        Ok(scan_missed_sequence(field, had_corrupt_line))
    }

    fn file_tool_input_scoped_impl(
        &self,
        chat_id: &str,
        tool_call_id: &str,
        parent_tool_use_id: Option<&str>,
        max_bytes: usize,
    ) -> Result<(Option<FileToolInputSnapshot>, ReverseScanStats), JournalError> {
        let path = self.path_for(chat_id);
        let mut authoritative_diff: Option<ToolDiff> = None;
        let mut path_only_fallback: Option<FileToolInputSnapshot> = None;
        let mut resolved: Option<Option<FileToolInputSnapshot>> = None;
        let stats = scan_lines_reverse_until(&path, |line| {
            let parsed = match serde_json::from_slice::<JournalLine>(line) {
                Ok(parsed) => parsed,
                Err(err) => {
                    tracing::warn!(path = %path.display(), error = %err, "journal: skipping malformed line");
                    return Ok(false);
                }
            };
            let event = parsed.event;
            let event = match (parent_tool_use_id, event) {
                (None, AgentEvent::Subagent { .. }) => return Ok(false),
                (None, event) => event,
                (Some(scope), event) => match event_in_subagent_scope(event, scope) {
                    Some(event) => event,
                    None => return Ok(false),
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
                        _ => {
                            resolved = Some(None);
                            return Ok(true);
                        }
                    };
                    if let Some(diff) = authoritative_diff.take() {
                        snapshot.path = diff.path;
                        snapshot.content = None;
                        snapshot.old_string = diff.old_text;
                        snapshot.new_string = Some(diff.new_text);
                        cap_file_snapshot_serialized(&mut snapshot, max_bytes);
                        resolved = Some(Some(snapshot));
                        return Ok(true);
                    }
                    if has_complete_body {
                        cap_file_snapshot_serialized(&mut snapshot, max_bytes);
                        resolved = Some(Some(snapshot));
                        return Ok(true);
                    }
                    path_only_fallback.get_or_insert(snapshot);
                }
                _ => {}
            }
            Ok(false)
        })?;
        let snapshot = if stats.oversized_line {
            None
        } else if let Some(resolved) = resolved {
            resolved
        } else if let Some(mut snapshot) = path_only_fallback {
            cap_file_snapshot_serialized(&mut snapshot, max_bytes);
            Some(snapshot)
        } else {
            None
        };
        Ok((snapshot, stats))
    }

    #[cfg(test)]
    fn file_tool_input_scoped_with_stats(
        &self,
        chat_id: &str,
        tool_call_id: &str,
        parent_tool_use_id: Option<&str>,
        max_bytes: usize,
    ) -> Result<(Option<FileToolInputSnapshot>, ReverseScanStats), JournalError> {
        self.file_tool_input_scoped_impl(chat_id, tool_call_id, parent_tool_use_id, max_bytes)
    }

    /// Crash-recovery scan: chat ids whose journal's last event is NOT a `Done` — their
    /// runs died mid-stream and need recovery (stamp `aborted`, close the journal).
    pub fn stale_sessions(&self) -> Result<Vec<String>, JournalError> {
        let mut stale = Vec::new();
        for (chat_id, path) in self.journal_files()? {
            let last = read_lines(&path)?.into_iter().next_back();
            match last {
                Some((_, AgentEvent::Done { .. })) | None => {}
                Some(_) => stale.push(chat_id),
            }
        }
        stale.sort();
        Ok(stale)
    }

    /// Every chat this device journaled here, `Done`-closed or not. A closed
    /// journal does NOT prove the doc settled — the journal append is
    /// synchronous while the doc fold lands later — so boot recovery sweeps
    /// this wider set for abandoned `streaming` entries.
    pub fn journaled_chats(&self) -> Result<Vec<String>, JournalError> {
        let mut chats: Vec<String> = self
            .journal_files()?
            .into_iter()
            .map(|(chat_id, _)| chat_id)
            .collect();
        chats.sort();
        Ok(chats)
    }

    fn journal_files(&self) -> Result<Vec<(String, PathBuf)>, JournalError> {
        let mut files = Vec::new();
        for entry in std::fs::read_dir(&self.dir)? {
            let path = entry?.path();
            if path.extension().and_then(|e| e.to_str()) != Some("jsonl") {
                continue;
            }
            let Some(chat_id) = path.file_stem().and_then(|s| s.to_str()) else {
                continue;
            };
            files.push((chat_id.to_string(), path));
        }
        Ok(files)
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

const REVERSE_SCAN_CHUNK_BYTES: usize = 64 * 1024;
/// Supports the 1 MiB historical response even under worst-case `\u00XX`
/// JSON escaping, while placing an absolute ceiling on reverse-scan carry.
const MAX_REVERSE_SCAN_LINE_BYTES: usize = 8 * 1024 * 1024;
const MAX_RAW_REVEAL_SCAN_LINES: usize = 10_000;
const MAX_RAW_REVEAL_SCAN_BYTES: u64 = 64 * 1024 * 1024;

enum BoundedLineRead {
    Line,
    Oversized,
    BudgetExceeded,
    Eof,
}

fn read_bounded_line<R: std::io::BufRead>(
    reader: &mut R,
    line_buf: &mut Vec<u8>,
    max_line_bytes: usize,
    remaining_bytes_budget: &mut u64,
) -> Result<BoundedLineRead, std::io::Error> {
    line_buf.clear();
    let mut total_read = 0;
    let mut found_newline = false;

    while total_read <= max_line_bytes {
        if *remaining_bytes_budget == 0 {
            return Ok(BoundedLineRead::BudgetExceeded);
        }
        let available = reader.fill_buf()?;
        if available.is_empty() {
            break;
        }
        if let Some(pos) = available.iter().position(|&b| b == b'\n') {
            let chunk_len = pos;
            if line_buf.len().saturating_add(chunk_len) > max_line_bytes {
                reader.consume(pos + 1);
                *remaining_bytes_budget = remaining_bytes_budget.saturating_sub((pos + 1) as u64);
                return Ok(BoundedLineRead::Oversized);
            }
            if (chunk_len as u64) > *remaining_bytes_budget {
                return Ok(BoundedLineRead::BudgetExceeded);
            }
            line_buf.extend_from_slice(&available[..pos]);
            reader.consume(pos + 1);
            *remaining_bytes_budget = remaining_bytes_budget.saturating_sub((pos + 1) as u64);
            found_newline = true;
            break;
        } else {
            let len = available.len();
            if line_buf.len().saturating_add(len) > max_line_bytes {
                reader.consume(len);
                *remaining_bytes_budget = remaining_bytes_budget.saturating_sub(len as u64);
                loop {
                    let next = reader.fill_buf()?;
                    if next.is_empty() {
                        break;
                    }
                    if let Some(pos) = next.iter().position(|&b| b == b'\n') {
                        reader.consume(pos + 1);
                        *remaining_bytes_budget =
                            remaining_bytes_budget.saturating_sub((pos + 1) as u64);
                        break;
                    }
                    let next_len = next.len();
                    reader.consume(next_len);
                    *remaining_bytes_budget =
                        remaining_bytes_budget.saturating_sub(next_len as u64);
                }
                return Ok(BoundedLineRead::Oversized);
            }
            if (len as u64) > *remaining_bytes_budget {
                return Ok(BoundedLineRead::BudgetExceeded);
            }
            line_buf.extend_from_slice(available);
            reader.consume(len);
            *remaining_bytes_budget = remaining_bytes_budget.saturating_sub(len as u64);
            total_read += len;
        }
    }

    if line_buf.len() > max_line_bytes {
        return Ok(BoundedLineRead::Oversized);
    }
    if line_buf.is_empty() && !found_newline {
        return Ok(BoundedLineRead::Eof);
    }
    Ok(BoundedLineRead::Line)
}

fn scan_lines_reverse_until(
    path: &Path,
    mut visit: impl FnMut(&[u8]) -> Result<bool, JournalError>,
) -> Result<ReverseScanStats, JournalError> {
    let mut file = match File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(ReverseScanStats::default());
        }
        Err(error) => return Err(error.into()),
    };
    let mut position = file.metadata()?.len() as usize;
    let mut carry = Vec::new();
    let mut first_chunk = true;
    let mut stats = ReverseScanStats::default();

    while position > 0 {
        let chunk_len = position.min(REVERSE_SCAN_CHUNK_BYTES);
        position -= chunk_len;
        file.seek(SeekFrom::Start(position as u64))?;
        let mut data = vec![0; chunk_len];
        file.read_exact(&mut data)?;
        data.extend_from_slice(&carry);
        stats.max_buffer_bytes = stats.max_buffer_bytes.max(data.len());

        let mut end = data.len();
        if first_chunk && data.last() == Some(&b'\n') {
            end = end.saturating_sub(1);
        }
        first_chunk = false;
        while let Some(newline) = data[..end].iter().rposition(|byte| *byte == b'\n') {
            let line = &data[newline + 1..end];
            if line.len() > MAX_REVERSE_SCAN_LINE_BYTES {
                stats.oversized_line = true;
                return Ok(stats);
            }
            if !line.iter().all(u8::is_ascii_whitespace) {
                stats.lines_scanned += 1;
                if visit(line)? {
                    return Ok(stats);
                }
            }
            end = newline;
        }
        if end > MAX_REVERSE_SCAN_LINE_BYTES {
            stats.oversized_line = true;
            return Ok(stats);
        }
        carry.clear();
        carry.extend_from_slice(&data[..end]);
    }

    if !carry.iter().all(u8::is_ascii_whitespace) {
        stats.lines_scanned += 1;
        let _ = visit(&carry)?;
    }
    Ok(stats)
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

const MAX_RAW_REVEAL_BYTES: usize = 1024 * 1024;

fn cap_reveal_text(mut text: String) -> String {
    truncate_utf8_bytes(&mut text, MAX_RAW_REVEAL_BYTES);
    text
}

fn mismatched_call_id(
    field: TrajectoryRawField,
    expected_id: &str,
    journal_id: &str,
) -> TrajectoryRawRevealResult {
    tracing::debug!(
        expected_call_id = expected_id,
        journal_call_id = journal_id,
        "journal: raw reveal call id mismatch"
    );
    TrajectoryRawRevealResult::unavailable(
        field,
        TrajectoryUnavailableReason::MismatchedReference,
        Some("Raw reference does not match the journal event".into()),
    )
}

/// The scan passed `source_seq` (or ran out of lines) without matching. A corrupt line in the
/// scanned prefix means the record may well have been there but was unreadable, which the client
/// must be able to distinguish from a genuinely absent sequence.
fn scan_missed_sequence(
    field: TrajectoryRawField,
    had_corrupt_line: bool,
) -> TrajectoryRawRevealResult {
    if had_corrupt_line {
        TrajectoryRawRevealResult::unavailable(
            field,
            TrajectoryUnavailableReason::SourceCorrupt,
            Some("Journal file contains corrupt line".into()),
        )
    } else {
        TrajectoryRawRevealResult::unavailable(
            field,
            TrajectoryUnavailableReason::NotFound,
            Some("Event sequence not found in journal".into()),
        )
    }
}

fn extract_raw_payload(event: &AgentEvent, call_id: Option<&str>) -> TrajectoryRawRevealResult {
    match event {
        AgentEvent::SessionStarted {
            harness,
            model,
            tools,
            cwd,
            session_id,
            assistant_message_id,
        } => {
            let info = serde_json::json!({
                "harness": harness,
                "model": model,
                "tools": tools,
                "cwd": cwd,
                "sessionId": session_id,
                "assistantMessageId": assistant_message_id,
            });
            TrajectoryRawRevealResult::available(
                TrajectoryRawField::Payload,
                cap_reveal_text(serde_json::to_string_pretty(&info).unwrap_or_default()),
            )
        }
        AgentEvent::UserMessage { text } => TrajectoryRawRevealResult::available(
            TrajectoryRawField::Payload,
            cap_reveal_text(text.clone()),
        ),
        AgentEvent::TextDelta { text } => TrajectoryRawRevealResult::available(
            TrajectoryRawField::Payload,
            cap_reveal_text(text.clone()),
        ),
        AgentEvent::ReasoningDelta { text } => TrajectoryRawRevealResult::available(
            TrajectoryRawField::Payload,
            cap_reveal_text(text.clone()),
        ),
        AgentEvent::ToolCall { id, call } => {
            if let Some(expected_id) = call_id
                && id != expected_id
            {
                return mismatched_call_id(TrajectoryRawField::Payload, expected_id, id);
            }
            let raw_text = match call {
                ToolCall::WriteFile {
                    content: Some(c), ..
                } => c.clone(),
                ToolCall::WriteFile {
                    path,
                    content: None,
                } => format!("WriteFile: {path}"),
                ToolCall::EditFile {
                    old_string,
                    new_string,
                    path,
                } => serde_json::json!({
                    "path": path,
                    "oldString": old_string,
                    "newString": new_string,
                })
                .to_string(),
                other => {
                    serde_json::to_string_pretty(other).unwrap_or_else(|_| format!("{:?}", other))
                }
            };
            TrajectoryRawRevealResult::available(
                TrajectoryRawField::Payload,
                cap_reveal_text(raw_text),
            )
        }
        AgentEvent::ToolCallPreview { id, call } => {
            if let Some(expected_id) = call_id
                && id != expected_id
            {
                return mismatched_call_id(TrajectoryRawField::Payload, expected_id, id);
            }
            TrajectoryRawRevealResult::available(
                TrajectoryRawField::Payload,
                cap_reveal_text(
                    serde_json::to_string_pretty(call).unwrap_or_else(|_| format!("{:?}", call)),
                ),
            )
        }
        AgentEvent::InputRequested { questions, .. } => {
            let raw_text = serde_json::to_string_pretty(questions).unwrap_or_default();
            TrajectoryRawRevealResult::available(
                TrajectoryRawField::Payload,
                cap_reveal_text(raw_text),
            )
        }
        AgentEvent::Error { message } => TrajectoryRawRevealResult::available(
            TrajectoryRawField::Payload,
            cap_reveal_text(message.clone()),
        ),
        _ => TrajectoryRawRevealResult::unavailable(
            TrajectoryRawField::Payload,
            TrajectoryUnavailableReason::MismatchedReference,
            Some("Event does not have a raw payload field".into()),
        ),
    }
}

fn extract_raw_result(event: &AgentEvent, call_id: Option<&str>) -> TrajectoryRawRevealResult {
    match event {
        AgentEvent::ToolResult {
            id,
            output,
            diff,
            is_error: _,
            ..
        } => {
            if let Some(expected_id) = call_id
                && id != expected_id
            {
                return mismatched_call_id(TrajectoryRawField::Result, expected_id, id);
            }
            if let Some(diff) = diff {
                let diff_text =
                    serde_json::to_string_pretty(diff).unwrap_or_else(|_| format!("{:?}", diff));
                TrajectoryRawRevealResult::available(
                    TrajectoryRawField::Result,
                    cap_reveal_text(diff_text),
                )
            } else if let Some(out) = output {
                TrajectoryRawRevealResult::available(
                    TrajectoryRawField::Result,
                    cap_reveal_text(out.clone()),
                )
            } else {
                TrajectoryRawRevealResult::unavailable(
                    TrajectoryRawField::Result,
                    TrajectoryUnavailableReason::NotFound,
                    Some("Tool result has no raw output or diff".into()),
                )
            }
        }
        AgentEvent::Done {
            status: _,
            result,
            error,
            ..
        } => {
            if let Some(err) = error {
                TrajectoryRawRevealResult::available(
                    TrajectoryRawField::Result,
                    cap_reveal_text(err.clone()),
                )
            } else if let Some(res) = result {
                TrajectoryRawRevealResult::available(
                    TrajectoryRawField::Result,
                    cap_reveal_text(res.clone()),
                )
            } else {
                TrajectoryRawRevealResult::unavailable(
                    TrajectoryRawField::Result,
                    TrajectoryUnavailableReason::NotFound,
                    Some("Done event has no raw result or error text".into()),
                )
            }
        }
        AgentEvent::Error { message } => TrajectoryRawRevealResult::available(
            TrajectoryRawField::Result,
            cap_reveal_text(message.clone()),
        ),
        _ => TrajectoryRawRevealResult::unavailable(
            TrajectoryRawField::Result,
            TrajectoryUnavailableReason::MismatchedReference,
            Some("Event does not have a raw result field".into()),
        ),
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

    #[test]
    fn file_tool_input_reverse_scan_is_bounded_by_tail_not_journal_history() {
        let dir = tempfile::tempdir().unwrap();
        let journal = RunJournal::open(dir.path()).unwrap();
        for index in 0..2_048 {
            journal
                .append(
                    "chat",
                    &AgentEvent::TextDelta {
                        text: format!("old history {index}: {}", "x".repeat(80)),
                    },
                )
                .unwrap();
        }
        journal
            .append(
                "chat",
                &AgentEvent::ToolCall {
                    id: "write-tail".into(),
                    call: zeron_proto::ToolCall::WriteFile {
                        path: "tail.txt".into(),
                        content: Some("newest body".into()),
                    },
                },
            )
            .unwrap();
        journal.append("chat", &done()).unwrap();

        let (snapshot, stats) = journal
            .file_tool_input_scoped_with_stats("chat", "write-tail", None, 1_048_576)
            .unwrap();
        assert_eq!(snapshot.unwrap().content.as_deref(), Some("newest body"));
        assert!(
            stats.lines_scanned <= 2,
            "scanned {} lines",
            stats.lines_scanned
        );
        assert!(
            stats.max_buffer_bytes <= 128 * 1024,
            "buffered {} bytes for a much larger journal",
            stats.max_buffer_bytes
        );
        assert!(std::fs::metadata(journal.path_for("chat")).unwrap().len() > 200_000);
    }

    #[test]
    fn oversized_write_line_is_unavailable_without_unbounded_carry() {
        let dir = tempfile::tempdir().unwrap();
        let journal = RunJournal::open(dir.path()).unwrap();
        journal
            .append(
                "chat",
                &AgentEvent::ToolCall {
                    id: "write-huge".into(),
                    call: zeron_proto::ToolCall::WriteFile {
                        path: "huge.txt".into(),
                        content: Some("x".repeat(MAX_REVERSE_SCAN_LINE_BYTES + 1)),
                    },
                },
            )
            .unwrap();

        let (snapshot, stats) = journal
            .file_tool_input_scoped_with_stats("chat", "write-huge", None, 1_048_576)
            .unwrap();
        assert_eq!(snapshot, None);
        assert!(stats.oversized_line);
        assert!(stats.max_buffer_bytes <= MAX_REVERSE_SCAN_LINE_BYTES + REVERSE_SCAN_CHUNK_BYTES);
    }

    #[test]
    fn oversized_malformed_tail_blocks_older_fallback_without_unbounded_carry() {
        let dir = tempfile::tempdir().unwrap();
        let journal = RunJournal::open(dir.path()).unwrap();
        journal
            .append(
                "chat",
                &AgentEvent::ToolCall {
                    id: "write-1".into(),
                    call: zeron_proto::ToolCall::WriteFile {
                        path: "old.txt".into(),
                        content: Some("older body".into()),
                    },
                },
            )
            .unwrap();
        let mut file = OpenOptions::new()
            .append(true)
            .open(journal.path_for("chat"))
            .unwrap();
        file.write_all(b"{\"seq\":999,\"event\":\"").unwrap();
        file.write_all(&vec![b'x'; MAX_REVERSE_SCAN_LINE_BYTES + 1])
            .unwrap();
        file.flush().unwrap();

        let (snapshot, stats) = journal
            .file_tool_input_scoped_with_stats("chat", "write-1", None, 1_048_576)
            .unwrap();
        assert_eq!(snapshot, None, "must not fabricate the older body");
        assert!(stats.oversized_line);
        assert!(stats.max_buffer_bytes <= MAX_REVERSE_SCAN_LINE_BYTES + REVERSE_SCAN_CHUNK_BYTES);
    }

    #[test]
    fn raw_reveal_mismatched_call_id_does_not_disclose_identifiers() {
        let dir = tempfile::tempdir().unwrap();
        let journal = RunJournal::open(dir.path()).unwrap();
        let cases = [
            (
                "tool-call",
                AgentEvent::ToolCall {
                    id: "journal-secret-call-id".into(),
                    call: ToolCall::Exec {
                        command: "true".into(),
                    },
                },
                TrajectoryRawField::Payload,
            ),
            (
                "tool-preview",
                AgentEvent::ToolCallPreview {
                    id: "journal-secret-preview-id".into(),
                    call: ToolCall::Exec {
                        command: "true".into(),
                    },
                },
                TrajectoryRawField::Payload,
            ),
            (
                "tool-result",
                AgentEvent::ToolResult {
                    id: "journal-secret-result-id".into(),
                    is_error: false,
                    output: Some("ok".into()),
                    diff: None,
                    execution: None,
                },
                TrajectoryRawField::Result,
            ),
        ];

        for (chat_id, event, field) in cases {
            let seq = journal.append(chat_id, &event).unwrap();
            let result = journal
                .raw_reveal(chat_id, seq, None, Some("client-call-id"), field)
                .unwrap();
            match result {
                TrajectoryRawRevealResult::Unavailable {
                    reason, message, ..
                } => {
                    assert_eq!(reason, TrajectoryUnavailableReason::MismatchedReference);
                    assert_eq!(
                        message.as_deref(),
                        Some("Raw reference does not match the journal event")
                    );
                    let message = message.unwrap();
                    assert!(!message.contains("client-call-id"));
                    assert!(!message.contains("journal-secret"));
                }
                other => panic!("expected mismatched reference, got {other:?}"),
            }
        }
    }

    #[test]
    fn raw_reveal_corrupt_prefix_precedes_sequence_not_found() {
        let dir = tempfile::tempdir().unwrap();
        let journal = RunJournal::open(dir.path()).unwrap();
        let valid_line = serde_json::to_vec(&JournalLine {
            seq: 2,
            event: text("later"),
        })
        .unwrap();
        let mut contents = b"{malformed}\n".to_vec();
        contents.extend_from_slice(&valid_line);
        contents.push(b'\n');
        std::fs::write(journal.path_for("chat"), contents).unwrap();

        let result = journal
            .raw_reveal("chat", 1, None, None, TrajectoryRawField::Payload)
            .unwrap();
        assert!(matches!(
            result,
            TrajectoryRawRevealResult::Unavailable {
                reason: TrajectoryUnavailableReason::SourceCorrupt,
                ..
            }
        ));
    }

    #[test]
    fn test_trajectory_raw_reveal_non_canonical_chat_id_rejected_as_unavailable() {
        let dir = tempfile::tempdir().unwrap();
        let journal = RunJournal::open(dir.path()).unwrap();

        // Append an event to the canonical chat "chat_aliased"
        let seq = journal
            .append("chat_aliased", &text("secret content"))
            .unwrap();
        assert_eq!(seq, 1);

        // Attempting to reveal via non-canonical chat "chat/aliased" (which would sanitize to "chat_aliased")
        // must be rejected with typed Unavailable (NotFound) rather than leaking "chat_aliased" data.
        let result = journal
            .raw_reveal("chat/aliased", 1, None, None, TrajectoryRawField::Payload)
            .unwrap();
        match result {
            TrajectoryRawRevealResult::Unavailable { reason, .. } => {
                assert_eq!(reason, TrajectoryUnavailableReason::NotFound);
            }
            other => panic!("expected typed Unavailable for non-canonical chat_id, got: {other:?}"),
        }

        // Canonical reveal succeeds normally:
        let canon_result = journal
            .raw_reveal("chat_aliased", 1, None, None, TrajectoryRawField::Payload)
            .unwrap();
        match canon_result {
            TrajectoryRawRevealResult::Available { text, .. } => {
                assert_eq!(text, "secret content");
            }
            other => panic!("expected Available for canonical chat_id, got: {other:?}"),
        }
    }

    #[test]
    fn test_trajectory_raw_reveal_corrupted_continuation_returns_typed_unavailable() {
        let dir = tempfile::tempdir().unwrap();
        let journal = RunJournal::open(dir.path()).unwrap();

        let path = journal.path_for("chat_corrupt_cont");
        let line1 = serde_json::to_vec(&JournalLine {
            seq: 1,
            event: text("part1"),
        })
        .unwrap();
        let mut contents = line1;
        contents.push(b'\n');
        contents.extend_from_slice(b"{\"seq\":2,\"event\":{broken_json\n");
        std::fs::write(&path, contents).unwrap();

        let result = journal
            .raw_reveal(
                "chat_corrupt_cont",
                1,
                None,
                None,
                TrajectoryRawField::Payload,
            )
            .unwrap();
        match result {
            TrajectoryRawRevealResult::Unavailable { reason, .. } => {
                assert_eq!(reason, TrajectoryUnavailableReason::SourceCorrupt);
            }
            other => panic!("expected SourceCorrupt, got: {other:?}"),
        }
    }

    #[test]
    fn test_trajectory_raw_reveal_oversized_continuation_returns_typed_unavailable() {
        let dir = tempfile::tempdir().unwrap();
        let journal = RunJournal::open(dir.path()).unwrap();

        let path = journal.path_for("chat_oversized_cont");
        let line1 = serde_json::to_vec(&JournalLine {
            seq: 1,
            event: text("part1"),
        })
        .unwrap();
        let mut contents = line1;
        contents.push(b'\n');
        contents.extend_from_slice(b"{\"seq\":2,\"event\":{\"kind\":\"textDelta\",\"text\":\"");
        contents.extend_from_slice(&vec![b'x'; MAX_REVERSE_SCAN_LINE_BYTES + 10]);
        contents.extend_from_slice(b"\"}}\n");
        std::fs::write(&path, contents).unwrap();

        let result = journal
            .raw_reveal(
                "chat_oversized_cont",
                1,
                None,
                None,
                TrajectoryRawField::Payload,
            )
            .unwrap();
        match result {
            TrajectoryRawRevealResult::Unavailable { reason, .. } => {
                assert_eq!(reason, TrajectoryUnavailableReason::SourceOversized);
            }
            other => panic!("expected SourceOversized, got: {other:?}"),
        }
    }

    #[test]
    fn test_trajectory_raw_reveal_sequence_gap_continuation_returns_typed_unavailable() {
        let dir = tempfile::tempdir().unwrap();
        let journal = RunJournal::open(dir.path()).unwrap();

        let path = journal.path_for("chat_gap_cont");
        let line1 = serde_json::to_vec(&JournalLine {
            seq: 1,
            event: text("part1"),
        })
        .unwrap();
        let line3 = serde_json::to_vec(&JournalLine {
            seq: 3, // gap: seq 2 is missing!
            event: text("part2"),
        })
        .unwrap();
        let mut contents = line1;
        contents.push(b'\n');
        contents.extend_from_slice(&line3);
        contents.push(b'\n');
        std::fs::write(&path, contents).unwrap();

        let result = journal
            .raw_reveal("chat_gap_cont", 1, None, None, TrajectoryRawField::Payload)
            .unwrap();
        match result {
            TrajectoryRawRevealResult::Unavailable { reason, .. } => {
                assert_eq!(reason, TrajectoryUnavailableReason::SourceCorrupt);
            }
            other => panic!("expected SourceCorrupt on sequence gap, got: {other:?}"),
        }
    }

    #[test]
    fn test_trajectory_raw_reveal_scan_line_budget_exhaustion() {
        let dir = tempfile::tempdir().unwrap();
        let journal = RunJournal::open(dir.path()).unwrap();
        let path = journal.path_for("chat_huge_budget");

        use std::io::Write;
        let mut file = std::fs::File::create(&path).unwrap();
        for seq in 1..=(MAX_RAW_REVEAL_SCAN_LINES + 50) {
            writeln!(
                file,
                "{{\"seq\":{seq},\"event\":{{\"kind\":\"textDelta\",\"text\":\"t\"}}}}"
            )
            .unwrap();
        }
        file.flush().unwrap();

        // Looking for seq (MAX_RAW_REVEAL_SCAN_LINES + 40) must hit line budget exhaustion and return SourceOversized:
        let result = journal
            .raw_reveal(
                "chat_huge_budget",
                (MAX_RAW_REVEAL_SCAN_LINES + 40) as u64,
                None,
                None,
                TrajectoryRawField::Payload,
            )
            .unwrap();
        match result {
            TrajectoryRawRevealResult::Unavailable { reason, .. } => {
                assert_eq!(reason, TrajectoryUnavailableReason::SourceOversized);
            }
            other => panic!("expected SourceOversized when exceeding line budget, got: {other:?}"),
        }
    }
}
