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

use std::collections::{HashMap, HashSet, VecDeque};
use std::fs;
use std::io::BufRead;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use chrono::{TimeZone, Utc};
use rusqlite::{Connection, OpenFlags, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use std::sync::mpsc::{SyncSender, TrySendError, sync_channel};
use tokio::sync::{broadcast, oneshot};
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

/// Store commit and lifecycle events emitted to watchers.
#[derive(Debug, Clone)]
pub enum TrajectoryStoreEvent {
    RecordsCommitted {
        chat_id: String,
        records: Arc<Vec<TrajectoryRecord>>,
        watermark: (u64, u32),
        rev: u64,
    },
    DegradedRecorded {
        chat_id: String,
        interval: TrajectoryDegradedInterval,
    },
    ChatDeleted {
        chat_id: String,
    },
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
    // v2 — monotonic commit revision for lossless snapshot/live resume
    "ALTER TABLE trajectory_records ADD COLUMN rev INTEGER NOT NULL DEFAULT 0;
     CREATE INDEX idx_traj_chat_rev ON trajectory_records(chat_id, rev);",
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
        fingerprint: Option<String>,
        imported_records: usize,
        records: Vec<TrajectoryRecord>,
        reply: Option<ReplySender<Result<(), TrajectoryStoreError>>>,
    },
    Flush(ReplySender<()>),
}

/// Default capacity for the nonblocking capture queue.
const CAPTURE_QUEUE_CAPACITY: usize = 2048;
const MAX_LEGACY_LINE_BYTES: usize = 8 * 1024 * 1024;
const LEGACY_IMPORT_CHUNK_SIZE: usize = 1_000;
const MAX_IN_MEMORY_DEGRADED_INTERVALS: usize = 2_048;

/// Device-local SQLite trajectory store.
#[derive(Clone)]
pub struct TrajectoryStore {
    pub(crate) db_path: PathBuf,
    pub(crate) journals_dir: PathBuf,
    pub(crate) writer_tx: SyncSender<WriterCommand>,
    pub(crate) in_memory_degraded: Arc<Mutex<VecDeque<TrajectoryDegradedInterval>>>,
    pub(crate) degraded_reason: Arc<Mutex<Option<String>>>,
    pub(crate) events_tx: broadcast::Sender<TrajectoryStoreEvent>,
    pub(crate) legacy_importing: Arc<Mutex<HashSet<String>>>,
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
        let (events_tx, _) = broadcast::channel(2048);
        let writer_events_tx = events_tx.clone();
        let writer_db_path = db_path.clone();
        let in_memory_degraded = Arc::new(Mutex::new(VecDeque::new()));
        let writer_in_mem = in_memory_degraded.clone();
        let degraded_reason = Arc::new(Mutex::new(None));
        let writer_degraded_reason = degraded_reason.clone();
        let legacy_importing = Arc::new(Mutex::new(HashSet::new()));
        std::thread::Builder::new()
            .name("trajectory-writer".into())
            .spawn(move || {
                let mut conn = match Connection::open(&writer_db_path) {
                    Ok(c) => c,
                    Err(err) => {
                        tracing::error!(error = %err, "failed to open trajectory writer connection");
                        let reason = format!("failed to open writer connection: {err}");
                        *writer_degraded_reason.lock().unwrap_or_else(|e| e.into_inner()) =
                            Some(reason.clone());
                        drain_queue_as_degraded(
                            &writer_rx,
                            &writer_in_mem,
                            &writer_events_tx,
                            &reason,
                        );
                        return;
                    }
                };
                if let Err(err) = conn.pragma_update(None, "journal_mode", "WAL") {
                    tracing::error!(error = %err, "failed to set WAL journal_mode on writer");
                }
                if let Err(err) = conn.pragma_update(None, "synchronous", "NORMAL") {
                    tracing::error!(error = %err, "failed to set synchronous NORMAL on writer");
                }
                if let Err(err) = conn.busy_timeout(Duration::from_secs(5)) {
                    tracing::error!(error = %err, "failed to set busy_timeout on writer");
                }
                let mut next_rev = match conn.query_row(
                    "SELECT COALESCE(MAX(rev), 0) + 1 FROM trajectory_records",
                    [],
                    |row| row.get::<_, i64>(0),
                ) {
                    Ok(rev) if rev > 0 => rev as u64,
                    Ok(_) => 1,
                    Err(err) => {
                        tracing::error!(error = %err, "failed to seed trajectory writer revision");
                        let reason = format!("failed to seed revision: {err}");
                        *writer_degraded_reason.lock().unwrap_or_else(|e| e.into_inner()) =
                            Some(reason.clone());
                        drain_queue_as_degraded(
                            &writer_rx,
                            &writer_in_mem,
                            &writer_events_tx,
                            &reason,
                        );
                        return;
                    }
                };

                let mut degraded_persist_failed = false;
                while let Ok(cmd) = writer_rx.recv() {
                    if !degraded_persist_failed {
                        if !persist_pending_degraded(&mut conn, &writer_in_mem) {
                            degraded_persist_failed = true;
                        }
                    }
                    match cmd {
                        WriterCommand::WriteRecords(mut records) => {
                            // Drain any immediately available batched records.
                            while let Ok(next) = writer_rx.try_recv() {
                                match next {
                                    WriterCommand::WriteRecords(more) => records.extend(more),
                                    other => {
                                        let ok = flush_batch_to_writer(
                                            &mut conn,
                                            &records,
                                            &writer_in_mem,
                                            &writer_events_tx,
                                            &mut next_rev,
                                        );
                                        if ok {
                                            degraded_persist_failed = false;
                                        }
                                        records.clear();
                                        handle_writer_command(
                                            &mut conn,
                                            other,
                                            &writer_in_mem,
                                            &writer_events_tx,
                                            &mut next_rev,
                                        );
                                        break;
                                    }
                                }
                            }
                            if !records.is_empty() {
                                let ok = flush_batch_to_writer(
                                    &mut conn,
                                    &records,
                                    &writer_in_mem,
                                    &writer_events_tx,
                                    &mut next_rev,
                                );
                                if ok {
                                    degraded_persist_failed = false;
                                }
                            }
                        }
                        other => {
                            handle_writer_command(
                                &mut conn,
                                other,
                                &writer_in_mem,
                                &writer_events_tx,
                                &mut next_rev,
                            );
                        }
                    }
                }
                let _ = persist_pending_degraded(&mut conn, &writer_in_mem);
            })
            .map_err(|e| TrajectoryStoreError::Other(e.to_string()))?;

        Ok(Self {
            db_path,
            journals_dir,
            writer_tx,
            in_memory_degraded,
            degraded_reason,
            events_tx,
            legacy_importing,
        })
    }

    /// Construct a degraded trajectory store that logs operations and reports degradation without panicking.
    pub fn degraded(store_root: impl AsRef<Path>, reason: impl Into<String>) -> Self {
        let db_path = store_root.as_ref().join("trajectory.sqlite3");
        let journals_dir = store_root.as_ref().join("journals");
        let (writer_tx, _) = sync_channel(1);
        let (events_tx, _) = broadcast::channel(2048);
        Self {
            db_path,
            journals_dir,
            writer_tx,
            in_memory_degraded: Arc::new(Mutex::new(VecDeque::new())),
            degraded_reason: Arc::new(Mutex::new(Some(reason.into()))),
            events_tx,
            legacy_importing: Arc::new(Mutex::new(HashSet::new())),
        }
    }
    /// True if this store is running in degraded mode due to initialization failure or writer termination.
    pub fn is_degraded(&self) -> bool {
        self.degraded_reason
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .is_some()
    }

    /// Subscribe to real-time committed records, degraded intervals, and deletions.
    pub fn subscribe_events(&self) -> broadcast::Receiver<TrajectoryStoreEvent> {
        self.events_tx.subscribe()
    }

    /// Access the event broadcast sender.
    pub fn events_tx(&self) -> broadcast::Sender<TrajectoryStoreEvent> {
        self.events_tx.clone()
    }

    /// Determine the minimum committed native (non-legacy) `source_seq` for `chat_id`.
    pub fn min_native_source_seq(
        &self,
        chat_id: &str,
    ) -> Result<Option<u64>, TrajectoryStoreError> {
        let conn = self.reader()?;
        let min_seq: Option<i64> = conn.query_row(
            "SELECT MIN(records.source_seq)
             FROM trajectory_records AS records
             JOIN trajectory_runs AS runs
               ON runs.chat_id = records.chat_id AND runs.run_id = records.run_id
             WHERE records.chat_id = ?1 AND runs.is_legacy = 0",
            params![chat_id],
            |row| row.get(0),
        )?;
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
        {
            let mut importing = self
                .legacy_importing
                .lock()
                .unwrap_or_else(|error| error.into_inner());
            if importing.contains(chat_id) {
                drop(importing);
                let _ = self.sync_flush();
                return Ok(());
            }
            importing.insert(chat_id.to_string());
        }
        struct LegacyImportGuard {
            chat_id: String,
            importing: Arc<Mutex<HashSet<String>>>,
        }
        impl Drop for LegacyImportGuard {
            fn drop(&mut self) {
                let mut importing = self
                    .importing
                    .lock()
                    .unwrap_or_else(|error| error.into_inner());
                importing.remove(&self.chat_id);
            }
        }
        let _guard = LegacyImportGuard {
            chat_id: chat_id.to_string(),
            importing: self.legacy_importing.clone(),
        };

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

    pub fn record_degraded_in_memory(&self, degraded: TrajectoryDegradedInterval) {
        let mut in_mem = self
            .in_memory_degraded
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        for existing in in_mem.iter_mut().rev() {
            if existing.chat_id == degraded.chat_id
                && existing.run_id == degraded.run_id
                && existing.reason == degraded.reason
                && existing.to_seq.saturating_add(1) >= degraded.from_seq
                && degraded.to_seq.saturating_add(1) >= existing.from_seq
            {
                existing.from_seq = existing.from_seq.min(degraded.from_seq);
                existing.to_seq = existing.to_seq.max(degraded.to_seq);
                existing.recorded_at = existing.recorded_at.max(degraded.recorded_at);
                return;
            }
        }
        if in_mem.len() >= MAX_IN_MEMORY_DEGRADED_INTERVALS {
            in_mem.pop_front();
        }
        in_mem.push_back(degraded);
    }

    pub fn record_degraded_interval(
        &self,
        chat_id: &str,
        run_id: &str,
        from_seq: u64,
        to_seq: u64,
        reason: impl Into<String>,
    ) -> Result<(), TrajectoryStoreError> {
        self.record_degraded_in_memory(TrajectoryDegradedInterval {
            chat_id: chat_id.to_string(),
            run_id: run_id.to_string(),
            from_seq,
            to_seq,
            reason: reason.into(),
            recorded_at: Utc::now(),
        });
        Ok(())
    }

    /// Enqueue a captured record nonblockingly.
    ///
    /// If the queue is saturated or the store is degraded, this method records a degraded interval rather than
    /// blocking synchronous publication.
    pub fn try_enqueue(&self, record: TrajectoryRecord) -> Result<(), TrajectoryStoreError> {
        if let Some(reason) = self
            .degraded_reason
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .as_ref()
        {
            let degraded = TrajectoryDegradedInterval {
                chat_id: record.chat_id.clone(),
                run_id: record.run_id,
                from_seq: record.source_seq,
                to_seq: record.source_seq,
                reason: format!("Store degraded: {}", reason),
                recorded_at: Utc::now(),
            };
            self.record_degraded_in_memory(degraded.clone());
            let _ = self.events_tx.send(TrajectoryStoreEvent::DegradedRecorded {
                chat_id: degraded.chat_id.clone(),
                interval: degraded,
            });
            return Err(TrajectoryStoreError::Other(format!(
                "store degraded: {}",
                reason
            )));
        }
        match self
            .writer_tx
            .try_send(WriterCommand::WriteRecords(vec![record]))
        {
            Ok(()) => Ok(()),
            Err(TrySendError::Full(WriterCommand::WriteRecords(mut recs))) => {
                if let Some(record) = recs.pop() {
                    tracing::warn!(
                        chat = %record.chat_id,
                        run = %record.run_id,
                        seq = record.source_seq,
                        "trajectory capture queue saturated; recording degraded interval"
                    );
                    let degraded = TrajectoryDegradedInterval {
                        chat_id: record.chat_id.clone(),
                        run_id: record.run_id,
                        from_seq: record.source_seq,
                        to_seq: record.source_seq,
                        reason: "Queue saturated".into(),
                        recorded_at: Utc::now(),
                    };
                    self.record_degraded_in_memory(degraded.clone());
                    let _ = self.events_tx.send(TrajectoryStoreEvent::DegradedRecorded {
                        chat_id: degraded.chat_id.clone(),
                        interval: degraded,
                    });
                }
                Err(TrajectoryStoreError::QueueFull)
            }
            Err(TrySendError::Full(_)) => Err(TrajectoryStoreError::QueueFull),
            Err(TrySendError::Disconnected(WriterCommand::WriteRecords(mut recs))) => {
                let mut reason_guard = self
                    .degraded_reason
                    .lock()
                    .unwrap_or_else(|e| e.into_inner());
                if reason_guard.is_none() {
                    *reason_guard = Some("Writer channel closed".into());
                }
                drop(reason_guard);
                if let Some(record) = recs.pop() {
                    let degraded = TrajectoryDegradedInterval {
                        chat_id: record.chat_id.clone(),
                        run_id: record.run_id,
                        from_seq: record.source_seq,
                        to_seq: record.source_seq,
                        reason: "Writer channel closed".into(),
                        recorded_at: Utc::now(),
                    };
                    self.record_degraded_in_memory(degraded.clone());
                    let _ = self.events_tx.send(TrajectoryStoreEvent::DegradedRecorded {
                        chat_id: degraded.chat_id.clone(),
                        interval: degraded,
                    });
                }
                Err(TrajectoryStoreError::ChannelClosed)
            }
            Err(TrySendError::Disconnected(_)) => {
                let mut reason_guard = self
                    .degraded_reason
                    .lock()
                    .unwrap_or_else(|e| e.into_inner());
                if reason_guard.is_none() {
                    *reason_guard = Some("Writer channel closed".into());
                }
                Err(TrajectoryStoreError::ChannelClosed)
            }
        }
    }

    /// Enqueue a batch of records nonblockingly.
    /// Record one degraded interval PER (chat, run) present in a rejected
    /// batch. A batch spans several Chats, so accounting only the first
    /// record's Chat silently loses the evidence for every other Chat in it —
    /// exactly the false "complete history" this store must never report.
    fn record_degraded_batch(&self, records: &[TrajectoryRecord], reason: &str) {
        let mut groups: Vec<(String, String, u64, u64)> = Vec::new();
        for record in records {
            match groups
                .iter_mut()
                .find(|(chat, run, _, _)| chat == &record.chat_id && run == &record.run_id)
            {
                Some((_, _, from, to)) => {
                    *from = (*from).min(record.source_seq);
                    *to = (*to).max(record.source_seq);
                }
                None => groups.push((
                    record.chat_id.clone(),
                    record.run_id.clone(),
                    record.source_seq,
                    record.source_seq,
                )),
            }
        }
        for (chat_id, run_id, from_seq, to_seq) in groups {
            let degraded = TrajectoryDegradedInterval {
                chat_id: chat_id.clone(),
                run_id,
                from_seq,
                to_seq,
                reason: reason.to_string(),
                recorded_at: Utc::now(),
            };
            self.record_degraded_in_memory(degraded.clone());
            let _ = self.events_tx.send(TrajectoryStoreEvent::DegradedRecorded {
                chat_id,
                interval: degraded,
            });
        }
    }

    pub fn try_enqueue_batch(
        &self,
        records: Vec<TrajectoryRecord>,
    ) -> Result<(), TrajectoryStoreError> {
        if records.is_empty() {
            return Ok(());
        }
        if let Some(reason) = self
            .degraded_reason
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .as_ref()
        {
            self.record_degraded_batch(&records, &format!("Store degraded: {}", reason));
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
                self.record_degraded_batch(&recs, "Queue saturated during batch");
                Err(TrajectoryStoreError::QueueFull)
            }
            Err(TrySendError::Full(_)) => Err(TrajectoryStoreError::QueueFull),
            Err(TrySendError::Disconnected(WriterCommand::WriteRecords(recs))) => {
                let mut reason_guard = self
                    .degraded_reason
                    .lock()
                    .unwrap_or_else(|e| e.into_inner());
                if reason_guard.is_none() {
                    *reason_guard = Some("Writer channel closed".into());
                }
                drop(reason_guard);
                self.record_degraded_batch(&recs, "Writer channel closed");
                Err(TrajectoryStoreError::ChannelClosed)
            }
            Err(TrySendError::Disconnected(_)) => {
                let mut reason_guard = self
                    .degraded_reason
                    .lock()
                    .unwrap_or_else(|e| e.into_inner());
                if reason_guard.is_none() {
                    *reason_guard = Some("Writer channel closed".into());
                }
                Err(TrajectoryStoreError::ChannelClosed)
            }
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

        let rows = if let Some(lim) = limit {
            let mut stmt = conn.prepare(
                "SELECT chat_id, run_id, source_seq, sub_seq, lane, kind, status, is_partial,
                        title, summary, turn_id, step_id, call_id, parent_tool_use_id,
                        timing, usage, payload, result, error_message, is_degraded
                 FROM trajectory_records
                 WHERE chat_id = ?1 AND source_seq >= ?2
                 ORDER BY source_seq ASC, sub_seq ASC
                 LIMIT ?3",
            )?;
            stmt.query_map(params![chat_id, from as i64, lim as i64], row_to_record)?
                .collect::<Result<Vec<_>, _>>()?
        } else {
            let mut stmt = conn.prepare(
                "SELECT chat_id, run_id, source_seq, sub_seq, lane, kind, status, is_partial,
                        title, summary, turn_id, step_id, call_id, parent_tool_use_id,
                        timing, usage, payload, result, error_message, is_degraded
                 FROM trajectory_records
                 WHERE chat_id = ?1 AND source_seq >= ?2
                 ORDER BY source_seq ASC, sub_seq ASC",
            )?;
            stmt.query_map(params![chat_id, from as i64], row_to_record)?
                .collect::<Result<Vec<_>, _>>()?
        };

        Ok(rows)
    }

    /// Read all records for `chat_id` in chronological order.
    pub fn list_all_records(
        &self,
        chat_id: &str,
    ) -> Result<Vec<TrajectoryRecord>, TrajectoryStoreError> {
        self.list_records(chat_id, None, None)
    }

    /// Read an ordered slice of records for `chat_id` after `after_cursor` in `(source_seq, sub_seq)` order.
    pub fn list_records_after_cursor(
        &self,
        chat_id: &str,
        after_cursor: Option<zeron_rpc::TrajectoryCursor>,
        limit: Option<usize>,
    ) -> Result<Vec<TrajectoryRecord>, TrajectoryStoreError> {
        if self.is_degraded() {
            return Ok(Vec::new());
        }
        let _ = self.ensure_legacy_imported(chat_id);
        let conn = self.reader()?;

        let rows = match (after_cursor, limit) {
            (Some(cursor), Some(lim)) => {
                let mut stmt = conn.prepare(
                    "SELECT chat_id, run_id, source_seq, sub_seq, lane, kind, status, is_partial,
                            title, summary, turn_id, step_id, call_id, parent_tool_use_id,
                            timing, usage, payload, result, error_message, is_degraded
                     FROM trajectory_records
                     WHERE chat_id = ?1
                       AND (source_seq > ?2
                            OR (source_seq = ?2 AND sub_seq > ?3)
                            OR (?4 > 0 AND rev > ?4))
                     ORDER BY source_seq ASC, sub_seq ASC
                     LIMIT ?5",
                )?;
                stmt.query_map(
                    params![
                        chat_id,
                        cursor.source_seq as i64,
                        cursor.sub_seq as i64,
                        cursor.rev as i64,
                        lim as i64
                    ],
                    row_to_record,
                )?
                .collect::<Result<Vec<_>, _>>()?
            }
            (Some(cursor), None) => {
                let mut stmt = conn.prepare(
                    "SELECT chat_id, run_id, source_seq, sub_seq, lane, kind, status, is_partial,
                            title, summary, turn_id, step_id, call_id, parent_tool_use_id,
                            timing, usage, payload, result, error_message, is_degraded
                     FROM trajectory_records
                     WHERE chat_id = ?1
                       AND (source_seq > ?2
                            OR (source_seq = ?2 AND sub_seq > ?3)
                            OR (?4 > 0 AND rev > ?4))
                     ORDER BY source_seq ASC, sub_seq ASC",
                )?;
                stmt.query_map(
                    params![
                        chat_id,
                        cursor.source_seq as i64,
                        cursor.sub_seq as i64,
                        cursor.rev as i64,
                    ],
                    row_to_record,
                )?
                .collect::<Result<Vec<_>, _>>()?
            }
            (None, Some(lim)) => {
                let mut stmt = conn.prepare(
                    "SELECT chat_id, run_id, source_seq, sub_seq, lane, kind, status, is_partial,
                            title, summary, turn_id, step_id, call_id, parent_tool_use_id,
                            timing, usage, payload, result, error_message, is_degraded
                     FROM trajectory_records
                     WHERE chat_id = ?1
                     ORDER BY source_seq ASC, sub_seq ASC
                     LIMIT ?2",
                )?;
                stmt.query_map(params![chat_id, lim as i64], row_to_record)?
                    .collect::<Result<Vec<_>, _>>()?
            }
            (None, None) => {
                let mut stmt = conn.prepare(
                    "SELECT chat_id, run_id, source_seq, sub_seq, lane, kind, status, is_partial,
                            title, summary, turn_id, step_id, call_id, parent_tool_use_id,
                            timing, usage, payload, result, error_message, is_degraded
                     FROM trajectory_records
                     WHERE chat_id = ?1
                     ORDER BY source_seq ASC, sub_seq ASC",
                )?;
                stmt.query_map(params![chat_id], row_to_record)?
                    .collect::<Result<Vec<_>, _>>()?
            }
        };

        Ok(rows)
    }

    /// Stream bounded snapshot pages for `chat_id` under a single SQLite WAL read transaction.
    ///
    /// The read transaction isolates the snapshot point-in-time from any concurrent commits.
    pub fn stream_snapshot_pages<F>(
        &self,
        chat_id: &str,
        after_cursor: Option<zeron_rpc::TrajectoryCursor>,
        page_size: usize,
        mut emit_page: F,
    ) -> Result<(), TrajectoryStoreError>
    where
        F: FnMut(Vec<TrajectoryRecord>, Option<zeron_rpc::TrajectoryCursor>, bool) -> bool,
    {
        if self.is_degraded() {
            let _ = emit_page(Vec::new(), after_cursor, false);
            return Ok(());
        }
        let _ = self.ensure_legacy_imported(chat_id);
        let conn = self.reader()?;
        let tx = conn.unchecked_transaction()?;
        let snapshot_rev = tx.query_row(
            "SELECT COALESCE(MAX(rev), 0) FROM trajectory_records WHERE chat_id = ?1",
            params![chat_id],
            |row| row.get::<_, i64>(0),
        )? as u64;

        let mut stmt = match after_cursor {
            Some(_) => tx.prepare(
                "SELECT chat_id, run_id, source_seq, sub_seq, lane, kind, status, is_partial,
                        title, summary, turn_id, step_id, call_id, parent_tool_use_id,
                        timing, usage, payload, result, error_message, is_degraded
                 FROM trajectory_records
                 WHERE chat_id = ?1
                   AND (source_seq > ?2
                        OR (source_seq = ?2 AND sub_seq > ?3)
                        OR (?4 > 0 AND rev > ?4))
                 ORDER BY source_seq ASC, sub_seq ASC",
            )?,
            None => tx.prepare(
                "SELECT chat_id, run_id, source_seq, sub_seq, lane, kind, status, is_partial,
                        title, summary, turn_id, step_id, call_id, parent_tool_use_id,
                        timing, usage, payload, result, error_message, is_degraded
                 FROM trajectory_records
                 WHERE chat_id = ?1
                 ORDER BY source_seq ASC, sub_seq ASC",
            )?,
        };

        let mut rows = match after_cursor {
            Some(cursor) => stmt.query_map(
                params![
                    chat_id,
                    cursor.source_seq as i64,
                    cursor.sub_seq as i64,
                    cursor.rev as i64
                ],
                row_to_record,
            )?,
            None => stmt.query_map(params![chat_id], row_to_record)?,
        };

        let mut page_buffer = Vec::with_capacity(page_size);
        let mut current_watermark = Some(
            after_cursor
                .unwrap_or_else(|| zeron_rpc::TrajectoryCursor::new(0, 0))
                .with_rev(snapshot_rev),
        );
        let mut next_item: Option<TrajectoryRecord> = None;

        if let Some(row_res) = rows.next() {
            next_item = Some(row_res?);
        }

        if next_item.is_none() {
            let _ = emit_page(Vec::new(), current_watermark, false);
            return Ok(());
        }

        while let Some(item) = next_item.take() {
            page_buffer.push(item);

            if page_buffer.len() == page_size {
                let has_more = if let Some(row_res) = rows.next() {
                    next_item = Some(row_res?);
                    true
                } else {
                    false
                };

                if let Some(last) = page_buffer.last() {
                    current_watermark =
                        Some(zeron_rpc::TrajectoryCursor::from(last).with_rev(snapshot_rev));
                }

                let keep_going = emit_page(
                    std::mem::replace(&mut page_buffer, Vec::with_capacity(page_size)),
                    current_watermark,
                    has_more,
                );
                if !keep_going || !has_more {
                    return Ok(());
                }
            } else if let Some(row_res) = rows.next() {
                next_item = Some(row_res?);
            }
        }

        if !page_buffer.is_empty() {
            if let Some(last) = page_buffer.last() {
                current_watermark =
                    Some(zeron_rpc::TrajectoryCursor::from(last).with_rev(snapshot_rev));
            }
            let _ = emit_page(page_buffer, current_watermark, false);
        }

        Ok(())
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

    /// Fetch the highest cursor `(source_seq, sub_seq)` currently committed for `chat_id`.
    pub fn get_watermark_cursor(
        &self,
        chat_id: &str,
    ) -> Result<Option<zeron_rpc::TrajectoryCursor>, TrajectoryStoreError> {
        if self.is_degraded() {
            return Ok(None);
        }
        let _ = self.ensure_legacy_imported(chat_id);
        let conn = self.reader()?;
        let mut stmt = conn.prepare(
            "SELECT source_seq, sub_seq FROM trajectory_records
             WHERE chat_id = ?1
             ORDER BY source_seq DESC, sub_seq DESC
             LIMIT 1",
        )?;
        let mut rows = stmt.query(params![chat_id])?;
        if let Some(row) = rows.next()? {
            let source_seq: i64 = row.get(0)?;
            let sub_seq: i64 = row.get(1)?;
            Ok(Some(zeron_rpc::TrajectoryCursor::new(
                source_seq as u64,
                sub_seq as u32,
            )))
        } else {
            Ok(None)
        }
    }

    /// Validate that a requested `TrajectoryRawRef` is attached to an existing stored record for this chat.
    ///
    /// This is a side-effect-free exact query against persisted records and does NOT trigger legacy import.
    pub fn validate_raw_ref(
        &self,
        raw_ref: &zeron_proto::trajectory::TrajectoryRawRef,
    ) -> Result<bool, TrajectoryStoreError> {
        if self.is_degraded() {
            return Ok(false);
        }
        let conn = self.reader()?;
        let mut stmt = conn.prepare(
            "SELECT payload, result FROM trajectory_records
             WHERE chat_id = ?1 AND source_seq = ?2",
        )?;
        let rows = stmt.query_map(params![raw_ref.chat_id, raw_ref.source_seq as i64], |row| {
            let p_json: Option<String> = row.get(0)?;
            let r_json: Option<String> = row.get(1)?;
            Ok((p_json, r_json))
        })?;
        for row in rows {
            let (p_json, r_json) = row?;
            match raw_ref.field {
                zeron_proto::trajectory::TrajectoryRawField::Payload => {
                    if let Some(json_str) = p_json {
                        if let Ok(p) = serde_json::from_str::<
                            zeron_proto::trajectory::TrajectoryPayloadPreview,
                        >(&json_str)
                        {
                            if let Some(r_ref) = p.raw_ref {
                                if r_ref.chat_id == raw_ref.chat_id
                                    && r_ref.source_seq == raw_ref.source_seq
                                    && r_ref.parent_tool_use_id == raw_ref.parent_tool_use_id
                                    && r_ref.call_id == raw_ref.call_id
                                    && r_ref.field == raw_ref.field
                                    && r_ref.source_version == raw_ref.source_version
                                {
                                    return Ok(true);
                                }
                            }
                        }
                    }
                }
                zeron_proto::trajectory::TrajectoryRawField::Result => {
                    if let Some(json_str) = r_json {
                        if let Ok(r) = serde_json::from_str::<
                            zeron_proto::trajectory::TrajectoryResultPreview,
                        >(&json_str)
                        {
                            if let Some(r_ref) = r.raw_ref {
                                if r_ref.chat_id == raw_ref.chat_id
                                    && r_ref.source_seq == raw_ref.source_seq
                                    && r_ref.parent_tool_use_id == raw_ref.parent_tool_use_id
                                    && r_ref.call_id == raw_ref.call_id
                                    && r_ref.field == raw_ref.field
                                    && r_ref.source_version == raw_ref.source_version
                                {
                                    return Ok(true);
                                }
                            }
                        }
                    }
                }
            }
        }
        Ok(false)
    }

    /// Fetch degraded intervals for `chat_id` with optional limit.
    pub fn get_degraded_intervals_with_limit(
        &self,
        chat_id: &str,
        limit: Option<usize>,
    ) -> Result<Vec<TrajectoryDegradedInterval>, TrajectoryStoreError> {
        let conn = self.reader().ok();
        self.fetch_degraded_intervals(conn.as_ref(), chat_id, limit)
    }

    /// Fetch all degraded intervals for `chat_id`.
    pub fn get_degraded_intervals(
        &self,
        chat_id: &str,
    ) -> Result<Vec<TrajectoryDegradedInterval>, TrajectoryStoreError> {
        self.get_degraded_intervals_with_limit(chat_id, None)
    }

    fn fetch_degraded_intervals(
        &self,
        conn: Option<&Connection>,
        chat_id: &str,
        limit: Option<usize>,
    ) -> Result<Vec<TrajectoryDegradedInterval>, TrajectoryStoreError> {
        let mut intervals = Vec::new();
        // In-memory degraded intervals
        {
            let in_mem = self
                .in_memory_degraded
                .lock()
                .unwrap_or_else(|error| error.into_inner());
            for inv in in_mem.iter() {
                if inv.chat_id == chat_id {
                    intervals.push(inv.clone());
                }
            }
        }

        // Persisted degraded intervals from SQLite
        if let Some(conn) = conn {
            let mut stmt = match limit {
                Some(_) => conn.prepare(
                    "SELECT chat_id, run_id, from_seq, to_seq, reason, recorded_at
                     FROM trajectory_degraded_intervals
                     WHERE chat_id = ?1
                     ORDER BY from_seq DESC
                     LIMIT ?2",
                )?,
                None => conn.prepare(
                    "SELECT chat_id, run_id, from_seq, to_seq, reason, recorded_at
                     FROM trajectory_degraded_intervals
                     WHERE chat_id = ?1
                     ORDER BY from_seq ASC",
                )?,
            };

            let row_mapper = |row: &rusqlite::Row| -> rusqlite::Result<TrajectoryDegradedInterval> {
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
            };
            let rows = match limit {
                Some(lim) => stmt.query_map(params![chat_id, lim as i64], row_mapper)?,
                None => stmt.query_map(params![chat_id], row_mapper)?,
            };

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

        intervals.sort_by_key(|i| i.from_seq);
        if let Some(lim) = limit {
            if intervals.len() > lim {
                intervals.truncate(lim);
            }
        }
        if intervals.is_empty() {
            if let Some(reason) = self
                .degraded_reason
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .as_ref()
            {
                intervals.push(TrajectoryDegradedInterval {
                    chat_id: chat_id.to_string(),
                    run_id: "init".to_string(),
                    from_seq: 0,
                    to_seq: 0,
                    reason: format!("Store initialization failed: {}", reason),
                    recorded_at: Utc::now(),
                });
            }
        }
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
        let tx = conn.unchecked_transaction()?;
        let record_count: i64 = tx.query_row(
            "SELECT COUNT(*) FROM trajectory_records WHERE chat_id = ?1",
            params![chat_id],
            |r| r.get(0),
        )?;
        let run_count: i64 = tx.query_row(
            "SELECT COUNT(DISTINCT run_id) FROM trajectory_records WHERE chat_id = ?1",
            params![chat_id],
            |r| r.get(0),
        )?;
        let last_watermark: Option<u64> = tx
            .query_row(
                "SELECT last_source_seq FROM trajectory_watermarks WHERE chat_id = ?1",
                params![chat_id],
                |row| row.get::<_, i64>(0),
            )
            .optional()?
            .map(|s| s as u64);

        let degraded_intervals = self.fetch_degraded_intervals(Some(&tx), chat_id, None)?;
        let degraded_count = degraded_intervals.len();

        let db_size_bytes = fs::metadata(&self.db_path).map(|m| m.len()).unwrap_or(0);

        Ok(TrajectoryDiagnostics {
            chat_id: chat_id.to_string(),
            record_count: record_count as usize,
            run_count: run_count as usize,
            last_watermark,
            degraded_count,
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
                fingerprint: Some(fingerprint),
                imported_records: 0,
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
        let mut reader = std::io::BufReader::new(file);
        let mut line_buffer = Vec::new();

        let run_id = format!("legacy_{}", chat_id);
        let mut records: Vec<TrajectoryRecord> = Vec::with_capacity(LEGACY_IMPORT_CHUNK_SIZE);
        let mut committed_records = 0;
        let mut trailing_segment_has_records = false;
        let mut last_trailing_seq = 0;
        let mut pending_tools: HashMap<String, TrajectoryRecord> = HashMap::new();
        let mut tool_names: HashMap<String, String> = HashMap::new();

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

        #[derive(Deserialize)]
        struct LegacyLine {
            seq: u64,
            event: zeron_proto::AgentEvent,
        }

        loop {
            let oversized =
                match read_bounded_line(&mut reader, &mut line_buffer, MAX_LEGACY_LINE_BYTES) {
                    Ok(Some(oversized)) => oversized,
                    Ok(None) | Err(_) => break,
                };
            if oversized {
                continue;
            }
            let line = match std::str::from_utf8(&line_buffer) {
                Ok(line) => line,
                Err(_) => break,
            };
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            let parsed: LegacyLine = match serde_json::from_str(trimmed) {
                Ok(parsed) => parsed,
                Err(_) => break,
            };

            let seq = parsed.seq;
            let event = parsed.event;
            if min_native_seq.is_some_and(|cutover_seq| seq >= cutover_seq) {
                break;
            }

            match &event {
                zeron_proto::AgentEvent::ReasoningDelta { text } => {
                    flush_text(&mut records, &mut current_text);
                    if let Some((_, accumulated)) = &mut current_reasoning {
                        accumulated.push_str(text);
                    } else {
                        current_reasoning = Some((seq, text.clone()));
                    }
                    trailing_segment_has_records = true;
                    last_trailing_seq = seq;
                }
                zeron_proto::AgentEvent::TextDelta { text } => {
                    flush_reasoning(&mut records, &mut current_reasoning);
                    if let Some((_, accumulated)) = &mut current_text {
                        accumulated.push_str(text);
                    } else {
                        current_text = Some((seq, text.clone()));
                    }
                    trailing_segment_has_records = true;
                    last_trailing_seq = seq;
                }
                _ => {
                    flush_reasoning(&mut records, &mut current_reasoning);
                    flush_text(&mut records, &mut current_text);

                    let originating_tool_name = match &event {
                        zeron_proto::AgentEvent::ToolResult { id, .. } => {
                            tool_names.get(id).map(String::as_str)
                        }
                        _ => None,
                    };
                    if let Some(mut record) = project_event_to_record(
                        chat_id,
                        &run_id,
                        seq,
                        &event,
                        None,
                        originating_tool_name,
                    ) {
                        record.timing = Some(TrajectoryTiming::sequence_only());
                        if let zeron_proto::AgentEvent::ToolCall { id, call } = &event {
                            tool_names.insert(id.clone(), tool_name_for(call));
                            pending_tools.insert(id.clone(), record.clone());
                        }
                        records.push(record);
                        trailing_segment_has_records = true;
                        last_trailing_seq = seq;
                    }

                    match &event {
                        zeron_proto::AgentEvent::ToolResult { id, .. } => {
                            pending_tools.remove(id);
                            tool_names.remove(id);
                        }
                        zeron_proto::AgentEvent::Done { .. } => {
                            pending_tools.clear();
                            tool_names.clear();
                            trailing_segment_has_records = false;
                            last_trailing_seq = 0;
                        }
                        _ => {}
                    }
                }
            }

            flush_legacy_chunks(
                &self.writer_tx,
                chat_id,
                &mut records,
                &mut committed_records,
            )?;
        }

        flush_reasoning(&mut records, &mut current_reasoning);
        flush_text(&mut records, &mut current_text);

        let mut replayed_updates = 0;
        if trailing_segment_has_records {
            for pending in pending_tools.values() {
                if let Some(record) = records.iter_mut().find(|record| record.id == pending.id) {
                    record.status = TrajectoryStatus::Unsettled;
                } else {
                    let mut record = pending.clone();
                    record.status = TrajectoryStatus::Unsettled;
                    records.push(record);
                    replayed_updates += 1;
                }
            }

            let (terminal_seq, terminal_sub_seq) = if min_native_seq.is_some() {
                (last_trailing_seq, u32::MAX)
            } else {
                (last_trailing_seq + 1, 0)
            };
            records.push(TrajectoryRecord {
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
            });
        }

        flush_legacy_chunks(
            &self.writer_tx,
            chat_id,
            &mut records,
            &mut committed_records,
        )?;
        let imported_records = committed_records + records.len() - replayed_updates;
        send_legacy_import_command(
            &self.writer_tx,
            chat_id,
            Some(fingerprint),
            imported_records,
            records,
        )?;

        Ok(true)
    }
}
fn read_bounded_line<R: BufRead>(
    reader: &mut R,
    buffer: &mut Vec<u8>,
    byte_cap: usize,
) -> std::io::Result<Option<bool>> {
    buffer.clear();
    let mut saw_bytes = false;
    let mut oversized = false;
    loop {
        let available = reader.fill_buf()?;
        if available.is_empty() {
            return Ok(saw_bytes.then_some(oversized));
        }
        saw_bytes = true;
        if let Some(newline) = available.iter().position(|byte| *byte == b'\n') {
            if !oversized {
                if buffer.len().saturating_add(newline) > byte_cap {
                    oversized = true;
                } else {
                    buffer.extend_from_slice(&available[..newline]);
                }
            }
            reader.consume(newline + 1);
            return Ok(Some(oversized));
        }

        let len = available.len();
        if !oversized {
            if buffer.len().saturating_add(len) > byte_cap {
                oversized = true;
            } else {
                buffer.extend_from_slice(available);
            }
        }
        reader.consume(len);
    }
}

fn send_legacy_import_command(
    writer_tx: &SyncSender<WriterCommand>,
    chat_id: &str,
    fingerprint: Option<String>,
    imported_records: usize,
    records: Vec<TrajectoryRecord>,
) -> Result<(), TrajectoryStoreError> {
    let (reply_tx, reply_rx) = sync_channel(1);
    writer_tx
        .send(WriterCommand::ImportLegacy {
            chat_id: chat_id.to_string(),
            fingerprint,
            imported_records,
            records,
            reply: Some(ReplySender::Sync(reply_tx)),
        })
        .map_err(|_| TrajectoryStoreError::ChannelClosed)?;
    reply_rx
        .recv()
        .map_err(|_| TrajectoryStoreError::ChannelClosed)?
}

fn flush_legacy_chunks(
    writer_tx: &SyncSender<WriterCommand>,
    chat_id: &str,
    records: &mut Vec<TrajectoryRecord>,
    committed_records: &mut usize,
) -> Result<(), TrajectoryStoreError> {
    while records.len() >= LEGACY_IMPORT_CHUNK_SIZE {
        let remainder = records.split_off(LEGACY_IMPORT_CHUNK_SIZE);
        let chunk = std::mem::replace(records, remainder);
        *committed_records += chunk.len();
        send_legacy_import_command(writer_tx, chat_id, None, 0, chunk)?;
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Internal SQLite Helpers
// ---------------------------------------------------------------------------

fn discard_persisted_degraded(
    current: &mut VecDeque<TrajectoryDegradedInterval>,
    persisted: &[TrajectoryDegradedInterval],
) {
    for saved in persisted {
        let Some(index) = current.iter().position(|interval| {
            interval.chat_id == saved.chat_id
                && interval.run_id == saved.run_id
                && interval.reason == saved.reason
                && interval.recorded_at == saved.recorded_at
                && interval.from_seq <= saved.from_seq
                && interval.to_seq >= saved.to_seq
        }) else {
            continue;
        };

        let interval = &mut current[index];
        match (
            interval.from_seq < saved.from_seq,
            interval.to_seq > saved.to_seq,
        ) {
            (false, false) => {
                current.remove(index);
            }
            (false, true) => {
                interval.from_seq = saved.to_seq + 1;
            }
            (true, false) => {
                interval.to_seq = saved.from_seq - 1;
            }
            (true, true) => {
                let mut right = interval.clone();
                interval.to_seq = saved.from_seq - 1;
                right.from_seq = saved.to_seq + 1;
                if current.len() < MAX_IN_MEMORY_DEGRADED_INTERVALS {
                    current.insert(index + 1, right);
                }
            }
        }
    }
}

fn persist_pending_degraded(
    conn: &mut Connection,
    in_mem: &Arc<Mutex<VecDeque<TrajectoryDegradedInterval>>>,
) -> bool {
    let pending: Vec<TrajectoryDegradedInterval> = {
        let in_mem_lock = in_mem.lock().unwrap_or_else(|error| error.into_inner());
        in_mem_lock.iter().cloned().collect()
    };
    if pending.is_empty() {
        return true;
    }

    let result = (|| -> Result<(), rusqlite::Error> {
        let tx = conn.transaction()?;
        {
            let mut find_stmt = tx.prepare_cached(
                "SELECT id, from_seq, to_seq, recorded_at FROM trajectory_degraded_intervals
                 WHERE chat_id = ?1 AND run_id = ?2 AND reason = ?3
                   AND to_seq >= ?4 - 1 AND from_seq <= ?5 + 1
                 ORDER BY from_seq ASC",
            )?;
            let mut update_stmt = tx.prepare_cached(
                "UPDATE trajectory_degraded_intervals
                 SET from_seq = ?1, to_seq = ?2, recorded_at = ?3
                 WHERE id = ?4",
            )?;
            let mut delete_stmt =
                tx.prepare_cached("DELETE FROM trajectory_degraded_intervals WHERE id = ?1")?;
            let mut insert_stmt = tx.prepare_cached(
                "INSERT INTO trajectory_degraded_intervals
                    (chat_id, run_id, from_seq, to_seq, reason, recorded_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            )?;

            for interval in &pending {
                let from_val = interval.from_seq as i64;
                let to_val = interval.to_seq as i64;
                let rec_ms = interval.recorded_at.timestamp_millis();

                let overlapping: Vec<(i64, i64, i64, i64)> = find_stmt
                    .query_map(
                        params![
                            interval.chat_id,
                            interval.run_id,
                            interval.reason,
                            from_val,
                            to_val
                        ],
                        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
                    )?
                    .collect::<Result<Vec<_>, _>>()?;

                if overlapping.is_empty() {
                    insert_stmt.execute(params![
                        interval.chat_id,
                        interval.run_id,
                        from_val,
                        to_val,
                        interval.reason,
                        rec_ms,
                    ])?;
                } else {
                    let mut merged_from = from_val;
                    let mut merged_to = to_val;
                    let mut merged_rec = rec_ms;
                    let primary_id = overlapping[0].0;

                    for (_, f, t, r) in &overlapping {
                        merged_from = merged_from.min(*f);
                        merged_to = merged_to.max(*t);
                        merged_rec = merged_rec.max(*r);
                    }

                    update_stmt.execute(params![merged_from, merged_to, merged_rec, primary_id])?;

                    for (id, _, _, _) in &overlapping[1..] {
                        delete_stmt.execute(params![id])?;
                    }
                }
            }
        }
        tx.commit()
    })();

    match result {
        Ok(()) => {
            let mut current = in_mem.lock().unwrap_or_else(|error| error.into_inner());
            discard_persisted_degraded(&mut current, &pending);
            true
        }
        Err(error) => {
            tracing::error!(%error, "failed to persist trajectory degraded intervals");
            false
        }
    }
}

/// A writer that dies during startup leaves whatever capture already enqueued
/// sitting in the channel. Dropping the receiver silently would report those
/// records as never captured at all, so the dying thread accounts them as a
/// degraded interval per (Chat, run) before it goes away. Later enqueues get
/// `Disconnected` and are accounted by the caller.
fn drain_queue_as_degraded(
    writer_rx: &std::sync::mpsc::Receiver<WriterCommand>,
    in_mem: &Arc<Mutex<VecDeque<TrajectoryDegradedInterval>>>,
    events_tx: &broadcast::Sender<TrajectoryStoreEvent>,
    reason: &str,
) {
    while let Ok(cmd) = writer_rx.try_recv() {
        let records = match cmd {
            WriterCommand::WriteRecords(records) => records,
            _ => continue,
        };
        let mut by_chat: std::collections::HashMap<(String, String), (u64, u64)> =
            std::collections::HashMap::new();
        for record in &records {
            let entry = by_chat
                .entry((record.chat_id.clone(), record.run_id.clone()))
                .or_insert((record.source_seq, record.source_seq));
            entry.0 = entry.0.min(record.source_seq);
            entry.1 = entry.1.max(record.source_seq);
        }
        for ((chat_id, run_id), (from_seq, to_seq)) in by_chat {
            let degraded = TrajectoryDegradedInterval {
                chat_id: chat_id.clone(),
                run_id,
                from_seq,
                to_seq,
                reason: reason.to_string(),
                recorded_at: Utc::now(),
            };
            {
                let mut pending = in_mem.lock().unwrap_or_else(|error| error.into_inner());
                if pending.len() >= MAX_IN_MEMORY_DEGRADED_INTERVALS {
                    pending.pop_front();
                }
                pending.push_back(degraded.clone());
            }
            let _ = events_tx.send(TrajectoryStoreEvent::DegradedRecorded {
                chat_id,
                interval: degraded,
            });
        }
    }
}

fn flush_batch_to_writer(
    conn: &mut Connection,
    records: &[TrajectoryRecord],
    in_mem: &Arc<Mutex<VecDeque<TrajectoryDegradedInterval>>>,
    events_tx: &broadcast::Sender<TrajectoryStoreEvent>,
    next_rev: &mut u64,
) -> bool {
    if records.is_empty() {
        return true;
    }
    let rev = *next_rev;
    if let Err(err) = write_records_tx(conn, records, rev) {
        tracing::error!(error = %err, "trajectory writer batch failed; recording degraded interval");
        let mut by_chat: HashMap<String, Vec<&TrajectoryRecord>> = HashMap::new();
        for record in records {
            by_chat
                .entry(record.chat_id.clone())
                .or_default()
                .push(record);
        }
        for (chat_id, chat_records) in by_chat {
            if let Some(first) = chat_records.first() {
                let last = chat_records.last().unwrap_or(first);
                let degraded = TrajectoryDegradedInterval {
                    chat_id: chat_id.clone(),
                    run_id: first.run_id.clone(),
                    from_seq: first.source_seq,
                    to_seq: last.source_seq,
                    reason: format!("Durable write failed: {}", err),
                    recorded_at: Utc::now(),
                };
                let mut pending = in_mem.lock().unwrap_or_else(|error| error.into_inner());
                if pending.len() >= MAX_IN_MEMORY_DEGRADED_INTERVALS {
                    pending.pop_front();
                }
                pending.push_back(degraded.clone());
                drop(pending);
                let _ = events_tx.send(TrajectoryStoreEvent::DegradedRecorded {
                    chat_id: degraded.chat_id.clone(),
                    interval: degraded,
                });
            }
        }
        return false;
    }

    *next_rev = next_rev.saturating_add(1);
    if events_tx.receiver_count() > 0 {
        let mut by_chat: HashMap<String, Vec<TrajectoryRecord>> = HashMap::new();
        for record in records {
            by_chat
                .entry(record.chat_id.clone())
                .or_default()
                .push(record.clone());
        }
        for (chat_id, chat_records) in by_chat {
            let max_cursor = chat_records
                .iter()
                .map(|record| (record.source_seq, record.sub_seq))
                .max()
                .unwrap_or((0, 0));
            let _ = events_tx.send(TrajectoryStoreEvent::RecordsCommitted {
                chat_id,
                records: Arc::new(chat_records),
                watermark: max_cursor,
                rev,
            });
        }
    }
    true
}

fn handle_writer_command(
    conn: &mut Connection,
    cmd: WriterCommand,
    in_mem: &Arc<Mutex<VecDeque<TrajectoryDegradedInterval>>>,
    events_tx: &broadcast::Sender<TrajectoryStoreEvent>,
    next_rev: &mut u64,
) {
    match cmd {
        WriterCommand::WriteRecords(records) => {
            flush_batch_to_writer(conn, &records, in_mem, events_tx, next_rev);
        }
        WriterCommand::DeleteChat(chat_id, reply) => {
            let result = delete_chat_tx(conn, &chat_id);
            if result.is_ok() {
                let _ = events_tx.send(TrajectoryStoreEvent::ChatDeleted {
                    chat_id: chat_id.clone(),
                });
            }
            if let Some(reply) = reply {
                reply.send(result);
            }
        }
        WriterCommand::RetainChats(live_ids, reply) => {
            let result = retain_chats_tx(conn, &live_ids, events_tx);
            if let Some(reply) = reply {
                reply.send(result);
            }
        }
        WriterCommand::Flush(reply) => {
            let _ = persist_pending_degraded(conn, in_mem);
            reply.send(());
        }
        WriterCommand::ImportLegacy {
            chat_id,
            fingerprint,
            imported_records,
            records,
            reply,
        } => {
            let result = (|| {
                let rev = *next_rev;
                if !records.is_empty() {
                    write_records_tx(conn, &records, rev)?;
                    *next_rev = next_rev.saturating_add(1);
                    if events_tx.receiver_count() > 0 {
                        let max_cursor = records
                            .iter()
                            .map(|record| (record.source_seq, record.sub_seq))
                            .max()
                            .unwrap_or((0, 0));
                        let _ = events_tx.send(TrajectoryStoreEvent::RecordsCommitted {
                            chat_id: chat_id.clone(),
                            records: Arc::new(records),
                            watermark: max_cursor,
                            rev,
                        });
                    }
                }
                if let Some(fingerprint) = fingerprint {
                    record_legacy_import_tx(conn, &chat_id, &fingerprint, imported_records)?;
                }
                Ok(())
            })();
            if let Some(reply) = reply {
                reply.send(result);
            }
        }
    }
}

fn retain_chats_tx(
    conn: &mut Connection,
    live_ids: &[String],
    events_tx: &broadcast::Sender<TrajectoryStoreEvent>,
) -> Result<usize, TrajectoryStoreError> {
    let live_ids: HashSet<&str> = live_ids.iter().map(String::as_str).collect();
    let tx = conn.transaction()?;
    let all_chats = {
        let mut stmt = tx.prepare(
            "SELECT chat_id FROM trajectory_records
             UNION SELECT chat_id FROM trajectory_watermarks
             UNION SELECT chat_id FROM trajectory_legacy_imports
             UNION SELECT chat_id FROM trajectory_runs
             UNION SELECT chat_id FROM trajectory_degraded_intervals",
        )?;
        stmt.query_map([], |row| row.get::<_, String>(0))?
            .collect::<Result<Vec<_>, _>>()?
    };

    let mut deleted = Vec::new();
    for chat_id in all_chats {
        if !live_ids.contains(chat_id.as_str()) {
            delete_chat_in_tx(&tx, &chat_id)?;
            deleted.push(chat_id);
        }
    }
    tx.commit()?;

    let total_deleted = deleted.len();
    for chat_id in deleted {
        let _ = events_tx.send(TrajectoryStoreEvent::ChatDeleted { chat_id });
    }
    Ok(total_deleted)
}

fn write_records_tx(
    conn: &mut Connection,
    records: &[TrajectoryRecord],
    rev: u64,
) -> Result<(), TrajectoryStoreError> {
    if records.is_empty() {
        return Ok(());
    }
    let rev = i64::try_from(rev)
        .map_err(|_| TrajectoryStoreError::Other("trajectory revision overflow".into()))?;
    let tx = conn.transaction()?;
    let now = Utc::now().timestamp_millis();
    let mut groups: HashMap<(&str, &str), (String, Option<String>, u64)> = HashMap::new();

    {
        let mut record_stmt = tx.prepare_cached(
            "INSERT INTO trajectory_records (
                chat_id, run_id, source_seq, sub_seq, lane, kind, status, is_partial,
                title, summary, turn_id, step_id, call_id, parent_tool_use_id,
                timing, usage, payload, result, error_message, is_degraded, created_at, rev
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22)
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
                is_degraded = excluded.is_degraded,
                rev = excluded.rev",
        )?;
        for record in records {
            let kind_json = serde_json::to_string(&record.kind)?;
            let status = serde_json::to_string(&record.status)?;
            let timing = record
                .timing
                .as_ref()
                .map(serde_json::to_string)
                .transpose()?;
            let usage = record
                .usage
                .as_ref()
                .map(serde_json::to_string)
                .transpose()?;
            let payload = record
                .payload
                .as_ref()
                .map(serde_json::to_string)
                .transpose()?;
            let result = record
                .result
                .as_ref()
                .map(serde_json::to_string)
                .transpose()?;

            record_stmt.execute(params![
                record.chat_id,
                record.run_id,
                record.source_seq as i64,
                record.sub_seq as i64,
                record.lane.as_str(),
                kind_json,
                status,
                if record.is_partial { 1 } else { 0 },
                record.title,
                record.summary,
                record.turn_id,
                record.step_id,
                record.call_id,
                record.parent_tool_use_id,
                timing,
                usage,
                payload,
                result,
                record.error_message,
                if record.is_degraded { 1 } else { 0 },
                now,
                rev,
            ])?;
            groups
                .entry((&record.chat_id, &record.run_id))
                .and_modify(|group| {
                    group.0.clone_from(&status);
                    group.1.clone_from(&timing);
                    group.2 = group.2.max(record.source_seq);
                })
                .or_insert((status, timing, record.source_seq));
        }
    }

    {
        let mut run_stmt = tx.prepare_cached(
            "INSERT INTO trajectory_runs
                (chat_id, run_id, label, is_legacy, status, timing, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
             ON CONFLICT(chat_id, run_id) DO UPDATE SET
                status = excluded.status,
                timing = excluded.timing,
                updated_at = excluded.updated_at",
        )?;
        let mut watermark_stmt = tx.prepare_cached(
            "INSERT INTO trajectory_watermarks (chat_id, last_source_seq, updated_at)
             VALUES (?1, ?2, ?3)
             ON CONFLICT(chat_id) DO UPDATE SET
                last_source_seq = MAX(trajectory_watermarks.last_source_seq, excluded.last_source_seq),
                updated_at = excluded.updated_at",
        )?;
        for ((chat_id, run_id), (status, timing, max_source_seq)) in groups {
            let is_legacy = run_id.starts_with("legacy");
            run_stmt.execute(params![
                chat_id,
                run_id,
                if is_legacy { "Legacy Run" } else { "Run" },
                if is_legacy { 1 } else { 0 },
                status,
                timing,
                now,
                now,
            ])?;
            watermark_stmt.execute(params![chat_id, max_source_seq as i64, now])?;
        }
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

fn delete_chat_in_tx(conn: &Connection, chat_id: &str) -> Result<(), TrajectoryStoreError> {
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

fn delete_chat_tx(conn: &mut Connection, chat_id: &str) -> Result<(), TrajectoryStoreError> {
    let tx = conn.transaction()?;
    delete_chat_in_tx(&tx, chat_id)?;
    tx.commit()?;
    Ok(())
}

/// Project a normalized AgentEvent into a TrajectoryRecord.
pub fn project_event_to_record(
    chat_id: &str,
    run_id: &str,
    seq: u64,
    event: &AgentEvent,
    parent_tool_use_id: Option<String>,
    originating_tool_name: Option<&str>,
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
            let tool_name = originating_tool_name.unwrap_or("tool").to_string();
            let kind = if diff.is_some() {
                TrajectoryRecordKind::ToolDiff { tool_name }
            } else {
                TrajectoryRecordKind::ToolResult { tool_name }
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
            originating_tool_name,
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
        "model" => TrajectoryLane::Model,
        "tools" => TrajectoryLane::Tools,
        _ => TrajectoryLane::Unknown,
    };

    let kind: TrajectoryRecordKind = serde_json::from_str(&kind_json).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(5, rusqlite::types::Type::Text, Box::new(e))
    })?;

    let status: TrajectoryStatus =
        serde_json::from_str(&status_str).unwrap_or_else(|_| match status_str.as_str() {
            "Running" | "\"running\"" => TrajectoryStatus::Running,
            "Completed" | "\"completed\"" => TrajectoryStatus::Completed,
            "Error" | "\"error\"" => TrajectoryStatus::Error,
            "Interrupted" | "\"interrupted\"" => TrajectoryStatus::Interrupted,
            "Unsettled" | "\"unsettled\"" => TrajectoryStatus::Unsettled,
            "Degraded" | "\"degraded\"" => TrajectoryStatus::Degraded,
            _ => TrajectoryStatus::Unknown,
        });

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
    if current > MIGRATIONS.len() as i64 {
        return Err(TrajectoryStoreError::Other(format!(
            "trajectory database has newer schema version {current}; supported maximum is {}",
            MIGRATIONS.len()
        )));
    }
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
        let (events_tx, _) = broadcast::channel(1);
        let broken_store = TrajectoryStore {
            db_path: store.db_path.clone(),
            journals_dir: temp.path().join("journals"),
            writer_tx: closed_tx,
            in_memory_degraded: Arc::new(Mutex::new(VecDeque::new())),
            degraded_reason: Arc::new(Mutex::new(None)),
            events_tx,
            legacy_importing: Arc::new(Mutex::new(HashSet::new())),
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
        let (events_tx, _) = broadcast::channel(1);
        let store = TrajectoryStore {
            db_path: temp.path().join("trajectory.sqlite3"),
            journals_dir: temp.path().join("journals"),
            writer_tx,
            in_memory_degraded: Arc::new(Mutex::new(VecDeque::new())),
            degraded_reason: Arc::new(Mutex::new(None)),
            events_tx,
            legacy_importing: Arc::new(Mutex::new(HashSet::new())),
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
            project_event_to_record("chat_sec", "run_1", 10, &tool_error_event, None, None)
                .unwrap();
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
    #[tokio::test]
    async fn test_trajectory_store_resume_observes_replacement_revision_after_reopen() {
        let temp = TempDir::new().unwrap();
        let store = TrajectoryStore::open(temp.path()).unwrap();
        let mut partial = sample_record("chat_rev", "run_rev", 7, 0);
        partial.is_partial = true;
        partial.summary = "partial".into();
        store
            .try_enqueue_batch(vec![partial, sample_record("chat_rev", "run_rev", 8, 0)])
            .unwrap();
        store.flush().await.unwrap();
        let distinct_revs: i64 = store
            .reader()
            .unwrap()
            .query_row(
                "SELECT COUNT(DISTINCT rev) FROM trajectory_records WHERE chat_id = 'chat_rev'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(distinct_revs, 1, "one committed batch must have one rev");

        let mut snapshot_cursor = None;
        store
            .stream_snapshot_pages("chat_rev", None, 100, |records, cursor, _| {
                assert_eq!(records.len(), 2);
                snapshot_cursor = cursor;
                true
            })
            .unwrap();
        let snapshot_cursor = snapshot_cursor.unwrap();
        assert!(snapshot_cursor.rev > 0);

        drop(store);
        let reopened = TrajectoryStore::open(temp.path()).unwrap();
        let mut final_record = sample_record("chat_rev", "run_rev", 7, 0);
        final_record.summary = "final".into();
        reopened.try_enqueue(final_record).unwrap();
        reopened.flush().await.unwrap();

        let resumed = reopened
            .list_records_after_cursor("chat_rev", Some(snapshot_cursor), None)
            .unwrap();
        assert_eq!(resumed.len(), 1);
        assert_eq!(resumed[0].summary, "final");

        let replacement_rev: i64 = reopened
            .reader()
            .unwrap()
            .query_row(
                "SELECT rev FROM trajectory_records WHERE chat_id = 'chat_rev' AND source_seq = 7",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert!(replacement_rev as u64 > snapshot_cursor.rev);
    }

    #[test]
    fn test_trajectory_store_empty_snapshot_cursor_carries_revision() {
        let temp = TempDir::new().unwrap();
        let store = TrajectoryStore::open(temp.path()).unwrap();
        let mut emitted = None;
        store
            .stream_snapshot_pages("empty", None, 10, |records, cursor, has_more| {
                assert!(records.is_empty());
                assert!(!has_more);
                emitted = cursor;
                true
            })
            .unwrap();
        assert_eq!(
            emitted,
            Some(zeron_rpc::TrajectoryCursor::new(0, 0).with_rev(0))
        );
    }

    #[tokio::test]
    async fn test_trajectory_store_retain_removes_metadata_only_chats() {
        let temp = TempDir::new().unwrap();
        let store = TrajectoryStore::open(temp.path()).unwrap();
        store
            .try_enqueue(sample_record("records_only", "r1", 1, 0))
            .unwrap();
        store.flush().await.unwrap();

        let conn = Connection::open(&store.db_path).unwrap();
        conn.execute_batch(
            "INSERT INTO trajectory_watermarks VALUES ('watermark_only', 1, 1);
             INSERT INTO trajectory_runs VALUES ('run_only', 'r1', 'Run', 0, 'Completed', NULL, 1, 1);
             INSERT INTO trajectory_degraded_intervals
                (chat_id, run_id, from_seq, to_seq, reason, recorded_at)
                VALUES ('degraded_only', 'r1', 1, 1, 'gap', 1);
             INSERT INTO trajectory_legacy_imports
                (chat_id, source_fingerprint, imported_records, imported_at)
                VALUES ('legacy_only', 'fp', 0, 1);",
        )
        .unwrap();
        drop(conn);

        let deleted = store.retain_chats_only(&[]).await.unwrap();
        assert_eq!(deleted, 5);
        store.flush().await.unwrap();

        let conn = store.reader().unwrap();
        for table in [
            "trajectory_records",
            "trajectory_watermarks",
            "trajectory_runs",
            "trajectory_degraded_intervals",
            "trajectory_legacy_imports",
        ] {
            let count: i64 = conn
                .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
                    row.get(0)
                })
                .unwrap();
            assert_eq!(count, 0, "{table} retained an orphan");
        }
    }

    #[tokio::test]
    async fn test_trajectory_legacy_import_skips_oversized_line_and_continues() {
        use std::io::Write;

        let temp = TempDir::new().unwrap();
        let store = TrajectoryStore::open(temp.path()).unwrap();
        let journal_path = temp.path().join("oversized.jsonl");
        let mut file = fs::File::create(&journal_path).unwrap();
        writeln!(
            file,
            r#"{{"seq":1,"event":{{"type":"sessionStarted","harness":"mock","model":"m","cwd":"/","sessionId":"s","assistantMessageId":"m"}}}}"#
        )
        .unwrap();
        file.write_all(&vec![b'x'; 8 * 1024 * 1024 + 1]).unwrap();
        file.write_all(b"\n").unwrap();
        writeln!(
            file,
            r#"{{"seq":2,"event":{{"type":"userMessage","text":"after oversized"}}}}"#
        )
        .unwrap();
        writeln!(
            file,
            r#"{{"seq":3,"event":{{"type":"done","status":"completed","result":"ok"}}}}"#
        )
        .unwrap();
        drop(file);

        assert!(
            store
                .import_legacy_journal("chat_oversized", &journal_path)
                .unwrap()
        );
        let records = store.list_all_records("chat_oversized").unwrap();
        assert_eq!(records.len(), 3);
        assert_eq!(records[1].summary, "after oversized");
    }

    #[tokio::test]
    async fn test_trajectory_legacy_import_commits_in_bounded_chunks() {
        let temp = TempDir::new().unwrap();
        let store = TrajectoryStore::open(temp.path()).unwrap();
        let mut events = store.subscribe_events();
        let journal_path = temp.path().join("chunked.jsonl");
        let mut lines = Vec::with_capacity(2_002);
        lines.push(
            r#"{"seq":1,"event":{"type":"sessionStarted","harness":"mock","model":"m","cwd":"/","sessionId":"s","assistantMessageId":"m"}}"#
                .to_string(),
        );
        for seq in 2..=2_001 {
            lines.push(format!(
                r#"{{"seq":{seq},"event":{{"type":"userMessage","text":"message {seq}"}}}}"#
            ));
        }
        lines.push(
            r#"{"seq":2002,"event":{"type":"done","status":"completed","result":"ok"}}"#
                .to_string(),
        );
        fs::write(&journal_path, lines.join("\n")).unwrap();

        assert!(
            store
                .import_legacy_journal("chat_chunked", &journal_path)
                .unwrap()
        );
        let mut committed_sizes = Vec::new();
        while let Ok(event) = events.try_recv() {
            if let TrajectoryStoreEvent::RecordsCommitted { records, .. } = event {
                committed_sizes.push(records.len());
            }
        }
        assert!(committed_sizes.len() >= 3, "{committed_sizes:?}");
        assert!(committed_sizes.iter().all(|size| *size <= 1_000));
        assert_eq!(committed_sizes.iter().sum::<usize>(), 2_002);
    }

    #[tokio::test]
    async fn test_trajectory_legacy_completed_run_followed_by_crash_is_interrupted() {
        let temp = TempDir::new().unwrap();
        let store = TrajectoryStore::open(temp.path()).unwrap();
        let journal_path = temp.path().join("completed_then_crashed.jsonl");
        let lines = [
            r#"{"seq":1,"event":{"type":"sessionStarted","harness":"mock","model":"m","cwd":"/","sessionId":"s1","assistantMessageId":"m1"}}"#,
            r#"{"seq":2,"event":{"type":"done","status":"completed","result":"ok"}}"#,
            r#"{"seq":3,"event":{"type":"sessionStarted","harness":"mock","model":"m","cwd":"/","sessionId":"s2","assistantMessageId":"m2"}}"#,
            r#"{"seq":4,"event":{"type":"toolCall","id":"second_run_call","call":{"kind":"exec","command":"sleep 100"}}}"#,
        ];
        fs::write(&journal_path, lines.join("\n")).unwrap();

        assert!(
            store
                .import_legacy_journal("chat_two_runs", &journal_path)
                .unwrap()
        );
        let records = store.list_all_records("chat_two_runs").unwrap();
        assert_eq!(
            records
                .iter()
                .find(|record| record.call_id.as_deref() == Some("second_run_call"))
                .unwrap()
                .status,
            TrajectoryStatus::Unsettled
        );
        assert_eq!(records.last().unwrap().kind, TrajectoryRecordKind::Done);
        assert_eq!(
            records.last().unwrap().status,
            TrajectoryStatus::Interrupted
        );
    }

    #[test]
    fn test_trajectory_store_degraded_intervals_merge_and_are_capped() {
        let temp = TempDir::new().unwrap();
        let store = TrajectoryStore::open(temp.path()).unwrap();
        for seq in 1..=3 {
            store.record_degraded_in_memory(TrajectoryDegradedInterval {
                chat_id: "merge".into(),
                run_id: "run".into(),
                from_seq: seq,
                to_seq: seq,
                reason: "Queue saturated".into(),
                recorded_at: Utc::now(),
            });
        }
        let merged = store
            .in_memory_degraded
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        assert_eq!(merged.len(), 1);
        assert_eq!((merged[0].from_seq, merged[0].to_seq), (1, 3));
        drop(merged);

        for seq in 0..=2_048 {
            store.record_degraded_in_memory(TrajectoryDegradedInterval {
                chat_id: format!("chat_{seq}"),
                run_id: "run".into(),
                from_seq: seq,
                to_seq: seq,
                reason: "gap".into(),
                recorded_at: Utc::now(),
            });
        }
        assert!(
            store
                .in_memory_degraded
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .len()
                <= 2_048
        );
    }

    #[test]
    fn test_trajectory_store_pending_degraded_survives_reopen() {
        let temp = TempDir::new().unwrap();
        let store = TrajectoryStore::open(temp.path()).unwrap();
        store.record_degraded_in_memory(TrajectoryDegradedInterval {
            chat_id: "persisted_gap".into(),
            run_id: "run".into(),
            from_seq: 10,
            to_seq: 12,
            reason: "Queue saturated".into(),
            recorded_at: Utc::now(),
        });
        store.sync_flush().unwrap();
        drop(store);

        let reopened = TrajectoryStore::open(temp.path()).unwrap();
        let intervals = reopened.get_degraded_intervals("persisted_gap").unwrap();
        assert_eq!(intervals.len(), 1);
        assert_eq!((intervals[0].from_seq, intervals[0].to_seq), (10, 12));
    }

    #[test]
    fn test_trajectory_store_poisoned_capture_mutexes_fail_open() {
        let temp = TempDir::new().unwrap();
        let store = TrajectoryStore::open(temp.path()).unwrap();
        let degraded_reason = store.degraded_reason.clone();
        let _ = std::thread::spawn(move || {
            let _guard = degraded_reason.lock().unwrap();
            panic!("poison degraded reason");
        })
        .join();
        assert!(!store.is_degraded());

        let in_memory = store.in_memory_degraded.clone();
        let _ = std::thread::spawn(move || {
            let _guard = in_memory.lock().unwrap();
            panic!("poison degraded intervals");
        })
        .join();
        store.record_degraded_in_memory(TrajectoryDegradedInterval {
            chat_id: "poison".into(),
            run_id: "run".into(),
            from_seq: 1,
            to_seq: 1,
            reason: "gap".into(),
            recorded_at: Utc::now(),
        });
        assert_eq!(store.get_degraded_intervals("poison").unwrap().len(), 1);
    }

    #[tokio::test]
    async fn test_trajectory_store_raw_ref_rejects_source_version_mismatch_for_both_fields() {
        let temp = TempDir::new().unwrap();
        let store = TrajectoryStore::open(temp.path()).unwrap();
        let mut record = sample_record("raw_version", "run", 1, 0);
        record.payload = Some(TrajectoryPayloadPreview {
            summary: "payload".into(),
            sanitized_text: None,
            schema_info: None,
            raw_ref: Some(TrajectoryRawRef::new(
                "raw_version",
                1,
                None,
                Some("call".into()),
                TrajectoryRawField::Payload,
            )),
        });
        record.result = Some(TrajectoryResultPreview {
            summary: "result".into(),
            sanitized_text: None,
            is_error: false,
            exit_code: None,
            raw_ref: Some(TrajectoryRawRef::new(
                "raw_version",
                1,
                None,
                Some("call".into()),
                TrajectoryRawField::Result,
            )),
        });
        store.try_enqueue(record).unwrap();
        store.flush().await.unwrap();

        for field in [TrajectoryRawField::Payload, TrajectoryRawField::Result] {
            let forged = TrajectoryRawRef::new("raw_version", 1, None, Some("call".into()), field)
                .with_version(CURRENT_RAW_SOURCE_VERSION + 1);
            assert!(!store.validate_raw_ref(&forged).unwrap());
        }
    }

    #[test]
    fn test_trajectory_store_rejects_newer_schema_version() {
        let temp = TempDir::new().unwrap();
        let db_path = temp.path().join("trajectory.sqlite3");
        let conn = Connection::open(&db_path).unwrap();
        conn.execute_batch(
            "CREATE TABLE schema_migrations (
                version INTEGER PRIMARY KEY,
                applied_at INTEGER NOT NULL
             ) STRICT;
             INSERT INTO schema_migrations VALUES (999, 1);",
        )
        .unwrap();
        drop(conn);

        let error = match TrajectoryStore::open(temp.path()) {
            Ok(_) => panic!("newer schema unexpectedly opened"),
            Err(error) => error,
        };
        assert!(error.to_string().contains("newer schema version"));
    }

    #[tokio::test]
    async fn test_trajectory_store_native_minimum_uses_run_legacy_flag() {
        let temp = TempDir::new().unwrap();
        let store = TrajectoryStore::open(temp.path()).unwrap();
        store
            .try_enqueue_batch(vec![
                sample_record("legacy_flags", "native", 1, 0),
                sample_record("legacy_flags", "legacyXnative", 10, 0),
            ])
            .unwrap();
        store.flush().await.unwrap();
        let conn = Connection::open(&store.db_path).unwrap();
        conn.execute(
            "UPDATE trajectory_runs SET is_legacy = 1 WHERE chat_id = ?1 AND run_id = 'native'",
            params!["legacy_flags"],
        )
        .unwrap();
        conn.execute(
            "UPDATE trajectory_runs SET is_legacy = 0 WHERE chat_id = ?1 AND run_id = 'legacyXnative'",
            params!["legacy_flags"],
        )
        .unwrap();
        drop(conn);

        assert_eq!(
            store.min_native_source_seq("legacy_flags").unwrap(),
            Some(10)
        );
    }

    #[tokio::test]
    async fn test_trajectory_store_batch_upserts_run_and_watermark_once_per_group() {
        let temp = TempDir::new().unwrap();
        let store = TrajectoryStore::open(temp.path()).unwrap();
        let conn = Connection::open(&store.db_path).unwrap();
        conn.execute_batch(
            "CREATE TABLE test_upserts (target TEXT NOT NULL);
             CREATE TRIGGER test_run_insert AFTER INSERT ON trajectory_runs
                BEGIN INSERT INTO test_upserts VALUES ('run'); END;
             CREATE TRIGGER test_run_update AFTER UPDATE ON trajectory_runs
                BEGIN INSERT INTO test_upserts VALUES ('run'); END;
             CREATE TRIGGER test_watermark_insert AFTER INSERT ON trajectory_watermarks
                BEGIN INSERT INTO test_upserts VALUES ('watermark'); END;
             CREATE TRIGGER test_watermark_update AFTER UPDATE ON trajectory_watermarks
                BEGIN INSERT INTO test_upserts VALUES ('watermark'); END;",
        )
        .unwrap();
        drop(conn);

        store
            .try_enqueue_batch(vec![
                sample_record("grouped", "run", 3, 0),
                sample_record("grouped", "run", 1, 0),
                sample_record("grouped", "run", 2, 0),
            ])
            .unwrap();
        store.flush().await.unwrap();

        let conn = store.reader().unwrap();
        for target in ["run", "watermark"] {
            let count: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM test_upserts WHERE target = ?1",
                    params![target],
                    |row| row.get(0),
                )
                .unwrap();
            assert_eq!(count, 1, "{target} upsert ran per record");
        }
        assert_eq!(store.get_watermark("grouped").unwrap(), Some(3));
    }
    #[test]
    fn test_trajectory_store_tool_diff_uses_originating_tool_name_and_path_preview() {
        let event = AgentEvent::ToolResult {
            id: "call".into(),
            is_error: false,
            output: None,
            diff: Some(zeron_proto::agent::ToolDiff {
                path: "/workspace/src/lib.rs".into(),
                old_text: Some("old".into()),
                new_text: "new".into(),
            }),
            execution: None,
        };
        let record = project_event_to_record("chat", "run", 1, &event, None, Some("edit")).unwrap();
        assert_eq!(
            record.kind,
            TrajectoryRecordKind::ToolDiff {
                tool_name: "edit".into()
            }
        );
        assert!(record.summary.contains("/workspace/src/lib.rs"));
        assert!(
            record
                .result
                .as_ref()
                .and_then(|result| result.sanitized_text.as_ref())
                .is_some_and(|preview| preview.contains("/workspace/src/lib.rs"))
        );
    }
    #[test]
    fn test_trajectory_store_persisted_degraded_reconcile_keeps_only_concurrent_extension() {
        let recorded_at = Utc::now();
        let persisted = TrajectoryDegradedInterval {
            chat_id: "chat".into(),
            run_id: "run".into(),
            from_seq: 1,
            to_seq: 5,
            reason: "Queue saturated".into(),
            recorded_at,
        };
        let mut current = VecDeque::from([TrajectoryDegradedInterval {
            to_seq: 6,
            ..persisted.clone()
        }]);
        discard_persisted_degraded(&mut current, &[persisted]);
        assert_eq!(current.len(), 1);
        assert_eq!((current[0].from_seq, current[0].to_seq), (6, 6));
    }

    #[tokio::test]
    async fn test_trajectory_store_diagnostics_reports_in_memory_degraded_and_watermark_consistency()
     {
        let temp = TempDir::new().unwrap();
        let store = TrajectoryStore::open(temp.path()).unwrap();

        // Enqueue one valid record
        let record = sample_record("diag_chat", "r1", 1, 0);
        store.try_enqueue(record).unwrap();
        store.flush().await.unwrap();

        // Record an in-memory degraded interval (simulating queue saturation without SQLite persistence yet)
        store.record_degraded_in_memory(TrajectoryDegradedInterval {
            chat_id: "diag_chat".into(),
            run_id: "r1".into(),
            from_seq: 2,
            to_seq: 5,
            reason: "Queue saturated".into(),
            recorded_at: Utc::now(),
        });

        let diag = store.diagnostics("diag_chat").unwrap();
        assert_eq!(diag.record_count, 1);
        assert_eq!(diag.run_count, 1);
        assert_eq!(diag.last_watermark, Some(1));
        assert_eq!(
            diag.degraded_count, 1,
            "diagnostics must include in-memory degraded intervals"
        );
    }

    #[tokio::test]
    async fn test_trajectory_store_writer_failure_observable_degraded_state() {
        let temp = TempDir::new().unwrap();
        let (closed_tx, _) = sync_channel(1);
        let (events_tx, _) = broadcast::channel(1);
        let broken_store = TrajectoryStore {
            db_path: temp.path().join("trajectory.sqlite3"),
            journals_dir: temp.path().join("journals"),
            writer_tx: closed_tx,
            in_memory_degraded: Arc::new(Mutex::new(VecDeque::new())),
            degraded_reason: Arc::new(Mutex::new(None)),
            events_tx,
            legacy_importing: Arc::new(Mutex::new(HashSet::new())),
        };

        let rec = sample_record("obs_chat", "r1", 1, 0);
        let res = broken_store.try_enqueue(rec);
        assert!(matches!(res, Err(TrajectoryStoreError::ChannelClosed)));

        // Store is observably degraded now
        assert!(
            broken_store.is_degraded(),
            "store must report is_degraded == true after writer disconnect"
        );
        let diag = broken_store.diagnostics("obs_chat").unwrap();
        assert_eq!(diag.degraded_count, 1);
        assert_eq!(diag.record_count, 0);
    }

    #[tokio::test]
    async fn test_trajectory_store_try_enqueue_does_not_clone_on_success_and_broadcast_zero_subscribers()
     {
        let temp = TempDir::new().unwrap();
        let store = TrajectoryStore::open(temp.path()).unwrap();

        // 0 subscribers -> enqueue and flush should succeed without cloning/broadcasting errors
        assert_eq!(store.events_tx.receiver_count(), 0);
        let rec = sample_record("bench_chat", "r1", 1, 0);
        store.try_enqueue(rec).unwrap();
        store.flush().await.unwrap();

        let records = store.list_all_records("bench_chat").unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].source_seq, 1);

        // With subscriber -> receives event
        let mut sub = store.subscribe_events();
        assert_eq!(store.events_tx.receiver_count(), 1);
        let rec2 = sample_record("bench_chat", "r1", 2, 0);
        store.try_enqueue(rec2).unwrap();
        store.flush().await.unwrap();

        let event = sub.recv().await.unwrap();
        match event {
            TrajectoryStoreEvent::RecordsCommitted {
                chat_id,
                records,
                watermark,
                rev,
            } => {
                assert_eq!(chat_id, "bench_chat");
                assert_eq!(records.len(), 1);
                assert_eq!(watermark, (2, 0));
                assert!(rev >= 2);
            }
            other => panic!("unexpected event: {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_trajectory_store_interleaved_saturation_coalescing_and_queue_bounds() {
        let temp = TempDir::new().unwrap();
        let store = TrajectoryStore::open(temp.path()).unwrap();

        // Interleave sequence intervals for two chats
        for seq in 1..=5 {
            store.record_degraded_in_memory(TrajectoryDegradedInterval {
                chat_id: "chat_A".into(),
                run_id: "r1".into(),
                from_seq: seq,
                to_seq: seq,
                reason: "Queue saturated".into(),
                recorded_at: Utc::now(),
            });
            store.record_degraded_in_memory(TrajectoryDegradedInterval {
                chat_id: "chat_B".into(),
                run_id: "r1".into(),
                from_seq: seq,
                to_seq: seq,
                reason: "Queue saturated".into(),
                recorded_at: Utc::now(),
            });
        }

        let intervals_a = store.get_degraded_intervals("chat_A").unwrap();
        assert_eq!(intervals_a.len(), 1);
        assert_eq!((intervals_a[0].from_seq, intervals_a[0].to_seq), (1, 5));

        let intervals_b = store.get_degraded_intervals("chat_B").unwrap();
        assert_eq!(intervals_b.len(), 1);
        assert_eq!((intervals_b[0].from_seq, intervals_b[0].to_seq), (1, 5));
    }

    #[tokio::test]
    async fn test_trajectory_store_degraded_intervals_coalesce_on_insert_and_limit() {
        let temp = TempDir::new().unwrap();
        let store = TrajectoryStore::open(temp.path()).unwrap();

        // Record discrete degraded intervals
        store.record_degraded_in_memory(TrajectoryDegradedInterval {
            chat_id: "c_limit".into(),
            run_id: "r1".into(),
            from_seq: 1,
            to_seq: 3,
            reason: "gap1".into(),
            recorded_at: Utc::now(),
        });
        store.record_degraded_in_memory(TrajectoryDegradedInterval {
            chat_id: "c_limit".into(),
            run_id: "r1".into(),
            from_seq: 10,
            to_seq: 15,
            reason: "gap2".into(),
            recorded_at: Utc::now(),
        });

        // Flush to SQLite
        store.sync_flush().unwrap();

        // Persisted rows check
        let all = store.get_degraded_intervals("c_limit").unwrap();
        assert_eq!(all.len(), 2);

        // With limit
        let limited = store
            .get_degraded_intervals_with_limit("c_limit", Some(1))
            .unwrap();
        assert_eq!(limited.len(), 1);

        // Coalesce insert on SQLite
        store.record_degraded_in_memory(TrajectoryDegradedInterval {
            chat_id: "c_limit".into(),
            run_id: "r1".into(),
            from_seq: 4,
            to_seq: 5,
            reason: "gap1".into(),
            recorded_at: Utc::now(),
        });
        store.sync_flush().unwrap();

        let after_coalesce = store.get_degraded_intervals("c_limit").unwrap();
        assert_eq!(after_coalesce.len(), 2);
        assert_eq!(
            (after_coalesce[0].from_seq, after_coalesce[0].to_seq),
            (1, 5)
        );
    }

    #[test]
    fn test_trajectory_store_pragmas_journal_mode_wal() {
        let temp = TempDir::new().unwrap();
        let store = TrajectoryStore::open(temp.path()).unwrap();
        let conn = store.reader().unwrap();
        let journal_mode: String = conn
            .query_row("PRAGMA journal_mode", [], |r| r.get(0))
            .unwrap();
        assert_eq!(journal_mode.to_lowercase(), "wal");

        let sync_mode: i64 = conn
            .query_row("PRAGMA synchronous", [], |r| r.get(0))
            .unwrap();
        // NORMAL synchronous is 1
        assert_eq!(sync_mode, 1);
    }

    #[tokio::test]
    async fn test_trajectory_store_stream_snapshot_pages_point_in_time_isolation() {
        let temp = TempDir::new().unwrap();
        let store = TrajectoryStore::open(temp.path()).unwrap();

        // Seed 2 initial records
        store
            .try_enqueue_batch(vec![
                sample_record("snap_iso", "r1", 1, 0),
                sample_record("snap_iso", "r1", 2, 0),
            ])
            .unwrap();
        store.flush().await.unwrap();

        let mut pages_seen = Vec::new();
        let store_clone = store.clone();

        store
            .stream_snapshot_pages("snap_iso", None, 1, |page, _cursor, has_more| {
                pages_seen.push(page);
                // Concurrently enqueue and commit new records during snapshot iteration!
                if pages_seen.len() == 1 {
                    store_clone
                        .try_enqueue_batch(vec![
                            sample_record("snap_iso", "r1", 3, 0),
                            sample_record("snap_iso", "r1", 4, 0),
                        ])
                        .unwrap();
                    store_clone.sync_flush().unwrap();
                }
                has_more
            })
            .unwrap();

        // Snapshot pages must see strictly records 1 and 2, isolated from concurrent commits 3 and 4
        assert_eq!(pages_seen.len(), 2);
        assert_eq!(pages_seen[0].len(), 1);
        assert_eq!(pages_seen[0][0].source_seq, 1);
        assert_eq!(pages_seen[1].len(), 1);
        assert_eq!(pages_seen[1][0].source_seq, 2);

        // But list_all_records afterwards sees all 4 records
        let all = store.list_all_records("snap_iso").unwrap();
        assert_eq!(all.len(), 4);
    }

    #[tokio::test]
    async fn test_trajectory_store_upgrade_schema_v1_to_v2_retains_records_and_seeds_rev() {
        let temp = TempDir::new().unwrap();
        let db_path = temp.path().join("trajectory.sqlite3");

        // Manually build a v1 database before opening TrajectoryStore
        {
            let conn = Connection::open(&db_path).unwrap();
            conn.execute_batch(
                "CREATE TABLE schema_migrations (
                    version    INTEGER PRIMARY KEY,
                    applied_at INTEGER NOT NULL
                 ) STRICT;",
            )
            .unwrap();
            conn.execute(
                "INSERT INTO schema_migrations VALUES (1, ?1)",
                params![Utc::now().timestamp_millis()],
            )
            .unwrap();
            conn.execute_batch(MIGRATIONS[0]).unwrap();

            // Insert a v1 record (no rev column in v1 table)
            let kind_json = serde_json::to_string(&TrajectoryRecordKind::SessionStarted).unwrap();
            let status_json = serde_json::to_string(&TrajectoryStatus::Running).unwrap();
            conn.execute(
                "INSERT INTO trajectory_records (
                    chat_id, run_id, source_seq, sub_seq, lane, kind, status, is_partial,
                    title, summary, created_at
                 ) VALUES ('up_chat', 'r1', 1, 0, 'input', ?1, ?2, 0, 'Title', 'Summary', 1000)",
                params![kind_json, status_json],
            )
            .unwrap();
        }

        // Open store -> runs migration v2 (ALTER TABLE ADD COLUMN rev ...)
        let store = TrajectoryStore::open(temp.path()).unwrap();

        // Old record is readable with rev = 0
        let records = store.list_all_records("up_chat").unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].source_seq, 1);

        // Enqueue a new record -> writer assigns monotonic revision >= 1
        let rec2 = sample_record("up_chat", "r1", 2, 0);
        store.try_enqueue(rec2).unwrap();
        store.flush().await.unwrap();

        let updated_records = store.list_all_records("up_chat").unwrap();
        assert_eq!(updated_records.len(), 2);
        assert_eq!(updated_records[1].source_seq, 2);

        // Verify rev in SQLite
        let conn = store.reader().unwrap();
        let rev2: i64 = conn
            .query_row(
                "SELECT rev FROM trajectory_records WHERE chat_id = 'up_chat' AND source_seq = 2",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert!(rev2 >= 1, "expected seeded next_rev >= 1, got {rev2}");
    }
}
