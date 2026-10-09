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
        last_valid_line(&self.path_for(chat_id))
    }

    /// Valid events newest-first, read backwards from the end of the file — for
    /// callers that want the most recent match and can stop early instead of
    /// materialising the whole journal like `replay` does.
    pub fn events_rev(
        &self,
        chat_id: &str,
    ) -> Result<impl Iterator<Item = Result<(u64, AgentEvent), JournalError>>, JournalError> {
        RevEvents::open(&self.path_for(chat_id))
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
            // Boot recovery only needs the newest valid event, not the full journal.
            let last = last_valid_line(&path)?;
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
        if let Some(parsed) = parse_line(path, &line?) {
            out.push(parsed);
        }
    }
    Ok(out)
}

/// One journal line → `(seq, event)`; blank lines are skipped silently, malformed
/// ones (torn tail writes) with a warning.
fn parse_line(path: &Path, line: &str) -> Option<(u64, AgentEvent)> {
    if line.trim().is_empty() {
        return None;
    }
    match serde_json::from_str::<JournalLine>(line) {
        Ok(parsed) => Some((parsed.seq, parsed.event)),
        Err(err) => {
            tracing::warn!(path = %path.display(), error = %err, "journal: skipping malformed line");
            None
        }
    }
}

/// The last valid event in the file — what `read_lines(path).last()` returns, without
/// reading more than the tail.
fn last_valid_line(path: &Path) -> Result<Option<(u64, AgentEvent)>, JournalError> {
    RevEvents::open(path)?.next().transpose()
}

/// Next seq (last valid seq + 1, starting at 1) and whether the file ends mid-line.
fn scan_tail(path: &Path) -> Result<(u64, bool), JournalError> {
    let Some(mut lines) = RevLines::open(path)? else {
        return Ok((1, false));
    };
    let needs_newline = lines.ends_mid_line()?;
    let next_seq = RevEvents::from_lines(path, Some(lines))
        .next()
        .transpose()?
        .map(|(seq, _)| seq + 1)
        .unwrap_or(1);
    Ok((next_seq, needs_newline))
}

/// First tail-read size. Journal lines are usually far shorter; a longer line grows the
/// read geometrically, so a multi-MB line costs O(len) rather than O(len²).
const TAIL_CHUNK: usize = 64 * 1024;

/// Raw lines of a file, last to first, read backwards in chunks — memory is bounded by
/// the longest line rather than the file. `\n` separators are stripped (a preceding `\r`
/// is left in place; JSON parsing treats it as whitespace), so a file ending in `\n`
/// yields an empty line first, which callers skip like any blank line.
struct RevLines {
    file: File,
    /// File offset of `buf[0]`; bytes before it have not been read yet.
    pos: u64,
    /// Read but not yet yielded: the file's bytes `[pos, pos + buf.len())`.
    buf: Vec<u8>,
    finished: bool,
}

impl RevLines {
    /// `None` when the file does not exist (an empty journal, as `read_lines` treats it).
    fn open(path: &Path) -> std::io::Result<Option<Self>> {
        let file = match File::open(path) {
            Ok(f) => f,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(e),
        };
        let pos = file.metadata()?.len();
        Ok(Some(Self {
            file,
            pos,
            buf: Vec::new(),
            finished: false,
        }))
    }

    /// Prepend the previous chunk of the file to `buf` (at least as large as `buf`).
    fn fill(&mut self) -> std::io::Result<()> {
        let want = TAIL_CHUNK.max(self.buf.len()) as u64;
        let n = want.min(self.pos);
        self.pos -= n;
        let mut chunk = vec![0u8; n as usize];
        self.file.seek(SeekFrom::Start(self.pos))?;
        self.file.read_exact(&mut chunk)?;
        chunk.extend_from_slice(&self.buf);
        self.buf = chunk;
        Ok(())
    }

    /// True when the file is non-empty and its last byte is not `\n` (a torn write).
    /// Call before the first `next_line`.
    fn ends_mid_line(&mut self) -> std::io::Result<bool> {
        if self.buf.is_empty() && self.pos > 0 {
            self.fill()?;
        }
        Ok(self.buf.last().is_some_and(|b| *b != b'\n'))
    }

    fn next_line(&mut self) -> std::io::Result<Option<Vec<u8>>> {
        loop {
            if self.finished {
                return Ok(None);
            }
            if let Some(i) = self.buf.iter().rposition(|b| *b == b'\n') {
                let line = self.buf.split_off(i + 1);
                self.buf.truncate(i);
                return Ok(Some(line));
            }
            if self.pos == 0 {
                self.finished = true;
                return Ok(Some(std::mem::take(&mut self.buf)));
            }
            self.fill()?;
        }
    }
}

/// Valid journal events newest-first: `read_lines` in reverse, with the same
/// blank/malformed-line skipping, reading only as far back as the caller iterates.
struct RevEvents {
    path: PathBuf,
    /// `None` once exhausted, after an I/O error, or when the file does not exist.
    lines: Option<RevLines>,
}

impl RevEvents {
    fn open(path: &Path) -> Result<Self, JournalError> {
        Ok(Self::from_lines(path, RevLines::open(path)?))
    }

    fn from_lines(path: &Path, lines: Option<RevLines>) -> Self {
        Self {
            path: path.to_path_buf(),
            lines,
        }
    }
}

impl Iterator for RevEvents {
    type Item = Result<(u64, AgentEvent), JournalError>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let raw = match self.lines.as_mut()?.next_line() {
                Ok(Some(raw)) => raw,
                Ok(None) => {
                    self.lines = None;
                    return None;
                }
                Err(err) => {
                    self.lines = None;
                    return Some(Err(err.into()));
                }
            };
            // A torn write can split a multi-byte char: treat it as malformed too.
            let Ok(line) = std::str::from_utf8(&raw) else {
                tracing::warn!(path = %self.path.display(), "journal: skipping non-UTF-8 line");
                continue;
            };
            if let Some(parsed) = parse_line(&self.path, line) {
                return Some(Ok(parsed));
            }
        }
    }
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

    fn line(seq: u64, event: AgentEvent) -> String {
        serde_json::to_string(&JournalLine { seq, event }).unwrap()
    }

    fn rev_lines(path: &Path) -> Vec<Vec<u8>> {
        let Some(mut lines) = RevLines::open(path).unwrap() else {
            return Vec::new();
        };
        std::iter::from_fn(|| lines.next_line().unwrap()).collect()
    }

    /// Journal contents covering the tail reader's edge cases; each must read the same
    /// backwards as the full forward parse did.
    fn tail_cases() -> Vec<(&'static str, Vec<u8>)> {
        let huge = "x".repeat(5 * TAIL_CHUNK + 123);
        let a = line(1, text("a"));
        let b = line(2, text("b"));
        let big = line(3, text(&huge));
        let fin = line(4, done());
        vec![
            ("empty", Vec::new()),
            ("only-newlines", b"\n\n\r\n  \n".to_vec()),
            ("trailing-newline", format!("{a}\n{b}\n").into_bytes()),
            ("no-trailing-newline", format!("{a}\n{b}").into_bytes()),
            ("crlf", format!("{a}\r\n{b}\r\n").into_bytes()),
            (
                "blank-lines-at-end",
                format!("{a}\n{b}\n\n  \n").into_bytes(),
            ),
            (
                "garbage-last-line",
                format!("{a}\n{b}\n{{\"seq\":3,\"event\":{{\"type\":\"textD").into_bytes(),
            ),
            (
                "garbage-then-newline",
                format!("{a}\n{b}\nnot json\n\n").into_bytes(),
            ),
            ("all-garbage", b"nope\n{\"seq\":\nstill nope".to_vec()),
            (
                "multi-chunk-last-line",
                format!("{a}\n{big}\n").into_bytes(),
            ),
            (
                "multi-chunk-last-line-unterminated",
                format!("{a}\n{big}").into_bytes(),
            ),
            (
                "multi-chunk-middle-line",
                format!("{a}\n{big}\n{fin}\n").into_bytes(),
            ),
            (
                "multi-chunk-garbage-tail",
                format!("{a}\n{b}\n{}", &big[..big.len() - 7]).into_bytes(),
            ),
            ("multi-chunk-only-line", big.clone().into_bytes()),
        ]
    }

    #[test]
    fn rev_lines_are_forward_lines_reversed() {
        let dir = tempfile::tempdir().unwrap();
        for (name, bytes) in tail_cases() {
            let path = dir.path().join(format!("{name}.jsonl"));
            std::fs::write(&path, &bytes).unwrap();
            let mut expected: Vec<Vec<u8>> =
                bytes.split(|b| *b == b'\n').map(<[u8]>::to_vec).collect();
            expected.reverse();
            assert_eq!(rev_lines(&path), expected, "{name}");
        }
        assert!(rev_lines(&dir.path().join("missing.jsonl")).is_empty());
    }

    #[test]
    fn tail_reads_match_a_full_forward_parse() {
        let dir = tempfile::tempdir().unwrap();
        for (name, bytes) in tail_cases() {
            let path = dir.path().join(format!("{name}.jsonl"));
            std::fs::write(&path, &bytes).unwrap();
            let all = read_lines(&path).unwrap();

            // Every valid event, newest-first.
            let mut expected = all.clone();
            expected.reverse();
            let rev: Vec<_> = RevEvents::open(&path)
                .unwrap()
                .collect::<Result<_, _>>()
                .unwrap();
            assert_eq!(rev, expected, "{name}");

            assert_eq!(
                last_valid_line(&path).unwrap(),
                all.last().cloned(),
                "{name}"
            );
            let old_scan_tail = (
                all.last().map(|(seq, _)| seq + 1).unwrap_or(1),
                bytes.last().is_some_and(|b| *b != b'\n'),
            );
            assert_eq!(scan_tail(&path).unwrap(), old_scan_tail, "{name}");
        }
        let missing = dir.path().join("missing.jsonl");
        assert_eq!(last_valid_line(&missing).unwrap(), None);
        assert_eq!(scan_tail(&missing).unwrap(), (1, false));
    }

    #[test]
    fn stale_sessions_unchanged_by_tail_reads() {
        let dir = tempfile::tempdir().unwrap();
        for (name, bytes) in tail_cases() {
            std::fs::write(dir.path().join(format!("{name}.jsonl")), bytes).unwrap();
        }
        std::fs::write(dir.path().join("ignored.resume"), "2").unwrap();
        let journal = RunJournal::open(dir.path()).unwrap();

        // The pre-tail-read implementation: full parse, inspect the last event.
        let mut expected = Vec::new();
        for entry in std::fs::read_dir(dir.path()).unwrap() {
            let path = entry.unwrap().path();
            if path.extension().and_then(|e| e.to_str()) != Some("jsonl") {
                continue;
            }
            let last = read_lines(&path).unwrap().into_iter().next_back();
            if matches!(last, Some((_, ref event)) if !matches!(event, AgentEvent::Done { .. })) {
                expected.push(path.file_stem().unwrap().to_str().unwrap().to_string());
            }
        }
        expected.sort();
        assert_eq!(journal.stale_sessions().unwrap(), expected);
        assert!(expected.contains(&"multi-chunk-garbage-tail".to_string()));
        assert!(!expected.contains(&"multi-chunk-middle-line".to_string()));
    }

    #[test]
    fn append_after_multi_chunk_torn_tail_isolates_it() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("chat-1.jsonl");
        let big = line(7, text(&"y".repeat(3 * TAIL_CHUNK)));
        std::fs::write(
            &path,
            format!("{}\n{}", line(6, text("a")), &big[..big.len() / 2]),
        )
        .unwrap();
        let journal = RunJournal::open(dir.path()).unwrap();
        assert_eq!(journal.append("chat-1", &text("b")).unwrap(), 7);
        assert!(matches!(
            journal.last_event("chat-1").unwrap(),
            Some((7, AgentEvent::TextDelta { .. }))
        ));
        assert_eq!(journal.replay("chat-1", 0).unwrap().len(), 2);
    }

    #[test]
    fn non_utf8_tail_line_is_skipped_as_malformed() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("chat-1.jsonl");
        let mut bytes = format!("{}\n", line(1, done())).into_bytes();
        // A torn write that split a multi-byte char.
        bytes
            .extend_from_slice(b"{\"seq\":2,\"event\":{\"type\":\"textDelta\",\"text\":\"\xE2\x82");
        std::fs::write(&path, bytes).unwrap();
        let journal = RunJournal::open(dir.path()).unwrap();
        assert!(matches!(
            journal.last_event("chat-1").unwrap(),
            Some((1, AgentEvent::Done { .. }))
        ));
        assert!(journal.stale_sessions().unwrap().is_empty());
    }
}
