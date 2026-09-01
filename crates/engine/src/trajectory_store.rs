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
use std::sync::{Arc, Mutex};
use std::time::Duration;

use chrono::{TimeZone, Utc};
use rusqlite::{Connection, OpenFlags, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use std::sync::mpsc::{SyncSender, TrySendError, sync_channel};
use tokio::sync::oneshot;
use zeron_proto::trajectory::*;
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

pub(crate) enum ReplySender<T: Send + 'static> {
    Async(oneshot::Sender<T>),
    Sync(SyncSender<T>),
}

impl<T: Send + 'static> ReplySender<T> {
    fn send(self, val: T) {
        match self {
            ReplySender::Async(tx) => {
                let _ = tx.send(val);
            }
            ReplySender::Sync(tx) => {
                let _ = tx.send(val);
            }
        }
    }
}

/// Commands for the background writer task.
pub(crate) enum WriterCommand {
    WriteRecords(Vec<TrajectoryRecord>),
    DeleteChat(
        String,
        Option<ReplySender<Result<(), TrajectoryStoreError>>>,
    ),
    RetainChats(
        Vec<String>,
        Option<ReplySender<Result<usize, TrajectoryStoreError>>>,
    ),
    ImportLegacy {
        chat_id: String,
        fingerprint: String,
        records: Vec<TrajectoryRecord>,
        reply: Option<ReplySender<Result<(), TrajectoryStoreError>>>,
    },
    Flush(ReplySender<()>),
}

/// Default capacity for the nonblocking capture queue.
const CAPTURE_QUEUE_CAPACITY: usize = 2048;

/// Device-local SQLite trajectory store.
#[derive(Clone)]
pub struct TrajectoryStore {
    pub(crate) db_path: PathBuf,
    pub(crate) journals_dir: PathBuf,
    pub(crate) writer_tx: SyncSender<WriterCommand>,
    pub(crate) in_memory_degraded: Arc<Mutex<Vec<TrajectoryDegradedInterval>>>,
    pub(crate) degraded_reason: Arc<Mutex<Option<String>>>,
}

impl TrajectoryStore {
    /// Open or create the trajectory store at `{store_root}/trajectory.sqlite3`.
    pub fn open(store_root: impl AsRef<Path>) -> Result<Self, TrajectoryStoreError> {
        let store_root = store_root.as_ref();
        fs::create_dir_all(store_root)?;
        let db_path = store_root.join("trajectory.sqlite3");
        let journals_dir = store_root.join("journals");
        // Run initial migrations on open
        {
            let mut conn = Connection::open(&db_path)?;
            conn.pragma_update(None, "journal_mode", "WAL")?;
            conn.pragma_update(None, "synchronous", "NORMAL")?;
            conn.busy_timeout(Duration::from_secs(5))?;
            migrate(&mut conn)?;
        }

        let (writer_tx, writer_rx) = sync_channel::<WriterCommand>(CAPTURE_QUEUE_CAPACITY);
        let writer_db_path = db_path.clone();
        let in_memory_degraded = Arc::new(Mutex::new(Vec::new()));
        let writer_in_mem = in_memory_degraded.clone();

        // Spawn ordered background writer OS thread
        std::thread::Builder::new()
            .name("trajectory-writer".into())
            .spawn(move || {
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

                while let Ok(cmd) = writer_rx.recv() {
                    match cmd {
                        WriterCommand::WriteRecords(mut records) => {
                            // Drain any immediately available batched records
                            while let Ok(next) = writer_rx.try_recv() {
                                match next {
                                    WriterCommand::WriteRecords(more) => records.extend(more),
                                    other => {
                                        flush_batch_to_writer(&mut conn, &records, &writer_in_mem);
                                        records.clear();
                                        handle_writer_command(&mut conn, other, &writer_in_mem);
                                        break;
                                    }
                                }
                            }
                            if !records.is_empty() {
                                flush_batch_to_writer(&mut conn, &records, &writer_in_mem);
                            }
                        }
                        other => {
                            handle_writer_command(&mut conn, other, &writer_in_mem);
                        }
                    }
                }
            })
            .map_err(|e| TrajectoryStoreError::Other(e.to_string()))?;

        Ok(Self {
            db_path,
            journals_dir,
            writer_tx,
            in_memory_degraded,
            degraded_reason: Arc::new(Mutex::new(None)),
        })
    }

    /// Construct a degraded trajectory store that logs operations and reports degradation without panicking.
    pub fn degraded(store_root: impl AsRef<Path>, reason: impl Into<String>) -> Self {
        let db_path = store_root.as_ref().join("trajectory.sqlite3");
        let journals_dir = store_root.as_ref().join("journals");
        let (writer_tx, _) = sync_channel(1);
        Self {
            db_path,
            journals_dir,
            writer_tx,
            in_memory_degraded: Arc::new(Mutex::new(Vec::new())),
            degraded_reason: Arc::new(Mutex::new(Some(reason.into()))),
        }
    }

    /// True if this store is running in degraded mode due to initialization failure.
    pub fn is_degraded(&self) -> bool {
        self.degraded_reason.lock().unwrap().is_some()
    }

    /// Determine the minimum committed native (non-legacy) `source_seq` for `chat_id`.
    pub fn min_native_source_seq(
        &self,
        chat_id: &str,
    ) -> Result<Option<u64>, TrajectoryStoreError> {
        let conn = self.reader()?;
        let min_seq: Option<i64> = conn
            .query_row(
                "SELECT MIN(source_seq) FROM trajectory_records WHERE chat_id = ?1 AND run_id NOT LIKE 'legacy_%'",
                params![chat_id],
                |r| r.get(0),
            )
            .optional()?
            .flatten();
        Ok(min_seq.map(|s| s as u64))
    }

    /// Check if native (non-legacy) records exist for `chat_id`.
    pub fn has_native_records(&self, chat_id: &str) -> Result<bool, TrajectoryStoreError> {
        Ok(self.min_native_source_seq(chat_id)?.is_some())
    }

    /// Check if legacy journal has already been imported for `chat_id`.
    pub fn has_legacy_import(&self, chat_id: &str) -> Result<bool, TrajectoryStoreError> {
        let conn = self.reader()?;
        let exists: bool = conn
            .query_row(
                "SELECT 1 FROM trajectory_legacy_imports WHERE chat_id = ?1 LIMIT 1",
                params![chat_id],
                |_| Ok(true),
            )
            .optional()?
            .unwrap_or(false);
        Ok(exists)
    }

    /// Lazily ensure eligible legacy Run Journal data for `chat_id` is imported on first access.
    pub fn ensure_legacy_imported(&self, chat_id: &str) -> Result<(), TrajectoryStoreError> {
        if self.is_degraded() {
            return Ok(());
        }
        // One-shot: if already imported once, do nothing
        if self.has_legacy_import(chat_id).unwrap_or(false) {
            return Ok(());
        }
        // Synchronize with writer so any already enqueued native records are committed
        let _ = self.sync_flush();
        if self.has_legacy_import(chat_id).unwrap_or(false) {
            return Ok(());
        }
        let journal_path = crate::run_journal::journal_paths(&self.journals_dir, chat_id).0;
        if journal_path.exists() {
            let _ = self.import_legacy_journal(chat_id, &journal_path)?;
        }
        Ok(())
    }

    fn record_degraded_in_memory(&self, degraded: TrajectoryDegradedInterval) {
        let mut in_mem = self.in_memory_degraded.lock().unwrap();
        if !in_mem.iter().any(|d| {
            d.chat_id == degraded.chat_id
                && d.run_id == degraded.run_id
                && d.from_seq == degraded.from_seq
                && d.to_seq == degraded.to_seq
                && d.reason == degraded.reason
        }) {
            in_mem.push(degraded);
        }
    }

    /// Enqueue a captured record nonblockingly.
    ///
    /// If the queue is saturated or the store is degraded, this method records a degraded interval rather than
    /// blocking synchronous publication.
    pub fn try_enqueue(&self, record: TrajectoryRecord) -> Result<(), TrajectoryStoreError> {
        if let Some(reason) = self.degraded_reason.lock().unwrap().as_ref() {
            let degraded = TrajectoryDegradedInterval {
                chat_id: record.chat_id.clone(),
                run_id: record.run_id.clone(),
                from_seq: record.source_seq,
                to_seq: record.source_seq,
                reason: format!("Store degraded: {}", reason),
                recorded_at: Utc::now(),
            };
            self.record_degraded_in_memory(degraded);
            return Err(TrajectoryStoreError::Other(format!(
                "store degraded: {}",
                reason
            )));
        }
        match self
            .writer_tx
            .try_send(WriterCommand::WriteRecords(vec![record.clone()]))
        {
            Ok(()) => Ok(()),
            Err(TrySendError::Full(_)) => {
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
                // Direct in-memory recording does not depend on space in the full queue!
                self.record_degraded_in_memory(degraded);
                Err(TrajectoryStoreError::QueueFull)
            }
            Err(TrySendError::Disconnected(_)) => {
                let degraded = TrajectoryDegradedInterval {
                    chat_id: record.chat_id.clone(),
                    run_id: record.run_id.clone(),
                    from_seq: record.source_seq,
                    to_seq: record.source_seq,
                    reason: "Writer channel closed".into(),
                    recorded_at: Utc::now(),
                };
                self.record_degraded_in_memory(degraded);
                Err(TrajectoryStoreError::ChannelClosed)
            }
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
        if let Some(reason) = self.degraded_reason.lock().unwrap().as_ref() {
            if let Some(first) = records.first() {
                let last = records.last().unwrap_or(first);
                let degraded = TrajectoryDegradedInterval {
                    chat_id: first.chat_id.clone(),
                    run_id: first.run_id.clone(),
                    from_seq: first.source_seq,
                    to_seq: last.source_seq,
                    reason: format!("Store degraded: {}", reason),
                    recorded_at: Utc::now(),
                };
                self.record_degraded_in_memory(degraded);
            }
            return Err(TrajectoryStoreError::Other(format!(
                "store degraded: {}",
                reason
            )));
        }
        match self
            .writer_tx
            .try_send(WriterCommand::WriteRecords(records))
        {
            Ok(()) => Ok(()),
            Err(TrySendError::Full(WriterCommand::WriteRecords(recs))) => {
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
                    self.record_degraded_in_memory(degraded);
                }
                Err(TrajectoryStoreError::QueueFull)
            }
            Err(TrySendError::Full(_)) => Err(TrajectoryStoreError::QueueFull),
            Err(TrySendError::Disconnected(_)) => Err(TrajectoryStoreError::ChannelClosed),
        }
    }
    /// Flush the background writer queue and await completion.
    pub async fn flush(&self) -> Result<(), TrajectoryStoreError> {
        let (tx, rx) = oneshot::channel();
        let writer_tx = self.writer_tx.clone();
        tokio::task::spawn_blocking(move || {
            writer_tx.send(WriterCommand::Flush(ReplySender::Async(tx)))
        })
        .await
        .map_err(|_| TrajectoryStoreError::ChannelClosed)?
        .map_err(|_| TrajectoryStoreError::ChannelClosed)?;
        rx.await.map_err(|_| TrajectoryStoreError::ChannelClosed)
    }

    /// Synchronously flush the background writer queue and await completion.
    pub fn sync_flush(&self) -> Result<(), TrajectoryStoreError> {
        if self.is_degraded() {
            return Ok(());
        }
        let (tx, rx) = sync_channel(1);
        self.writer_tx
            .send(WriterCommand::Flush(ReplySender::Sync(tx)))
            .map_err(|_| TrajectoryStoreError::ChannelClosed)?;
        rx.recv().map_err(|_| TrajectoryStoreError::ChannelClosed)
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
        if self.is_degraded() {
            return Ok(Vec::new());
        }
        let _ = self.ensure_legacy_imported(chat_id);
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
        if self.is_degraded() {
            return Ok(None);
        }
        let _ = self.ensure_legacy_imported(chat_id);
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
        let mut intervals = Vec::new();

        if let Some(reason) = self.degraded_reason.lock().unwrap().as_ref() {
            intervals.push(TrajectoryDegradedInterval {
                chat_id: chat_id.to_string(),
                run_id: "init".to_string(),
                from_seq: 0,
                to_seq: 0,
                reason: format!("Store initialization failed: {}", reason),
                recorded_at: Utc::now(),
            });
        }

        // In-memory degraded intervals
        {
            let in_mem = self.in_memory_degraded.lock().unwrap();
            for inv in in_mem.iter() {
                if inv.chat_id == chat_id {
                    intervals.push(inv.clone());
                }
            }
        }

        // Persisted degraded intervals from SQLite (if reader succeeds)
        if let Ok(conn) = self.reader() {
            if let Ok(mut stmt) = conn.prepare(
                "SELECT chat_id, run_id, from_seq, to_seq, reason, recorded_at
                 FROM trajectory_degraded_intervals
                 WHERE chat_id = ?1
                 ORDER BY from_seq ASC",
            ) {
                if let Ok(rows) = stmt.query_map(params![chat_id], |row| {
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
                }) {
                    for r in rows.flatten() {
                        if !intervals.iter().any(|existing| {
                            existing.run_id == r.run_id
                                && existing.from_seq == r.from_seq
                                && existing.to_seq == r.to_seq
                                && existing.reason == r.reason
                        }) {
                            intervals.push(r);
                        }
                    }
                }
            }
        }

        intervals.sort_by_key(|i| i.from_seq);
        Ok(intervals)
    }

    /// Fetch diagnostics summary for `chat_id`.
    pub fn diagnostics(
        &self,
        chat_id: &str,
    ) -> Result<TrajectoryDiagnostics, TrajectoryStoreError> {
        if self.is_degraded() {
            return Ok(TrajectoryDiagnostics {
                chat_id: chat_id.to_string(),
                record_count: 0,
                run_count: 0,
                last_watermark: None,
                degraded_count: 1,
                db_size_bytes: 0,
            });
        }
        let _ = self.ensure_legacy_imported(chat_id);
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
        let writer_tx = self.writer_tx.clone();
        let chat_id = chat_id.to_string();
        tokio::task::spawn_blocking(move || {
            writer_tx.send(WriterCommand::DeleteChat(
                chat_id,
                Some(ReplySender::Async(tx)),
            ))
        })
        .await
        .map_err(|_| TrajectoryStoreError::ChannelClosed)?
        .map_err(|_| TrajectoryStoreError::ChannelClosed)?;
        rx.await.map_err(|_| TrajectoryStoreError::ChannelClosed)?
    }

    /// Retain only the specified active Chat IDs, removing stale records from any deleted Chats.
    pub async fn retain_chats_only(
        &self,
        live_chat_ids: &[String],
    ) -> Result<usize, TrajectoryStoreError> {
        let (tx, rx) = oneshot::channel();
        let writer_tx = self.writer_tx.clone();
        let live_chat_ids = live_chat_ids.to_vec();
        tokio::task::spawn_blocking(move || {
            writer_tx.send(WriterCommand::RetainChats(
                live_chat_ids,
                Some(ReplySender::Async(tx)),
            ))
        })
        .await
        .map_err(|_| TrajectoryStoreError::ChannelClosed)?
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

    /// Synchronous delete chat helper (for direct tests), routed through the ordered writer.
    pub fn sync_delete_chat(&self, chat_id: &str) -> Result<(), TrajectoryStoreError> {
        let (tx, rx) = sync_channel(1);
        self.writer_tx
            .send(WriterCommand::DeleteChat(
                chat_id.to_string(),
                Some(ReplySender::Sync(tx)),
            ))
            .map_err(|_| TrajectoryStoreError::ChannelClosed)?;
        rx.recv().map_err(|_| TrajectoryStoreError::ChannelClosed)?
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
        if self.is_degraded() {
            return Ok(false);
        }

        // Synchronize with writer before reading import status and native minimum seq
        let _ = self.sync_flush();

        // Check if already imported (one-shot / idempotent)
        if self.has_legacy_import(chat_id)? {
            return Ok(false);
        }

        // Determine the minimum committed native source_seq for this chat
        let min_native_seq = self.min_native_source_seq(chat_id)?;

        // If native rows start at seq 1 (N = 1), record a zero-row completed import marker and import nothing
        if min_native_seq == Some(1) {
            let metadata = fs::metadata(path)?;
            let file_len = metadata.len();
            let mtime = metadata
                .modified()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_millis())
                .unwrap_or(0);
            let fingerprint = format!("{}:{}:{}", path.display(), file_len, mtime);

            let (tx, rx) = sync_channel(1);
            let cmd = WriterCommand::ImportLegacy {
                chat_id: chat_id.to_string(),
                fingerprint,
                records: Vec::new(),
                reply: Some(ReplySender::Sync(tx)),
            };
            self.writer_tx
                .send(cmd)
                .map_err(|_| TrajectoryStoreError::ChannelClosed)?;
            rx.recv()
                .map_err(|_| TrajectoryStoreError::ChannelClosed)??;
            return Ok(true);
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

        let file = fs::File::open(path)?;
        let reader = std::io::BufReader::new(file);
        use std::io::BufRead;

        let run_id = format!("legacy_{}", chat_id);
        let mut records: Vec<TrajectoryRecord> = Vec::new();
        let mut has_done = false;
        let mut pending_tools: std::collections::HashSet<String> = std::collections::HashSet::new();

        let mut current_text: Option<(u64, String)> = None;
        let mut current_reasoning: Option<(u64, String)> = None;

        let flush_text = |records: &mut Vec<TrajectoryRecord>,
                          current: &mut Option<(u64, String)>| {
            if let Some((first_seq, text)) = current.take() {
                if !text.is_empty() {
                    let (sum, _prev) =
                        zeron_proto::trajectory::sanitize_prompt_preview(&text, 1024);
                    let rec = TrajectoryRecord {
                        id: TrajectoryRecordId::new(&run_id, first_seq, 0),
                        chat_id: chat_id.to_string(),
                        run_id: run_id.clone(),
                        source_seq: first_seq,
                        sub_seq: 0,
                        lane: TrajectoryLane::Model,
                        kind: TrajectoryRecordKind::AssistantMessage,
                        status: TrajectoryStatus::Completed,
                        is_partial: false,
                        title: "Assistant".into(),
                        summary: sum,
                        turn_id: None,
                        step_id: None,
                        call_id: None,
                        parent_tool_use_id: None,
                        timing: Some(TrajectoryTiming::sequence_only()),
                        usage: None,
                        payload: Some(TrajectoryPayloadPreview {
                            summary: "Response completed".to_string(),
                            sanitized_text: Some(zeron_proto::trajectory::truncate_preview(
                                &text, 1024,
                            )),
                            schema_info: None,
                            raw_ref: Some(TrajectoryRawRef::new(
                                chat_id,
                                first_seq,
                                None,
                                None,
                                TrajectoryRawField::Payload,
                            )),
                        }),
                        result: None,
                        error_message: None,
                        is_degraded: false,
                    };
                    records.push(rec);
                }
            }
        };

        let flush_reasoning = |records: &mut Vec<TrajectoryRecord>,
                               current: &mut Option<(u64, String)>| {
            if let Some((first_seq, text)) = current.take() {
                if !text.is_empty() {
                    let (sum, _prev) =
                        zeron_proto::trajectory::sanitize_prompt_preview(&text, 1024);
                    let rec = TrajectoryRecord {
                        id: TrajectoryRecordId::new(&run_id, first_seq, 1),
                        chat_id: chat_id.to_string(),
                        run_id: run_id.clone(),
                        source_seq: first_seq,
                        sub_seq: 1,
                        lane: TrajectoryLane::Model,
                        kind: TrajectoryRecordKind::Reasoning,
                        status: TrajectoryStatus::Completed,
                        is_partial: false,
                        title: "Reasoning".into(),
                        summary: sum,
                        turn_id: None,
                        step_id: None,
                        call_id: None,
                        parent_tool_use_id: None,
                        timing: Some(TrajectoryTiming::sequence_only()),
                        usage: None,
                        payload: Some(TrajectoryPayloadPreview {
                            summary: "Reasoning completed".to_string(),
                            sanitized_text: Some(zeron_proto::trajectory::truncate_preview(
                                &text, 1024,
                            )),
                            schema_info: None,
                            raw_ref: Some(TrajectoryRawRef::new(
                                chat_id,
                                first_seq,
                                None,
                                None,
                                TrajectoryRawField::Payload,
                            )),
                        }),
                        result: None,
                        error_message: None,
                        is_degraded: false,
                    };
                    records.push(rec);
                }
            }
        };

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

            // Coverage cutover: if native rows start at N, import only events with seq < N
            if let Some(cutover_seq) = min_native_seq {
                if seq >= cutover_seq {
                    break;
                }
            }

            match &event {
                zeron_proto::AgentEvent::ReasoningDelta { text } => {
                    flush_text(&mut records, &mut current_text);
                    if let Some((_, accumulated)) = &mut current_reasoning {
                        accumulated.push_str(text);
                    } else {
                        current_reasoning = Some((seq, text.clone()));
                    }
                }
                zeron_proto::AgentEvent::TextDelta { text } => {
                    flush_reasoning(&mut records, &mut current_reasoning);
                    if let Some((_, accumulated)) = &mut current_text {
                        accumulated.push_str(text);
                    } else {
                        current_text = Some((seq, text.clone()));
                    }
                }
                _ => {
                    flush_reasoning(&mut records, &mut current_reasoning);
                    flush_text(&mut records, &mut current_text);

                    if matches!(event, zeron_proto::AgentEvent::Done { .. }) {
                        has_done = true;
                    }
                    if let zeron_proto::AgentEvent::ToolCall { id, .. } = &event {
                        pending_tools.insert(id.clone());
                    }
                    if let zeron_proto::AgentEvent::ToolResult { id, .. } = &event {
                        pending_tools.remove(id);
                    }

                    if let Some(mut rec) =
                        project_event_to_record(chat_id, &run_id, seq, &event, None)
                    {
                        rec.timing = Some(TrajectoryTiming::sequence_only());
                        records.push(rec);
                    }
                }
            }
        }

        flush_reasoning(&mut records, &mut current_reasoning);
        flush_text(&mut records, &mut current_text);

        // If the journal had no terminal Done event, mark unsettled tool calls
        // and add an interrupted record.
        if !has_done && !records.is_empty() {
            for rec in &mut records {
                if let Some(call_id) = &rec.call_id {
                    if pending_tools.contains(call_id)
                        && matches!(rec.kind, TrajectoryRecordKind::ToolCall { .. })
                    {
                        rec.status = TrajectoryStatus::Unsettled;
                    }
                }
            }
            let last_seq = records.last().map(|r| r.source_seq).unwrap_or(0);
            let (terminal_seq, terminal_sub_seq) = if min_native_seq.is_some() {
                // When native rows follow at >= last_seq + 1, place the Interrupted terminal
                // at the final prefix source sequence with a reserved nonconflicting sub-sequence (u32::MAX),
                // so it does not collide with or share the ordering slot of the first native row (cutover_seq, 0).
                (last_seq, u32::MAX)
            } else {
                // In legacy-only journals without native continuation, place at last_seq + 1, 0.
                (last_seq + 1, 0)
            };
            let interrupted_rec = TrajectoryRecord {
                id: TrajectoryRecordId::new(&run_id, terminal_seq, terminal_sub_seq),
                chat_id: chat_id.to_string(),
                run_id: run_id.clone(),
                source_seq: terminal_seq,
                sub_seq: terminal_sub_seq,
                lane: TrajectoryLane::Model,
                kind: TrajectoryRecordKind::Done,
                status: TrajectoryStatus::Interrupted,
                is_partial: false,
                title: "Done (Interrupted)".to_string(),
                summary: "Interrupted".to_string(),
                turn_id: None,
                step_id: None,
                call_id: None,
                parent_tool_use_id: None,
                timing: Some(TrajectoryTiming::sequence_only()),
                usage: None,
                payload: None,
                result: None,
                error_message: Some("Run interrupted".to_string()),
                is_degraded: false,
            };
            records.push(interrupted_rec);
        }

        let (tx, rx) = sync_channel(1);
        let cmd = WriterCommand::ImportLegacy {
            chat_id: chat_id.to_string(),
            fingerprint,
            records,
            reply: Some(ReplySender::Sync(tx)),
        };

        self.writer_tx
            .send(cmd)
            .map_err(|_| TrajectoryStoreError::ChannelClosed)?;

        rx.recv()
            .map_err(|_| TrajectoryStoreError::ChannelClosed)??;

        Ok(true)
    }
}

// ---------------------------------------------------------------------------
// Internal SQLite Helpers
// ---------------------------------------------------------------------------

fn flush_batch_to_writer(
    conn: &mut Connection,
    records: &[TrajectoryRecord],
    in_mem: &Arc<Mutex<Vec<TrajectoryDegradedInterval>>>,
) {
    if records.is_empty() {
        return;
    }
    if let Err(err) = write_records_tx(conn, records) {
        tracing::error!(error = %err, "trajectory writer batch failed; recording degraded interval");
        if let Some(first) = records.first() {
            let last = records.last().unwrap_or(first);
            let degraded = TrajectoryDegradedInterval {
                chat_id: first.chat_id.clone(),
                run_id: first.run_id.clone(),
                from_seq: first.source_seq,
                to_seq: last.source_seq,
                reason: format!("Durable write failed: {}", err),
                recorded_at: Utc::now(),
            };
            in_mem.lock().unwrap().push(degraded.clone());
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
    }
}

fn handle_writer_command(
    conn: &mut Connection,
    cmd: WriterCommand,
    in_mem: &Arc<Mutex<Vec<TrajectoryDegradedInterval>>>,
) {
    match cmd {
        WriterCommand::WriteRecords(recs) => {
            flush_batch_to_writer(conn, &recs, in_mem);
        }
        WriterCommand::DeleteChat(chat_id, reply) => {
            let res = delete_chat_tx(conn, &chat_id);
            if let Some(tx) = reply {
                tx.send(res);
            }
        }
        WriterCommand::RetainChats(live_ids, reply) => {
            let res = retain_chats_tx(conn, &live_ids);
            if let Some(tx) = reply {
                tx.send(res);
            }
        }
        WriterCommand::Flush(reply) => {
            reply.send(());
        }
        WriterCommand::ImportLegacy {
            chat_id,
            fingerprint,
            records,
            reply,
        } => {
            let res = (|| {
                let count = records.len();
                write_records_tx(conn, &records)?;
                record_legacy_import_tx(conn, &chat_id, &fingerprint, count)?;
                Ok(())
            })();
            if let Some(tx) = reply {
                tx.send(res);
            }
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

fn record_legacy_import_tx(
    conn: &Connection,
    chat_id: &str,
    fingerprint: &str,
    records_count: usize,
) -> Result<(), TrajectoryStoreError> {
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
                    summary: sum.clone(),
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
                error_message: if *is_error { Some(sum) } else { None },
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
    async fn test_trajectory_store_open_and_reopen() {
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
    async fn test_trajectory_store_writer_batching_and_paging() {
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
    async fn test_trajectory_store_diagnostics_and_deletion() {
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
    async fn test_trajectory_store_degraded_interval_reporting() {
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

        store.record_degraded_in_memory(degraded);

        let intervals = store.get_degraded_intervals("c1").unwrap();
        assert_eq!(intervals.len(), 1);
        assert_eq!(intervals[0].from_seq, 10);
        assert_eq!(intervals[0].to_seq, 15);
        assert_eq!(intervals[0].reason, "Simulated store degradation");
    }

    #[tokio::test]
    async fn test_trajectory_store_channel_loss() {
        let temp = TempDir::new().unwrap();
        let store = TrajectoryStore::open(temp.path()).unwrap();

        // Dropping the receiver / closing writer simulates channel loss
        let (closed_tx, _) = sync_channel(1);
        let broken_store = TrajectoryStore {
            db_path: store.db_path.clone(),
            journals_dir: temp.path().join("journals"),
            writer_tx: closed_tx,
            in_memory_degraded: Arc::new(Mutex::new(Vec::new())),
            degraded_reason: Arc::new(Mutex::new(None)),
        };

        let rec = sample_record("chat_fail", "r1", 100, 0);
        let res = broken_store.try_enqueue(rec);
        assert!(matches!(res, Err(TrajectoryStoreError::ChannelClosed)));

        // The degraded interval must be queryable immediately through in-memory state
        let intervals = broken_store.get_degraded_intervals("chat_fail").unwrap();
        assert_eq!(intervals.len(), 1);
        assert_eq!(intervals[0].from_seq, 100);
        assert_eq!(intervals[0].to_seq, 100);
        assert_eq!(intervals[0].reason, "Writer channel closed");
    }
    #[tokio::test]
    async fn test_trajectory_store_durable_write_failure() {
        let temp = TempDir::new().unwrap();
        let store = TrajectoryStore::open(temp.path()).unwrap();

        // Induce a real SQLite durable write failure on the writer connection by installing an abort trigger
        {
            let conn = Connection::open(&store.db_path).unwrap();
            conn.execute_batch(
                "CREATE TRIGGER test_fail_write_records BEFORE INSERT ON trajectory_records
                 BEGIN
                     SELECT RAISE(ABORT, 'forced sqlite durable write failure');
                 END;",
            )
            .unwrap();
        }

        let rec1 = sample_record("chat_durable_fail", "r1", 100, 0);
        let rec2 = sample_record("chat_durable_fail", "r1", 101, 0);
        store.try_enqueue_batch(vec![rec1, rec2]).unwrap();

        // Writer processes the batch through flush_batch_to_writer and write_records_tx,
        // hits the trigger abort error, rolls back records, and records a degraded interval.
        store.flush().await.unwrap();

        // Records were not persisted to SQLite
        let records = store.list_all_records("chat_durable_fail").unwrap();
        assert_eq!(
            records.len(),
            0,
            "failed durable write must not persist uncommitted records"
        );

        // Degraded interval covers the lost sequence range and is queryable
        let intervals = store.get_degraded_intervals("chat_durable_fail").unwrap();
        assert_eq!(intervals.len(), 1);
        assert_eq!(intervals[0].chat_id, "chat_durable_fail");
        assert_eq!(intervals[0].run_id, "r1");
        assert_eq!(intervals[0].from_seq, 100);
        assert_eq!(intervals[0].to_seq, 101);
        assert!(
            intervals[0].reason.contains("Durable write failed:"),
            "expected 'Durable write failed:' prefix in reason: {}",
            intervals[0].reason
        );
        assert!(
            intervals[0]
                .reason
                .contains("forced sqlite durable write failure"),
            "expected underlying SQLite error in reason: {}",
            intervals[0].reason
        );
    }
    #[tokio::test]
    async fn test_trajectory_store_queue_saturation_direct_marker() {
        let temp = TempDir::new().unwrap();
        let (writer_tx, writer_rx) = sync_channel::<WriterCommand>(1);
        let store = TrajectoryStore {
            db_path: temp.path().join("trajectory.sqlite3"),
            journals_dir: temp.path().join("journals"),
            writer_tx,
            in_memory_degraded: Arc::new(Mutex::new(Vec::new())),
            degraded_reason: Arc::new(Mutex::new(None)),
        };

        // Fill the 1-capacity queue
        store
            .writer_tx
            .try_send(WriterCommand::WriteRecords(vec![sample_record(
                "c_sat", "r1", 1, 0,
            )]))
            .unwrap();

        // Second enqueue must saturate and fail with QueueFull
        let rec2 = sample_record("c_sat", "r1", 2, 0);
        let res = store.try_enqueue(rec2);
        assert!(matches!(res, Err(TrajectoryStoreError::QueueFull)));
        // Saturated queue must record marker in memory without needing space in the full queue
        let intervals = store.get_degraded_intervals("c_sat").unwrap();
        assert_eq!(intervals.len(), 1);
        assert_eq!(intervals[0].from_seq, 2);
        assert_eq!(intervals[0].to_seq, 2);
        assert_eq!(intervals[0].reason, "Queue saturated");

        // Clean up writer rx
        let _ = writer_rx.try_recv();
    }

    #[tokio::test]
    async fn test_trajectory_store_tool_error_sanitization_no_raw_secret() {
        let temp = TempDir::new().unwrap();
        let store = TrajectoryStore::open(temp.path()).unwrap();

        let secret = "SECRET_TOKEN_ABCD_99999_NEVER_PERSIST";
        let tool_error_event = zeron_proto::AgentEvent::ToolResult {
            id: "tool_call_123".into(),
            is_error: true,
            output: Some(format!("Error: API failed with auth key {}", secret)),
            diff: None,
            execution: Some(zeron_proto::agent::ToolExecutionMeta {
                exit_code: Some(1),
                duration_ms: Some(150),
            }),
        };

        let record =
            project_event_to_record("chat_sec", "run_1", 10, &tool_error_event, None).unwrap();
        store.try_enqueue(record).unwrap();
        store.flush().await.unwrap();

        // Query records via store API
        let records = store.list_all_records("chat_sec").unwrap();
        assert_eq!(records.len(), 1);
        let rec = &records[0];
        assert_eq!(rec.status, TrajectoryStatus::Error);
        assert_eq!(rec.summary, "Failed (exit code 1)");
        assert_eq!(rec.error_message, Some("Failed (exit code 1)".to_string()));

        // Prove secret is absent from all record fields
        assert!(!rec.summary.contains(secret));
        assert!(!rec.title.contains(secret));
        assert!(!rec.error_message.as_ref().unwrap().contains(secret));
        let res_preview = rec.result.as_ref().unwrap();
        assert!(!res_preview.summary.contains(secret));
        assert!(
            !res_preview
                .sanitized_text
                .as_ref()
                .unwrap()
                .contains(secret)
        );

        // Directly query the raw SQLite database text to ensure NO column contains the secret string
        let conn = store.reader().unwrap();
        let mut stmt = conn
            .prepare("SELECT chat_id, run_id, title, summary, payload, result, error_message FROM trajectory_records WHERE chat_id = ?1")
            .unwrap();
        let rows = stmt
            .query_map(params!["chat_sec"], |row| {
                let r: Vec<Option<String>> = (0..7).map(|i| row.get(i).ok()).collect();
                Ok(r)
            })
            .unwrap();
        for row in rows {
            let cols = row.unwrap();
            for col in cols.into_iter().flatten() {
                assert!(
                    !col.contains(secret),
                    "Found secret '{}' persisted in SQLite column: {}",
                    secret,
                    col
                );
            }
        }
    }

    #[tokio::test]
    async fn test_trajectory_store_open_failure_fail_open_degraded() {
        let temp = TempDir::new().unwrap();
        let store = TrajectoryStore::degraded(temp.path(), "simulated migration or open failure");

        assert!(store.is_degraded());

        // List records returns empty without panicking
        let records = store.list_all_records("chat_deg").unwrap();
        assert_eq!(records.len(), 0);

        // Diagnostics exposes degraded status
        let diag = store.diagnostics("chat_deg").unwrap();
        assert_eq!(diag.degraded_count, 1);
        assert_eq!(diag.record_count, 0);

        // Degraded intervals report the initialization failure
        let intervals = store.get_degraded_intervals("chat_deg").unwrap();
        assert_eq!(intervals.len(), 1);
        assert!(
            intervals[0]
                .reason
                .contains("simulated migration or open failure")
        );

        // Enqueueing during degraded mode returns error and records interval
        let rec = sample_record("chat_deg", "r1", 5, 0);
        let res = store.try_enqueue(rec);
        assert!(res.is_err());
    }

    #[tokio::test]
    async fn test_trajectory_legacy_import_sequence_only_and_idempotent() {
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
    async fn test_trajectory_legacy_import_corrupt_tail() {
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
        // Exactly the 3 valid prefix records plus terminal interrupted record are imported
        assert_eq!(records.len(), 4);
        assert_eq!(records[0].source_seq, 1);
        assert_eq!(records[1].source_seq, 2);
        assert_eq!(records[2].source_seq, 3);
        assert_eq!(records[3].status, TrajectoryStatus::Interrupted);
        assert_eq!(records[3].kind, TrajectoryRecordKind::Done);
    }

    #[tokio::test]
    async fn test_trajectory_legacy_import_interrupted_and_unsettled() {
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
        // SessionStarted, ToolCall (Unsettled), Done (Interrupted)
        assert_eq!(records.len(), 3);
        assert_eq!(records[1].status, TrajectoryStatus::Unsettled);
        assert_eq!(records[2].status, TrajectoryStatus::Interrupted);
        assert_eq!(records[2].kind, TrajectoryRecordKind::Done);

        let groups = zeron_proto::trajectory::group_records(&records);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].status, TrajectoryStatus::Interrupted);
    }

    #[tokio::test]
    async fn test_trajectory_legacy_production_path_lazy_projection() {
        let temp = TempDir::new().unwrap();
        let journals_dir = temp.path().join("journals");
        fs::create_dir_all(&journals_dir).unwrap();

        // Write legacy journal directly to the standard journals directory
        let journal_path = journals_dir.join("chat_lazy.jsonl");
        let lines = vec![
            r#"{"seq":1,"event":{"type":"sessionStarted","harness":"mock","model":"mock-model","cwd":"/work","sessionId":"s1","assistantMessageId":"m1"}}"#,
            r#"{"seq":2,"event":{"type":"userMessage","text":"Hello lazy legacy"}}"#,
            r#"{"seq":3,"event":{"type":"done","status":"completed","result":"ok"}}"#,
        ];
        fs::write(&journal_path, lines.join("\n")).unwrap();

        // Open store without manually calling import_legacy_journal
        let store = TrajectoryStore::open(temp.path()).unwrap();

        // Querying on first access automatically projects the legacy journal
        let records = store.list_all_records("chat_lazy").unwrap();
        assert_eq!(records.len(), 3);
        assert_eq!(records[0].source_seq, 1);
        assert_eq!(records[1].summary, "Hello lazy legacy");

        // Watermark and diagnostics reflect the lazily projected data
        let watermark = store.get_watermark("chat_lazy").unwrap();
        assert_eq!(watermark, Some(3));

        let diag = store.diagnostics("chat_lazy").unwrap();
        assert_eq!(diag.record_count, 3);
        assert_eq!(diag.last_watermark, Some(3));
    }

    #[tokio::test]
    async fn test_trajectory_store_ordered_writer_concurrency() {
        let temp = TempDir::new().unwrap();
        let store = Arc::new(TrajectoryStore::open(temp.path()).unwrap());

        let mut handles = Vec::new();

        for task_idx in 0..8 {
            let store_clone = store.clone();
            handles.push(tokio::spawn(async move {
                let chat_id = format!("chat_concurrent_{}", task_idx);
                for seq in 1..=20 {
                    let rec = sample_record(&chat_id, "run_1", seq, 0);
                    store_clone.try_enqueue(rec).unwrap();
                }
                store_clone.flush().await.unwrap();
                let records = store_clone.list_all_records(&chat_id).unwrap();
                assert_eq!(records.len(), 20);
            }));
        }

        for h in handles {
            h.await.unwrap();
        }
    }

    #[tokio::test]
    async fn test_trajectory_store_lifecycle_retention_and_chat_deletion() {
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
    #[tokio::test]
    async fn test_trajectory_legacy_eligibility_production_layout_no_duplicate_legacy_run() {
        let temp = TempDir::new().unwrap();
        let journal =
            Arc::new(crate::run_journal::RunJournal::open(temp.path().join("journals")).unwrap());
        let store = Arc::new(TrajectoryStore::open(temp.path()).unwrap());
        let engine = crate::sessions::SessionsEngine::new(
            "device".into(),
            journal,
            Arc::new(crate::HarnessRegistry::new()),
        );
        engine.set_trajectory_store(store.clone());

        let chat_id = "chat_native_live";
        // Native capture via live publish
        engine.publish(
            chat_id,
            &AgentEvent::SessionStarted {
                harness: zeron_proto::HarnessId::Mock,
                model: "model".into(),
                cwd: "/work".into(),
                session_id: "s1".into(),
                assistant_message_id: "m1".into(),
                tools: Vec::new(),
            },
        );
        engine.publish(
            chat_id,
            &AgentEvent::UserMessage {
                text: "Hello live".into(),
            },
        );
        engine.publish(
            chat_id,
            &AgentEvent::Done {
                status: DoneStatus::Completed,
                result: Some("done".into()),
                error: None,
                session_id: Some("s1".into()),
            },
        );

        store.flush().await.unwrap();

        // Reading records must NOT import the live journal as a parallel legacy run
        let records = store.list_all_records(chat_id).unwrap();
        assert_eq!(
            records.len(),
            3,
            "must have exactly 3 native records and no legacy duplicate"
        );
        for r in &records {
            assert!(
                !r.run_id.starts_with("legacy_"),
                "run_id must be native, got {}",
                r.run_id
            );
        }
        let groups = zeron_proto::trajectory::group_records(&records);
        assert_eq!(groups.len(), 1, "must have exactly 1 run");
        assert!(!groups[0].is_legacy, "run must not be marked legacy");
    }
    #[tokio::test]
    async fn test_trajectory_legacy_prefix_cutover_preserves_history_and_native_rows() {
        let temp = TempDir::new().unwrap();
        let journals_dir = temp.path().join("journals");
        fs::create_dir_all(&journals_dir).unwrap();
        let journal_path = journals_dir.join("chat_prefix.jsonl");

        // Historical journal prefix seq 1..100
        let mut lines = Vec::new();
        lines.push(r#"{"seq":1,"event":{"type":"sessionStarted","harness":"mock","model":"mock-model","cwd":"/work","sessionId":"s1","assistantMessageId":"m1"}}"#.to_string());
        for i in 2..100 {
            lines.push(format!(
                r#"{{"seq":{},"event":{{"type":"userMessage","text":"Historical message {}"}}}}"#,
                i, i
            ));
        }
        lines.push(
            r#"{"seq":100,"event":{"type":"done","status":"completed","result":"ok"}}"#.to_string(),
        );
        fs::write(&journal_path, lines.join("\n")).unwrap();

        let store = TrajectoryStore::open(temp.path()).unwrap();

        // Enqueue native rows beginning at seq 101
        store
            .try_enqueue(sample_record("chat_prefix", "run_native", 101, 0))
            .unwrap();
        store
            .try_enqueue(sample_record("chat_prefix", "run_native", 102, 0))
            .unwrap();

        // First read returns honest prefix (1..100) + native rows (101, 102) exactly once
        let records = store.list_all_records("chat_prefix").unwrap();
        assert_eq!(
            records.len(),
            102,
            "must have exactly 100 legacy prefix records + 2 native records"
        );

        for (idx, r) in records.iter().enumerate() {
            let expected_seq = (idx + 1) as u64;
            assert_eq!(r.source_seq, expected_seq);
            if expected_seq <= 100 {
                assert!(
                    r.run_id.starts_with("legacy_"),
                    "record at seq {} must be legacy, got {}",
                    expected_seq,
                    r.run_id
                );
            } else {
                assert_eq!(
                    r.run_id, "run_native",
                    "record at seq {} must be native",
                    expected_seq
                );
            }
        }

        // Verify legacy import marker is recorded
        assert!(store.has_legacy_import("chat_prefix").unwrap());
        let conn = store.reader().unwrap();
        let imported_count: i64 = conn
            .query_row(
                "SELECT imported_records FROM trajectory_legacy_imports WHERE chat_id = ?1",
                params!["chat_prefix"],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(imported_count, 100);

        // Subsequent read returns identical records
        let records2 = store.list_all_records("chat_prefix").unwrap();
        assert_eq!(records2, records);
    }

    #[tokio::test]
    async fn test_trajectory_legacy_prefix_unmatched_tool_call_and_native_continuation_interrupted()
    {
        let temp = TempDir::new().unwrap();
        let journals_dir = temp.path().join("journals");
        fs::create_dir_all(&journals_dir).unwrap();
        let journal_path = journals_dir.join("chat_unsettled_native.jsonl");

        // Legacy prefix with seq 1..2: unmatched ToolCall and no Done event
        let lines = vec![
            r#"{"seq":1,"event":{"type":"sessionStarted","harness":"mock","model":"mock-model","cwd":"/work","sessionId":"s1","assistantMessageId":"m1"}}"#,
            r#"{"seq":2,"event":{"type":"toolCall","id":"c_unsettled_pre","call":{"kind":"exec","command":"sleep 100"}}}"#,
        ];
        fs::write(&journal_path, lines.join("\n")).unwrap();

        let store = TrajectoryStore::open(temp.path()).unwrap();

        // Enqueue native rows starting at seq 3 (N = 3)
        let native_user = sample_record("chat_unsettled_native", "run_native_3", 3, 0);
        let mut native_done = sample_record("chat_unsettled_native", "run_native_3", 4, 0);
        native_done.kind = TrajectoryRecordKind::Done;
        native_done.status = TrajectoryStatus::Completed;
        store.try_enqueue(native_user).unwrap();
        store.try_enqueue(native_done).unwrap();

        let records = store.list_all_records("chat_unsettled_native").unwrap();
        // Expected records:
        // 1. legacy SessionStarted (1, 0)
        // 2. legacy ToolCall Unsettled (2, 0)
        // 3. legacy Done Interrupted (2, reserved)
        // 4. native UserMessage (3, 0)
        // 5. native Done (4, 0)
        assert_eq!(
            records.len(),
            5,
            "must have 3 legacy records + 2 native records"
        );
        assert_eq!(records[0].source_seq, 1);
        assert_eq!(records[0].run_id, "legacy_chat_unsettled_native");

        assert_eq!(records[1].source_seq, 2);
        assert_eq!(records[1].sub_seq, 0);
        assert_eq!(records[1].status, TrajectoryStatus::Unsettled);
        assert_eq!(records[1].run_id, "legacy_chat_unsettled_native");

        assert_eq!(records[2].source_seq, 2);
        assert_eq!(records[2].kind, TrajectoryRecordKind::Done);
        assert_eq!(records[2].status, TrajectoryStatus::Interrupted);
        assert_eq!(records[2].run_id, "legacy_chat_unsettled_native");
        assert!(
            records[2].sub_seq > 0,
            "terminal must have nonconflicting reserved sub_seq"
        );

        assert_eq!(records[3].source_seq, 3);
        assert_eq!(records[3].sub_seq, 0);
        assert_eq!(records[3].run_id, "run_native_3");

        assert_eq!(records[4].source_seq, 4);
        assert_eq!(records[4].sub_seq, 0);
        assert_eq!(records[4].run_id, "run_native_3");

        // Pagination check
        let page_prefix = store
            .list_records("chat_unsettled_native", Some(1), Some(3))
            .unwrap();
        assert_eq!(page_prefix.len(), 3);
        assert_eq!(page_prefix[0].id, records[0].id);
        assert_eq!(page_prefix[1].id, records[1].id);
        assert_eq!(page_prefix[2].id, records[2].id);

        let page_native = store
            .list_records("chat_unsettled_native", Some(3), Some(10))
            .unwrap();
        assert_eq!(page_native.len(), 2);
        assert_eq!(page_native[0].id, records[3].id);
        assert_eq!(page_native[1].id, records[4].id);

        // Groups check
        let groups = zeron_proto::trajectory::group_records(&records);
        assert_eq!(groups.len(), 2, "must have legacy run + native run");
        assert!(groups[0].is_legacy);
        assert_eq!(groups[0].status, TrajectoryStatus::Interrupted);
        assert!(!groups[1].is_legacy);
        assert_eq!(groups[1].status, TrajectoryStatus::Completed);
    }

    #[tokio::test]
    async fn test_trajectory_legacy_enqueue_seq_1_and_immediate_read_no_duplicate() {
        let temp = TempDir::new().unwrap();
        let journals_dir = temp.path().join("journals");
        fs::create_dir_all(&journals_dir).unwrap();
        let journal_path = journals_dir.join("chat_race.jsonl");

        let lines = vec![
            r#"{"seq":1,"event":{"type":"sessionStarted","harness":"mock","model":"mock-model","cwd":"/work","sessionId":"s1","assistantMessageId":"m1"}}"#,
            r#"{"seq":2,"event":{"type":"userMessage","text":"Hello live"}}"#,
            r#"{"seq":3,"event":{"type":"done","status":"completed","result":"ok"}}"#,
        ];
        fs::write(&journal_path, lines.join("\n")).unwrap();

        let store = TrajectoryStore::open(temp.path()).unwrap();

        // Enqueue native seq 1
        store
            .try_enqueue(sample_record("chat_race", "run_live", 1, 0))
            .unwrap();

        // Immediately read without async flush
        let records = store.list_all_records("chat_race").unwrap();
        assert_eq!(
            records.len(),
            1,
            "must have exactly 1 native record and zero legacy duplicate records"
        );
        assert_eq!(records[0].run_id, "run_live");
        assert_eq!(records[0].source_seq, 1);

        // Verify legacy import marker is recorded as 0-row completed import
        assert!(store.has_legacy_import("chat_race").unwrap());
        let conn = store.reader().unwrap();
        let imported_count: i64 = conn
            .query_row(
                "SELECT imported_records FROM trajectory_legacy_imports WHERE chat_id = ?1",
                params!["chat_race"],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(imported_count, 0);
    }

    #[tokio::test]
    async fn test_trajectory_legacy_import_one_shot_idempotent_after_journal_growth() {
        let temp = TempDir::new().unwrap();
        let store = TrajectoryStore::open(temp.path()).unwrap();
        let journal_path = temp.path().join("journals").join("chat_grow.jsonl");
        fs::create_dir_all(journal_path.parent().unwrap()).unwrap();

        // Initial legacy journal with 2 events (no Done yet)
        let lines1 = vec![
            r#"{"seq":1,"event":{"type":"sessionStarted","harness":"mock","model":"m","cwd":"/","sessionId":"s","assistantMessageId":"m"}}"#,
            r#"{"seq":2,"event":{"type":"userMessage","text":"First line"}}"#,
        ];
        fs::write(&journal_path, lines1.join("\n")).unwrap();

        // First read triggers one-shot import (and adds Interrupted since no Done)
        let records1 = store.list_all_records("chat_grow").unwrap();
        assert_eq!(records1.len(), 3); // SessionStarted, UserMessage, Done(Interrupted)
        assert_eq!(records1[2].status, TrajectoryStatus::Interrupted);

        // Check legacy import table has exactly 1 entry
        let conn = store.reader().unwrap();
        let import_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM trajectory_legacy_imports WHERE chat_id = ?1",
                params!["chat_grow"],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(import_count, 1);

        // Now simulate journal file growth on disk (new lines appended, file len/mtime changed)
        let lines2 = vec![
            r#"{"seq":1,"event":{"type":"sessionStarted","harness":"mock","model":"m","cwd":"/","sessionId":"s","assistantMessageId":"m"}}"#,
            r#"{"seq":2,"event":{"type":"userMessage","text":"First line"}}"#,
            r#"{"seq":3,"event":{"type":"userMessage","text":"Appended line"}}"#,
            r#"{"seq":4,"event":{"type":"done","status":"completed","result":"ok"}}"#,
        ];
        std::thread::sleep(Duration::from_millis(50));
        fs::write(&journal_path, lines2.join("\n")).unwrap();

        // Repeated reads (list, watermark, diagnostics) must NOT trigger re-parse or duplicate rows
        let records2 = store.list_all_records("chat_grow").unwrap();
        assert_eq!(
            records2.len(),
            3,
            "one-shot legacy import must not re-import after journal growth"
        );
        assert_eq!(records2, records1);

        let wm = store.get_watermark("chat_grow").unwrap();
        assert_eq!(wm, Some(3));

        let diag = store.diagnostics("chat_grow").unwrap();
        assert_eq!(diag.record_count, 3);
    }

    #[tokio::test]
    async fn test_trajectory_store_saturated_queue_deletion_acknowledged() {
        let temp = TempDir::new().unwrap();
        let store = TrajectoryStore::open(temp.path()).unwrap();

        // Seed some records for chat_victim and chat_survivor and commit them
        store
            .try_enqueue(sample_record("chat_victim", "r1", 1, 0))
            .unwrap();
        store
            .try_enqueue(sample_record("chat_survivor", "r1", 1, 0))
            .unwrap();
        store.flush().await.unwrap();

        // Saturate the capture queue until QueueFull is deterministically returned
        let mut observed_queue_full = false;
        for i in 1..=10_000 {
            if let Err(TrajectoryStoreError::QueueFull) =
                store.try_enqueue(sample_record("chat_flood", "r_flood", i, 0))
            {
                observed_queue_full = true;
                break;
            }
        }
        assert!(
            observed_queue_full,
            "precondition: capture queue must be demonstrably saturated with QueueFull immediately before delete submission"
        );

        // Authoritative delete of chat_victim while queue is flooded/saturated
        let del_res = store.delete_chat("chat_victim").await;
        assert!(
            del_res.is_ok(),
            "delete_chat must succeed and acknowledge even under queue saturation"
        );

        store.flush().await.unwrap();

        let victim_records = store.list_all_records("chat_victim").unwrap();
        assert_eq!(
            victim_records.len(),
            0,
            "victim chat records must be completely deleted"
        );

        let survivor_records = store.list_all_records("chat_survivor").unwrap();
        assert_eq!(
            survivor_records.len(),
            1,
            "survivor chat records must remain intact"
        );
    }

    #[tokio::test]
    async fn test_trajectory_legacy_import_coalesce_text_and_reasoning_deltas() {
        let temp = TempDir::new().unwrap();
        let store = TrajectoryStore::open(temp.path()).unwrap();
        let journal_path = temp.path().join("deltas_chat.jsonl");

        let lines = vec![
            r#"{"seq":1,"event":{"type":"sessionStarted","harness":"mock","model":"m","cwd":"/","sessionId":"s","assistantMessageId":"m"}}"#,
            r#"{"seq":2,"event":{"type":"userMessage","text":"Explain rust"}}"#,
            r#"{"seq":3,"event":{"type":"reasoningDelta","text":"Thinking about "}}"#,
            r#"{"seq":4,"event":{"type":"reasoningDelta","text":"ownership and borrowing."}}"#,
            r#"{"seq":5,"event":{"type":"textDelta","text":"Rust provides "}}"#,
            r#"{"seq":6,"event":{"type":"textDelta","text":"memory safety without GC."}}"#,
            r#"{"seq":7,"event":{"type":"done","status":"completed","result":"finished"}}"#,
        ];
        fs::write(&journal_path, lines.join("\n")).unwrap();

        let imported = store
            .import_legacy_journal("chat_deltas", &journal_path)
            .unwrap();
        assert!(imported);

        let records = store.list_all_records("chat_deltas").unwrap();
        // Expect: SessionStarted(1), UserMessage(2), Reasoning(3), AssistantMessage(5), Done(7)
        assert_eq!(records.len(), 5);

        let reasoning_rec = &records[2];
        assert_eq!(reasoning_rec.source_seq, 3);
        assert_eq!(reasoning_rec.kind, TrajectoryRecordKind::Reasoning);
        assert_eq!(reasoning_rec.lane, TrajectoryLane::Model);
        assert_eq!(reasoning_rec.title, "Reasoning");
        assert!(reasoning_rec.summary.contains("ownership and borrowing"));

        let text_rec = &records[3];
        assert_eq!(text_rec.source_seq, 5);
        assert_eq!(text_rec.kind, TrajectoryRecordKind::AssistantMessage);
        assert_eq!(text_rec.lane, TrajectoryLane::Model);
        assert_eq!(text_rec.title, "Assistant");
        assert!(text_rec.summary.contains("memory safety without GC"));

        assert_ne!(
            reasoning_rec.id, text_rec.id,
            "reasoning and assistant records must have distinct identities"
        );
    }
}
