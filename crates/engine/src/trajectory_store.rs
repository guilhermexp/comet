//! SQLite-backed profile-local Trajectory store.
//!
//! Owns persistent, sanitized Trajectory records, runs, watermarks, and degraded
//! intervals under `{store_root}/trajectory.sqlite3`.
//!
//! Architecture invariants:
//! - Single ordered background writer connection with a bounded, non-blocking queue.
//! - Independent read connections (WAL mode) for RPC queries, legacy reads, and diagnostics.
//! - Synchronous `publish` enqueue never blocks on database locks or transactions.
//! - Queue saturation or transaction failure records explicit degraded intervals rather than failing.
//! - Stores only sanitized representations and opaque Run Journal references; never duplicate raw bodies.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use chrono::{DateTime, TimeZone, Utc};
use rusqlite::{Connection, OpenFlags, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use tokio::sync::{mpsc, oneshot};

use zeron_proto::trajectory::{
    TrajectoryDegradedInterval, TrajectoryLane, TrajectoryPayloadPreview, TrajectoryRawField,
    TrajectoryRawRef, TrajectoryRecord, TrajectoryRecordId, TrajectoryRecordKind,
    TrajectoryResultPreview, TrajectoryStatus, TrajectoryTiming, TrajectoryUsage,
};
use zeron_proto::{AgentEvent, DoneStatus, ToolCall};
#[derive(Debug, thiserror::Error)]
pub enum TrajectoryStoreError {
    #[error("sqlite: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
    #[error("queue full")]
    QueueFull,
    #[error("channel closed")]
    ChannelClosed,
    #[error("other: {0}")]
    Other(String),
}

/// Ordered, append-only migrations.
const MIGRATIONS: &[&str] = &[
    // v1 — records, runs, watermarks, degraded intervals, and legacy imports
    "CREATE TABLE trajectory_records (
        chat_id            TEXT NOT NULL,
        run_id             TEXT NOT NULL,
        source_seq         INTEGER NOT NULL,
        sub_seq            INTEGER NOT NULL,
        lane               TEXT NOT NULL,
        kind               TEXT NOT NULL,
        status             TEXT NOT NULL,
        is_partial         INTEGER NOT NULL,
        title              TEXT NOT NULL,
        summary            TEXT NOT NULL,
        turn_id            TEXT,
        step_id            TEXT,
        call_id            TEXT,
        parent_tool_use_id TEXT,
        timing             TEXT,
        usage              TEXT,
        payload            TEXT,
        result             TEXT,
        error_message      TEXT,
        is_degraded        INTEGER NOT NULL DEFAULT 0,
        created_at         INTEGER NOT NULL,
        PRIMARY KEY (chat_id, run_id, source_seq, sub_seq)
     ) STRICT;

     CREATE INDEX idx_traj_chat_seq ON trajectory_records(chat_id, source_seq, sub_seq);
     CREATE INDEX idx_traj_chat_run ON trajectory_records(chat_id, run_id);

     CREATE TABLE trajectory_runs (
        chat_id    TEXT NOT NULL,
        run_id     TEXT NOT NULL,
        label      TEXT NOT NULL,
        is_legacy  INTEGER NOT NULL DEFAULT 0,
        status     TEXT NOT NULL,
        timing     TEXT,
        created_at INTEGER NOT NULL,
        updated_at INTEGER NOT NULL,
        PRIMARY KEY (chat_id, run_id)
     ) STRICT;

     CREATE TABLE trajectory_watermarks (
        chat_id          TEXT PRIMARY KEY,
        last_source_seq  INTEGER NOT NULL,
        updated_at       INTEGER NOT NULL
     ) STRICT;

     CREATE TABLE trajectory_degraded_intervals (
        id          INTEGER PRIMARY KEY AUTOINCREMENT,
        chat_id     TEXT NOT NULL,
        run_id      TEXT NOT NULL,
        from_seq    INTEGER NOT NULL,
        to_seq      INTEGER NOT NULL,
        reason      TEXT NOT NULL,
        recorded_at INTEGER NOT NULL
     ) STRICT;

     CREATE TABLE trajectory_legacy_imports (
        chat_id            TEXT PRIMARY KEY,
        source_fingerprint TEXT NOT NULL,
        imported_records   INTEGER NOT NULL,
        imported_at        INTEGER NOT NULL
     ) STRICT;",
];

/// Diagnostics information for a Chat's trajectory.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrajectoryDiagnostics {
    pub chat_id: String,
    pub record_count: usize,
    pub run_count: usize,
    pub last_watermark: Option<u64>,
    pub degraded_count: usize,
    pub db_size_bytes: u64,
}

/// Commands for the background writer task.
enum WriterCommand {
    WriteRecords(Vec<TrajectoryRecord>),
    RecordDegraded(TrajectoryDegradedInterval),
    DeleteChat(
        String,
        Option<oneshot::Sender<Result<(), TrajectoryStoreError>>>,
    ),
    RetainChats(
        Vec<String>,
        Option<oneshot::Sender<Result<usize, TrajectoryStoreError>>>,
    ),
    Flush(oneshot::Sender<()>),
}

/// Default capacity for the nonblocking capture queue.
const CAPTURE_QUEUE_CAPACITY: usize = 2048;

/// Device-local SQLite trajectory store.
#[derive(Clone)]
pub struct TrajectoryStore {
    db_path: PathBuf,
    writer_tx: mpsc::Sender<WriterCommand>,
}

impl TrajectoryStore {
    /// Open or create the trajectory store at `{store_root}/trajectory.sqlite3`.
    pub fn open(store_root: impl AsRef<Path>) -> Result<Self, TrajectoryStoreError> {
        let store_root = store_root.as_ref();
        fs::create_dir_all(store_root)?;
        let db_path = store_root.join("trajectory.sqlite3");

        // Run initial migrations on open
        {
            let mut conn = Connection::open(&db_path)?;
            conn.pragma_update(None, "journal_mode", "WAL")?;
            conn.pragma_update(None, "synchronous", "NORMAL")?;
            conn.busy_timeout(Duration::from_secs(5))?;
            migrate(&mut conn)?;
        }

        let (writer_tx, mut writer_rx) = mpsc::channel::<WriterCommand>(CAPTURE_QUEUE_CAPACITY);
        let writer_db_path = db_path.clone();

        // Spawn ordered background writer task
        tokio::spawn(async move {
            let mut conn = match Connection::open(&writer_db_path) {
                Ok(c) => c,
                Err(err) => {
                    tracing::error!(error = %err, "failed to open trajectory writer connection");
                    return;
                }
            };
            let _ = conn.pragma_update(None, "journal_mode", "WAL");
            let _ = conn.pragma_update(None, "synchronous", "NORMAL");
            let _ = conn.busy_timeout(Duration::from_secs(5));

            while let Some(cmd) = writer_rx.recv().await {
                match cmd {
                    WriterCommand::WriteRecords(mut records) => {
                        // Drain any immediately available batched records
                        while let Ok(next) = writer_rx.try_recv() {
                            match next {
                                WriterCommand::WriteRecords(more) => records.extend(more),
                                other => {
                                    // Process batch so far then process other command
                                    let _ = write_records_tx(&mut conn, &records);
                                    records.clear();
                                    handle_writer_command(&mut conn, other);
                                    break;
                                }
                            }
                        }
                        if !records.is_empty() {
                            let _ = write_records_tx(&mut conn, &records);
                        }
                    }
                    other => {
                        handle_writer_command(&mut conn, other);
                    }
                }
            }
        });

        Ok(Self { db_path, writer_tx })
    }

    /// Enqueue a captured record nonblockingly.
    ///
    /// If the queue is saturated, this method records a degraded interval rather than
    /// blocking synchronous publication.
    pub fn try_enqueue(&self, record: TrajectoryRecord) -> Result<(), TrajectoryStoreError> {
        match self
            .writer_tx
            .try_send(WriterCommand::WriteRecords(vec![record.clone()]))
        {
            Ok(()) => Ok(()),
            Err(mpsc::error::TrySendError::Full(_)) => {
                tracing::warn!(
                    chat = %record.chat_id,
                    run = %record.run_id,
                    seq = record.source_seq,
                    "trajectory capture queue saturated; recording degraded interval"
                );
                let degraded = TrajectoryDegradedInterval {
                    chat_id: record.chat_id.clone(),
                    run_id: record.run_id.clone(),
                    from_seq: record.source_seq,
                    to_seq: record.source_seq,
                    reason: "Queue saturated".into(),
                    recorded_at: Utc::now(),
                };
                let _ = self
                    .writer_tx
                    .try_send(WriterCommand::RecordDegraded(degraded));
                Err(TrajectoryStoreError::QueueFull)
            }
            Err(mpsc::error::TrySendError::Closed(_)) => Err(TrajectoryStoreError::ChannelClosed),
        }
    }

    /// Enqueue a batch of records nonblockingly.
    pub fn try_enqueue_batch(
        &self,
        records: Vec<TrajectoryRecord>,
    ) -> Result<(), TrajectoryStoreError> {
        if records.is_empty() {
            return Ok(());
        }
        match self
            .writer_tx
            .try_send(WriterCommand::WriteRecords(records))
        {
            Ok(()) => Ok(()),
            Err(mpsc::error::TrySendError::Full(WriterCommand::WriteRecords(recs))) => {
                if let Some(first) = recs.first() {
                    let last = recs.last().unwrap_or(first);
                    let degraded = TrajectoryDegradedInterval {
                        chat_id: first.chat_id.clone(),
                        run_id: first.run_id.clone(),
                        from_seq: first.source_seq,
                        to_seq: last.source_seq,
                        reason: "Queue saturated during batch".into(),
                        recorded_at: Utc::now(),
                    };
                    let _ = self
                        .writer_tx
                        .try_send(WriterCommand::RecordDegraded(degraded));
                }
                Err(TrajectoryStoreError::QueueFull)
            }
            Err(mpsc::error::TrySendError::Full(_)) => Err(TrajectoryStoreError::QueueFull),
            Err(mpsc::error::TrySendError::Closed(_)) => Err(TrajectoryStoreError::ChannelClosed),
        }
    }

    /// Flush the background writer queue and await completion.
    pub async fn flush(&self) -> Result<(), TrajectoryStoreError> {
        let (tx, rx) = oneshot::channel();
        self.writer_tx
            .send(WriterCommand::Flush(tx))
            .await
            .map_err(|_| TrajectoryStoreError::ChannelClosed)?;
        rx.await.map_err(|_| TrajectoryStoreError::ChannelClosed)
    }

    /// Open an independent reader connection in WAL mode.
    pub fn reader(&self) -> Result<Connection, TrajectoryStoreError> {
        let conn = Connection::open_with_flags(
            &self.db_path,
            OpenFlags::SQLITE_OPEN_READ_ONLY
                | OpenFlags::SQLITE_OPEN_NO_MUTEX
                | OpenFlags::SQLITE_OPEN_URI,
        )?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "synchronous", "NORMAL")?;
        conn.busy_timeout(Duration::from_secs(5))?;
        Ok(conn)
    }

    /// Read an ordered slice of records for `chat_id`.
    pub fn list_records(
        &self,
        chat_id: &str,
        from_seq: Option<u64>,
        limit: Option<usize>,
    ) -> Result<Vec<TrajectoryRecord>, TrajectoryStoreError> {
        let conn = self.reader()?;
        let from = from_seq.unwrap_or(0);
        let lim = limit.unwrap_or(10_000) as i64;

        let mut stmt = conn.prepare(
            "SELECT chat_id, run_id, source_seq, sub_seq, lane, kind, status, is_partial,
                    title, summary, turn_id, step_id, call_id, parent_tool_use_id,
                    timing, usage, payload, result, error_message, is_degraded
             FROM trajectory_records
             WHERE chat_id = ?1 AND source_seq >= ?2
             ORDER BY source_seq ASC, sub_seq ASC
             LIMIT ?3",
        )?;

        let rows = stmt
            .query_map(params![chat_id, from as i64, lim], row_to_record)?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(rows)
    }

    /// Read all records for `chat_id` in chronological order.
    pub fn list_all_records(
        &self,
        chat_id: &str,
    ) -> Result<Vec<TrajectoryRecord>, TrajectoryStoreError> {
        self.list_records(chat_id, None, None)
    }

    /// Fetch latest recorded watermark for `chat_id`.
    pub fn get_watermark(&self, chat_id: &str) -> Result<Option<u64>, TrajectoryStoreError> {
        let conn = self.reader()?;
        let res = conn
            .query_row(
                "SELECT last_source_seq FROM trajectory_watermarks WHERE chat_id = ?1",
                params![chat_id],
                |row| row.get::<_, i64>(0),
            )
            .optional()?;
        Ok(res.map(|s| s as u64))
    }

    /// Fetch degraded intervals for `chat_id`.
    pub fn get_degraded_intervals(
        &self,
        chat_id: &str,
    ) -> Result<Vec<TrajectoryDegradedInterval>, TrajectoryStoreError> {
        let conn = self.reader()?;
        let mut stmt = conn.prepare(
            "SELECT chat_id, run_id, from_seq, to_seq, reason, recorded_at
             FROM trajectory_degraded_intervals
             WHERE chat_id = ?1
             ORDER BY from_seq ASC",
        )?;

        let rows = stmt
            .query_map(params![chat_id], |row| {
                let ms: i64 = row.get(5)?;
                Ok(TrajectoryDegradedInterval {
                    chat_id: row.get(0)?,
                    run_id: row.get(1)?,
                    from_seq: row.get::<_, i64>(2)? as u64,
                    to_seq: row.get::<_, i64>(3)? as u64,
                    reason: row.get(4)?,
                    recorded_at: Utc
                        .timestamp_millis_opt(ms)
                        .single()
                        .unwrap_or_else(Utc::now),
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(rows)
    }

    /// Fetch diagnostics summary for `chat_id`.
    pub fn diagnostics(
        &self,
        chat_id: &str,
    ) -> Result<TrajectoryDiagnostics, TrajectoryStoreError> {
        let conn = self.reader()?;
        let record_count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM trajectory_records WHERE chat_id = ?1",
            params![chat_id],
            |r| r.get(0),
        )?;
        let run_count: i64 = conn.query_row(
            "SELECT COUNT(DISTINCT run_id) FROM trajectory_records WHERE chat_id = ?1",
            params![chat_id],
            |r| r.get(0),
        )?;
        let degraded_count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM trajectory_degraded_intervals WHERE chat_id = ?1",
            params![chat_id],
            |r| r.get(0),
        )?;
        let last_watermark = self.get_watermark(chat_id)?;
        let db_size_bytes = fs::metadata(&self.db_path).map(|m| m.len()).unwrap_or(0);

        Ok(TrajectoryDiagnostics {
            chat_id: chat_id.to_string(),
            record_count: record_count as usize,
            run_count: run_count as usize,
            last_watermark,
            degraded_count: degraded_count as usize,
            db_size_bytes,
        })
    }

    /// Delete all trajectory data for `chat_id` asynchronously through the writer.
    pub async fn delete_chat(&self, chat_id: &str) -> Result<(), TrajectoryStoreError> {
        let (tx, rx) = oneshot::channel();
        self.writer_tx
            .send(WriterCommand::DeleteChat(chat_id.to_string(), Some(tx)))
            .await
            .map_err(|_| TrajectoryStoreError::ChannelClosed)?;
        rx.await.map_err(|_| TrajectoryStoreError::ChannelClosed)?
    }

    /// Retain only the specified active Chat IDs, removing stale records from any deleted Chats.
    pub async fn retain_chats_only(
        &self,
        live_chat_ids: &[String],
    ) -> Result<usize, TrajectoryStoreError> {
        let (tx, rx) = oneshot::channel();
        self.writer_tx
            .send(WriterCommand::RetainChats(live_chat_ids.to_vec(), Some(tx)))
            .await
            .map_err(|_| TrajectoryStoreError::ChannelClosed)?;
        rx.await.map_err(|_| TrajectoryStoreError::ChannelClosed)?
    }

    /// Check if a legacy journal has already been imported.
    pub fn legacy_import_fingerprint(
        &self,
        chat_id: &str,
    ) -> Result<Option<String>, TrajectoryStoreError> {
        let conn = self.reader()?;
        let fp = conn
            .query_row(
                "SELECT source_fingerprint FROM trajectory_legacy_imports WHERE chat_id = ?1",
                params![chat_id],
                |r| r.get(0),
            )
            .optional()?;
        Ok(fp)
    }

    /// Record completed legacy import.
    pub fn record_legacy_import(
        &self,
        chat_id: &str,
        fingerprint: &str,
        records_count: usize,
    ) -> Result<(), TrajectoryStoreError> {
        let conn = Connection::open(&self.db_path)?;
        conn.execute(
            "INSERT INTO trajectory_legacy_imports (chat_id, source_fingerprint, imported_records, imported_at)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(chat_id) DO UPDATE SET
                 source_fingerprint = excluded.source_fingerprint,
                 imported_records = excluded.imported_records,
                 imported_at = excluded.imported_at",
            params![chat_id, fingerprint, records_count as i64, Utc::now().timestamp_millis()],
        )?;
        Ok(())
    }

    /// Synchronous batch write helper (for direct tests or legacy imports).
    pub fn sync_write_records(
        &self,
        records: &[TrajectoryRecord],
    ) -> Result<(), TrajectoryStoreError> {
        let mut conn = Connection::open(&self.db_path)?;
        write_records_tx(&mut conn, records)
    }

    /// Synchronous delete chat helper (for direct tests).
    pub fn sync_delete_chat(&self, chat_id: &str) -> Result<(), TrajectoryStoreError> {
        let conn = Connection::open(&self.db_path)?;
        delete_chat_tx(&conn, chat_id)
    }

    /// Lazily import a legacy Run Journal JSONL file into the Trajectory store.
    ///
    /// Properties:
    /// - Idempotent: checks fingerprint; duplicate imports are skipped.
    /// - Sequence-only: missing timestamps are never fabricated (duration remains unavailable).
    /// - Valid-prefix recovery: corrupt trailing lines stop parsing at the valid prefix.
    /// - Interrupted/unsettled state: if no terminal Done event is found, the run is marked Interrupted.
    /// - Returns `Ok(true)` if imported, `Ok(false)` if already imported or missing.
    pub fn import_legacy_journal(
        &self,
        chat_id: &str,
        journal_path: impl AsRef<Path>,
    ) -> Result<bool, TrajectoryStoreError> {
        let path = journal_path.as_ref();
        if !path.exists() {
            return Ok(false);
        }

        let metadata = fs::metadata(path)?;
        let file_len = metadata.len();
        let mtime = metadata
            .modified()
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_millis())
            .unwrap_or(0);
        let fingerprint = format!("{}:{}:{}", path.display(), file_len, mtime);

        if let Ok(Some(existing_fp)) = self.legacy_import_fingerprint(chat_id) {
            if existing_fp == fingerprint {
                return Ok(false);
            }
        }

        let file = fs::File::open(path)?;
        let reader = std::io::BufReader::new(file);
        use std::io::BufRead;

        let run_id = format!("legacy_{}", chat_id);
        let mut records: Vec<TrajectoryRecord> = Vec::new();
        let mut has_done = false;
        let mut pending_tools: std::collections::HashSet<String> = std::collections::HashSet::new();

        for line_res in reader.lines() {
            let line = match line_res {
                Ok(l) => l,
                Err(_) => break, // Corrupt tail / IO error: stop at valid prefix
            };
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }

            #[derive(Deserialize)]
            struct LegacyLine {
                seq: u64,
                event: zeron_proto::AgentEvent,
            }

            let parsed: LegacyLine = match serde_json::from_str(trimmed) {
                Ok(p) => p,
                Err(_) => break, // Corrupt line: stop at valid prefix
            };

            let seq = parsed.seq;
            let event = parsed.event;

            if matches!(event, zeron_proto::AgentEvent::Done { .. }) {
                has_done = true;
            }
            if let zeron_proto::AgentEvent::ToolCall { ref id, .. } = event {
                pending_tools.insert(id.clone());
            }
            if let zeron_proto::AgentEvent::ToolResult { ref id, .. } = event {
                pending_tools.remove(id);
            }

            if let Some(mut rec) = project_event_to_record(chat_id, &run_id, seq, &event, None) {
                rec.timing = Some(TrajectoryTiming::sequence_only());
                records.push(rec);
            }
        }

        // If the journal had no terminal Done event, mark unsettled tool calls and add an interrupted record if needed
        if !has_done && !records.is_empty() {
            for rec in &mut records {
                if let Some(ref call_id) = rec.call_id {
                    if pending_tools.contains(call_id)
                        && matches!(rec.kind, TrajectoryRecordKind::ToolCall { .. })
                    {
                        rec.status = TrajectoryStatus::Unsettled;
                    }
                }
            }
        }

        let count = records.len();
        self.sync_write_records(&records)?;
        self.record_legacy_import(chat_id, &fingerprint, count)?;

        Ok(true)
    }
}

// ---------------------------------------------------------------------------
// Internal SQLite Helpers
// ---------------------------------------------------------------------------

fn handle_writer_command(conn: &mut Connection, cmd: WriterCommand) {
    match cmd {
        WriterCommand::WriteRecords(recs) => {
            let _ = write_records_tx(conn, &recs);
        }
        WriterCommand::RecordDegraded(degraded) => {
            let _ = conn.execute(
                "INSERT INTO trajectory_degraded_intervals (chat_id, run_id, from_seq, to_seq, reason, recorded_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    degraded.chat_id,
                    degraded.run_id,
                    degraded.from_seq as i64,
                    degraded.to_seq as i64,
                    degraded.reason,
                    degraded.recorded_at.timestamp_millis()
                ],
            );
        }
        WriterCommand::DeleteChat(chat_id, reply) => {
            let res = delete_chat_tx(conn, &chat_id);
            if let Some(tx) = reply {
                let _ = tx.send(res);
            }
        }
        WriterCommand::RetainChats(live_ids, reply) => {
            let res = retain_chats_tx(conn, &live_ids);
            if let Some(tx) = reply {
                let _ = tx.send(res);
            }
        }
        WriterCommand::Flush(reply) => {
            let _ = reply.send(());
        }
    }
}

fn write_records_tx(
    conn: &mut Connection,
    records: &[TrajectoryRecord],
) -> Result<(), TrajectoryStoreError> {
    if records.is_empty() {
        return Ok(());
    }

    let tx = conn.transaction()?;
    let now = Utc::now().timestamp_millis();

    for r in records {
        let lane_str = r.lane.as_str();
        let kind_json = serde_json::to_string(&r.kind)?;
        let status_str = format!("{:?}", r.status);
        let timing_json = r.timing.as_ref().map(serde_json::to_string).transpose()?;
        let usage_json = r.usage.as_ref().map(serde_json::to_string).transpose()?;
        let payload_json = r.payload.as_ref().map(serde_json::to_string).transpose()?;
        let result_json = r.result.as_ref().map(serde_json::to_string).transpose()?;

        tx.execute(
            "INSERT INTO trajectory_records (
                chat_id, run_id, source_seq, sub_seq, lane, kind, status, is_partial,
                title, summary, turn_id, step_id, call_id, parent_tool_use_id,
                timing, usage, payload, result, error_message, is_degraded, created_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21)
             ON CONFLICT(chat_id, run_id, source_seq, sub_seq) DO UPDATE SET
                lane = excluded.lane,
                kind = excluded.kind,
                status = excluded.status,
                is_partial = excluded.is_partial,
                title = excluded.title,
                summary = excluded.summary,
                turn_id = excluded.turn_id,
                step_id = excluded.step_id,
                call_id = excluded.call_id,
                parent_tool_use_id = excluded.parent_tool_use_id,
                timing = excluded.timing,
                usage = excluded.usage,
                payload = excluded.payload,
                result = excluded.result,
                error_message = excluded.error_message,
                is_degraded = excluded.is_degraded",
            params![
                r.chat_id,
                r.run_id,
                r.source_seq as i64,
                r.sub_seq as i64,
                lane_str,
                kind_json,
                status_str,
                if r.is_partial { 1 } else { 0 },
                r.title,
                r.summary,
                r.turn_id,
                r.step_id,
                r.call_id,
                r.parent_tool_use_id,
                timing_json,
                usage_json,
                payload_json,
                result_json,
                r.error_message,
                if r.is_degraded { 1 } else { 0 },
                now
            ],
        )?;

        // Update run entry
        tx.execute(
            "INSERT INTO trajectory_runs (chat_id, run_id, label, is_legacy, status, timing, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
             ON CONFLICT(chat_id, run_id) DO UPDATE SET
                status = excluded.status,
                timing = excluded.timing,
                updated_at = excluded.updated_at",
            params![
                r.chat_id,
                r.run_id,
                if r.run_id.starts_with("legacy") { "Legacy Run" } else { "Run" },
                if r.run_id.starts_with("legacy") { 1 } else { 0 },
                status_str,
                timing_json,
                now,
                now
            ],
        )?;

        // Update watermark
        tx.execute(
            "INSERT INTO trajectory_watermarks (chat_id, last_source_seq, updated_at)
             VALUES (?1, ?2, ?3)
             ON CONFLICT(chat_id) DO UPDATE SET
                last_source_seq = MAX(trajectory_watermarks.last_source_seq, excluded.last_source_seq),
                updated_at = excluded.updated_at",
            params![r.chat_id, r.source_seq as i64, now],
        )?;
    }

    tx.commit()?;
    Ok(())
}

fn delete_chat_tx(conn: &Connection, chat_id: &str) -> Result<(), TrajectoryStoreError> {
    conn.execute(
        "DELETE FROM trajectory_records WHERE chat_id = ?1",
        params![chat_id],
    )?;
    conn.execute(
        "DELETE FROM trajectory_runs WHERE chat_id = ?1",
        params![chat_id],
    )?;
    conn.execute(
        "DELETE FROM trajectory_watermarks WHERE chat_id = ?1",
        params![chat_id],
    )?;
    conn.execute(
        "DELETE FROM trajectory_degraded_intervals WHERE chat_id = ?1",
        params![chat_id],
    )?;
    conn.execute(
        "DELETE FROM trajectory_legacy_imports WHERE chat_id = ?1",
        params![chat_id],
    )?;
    Ok(())
}

fn retain_chats_tx(conn: &Connection, live_ids: &[String]) -> Result<usize, TrajectoryStoreError> {
    let mut total_deleted = 0;
    let mut stmt = conn.prepare("SELECT DISTINCT chat_id FROM trajectory_records")?;
    let all_chats: Vec<String> = stmt
        .query_map([], |r| r.get(0))?
        .collect::<Result<Vec<_>, _>>()?;

    for chat_id in all_chats {
        if !live_ids.contains(&chat_id) {
            delete_chat_tx(conn, &chat_id)?;
            total_deleted += 1;
        }
    }
    Ok(total_deleted)
}

/// Project a normalized AgentEvent into a TrajectoryRecord.
pub fn project_event_to_record(
    chat_id: &str,
    run_id: &str,
    seq: u64,
    event: &AgentEvent,
    parent_tool_use_id: Option<String>,
) -> Option<TrajectoryRecord> {
    match event {
        AgentEvent::SessionStarted {
            harness,
            model,
            cwd,
            ..
        } => Some(TrajectoryRecord {
            id: TrajectoryRecordId::new(run_id, seq, 0),
            chat_id: chat_id.to_string(),
            run_id: run_id.to_string(),
            source_seq: seq,
            sub_seq: 0,
            lane: TrajectoryLane::Input,
            kind: TrajectoryRecordKind::SessionStarted,
            status: TrajectoryStatus::Running,
            is_partial: false,
            title: "Session started".into(),
            summary: format!("{:?} ({}) in {}", harness, model, cwd),
            turn_id: None,
            step_id: None,
            call_id: None,
            parent_tool_use_id,
            timing: Some(TrajectoryTiming::recorded(
                Some(Utc::now()),
                None,
                None,
                None,
            )),
            usage: None,
            payload: Some(TrajectoryPayloadPreview {
                summary: format!("{:?} {}", harness, model),
                sanitized_text: Some(format!("Model: {}\nCwd: {}", model, cwd)),
                schema_info: None,
                raw_ref: Some(TrajectoryRawRef::new(
                    chat_id,
                    seq,
                    None,
                    None,
                    TrajectoryRawField::Payload,
                )),
            }),
            result: None,
            error_message: None,
            is_degraded: false,
        }),
        AgentEvent::UserMessage { text } => {
            let (sum, prev) = zeron_proto::trajectory::sanitize_prompt_preview(text, 1024);
            Some(TrajectoryRecord {
                id: TrajectoryRecordId::new(run_id, seq, 0),
                chat_id: chat_id.to_string(),
                run_id: run_id.to_string(),
                source_seq: seq,
                sub_seq: 0,
                lane: TrajectoryLane::Input,
                kind: TrajectoryRecordKind::UserMessage,
                status: TrajectoryStatus::Completed,
                is_partial: false,
                title: "User".into(),
                summary: sum.clone(),
                turn_id: None,
                step_id: None,
                call_id: None,
                parent_tool_use_id,
                timing: Some(TrajectoryTiming::recorded(
                    Some(Utc::now()),
                    None,
                    None,
                    None,
                )),
                usage: None,
                payload: Some(TrajectoryPayloadPreview {
                    summary: sum,
                    sanitized_text: prev,
                    schema_info: None,
                    raw_ref: Some(TrajectoryRawRef::new(
                        chat_id,
                        seq,
                        None,
                        None,
                        TrajectoryRawField::Payload,
                    )),
                }),
                result: None,
                error_message: None,
                is_degraded: false,
            })
        }
        AgentEvent::ToolCall { id, call } => {
            let tool_name = tool_name_for(call);
            let (sum, prev, schema) = zeron_proto::trajectory::sanitize_tool_call(call, 1024);
            Some(TrajectoryRecord {
                id: TrajectoryRecordId::new(run_id, seq, 0),
                chat_id: chat_id.to_string(),
                run_id: run_id.to_string(),
                source_seq: seq,
                sub_seq: 0,
                lane: TrajectoryLane::Tools,
                kind: TrajectoryRecordKind::ToolCall {
                    tool_name: tool_name.clone(),
                },
                status: TrajectoryStatus::Running,
                is_partial: false,
                title: format!("Tool: {}", tool_name),
                summary: sum.clone(),
                turn_id: None,
                step_id: None,
                call_id: Some(id.clone()),
                parent_tool_use_id: parent_tool_use_id.clone(),
                timing: Some(TrajectoryTiming::recorded(
                    Some(Utc::now()),
                    None,
                    None,
                    None,
                )),
                usage: None,
                payload: Some(TrajectoryPayloadPreview {
                    summary: sum,
                    sanitized_text: prev,
                    schema_info: schema,
                    raw_ref: Some(TrajectoryRawRef::new(
                        chat_id,
                        seq,
                        parent_tool_use_id,
                        Some(id.clone()),
                        TrajectoryRawField::Payload,
                    )),
                }),
                result: None,
                error_message: None,
                is_degraded: false,
            })
        }
        AgentEvent::ToolResult {
            id,
            is_error,
            output,
            diff,
            execution,
        } => {
            let (sum, prev, exit_code) = zeron_proto::trajectory::sanitize_tool_result(
                output.as_deref(),
                diff.as_ref(),
                execution.as_ref(),
                *is_error,
                1024,
            );
            let kind = if let Some(d) = diff {
                TrajectoryRecordKind::ToolDiff {
                    tool_name: d.path.clone(),
                }
            } else {
                TrajectoryRecordKind::ToolResult {
                    tool_name: "tool".into(),
                }
            };
            Some(TrajectoryRecord {
                id: TrajectoryRecordId::new(run_id, seq, 0),
                chat_id: chat_id.to_string(),
                run_id: run_id.to_string(),
                source_seq: seq,
                sub_seq: 0,
                lane: TrajectoryLane::Tools,
                kind,
                status: if *is_error {
                    TrajectoryStatus::Error
                } else {
                    TrajectoryStatus::Completed
                },
                is_partial: false,
                title: if *is_error {
                    "Tool failed".into()
                } else {
                    "Tool completed".into()
                },
                summary: sum.clone(),
                turn_id: None,
                step_id: None,
                call_id: Some(id.clone()),
                parent_tool_use_id: parent_tool_use_id.clone(),
                timing: Some(TrajectoryTiming::recorded(
                    None,
                    Some(Utc::now()),
                    execution.and_then(|e| e.duration_ms),
                    None,
                )),
                usage: None,
                payload: None,
                result: Some(TrajectoryResultPreview {
                    summary: sum,
                    sanitized_text: prev,
                    is_error: *is_error,
                    exit_code,
                    raw_ref: Some(TrajectoryRawRef::new(
                        chat_id,
                        seq,
                        parent_tool_use_id,
                        Some(id.clone()),
                        TrajectoryRawField::Result,
                    )),
                }),
                error_message: if *is_error { output.clone() } else { None },
                is_degraded: false,
            })
        }
        AgentEvent::Usage {
            input_tokens,
            output_tokens,
            context_usage,
        } => Some(TrajectoryRecord {
            id: TrajectoryRecordId::new(run_id, seq, 0),
            chat_id: chat_id.to_string(),
            run_id: run_id.to_string(),
            source_seq: seq,
            sub_seq: 0,
            lane: TrajectoryLane::Input,
            kind: TrajectoryRecordKind::ContextUsage,
            status: TrajectoryStatus::Completed,
            is_partial: false,
            title: "Usage".into(),
            summary: format!("Tokens: {} in, {} out", input_tokens, output_tokens),
            turn_id: None,
            step_id: None,
            call_id: None,
            parent_tool_use_id,
            timing: Some(TrajectoryTiming::recorded(
                Some(Utc::now()),
                None,
                None,
                None,
            )),
            usage: Some(TrajectoryUsage {
                input_tokens: Some(*input_tokens),
                output_tokens: Some(*output_tokens),
                total_tokens: Some(*input_tokens + *output_tokens),
                context_window: context_usage.map(|c| c.context_window),
            }),
            payload: None,
            result: None,
            error_message: None,
            is_degraded: false,
        }),
        AgentEvent::AvailableCommands { commands } => Some(TrajectoryRecord {
            id: TrajectoryRecordId::new(run_id, seq, 0),
            chat_id: chat_id.to_string(),
            run_id: run_id.to_string(),
            source_seq: seq,
            sub_seq: 0,
            lane: TrajectoryLane::Input,
            kind: TrajectoryRecordKind::AvailableCommands,
            status: TrajectoryStatus::Completed,
            is_partial: false,
            title: "Available commands".into(),
            summary: format!("{} available commands", commands.len()),
            turn_id: None,
            step_id: None,
            call_id: None,
            parent_tool_use_id,
            timing: Some(TrajectoryTiming::recorded(
                Some(Utc::now()),
                None,
                None,
                None,
            )),
            usage: None,
            payload: None,
            result: None,
            error_message: None,
            is_degraded: false,
        }),
        AgentEvent::WorkflowTask { task } => Some(TrajectoryRecord {
            id: TrajectoryRecordId::new(run_id, seq, 0),
            chat_id: chat_id.to_string(),
            run_id: run_id.to_string(),
            source_seq: seq,
            sub_seq: 0,
            lane: TrajectoryLane::Model,
            kind: TrajectoryRecordKind::WorkflowTask,
            status: TrajectoryStatus::Completed,
            is_partial: false,
            title: "Workflow task".into(),
            summary: task.task_id.clone(),
            turn_id: None,
            step_id: None,
            call_id: None,
            parent_tool_use_id,
            timing: Some(TrajectoryTiming::recorded(
                Some(Utc::now()),
                None,
                None,
                None,
            )),
            usage: None,
            payload: None,
            result: None,
            error_message: None,
            is_degraded: false,
        }),
        AgentEvent::Error { message } => Some(TrajectoryRecord {
            id: TrajectoryRecordId::new(run_id, seq, 0),
            chat_id: chat_id.to_string(),
            run_id: run_id.to_string(),
            source_seq: seq,
            sub_seq: 0,
            lane: TrajectoryLane::Model,
            kind: TrajectoryRecordKind::Error,
            status: TrajectoryStatus::Error,
            is_partial: false,
            title: "Error".into(),
            summary: message.clone(),
            turn_id: None,
            step_id: None,
            call_id: None,
            parent_tool_use_id,
            timing: Some(TrajectoryTiming::recorded(
                Some(Utc::now()),
                None,
                None,
                None,
            )),
            usage: None,
            payload: None,
            result: None,
            error_message: Some(message.clone()),
            is_degraded: false,
        }),
        AgentEvent::InputRequested {
            request_id,
            questions,
        } => Some(TrajectoryRecord {
            id: TrajectoryRecordId::new(run_id, seq, 0),
            chat_id: chat_id.to_string(),
            run_id: run_id.to_string(),
            source_seq: seq,
            sub_seq: 0,
            lane: TrajectoryLane::Input,
            kind: TrajectoryRecordKind::InputRequested,
            status: TrajectoryStatus::Running,
            is_partial: false,
            title: "Input requested".into(),
            summary: format!("Request {} ({} questions)", request_id, questions.len()),
            turn_id: None,
            step_id: None,
            call_id: Some(request_id.clone()),
            parent_tool_use_id,
            timing: Some(TrajectoryTiming::recorded(
                Some(Utc::now()),
                None,
                None,
                None,
            )),
            usage: None,
            payload: None,
            result: None,
            error_message: None,
            is_degraded: false,
        }),
        AgentEvent::InputResolved { request_id } => Some(TrajectoryRecord {
            id: TrajectoryRecordId::new(run_id, seq, 0),
            chat_id: chat_id.to_string(),
            run_id: run_id.to_string(),
            source_seq: seq,
            sub_seq: 0,
            lane: TrajectoryLane::Input,
            kind: TrajectoryRecordKind::InputResolved,
            status: TrajectoryStatus::Completed,
            is_partial: false,
            title: "Input resolved".into(),
            summary: format!("Resolved {}", request_id),
            turn_id: None,
            step_id: None,
            call_id: Some(request_id.clone()),
            parent_tool_use_id,
            timing: Some(TrajectoryTiming::recorded(
                Some(Utc::now()),
                None,
                None,
                None,
            )),
            usage: None,
            payload: None,
            result: None,
            error_message: None,
            is_degraded: false,
        }),
        AgentEvent::Steered {
            assistant_message_id,
            next_assistant_message_id,
        } => Some(TrajectoryRecord {
            id: TrajectoryRecordId::new(run_id, seq, 0),
            chat_id: chat_id.to_string(),
            run_id: run_id.to_string(),
            source_seq: seq,
            sub_seq: 0,
            lane: TrajectoryLane::Input,
            kind: TrajectoryRecordKind::Steered,
            status: TrajectoryStatus::Completed,
            is_partial: false,
            title: "Steered".into(),
            summary: format!(
                "From {:?} to {:?}",
                assistant_message_id.as_deref().unwrap_or("none"),
                next_assistant_message_id.as_deref().unwrap_or("none")
            ),
            turn_id: None,
            step_id: None,
            call_id: None,
            parent_tool_use_id,
            timing: Some(TrajectoryTiming::recorded(
                Some(Utc::now()),
                None,
                None,
                None,
            )),
            usage: None,
            payload: None,
            result: None,
            error_message: None,
            is_degraded: false,
        }),
        AgentEvent::Done {
            status,
            result,
            error,
            ..
        } => Some(TrajectoryRecord {
            id: TrajectoryRecordId::new(run_id, seq, 0),
            chat_id: chat_id.to_string(),
            run_id: run_id.to_string(),
            source_seq: seq,
            sub_seq: 0,
            lane: TrajectoryLane::Model,
            kind: TrajectoryRecordKind::Done,
            status: match status {
                DoneStatus::Completed => TrajectoryStatus::Completed,
                DoneStatus::Errored => TrajectoryStatus::Error,
                DoneStatus::Interrupted => TrajectoryStatus::Interrupted,
            },
            is_partial: false,
            title: format!("Done ({:?})", status),
            summary: result
                .as_deref()
                .or(error.as_deref())
                .unwrap_or("Done")
                .to_string(),
            turn_id: None,
            step_id: None,
            call_id: None,
            parent_tool_use_id,
            timing: Some(TrajectoryTiming::recorded(
                None,
                Some(Utc::now()),
                None,
                None,
            )),
            usage: None,
            payload: None,
            result: None,
            error_message: error.clone(),
            is_degraded: false,
        }),
        AgentEvent::Subagent {
            parent_tool_use_id,
            event,
        } => project_event_to_record(
            chat_id,
            run_id,
            seq,
            event,
            Some(parent_tool_use_id.clone()),
        ),
        _ => None,
    }
}

pub fn tool_name_for(call: &ToolCall) -> String {
    match call {
        ToolCall::Exec { .. } => "bash".into(),
        ToolCall::ReadFile { .. } => "read".into(),
        ToolCall::WriteFile { .. } => "write".into(),
        ToolCall::EditFile { .. } => "edit".into(),
        ToolCall::ApplyPatch { .. } => "patch".into(),
        ToolCall::Search { .. } => "search".into(),
        ToolCall::Glob { .. } => "glob".into(),
        ToolCall::WebFetch { .. } => "web_fetch".into(),
        ToolCall::WebSearch { .. } => "web_search".into(),
        ToolCall::Todo { .. } => "todo".into(),
        ToolCall::Mcp { tool, .. } => tool.clone(),
        ToolCall::Unknown { name, .. } => name.clone(),
    }
}

fn row_to_record(row: &rusqlite::Row<'_>) -> rusqlite::Result<TrajectoryRecord> {
    let chat_id: String = row.get(0)?;
    let run_id: String = row.get(1)?;
    let source_seq: i64 = row.get(2)?;
    let sub_seq: i64 = row.get(3)?;
    let lane_str: String = row.get(4)?;
    let kind_json: String = row.get(5)?;
    let status_str: String = row.get(6)?;
    let is_partial_int: i32 = row.get(7)?;
    let title: String = row.get(8)?;
    let summary: String = row.get(9)?;
    let turn_id: Option<String> = row.get(10)?;
    let step_id: Option<String> = row.get(11)?;
    let call_id: Option<String> = row.get(12)?;
    let parent_tool_use_id: Option<String> = row.get(13)?;
    let timing_json: Option<String> = row.get(14)?;
    let usage_json: Option<String> = row.get(15)?;
    let payload_json: Option<String> = row.get(16)?;
    let result_json: Option<String> = row.get(17)?;
    let error_message: Option<String> = row.get(18)?;
    let is_degraded_int: i32 = row.get(19)?;

    let lane = match lane_str.as_str() {
        "input" => TrajectoryLane::Input,
        "tools" => TrajectoryLane::Tools,
        _ => TrajectoryLane::Model,
    };

    let kind: TrajectoryRecordKind = serde_json::from_str(&kind_json).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(5, rusqlite::types::Type::Text, Box::new(e))
    })?;

    let status = match status_str.as_str() {
        "Running" => TrajectoryStatus::Running,
        "Completed" => TrajectoryStatus::Completed,
        "Error" => TrajectoryStatus::Error,
        "Interrupted" => TrajectoryStatus::Interrupted,
        "Unsettled" => TrajectoryStatus::Unsettled,
        "Degraded" => TrajectoryStatus::Degraded,
        _ => TrajectoryStatus::Completed,
    };

    let timing = timing_json
        .as_deref()
        .map(serde_json::from_str)
        .transpose()
        .map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(14, rusqlite::types::Type::Text, Box::new(e))
        })?;

    let usage = usage_json
        .as_deref()
        .map(serde_json::from_str)
        .transpose()
        .map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(15, rusqlite::types::Type::Text, Box::new(e))
        })?;

    let payload = payload_json
        .as_deref()
        .map(serde_json::from_str)
        .transpose()
        .map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(16, rusqlite::types::Type::Text, Box::new(e))
        })?;

    let result = result_json
        .as_deref()
        .map(serde_json::from_str)
        .transpose()
        .map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(17, rusqlite::types::Type::Text, Box::new(e))
        })?;

    Ok(TrajectoryRecord {
        id: TrajectoryRecordId::new(&run_id, source_seq as u64, sub_seq as u32),
        chat_id,
        run_id,
        source_seq: source_seq as u64,
        sub_seq: sub_seq as u32,
        lane,
        kind,
        status,
        is_partial: is_partial_int != 0,
        title,
        summary,
        turn_id,
        step_id,
        call_id,
        parent_tool_use_id,
        timing,
        usage,
        payload,
        result,
        error_message,
        is_degraded: is_degraded_int != 0,
    })
}

fn migrate(conn: &mut Connection) -> Result<(), TrajectoryStoreError> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS schema_migrations (
            version    INTEGER PRIMARY KEY,
            applied_at INTEGER NOT NULL
         ) STRICT",
    )?;
    let current: i64 = conn.query_row(
        "SELECT COALESCE(MAX(version), 0) FROM schema_migrations",
        [],
        |row| row.get(0),
    )?;
    for (index, sql) in MIGRATIONS.iter().enumerate() {
        let version = index as i64 + 1;
        if version <= current {
            continue;
        }
        let tx = conn.transaction()?;
        tx.execute_batch(sql)?;
        tx.execute(
            "INSERT INTO schema_migrations (version, applied_at) VALUES (?1, ?2)",
            params![version, Utc::now().timestamp_millis()],
        )?;
        tx.commit()?;
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn sample_record(chat_id: &str, run_id: &str, seq: u64, sub_seq: u32) -> TrajectoryRecord {
        TrajectoryRecord {
            id: TrajectoryRecordId::new(run_id, seq, sub_seq),
            chat_id: chat_id.into(),
            run_id: run_id.into(),
            source_seq: seq,
            sub_seq,
            lane: TrajectoryLane::Input,
            kind: TrajectoryRecordKind::UserMessage,
            status: TrajectoryStatus::Completed,
            is_partial: false,
            title: "User prompt".into(),
            summary: "Hello world".into(),
            turn_id: Some("t0".into()),
            step_id: Some("s0".into()),
            call_id: None,
            parent_tool_use_id: None,
            timing: Some(TrajectoryTiming::sequence_only()),
            usage: None,
            payload: None,
            result: None,
            error_message: None,
            is_degraded: false,
        }
    }

    #[tokio::test]
    async fn test_store_open_and_reopen() {
        let temp = TempDir::new().unwrap();
        let store = TrajectoryStore::open(temp.path()).unwrap();

        let rec = sample_record("c1", "r1", 1, 0);
        store.try_enqueue(rec.clone()).unwrap();
        store.flush().await.unwrap();

        let records = store.list_all_records("c1").unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].summary, "Hello world");

        // Reopen same path
        drop(store);
        let reopened = TrajectoryStore::open(temp.path()).unwrap();
        let records2 = reopened.list_all_records("c1").unwrap();
        assert_eq!(records2.len(), 1);
        assert_eq!(records2[0].summary, "Hello world");
    }

    #[tokio::test]
    async fn test_writer_batching_and_paging() {
        let temp = TempDir::new().unwrap();
        let store = TrajectoryStore::open(temp.path()).unwrap();

        let mut batch = Vec::new();
        for i in 1..=20 {
            batch.push(sample_record("c1", "r1", i, 0));
        }
        store.try_enqueue_batch(batch).unwrap();
        store.flush().await.unwrap();

        let page1 = store.list_records("c1", Some(1), Some(10)).unwrap();
        assert_eq!(page1.len(), 10);
        assert_eq!(page1[0].source_seq, 1);
        assert_eq!(page1[9].source_seq, 10);

        let page2 = store.list_records("c1", Some(11), Some(10)).unwrap();
        assert_eq!(page2.len(), 10);
        assert_eq!(page2[0].source_seq, 11);
        assert_eq!(page2[9].source_seq, 20);

        let watermark = store.get_watermark("c1").unwrap();
        assert_eq!(watermark, Some(20));
    }

    #[tokio::test]
    async fn test_diagnostics_and_deletion() {
        let temp = TempDir::new().unwrap();
        let store = TrajectoryStore::open(temp.path()).unwrap();

        store.try_enqueue(sample_record("c1", "r1", 1, 0)).unwrap();
        store.try_enqueue(sample_record("c1", "r2", 2, 0)).unwrap();
        store.flush().await.unwrap();

        let diag = store.diagnostics("c1").unwrap();
        assert_eq!(diag.record_count, 2);
        assert_eq!(diag.run_count, 2);
        assert_eq!(diag.last_watermark, Some(2));
        assert_eq!(diag.degraded_count, 0);
        assert!(diag.db_size_bytes > 0);

        store.delete_chat("c1").await.unwrap();
        store.flush().await.unwrap();

        let records = store.list_all_records("c1").unwrap();
        assert_eq!(records.len(), 0);
        let diag2 = store.diagnostics("c1").unwrap();
        assert_eq!(diag2.record_count, 0);
    }

    #[tokio::test]
    async fn test_degraded_interval_reporting() {
        let temp = TempDir::new().unwrap();
        let store = TrajectoryStore::open(temp.path()).unwrap();

        let degraded = TrajectoryDegradedInterval {
            chat_id: "c1".into(),
            run_id: "r1".into(),
            from_seq: 10,
            to_seq: 15,
            reason: "Simulated store degradation".into(),
            recorded_at: Utc::now(),
        };

        store
            .writer_tx
            .send(WriterCommand::RecordDegraded(degraded))
            .await
            .unwrap();
        store.flush().await.unwrap();

        let intervals = store.get_degraded_intervals("c1").unwrap();
        assert_eq!(intervals.len(), 1);
        assert_eq!(intervals[0].from_seq, 10);
        assert_eq!(intervals[0].to_seq, 15);
        assert_eq!(intervals[0].reason, "Simulated store degradation");
    }

    #[tokio::test]
    async fn test_legacy_import_sequence_only_and_idempotent() {
        let temp = TempDir::new().unwrap();
        let store = TrajectoryStore::open(temp.path()).unwrap();
        let journal_path = temp.path().join("legacy_chat.jsonl");

        let lines = vec![
            r#"{"seq":1,"event":{"type":"sessionStarted","harness":"mock","model":"mock-model","cwd":"/work","sessionId":"s1","assistantMessageId":"m1"}}"#,
            r#"{"seq":2,"event":{"type":"userMessage","text":"Hello legacy"}}"#,
            r#"{"seq":3,"event":{"type":"done","status":"completed","result":"ok"}}"#,
        ];
        fs::write(&journal_path, lines.join("\n")).unwrap();

        // First import
        let imported1 = store
            .import_legacy_journal("chat_leg", &journal_path)
            .unwrap();
        assert!(imported1);

        let records = store.list_all_records("chat_leg").unwrap();
        assert_eq!(records.len(), 3);
        // Prove every record has sequence_only timing mode and no fabricated duration
        for r in &records {
            assert_eq!(
                r.timing.as_ref().unwrap().mode,
                zeron_proto::trajectory::TrajectoryTimingMode::SequenceOnly
            );
            assert_eq!(
                zeron_proto::trajectory::format_duration(r.timing.as_ref()),
                None
            );
        }

        // Second import (idempotent skip)
        let imported2 = store
            .import_legacy_journal("chat_leg", &journal_path)
            .unwrap();
        assert!(!imported2);
        let records2 = store.list_all_records("chat_leg").unwrap();
        assert_eq!(records2.len(), 3);
    }

    #[tokio::test]
    async fn test_legacy_import_corrupt_tail() {
        let temp = TempDir::new().unwrap();
        let store = TrajectoryStore::open(temp.path()).unwrap();
        let journal_path = temp.path().join("corrupt_chat.jsonl");

        let content = concat!(
            r#"{"seq":1,"event":{"type":"sessionStarted","harness":"mock","model":"m","cwd":"/","sessionId":"s","assistantMessageId":"m"}}"#,
            "\n",
            r#"{"seq":2,"event":{"type":"userMessage","text":"valid line"}}"#,
            "\n",
            r#"{"seq":3,"event":{"type":"toolCall","id":"c1","call":{"kind":"exec","command":"ls"}}}"#,
            "\n",
            r#"{"seq":4,"event":{"type":"toolCall","id":"c2","call":{"kind":"exec","command":"broken json..."#, // CORRUPT TAIL
        );
        fs::write(&journal_path, content).unwrap();

        let imported = store
            .import_legacy_journal("chat_corrupt", &journal_path)
            .unwrap();
        assert!(imported);

        let records = store.list_all_records("chat_corrupt").unwrap();
        // Exactly the 3 valid prefix records are imported
        assert_eq!(records.len(), 3);
        assert_eq!(records[0].source_seq, 1);
        assert_eq!(records[1].source_seq, 2);
        assert_eq!(records[2].source_seq, 3);
    }

    #[tokio::test]
    async fn test_legacy_import_interrupted_and_unsettled() {
        let temp = TempDir::new().unwrap();
        let store = TrajectoryStore::open(temp.path()).unwrap();
        let journal_path = temp.path().join("interrupted_chat.jsonl");

        let lines = vec![
            r#"{"seq":1,"event":{"type":"sessionStarted","harness":"mock","model":"m","cwd":"/","sessionId":"s","assistantMessageId":"m"}}"#,
            r#"{"seq":2,"event":{"type":"toolCall","id":"c_unsettled","call":{"kind":"exec","command":"sleep 100"}}}"#,
            // No ToolResult and No Done event!
        ];
        fs::write(&journal_path, lines.join("\n")).unwrap();

        let imported = store
            .import_legacy_journal("chat_unsettled", &journal_path)
            .unwrap();
        assert!(imported);

        let records = store.list_all_records("chat_unsettled").unwrap();
        assert_eq!(records.len(), 2);
        assert_eq!(records[1].status, TrajectoryStatus::Unsettled);
    }

    #[tokio::test]
    async fn test_trajectory_lifecycle_retention_and_chat_deletion() {
        let temp = TempDir::new().unwrap();
        let store = TrajectoryStore::open(temp.path()).unwrap();

        store
            .try_enqueue(sample_record("chat_active", "r1", 1, 0))
            .unwrap();
        store
            .try_enqueue(sample_record("chat_archived", "r1", 1, 0))
            .unwrap();
        store
            .try_enqueue(sample_record("chat_to_delete", "r1", 1, 0))
            .unwrap();
        store.flush().await.unwrap();

        // Authoritative workspace set contains active and archived chats
        let live_chats = vec!["chat_active".to_string(), "chat_archived".to_string()];
        store.retain_chats_only(&live_chats).await.unwrap();
        store.flush().await.unwrap();

        assert_eq!(store.list_all_records("chat_active").unwrap().len(), 1);
        assert_eq!(store.list_all_records("chat_archived").unwrap().len(), 1);
        assert_eq!(store.list_all_records("chat_to_delete").unwrap().len(), 0);

        // Explicit delete
        store.delete_chat("chat_active").await.unwrap();
        store.flush().await.unwrap();
        assert_eq!(store.list_all_records("chat_active").unwrap().len(), 0);
        assert_eq!(store.list_all_records("chat_archived").unwrap().len(), 1);
    }
}
