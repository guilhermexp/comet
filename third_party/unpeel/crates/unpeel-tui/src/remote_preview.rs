//! Remote terminal feed for a TUI acting only as a Controller.
//!
//! Unlike `stream.rs`, this module never opens a local session directory or
//! control socket. It consumes committed output pages from the shared
//! `RemoteSessionBackend`, feeds them into the same Ghostty VT used by the
//! local TUI, and leaves the last good frame visible while a read reconnects.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use unpeel_core::remote_session_backend::{
    RemoteOutputPollOptions, RemoteSessionBackend, REMOTE_OUTPUT_DEFAULT_LIMIT,
};
use unpeel_core::terminal_viewport::{TerminalViewportSnapshot, TerminalViewportState};

const OUTPUT_WAIT: Duration = Duration::from_secs(1);
const RETRY_DELAY: Duration = Duration::from_millis(350);
const SYNC_MAX_HOLD: Duration = Duration::from_millis(150);

type Wake = Arc<dyn Fn() + Send + Sync>;

pub struct RemoteLiveStream {
    viewport: Arc<Mutex<TerminalViewportState>>,
    grid: Arc<Mutex<(u16, u16)>>,
    stop: Arc<AtomicBool>,
    dirty: Arc<AtomicBool>,
    connected: Arc<AtomicBool>,
    last_error: Arc<Mutex<Option<String>>>,
    sync_open_since: Arc<Mutex<Option<std::time::Instant>>>,
}

impl RemoteLiveStream {
    pub fn start(
        backend: RemoteSessionBackend,
        session_id: String,
        columns: u16,
        rows: u16,
        wake: Wake,
    ) -> Self {
        let columns = columns.max(4);
        let rows = rows.max(2);
        let viewport = Arc::new(Mutex::new(TerminalViewportState::new(columns, rows)));
        let grid = Arc::new(Mutex::new((columns, rows)));
        let stop = Arc::new(AtomicBool::new(false));
        let dirty = Arc::new(AtomicBool::new(false));
        let connected = Arc::new(AtomicBool::new(false));
        let last_error = Arc::new(Mutex::new(None));
        let sync_open_since = Arc::new(Mutex::new(None));

        {
            let worker_session_id = session_id.clone();
            let worker_viewport = Arc::clone(&viewport);
            let worker_grid = Arc::clone(&grid);
            let worker_stop = Arc::clone(&stop);
            let worker_dirty = Arc::clone(&dirty);
            let worker_connected = Arc::clone(&connected);
            let worker_error = Arc::clone(&last_error);
            std::thread::Builder::new()
                .name("unpeel-remote-preview".to_owned())
                .spawn(move || {
                    run_output_feed(
                        backend,
                        &worker_session_id,
                        worker_viewport,
                        worker_grid,
                        worker_stop,
                        worker_dirty,
                        worker_connected,
                        worker_error,
                        wake,
                    )
                })
                .expect("spawn remote terminal feed");
        }

        Self {
            viewport,
            grid,
            stop,
            dirty,
            connected,
            last_error,
            sync_open_since,
        }
    }

    pub fn resize(&self, columns: u16, rows: u16) {
        let next = (columns.max(4), rows.max(2));
        let changed = self
            .grid
            .lock()
            .map(|mut grid| {
                if *grid == next {
                    false
                } else {
                    *grid = next;
                    true
                }
            })
            .unwrap_or(false);
        if changed {
            if let Ok(mut viewport) = self.viewport.lock() {
                viewport.resize(next.0, next.1);
            }
            self.dirty.store(true, Ordering::Release);
        }
    }

    pub fn is_dirty(&self) -> bool {
        self.dirty.load(Ordering::Acquire) && !self.sync_frame_should_wait()
    }

    pub fn is_connected(&self) -> bool {
        self.connected.load(Ordering::Acquire)
    }

    pub fn last_error(&self) -> Option<String> {
        self.last_error.lock().ok()?.clone()
    }

    /// Whether the Host stream is inside a DEC 2026 synchronized update.
    /// `None` means the feed worker owns the renderer momentarily.
    pub fn synchronized_output_active(&self) -> Option<bool> {
        let viewport = self.viewport.try_lock().ok()?;
        Some(viewport.synchronized_output_active())
    }

    fn sync_frame_should_wait(&self) -> bool {
        let Some(active) = self.synchronized_output_active() else {
            return false;
        };
        let Ok(mut opened) = self.sync_open_since.lock() else {
            return false;
        };
        if !active {
            *opened = None;
            return false;
        }
        let since = opened.get_or_insert_with(std::time::Instant::now);
        since.elapsed() < SYNC_MAX_HOLD
    }

    pub fn snapshot(&self, scroll_offset_rows: u32) -> Option<TerminalViewportSnapshot> {
        let mut viewport = self.viewport.try_lock().ok()?;
        let sync_active = viewport.synchronized_output_active();
        let wait_for_close = self
            .sync_open_since
            .lock()
            .map(|mut opened| {
                if !sync_active {
                    *opened = None;
                    false
                } else {
                    let since = opened.get_or_insert_with(std::time::Instant::now);
                    since.elapsed() < SYNC_MAX_HOLD
                }
            })
            .unwrap_or(false);
        if wait_for_close {
            // Keep dirty set: the matching mode exit will wake the UI, and
            // publishing this intermediate grid would expose an application
            // repaint that explicitly declared itself incomplete. The hold
            // is bounded so a lost close marker cannot freeze the preview.
            return None;
        }
        let snapshot = viewport.snapshot(scroll_offset_rows, None);
        // Consume dirtiness while holding the same lock the feed worker must
        // take before changing the frame. Any page applied after this point
        // sets the flag again, so the UI cannot erase a newer wake after
        // publishing an older snapshot.
        self.dirty.store(false, Ordering::Release);
        Some(snapshot)
    }
}

impl Drop for RemoteLiveStream {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
    }
}

#[allow(clippy::too_many_arguments)]
fn run_output_feed(
    backend: RemoteSessionBackend,
    session_id: &str,
    viewport: Arc<Mutex<TerminalViewportState>>,
    grid: Arc<Mutex<(u16, u16)>>,
    stop: Arc<AtomicBool>,
    dirty: Arc<AtomicBool>,
    connected: Arc<AtomicBool>,
    last_error: Arc<Mutex<Option<String>>>,
    wake: Wake,
) {
    if let Err(error) = backend.reset_output_cursor(session_id) {
        set_error(&last_error, error.to_string());
        wake();
        return;
    }

    while !stop.load(Ordering::Acquire) {
        let page = match backend.poll_output(
            session_id,
            RemoteOutputPollOptions {
                limit: REMOTE_OUTPUT_DEFAULT_LIMIT,
                wait: OUTPUT_WAIT,
            },
        ) {
            Ok(page) => page,
            Err(error) => {
                connected.store(false, Ordering::Release);
                set_error(&last_error, error.to_string());
                wake();
                if wait_for_stop(&stop, RETRY_DELAY) {
                    return;
                }
                continue;
            }
        };

        if stop.load(Ordering::Acquire) {
            page.discard();
            return;
        }

        let page_has_bytes = !page.bytes().is_empty();
        let reset = page.reset_required();
        let page_offset = page.offset();
        let page_next_offset = page.next_offset();
        let page_truncated = page.truncated() || page_offset > 0;
        let applied = if let Ok(mut terminal) = viewport.lock() {
            let (columns, rows) = grid.lock().map(|value| *value).unwrap_or((80, 24));
            if let Err(error) = feed_output_page(
                &mut terminal,
                (columns, rows),
                page_offset,
                page_next_offset,
                page.bytes(),
                reset,
                page_truncated,
            ) {
                drop(terminal);
                page.discard();
                if stop.load(Ordering::Acquire) {
                    return;
                }
                reset_renderer_and_cursor(&backend, session_id, &viewport, &grid);
                set_error(&last_error, error.message().to_owned());
                wake();
                continue;
            }

            // Keep the viewport lock through commit. The UI can never
            // publish bytes whose semantic cursor did not commit.
            match page.commit() {
                Ok(()) => true,
                Err(error) => {
                    drop(terminal);
                    if stop.load(Ordering::Acquire) {
                        return;
                    }
                    reset_renderer_and_cursor(&backend, session_id, &viewport, &grid);
                    set_error(&last_error, error.to_string());
                    wake();
                    continue;
                }
            }
        } else {
            page.discard();
            false
        };

        if applied {
            connected.store(true, Ordering::Release);
            clear_error(&last_error);
            // A reset represents a new valid frame even when the Host tail
            // is empty. Empty continuation polls do not cause redraw churn.
            if reset || page_has_bytes {
                dirty.store(true, Ordering::Release);
                wake();
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FeedPageError {
    CursorDiverged,
    LengthInconsistent,
}

impl FeedPageError {
    fn message(self) -> &'static str {
        match self {
            Self::CursorDiverged => "remote output cursor diverged; fetching a fresh tail",
            Self::LengthInconsistent => {
                "remote output page length was inconsistent; fetching a fresh tail"
            }
        }
    }
}

/// Apply one staged Host page to the renderer without advancing the semantic
/// backend cursor. The caller keeps the viewport locked until `page.commit()`
/// succeeds, so a frame can never be published ahead of its committed cursor.
fn feed_output_page(
    terminal: &mut TerminalViewportState,
    grid: (u16, u16),
    page_offset: u64,
    page_next_offset: u64,
    bytes: &[u8],
    reset: bool,
    history_truncated: bool,
) -> Result<(), FeedPageError> {
    if reset {
        terminal.reset_at_output_offset(grid.0, grid.1, page_offset, history_truncated);
    } else if terminal.output_offset() != page_offset {
        return Err(FeedPageError::CursorDiverged);
    }

    terminal.feed(bytes);
    if terminal.output_offset() != page_next_offset {
        return Err(FeedPageError::LengthInconsistent);
    }
    Ok(())
}

fn reset_renderer_and_cursor(
    backend: &RemoteSessionBackend,
    session_id: &str,
    viewport: &Mutex<TerminalViewportState>,
    grid: &Mutex<(u16, u16)>,
) {
    let _ = backend.reset_output_cursor(session_id);
    let (columns, rows) = grid.lock().map(|value| *value).unwrap_or((80, 24));
    if let Ok(mut terminal) = viewport.lock() {
        terminal.reset_at_output_offset(columns, rows, 0, false);
    }
}

fn wait_for_stop(stop: &AtomicBool, duration: Duration) -> bool {
    let deadline = std::time::Instant::now() + duration;
    while std::time::Instant::now() < deadline {
        if stop.load(Ordering::Acquire) {
            return true;
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    stop.load(Ordering::Acquire)
}

fn set_error(slot: &Mutex<Option<String>>, error: String) {
    if let Ok(mut slot) = slot.lock() {
        *slot = Some(error);
    }
}

fn clear_error(slot: &Mutex<Option<String>>) {
    if let Ok(mut slot) = slot.lock() {
        *slot = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_stream(dirty: bool) -> RemoteLiveStream {
        RemoteLiveStream {
            viewport: Arc::new(Mutex::new(TerminalViewportState::new(8, 2))),
            grid: Arc::new(Mutex::new((8, 2))),
            stop: Arc::new(AtomicBool::new(false)),
            dirty: Arc::new(AtomicBool::new(dirty)),
            connected: Arc::new(AtomicBool::new(true)),
            last_error: Arc::new(Mutex::new(None)),
            sync_open_since: Arc::new(Mutex::new(None)),
        }
    }

    fn visible_rows(snapshot: &TerminalViewportSnapshot) -> Vec<&str> {
        snapshot
            .viewport_rows
            .iter()
            .map(|row| row.text.as_str())
            .collect()
    }

    #[test]
    fn reset_page_replaces_stale_frame_and_rebases_absolute_cursor() {
        let mut terminal = TerminalViewportState::new(8, 2);
        terminal.feed(b"stale");

        feed_output_page(&mut terminal, (6, 1), 40, 45, b"fresh", true, true).unwrap();

        let snapshot = terminal.snapshot(0, None);
        assert_eq!(snapshot.cols, 6);
        assert_eq!(snapshot.rows, 1);
        assert_eq!(snapshot.output_offset, 45);
        assert!(snapshot.truncated);
        assert_eq!(visible_rows(&snapshot), vec!["fresh"]);
    }

    #[test]
    fn continuation_pages_require_and_advance_the_exact_renderer_cursor() {
        let mut terminal = TerminalViewportState::new(8, 2);
        terminal.reset_at_output_offset(8, 2, 10, false);

        feed_output_page(&mut terminal, (8, 2), 10, 15, b"hello", false, false).unwrap();
        feed_output_page(&mut terminal, (8, 2), 15, 16, b"!", false, false).unwrap();

        let snapshot = terminal.snapshot(0, None);
        assert_eq!(snapshot.output_offset, 16);
        assert_eq!(visible_rows(&snapshot)[0], "hello!");
    }

    #[test]
    fn divergent_continuation_is_rejected_before_mutating_the_frame() {
        let mut terminal = TerminalViewportState::new(8, 2);
        terminal.feed(b"keep");

        let error = feed_output_page(&mut terminal, (8, 2), 3, 4, b"x", false, false).unwrap_err();

        assert_eq!(error, FeedPageError::CursorDiverged);
        let snapshot = terminal.snapshot(0, None);
        assert_eq!(snapshot.output_offset, 4);
        assert_eq!(visible_rows(&snapshot)[0], "keep");
    }

    #[test]
    fn inconsistent_declared_page_length_is_detected() {
        let mut terminal = TerminalViewportState::new(8, 2);

        let error = feed_output_page(&mut terminal, (8, 2), 0, 99, b"x", false, false).unwrap_err();

        assert_eq!(error, FeedPageError::LengthInconsistent);
        assert_eq!(terminal.output_offset(), 1);
    }

    #[test]
    fn snapshot_consumes_only_the_frame_it_published() {
        let stream = test_stream(true);

        let first = stream.snapshot(0).unwrap();
        assert_eq!((first.cols, first.rows), (8, 2));
        assert!(!stream.is_dirty());

        // A frame change after the snapshot re-arms the wake and is not
        // consumed until that changed frame is itself snapshotted.
        stream.resize(6, 3);
        assert!(stream.is_dirty());
        let resized = stream.snapshot(0).unwrap();
        assert_eq!((resized.cols, resized.rows), (6, 3));
        assert!(!stream.is_dirty());
    }

    #[test]
    fn post_snapshot_feed_rearms_dirty_after_the_old_frame_was_consumed() {
        let stream = Arc::new(test_stream(true));
        let old_frame = stream.snapshot(0).unwrap();
        assert_eq!(old_frame.output_offset, 0);

        let (applied_tx, applied_rx) = std::sync::mpsc::channel();
        let worker = {
            let stream = Arc::clone(&stream);
            std::thread::spawn(move || {
                stream.viewport.lock().unwrap().feed(b"new");
                stream.dirty.store(true, Ordering::Release);
                applied_tx.send(()).unwrap();
            })
        };
        applied_rx.recv_timeout(Duration::from_secs(1)).unwrap();

        assert!(stream.is_dirty());
        let new_frame = stream.snapshot(0).unwrap();
        assert_eq!(new_frame.output_offset, 3);
        assert!(!stream.is_dirty());
        worker.join().unwrap();
    }

    #[test]
    fn synchronized_output_is_not_published_until_the_mode_exits() {
        let stream = test_stream(false);
        {
            let mut viewport = stream.viewport.lock().unwrap();
            viewport.feed(b"\x1b[?2026hpartial");
        }
        stream.dirty.store(true, Ordering::Release);

        assert_eq!(stream.synchronized_output_active(), Some(true));
        assert!(stream.snapshot(0).is_none());
        assert!(stream.dirty.load(Ordering::Acquire));
        assert!(!stream.is_dirty());

        {
            let mut viewport = stream.viewport.lock().unwrap();
            viewport.feed(b" complete\x1b[?2026l");
        }
        stream.dirty.store(true, Ordering::Release);

        assert_eq!(stream.synchronized_output_active(), Some(false));
        let frame = stream.snapshot(0).unwrap();
        assert_eq!(frame.output_offset, 32);
        assert!(!stream.is_dirty());
    }

    #[test]
    fn unclosed_synchronized_output_cannot_freeze_the_preview_forever() {
        let stream = test_stream(true);
        stream.viewport.lock().unwrap().feed(b"\x1b[?2026hpartial");
        *stream.sync_open_since.lock().unwrap() =
            Some(std::time::Instant::now() - SYNC_MAX_HOLD - Duration::from_millis(1));

        assert!(stream.is_dirty());
        let frame = stream.snapshot(0).expect("stale sync hold must expire");
        assert_eq!(frame.output_offset, b"\x1b[?2026hpartial".len() as u64);
        assert!(!stream.is_dirty());
    }
}
