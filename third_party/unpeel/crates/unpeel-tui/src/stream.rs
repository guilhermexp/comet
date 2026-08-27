//! Live streamed-VT feed for the selected session's preview pane.
//!
//! The snapshot poller (`snapshots.rs`) asks the host to render and serialize
//! a full styled grid per fetch, so it is throttled — typing echo used to wait
//! out that throttle plus a tick. This module takes the attach client's path
//! instead: replay a bounded, host-aligned tail of `output.bin` into a local
//! ghostty-vt, then subscribe the control socket's output stream at exactly
//! that offset (the host's broadcaster replays the gap; subscribing far behind
//! it would get the socket dropped). Bytes feed the VT the moment they arrive
//! and the main loop is woken to publish a frame, so echo latency is the
//! host's 1ms batch flush plus one local render.
//!
//! One stream exists at a time — for the selected, running session, while the
//! TUI owns its grid (a phone-resized grid keeps the socket-snapshot path,
//! which letterboxes the host's true grid). The stream never answers terminal
//! probes (`answers_queries: false`): the TUI is a renderer, not a real
//! passthrough terminal, and the host keeps probe-answering exactly as it
//! does under the snapshot path.

use std::io::{BufReader, Write};
use std::os::unix::net::UnixStream;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use unpeel_core::session_host::{
    read_output_chunk, read_output_stream_frame, refresh_manifest_health, HostedSessionState,
    OutputStreamRead,
};
use unpeel_core::terminal_viewport::{TerminalViewportSnapshot, TerminalViewportState};

use crate::WakeGate;

/// Matches the snapshot replay budget (`DEFAULT_VIEWPORT_REPLAY_MAX_BYTES`):
/// enough history for scrollback without making stream start-up heavy.
const REPLAY_TAIL_BYTES: usize = 512 * 1024;
/// How often the reader thread surfaces to check the stop flag.
const STREAM_READ_TIMEOUT: Duration = Duration::from_millis(250);
const SOCKET_WRITE_TIMEOUT: Duration = Duration::from_millis(1_000);
/// How recently DEC 2026 synchronized-output activity must have been seen
/// for the publisher to treat the app as frame-declaring. Expires so a
/// session that moves on to a non-2026 program (the agent exits to a shell)
/// falls back to the quiet-gap heuristic instead of trusting stale state
/// forever.
const SYNC_RECENT: Duration = Duration::from_secs(2);

pub struct LiveStream {
    pub session_id: String,
    vt: Arc<Mutex<TerminalViewportState>>,
    stop: Arc<AtomicBool>,
    dead: Arc<AtomicBool>,
    dirty: Arc<AtomicBool>,
    /// Set once at least one live frame arrived — distinguishes a stream
    /// that worked and then lost its host (retry quickly) from one that
    /// never connected at all (dead fixture or old host: back off).
    went_live: Arc<AtomicBool>,
    /// Micros since `started` at which the reader last fed bytes into the
    /// VT. The publisher uses this to coalesce redraw bursts: a frame taken
    /// while bytes are still streaming in can catch a full-screen repaint
    /// half-done (the flicker), so it waits for a short quiet gap first.
    last_feed_micros: Arc<std::sync::atomic::AtomicU64>,
    /// Size of the last feed. A keystroke echo is a tiny read; a mid-burst
    /// repaint chunk arrives at the host's batch size. The quiet-gap
    /// heuristic only holds after LARGE feeds, so echo in non-2026 apps
    /// (a plain shell) publishes immediately (herdr's policy: coalescing
    /// shapes sustained throughput, never the first byte).
    last_feed_bytes: Arc<std::sync::atomic::AtomicU64>,
    /// Micros since `started` at which the feed last entered a DEC 2026
    /// synchronized update (per the VT's own mode state, sampled after each
    /// feed), 0 while outside one. While non-zero the grid is a declared
    /// mid-repaint — the publisher must not snapshot it.
    sync_open_micros: Arc<std::sync::atomic::AtomicU64>,
    /// Micros since `started` of the last observed 2026 activity (mode seen
    /// set, or a set→reset transition), 0 if never. Recency gates whether
    /// the publisher trusts mode state over quiet gaps (see `SYNC_RECENT`).
    sync_marker_micros: Arc<std::sync::atomic::AtomicU64>,
    /// Grid the main thread last set on the VT (it is the only resizer).
    grid: std::cell::Cell<(u16, u16)>,
    pub started: Instant,
}

impl LiveStream {
    /// Spawn the replay + subscribe thread. The stream reports failures by
    /// flipping `dead` (and waking the loop) rather than erroring: the caller
    /// falls back to the socket-snapshot path and retries with backoff.
    pub fn start(session_id: String, cols: u16, rows: u16, wake: WakeGate) -> Self {
        let vt = Arc::new(Mutex::new(TerminalViewportState::new(
            cols.max(4),
            rows.max(2),
        )));
        let stop = Arc::new(AtomicBool::new(false));
        let dead = Arc::new(AtomicBool::new(false));
        let dirty = Arc::new(AtomicBool::new(false));
        let went_live = Arc::new(AtomicBool::new(false));
        let last_feed_micros = Arc::new(std::sync::atomic::AtomicU64::new(0));
        let last_feed_bytes = Arc::new(std::sync::atomic::AtomicU64::new(0));
        let sync_open_micros = Arc::new(std::sync::atomic::AtomicU64::new(0));
        let sync_marker_micros = Arc::new(std::sync::atomic::AtomicU64::new(0));
        let started = Instant::now();
        {
            let session_id = session_id.clone();
            let vt = Arc::clone(&vt);
            let stop = Arc::clone(&stop);
            let dead = Arc::clone(&dead);
            let dirty = Arc::clone(&dirty);
            let went_live = Arc::clone(&went_live);
            let last_feed_micros = Arc::clone(&last_feed_micros);
            let last_feed_bytes = Arc::clone(&last_feed_bytes);
            let sync_open_micros = Arc::clone(&sync_open_micros);
            let sync_marker_micros = Arc::clone(&sync_marker_micros);
            std::thread::spawn(move || {
                let feed_clock = FeedClock {
                    started,
                    micros: last_feed_micros,
                    bytes: last_feed_bytes,
                };
                let sync = SyncTracker {
                    started,
                    open_micros: sync_open_micros,
                    marker_micros: sync_marker_micros,
                };
                run_stream(
                    &session_id,
                    &vt,
                    &stop,
                    &dirty,
                    &went_live,
                    &feed_clock,
                    &sync,
                    &wake,
                );
                dead.store(true, Ordering::Relaxed);
                if !stop.load(Ordering::Relaxed) {
                    wake.wake();
                }
            });
        }
        LiveStream {
            session_id,
            vt,
            stop,
            dead,
            dirty,
            went_live,
            last_feed_micros,
            last_feed_bytes,
            sync_open_micros,
            sync_marker_micros,
            grid: std::cell::Cell::new((cols.max(4), rows.max(2))),
            started,
        }
    }

    pub fn is_dead(&self) -> bool {
        self.dead.load(Ordering::Relaxed)
    }

    /// When a dead stream may be retried. A stream that streamed real frames
    /// lost a live host (restart, brief hiccup) — retry quickly. One that
    /// never produced a frame is talking to something that can't serve it (a
    /// fixture manifest, a pre-stream host) — poll slowly and let the socket
    /// snapshot path carry the preview.
    pub fn retry_delay(&self) -> Duration {
        if self.went_live.load(Ordering::Relaxed) {
            Duration::from_secs(1)
        } else {
            Duration::from_secs(5)
        }
    }

    /// Keep the local VT on the PTY's grid (the pane, which the host is told
    /// to match by `resize_selected_to_pane`).
    pub fn resize(&self, cols: u16, rows: u16) {
        let cols = cols.max(4);
        let rows = rows.max(2);
        if self.grid.get() == (cols, rows) {
            return;
        }
        self.grid.set((cols, rows));
        if let Ok(mut vt) = self.vt.lock() {
            vt.resize(cols, rows);
        }
        self.dirty.store(true, Ordering::Relaxed);
    }

    /// True when bytes arrived since the last consumed frame; the main loop
    /// publishes when set (peek with `is_dirty`, clear with `take_dirty` so a
    /// throttled frame isn't lost).
    pub fn is_dirty(&self) -> bool {
        self.dirty.load(Ordering::Relaxed)
    }

    pub fn take_dirty(&self) -> bool {
        self.dirty.swap(false, Ordering::Relaxed)
    }

    /// How long since the reader last fed bytes into the VT. The publisher
    /// holds frames while this is small — mid-burst the grid may be a
    /// half-applied repaint (Claude clears with 2J and redraws; the chunk
    /// boundary can land anywhere in between).
    pub fn since_last_feed(&self) -> Duration {
        let fed = Duration::from_micros(self.last_feed_micros.load(Ordering::Relaxed));
        self.started.elapsed().saturating_sub(fed)
    }

    /// Size of the most recent feed — see `last_feed_bytes` on the struct.
    pub fn last_feed_len(&self) -> u64 {
        self.last_feed_bytes.load(Ordering::Relaxed)
    }

    /// False until the reader's initial tail replay lands in the VT. A
    /// brand-new stream holds an EMPTY grid, and the (re)start resets the
    /// publish key — publishing that first key-changed frame before the
    /// replay wins the race paints the pane blank for a beat (the "randomly
    /// goes blank then back" blink on every stream restart).
    pub fn has_fed(&self) -> bool {
        self.last_feed_micros.load(Ordering::Relaxed) != 0
    }

    /// Some(elapsed since the open marker) while the feed sits inside a DEC
    /// 2026 synchronized update — the app has started a repaint (typically
    /// erase-then-redraw) and not yet declared it complete. A snapshot taken
    /// now would show the blank/half-painted grid: the blink.
    pub fn sync_open_elapsed(&self) -> Option<Duration> {
        let opened = self.sync_open_micros.load(Ordering::Relaxed);
        if opened == 0 {
            return None;
        }
        Some(
            self.started
                .elapsed()
                .saturating_sub(Duration::from_micros(opened)),
        )
    }

    /// True while the app is actively declaring its frames with 2026 markers
    /// (one seen within `SYNC_RECENT`). The publisher then trusts marker
    /// state — publish freely between frames, never inside one — instead of
    /// the quiet-gap heuristic.
    pub fn sync_frames_active(&self) -> bool {
        let last = self.sync_marker_micros.load(Ordering::Relaxed);
        last != 0
            && self
                .started
                .elapsed()
                .saturating_sub(Duration::from_micros(last))
                < SYNC_RECENT
    }

    /// `try_lock`, not `lock`: the feeder holds the VT during the initial
    /// tail replay (tens of ms), and the UI thread must not stall on it. A
    /// skipped frame is safe — every feed ends by setting dirty and waking
    /// the loop, so the publish retries immediately after.
    pub fn snapshot(&self, scroll_offset_rows: u32) -> Option<TerminalViewportSnapshot> {
        let mut vt = self.vt.try_lock().ok()?;
        Some(vt.snapshot(scroll_offset_rows, None))
    }
}

impl Drop for LiveStream {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

/// Reader-side clock for `LiveStream::since_last_feed`: stamps "bytes just
/// landed in the VT" as micros since the stream started, lock-free.
struct FeedClock {
    started: Instant,
    micros: Arc<std::sync::atomic::AtomicU64>,
    bytes: Arc<std::sync::atomic::AtomicU64>,
}

impl FeedClock {
    fn stamp(&self, fed_bytes: usize) {
        // `.max(1)`: 0 must keep meaning "never fed" (see `has_fed`).
        self.micros.store(
            (self.started.elapsed().as_micros() as u64).max(1),
            Ordering::Relaxed,
        );
        self.bytes.store(fed_bytes as u64, Ordering::Relaxed);
    }
}

/// Reader-side observer of the VT's DEC 2026 mode, sampled after each feed
/// while the VT lock is already held — ghostty itself parses the markers
/// (herdr does exactly this), so chunk-boundary splits and marker bytes
/// appearing in literal text are handled for free.
struct SyncTracker {
    started: Instant,
    open_micros: Arc<std::sync::atomic::AtomicU64>,
    marker_micros: Arc<std::sync::atomic::AtomicU64>,
}

impl SyncTracker {
    /// Returns true when this observation *completed* a frame (mode went
    /// set→reset). The caller must wake the publisher unconditionally then:
    /// the dirty flag's edge-triggered wake already fired for the chunk
    /// that OPENED the frame, so without a fresh wake the completed frame
    /// would sit until the next tick — felt as sluggish typing echo.
    fn observe(&self, open: bool) -> bool {
        // `.max(1)` keeps 0 meaning "closed"/"never".
        let now = (self.started.elapsed().as_micros() as u64).max(1);
        if open {
            self.marker_micros.store(now, Ordering::Relaxed);
            // Keep the original open stamp so the publisher's stuck-open
            // cap measures from when the update actually began.
            let _ = self
                .open_micros
                .compare_exchange(0, now, Ordering::Relaxed, Ordering::Relaxed);
            false
        } else if self.open_micros.swap(0, Ordering::Relaxed) != 0 {
            self.marker_micros.store(now, Ordering::Relaxed);
            true
        } else {
            false
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn run_stream(
    session_id: &str,
    vt: &Mutex<TerminalViewportState>,
    stop: &AtomicBool,
    dirty: &AtomicBool,
    went_live: &AtomicBool,
    feed_clock: &FeedClock,
    sync: &SyncTracker,
    wake: &WakeGate,
) {
    // Connect before the replay: a host that can't serve (not yet bound,
    // exited, a fixture) makes the retry cost one failed syscall instead of
    // a tail replay through the VT.
    let socket_path = unpeel_core::app_paths::app_sessions_root()
        .join(session_id)
        .join("session.sock");
    let Ok(mut socket) = UnixStream::connect(&socket_path) else {
        return;
    };
    let _ = socket.set_write_timeout(Some(SOCKET_WRITE_TIMEOUT));

    // Bounded, escape/UTF-8-aligned tail replay; its next_offset is where the
    // live subscription must begin so no byte is missed or doubled. Bytes
    // written between this read and the subscribe below are replayed by the
    // host's in-memory broadcaster ring.
    let Ok(replay) = read_output_chunk(
        session_id,
        None,
        Some(REPLAY_TAIL_BYTES),
        Some(REPLAY_TAIL_BYTES),
    ) else {
        return;
    };
    if !replay.exists || replay.exited {
        return;
    }
    let mut frame_done = false;
    if let Ok(mut guard) = vt.lock() {
        guard.feed(&replay.data);
        frame_done = sync.observe(guard.synchronized_output_active());
    }
    feed_clock.stamp(replay.data.len());
    mark_dirty(dirty, wake);
    if frame_done {
        wake.wake();
    }

    let mut command = serde_json::json!({
        "type": "stream_output",
        "offset": replay.next_offset,
        "answers_queries": false,
    })
    .to_string();
    command.push('\n');
    if socket.write_all(command.as_bytes()).is_err() {
        return;
    }
    let _ = socket.set_read_timeout(Some(STREAM_READ_TIMEOUT));
    let mut reader = BufReader::new(socket);

    let mut idle_timeouts: u32 = 0;
    let mut health_strikes: u32 = 0;
    while !stop.load(Ordering::Relaxed) {
        match read_output_stream_frame(&mut reader) {
            Ok(OutputStreamRead::Chunk(chunk)) => {
                idle_timeouts = 0;
                health_strikes = 0;
                went_live.store(true, Ordering::Relaxed);
                if !chunk.data.is_empty() {
                    let mut frame_done = false;
                    if let Ok(mut guard) = vt.lock() {
                        guard.feed(&chunk.data);
                        frame_done = sync.observe(guard.synchronized_output_active());
                    }
                    feed_clock.stamp(chunk.data.len());
                    mark_dirty(dirty, wake);
                    // The dirty edge fired when the frame OPENED; without a
                    // fresh wake here the completed frame waits for the next
                    // tick — felt as sluggish typing echo.
                    if frame_done {
                        wake.wake();
                    }
                }
                if chunk.exited || !chunk.exists {
                    return;
                }
            }
            Ok(OutputStreamRead::TimedOut) => {
                // The snapshot poller this stream replaces was also the
                // observer that noticed dying hosts: its reads run
                // refresh_manifest_health, which flips a running manifest
                // to exited once the host's socket/heartbeat is gone — the
                // "observer-side reaper" stop_session relies on when a
                // SIGTERM-immune child (an interactive shell at its prompt)
                // keeps the host process alive. Carry that observation
                // here: on a quiet stream, re-check health about once a
                // second and end the stream when the session stops running.
                idle_timeouts += 1;
                if idle_timeouts % 4 == 0 {
                    // Two consecutive strikes before giving up: a single
                    // failed read here used to end the stream — and with it
                    // blank the preview — on any transient (a manifest read
                    // racing a writer, a slow disk). A genuinely dead host
                    // just costs one extra second to notice.
                    match refresh_manifest_health(session_id) {
                        Some(m) if m.state == HostedSessionState::Running => health_strikes = 0,
                        _ => {
                            health_strikes += 1;
                            if health_strikes >= 2 {
                                return;
                            }
                        }
                    }
                }
            }
            Ok(OutputStreamRead::Closed) | Err(_) => return,
        }
    }
}

/// Wake the main loop only on the clean→dirty edge, so a chunk storm sends
/// one wake per published frame instead of one per chunk.
fn mark_dirty(dirty: &AtomicBool, wake: &WakeGate) {
    if !dirty.swap(true, Ordering::Relaxed) {
        wake.wake();
    }
}
