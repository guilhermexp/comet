//! unpeel-tui — a herdr-style monitor over Unpeel hosted sessions.
//!
//! Pure client of the shared contracts: manifests + `app-state.json` for the
//! sidebar, the ported activity engine fed by live hook broadcasts (own port
//! in `~/.unpeel/app-ports`) for busy/idle/attention, a live streamed VT for
//! the selected session's preview (`stream.rs` — output bytes into a local
//! ghostty-vt, same latency class as the app's attach) with virtual viewport
//! snapshots as the fallback (dead hosts, phone-owned grids; never perturbs
//! the grid), and the native app's authed `/mcp/*` bridge for lifecycle
//! verbs — stop-and-archive, restart with resume, remove, pin. It never
//! spawns hosts; PTY writes are user keystrokes in the focused pane only.

mod activity;
mod approvals;
mod bridge;
mod cli;
mod control;
mod herdr;
mod hook_listener;
mod keys;
mod mascot;
mod mobile;
mod overlay;
mod pairing;
mod palette;
mod relay;
mod remote_preview;
mod remote_scope;
mod runtime_presentation;
mod sessions;
mod snapshots;
mod stream;
mod ui;
mod update;
mod workspaces;

use std::collections::{HashMap, HashSet};
use std::io;
use std::sync::mpsc;
use std::time::{Duration, Instant, SystemTime};

use ratatui::crossterm::event::{
    self, DisableBracketedPaste, DisableMouseCapture, EnableBracketedPaste, EnableMouseCapture,
    Event, KeyCode, KeyEvent, KeyEventKind, MouseButton, MouseEvent, MouseEventKind,
};
use ratatui::crossterm::execute;
use ratatui::crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, BeginSynchronizedUpdate, EndSynchronizedUpdate,
    EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::layout::Rect;
use ratatui::prelude::CrosstermBackend;
use ratatui::Terminal;
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;
use unpeel_core::terminal_viewport::TerminalViewportSnapshot;

use activity::ActivityEngine;
use sessions::{
    load_app_state, model_from_bridge, scan_sidebar, ScanCache, SessionRow, SidebarItem,
    SidebarModel, Status,
};
use snapshots::{SnapshotRequest, SnapshotService};

const TICK: Duration = Duration::from_millis(100);
/// Bound how long one pass drains terminal input before presenting a frame.
/// Precision trackpads can keep crossterm's channel continuously non-empty;
/// draining it to empty made scrolling starve redraws indefinitely.
const INPUT_DRAIN_BUDGET: Duration = Duration::from_millis(4);
/// Floor between published live-VT frames: a chunk storm renders at ~60fps
/// instead of once per chunk. A lone keystroke echo is never throttled — the
/// previous frame is always older than this by the time a human types.
const LIVE_FRAME_MIN_INTERVAL: Duration = Duration::from_millis(16);
/// Burst coalescing for the live preview: don't render while bytes are still
/// streaming in. A full-screen agent repaints with `2J` + redraw, and the
/// socket chunks it arbitrarily — a frame taken mid-burst shows a blank or
/// half-painted grid for a whole TUI frame (the "random blinking"; the
/// desktop app never shows this only because its Metal surface replaces such
/// a frame within milliseconds). Publish once the stream has been quiet this
/// long…
const QUIET_FLUSH: Duration = Duration::from_millis(8);
/// …but never hold longer than this, so continuous output (build logs,
/// streaming tokens, which have no quiet gaps) still renders steadily.
const BURST_MAX_HOLD: Duration = Duration::from_millis(40);
/// When the app declares its repaints with DEC 2026 markers (Claude wraps
/// every frame), marker state replaces the gap heuristic: never publish
/// while a sync block is open. Capped in case a close marker never comes
/// (the app crashed mid-frame) — past the cap frames flow again.
const SYNC_MAX_HOLD: Duration = Duration::from_millis(150);
/// The quiet-gap heuristic only holds after a feed at least this large. A
/// keystroke echo is a handful of bytes and must render NOW; coalescing is
/// for mid-burst repaint chunks, which arrive at the host's batch size
/// (herdr's policy: delays shape sustained throughput, never the first
/// byte of an echo).
const QUIET_FLUSH_MIN_FEED_BYTES: u64 = 1024;
/// Two clicks on the same session row within this window is a double-click
/// (crossterm reports only individual presses; see `App::last_click`).
const DOUBLE_CLICK: Duration = Duration::from_millis(400);
/// How many columns before the sidebar divider count as the header's "+ New"
/// hover affordance: the label plus a little slack, so the target is
/// clickable without being so wide it eats collapse clicks on the name.
const HEADER_ADD_ZONE: u16 = ui::HEADER_ADD_LABEL.len() as u16 + 2;
/// How long an outcome toast ("removed", "renamed", …) stays in the top-right
/// corner before dismissing itself. Any keypress dismisses it sooner.
const TOAST_TTL: Duration = Duration::from_secs(5);
/// Failed auto-archive attempts are retried, but never on every one-second
/// sidebar scan. One minute keeps transient Host churn recoverable without
/// turning a persistent failure into an archive request loop.
const AUTO_ARCHIVE_RETRY_DELAY_MS: u64 = 60_000;

/// "now / 5m / 3h / 2d", shared by the All recent event copy.
fn activity_age(at_ms: u64, now_ms: u64) -> String {
    let seconds = now_ms.saturating_sub(at_ms) / 1_000;
    if seconds < 60 {
        return "now".to_string();
    }
    let minutes = seconds / 60;
    if minutes < 60 {
        return format!("{minutes}m");
    }
    let hours = minutes / 60;
    if hours < 24 {
        return format!("{hours}h");
    }
    format!("{}d", hours / 24)
}

/// What wakes the main loop: a terminal event from the reader thread, or a
/// nudge from the live output stream that new bytes are ready to render.
pub enum AppEvent {
    Term(Event),
    MouseMoved,
    ScrollBurst,
    Wake,
}

/// Edge-triggered wakeup for live terminal output.
///
/// Output and keyboard events share the reactor channel. A busy fullscreen
/// agent can close hundreds of synchronized frames while the outer terminal
/// is still drawing one; queuing every close puts those stale redraw nudges
/// ahead of later keystrokes. The dirty VT state is already cumulative, so
/// one outstanding wake is sufficient.
#[derive(Clone)]
pub struct WakeGate {
    sender: mpsc::Sender<AppEvent>,
    queued: std::sync::Arc<std::sync::atomic::AtomicBool>,
}

impl WakeGate {
    fn new(sender: mpsc::Sender<AppEvent>) -> Self {
        Self {
            sender,
            queued: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
        }
    }

    pub fn wake(&self) {
        if self
            .queued
            .compare_exchange(
                false,
                true,
                std::sync::atomic::Ordering::AcqRel,
                std::sync::atomic::Ordering::Acquire,
            )
            .is_ok()
            && self.sender.send(AppEvent::Wake).is_err()
        {
            self.queued
                .store(false, std::sync::atomic::Ordering::Release);
        }
    }

    fn consumed(&self) {
        self.queued
            .store(false, std::sync::atomic::Ordering::Release);
    }
}

/// Last-value gate for terminal any-motion reports.
///
/// Crossterm enables DEC 1003 as part of mouse capture, so merely moving the
/// pointer can otherwise enqueue hundreds of hover events ahead of typing.
/// Hover and child any-motion handling care about the newest position; drag,
/// press, release, and wheel events remain lossless ordered `Term` events.
#[derive(Clone)]
struct MouseMotionGate {
    sender: mpsc::Sender<AppEvent>,
    state: std::sync::Arc<std::sync::Mutex<MouseMotionState>>,
}

#[derive(Default)]
struct MouseMotionState {
    queued: bool,
    latest: Option<MouseEvent>,
}

impl MouseMotionGate {
    fn new(sender: mpsc::Sender<AppEvent>) -> Self {
        Self {
            sender,
            state: std::sync::Arc::new(std::sync::Mutex::new(MouseMotionState::default())),
        }
    }

    fn push(&self, mouse: MouseEvent) -> bool {
        let should_notify = {
            let mut state = self.state.lock().unwrap();
            state.latest = Some(mouse);
            if state.queued {
                false
            } else {
                state.queued = true;
                true
            }
        };
        if should_notify && self.sender.send(AppEvent::MouseMoved).is_err() {
            self.state.lock().unwrap().queued = false;
            return false;
        }
        true
    }

    fn take(&self) -> Option<MouseEvent> {
        let mut state = self.state.lock().unwrap();
        state.queued = false;
        state.latest.take()
    }
}

#[derive(Clone)]
struct ScrollGate {
    sender: mpsc::Sender<AppEvent>,
    state: std::sync::Arc<std::sync::Mutex<ScrollState>>,
}

#[derive(Default)]
struct ScrollState {
    runs: std::collections::VecDeque<ScrollRun>,
    mergeable: bool,
}

#[derive(Clone, Copy)]
struct ScrollRun {
    event: MouseEvent,
    count: usize,
}

impl ScrollRun {
    fn matches(&self, event: &MouseEvent) -> bool {
        self.event.kind == event.kind
            && self.event.column == event.column
            && self.event.row == event.row
            && self.event.modifiers == event.modifiers
    }
}

impl ScrollGate {
    fn new(sender: mpsc::Sender<AppEvent>) -> Self {
        Self {
            sender,
            state: std::sync::Arc::new(std::sync::Mutex::new(ScrollState::default())),
        }
    }

    fn push(&self, event: MouseEvent) -> bool {
        let should_notify = {
            let mut state = self.state.lock().unwrap();
            if state.mergeable && state.runs.back().is_some_and(|run| run.matches(&event)) {
                state.runs.back_mut().unwrap().count += 1;
                false
            } else {
                state.runs.push_back(ScrollRun { event, count: 1 });
                state.mergeable = true;
                true
            }
        };
        !should_notify || self.sender.send(AppEvent::ScrollBurst).is_ok()
    }

    /// A key, paste, resize, click, or drag is an ordering barrier: scrolling
    /// read after it must get a new marker behind it in the reactor FIFO.
    fn seal(&self) {
        self.state.lock().unwrap().mergeable = false;
    }

    fn take(&self) -> Option<ScrollRun> {
        let mut state = self.state.lock().unwrap();
        let run = state.runs.pop_front();
        if state.runs.is_empty() {
            state.mergeable = false;
        }
        run
    }
}

/// Lazily drain the currently queued event batch without prefetching.
///
/// Keeping this lazy is load-bearing: if the UI reaches its time budget,
/// every event not yet processed must remain in the channel for the next
/// frame. Prefetching after the last processed event drops that event when
/// the iterator is abandoned.
fn queued_app_events(
    first: AppEvent,
    events: &mpsc::Receiver<AppEvent>,
) -> impl Iterator<Item = AppEvent> + '_ {
    std::iter::once(first).chain(events.try_iter())
}

fn terminal_event_is_priority(event: &Event) -> bool {
    matches!(
        event,
        Event::Key(key) if key.kind != KeyEventKind::Release
    ) || matches!(event, Event::Paste(_))
}

#[cfg(test)]
mod event_drain_tests {
    use super::*;

    #[test]
    fn output_wake_gate_keeps_only_one_notification_pending() {
        let (tx, rx) = mpsc::channel();
        let wake = WakeGate::new(tx);
        for _ in 0..10_000 {
            wake.wake();
        }

        assert!(matches!(rx.try_recv(), Ok(AppEvent::Wake)));
        assert!(matches!(rx.try_recv(), Err(mpsc::TryRecvError::Empty)));

        wake.consumed();
        wake.wake();
        assert!(matches!(rx.try_recv(), Ok(AppEvent::Wake)));
    }

    #[test]
    fn mouse_motion_gate_keeps_only_the_latest_position() {
        let (tx, rx) = mpsc::channel();
        let motion = MouseMotionGate::new(tx);
        for column in 0..1_000u16 {
            assert!(motion.push(MouseEvent {
                kind: MouseEventKind::Moved,
                column,
                row: 7,
                modifiers: ratatui::crossterm::event::KeyModifiers::NONE,
            }));
        }

        assert!(matches!(rx.try_recv(), Ok(AppEvent::MouseMoved)));
        assert!(matches!(rx.try_recv(), Err(mpsc::TryRecvError::Empty)));
        let latest = motion.take().unwrap();
        assert_eq!((latest.column, latest.row), (999, 7));
    }

    #[test]
    fn scroll_gate_coalesces_runs_but_preserves_key_barriers() {
        let (tx, rx) = mpsc::channel();
        let scroll = ScrollGate::new(tx.clone());
        let wheel = MouseEvent {
            kind: MouseEventKind::ScrollDown,
            column: 80,
            row: 12,
            modifiers: ratatui::crossterm::event::KeyModifiers::NONE,
        };
        for _ in 0..1_000 {
            assert!(scroll.push(wheel));
        }
        scroll.seal();
        tx.send(AppEvent::Term(Event::Key(KeyEvent::new(
            KeyCode::Char('x'),
            ratatui::crossterm::event::KeyModifiers::NONE,
        ))))
        .unwrap();
        assert!(scroll.push(wheel));

        assert!(matches!(rx.try_recv(), Ok(AppEvent::ScrollBurst)));
        assert_eq!(scroll.take().unwrap().count, 1_000);
        assert!(matches!(
            rx.try_recv(),
            Ok(AppEvent::Term(Event::Key(KeyEvent {
                code: KeyCode::Char('x'),
                ..
            })))
        ));
        assert!(matches!(rx.try_recv(), Ok(AppEvent::ScrollBurst)));
        assert_eq!(scroll.take().unwrap().count, 1);
    }

    #[test]
    fn event_batch_leaves_the_first_overflow_keystroke_queued() {
        let (tx, rx) = mpsc::channel();
        const FRAME_LIMIT: usize = 256;
        for _ in 0..FRAME_LIMIT {
            tx.send(AppEvent::Wake).unwrap();
        }
        tx.send(AppEvent::Term(Event::Key(KeyEvent::new(
            KeyCode::Char('x'),
            ratatui::crossterm::event::KeyModifiers::NONE,
        ))))
        .unwrap();

        let first = rx.recv().unwrap();
        assert_eq!(
            queued_app_events(first, &rx).take(FRAME_LIMIT).count(),
            FRAME_LIMIT
        );
        assert!(matches!(
            rx.try_recv(),
            Ok(AppEvent::Term(Event::Key(KeyEvent {
                code: KeyCode::Char('x'),
                ..
            })))
        ));
        assert!(matches!(rx.try_recv(), Err(mpsc::TryRecvError::Empty)));
    }
}

/// Last rendered terminal width, so mouse hit-testing can reason about
/// right-aligned chrome without threading the frame size everywhere.
pub static LAST_TERM_WIDTH: std::sync::atomic::AtomicU16 = std::sync::atomic::AtomicU16::new(0);
const RESCAN_INTERVAL: Duration = Duration::from_millis(1_000);
/// Match the native frontend's cross-process Resume Agent settle window.
/// A departing provider can emit its ordinary Stop while the Session Host is
/// still terminating it; irreversible completion effects wait for the Host's
/// replacement runtime generation to either advance or remain stable.
const DEFERRED_STOP_EFFECT_DELAY: Duration = Duration::from_secs(3);

#[derive(Clone, Copy, Debug)]
pub enum Verb {
    Stop,
    /// Recreate an exited hosted terminal and resume its saved launch.
    Resume,
    /// Resume a returned managed runtime inside the existing hosted terminal.
    ResumeAgent,
    Remove,
    Pin(bool),
}

/// A verb the user fired that hasn't reported back yet — drives the
/// spinner in the status bar so `s` (which stops a host and waits on its
/// reaper) doesn't look like nothing happened.
pub struct InFlight {
    pub label: String,
}

pub struct VerbOutcome {
    pub message: String,
    /// Session to select once it appears in the model (new/restarted ids).
    pub select: Option<String>,
    /// A replacement Resume was definitively rejected before it could mint a
    /// new Session. Clear only the matching exact-selection intent; an
    /// unresolved bridge response may already have applied and deliberately
    /// leaves this unset.
    pub replacement_not_applied: Option<String>,
    /// Text the main thread should publish to the controller terminal's
    /// clipboard via OSC 52. Terminal output stays on the render thread so
    /// an async transcript read cannot interleave with a ratatui frame.
    pub clipboard: Option<String>,
}

const LOCAL_REPLACEMENT_RESCAN_OBSERVATIONS: u8 = 30;

#[derive(Default)]
struct LocalReplacementSelectionState {
    pending: Option<PendingLocalReplacementSelection>,
    /// Ambiguity or expiry clears `pending` but keeps default selection
    /// suppressed. Only an explicit user choice may end that fail-closed
    /// state; otherwise an unrelated first row could steal focus later.
    suppress_default: bool,
}

impl LocalReplacementSelectionState {
    fn begin(&mut self, pending: PendingLocalReplacementSelection) {
        self.pending = Some(pending);
        self.suppress_default = true;
    }

    fn clear(&mut self) {
        self.pending = None;
        self.suppress_default = false;
    }

    fn clear_if_source(&mut self, source_id: &str) {
        if self
            .pending
            .as_ref()
            .is_some_and(|pending| pending.source_id == source_id)
        {
            self.clear();
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct PendingLocalReplacementSelection {
    source_id: String,
    group_id: String,
    created_at: u64,
    runtime_id: Option<String>,
    cwd: Option<String>,
    baseline_session_ids: HashSet<String>,
    rescans_remaining: u8,
}

enum LocalReplacementSelectionResolution {
    Wait(PendingLocalReplacementSelection),
    Select(String),
    Cancel,
}

/// A fresh install has nothing configured. Rather than showing an empty
/// sidebar, offer what the machine already tells us: builtin presets for
/// the CLIs actually installed, and the directories existing sessions ran
/// in as candidate projects.
pub struct FirstRun {
    pub presets: Vec<unpeel_core::first_run::SeedPreset>,
    pub projects: Vec<unpeel_core::first_run::SuggestedProject>,
    pub accepted: Vec<bool>,
    pub row: usize,
}

/// The add-project dialog: a path being typed plus the directories it
/// could complete to. Starts at the home directory rather than an empty
/// line, since a project is always somewhere under it.
pub struct ProjectInput {
    pub query: String,
    pub matches: Vec<String>,
    pub selected: usize,
    /// Last clicked completion row, for double-click-to-descend.
    pub last_click: Option<(usize, Instant)>,
}

impl ProjectInput {
    pub fn new() -> Self {
        let home = std::env::var("HOME").unwrap_or_else(|_| "/".into());
        let mut input = ProjectInput {
            query: format!("{home}/"),
            matches: Vec::new(),
            selected: 0,
            last_click: None,
        };
        input.refresh();
        input
    }

    /// Directories under the typed path's parent that match its last
    /// segment — the completion list.
    pub fn refresh(&mut self) {
        let (dir, prefix) = match self.query.rfind('/') {
            Some(cut) => (
                self.query[..=cut].to_string(),
                self.query[cut + 1..].to_string(),
            ),
            None => ("./".to_string(), self.query.clone()),
        };
        let mut matches: Vec<String> = std::fs::read_dir(&dir)
            .into_iter()
            .flatten()
            .flatten()
            .filter(|entry| entry.path().is_dir())
            .filter_map(|entry| {
                let name = entry.file_name().to_string_lossy().into_owned();
                // Hidden dirs only when the user starts typing a dot.
                if name.starts_with('.') && !prefix.starts_with('.') {
                    return None;
                }
                name.to_lowercase()
                    .starts_with(&prefix.to_lowercase())
                    .then(|| format!("{dir}{name}"))
            })
            .collect();
        matches.sort();
        matches.truncate(200);
        self.matches = matches;
        self.selected = 0;
        // The indices a pending double-click refers to just changed.
        self.last_click = None;
    }

    /// Adopt the highlighted completion, leaving the trailing slash so the
    /// next keystroke keeps descending.
    pub fn complete(&mut self) {
        if let Some(pick) = self.matches.get(self.selected).cloned() {
            self.query = format!("{pick}/");
            self.refresh();
        }
    }

    pub fn expanded(&self) -> String {
        let trimmed = self.query.trim().trim_end_matches('/');
        if let Some(rest) = trimmed.strip_prefix("~/") {
            format!("{}/{rest}", std::env::var("HOME").unwrap_or_default())
        } else {
            trimmed.to_string()
        }
    }
}

impl Default for ProjectInput {
    fn default() -> Self {
        Self::new()
    }
}

/// Editing state for the rename dialog. Positions are character indices,
/// not byte offsets, so mouse selection and deletion cannot split UTF-8.
pub struct RenameInput {
    pub session_id: String,
    pub buffer: String,
    pub cursor: usize,
    pub selection_anchor: Option<usize>,
    pub dragging: bool,
}

impl RenameInput {
    pub fn new(session_id: String, buffer: String) -> Self {
        let cursor = buffer.chars().count();
        Self {
            session_id,
            buffer,
            cursor,
            selection_anchor: None,
            dragging: false,
        }
    }

    pub fn len(&self) -> usize {
        self.buffer.chars().count()
    }

    pub fn is_empty(&self) -> bool {
        self.buffer.is_empty()
    }

    pub fn selection(&self) -> Option<(usize, usize)> {
        let anchor = self.selection_anchor?;
        (anchor != self.cursor).then_some((anchor.min(self.cursor), anchor.max(self.cursor)))
    }

    fn byte_index(&self, char_index: usize) -> usize {
        self.buffer
            .char_indices()
            .nth(char_index.min(self.len()))
            .map(|(byte, _)| byte)
            .unwrap_or(self.buffer.len())
    }

    fn delete_selection(&mut self) -> bool {
        let Some((start, end)) = self.selection() else {
            self.selection_anchor = None;
            return false;
        };
        let start_byte = self.byte_index(start);
        let end_byte = self.byte_index(end);
        self.buffer.replace_range(start_byte..end_byte, "");
        self.cursor = start;
        self.selection_anchor = None;
        true
    }

    pub fn insert(&mut self, text: &str) {
        self.delete_selection();
        // A session title is one logical field. Pasted line breaks become
        // spaces; visual lines come from wrapping, not embedded newlines.
        let text = text.replace(['\r', '\n'], " ");
        let byte = self.byte_index(self.cursor);
        self.buffer.insert_str(byte, &text);
        self.cursor += text.chars().count();
        self.selection_anchor = None;
    }

    pub fn backspace(&mut self) {
        if self.delete_selection() || self.cursor == 0 {
            return;
        }
        let end = self.byte_index(self.cursor);
        let start = self.byte_index(self.cursor - 1);
        self.buffer.replace_range(start..end, "");
        self.cursor -= 1;
    }

    pub fn delete_forward(&mut self) {
        if self.delete_selection() || self.cursor >= self.len() {
            return;
        }
        let start = self.byte_index(self.cursor);
        let end = self.byte_index(self.cursor + 1);
        self.buffer.replace_range(start..end, "");
    }

    pub fn move_to(&mut self, position: usize, extend_selection: bool) {
        let position = position.min(self.len());
        if extend_selection {
            self.selection_anchor.get_or_insert(self.cursor);
        } else {
            self.selection_anchor = None;
        }
        self.cursor = position;
    }

    pub fn move_left(&mut self, extend_selection: bool) {
        if !extend_selection {
            if let Some((start, _)) = self.selection() {
                self.move_to(start, false);
                return;
            }
        }
        self.move_to(self.cursor.saturating_sub(1), extend_selection);
    }

    pub fn move_right(&mut self, extend_selection: bool) {
        if !extend_selection {
            if let Some((_, end)) = self.selection() {
                self.move_to(end, false);
                return;
            }
        }
        self.move_to((self.cursor + 1).min(self.len()), extend_selection);
    }

    pub fn begin_mouse_selection(&mut self, position: usize) {
        let position = position.min(self.len());
        self.cursor = position;
        self.selection_anchor = Some(position);
        self.dragging = true;
    }

    pub fn drag_mouse_selection(&mut self, position: usize) {
        if self.dragging {
            self.cursor = position.min(self.len());
        }
    }

    pub fn finish_mouse_selection(&mut self, position: usize) {
        self.drag_mouse_selection(position);
        self.dragging = false;
        if self.selection_anchor == Some(self.cursor) {
            self.selection_anchor = None;
        }
    }
}

pub struct Confirm {
    pub verb: Verb,
    pub session_id: String,
    pub grid: (u16, u16),
    pub prompt: String,
}

/// A pressed inline child folder that may become a sibling reorder. Like a
/// project-header drag, a press that never moves remains a fold/unfold click.
pub struct FolderDrag {
    pub project_id: String,
    pub parent_id: String,
    pub start: usize,
    pub drop_pos: usize,
}

/// What a context-menu row does. Labels live beside the action in the item
/// builders, so a row's index can never dispatch the wrong verb.
#[derive(Clone, PartialEq)]
pub enum CtxAction {
    // Project rows.
    NewSession,
    StopAll,
    Archived,
    ToggleCollapse,
    /// Fold/unfold a worktree folder row (keyed by its project id, unlike
    /// project headers which fold by name).
    ToggleWorktreeFold,
    Reveal,
    /// Opens the color submenu — same popup, new rows.
    FolderColor,
    /// Paint the project's chevron; None restores the default.
    SetColor(Option<&'static str>),
    /// Opens the sort submenu — same popup, new rows.
    SortSessions,
    /// Date sort (recently updated first) when true; custom (manual) order
    /// when false.
    SetSessionSort(bool),
    /// Opens the "New group…" name prompt for this project.
    NewGroup,
    /// Opens the name prompt for an existing plain group.
    RenameGroup,
    /// Plain groups archive their contents into the parent before their
    /// project record is removed. The parent id travels with the action so
    /// the confirmation cannot target a different row after a rescan.
    RemoveGroup(String),
    RemoveGroupConfirmed(String),
    /// Swaps the rows for an inline confirm, like the desktop's row-swap.
    RemoveProject,
    RemoveProjectConfirmed,
    CloseMenu,
    // Session rows.
    RenameSession,
    TogglePin,
    /// Opens the "Move to" submenu — same popup, new rows.
    MoveTo,
    /// Move the session under this group/worktree project via the
    /// `project-override.json` marker; None clears the marker (back to the
    /// manifest project).
    MoveToProject(Option<String>),
    RestartSession,
    /// Opens the transcript range submenu.
    CopyTranscript,
    /// Copy this many recent entries; zero means the whole conversation.
    CopyTranscriptEntries(usize),
    /// Copy the session's Unpeel id to the controlling terminal's clipboard.
    CopySessionId,
    StopSession,
    /// Archive a stopped session (Stop already archives a live one).
    ArchiveSession,
    RemoveSession,
}

/// The desktop's folder palette: rawValue + menu label. The rendering
/// hexes live beside the swatch drawing in ui.rs.
pub const FOLDER_COLORS: &[(&str, &str)] = &[
    ("sky", "Sky"),
    ("blue", "Blue"),
    ("violet", "Violet"),
    ("rose", "Rose"),
    ("amber", "Amber"),
    ("moss", "Moss"),
    ("teal", "Teal"),
    ("graphite", "Graphite"),
];

/// A right-clicked sidebar row (herdr-style): a popup anchored at the
/// click, items built for the row's state at open time. Only the anchor is
/// stored — the rect is derived per frame, so edge clamping and terminal
/// resizes are automatic.
pub struct ContextMenu {
    /// Popup title: the project or session the rows act on.
    pub title: String,
    /// The project the rows act on (a session menu carries its group).
    pub project_id: String,
    /// The project's header name — the collapse key.
    pub name: String,
    /// The right-clicked session, for session rows.
    pub session_id: Option<String>,
    pub anchor: (u16, u16),
    pub selected: usize,
    pub items: Vec<(String, CtxAction)>,
}

/// The one floating layer. Being a single enum is the point: two modals
/// can never be open at once, and a new one must be added here and handled
/// in `draw_overlays` and `handle_key` or the compiler objects — the shape
/// of bug where a dialog opened but silently never rendered (the settings
/// early-return, 2026-08-07) cannot come back.
///
/// `Pairing` is the one passive member: it only ever swallows the keys that
/// close it, everything else behaves as if it weren't there.
pub enum Modal {
    Help,
    FirstRun(FirstRun),
    /// Command palette (⌘K equivalent).
    Palette {
        query: String,
        selected: usize,
    },
    /// Activity popover dropped from the sidebar's top-right spinner/bell.
    /// Its action rows are every active job, every unread settled job, then
    /// the always-present "All recent" destination.
    Activity {
        selected: usize,
    },
    /// '+' add-project dialog.
    ProjectInput(ProjectInput),
    /// Wrapped rename editor. Enter commits via the app bridge when it's up.
    Rename(RenameInput),
    /// "New group…" name prompt: a child project (is_folder, no worktree)
    /// under `project_id`, created straight into app-state.json.
    GroupInput {
        project_id: String,
        buffer: String,
    },
    /// Rename a plain group record in app-state.json.
    GroupRename {
        project_id: String,
        buffer: String,
    },
    /// New-session preset picker: (label, command) rows + selection. Mouse
    /// `+` entry points carry an anchor and render as a dropdown; keyboard
    /// entry points leave it `None` and keep the centered dialog.
    PresetPicker {
        presets: Vec<(String, String)>,
        selected: usize,
        target: String,
        anchor: Option<(u16, u16)>,
    },
    /// Rendered QR + code text while a pairing window is open.
    Pairing {
        lines: Vec<String>,
        code: String,
    },
    /// The sidebar footer menu (herdr-style): a short list that links into
    /// Settings, the keybindings help, and the command palette. Opened by
    /// clicking "menu" on the sidebar's bottom edge.
    Menu {
        selected: usize,
    },
    /// Local sites the selected session's project serves (host-probed live
    /// loopback URLs), dropped down from the preview's top-right chip: open
    /// rows plus Stop rows for the session-owned servers. A single URL
    /// left-click opens directly; right-click always drops the menu.
    LocalUrls {
        rows: Vec<LocalUrlRow>,
        selected: usize,
    },
    /// Right-click on a project header or session row: the desktop's
    /// context menus, the parts of them that exist here.
    Context(ContextMenu),
}

/// One row of the local-sites dropdown. Stop rows exist only for servers
/// resolved (at menu-open time) to a hosted session's process tree —
/// Unpeel never offers to kill infrastructure it didn't start.
#[derive(Clone, Debug)]
pub enum LocalUrlRow {
    Open(String),
    Stop { url: String, label: String },
}

/// One actionable session in the activity dropdown. Owned strings keep the
/// renderer and mouse hit-testing independent from the live sidebar borrow;
/// the list is cheap (active + unread only) and rebuilt from canonical state.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ActivityMenuEntry {
    pub session_id: String,
    pub title: String,
    pub project: String,
    pub command: String,
    pub working: bool,
    pub unread: bool,
}

/// One row on the app-wide All recent page. Log snapshots remain renderable
/// after a Session is removed; `session_id` is therefore absent for a row
/// that can no longer be opened.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RecentActivityEntry {
    pub session_id: Option<String>,
    pub title: String,
    pub project: String,
    pub event: String,
    pub command: String,
    pub working: bool,
    pub unread: bool,
    pub at: u64,
}

impl LocalUrlRow {
    pub fn label(&self) -> String {
        match self {
            LocalUrlRow::Open(url) => url.clone(),
            LocalUrlRow::Stop { label, .. } => format!("stop {label}"),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MenuAction {
    OpenSettings,
    OpenKeybindings,
    OpenCommandPalette,
    Exit,
}

/// One footer-menu row. Keeping its action beside the visible label and
/// shortcut means adding or reordering rows cannot silently change what a
/// click opens.
#[derive(Clone, Copy)]
pub struct MenuItem {
    pub label: &'static str,
    pub shortcut: &'static str,
    pub action: MenuAction,
}

/// Rows of the footer menu, in order. The shortcuts are the same app-wide
/// bindings documented by the keybindings overlay.
pub const MENU_ITEMS: &[MenuItem] = &[
    MenuItem {
        label: "Settings",
        shortcut: ",",
        action: MenuAction::OpenSettings,
    },
    MenuItem {
        label: "Keybindings",
        shortcut: "?",
        action: MenuAction::OpenKeybindings,
    },
    MenuItem {
        label: "Command Palette",
        shortcut: "ctrl+k",
        action: MenuAction::OpenCommandPalette,
    },
    MenuItem {
        label: "Exit",
        shortcut: "q",
        action: MenuAction::Exit,
    },
];

/// A Herdr-style selection over the terminal grid. The snapshot is frozen
/// for the short drag so live output cannot move the text out from under the
/// pointer between mouse-down and mouse-up.
#[derive(Clone, Debug)]
pub struct TerminalSelection {
    pub session_id: String,
    pub snapshot: TerminalViewportSnapshot,
    anchor: (usize, u16),
    cursor: (usize, u16),
    dragging: bool,
    mouse_down: bool,
}

impl TerminalSelection {
    fn anchor(
        session_id: String,
        snapshot: TerminalViewportSnapshot,
        row: usize,
        col: u16,
    ) -> Self {
        let point = (
            row.min(snapshot.viewport_rows.len().saturating_sub(1)),
            col.min(snapshot.cols.saturating_sub(1)),
        );
        Self {
            session_id,
            snapshot,
            anchor: point,
            cursor: point,
            dragging: false,
            mouse_down: true,
        }
    }

    fn drag(&mut self, row: usize, col: u16) {
        self.cursor = (
            row.min(self.snapshot.viewport_rows.len().saturating_sub(1)),
            col.min(self.snapshot.cols.saturating_sub(1)),
        );
        self.dragging |= self.cursor != self.anchor;
    }

    fn ordered(&self) -> ((usize, u16), (usize, u16)) {
        if self.anchor <= self.cursor {
            (self.anchor, self.cursor)
        } else {
            (self.cursor, self.anchor)
        }
    }

    fn finish(&mut self) {
        self.mouse_down = false;
    }

    pub fn uses_frozen_snapshot(&self) -> bool {
        self.mouse_down
    }

    pub fn row_range(&self, row: usize) -> Option<(u16, u16)> {
        if !self.dragging {
            return None;
        }
        let ((start_row, start_col), (end_row, end_col)) = self.ordered();
        if row < start_row || row > end_row {
            return None;
        }
        Some(if start_row == end_row {
            (start_col, end_col)
        } else if row == start_row {
            (start_col, self.snapshot.cols.saturating_sub(1))
        } else if row == end_row {
            (0, end_col)
        } else {
            (0, self.snapshot.cols.saturating_sub(1))
        })
    }

    fn selected_text(&self) -> Option<String> {
        if !self.dragging {
            return None;
        }
        let ((start_row, start_col), (end_row, end_col)) = self.ordered();
        let mut result = String::new();
        let mut previous_wrapped = false;
        for row in start_row..=end_row {
            let source = self.snapshot.viewport_rows.get(row)?;
            let start = if row == start_row { start_col } else { 0 };
            let end = if row == end_row {
                end_col
            } else {
                self.snapshot.cols.saturating_sub(1)
            };
            if row > start_row && !previous_wrapped {
                result.push('\n');
            }
            result.push_str(text_in_cell_range(&source.text, start, end).trim_end());
            previous_wrapped = source.wrapped;
        }
        Some(result)
    }

    /// Turn a visual selection on the live cursor row into conservative
    /// line-editor input. Moving to the selection's right edge and emitting
    /// one Backspace per grapheme gives prompt editors the same useful
    /// "select, then delete" behavior as Claude's own composer without ever
    /// treating transcript rows as editable input.
    fn backspace_edit_sequence(&self) -> Option<String> {
        if !self.dragging {
            return None;
        }
        let ((start_row, start_col), (end_row, end_col)) = self.ordered();
        if start_row != end_row || start_row != self.snapshot.cursor_row as usize {
            return None;
        }
        let row = self.snapshot.viewport_rows.get(start_row)?;
        let mut cell = 0usize;
        let mut grapheme_index = 0usize;
        let mut selected_start = None;
        let mut selected_end = None;
        let mut cursor_index = 0usize;
        for grapheme in row.text.graphemes(true) {
            let width = UnicodeWidthStr::width(grapheme);
            if width == 0 {
                continue;
            }
            let next = cell.saturating_add(width);
            if next <= self.snapshot.cursor_col as usize {
                cursor_index = grapheme_index + 1;
            }
            if next > start_col as usize && cell <= end_col as usize {
                selected_start.get_or_insert(grapheme_index);
                selected_end = Some(grapheme_index + 1);
            }
            cell = next;
            grapheme_index += 1;
            if cell > end_col as usize && cell >= self.snapshot.cursor_col as usize {
                break;
            }
        }
        let selected_start = selected_start?;
        let selected_end = selected_end?;
        let delete_count = selected_end.saturating_sub(selected_start);
        if delete_count == 0 {
            return None;
        }

        let mut sequence = String::new();
        if cursor_index < selected_end {
            sequence.push_str(&"\x1b[C".repeat(selected_end - cursor_index));
        } else {
            sequence.push_str(&"\x1b[D".repeat(cursor_index - selected_end));
        }
        sequence.push_str(&"\x7f".repeat(delete_count));
        Some(sequence)
    }
}

fn text_in_cell_range(text: &str, start: u16, end: u16) -> String {
    let mut result = String::new();
    let mut cell = 0usize;
    for grapheme in text.graphemes(true) {
        let width = UnicodeWidthStr::width(grapheme);
        if width == 0 {
            if !result.is_empty() {
                result.push_str(grapheme);
            }
            continue;
        }
        let next = cell.saturating_add(width);
        if next > start as usize && cell <= end as usize {
            result.push_str(grapheme);
        }
        cell = next;
        if cell > end as usize {
            break;
        }
    }
    result
}

pub struct App {
    pub model: SidebarModel,
    pub engine: ActivityEngine,
    /// Stamp-gated decode caches for the 1s sidebar rescan.
    pub scan_cache: ScanCache,
    pub selected_id: Option<String>,
    /// When a project's archive library is open (`a` or the context
    /// menu's "Archived (N)"), the group whose archive the preview lists
    /// (and the row inside it that's highlighted).
    pub selected_archive: Option<(String, usize)>,
    /// Selected row while the app-wide All recent page replaces the terminal
    /// preview. `None` means the page is closed; the value is the actionable
    /// entry index (section labels are not selectable).
    pub selected_recent: Option<usize>,
    /// Native-compatible persisted activity history. The TUI refreshes the
    /// same JSONL feed the app owns, and appends lifecycle edges only while
    /// it is the standalone frontend.
    pub activity_log: unpeel_core::activity_log::ActivityLogStore,
    /// Live Stop state is immediate; Finished/history/unread waits briefly
    /// for a possible in-place runtime-generation edge.
    deferred_stop_effects: HashMap<String, DeferredStopEffects>,
    /// Runtime generations aligned with `model`'s last completed local scan.
    /// The live hook path may advance the ActivityEngine ahead of that scan,
    /// so keeping this separately lets the next model transition suppress a
    /// synthetic Busy→Idle completion caused only by Resume Agent.
    model_runtime_generations: HashMap<String, u64>,
    /// Where the open preset picker should spawn: (project id, cwd). Set
    /// when the picker is opened from a "+ New session" row, which has no
    /// selected session to borrow a destination from.
    pub pending_spawn_target: Option<(String, String)>,
    /// The selection the viewport was last scrolled to follow. See
    /// `clamp_scroll` — this is what lets the wheel scroll freely.
    last_selection_key: String,
    /// The "+ New session" row of an empty project, by project id.
    pub selected_new_session: Option<String>,
    /// The "+ Add project" row at the foot of the tree.
    pub selected_add_project: bool,
    /// A project header under the mouse: (name, row pressed, row now).
    /// Separate from `dragging_row` because it reorders the tree, not a
    /// block of sessions inside one project. Keeping the press row is what
    /// distinguishes a drag from a click — a header that collapsed the
    /// moment you grabbed it could never be dragged anywhere.
    pub dragging_project: Option<(String, usize, usize)>,
    /// Same press/drag/release gesture for group + worktree siblings.
    pub dragging_folder: Option<FolderDrag>,
    /// Archive search box (the desktop's archive filter).
    pub archive_query: String,
    /// The open floating layer, if any — see `Modal`.
    pub modal: Option<Modal>,
    /// A menu click cannot return directly from the event loop. It sets this
    /// latch so the loop takes the same clean terminal-restoration path as q.
    pub exit_requested: bool,
    /// Highlighted Git worktree folder row, by the worktree's project id.
    /// Plain organizational groups are structural headers, not selections.
    /// ⏎ (or a click) toggles it open — see `expanded_worktrees`.
    pub selected_worktree_folder: Option<String>,
    /// Worktree folder rows the user has opened, by worktree project id.
    /// In-memory only, like `collapsed` — folders default collapsed on
    /// every launch. Expansion is pure visibility (`visible_items`); the
    /// model always carries the sessions.
    pub expanded_worktrees: HashSet<String>,
    pub sidebar_scroll: usize,
    pub sidebar_width: u16,
    /// Last reported mouse position (col, row), from motion events. Drives
    /// the hover "+" on project headers; terminals that never report motion
    /// simply leave it None (the click zone works regardless).
    pub mouse_pos: Option<(u16, u16)>,
    pub dragging_divider: bool,
    pub collapsed: HashSet<String>,
    pub confirm: Option<Confirm>,
    pub info: Option<String>,
    /// Display-layer verdicts for detected local-site URLs, keyed by URL:
    /// (openable, checked-at). Session hosts are long-lived processes running
    /// whatever detection code they started with, so the chip re-verifies
    /// every manifest URL against the CURRENT probe rules before showing it.
    /// Checks run on throwaway threads; the 1s tick repaints with results.
    pub local_url_verdicts: std::sync::Arc<
        std::sync::Mutex<std::collections::HashMap<String, (bool, std::time::Instant)>>,
    >,
    /// URLs with a probe thread currently running (dedupe guard).
    pub local_url_checks_in_flight: std::sync::Arc<std::sync::Mutex<HashSet<String>>>,
    /// Local-site URLs already announced with a toast; cleared when the
    /// site drops out so a dev-server restart announces again.
    pub announced_local_urls: HashSet<String>,
    /// A newer published CLI version (update.rs). Renders as a persistent
    /// top-right toast when no transient `info` toast is up; a click
    /// dismisses it for that version (persisted, so it never re-toasts).
    pub update_available: Option<String>,
    /// One-time tip about the environment the TUI runs in (today: the Herdr
    /// right-click passthrough tip, herdr.rs). Waits behind the transient
    /// and update toasts; a click writes the marker and dismisses for good.
    pub env_hint: Option<EnvHint>,
    pub hook_port: Option<u16>,
    /// Latest app-computed sidebar payload from the background poller; None
    /// means the app is unreachable and the disk model is in use.
    pub sidebar_feed: std::sync::Arc<std::sync::Mutex<Option<Result<serde_json::Value, String>>>>,
    /// Why the sidebar is disk-derived (shown dim in the bar; empty = live).
    pub feed_note: &'static str,
    pub bridge_mode: bool,
    /// Additive native capability proving MobileRemoteServer can wait for and
    /// claim the exact persisted Direct endpoint without rewriting it.
    pub bridge_mobile_endpoint_handoff: bool,
    /// A positively identified released native app: `/mcp/sidebar` is absent
    /// but the same hook port answered the legacy `/mcp/list-presets` probe.
    pub legacy_bridge_mode: bool,
    /// A hook port accepted the sidebar request but did not answer within the
    /// short poll window. Native may be MainActor-blocked, so cleanup and Link
    /// authority fail closed until the candidate resolves.
    pub bridge_unresolved: bool,
    /// Preview scrollback offset in rows (0 = live tail).
    pub preview_scroll: u32,
    /// True while the preview pane owns the keyboard: keys forward to the
    /// session PTY, which is resized to the pane (herdr-style attach).
    pub terminal_focus: bool,
    /// Plain drag selection for terminals that have not enabled mouse
    /// reporting. Mouse capture stays on and terminal focus stays active.
    pub terminal_selection: Option<TerminalSelection>,
    /// App-parity persistent input stream. Sending is non-blocking so key and
    /// trackpad bursts never perform host round trips on the render thread.
    input: control::InteractiveInput,
    /// Sessions this TUI saw settle while unselected. Merged with the
    /// shared `read.json` receipts (and the app's own unread when the
    /// bridge is up) so every frontend agrees on the blue dot.
    pub local_unread: HashSet<String>,
    /// The authoritative unread set for this frame: what any frontend
    /// claims, minus anything a read receipt already covers.
    pub unread_ids: HashSet<String>,
    /// Read-only snapshot of the app's UserDefaults overlay (pins, manual
    /// order, titles, archived) for the disk-fallback sidebar.
    pub overlay: Option<overlay::NativeOverlay>,
    pub overlay_loaded_at: Option<Instant>,
    /// Select this session as soon as it appears (freshly spawned ids).
    pub pending_select: Option<String>,
    /// The native bridge's stopped-Session Resume receipt has no replacement
    /// id. Correlate it across bounded sidebar rescans instead of selecting
    /// whichever unrelated Session happens to be listed first.
    replacement_selection: LocalReplacementSelectionState,
    /// Phone-facing snapshot published every rescan; served by mobile.rs.
    pub mobile_snapshot: mobile::SharedSnapshot,
    pub mobile_server: Option<mobile::MobileServer>,
    /// Fail-closed mixed-version lease after released native rewrites
    /// canonical A→fallback B. Link stays native-owned through repair until
    /// the classified hook and fallback listener are both gone.
    pub legacy_mobile_handoff_latched: bool,
    pub legacy_mobile_handoff_classified: bool,
    pub legacy_mobile_fallback_port: Option<u16>,
    pub legacy_mobile_mismatch_observed_at: Option<Instant>,
    /// Off-LAN reach while serving app-lessly: the relay uplink thread.
    /// Same polite-guest lifecycle as the mobile server — the app owns the
    /// relay whenever it runs.
    pub relay_uplink: Option<relay::RelayUplink>,
    /// Runtime fail-closed latch. Deactivation, an authoritative service
    /// rejection, or a relay 401/403 stops Link until a fresh entitlement is
    /// committed; local deletion failures must never let reconcile restart
    /// the old cached bearer in this process.
    pub link_suppressed: bool,
    /// Exact bearer blocked by the fail-closed latch. If the native app later
    /// owns serving and writes a different valid Host-bound cache, standalone
    /// takeover may trust that new authority instead of staying stuck off.
    pub link_blocked_entitlement: Option<String>,
    pub(crate) link_worker: Option<mpsc::Sender<LinkWorkerRequest>>,
    pub link_activation_in_flight: bool,
    pub mobile_resizes: mobile::MobileResizes,
    pub approvals: std::sync::Arc<approvals::ApprovalHub>,
    pub pairing: std::sync::Arc<pairing::PairingWindow>,
    /// Settings overlay: (section index, row index within the section).
    pub settings: Option<(usize, usize)>,
    /// Settings ▸ Presets: the command being typed into the blank add row
    /// at the bottom of the list. Kept across settings visits until ⏎
    /// commits it or esc clears it.
    pub preset_add: String,
    /// Draft for Settings ▸ Remote ▸ Unpeel Link (license key or display
    /// name — the Link surface lives inside the Remote section since
    /// 2026-08-13, desktop parity).
    pub link_input: String,
    /// Selection mode: mouse capture released so the terminal emulator can
    /// drag-select and copy as usual. Keyboard still works.
    pub selection_mode: bool,
    /// Sidebar drag: (session id being dragged, current drop row).
    pub dragging_row: Option<(String, usize)>,
    /// Last left-click on a session row: (id, when). A second click on the
    /// same row within `DOUBLE_CLICK` opens the rename dialog — crossterm
    /// reports only individual presses, so double-click is detected here.
    pub last_click: Option<(String, Instant)>,
    /// Verb in flight, shown with a spinner until its outcome arrives.
    pub in_flight: Option<InFlight>,
    /// Auto-stop-and-archive sweep state: when each session was last seen
    /// ENTERING idle (ms). Any other status clears the entry, so a looping
    /// session resets its clock and never accumulates "inactivity".
    pub idle_since_ms: HashMap<String, u64>,
    /// Sessions currently handed to the background archive worker. An id is
    /// removed only when that worker reports an outcome, preventing duplicate
    /// concurrent stop requests.
    pub auto_archive_issued: HashSet<String>,
    /// Earliest retry time for a transiently failed automatic archive.
    /// Bounded backoff keeps the policy self-healing without request loops.
    pub auto_archive_retry_after_ms: HashMap<String, u64>,
}

/// A one-time tip about the environment the TUI runs in, supplied by
/// whichever integration detected itself at startup (today only Herdr's
/// right-click passthrough tip). Dismissal writes `marker` under the Unpeel
/// home so the tip never re-shows; a new integration adds a supplier, not
/// UI or dismissal plumbing.
pub struct EnvHint {
    pub text: String,
    /// Marker file name (relative to the Unpeel home) recording dismissal.
    pub marker: &'static str,
}

#[derive(Clone, Copy, Debug)]
struct DeferredStopEffects {
    /// Runtime generation visible when the Stop reached the Main loop.
    observed_generation: Option<u64>,
    publish_after: Instant,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DeferredStopResolution {
    Pending,
    Publish,
    Discard,
}

fn runtime_launch_metadata_on_disk(session_id: &str) -> (Option<u64>, Option<u64>) {
    let Some(manifest) = unpeel_core::session_host::load_manifest(session_id) else {
        return (None, None);
    };
    (
        Some(manifest.runtime_launch_generation),
        manifest.runtime_launched_at,
    )
}

fn model_runtime_generations(
    engine: &ActivityEngine,
    model: &SidebarModel,
) -> HashMap<String, u64> {
    model
        .rows
        .iter()
        .filter_map(|row| {
            engine
                .runtime_launch_generation(&row.id)
                .map(|generation| (row.id.clone(), generation))
        })
        .collect()
}

fn runtime_generation_edges(
    previous: &HashMap<String, u64>,
    current: &HashMap<String, u64>,
) -> HashSet<String> {
    current
        .iter()
        .filter_map(|(session_id, generation)| {
            if previous
                .get(session_id)
                .is_some_and(|previous| previous != generation)
            {
                Some(session_id.clone())
            } else {
                None
            }
        })
        .collect()
}

fn should_publish_deferred_stop_effects(
    observed_generation: Option<u64>,
    current_generation: Option<u64>,
) -> bool {
    observed_generation == current_generation
}

fn deferred_stop_resolution(
    deferred: DeferredStopEffects,
    current_generation: Option<u64>,
    now: Instant,
) -> DeferredStopResolution {
    if current_generation.is_some() && current_generation != deferred.observed_generation {
        return DeferredStopResolution::Discard;
    }
    if now < deferred.publish_after {
        return DeferredStopResolution::Pending;
    }
    if should_publish_deferred_stop_effects(deferred.observed_generation, current_generation) {
        DeferredStopResolution::Publish
    } else {
        DeferredStopResolution::Discard
    }
}

fn completed_turn(previous: Option<Status>, current: Status, completion_is_deferred: bool) -> bool {
    !completion_is_deferred
        && current == Status::Idle
        && matches!(
            previous,
            Some(Status::Starting | Status::Busy | Status::Attention)
        )
}

/// A turn completing outside the selected Session earns unread. `Starting`
/// matters here: a fast provider can finish before the first rescan ever
/// observes its `Busy` state, and dropping that edge loses the completion
/// forever. Attention is also active work from the user's point of view and
/// follows the same completion rule as the shared activity log.
fn settled_while_unobserved(
    previous: Option<Status>,
    current: Status,
    is_selected: bool,
    completion_is_deferred: bool,
) -> bool {
    !is_selected && completed_turn(previous, current, completion_is_deferred)
}

#[cfg(test)]
mod unread_transition_tests {
    use std::time::{Duration, Instant};

    use super::{
        completed_turn, deferred_stop_resolution, runtime_generation_edges,
        settled_while_unobserved, should_publish_deferred_stop_effects, DeferredStopEffects,
        DeferredStopResolution, Status,
    };

    #[test]
    fn active_completion_edges_become_unread_when_unobserved() {
        assert!(settled_while_unobserved(
            Some(Status::Busy),
            Status::Idle,
            false,
            false,
        ));
        assert!(settled_while_unobserved(
            Some(Status::Starting),
            Status::Idle,
            false,
            false,
        ));
        assert!(settled_while_unobserved(
            Some(Status::Attention),
            Status::Idle,
            false,
            false,
        ));
    }

    #[test]
    fn selection_and_non_completion_edges_do_not_become_unread() {
        assert!(!settled_while_unobserved(
            Some(Status::Busy),
            Status::Idle,
            true,
            false,
        ));
        assert!(!settled_while_unobserved(
            Some(Status::Idle),
            Status::Idle,
            false,
            false,
        ));
        assert!(!settled_while_unobserved(
            Some(Status::Busy),
            Status::Exited,
            false,
            false,
        ));
    }

    #[test]
    fn deferred_stop_cannot_publish_finished_history_or_local_unread() {
        assert!(!completed_turn(Some(Status::Busy), Status::Idle, true,));
        assert!(!settled_while_unobserved(
            Some(Status::Busy),
            Status::Idle,
            false,
            true,
        ));
    }

    #[test]
    fn deferred_stop_publishes_only_if_runtime_generation_stays_put() {
        assert!(should_publish_deferred_stop_effects(Some(4), Some(4)));
        assert!(!should_publish_deferred_stop_effects(Some(4), Some(5)));
        assert!(should_publish_deferred_stop_effects(None, None));
        assert!(!should_publish_deferred_stop_effects(Some(4), None));

        let now = Instant::now();
        let deferred = DeferredStopEffects {
            observed_generation: Some(4),
            publish_after: now + Duration::from_secs(3),
        };
        assert_eq!(
            deferred_stop_resolution(deferred, Some(4), now),
            DeferredStopResolution::Pending,
        );
        assert_eq!(
            deferred_stop_resolution(deferred, Some(5), now),
            DeferredStopResolution::Discard,
        );
        assert_eq!(
            deferred_stop_resolution(deferred, Some(4), now + Duration::from_secs(3),),
            DeferredStopResolution::Publish,
        );
    }

    #[test]
    fn generation_edge_cannot_create_finished_history_or_unread() {
        let previous = std::collections::HashMap::from([("session".to_string(), 1)]);
        let current = std::collections::HashMap::from([("session".to_string(), 2)]);
        let edges = runtime_generation_edges(&previous, &current);
        let suppress_completion = edges.contains("session");

        assert!(suppress_completion);
        assert!(!completed_turn(
            Some(Status::Busy),
            Status::Idle,
            suppress_completion,
        ));
        assert!(!settled_while_unobserved(
            Some(Status::Busy),
            Status::Idle,
            false,
            suppress_completion,
        ));
    }
}

impl App {
    fn new(
        hook_port: Option<u16>,
        sidebar_feed: std::sync::Arc<std::sync::Mutex<Option<Result<serde_json::Value, String>>>>,
        approvals: std::sync::Arc<approvals::ApprovalHub>,
    ) -> Self {
        let layout = load_layout();
        let mut engine = ActivityEngine::default();
        let overlay_snapshot = overlay::load();
        let mut scan_cache = ScanCache::default();
        let model = scan_sidebar(
            &mut engine,
            overlay_snapshot.as_ref(),
            &std::collections::HashSet::new(),
            &mut scan_cache,
        );
        let selected_id = layout
            .2
            .clone()
            .filter(|id| model.rows.iter().any(|r| r.id == *id))
            .or_else(|| first_listed_session(&model));
        let model_runtime_generations = model_runtime_generations(&engine, &model);
        let mut app = App {
            model,
            engine,
            scan_cache,
            selected_id,
            selected_archive: None,
            selected_recent: None,
            activity_log: unpeel_core::activity_log::ActivityLogStore::load_default()
                .unwrap_or_default(),
            deferred_stop_effects: HashMap::new(),
            model_runtime_generations,
            selected_new_session: None,
            selected_add_project: false,
            dragging_project: None,
            dragging_folder: None,
            pending_spawn_target: None,
            last_selection_key: String::new(),
            archive_query: String::new(),
            selected_worktree_folder: None,
            expanded_worktrees: HashSet::new(),
            modal: {
                let state: serde_json::Value =
                    std::fs::read(unpeel_core::app_paths::app_state_path())
                        .ok()
                        .and_then(|raw| serde_json::from_slice(&raw).ok())
                        .unwrap_or(serde_json::Value::Null);
                if unpeel_core::first_run::needs_seeding(&state) {
                    let projects = unpeel_core::first_run::suggested_projects(3);
                    Some(Modal::FirstRun(FirstRun {
                        presets: unpeel_core::first_run::installed_presets(),
                        accepted: vec![true; projects.len()],
                        projects,
                        row: 0,
                    }))
                } else {
                    None
                }
            },
            exit_requested: false,
            sidebar_scroll: 0,
            sidebar_width: layout.0,
            dragging_divider: false,
            collapsed: layout.1,
            mouse_pos: None,
            confirm: None,
            info: None,
            local_url_verdicts: Default::default(),
            local_url_checks_in_flight: Default::default(),
            announced_local_urls: HashSet::new(),
            update_available: None,
            env_hint: None,
            hook_port,
            sidebar_feed,
            bridge_mode: false,
            bridge_mobile_endpoint_handoff: false,
            legacy_bridge_mode: false,
            bridge_unresolved: false,
            feed_note: "",
            preview_scroll: 0,
            terminal_focus: false,
            terminal_selection: None,
            input: control::InteractiveInput::new(),
            local_unread: HashSet::new(),
            unread_ids: HashSet::new(),
            overlay: overlay_snapshot,
            overlay_loaded_at: Some(Instant::now()),
            pending_select: None,
            replacement_selection: LocalReplacementSelectionState::default(),
            mobile_snapshot: std::sync::Arc::new(std::sync::Mutex::new(
                sessions::MobileSnapshot::default(),
            )),
            mobile_server: None,
            legacy_mobile_handoff_latched: false,
            legacy_mobile_handoff_classified: false,
            legacy_mobile_fallback_port: None,
            legacy_mobile_mismatch_observed_at: None,
            relay_uplink: None,
            link_suppressed: false,
            link_blocked_entitlement: None,
            link_worker: None,
            link_activation_in_flight: false,
            mobile_resizes: std::sync::Arc::new(std::sync::Mutex::new(
                std::collections::HashMap::new(),
            )),
            approvals,
            pairing: std::sync::Arc::new(pairing::PairingWindow::default()),
            settings: None,
            link_input: String::new(),
            preset_add: String::new(),
            selection_mode: false,
            dragging_row: None,
            last_click: None,
            in_flight: None,
            idle_since_ms: HashMap::new(),
            auto_archive_issued: HashSet::new(),
            auto_archive_retry_after_ms: HashMap::new(),
        };
        if let Some(id) = app.selected_id.clone() {
            app.mark_read(&id);
        }
        app
    }

    fn defer_stop_effects_until_runtime_generation_settles(
        &mut self,
        session_id: &str,
        observed_generation: Option<u64>,
    ) {
        // Native owns shared activity/history while its sidebar bridge is
        // live. In standalone mode preserve the existing active→idle edge;
        // an already-idle duplicate Stop had no irreversible effect before
        // this restart guard and should not gain one now.
        let was_active = !self.bridge_mode
            && self.model.rows.iter().any(|row| {
                row.id == session_id
                    && matches!(
                        row.status,
                        Status::Starting | Status::Busy | Status::Attention
                    )
            });
        if !was_active {
            return;
        }
        self.deferred_stop_effects.insert(
            session_id.to_string(),
            DeferredStopEffects {
                observed_generation,
                publish_after: Instant::now() + DEFERRED_STOP_EFFECT_DELAY,
            },
        );
    }

    /// Resolve generation-stable Stop effects before scanning, but leave each
    /// resolved token installed through this scan. That suppresses the same
    /// active→idle edge even when the generation reset itself makes a freshly
    /// unlatched hook-capable row appear Idle. The caller removes the returned
    /// ids only after history and unread reconciliation finish.
    fn reconcile_deferred_stop_effects(&mut self) -> HashSet<String> {
        let now = Instant::now();
        let decisions = self
            .deferred_stop_effects
            .iter()
            .filter_map(|(session_id, deferred)| {
                let current_generation = runtime_launch_metadata_on_disk(session_id).0;
                match deferred_stop_resolution(*deferred, current_generation, now) {
                    DeferredStopResolution::Pending => None,
                    DeferredStopResolution::Publish => Some((session_id.clone(), true)),
                    DeferredStopResolution::Discard => Some((session_id.clone(), false)),
                }
            })
            .collect::<Vec<_>>();

        let mut resolved = HashSet::new();
        for (session_id, publish) in decisions {
            if publish && !self.bridge_mode {
                self.publish_deferred_stop_effects(&session_id);
            }
            resolved.insert(session_id);
        }
        resolved
    }

    fn publish_deferred_stop_effects(&mut self, session_id: &str) {
        let Some(row) = self
            .model
            .rows
            .iter()
            .find(|row| row.id == session_id)
            .cloned()
        else {
            return;
        };
        let now_ms = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;
        self.append_activity_event(
            &row,
            unpeel_core::activity_log::ActivityLogKind::Finished,
            now_ms,
        );
        if self.selected_id.as_deref() != Some(session_id) {
            self.local_unread.insert(session_id.to_string());
        }
    }

    fn append_activity_event(
        &mut self,
        row: &SessionRow,
        kind: unpeel_core::activity_log::ActivityLogKind,
        at: u64,
    ) {
        let title = row.label.trim();
        let entry = unpeel_core::activity_log::ActivityLogEntry {
            id: uuid::Uuid::new_v4().to_string(),
            session_id: row.id.clone(),
            kind,
            at,
            title: if title.is_empty() {
                "Untitled session".to_string()
            } else {
                title.to_string()
            },
            command: row.presentation_command().to_string(),
            project_id: if row.group_id.is_empty() {
                row.project_id.clone()
            } else {
                row.group_id.clone()
            },
            project_name: self.activity_project_name(row),
        };
        let _ = self.activity_log.append(entry);
    }

    fn rescan(&mut self) {
        let resolved_deferred_stops = self.reconcile_deferred_stop_effects();
        let previous_rows = self.model.rows.clone();
        let previous: std::collections::HashMap<String, Status> = self
            .model
            .rows
            .iter()
            .map(|r| (r.id.clone(), r.status))
            .collect();
        // Prefer the app-computed sidebar (exact desktop rows: overlays,
        // archived window, archive counts); fall back to the disk model.
        let feed = self
            .sidebar_feed
            .lock()
            .ok()
            .and_then(|guard| guard.clone());
        let bridge_mobile_endpoint_handoff = feed
            .as_ref()
            .and_then(|result| result.as_ref().ok())
            .and_then(|value| value.get("mobile_endpoint_handoff"))
            .and_then(serde_json::Value::as_u64)
            == Some(1);
        self.legacy_bridge_mode = matches!(
            &feed,
            Some(Err(error)) if error.contains("predate this route")
        );
        self.bridge_unresolved = matches!(
            &feed,
            Some(Err(error)) if error.contains("bridge is still resolving")
        );
        self.feed_note = match &feed {
            Some(Ok(_)) => "",
            // The app answers but its build has no /mcp/sidebar route: the
            // overlay fallback below still mirrors its organization.
            Some(Err(e)) if e.contains("predate this route") => "app update pending",
            Some(Err(e)) if e.contains("bridge is still resolving") => "connecting…",
            Some(Err(_)) => "app offline",
            None => "connecting…",
        };
        // Keep the native overlay warm in bridge mode too: older app builds
        // omitted child kind metadata from `/mcp/sidebar`, so the TUI uses
        // shared state to distinguish a plain group from a worktree.
        let stale = self
            .overlay_loaded_at
            .map(|t| t.elapsed() > Duration::from_secs(5))
            .unwrap_or(true);
        if stale {
            self.overlay = overlay::load();
            self.overlay_loaded_at = Some(Instant::now());
        }
        let app_state = load_app_state();
        let bridged = feed
            .and_then(|r| r.ok())
            .and_then(|value| model_from_bridge(&value, app_state.as_ref(), self.overlay.as_ref()));
        match bridged {
            Some(model) => {
                self.bridge_mode = true;
                self.bridge_mobile_endpoint_handoff = bridge_mobile_endpoint_handoff;
                self.model = model;
            }
            None => {
                self.bridge_mode = false;
                self.bridge_mobile_endpoint_handoff = false;
                // Carry the selection and last frame's unread set through:
                // a stopped session the user is on (or hasn't read) must not
                // fall out of the list under the recent-stopped window.
                let mut keep: std::collections::HashSet<String> =
                    self.unread_ids.iter().cloned().collect();
                if let Some(id) = &self.selected_id {
                    keep.insert(id.clone());
                }
                self.model = scan_sidebar(
                    &mut self.engine,
                    self.overlay.as_ref(),
                    &keep,
                    &mut self.scan_cache,
                );
            }
        }
        let current_model_runtime_generations =
            model_runtime_generations(&self.engine, &self.model);
        let runtime_generation_edges = runtime_generation_edges(
            &self.model_runtime_generations,
            &current_model_runtime_generations,
        );
        self.model_runtime_generations = current_model_runtime_generations;
        // Both frontends read the same durable activity feed. While the app
        // owns activity it also owns appends; in standalone mode the TUI
        // records the lifecycle edges it observes so a headless Host gets the
        // same All recent history. Startup is already baselined by App::new's
        // initial scan, so old sessions do not acquire synthetic events.
        let _ = self.activity_log.refresh();
        if !self.bridge_mode {
            let before = previous_rows
                .iter()
                .map(|row| (row.id.as_str(), row))
                .collect::<HashMap<_, _>>();
            let now_ms = SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64;
            let mut events = Vec::new();
            for row in &self.model.rows {
                let old = before.get(row.id.as_str()).copied();
                if old.is_none() && row.running {
                    events.push((
                        row.clone(),
                        unpeel_core::activity_log::ActivityLogKind::Started,
                    ));
                }
                if row.status == Status::Attention
                    && old.is_some_and(|old| old.status != Status::Attention)
                {
                    events.push((
                        row.clone(),
                        unpeel_core::activity_log::ActivityLogKind::NeedsInput,
                    ));
                }
                if completed_turn(
                    old.map(|old| old.status),
                    row.status,
                    self.deferred_stop_effects.contains_key(&row.id)
                        || runtime_generation_edges.contains(&row.id),
                ) {
                    events.push((
                        row.clone(),
                        unpeel_core::activity_log::ActivityLogKind::Finished,
                    ));
                }
                if row.status == Status::Exited
                    && !row.archived
                    && old.is_some_and(|old| old.status != Status::Exited)
                {
                    events.push((
                        row.clone(),
                        unpeel_core::activity_log::ActivityLogKind::Exited,
                    ));
                }
            }
            for (row, kind) in events {
                self.append_activity_event(&row, kind, now_ms);
            }
        }
        // Settled-while-unobserved → unread (fallback mode; the app computes
        // this in bridge mode and ships it on the row).
        if !self.bridge_mode {
            for row in &self.model.rows {
                if settled_while_unobserved(
                    previous.get(&row.id).copied(),
                    row.status,
                    self.selected_id.as_deref() == Some(row.id.as_str()),
                    self.deferred_stop_effects.contains_key(&row.id)
                        || runtime_generation_edges.contains(&row.id),
                ) {
                    self.local_unread.insert(row.id.clone());
                }
            }
            self.local_unread
                .retain(|id| self.model.rows.iter().any(|r| r.id == *id));
        }
        for session_id in resolved_deferred_stops {
            self.deferred_stop_effects.remove(&session_id);
        }
        if let Ok(mut guard) = self.mobile_snapshot.lock() {
            *guard =
                sessions::mobile_snapshot(&self.model, self.overlay.as_ref(), &self.unread_ids);
        }
        // Resolve the frame's unread set once: a receipt newer than the
        // session's last settle beats any claim (including a stale app
        // snapshot — its unread lives in memory and can lag or reset).
        self.unread_ids = self
            .model
            .rows
            .iter()
            .filter(|row| {
                let claimed = row.unread || self.local_unread.contains(&row.id);
                if !claimed {
                    return false;
                }
                match unpeel_core::session_ops::read_marker(&row.id) {
                    Some(read_at) => settled_at(&row.id, &row.command).is_some_and(|s| s > read_at),
                    None => true,
                }
            })
            .map(|row| row.id.clone())
            .collect();
        // Native's compatibility Resume receipt is only `{ok:true}` even
        // though the replacement gets a fresh Session id. Follow that exact
        // replacement by stable launch identity. Ambiguity/expiry leaves the
        // sidebar deliberately unselected instead of choosing its first row.
        if let Some(id) =
            resolve_local_replacement_selection(&mut self.replacement_selection, &self.model.rows)
        {
            self.selected_archive = None;
            self.selected_recent = None;
            self.selected_worktree_folder = None;
            self.selected_new_session = None;
            self.selected_add_project = false;
            self.selected_id = Some(id);
            self.preview_scroll = 0;
        }
        if let Some(id) = self.pending_select.clone() {
            if self.model.rows.iter().any(|r| r.id == id) {
                self.replacement_selection.clear();
                // Clear the other selection kinds or the highlight stays on
                // whatever started this session — the "+ New session" row
                // owns the selection until told otherwise.
                self.selected_archive = None;
                self.selected_recent = None;
                self.selected_worktree_folder = None;
                self.selected_new_session = None;
                self.selected_add_project = false;
                self.selected_id = Some(id);
                self.preview_scroll = 0;
                self.pending_select = None;
            }
        }
        if let Some(id) = self.selected_id.clone() {
            self.mark_read(&id);
        }
        let still_there = self
            .selected_id
            .as_deref()
            .is_some_and(|id| self.model.rows.iter().any(|r| r.id == id));
        if !still_there {
            self.selected_id = if self.replacement_selection.pending.is_some()
                || self.replacement_selection.suppress_default
            {
                None
            } else {
                first_listed_session(&self.model)
            };
        }
    }

    /// True when every project header is folded — the sidebar's bottom-right
    /// toggle shows "+" (expand-all) instead of "-". False with no headers,
    /// so an empty sidebar keeps the "-" resting face.
    pub fn all_headers_collapsed(&self) -> bool {
        let mut any = false;
        for item in &self.model.items {
            if let SidebarItem::Header(name) = item {
                any = true;
                if !self.collapsed.contains(name) {
                    return false;
                }
            }
        }
        any
    }

    /// Fold every project header, or unfold them all if everything is already
    /// folded. One implementation for its three doors: the `-` key, the
    /// palette's fold action, and the sidebar's bottom-right toggle. Keyed on
    /// the visible headers, not `collapsed.len()` — stale names of removed
    /// projects linger in the set and must not flip the direction.
    pub fn toggle_fold_all(&mut self) {
        if self.all_headers_collapsed() {
            self.collapsed.clear();
        } else {
            self.collapsed = self
                .model
                .items
                .iter()
                .filter_map(|item| match item {
                    SidebarItem::Header(name) => Some(name.clone()),
                    _ => None,
                })
                .collect();
        }
    }

    /// Sidebar items with collapsed sections filtered out.
    /// The sidebar as it should be PAINTED, which during a drag is not the
    /// sidebar as it is: the carried block is shown already in the place it
    /// would land. Tinting whatever row happens to sit under the cursor
    /// (what this used to do) tells you nothing about where the drop goes.
    pub fn sidebar_render(&self) -> SidebarRender {
        let mut items: Vec<SidebarItem> = self.visible_items().into_iter().cloned().collect();
        let mut carried = None;

        if let Some((dragged, drop_pos)) = &self.dragging_row {
            let root = self.block_root(dragged);
            let in_block = |item: &SidebarItem| match item {
                SidebarItem::Session(i) => self.block_root(&self.model.rows[*i].id) == root,
                _ => false,
            };
            let block: Vec<usize> = items
                .iter()
                .enumerate()
                .filter(|(_, item)| in_block(item))
                .map(|(pos, _)| pos)
                .collect();
            // The block is contiguous by construction (a root and its
            // descendants render together); bail if that ever stops holding.
            let contiguous = block.windows(2).all(|pair| pair[1] == pair[0] + 1);
            let target_root = items.get(*drop_pos).and_then(|item| match item {
                SidebarItem::Session(i) => Some(self.block_root(&self.model.rows[*i].id)),
                _ => None,
            });
            if let (Some(&start), true, Some(target_root)) =
                (block.first(), contiguous && !block.is_empty(), target_root)
            {
                if target_root != root {
                    let target_start = items
                        .iter()
                        .position(|item| match item {
                            SidebarItem::Session(i) => {
                                self.block_root(&self.model.rows[*i].id) == target_root
                            }
                            _ => false,
                        })
                        .unwrap_or(start);
                    let moving: Vec<SidebarItem> =
                        items.drain(start..start + block.len()).collect();
                    // Removing shifted everything after `start` left.
                    let insert_at = if target_start > start {
                        target_start - block.len() + 1
                    } else {
                        target_start
                    }
                    .min(items.len());
                    let length = moving.len();
                    for (offset, item) in moving.into_iter().enumerate() {
                        items.insert(insert_at + offset, item);
                    }
                    carried = Some((insert_at, insert_at + length - 1));
                } else {
                    carried = Some((start, start + block.len() - 1));
                }
            }
        }

        // A project drag previews the same way a session drag does: the whole
        // group — header, its "+ New session", its sessions, its archive —
        // renders where it would land.
        if let Some((dragged, _, drop_pos)) = &self.dragging_project {
            // Group extents: a header owns every row until the next header.
            // "+ Add project" belongs to no project and always stays last.
            let group_of = |start: usize| -> (usize, usize) {
                let mut end = start + 1;
                while end < items.len()
                    && !matches!(items[end], SidebarItem::Header(_) | SidebarItem::AddProject)
                {
                    end += 1;
                }
                (start, end)
            };
            let header_at = |name: &str| {
                items
                    .iter()
                    .position(|item| matches!(item, SidebarItem::Header(n) if n == name))
            };
            // Which group is the cursor over? Anything inside one counts.
            let target_header = items
                .iter()
                .enumerate()
                .take(drop_pos + 1)
                .filter_map(|(pos, item)| match item {
                    SidebarItem::Header(_) => Some(pos),
                    _ => None,
                })
                .next_back();

            if let (Some(src), Some(dst)) = (header_at(dragged), target_header) {
                if src != dst {
                    let (start, end) = group_of(src);
                    let (target_start, _) = group_of(dst);
                    let block: Vec<SidebarItem> = items.drain(start..end).collect();
                    let length = block.len();
                    let insert_at = if target_start > start {
                        target_start - length
                    } else {
                        target_start
                    }
                    .min(items.len());
                    for (offset, item) in block.into_iter().enumerate() {
                        items.insert(insert_at + offset, item);
                    }
                    carried = Some((insert_at, insert_at + length - 1));
                } else {
                    let (start, end) = group_of(src);
                    carried = Some((start, end.saturating_sub(1)));
                }
            }
        }

        // Inline folders (plain groups and Git worktrees) reorder only among
        // siblings with the same parent. An expanded folder carries its
        // visible session rows so the preview matches the eventual drop.
        if let Some(drag) = &self.dragging_folder {
            let block_of = |items: &[SidebarItem], project_id: &str| {
                let start = items.iter().position(|item| {
                    matches!(
                        item,
                        SidebarItem::WorktreeHeader { project_id: id, .. }
                            if id == project_id
                    )
                })?;
                let mut end = start + 1;
                while end < items.len() {
                    match &items[end] {
                        SidebarItem::Session(index)
                            if self.model.rows[*index].group_id == project_id =>
                        {
                            end += 1;
                        }
                        _ => break,
                    }
                }
                Some((start, end))
            };
            let target_id = items
                .iter()
                .enumerate()
                .take(drag.drop_pos.saturating_add(1))
                .filter_map(|(_, item)| match item {
                    SidebarItem::WorktreeHeader {
                        project_id, parent, ..
                    } if *parent == drag.parent_id => Some(project_id.clone()),
                    _ => None,
                })
                .next_back();
            if let (Some((start, end)), Some(target_id)) =
                (block_of(&items, &drag.project_id), target_id)
            {
                if target_id == drag.project_id {
                    carried = Some((start, end.saturating_sub(1)));
                } else if let Some((target_start, target_end)) = block_of(&items, &target_id) {
                    let block: Vec<SidebarItem> = items.drain(start..end).collect();
                    let length = block.len();
                    let insert_at = if start < target_start {
                        target_end.saturating_sub(length)
                    } else {
                        target_start
                    }
                    .min(items.len());
                    for (offset, item) in block.into_iter().enumerate() {
                        items.insert(insert_at + offset, item);
                    }
                    carried = Some((insert_at, insert_at + length.saturating_sub(1)));
                }
            }
        }

        let selected = items.iter().position(|item| self.item_is_selected(item));
        SidebarRender {
            items,
            selected,
            carried,
        }
    }

    /// The project `n` would start a session in: the one holding the
    /// selection. Named on its "+ New session" row so the key's target is
    /// never a guess.
    pub fn active_project_id(&self) -> Option<String> {
        if let Some(project) = &self.selected_new_session {
            return Some(project.clone());
        }
        let selected = self.selected_id.as_deref()?;
        self.model
            .rows
            .iter()
            .find(|row| row.id == selected)
            .map(|row| {
                if row.group_id.is_empty() {
                    row.project_id.clone()
                } else {
                    row.group_id.clone()
                }
            })
    }

    /// The sidebar row the mouse is over, if it is inside the list area.
    /// Feeds the hover "+" on project headers.
    pub fn hovered_sidebar_pos(&self) -> Option<usize> {
        let (col, row) = self.mouse_pos?;
        let divider = self.sidebar_width.saturating_sub(1);
        if col == 0 || col >= divider || row == 0 {
            return None;
        }
        Some(self.sidebar_scroll + (row - 1) as usize)
    }

    /// The project id behind a header row. Groups no longer all carry a
    /// "+ New session" row, so the id is read from whichever child names
    /// it: the row itself (empty projects) or any session/footer under it.
    pub fn project_id_for_header(&self, header: &str) -> Option<String> {
        let mut in_group = false;
        for item in &self.model.items {
            match item {
                SidebarItem::Header(name) => {
                    if in_group {
                        return None;
                    }
                    in_group = name == header;
                }
                _ if !in_group => {}
                SidebarItem::NewSession { project, .. } => return Some(project.clone()),
                SidebarItem::Session(i) => {
                    let row = &self.model.rows[*i];
                    return Some(if row.group_id.is_empty() {
                        row.project_id.clone()
                    } else {
                        row.group_id.clone()
                    });
                }
                // A folder row names its OWNING project — the header the
                // caller asked about — never the worktree child itself.
                SidebarItem::WorktreeHeader { parent, .. } => return Some(parent.clone()),
                _ => {}
            }
        }
        None
    }

    /// The desktop folder color for a header, as its rawValue ("sky", …).
    /// UserDefaults-backed like pins and titles, so it rides the same
    /// overlay; absent everywhere the overlay is (Linux, isolated workspaces).
    pub fn project_color_for_header(&self, header: &str) -> Option<String> {
        let id = self.project_id_for_header(header)?;
        self.project_color_for_id(&id)
    }

    /// Folder color by project id — worktree folder rows tint their chevron
    /// with the OWNING project's color (the id is in the sidebar item, no
    /// header-name lookup needed).
    pub fn project_color_for_id(&self, id: &str) -> Option<String> {
        self.overlay.as_ref()?.project_colors.get(id).cloned()
    }

    /// Live sessions under a project — what the context menu's "Stop all"
    /// acts on.
    pub fn running_ids_in_project(&self, project_id: &str) -> Vec<String> {
        self.model
            .rows
            .iter()
            .filter(|row| {
                let group = if row.group_id.is_empty() {
                    &row.project_id
                } else {
                    &row.group_id
                };
                group == project_id && row.running && !row.archived
            })
            .map(|row| row.id.clone())
            .collect()
    }

    /// Filed sessions in a project — what "Archived (N)" shows in the
    /// project menu, and what gates `a` opening the archive library.
    pub fn archived_count_in_project(&self, project_id: &str) -> usize {
        self.model
            .archived_counts
            .get(project_id)
            .copied()
            .unwrap_or(0)
    }

    /// The project owning the current selection — the one `a` opens the
    /// archive library for.
    pub fn selected_project_id(&self) -> Option<String> {
        if let Some(worktree) = &self.selected_worktree_folder {
            return Some(worktree.clone());
        }
        if let Some(project) = &self.selected_new_session {
            return Some(project.clone());
        }
        let row = self.selected_session()?;
        Some(if row.group_id.is_empty() {
            row.project_id.clone()
        } else {
            row.group_id.clone()
        })
    }

    /// The sessions `^1`…`^9` address: the ones under the same project
    /// header as the current selection, in the order they render. Matches
    /// the desktop's ⌘1-9, which is also scoped to the active project.
    pub fn quick_jump_ids(&self) -> Vec<String> {
        let items = self.visible_items();
        let selected = self.selected_id.as_deref();
        // Find the header block containing the selection (or the first).
        let mut current: Vec<String> = Vec::new();
        let mut found = false;
        for item in items {
            match item {
                SidebarItem::Header(_) => {
                    if found {
                        break;
                    }
                    current.clear();
                }
                SidebarItem::Session(i) => {
                    let row = &self.model.rows[*i];
                    if Some(row.id.as_str()) == selected {
                        found = true;
                    }
                    current.push(row.id.clone());
                }
                _ => {}
            }
        }
        current.truncate(9);
        current
    }

    /// Whether the bottom row has anything to carry this frame. It has no
    /// height at all otherwise — the resting UI is sidebar + terminal, with
    /// the two ways in on the sidebar's border and every key in `?`.
    /// Whether the bottom row carries an interactive state (approval, confirm,
    /// spinner, selection mode). Outcome messages (`info`) are NOT part of
    /// this — they render as a self-dismissing toast in the top-right instead.
    pub fn has_status_message(&self) -> bool {
        self.approvals.front().is_some()
            || self.confirm.is_some()
            || self.in_flight.is_some()
            || self.selection_mode
    }

    /// Whether this row carries the current selection, whatever kind it is.
    fn item_is_selected(&self, item: &SidebarItem) -> bool {
        match item {
            SidebarItem::Session(i) => {
                self.selected_worktree_folder.is_none()
                    && self.selected_archive.is_none()
                    && self.selected_recent.is_none()
                    && self.selected_new_session.is_none()
                    && self.selected_id.as_deref() == Some(self.model.rows[*i].id.as_str())
            }
            SidebarItem::WorktreeHeader {
                project_id,
                is_group: false,
                ..
            } => self.selected_worktree_folder.as_deref() == Some(project_id.as_str()),
            SidebarItem::WorktreeHeader { is_group: true, .. } => false,
            SidebarItem::NewSession { project, .. } => {
                self.selected_new_session.as_deref() == Some(project.as_str())
            }
            SidebarItem::AddProject => self.selected_add_project,
            SidebarItem::Header(_) => false,
        }
    }

    pub fn visible_items(&self) -> Vec<&SidebarItem> {
        let mut visible = Vec::new();
        let mut hiding = false;
        // The collapsed worktree whose sessions are being skipped. A
        // worktree's sessions/archive footer carry its project id as their
        // group, and the parent's own rows follow with a different group —
        // that mismatch is what ends the skip.
        let mut folded_worktree: Option<&str> = None;
        for item in &self.model.items {
            match item {
                SidebarItem::Header(name) => {
                    hiding = self.collapsed.contains(name);
                    folded_worktree = None;
                    visible.push(item);
                }
                SidebarItem::WorktreeHeader { project_id, .. } => {
                    folded_worktree = (!self.expanded_worktrees.contains(project_id))
                        .then_some(project_id.as_str());
                    if !hiding {
                        visible.push(item);
                    }
                }
                SidebarItem::Session(i) => {
                    let group = self.model.rows[*i].group_id.as_str();
                    if folded_worktree.is_some_and(|wt| wt != group) {
                        folded_worktree = None;
                    }
                    if !hiding && folded_worktree.is_none() {
                        visible.push(item);
                    }
                }
                SidebarItem::NewSession { project, .. } => {
                    // An empty folder's "+ New session" stands in for its
                    // sessions, so it folds with them; a mismatched project
                    // means the folder's block ended (parent-level rows
                    // carry the parent's id), same as the Session arm.
                    if folded_worktree.is_some_and(|wt| wt != project) {
                        folded_worktree = None;
                    }
                    if !hiding && folded_worktree.is_none() {
                        visible.push(item);
                    }
                }
                other => visible.push(other),
            }
        }
        visible
    }

    pub fn selected_visible_pos(&self) -> Option<usize> {
        if let Some(worktree) = &self.selected_worktree_folder {
            return self.visible_items().iter().position(|item| {
                matches!(
                    item,
                    SidebarItem::WorktreeHeader {
                        project_id,
                        is_group: false,
                        ..
                    } if project_id == worktree
                )
            });
        }
        if let Some(project) = &self.selected_new_session {
            return self.visible_items().iter().position(
                |item| matches!(item, SidebarItem::NewSession { project: p, .. } if p == project),
            );
        }
        if self.selected_add_project {
            return self
                .visible_items()
                .iter()
                .position(|item| matches!(item, SidebarItem::AddProject));
        }
        let id = self.selected_id.as_deref()?;
        self.visible_items().iter().position(|item| match item {
            SidebarItem::Session(i) => self.model.rows[*i].id == id,
            _ => false,
        })
    }

    /// Whether any of a worktree's sessions is working — the folder row's
    /// shimmer. Row-based (not item-based) so a collapsed folder still
    /// shows life, exactly like a collapsed project header.
    pub fn worktree_is_busy(&self, project_id: &str) -> bool {
        self.model.rows.iter().any(|r| {
            r.group_id == project_id && matches!(r.status, Status::Busy | Status::Starting)
        })
    }

    /// Whether any of a worktree's sessions needs attention — the folder
    /// row's ◆, visible even while the folder is collapsed.
    pub fn worktree_needs_attention(&self, project_id: &str) -> bool {
        self.model
            .rows
            .iter()
            .any(|r| r.group_id == project_id && r.status == Status::Attention)
    }

    /// Whether a session row renders under a worktree folder (one indent
    /// level deeper than a project's own sessions).
    pub fn session_in_worktree(&self, row: &SessionRow) -> bool {
        self.model.items.iter().any(|item| {
            matches!(item, SidebarItem::WorktreeHeader { project_id, .. } if *project_id == row.group_id)
        })
    }

    /// Archived sessions matching the search box — what the preview lists
    /// and what the verbs act on, so the highlighted row is always the one
    /// the user can see.
    pub fn archived_matches(&self, group: &str) -> Vec<&SessionRow> {
        let needle = self.archive_query.trim().to_lowercase();
        self.archived_rows(group)
            .into_iter()
            .filter(|r| {
                needle.is_empty()
                    || r.label.to_lowercase().contains(&needle)
                    || r.command.to_lowercase().contains(&needle)
            })
            .collect()
    }

    /// Archived sessions inside a group, newest first — the preview's list
    /// when the project's archive library is open.
    pub fn archived_rows(&self, group: &str) -> Vec<&SessionRow> {
        let mut rows: Vec<&SessionRow> = self
            .model
            .rows
            .iter()
            .filter(|r| r.archived && (r.group_id == group || r.project_id == group))
            .collect();
        rows.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        rows
    }

    fn select_item(&mut self, item: &SidebarItem) {
        self.select_item_impl(item, true);
    }

    /// Same selection semantics without the local read-receipt side effect.
    /// The remote Controller scope must never write local markers or POST the
    /// local bridge — its loop clears unread through the Host's mark-read
    /// effect instead.
    pub(crate) fn select_item_silent(&mut self, item: &SidebarItem) {
        self.select_item_impl(item, false);
    }

    fn select_item_impl(&mut self, item: &SidebarItem, mark_read: bool) {
        // Any explicit sidebar choice owns focus from here onward, including
        // re-selecting the Resume source after an unresolved bridge effect.
        self.replacement_selection.clear();
        match item {
            SidebarItem::Session(i) => {
                let id = self.model.rows[*i].id.clone();
                self.selected_archive = None;
                self.selected_recent = None;
                self.selected_worktree_folder = None;
                self.selected_new_session = None;
                self.selected_add_project = false;
                if mark_read {
                    self.mark_read(&id);
                }
                self.selected_id = Some(id);
                self.preview_scroll = 0;
            }
            // Git worktrees are navigable destinations. Landing here only
            // highlights the folder; ⏎ (or a click) folds/unfolds it.
            SidebarItem::WorktreeHeader {
                project_id,
                is_group: false,
                ..
            } => {
                self.selected_archive = None;
                self.selected_recent = None;
                self.selected_new_session = None;
                self.selected_worktree_folder = Some(project_id.clone());
            }
            // Plain groups are structural headers like project headers:
            // click folds/unfolds them, but no input path may select them.
            SidebarItem::WorktreeHeader { is_group: true, .. } => {}
            // Highlight only; ⏎ (or n) opens the picker for this project.
            SidebarItem::NewSession { project, .. } => {
                self.selected_archive = None;
                self.selected_recent = None;
                self.selected_worktree_folder = None;
                self.selected_new_session = Some(project.clone());
            }
            SidebarItem::AddProject => {
                self.selected_archive = None;
                self.selected_recent = None;
                self.selected_worktree_folder = None;
                self.selected_new_session = None;
                self.selected_add_project = true;
            }
            SidebarItem::Header(_) => {}
        }
    }

    /// The sidebar header a session renders under (its project name).
    pub fn project_name_for(&self, session_id: &str) -> String {
        let mut last_header = String::new();
        for item in &self.model.items {
            match item {
                SidebarItem::Header(name) => last_header = name.clone(),
                SidebarItem::Session(i) if self.model.rows[*i].id == session_id => {
                    return last_header;
                }
                _ => {}
            }
        }
        self.model
            .rows
            .iter()
            .find(|r| r.id == session_id)
            .map(|r| r.label.clone())
            .unwrap_or_default()
    }

    /// Land a dragged row at `drop_pos` (an index into the visible items)
    /// and persist the project's manual order. Blocks move whole: a parent
    /// carries its children, matching the desktop's subtree ordering.
    /// Move a whole project to where it was dropped. Projects order by a
    /// shared file, like sessions, so the drag shows up in the app too.
    pub fn commit_project_drag(&mut self, dragged: &str, drop_pos: usize) {
        let visible = self.visible_items();
        // Which project's row did we land on? Anything inside a project
        // counts as that project, so you can drop onto its sessions.
        let mut target: Option<String> = None;
        let mut current: Option<String> = None;
        for (pos, item) in visible.iter().enumerate() {
            if let SidebarItem::Header(name) = item {
                current = Some(name.clone());
            }
            if pos == drop_pos {
                target = current.clone();
                break;
            }
        }
        // Header order as rendered, which is what the user is rearranging.
        let mut names: Vec<String> = visible
            .iter()
            .filter_map(|item| match item {
                SidebarItem::Header(name) => Some(name.clone()),
                _ => None,
            })
            .collect();
        drop(visible);
        let Some(target) = target else { return };
        if target == dragged {
            return;
        }
        let (Some(from), Some(to)) = (
            names.iter().position(|n| n == dragged),
            names.iter().position(|n| n == &target),
        ) else {
            return;
        };
        let moved = names.remove(from);
        // The live preview inserts a downward-dragged project BEFORE the
        // project under the raw pointer. Removing an earlier source shifts
        // that target one slot left, so persist the carried project's shown
        // position rather than the cursor target's pre-removal index.
        let insert_at = if from < to { to - 1 } else { to };
        names.insert(insert_at, moved);
        // The file keys on project ids; headers carry names, so map back.
        let ids: Vec<String> = names
            .iter()
            .filter_map(|name| self.project_id_for_name(name))
            .collect();
        self.persist_project_sibling_order(&ids);
    }

    /// Reorder one inline folder against the sibling under the drop point.
    /// Dropping outside this parent is deliberately ignored.
    pub fn commit_folder_drag(&mut self, dragged: &str, parent: &str, drop_pos: usize) {
        let visible = self.visible_items();
        let target = visible
            .iter()
            .enumerate()
            .take(drop_pos.saturating_add(1))
            .filter_map(|(_, item)| match item {
                SidebarItem::WorktreeHeader {
                    project_id,
                    parent: item_parent,
                    ..
                } if item_parent == parent => Some(project_id.clone()),
                _ => None,
            })
            .next_back();
        drop(visible);
        let Some(target) = target else { return };
        if target == dragged {
            return;
        }
        let mut siblings: Vec<String> = self
            .model
            .items
            .iter()
            .filter_map(|item| match item {
                SidebarItem::WorktreeHeader {
                    project_id,
                    parent: item_parent,
                    ..
                } if item_parent == parent => Some(project_id.clone()),
                _ => None,
            })
            .collect();
        let (Some(from), Some(to)) = (
            siblings.iter().position(|id| id == dragged),
            siblings.iter().position(|id| id == &target),
        ) else {
            return;
        };
        let moved = siblings.remove(from);
        siblings.insert(to, moved);
        self.persist_project_sibling_order(&siblings);
    }

    /// Persist a sibling permutation into the one flat cross-frontend order
    /// file. Filtering that list by parent reproduces each sibling order,
    /// while ids from every other project/folder keep their current ranks.
    fn persist_project_sibling_order(&mut self, sibling_ids: &[String]) {
        if sibling_ids.is_empty() {
            return;
        }
        let sibling_set: HashSet<&str> = sibling_ids.iter().map(String::as_str).collect();
        let mut all_ids = Vec::new();
        for item in &self.model.items {
            let id = match item {
                SidebarItem::Header(name) => self.project_id_for_header(name),
                SidebarItem::WorktreeHeader { project_id, .. } => Some(project_id.clone()),
                _ => None,
            };
            if let Some(id) = id {
                if !all_ids.contains(&id) {
                    all_ids.push(id);
                }
            }
        }
        let slots: Vec<usize> = all_ids
            .iter()
            .enumerate()
            .filter_map(|(index, id)| sibling_set.contains(id.as_str()).then_some(index))
            .collect();
        if slots.len() != sibling_ids.len() {
            return;
        }
        for (slot, id) in slots.into_iter().zip(sibling_ids) {
            all_ids[slot] = id.clone();
        }
        if let Err(err) = unpeel_core::session_ops::set_project_sibling_order(sibling_ids, &all_ids)
        {
            self.info = Some(err);
            return;
        }
        self.rescan();
    }

    /// A project's id from the name its header shows. Item-walking (not a
    /// row scan): an expanded worktree's sessions render under the PARENT's
    /// header, so mapping name→id through an arbitrary row could hand back
    /// a worktree child's id and corrupt the shared top-level order file.
    fn project_id_for_name(&self, name: &str) -> Option<String> {
        self.project_id_for_header(name)
    }

    pub fn commit_drag(&mut self, dragged: &str, drop_pos: usize) {
        let Some(row) = self.model.rows.iter().find(|r| r.id == dragged) else {
            return;
        };
        // Persist against the group the row renders under (a real project,
        // or the cwd bucket) so the sidebar reads back what the drag wrote.
        let project_id = if row.group_id.is_empty() {
            row.project_id.clone()
        } else {
            row.group_id.clone()
        };
        // Date-sorted groups have no manual order to write — the drop would
        // persist an order the sidebar ignores and the row would snap back.
        if unpeel_core::session_ops::session_date_sorted(&project_id) {
            self.info =
                Some("sorted by date — set Sort sessions to Custom order to re-order".into());
            return;
        }
        let root = dragged.to_string();
        // Current session order for this group, in rendered order.
        let mut roots: Vec<String> = Vec::new();
        for item in &self.model.items {
            if let SidebarItem::Session(i) = item {
                let candidate = &self.model.rows[*i];
                let candidate_group = if candidate.group_id.is_empty() {
                    candidate.project_id.clone()
                } else {
                    candidate.group_id.clone()
                };
                if candidate_group != project_id {
                    continue;
                }
                let candidate_root = candidate.id.clone();
                if !roots.contains(&candidate_root) {
                    roots.push(candidate_root);
                }
            }
        }
        // Which root did we drop onto?
        let visible = self.visible_items();
        let Some(SidebarItem::Session(target_index)) = visible.get(drop_pos).copied() else {
            return;
        };
        let target_root = self.model.rows[*target_index].id.clone();
        if target_root == root {
            return;
        }
        let (Some(from), Some(to)) = (
            roots.iter().position(|r| *r == root),
            roots.iter().position(|r| *r == target_root),
        ) else {
            return;
        };
        let moved = roots.remove(from);
        roots.insert(to, moved);
        if let Err(err) = unpeel_core::session_ops::set_session_order(&project_id, &roots) {
            self.info = Some(err);
            return;
        }
        // Rebuild NOW. The order is on disk, but the model is only rescanned
        // on the next poll tick — and the drag preview ends the moment the
        // button comes up, so without this the row snaps back to where it
        // started and then jumps into place a beat later.
        self.rescan();
    }

    /// Sessions are flat within a group, so every row is its own reorder
    /// block. Kept as a helper because drag-preview and commit share it.
    fn block_root(&self, session_id: &str) -> String {
        session_id.to_string()
    }

    /// Showing a session to the user marks it read everywhere: locally, in
    /// the shared receipt (so app-less frontends and restarts agree), and
    /// in a running app's in-memory set via the bridge.
    pub fn mark_read(&mut self, session_id: &str) {
        self.local_unread.remove(session_id);
        let launch_command = self
            .model
            .rows
            .iter()
            .find(|row| row.id == session_id)
            .map(|row| row.command.as_str())
            .unwrap_or("");
        // Idempotent: once the receipt covers the latest settle there is
        // nothing left to clear, even when a stale app snapshot still says
        // unread — otherwise every rescan would re-POST the bridge.
        let already_read = match unpeel_core::session_ops::read_marker(session_id) {
            Some(read_at) => {
                settled_at(session_id, launch_command).is_none_or(|settled| settled <= read_at)
            }
            None => false,
        };
        if already_read {
            return;
        }
        let _ = unpeel_core::session_ops::mark_read(session_id);
        let own_port = self.hook_port;
        let id = session_id.to_string();
        std::thread::spawn(move || {
            let _ = bridge::post(
                own_port,
                "/mcp/mark-read",
                &serde_json::json!({ "session_id": id }),
            );
        });
    }

    /// True while a phone recently resized this session through the TUI's
    /// mobile server (mirrors the desktop's "Resized for mobile" banner).
    pub fn mobile_resized(&self, session_id: &str) -> bool {
        self.mobile_resizes
            .lock()
            .ok()
            .and_then(|g| {
                g.get(session_id)
                    .map(|at| at.elapsed() < Duration::from_secs(300))
            })
            .unwrap_or(false)
    }

    /// Activity surfaces use the child folder name, not just the top-level
    /// header: plain groups read "Parent › Child" while Git worktrees keep
    /// their own branch/folder name, matching the native dropdown.
    fn activity_project_name(&self, row: &SessionRow) -> String {
        let mut header = String::new();
        for item in &self.model.items {
            match item {
                SidebarItem::Header(name) => header = name.clone(),
                SidebarItem::WorktreeHeader {
                    project_id,
                    name,
                    is_group,
                    ..
                } if *project_id == row.group_id => {
                    return if *is_group && !header.is_empty() {
                        format!("{header} › {name}")
                    } else {
                        name.clone()
                    };
                }
                SidebarItem::Session(index) if self.model.rows[*index].id == row.id => break,
                _ => {}
            }
        }
        if header.is_empty() {
            self.project_name_for(&row.id)
        } else {
            header
        }
    }

    /// Active jobs followed by unread settled jobs: the exact two groups in
    /// the native activity popover. Rows follow the sidebar tree's DFS order
    /// (with deduplication for pinned rows), and any model-only row is appended
    /// as a defensive fallback. This stays entirely in memory because the
    /// top-border indicator calls it every animation frame.
    pub fn activity_menu_entries(&self) -> Vec<ActivityMenuEntry> {
        let mut order = Vec::new();
        let mut seen = HashSet::new();
        for item in &self.model.items {
            if let SidebarItem::Session(index) = item {
                if seen.insert(self.model.rows[*index].id.as_str()) {
                    order.push(&self.model.rows[*index]);
                }
            }
        }
        for row in &self.model.rows {
            if seen.insert(row.id.as_str()) {
                order.push(row);
            }
        }
        let mut entries = Vec::new();
        for working in [true, false] {
            for row in &order {
                let row_working = matches!(row.status, Status::Starting | Status::Busy);
                let unread = self.unread_ids.contains(&row.id) && !row_working;
                if row_working != working || (!working && !unread) {
                    continue;
                }
                let title = row.label.trim();
                entries.push(ActivityMenuEntry {
                    session_id: row.id.clone(),
                    title: if title.is_empty() {
                        "Untitled session".to_string()
                    } else {
                        title.to_string()
                    },
                    project: self.activity_project_name(row),
                    command: row.presentation_command().to_string(),
                    working,
                    unread,
                });
            }
        }
        entries
    }

    /// Current active jobs, then the native-compatible persisted event feed
    /// newest-first. Active sessions are removed from the feed section so a
    /// long-running job appears exactly once, like RecentActivityView.swift.
    pub fn recent_activity_entries(&self) -> Vec<RecentActivityEntry> {
        let active = self
            .activity_menu_entries()
            .into_iter()
            .filter(|entry| entry.working)
            .collect::<Vec<_>>();
        let active_ids = active
            .iter()
            .map(|entry| entry.session_id.clone())
            .collect::<HashSet<_>>();
        let now_ms = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;
        let mut rows = active
            .into_iter()
            .map(|entry| {
                let status = self
                    .model
                    .rows
                    .iter()
                    .find(|row| row.id == entry.session_id)
                    .map(|row| match row.status {
                        Status::Starting => "Starting",
                        _ => "Working",
                    })
                    .unwrap_or("Working");
                RecentActivityEntry {
                    session_id: Some(entry.session_id),
                    title: entry.title,
                    project: entry.project,
                    event: status.to_string(),
                    command: entry.command,
                    working: true,
                    unread: false,
                    at: now_ms,
                }
            })
            .collect::<Vec<_>>();

        rows.extend(
            self.activity_log
                .entries()
                .iter()
                .rev()
                .filter(|entry| !active_ids.contains(entry.session_id.as_str()))
                .map(|entry| {
                    let live = self
                        .model
                        .rows
                        .iter()
                        .find(|row| row.id == entry.session_id);
                    let title = live
                        .map(|row| row.label.trim())
                        .filter(|title| !title.is_empty())
                        .unwrap_or_else(|| entry.title.trim());
                    let project = if entry.project_name.trim().is_empty() {
                        live.map(|row| self.project_name_for(&row.id))
                            .unwrap_or_default()
                    } else {
                        entry.project_name.clone()
                    };
                    let age = activity_age(entry.at, now_ms);
                    let when = if age == "now" {
                        "just now".to_string()
                    } else {
                        format!("{age} ago")
                    };
                    let event = match entry.kind {
                        unpeel_core::activity_log::ActivityLogKind::Started => {
                            format!("Started {when}")
                        }
                        unpeel_core::activity_log::ActivityLogKind::NeedsInput => {
                            format!("Needed input {when}")
                        }
                        unpeel_core::activity_log::ActivityLogKind::Finished => {
                            format!("Finished {when}")
                        }
                        unpeel_core::activity_log::ActivityLogKind::Exited => {
                            format!("Exited {when}")
                        }
                    };
                    RecentActivityEntry {
                        session_id: live.map(|row| row.id.clone()),
                        title: if title.is_empty() {
                            "Untitled session".to_string()
                        } else {
                            title.to_string()
                        },
                        project,
                        event,
                        command: live
                            .map(|row| row.presentation_command().to_string())
                            .unwrap_or_else(|| entry.command.clone()),
                        working: false,
                        unread: live.is_some_and(|row| self.unread_ids.contains(&row.id)),
                        at: entry.at,
                    }
                }),
        );
        rows
    }

    /// Reveal a session chosen outside the sidebar (activity menu/history):
    /// unfold its top-level project and child folder before selecting it, so
    /// the terminal and highlight can never disagree.
    pub fn reveal_session(&mut self, session_id: &str, mark_read: bool) -> bool {
        let Some(index) = self.model.rows.iter().position(|row| row.id == session_id) else {
            return false;
        };
        let row = &self.model.rows[index];
        let group_id = if row.group_id.is_empty() {
            row.project_id.clone()
        } else {
            row.group_id.clone()
        };
        let child_parent = self.model.items.iter().find_map(|item| match item {
            SidebarItem::WorktreeHeader {
                project_id, parent, ..
            } if *project_id == group_id => Some(parent.clone()),
            _ => None,
        });
        let header_project = child_parent.as_deref().unwrap_or(&group_id);
        let header = self.model.items.iter().find_map(|item| match item {
            SidebarItem::Header(name)
                if self.project_id_for_header(name).as_deref() == Some(header_project) =>
            {
                Some(name.clone())
            }
            _ => None,
        });
        if let Some(header) = header {
            self.collapsed.remove(&header);
        }
        if child_parent.is_some() {
            self.expanded_worktrees.insert(group_id);
        }
        let item = SidebarItem::Session(index);
        self.select_item_impl(&item, mark_read);
        true
    }

    pub fn open_recent_activity(&mut self) {
        self.modal = None;
        self.settings = None;
        self.selected_archive = None;
        self.selected_recent = Some(0);
        self.terminal_focus = false;
        let last = self.recent_activity_entries().len().saturating_sub(1);
        if let Some(selected) = self.selected_recent.as_mut() {
            *selected = (*selected).min(last);
        }
    }

    pub fn close_recent_activity(&mut self) {
        self.selected_recent = None;
    }

    /// Everything the palette can act on, in desktop order: sessions
    /// ("All recent" style — working first, then by recency), projects,
    /// preset launches for the selected session's project, then app
    /// commands. Archived sessions are held back for the
    /// nothing-else-matched case.
    fn palette_items(&self) -> (Vec<palette::Item>, Vec<palette::Item>) {
        let mut items = Vec::new();
        let mut archived = Vec::new();
        let mut rows: Vec<&SessionRow> = self.model.rows.iter().collect();
        // Same lifecycle ordering as a sidebar group set to Recently
        // updated. The rescan already snapshotted each row, so palette
        // keystrokes never stat Session files and reading never moves a row.
        rows.sort_by(|left, right| sessions::compare_recent(left, right));
        for row in rows {
            let item = palette::Item {
                kind: palette::Kind::Session,
                title: row.label.clone(),
                subtitle: if row.archived {
                    format!("Archived · {}", self.project_name_for(&row.id))
                } else {
                    self.project_name_for(&row.id)
                },
                keywords: row.command.clone(),
                icon_command: row.presentation_command().to_string(),
                action: palette::Action::SelectSession(row.id.clone()),
            };
            if row.archived {
                archived.push(item);
            } else {
                items.push(item);
            }
        }
        for item in &self.model.items {
            if let SidebarItem::Header(name) = item {
                items.push(palette::Item {
                    kind: palette::Kind::Project,
                    title: name.clone(),
                    subtitle: String::new(),
                    keywords: name.clone(),
                    icon_command: String::new(),
                    action: palette::Action::SelectProject(name.clone()),
                });
            }
        }
        let project = self
            .selected_session()
            .map(|s| self.project_name_for(&s.id))
            .unwrap_or_default();
        let mut launches = vec![("Terminal".to_string(), String::new())];
        launches.extend(sessions::fallback_presets(self.overlay.as_ref()));
        for (label, command) in launches {
            items.push(palette::Item {
                kind: palette::Kind::Launch,
                title: format!("New session: {label}"),
                subtitle: format!("in {project}"),
                keywords: command.clone(),
                icon_command: command.clone(),
                action: palette::Action::Launch(command),
            });
        }
        for (title, subtitle, keywords, action) in [
            (
                "New Terminal",
                "n",
                "shell blank",
                palette::Action::NewTerminal(String::new()),
            ),
            (
                "Settings",
                ",",
                "preferences presets access mobile",
                palette::Action::OpenSettings,
            ),
            (
                "Fold / unfold projects",
                "-",
                "collapse expand",
                palette::Action::ToggleFold,
            ),
        ] {
            items.push(palette::Item {
                kind: palette::Kind::Command,
                title: title.into(),
                subtitle: subtitle.into(),
                keywords: keywords.into(),
                icon_command: String::new(),
                action,
            });
        }
        (items, archived)
    }

    /// The unfiltered palette, sectioned (caption, rows): every working
    /// session (any project — a job you kicked off elsewhere still matters),
    /// every unread-finished session (any project, the popover's blue-dot
    /// group), then the current project's remaining sessions so keyboard
    /// nav stays close to home. Idle sessions in other projects only
    /// surface by typing; a "projects" section switches project instead.
    /// Launches/commands close the list under an empty caption (drawn as a
    /// bare divider). Empty sections are dropped.
    pub fn palette_sections(&self) -> Vec<(String, Vec<palette::Item>)> {
        let (items, _) = self.palette_items();
        let current_name = self
            .selected_session()
            .map(|s| self.project_name_for(&s.id));
        let mut active = Vec::new();
        let mut recent = Vec::new();
        let mut current = Vec::new();
        let mut projects = Vec::new();
        let mut actions = Vec::new();
        for item in items {
            match &item.action {
                palette::Action::SelectSession(id) => {
                    let Some(row) = self.model.rows.iter().find(|r| r.id == *id) else {
                        continue;
                    };
                    if matches!(
                        row.status,
                        sessions::Status::Starting | sessions::Status::Busy
                    ) {
                        active.push(item);
                    } else if self.unread_ids.contains(&row.id) {
                        recent.push(item);
                    } else if current_name.is_none()
                        || current_name.as_deref() == Some(&self.project_name_for(&row.id))
                    {
                        current.push(item);
                    }
                }
                palette::Action::SelectProject(name) => {
                    if current_name.as_deref() != Some(name.as_str()) {
                        projects.push(item);
                    }
                }
                _ => actions.push(item),
            }
        }
        let home = current_name.unwrap_or_else(|| "sessions".into());
        [
            ("active".to_string(), active),
            ("recent".to_string(), recent),
            (home, current),
            ("projects".to_string(), projects),
            (String::new(), actions),
        ]
        .into_iter()
        .filter(|(_, rows)| !rows.is_empty())
        .collect()
    }

    /// Palette rows for a query: unfiltered shows the tiered sections
    /// flattened, otherwise fuzzy-ranked over EVERY item (other projects'
    /// idle sessions included), with archived appended only when nothing
    /// else matched.
    pub fn palette_matches(&self, query: &str) -> Vec<palette::Item> {
        const MAX: usize = 40;
        let trimmed = query.trim();
        if trimmed.is_empty() {
            return self
                .palette_sections()
                .into_iter()
                .flat_map(|(_, rows)| rows)
                .take(MAX)
                .collect();
        }
        let (items, archived) = self.palette_items();
        let mut scored: Vec<(palette::Item, i32)> = items
            .into_iter()
            .filter_map(|item| palette::best_score(trimmed, &item).map(|s| (item, s)))
            .collect();
        scored.sort_by(|a, b| b.1.cmp(&a.1));
        if scored.is_empty() {
            let mut fallback: Vec<(palette::Item, i32)> = archived
                .into_iter()
                .filter_map(|item| palette::best_score(trimmed, &item).map(|s| (item, s)))
                .collect();
            fallback.sort_by(|a, b| b.1.cmp(&a.1));
            return fallback.into_iter().take(MAX).map(|(i, _)| i).collect();
        }
        scored.into_iter().take(MAX).map(|(i, _)| i).collect()
    }

    /// Is any session under this project header working? Drives the
    /// header shimmer, so a collapsed group still shows life. Worktree
    /// folder rows under the header count too — their sessions are the
    /// group's sessions even while the folder (or the whole group) is
    /// folded shut.
    pub fn group_is_busy(&self, header: &str) -> bool {
        let mut current: Option<&str> = None;
        for item in &self.model.items {
            match item {
                SidebarItem::Header(name) => current = Some(name),
                SidebarItem::Session(i) if current == Some(header) => {
                    if matches!(self.model.rows[*i].status, Status::Busy | Status::Starting) {
                        return true;
                    }
                }
                SidebarItem::WorktreeHeader { project_id, .. } if current == Some(header) => {
                    if self.worktree_is_busy(project_id) {
                        return true;
                    }
                }
                _ => {}
            }
        }
        false
    }

    /// Any session currently working — drives the sidebar's global spinner.
    pub fn any_busy(&self) -> bool {
        self.model
            .rows
            .iter()
            .any(|r| matches!(r.status, Status::Busy | Status::Starting))
    }

    pub fn selected_session(&self) -> Option<&SessionRow> {
        let id = self.selected_id.as_deref()?;
        self.model.rows.iter().find(|r| r.id == id)
    }

    /// Union of detected local-site URLs across every live session in the
    /// selected session's project family (top-level project plus its
    /// groups/worktrees) — the dev server usually runs in one session while
    /// the user watches another, so the preview chip is project-scoped.
    pub fn local_site_urls(&self) -> Vec<String> {
        let Some(session) = self.selected_session() else {
            return Vec::new();
        };
        // Child project → parent, from the rendered folder rows (the only
        // place the TUI's sidebar model records the relationship).
        let mut parent_of = std::collections::HashMap::new();
        for item in &self.model.items {
            if let SidebarItem::WorktreeHeader {
                project_id, parent, ..
            } = item
            {
                parent_of.insert(project_id.as_str(), parent.as_str());
            }
        }
        let root_of = |id: &str| -> String {
            let mut current = id;
            let mut hops = 0;
            while let Some(parent) = parent_of.get(current) {
                current = parent;
                hops += 1;
                if hops >= 8 {
                    break;
                }
            }
            current.to_string()
        };
        let family_root = root_of(&session.project_id);
        let mut urls = Vec::new();
        for row in &self.model.rows {
            if !row.running
                || row.detected_local_urls.is_empty()
                || root_of(&row.project_id) != family_root
            {
                continue;
            }
            urls.extend(row.detected_local_urls.iter().cloned());
        }
        // One row per server: deep links collapse into the parent URL when
        // one exists for the same origin, then each survivor must pass the
        // current-rules probe.
        unpeel_core::local_urls::dedupe_by_origin(&urls)
            .into_iter()
            .filter(|url| self.local_url_verdict(url))
            .collect()
    }

    /// Toast newly live local sites for the selected project (mirrors the
    /// desktop's phone-connected toast) and re-arm ones that dropped out so
    /// a dev-server restart announces again. Called once per rescan tick.
    pub fn announce_new_local_urls(&mut self) {
        let live: HashSet<String> = self.local_site_urls().into_iter().collect();
        self.announced_local_urls.retain(|url| live.contains(url));
        for url in &live {
            if self.announced_local_urls.insert(url.clone()) {
                self.info = Some(format!("{url} is running"));
            }
        }
    }

    /// Current-rules verdict for one manifest URL, from the async cache. An
    /// unknown URL kicks a background probe and stays hidden until it passes
    /// (the next 1s tick paints it in); verdicts refresh every few seconds
    /// so a server that dies — or starts working — converges quickly.
    fn local_url_verdict(&self, url: &str) -> bool {
        const TTL: std::time::Duration = std::time::Duration::from_secs(5);
        let now = std::time::Instant::now();
        let cached = self.local_url_verdicts.lock().unwrap().get(url).copied();
        let fresh = cached.is_some_and(|(_, at)| now.duration_since(at) < TTL);
        if !fresh {
            let mut in_flight = self.local_url_checks_in_flight.lock().unwrap();
            if in_flight.insert(url.to_string()) {
                let url = url.to_string();
                let verdicts = std::sync::Arc::clone(&self.local_url_verdicts);
                let in_flight = std::sync::Arc::clone(&self.local_url_checks_in_flight);
                std::thread::spawn(move || {
                    let ok = unpeel_core::local_urls::url_is_openable_site(&url);
                    verdicts
                        .lock()
                        .unwrap()
                        .insert(url.clone(), (ok, std::time::Instant::now()));
                    in_flight.lock().unwrap().remove(&url);
                });
            }
        }
        cached.map(|(ok, _)| ok).unwrap_or(false)
    }

    fn move_selection(&mut self, delta: isize) {
        self.move_selection_impl(delta, true);
    }

    /// `move_selection` without the local read-receipt write — the remote
    /// Controller scope's selection path (see `select_item_silent`).
    pub(crate) fn move_selection_silent(&mut self, delta: isize) {
        self.move_selection_impl(delta, false);
    }

    fn move_selection_impl(&mut self, delta: isize, mark_read: bool) {
        let visible = self.visible_items();
        let session_positions: Vec<usize> = visible
            .iter()
            .enumerate()
            .filter_map(|(pos, item)| {
                matches!(
                    item,
                    SidebarItem::Session(_)
                        | SidebarItem::WorktreeHeader {
                            is_group: false,
                            ..
                        }
                        | SidebarItem::NewSession { .. }
                        | SidebarItem::AddProject
                )
                .then_some(pos)
            })
            .collect();
        if session_positions.is_empty() {
            return;
        }
        let current = self.selected_visible_pos();
        let current_rank = current
            .and_then(|pos| session_positions.iter().position(|&p| p == pos))
            .unwrap_or(0);
        let next_rank =
            (current_rank as isize + delta).clamp(0, session_positions.len() as isize - 1) as usize;
        let item = visible[session_positions[next_rank]].clone();
        drop(visible);
        self.select_item_impl(&item, mark_read);
    }

    /// Free-scroll the sidebar viewport, leaving the selection alone.
    pub fn scroll_sidebar(&mut self, delta: isize) {
        let max = self.visible_items().len().saturating_sub(1);
        let next = self.sidebar_scroll as isize + delta;
        self.sidebar_scroll = next.clamp(0, max as isize) as usize;
        // Whatever the viewport shows now is deliberate: don't let the next
        // frame's reveal drag it back to the selection.
        self.last_selection_key = self.selection_key();
    }

    /// Identity of the current selection — not its position, which shifts
    /// whenever the list changes underneath it.
    fn selection_key(&self) -> String {
        match (
            &self.selected_worktree_folder,
            &self.selected_archive,
            &self.selected_new_session,
            &self.selected_id,
        ) {
            (Some(worktree), ..) => format!("w:{worktree}"),
            (_, Some((group, _)), ..) => format!("a:{group}"),
            (_, _, Some(project), _) => format!("n:{project}"),
            (_, _, _, Some(id)) => format!("s:{id}"),
            _ => String::new(),
        }
    }

    /// Bound the viewport, and reveal the selection **only when it moved**.
    /// Revealing every frame would make free scrolling impossible: the wheel
    /// would scroll and the next frame would snap straight back.
    fn clamp_scroll(&mut self, viewport_height: usize) {
        let mut header_pin: Option<usize> = None;
        let visible = self.visible_items();
        let total = visible.len();
        let max_scroll = total.saturating_sub(viewport_height);
        let key = self.selection_key();
        let selection_moved = key != self.last_selection_key;
        if let Some(pos) = self.selected_visible_pos().filter(|_| selection_moved) {
            let mut scroll = self.sidebar_scroll;
            if pos < scroll {
                scroll = pos;
            } else if viewport_height > 0 && pos >= scroll + viewport_height {
                scroll = pos + 1 - viewport_height;
            }
            header_pin = Some(scroll);
            // Keep the group's title on screen when its first row is at the
            // top edge — otherwise scrolling up lands on a session whose
            // project header sits one line above the viewport, with no way
            // to bring it back (headers aren't selectable).
            let header_above =
                pos > 0 && matches!(visible.get(pos - 1), Some(SidebarItem::Header(_)));
            if header_above && header_pin == Some(pos) {
                header_pin = Some(pos - 1);
            }
        }
        drop(visible);
        self.last_selection_key = key;
        if let Some(pin) = header_pin {
            self.sidebar_scroll = pin;
        }
        self.sidebar_scroll = self.sidebar_scroll.min(max_scroll);
    }
}

/// What `draw_sidebar` should paint this frame.
/// The default selection: the first session listed OUTSIDE a worktree
/// folder. Folders default collapsed, so picking the raw first Session item
/// could select (and preview) a row the sidebar isn't even showing. Falls
/// back to any session when only worktree sessions exist.
fn first_listed_session(model: &SidebarModel) -> Option<String> {
    let worktrees: std::collections::HashSet<&str> = model
        .items
        .iter()
        .filter_map(|item| match item {
            SidebarItem::WorktreeHeader { project_id, .. } => Some(project_id.as_str()),
            _ => None,
        })
        .collect();
    model
        .items
        .iter()
        .find_map(|item| match item {
            SidebarItem::Session(i) if !worktrees.contains(model.rows[*i].group_id.as_str()) => {
                Some(model.rows[*i].id.clone())
            }
            _ => None,
        })
        .or_else(|| {
            model.items.iter().find_map(|item| match item {
                SidebarItem::Session(i) => Some(model.rows[*i].id.clone()),
                _ => None,
            })
        })
}

fn local_replacement_group_id(row: &SessionRow) -> &str {
    if row.group_id.is_empty() {
        &row.project_id
    } else {
        &row.group_id
    }
}

fn local_replacement_runtime_id(row: &SessionRow) -> Option<String> {
    row.active_runtime_id
        .clone()
        .or_else(|| runtime_presentation::legacy_slug(&row.command).map(str::to_owned))
}

fn local_replacement_selection_intent(
    source: &SessionRow,
    rows: &[SessionRow],
) -> PendingLocalReplacementSelection {
    PendingLocalReplacementSelection {
        source_id: source.id.clone(),
        group_id: local_replacement_group_id(source).to_owned(),
        created_at: source.created_at,
        runtime_id: local_replacement_runtime_id(source),
        cwd: (!source.cwd.is_empty()).then(|| source.cwd.clone()),
        baseline_session_ids: rows.iter().map(|row| row.id.clone()).collect(),
        rescans_remaining: LOCAL_REPLACEMENT_RESCAN_OBSERVATIONS,
    }
}

fn local_replacement_selection_resolution(
    pending: &PendingLocalReplacementSelection,
    rows: &[SessionRow],
) -> LocalReplacementSelectionResolution {
    let source_still_exists = rows.iter().any(|row| row.id == pending.source_id);
    let candidates: Vec<&SessionRow> = rows
        .iter()
        .filter(|row| {
            row.running
                && !row.archived
                && local_replacement_group_id(row) == pending.group_id
                && row.created_at == pending.created_at
                && pending.cwd.as_ref().is_none_or(|cwd| row.cwd == *cwd)
                && !pending.baseline_session_ids.contains(&row.id)
                && pending.runtime_id.as_ref().is_none_or(|runtime_id| {
                    local_replacement_runtime_id(row).as_ref() == Some(runtime_id)
                })
        })
        .collect();
    if candidates.len() > 1 {
        return LocalReplacementSelectionResolution::Cancel;
    }
    if !source_still_exists {
        if let Some(candidate) = candidates.first() {
            return LocalReplacementSelectionResolution::Select(candidate.id.clone());
        }
    }
    if pending.rescans_remaining <= 1 {
        return LocalReplacementSelectionResolution::Cancel;
    }
    let mut waiting = pending.clone();
    waiting.rescans_remaining -= 1;
    LocalReplacementSelectionResolution::Wait(waiting)
}

fn resolve_local_replacement_selection(
    state: &mut LocalReplacementSelectionState,
    rows: &[SessionRow],
) -> Option<String> {
    let pending = state.pending.take()?;
    match local_replacement_selection_resolution(&pending, rows) {
        LocalReplacementSelectionResolution::Wait(updated) => {
            state.pending = Some(updated);
            None
        }
        LocalReplacementSelectionResolution::Select(id) => {
            state.suppress_default = false;
            Some(id)
        }
        LocalReplacementSelectionResolution::Cancel => None,
    }
}

#[cfg(test)]
mod local_replacement_selection_tests {
    use super::*;

    fn row(id: &str, command: &str, created_at: u64, running: bool, cwd: &str) -> SessionRow {
        SessionRow {
            id: id.into(),
            project_id: "root".into(),
            label: id.into(),
            command: command.into(),
            active_runtime_id: None,
            resume_available: !running,
            resume_agent_available: false,
            running,
            status: if running {
                Status::Starting
            } else {
                Status::Exited
            },
            created_at,
            pinned: false,
            archived: false,
            activity_at: created_at,
            group_id: "worktree".into(),
            unread: false,
            cwd: cwd.into(),
            detected_local_urls: Vec::new(),
        }
    }

    #[test]
    fn local_replacement_requires_one_new_exact_launch_identity() {
        let source = row("source", "claude", 42, false, "/worktree");
        let decoy = row("decoy", "claude", 99, true, "/worktree");
        let baseline = vec![source.clone(), decoy.clone()];
        let pending = local_replacement_selection_intent(&source, &baseline);

        assert!(matches!(
            local_replacement_selection_resolution(&pending, std::slice::from_ref(&decoy)),
            LocalReplacementSelectionResolution::Wait(_)
        ));

        let wrong_runtime = row(
            "wrong-runtime",
            "codex resume thread",
            42,
            true,
            "/worktree",
        );
        assert!(matches!(
            local_replacement_selection_resolution(&pending, &[decoy.clone(), wrong_runtime]),
            LocalReplacementSelectionResolution::Wait(_)
        ));

        let exact = row(
            "replacement",
            "claude --resume conversation",
            42,
            true,
            "/worktree",
        );
        match local_replacement_selection_resolution(&pending, &[decoy, exact]) {
            LocalReplacementSelectionResolution::Select(id) => {
                assert_eq!(id, "replacement")
            }
            _ => panic!("one exact replacement must be selected"),
        }
    }

    #[test]
    fn ambiguous_local_replacement_cancels_adoption_permanently() {
        let source = row("source", "claude", 42, false, "/worktree");
        let mut state = LocalReplacementSelectionState::default();
        state.begin(local_replacement_selection_intent(
            &source,
            std::slice::from_ref(&source),
        ));
        let a = row("a", "claude --resume one", 42, true, "/worktree");
        let b = row("b", "claude --resume two", 42, true, "/worktree");

        assert!(resolve_local_replacement_selection(&mut state, &[a.clone(), b]).is_none());
        assert!(state.pending.is_none());
        assert!(state.suppress_default);
        assert!(resolve_local_replacement_selection(&mut state, &[a]).is_none());
        assert!(state.suppress_default);
    }

    #[test]
    fn expired_local_replacement_keeps_arbitrary_fallback_suppressed() {
        let source = row("source", "claude", 42, false, "/worktree");
        let mut pending =
            local_replacement_selection_intent(&source, std::slice::from_ref(&source));
        pending.rescans_remaining = 1;
        let mut state = LocalReplacementSelectionState::default();
        state.begin(pending);

        assert!(resolve_local_replacement_selection(&mut state, &[]).is_none());
        assert!(state.pending.is_none());
        assert!(state.suppress_default);
    }
}

pub struct SidebarRender {
    pub items: Vec<SidebarItem>,
    /// Position of the selected row within `items`.
    pub selected: Option<usize>,
    /// Inclusive range of the block being dragged, in its previewed home.
    pub carried: Option<(usize, usize)>,
}

/// The app is preferred for every verb (overlays, archive bookkeeping, UI
/// carry-over); when it's unreachable — or its build predates a route — the
/// shared core ops in `unpeel_core::session_ops` run the same lifecycle
/// against the on-disk contract, so the TUI works as a standalone way to
/// run Unpeel.
fn bridge_unavailable(err: &str) -> bool {
    err.contains("not reachable")
        || err.contains("auth token")
        || err.contains("predate this route")
}

fn bridge_effect_outcome_unknown(err: &str) -> bool {
    err.contains("bridge response is unresolved")
}

fn run_verb(
    app: &mut App,
    verb: Verb,
    session_id: String,
    grid: (u16, u16),
    results: &mpsc::Sender<VerbOutcome>,
) {
    if matches!(verb, Verb::Resume) {
        if let Some(pending) = app
            .model
            .rows
            .iter()
            .find(|row| row.id == session_id)
            .map(|source| local_replacement_selection_intent(source, &app.model.rows))
        {
            // Latch before the bridge call leaves this thread. Native can
            // replace a very small stopped Session before the next rescan.
            app.replacement_selection.begin(pending);
        }
    }
    let own_port = app.hook_port;
    let results = results.clone();
    app.info = None;
    app.in_flight = Some(InFlight {
        label: match verb {
            Verb::Stop => "stopping".into(),
            Verb::Resume => "resuming".into(),
            Verb::ResumeAgent => "resuming agent".into(),
            Verb::Remove => "removing".into(),
            Verb::Pin(true) => "pinning".into(),
            Verb::Pin(false) => "unpinning".into(),
        },
    });
    std::thread::spawn(move || {
        let mut replacement_not_applied = None;
        let message = match verb {
            Verb::Stop => match bridge::archive_session(own_port, &session_id) {
                Ok(()) => "stopped and archived".into(),
                Err(err) if bridge_unavailable(&err) => {
                    match unpeel_core::session_ops::archive_session(&session_id) {
                        Ok(()) => "stopped and archived".into(),
                        Err(e) => e,
                    }
                }
                Err(err) => err,
            },
            Verb::Resume => match bridge::restart_session(own_port, &session_id) {
                Ok(()) => "resuming session".into(),
                Err(err) if bridge_unavailable(&err) => {
                    match unpeel_core::session_ops::resume_session(
                        &session_id,
                        own_port,
                        grid.0,
                        grid.1,
                    ) {
                        Ok(new_id) => {
                            let _ = results.send(VerbOutcome {
                                message: "resuming session (standalone)".into(),
                                select: Some(new_id),
                                replacement_not_applied: None,
                                clipboard: None,
                            });
                            return;
                        }
                        Err(e) => {
                            replacement_not_applied = Some(session_id.clone());
                            e
                        }
                    }
                }
                Err(err) if bridge_effect_outcome_unknown(&err) => err,
                Err(err) => {
                    replacement_not_applied = Some(session_id.clone());
                    err
                }
            },
            Verb::ResumeAgent => match unpeel_core::session_ops::resume_agent(&session_id) {
                Ok(()) => "resuming agent".into(),
                Err(err) => err,
            },
            Verb::Remove => match bridge::close_session(own_port, &session_id) {
                Ok(()) => "removed".into(),
                Err(err) if bridge_unavailable(&err) => {
                    match unpeel_core::session_ops::remove_session(&session_id) {
                        Ok(()) => "removed".into(),
                        Err(e) => e,
                    }
                }
                Err(err) => err,
            },
            Verb::Pin(pinned) => match bridge::set_pinned(own_port, &session_id, pinned) {
                Ok(()) if pinned => "pinned".into(),
                Ok(()) => "unpinned".into(),
                Err(err) if bridge_unavailable(&err) => {
                    match set_pin_in_app_state(&session_id, pinned) {
                        Ok(()) if pinned => "pinned".into(),
                        Ok(()) => "unpinned".into(),
                        Err(e) => e,
                    }
                }
                Err(err) => err,
            },
        };
        let _ = results.send(VerbOutcome {
            message,
            select: None,
            replacement_not_applied,
            clipboard: None,
        });
    });
}

/// Resolve and render a provider conversation without blocking the TUI's
/// input/render loop. The shared formatter keeps this byte-for-byte aligned
/// with desktop and phone "Copy transcript", including the app-wide content
/// toggles in `app-state.json`.
fn copy_transcript(
    app: &mut App,
    session_id: String,
    entries: usize,
    results: &mpsc::Sender<VerbOutcome>,
) {
    let results = results.clone();
    app.info = None;
    app.in_flight = Some(InFlight {
        label: "copying transcript".into(),
    });
    std::thread::spawn(move || {
        let rendered = (|| {
            let markdown = unpeel_core::transcripts::read_session_transcript_markdown(
                &session_id,
                Some(entries),
                false,
            )?;
            let markdown = markdown.trim().to_string();
            if markdown.is_empty() {
                return Err("this session has no readable conversation transcript yet".into());
            }
            Ok(markdown)
        })();
        let outcome = match rendered {
            Ok(markdown) => VerbOutcome {
                message: "transcript copied".into(),
                select: None,
                replacement_not_applied: None,
                clipboard: Some(markdown),
            },
            Err(message) => VerbOutcome {
                message,
                select: None,
                replacement_not_applied: None,
                clipboard: None,
            },
        };
        let _ = results.send(outcome);
    });
}

/// Set the clipboard owned by the terminal controlling this TUI. OSC 52 is
/// intentional: for a headless host reached over SSH, `pbcopy`/`wl-copy`
/// would target the host (or no display) instead of the user's controller.
fn write_terminal_clipboard(text: &str) -> io::Result<()> {
    use base64::Engine;
    use std::io::Write;

    let encoded = base64::engine::general_purpose::STANDARD.encode(text.as_bytes());
    let mut stdout = io::stdout().lock();
    stdout.write_all(b"\x1b]52;c;")?;
    stdout.write_all(encoded.as_bytes())?;
    stdout.write_all(b"\x07")?;
    stdout.flush()
}

/// Spawn a fresh session from the preset picker (standalone core spawn —
/// hosts are app-independent, and a running app discovers the manifest on
/// its next rescan).
fn spawn_new_session(
    app: &mut App,
    command: String,
    cwd: String,
    project_id: String,
    term_w: u16,
    term_h: u16,
    results: &mpsc::Sender<VerbOutcome>,
) {
    // Explicit creation cancels any older fail-closed replacement adoption;
    // the new spawn returns its own exact id through `pending_select`.
    app.replacement_selection.clear();
    let own_port = app.hook_port;
    let (cols, rows) = preview_grid(app, term_w, term_h);
    let results = results.clone();
    app.info = None;
    app.in_flight = Some(InFlight {
        label: "starting session".into(),
    });
    std::thread::spawn(move || {
        let label = if command.trim().is_empty() {
            "Terminal".to_string()
        } else {
            command.clone()
        };
        let session = unpeel_core::state::SessionInfo {
            id: String::new(),
            project_id,
            label,
            custom_title: false,
            command,
            created_at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64,
            tag_id: None,
            worktree_path: None,
            worktree_branch: None,
            parent_session_id: None,
            spawned_by: None,
            role: None,
            task: None,
        };
        let outcome =
            match unpeel_core::session_ops::spawn_session(session, &cwd, own_port, cols, rows) {
                Ok(new_id) => VerbOutcome {
                    message: "new session started".into(),
                    select: Some(new_id),
                    replacement_not_applied: None,
                    clipboard: None,
                },
                Err(e) => VerbOutcome {
                    message: e,
                    select: None,
                    replacement_not_applied: None,
                    clipboard: None,
                },
            };
        let _ = results.send(outcome);
    });
}

fn request_verb(app: &mut App, verb: Verb, grid: (u16, u16), results: &mpsc::Sender<VerbOutcome>) {
    if let Some(id) = gate_verb(app, verb, grid) {
        run_verb(app, verb, id, grid, results);
    }
}

/// Resolve `r` from the selected Session's Host-advertised lifecycle shape.
/// A live Session can Resume Agent only after its managed runtime has returned
/// to the owned shell; an exited Session can only Resume by recreating its
/// hosted terminal. Active runtimes and live blank/passively observed shells
/// deliberately do neither.
fn request_restart(app: &mut App, grid: (u16, u16), results: &mpsc::Sender<VerbOutcome>) {
    let verb = selected_restart_verb(app);
    match verb {
        Some(verb) => request_verb(app, verb, grid, results),
        None => {
            app.info = app
                .selected_session()
                .map(|session| resume_unavailable_message(session).to_owned())
        }
    }
}

pub(crate) fn resume_unavailable_message(session: &SessionRow) -> &'static str {
    if !session.running {
        return "this session cannot be resumed";
    }
    if !unpeel_core::resume::can_resume(&session.command) {
        return "this live terminal has no managed agent to resume";
    }
    if session.active_runtime_id.is_some() {
        return "the managed agent is still active";
    }
    "Resume Agent is unavailable for this live Host"
}

pub(crate) fn selected_restart_verb(app: &App) -> Option<Verb> {
    app.selected_session().and_then(|session| {
        if session.resume_agent_available {
            Some(Verb::ResumeAgent)
        } else if session.resume_available {
            Some(Verb::Resume)
        } else {
            None
        }
    })
}

/// Shared sidebar-verb gate: precondition checks and the confirm dialog,
/// identical for the local loop and the remote Controller scope. Returns the
/// session to act on **now**; `None` means the verb was denied (`app.info`
/// explains) or a `Confirm` was posted for the caller's loop to execute on
/// `y` through its own backend.
pub(crate) fn gate_verb(app: &mut App, verb: Verb, grid: (u16, u16)) -> Option<String> {
    let session = app.selected_session()?;
    let id = session.id.clone();
    let label = session.label.clone();
    // Verb gating mirrors the sidebar: stop applies to live sessions,
    // remove to stopped ones.
    match verb {
        Verb::Stop if !session.running => {
            app.info = Some("session is already stopped".into());
            return None;
        }
        Verb::Remove if session.running => {
            app.info = Some("stop it first (s), then remove".into());
            return None;
        }
        Verb::Resume if !session.resume_available => {
            app.info = Some(
                if session.running {
                    "this live terminal cannot be resumed"
                } else {
                    "this session cannot be resumed"
                }
                .into(),
            );
            return None;
        }
        Verb::ResumeAgent if !session.resume_agent_available => {
            app.info = Some(resume_unavailable_message(session).into());
            return None;
        }
        _ => {}
    }
    let needs_confirm = match verb {
        Verb::Remove => true,
        Verb::Stop => matches!(session.status, Status::Busy | Status::Attention),
        _ => false,
    };
    if needs_confirm {
        let action = match verb {
            Verb::Remove => "Remove",
            _ => "Stop and archive",
        };
        app.confirm = Some(Confirm {
            verb,
            session_id: id,
            grid,
            prompt: format!("{action} \"{label}\""),
        });
        None
    } else {
        Some(id)
    }
}

fn preview_grid(app: &App, term_w: u16, term_h: u16) -> (u16, u16) {
    (
        term_w.saturating_sub(app.sidebar_width + 2).max(4),
        // Borders only: the status line overlays the last row rather than
        // taking one, so the pane runs to the bottom of the window.
        term_h.saturating_sub(2).max(2),
    )
}

/// Selecting a session takes its grid: resize the PTY to the preview pane
/// (skipped when it already matches) so the terminal lays out correctly the
/// moment it opens — same behavior as opening a session in the desktop app.
fn resize_selected_to_pane(app: &App, snapshots: &SnapshotService, term_w: u16, term_h: u16) {
    let Some(session) = app.selected_session() else {
        return;
    };
    if !session.running {
        return;
    }
    // A phone owns this session's grid: do NOT resize the shared PTY — that
    // is the width-fight (the phone fits to its grid, we shove it back to
    // the pane, repeat). The preview letterboxes the phone's true grid via
    // the virtual snapshot path instead. Reclaiming is explicit only
    // (entering terminal focus), which clears this and resizes directly.
    if app.mobile_resized(&session.id) {
        return;
    }
    let (cols, rows) = preview_grid(app, term_w, term_h);
    if let Some(snapshot) = snapshots.get(&session.id) {
        if snapshot.cols == cols && snapshot.rows == rows {
            return;
        }
    }
    // Taking the grid back supersedes any phone resize.
    if let Ok(mut guard) = app.mobile_resizes.lock() {
        guard.remove(&session.id);
    }
    let dir = session.dir();
    std::thread::spawn(move || {
        let _ = control::send_resize(&dir, cols, rows);
    });
}

/// Pseudo-command for the picker's footer row: ⏎ opens Settings ▸ Presets
/// instead of spawning anything.
pub const MANAGE_PRESETS_COMMAND: &str = "__manage_presets__";

fn activate_menu_action(app: &mut App, action: MenuAction) {
    app.modal = None;
    match action {
        MenuAction::OpenSettings => {
            app.selected_recent = None;
            app.settings = Some((0, 0));
        }
        MenuAction::OpenKeybindings => app.modal = Some(Modal::Help),
        MenuAction::OpenCommandPalette => {
            app.modal = Some(Modal::Palette {
                query: String::new(),
                selected: 0,
            });
        }
        MenuAction::Exit => app.exit_requested = true,
    }
}

/// Run a footer-menu row: close the menu and jump to what it points at.
fn activate_menu(app: &mut App, index: usize) {
    if let Some(item) = MENU_ITEMS.get(index) {
        activate_menu_action(app, item.action);
    } else {
        app.modal = None;
    }
}

/// The context-menu rows for a project, built for its state right now —
/// the desktop menu's shape, minus what has no TUI counterpart yet
/// (worktree creation, editors).
fn project_menu_items(app: &App, project_id: &str, name: &str) -> Vec<(String, CtxAction)> {
    let mut items = vec![("New session".to_string(), CtxAction::NewSession)];
    // Beside its sibling creation verb. Creatable only where the parent
    // record lives in the shared file: a child of an app-owned
    // (UserDefaults) project would reference a parent app-state.json
    // doesn't know, and normalize would orphan it top-level.
    if sessions::project_path(project_id).is_some() {
        items.push(("New group…".to_string(), CtxAction::NewGroup));
    }
    let running = app.running_ids_in_project(project_id).len();
    if running > 0 {
        items.push((format!("Stop all ({running})"), CtxAction::StopAll));
    }
    let archived = app.archived_count_in_project(project_id);
    if archived > 0 {
        items.push((format!("Archived ({archived})"), CtxAction::Archived));
    }
    let fold = if app.collapsed.contains(name) {
        "Expand"
    } else {
        "Collapse"
    };
    items.push((fold.to_string(), CtxAction::ToggleCollapse));
    // Colors live in the app's UserDefaults; no overlay (Linux, isolated workspaces)
    // means nowhere to read or write them.
    if app.overlay.is_some() {
        items.push(("Folder color ›".to_string(), CtxAction::FolderColor));
    }
    items.push(("Sort sessions ›".to_string(), CtxAction::SortSessions));
    if cfg!(target_os = "macos") && project_reveal_path(app, project_id).is_some() {
        items.push(("Reveal in Finder".to_string(), CtxAction::Reveal));
    }
    // Removable only where the record lives in the shared file — the app's
    // own (UserDefaults) projects have to be removed in the app, like the
    // desktop can't remove what it doesn't own either.
    if sessions::project_path(project_id).is_some() {
        items.push(("Remove project…".to_string(), CtxAction::RemoveProject));
    }
    items
}

/// The context-menu rows for a worktree folder row — the project menu's
/// verbs where they apply (new session in the worktree, stop all, its
/// archive, reveal), plus fold/unfold in place of the header collapse.
fn worktree_menu_items(
    app: &App,
    project_id: &str,
    parent_id: &str,
    is_group: bool,
) -> Vec<(String, CtxAction)> {
    let can_manage = sessions::project_path(project_id).is_some();
    let mut items = vec![("New session".to_string(), CtxAction::NewSession)];
    if is_group && can_manage {
        items.push(("Rename group…".to_string(), CtxAction::RenameGroup));
    }
    let running = app.running_ids_in_project(project_id).len();
    if running > 0 {
        items.push((format!("Stop all ({running})"), CtxAction::StopAll));
    }
    let archived = app.archived_count_in_project(project_id);
    if archived > 0 {
        items.push((format!("Archived ({archived})"), CtxAction::Archived));
    }
    let fold = if app.expanded_worktrees.contains(project_id) {
        "Collapse"
    } else {
        "Expand"
    };
    items.push((fold.to_string(), CtxAction::ToggleWorktreeFold));
    items.push(("Sort sessions ›".to_string(), CtxAction::SortSessions));
    if cfg!(target_os = "macos") && project_reveal_path(app, project_id).is_some() {
        items.push(("Reveal in Finder".to_string(), CtxAction::Reveal));
    }
    // Plain groups have their own verbs and safer removal semantics. Git
    // worktrees retain the existing project removal behavior.
    if can_manage {
        if is_group {
            items.push((
                "Remove group…".to_string(),
                CtxAction::RemoveGroup(parent_id.to_string()),
            ));
        } else {
            items.push(("Remove project…".to_string(), CtxAction::RemoveProject));
        }
    }
    items
}

/// The color submenu: Default plus the desktop's eight, current one ticked.
fn folder_color_items(app: &App, project_id: &str) -> Vec<(String, CtxAction)> {
    let current = app
        .overlay
        .as_ref()
        .and_then(|o| o.project_colors.get(project_id).cloned());
    let tick = |on: bool| if on { " ✓" } else { "" };
    let mut items = vec![(
        format!("Default{}", tick(current.is_none())),
        CtxAction::SetColor(None),
    )];
    for (raw, label) in FOLDER_COLORS {
        items.push((
            format!("{label}{}", tick(current.as_deref() == Some(*raw))),
            CtxAction::SetColor(Some(raw)),
        ));
    }
    items
}

/// The sort submenu: custom (the manual drag order, the default) or
/// recently updated (last activity), current one ticked. Date sort disables
/// re-ordering until switched back.
fn session_sort_items(project_id: &str) -> Vec<(String, CtxAction)> {
    let date_sorted = unpeel_core::session_ops::session_date_sorted(project_id);
    let tick = |on: bool| if on { " ✓" } else { "" };
    vec![
        (
            format!("Custom order{}", tick(!date_sorted)),
            CtxAction::SetSessionSort(false),
        ),
        (
            format!("Recently updated{}", tick(date_sorted)),
            CtxAction::SetSessionSort(true),
        ),
    ]
}

/// The context-menu rows for a session — the desktop's, trimmed to the
/// verbs that exist here (rename, pin, move-to, restart/resume, transcript,
/// archive, remove).
fn session_menu_items(app: &App, row: &SessionRow) -> Vec<(String, CtxAction)> {
    let mut items = vec![("Rename".to_string(), CtxAction::RenameSession)];
    items.push((
        if row.pinned { "Unpin" } else { "Pin" }.to_string(),
        CtxAction::TogglePin,
    ));
    // Only when there is somewhere to move to — a project with no group
    // or worktree folders has no destinations to offer.
    if !move_to_items(app, row).is_empty() {
        items.push(("Move to ›".to_string(), CtxAction::MoveTo));
    }
    if row.resume_agent_available {
        items.push(("Resume Agent".to_string(), CtxAction::RestartSession));
    } else if row.resume_available {
        items.push(("Resume".to_string(), CtxAction::RestartSession));
    }
    items.push(("Copy transcript ›".to_string(), CtxAction::CopyTranscript));
    items.push(("Copy session ID".to_string(), CtxAction::CopySessionId));
    if row.running {
        items.push(("Stop and archive".to_string(), CtxAction::StopSession));
    } else if !row.archived {
        items.push(("Archive".to_string(), CtxAction::ArchiveSession));
    }
    items.push((
        if row.running {
            "Remove session"
        } else {
            "Remove from list"
        }
        .to_string(),
        CtxAction::RemoveSession,
    ));
    items
}

/// The "Move to" submenu rows for a session: every plain organizational
/// group of the session's root project (minus wherever it already sits),
/// plus the root itself when the session currently renders inside a folder.
/// Git worktrees are intentionally not destinations: changing checkout needs
/// a restart/resume flow, not this display-only project override. Moving to
/// the session's own manifest project clears the override marker; anywhere
/// else writes it — so a marker only exists while it changes something.
fn move_to_items(app: &App, row: &SessionRow) -> Vec<(String, CtxAction)> {
    let current = if row.group_id.is_empty() {
        row.project_id.clone()
    } else {
        row.group_id.clone()
    };
    // Every folder row in the model: (child id, name, parent id). The
    // session's root is its current folder's parent — or the group itself
    // when it sits at the top level.
    let mut folders: Vec<(String, String, String, bool)> = Vec::new();
    let mut root = current.clone();
    for item in &app.model.items {
        if let SidebarItem::WorktreeHeader {
            project_id,
            parent,
            name,
            is_group,
            ..
        } = item
        {
            folders.push((project_id.clone(), name.clone(), parent.clone(), *is_group));
            if *project_id == current {
                root = parent.clone();
            }
        }
    }
    let action_for = |target: &str| {
        // Landing back on the manifest project is a marker removal, not a
        // marker pointing at where the session already belongs.
        if target == row.project_id {
            CtxAction::MoveToProject(None)
        } else {
            CtxAction::MoveToProject(Some(target.to_string()))
        }
    };
    let mut items = Vec::new();
    if current != root {
        let name = project_name_by_id(app, &root).unwrap_or_else(|| "project".into());
        items.push((format!("Move to {name}"), action_for(&root)));
    }
    for (id, name, parent, is_group) in folders {
        if is_group && parent == root && id != current {
            items.push((format!("Move to {name}"), action_for(&id)));
        }
    }
    items
}

/// A project's display name by id, from the shared file or the app overlay.
fn project_name_by_id(app: &App, project_id: &str) -> Option<String> {
    sessions::load_app_state()
        .and_then(|state| {
            state
                .projects
                .iter()
                .find(|p| p.id == project_id)
                .map(|p| p.name.clone())
        })
        .or_else(|| {
            app.overlay.as_ref().and_then(|o| {
                o.projects
                    .iter()
                    .find(|(id, _)| id == project_id)
                    .map(|(_, name)| name.clone())
            })
        })
}

/// Create a group: a child project of `parent_id` with the parent's path,
/// `is_folder`, and no worktree branch — straight into app-state.json,
/// which `edit()` announces to every other frontend.
fn add_group_to_app_state(parent_id: &str, name: &str) -> Result<(), String> {
    unpeel_core::app_state::edit(|state| {
        let projects = state
            .get_mut("projects")
            .and_then(|v| v.as_array_mut())
            .ok_or("app-state.json has no projects array")?;
        let parent_path = projects
            .iter()
            .find(|p| p.get("id").and_then(|v| v.as_str()) == Some(parent_id))
            .and_then(|p| p.get("path").and_then(|v| v.as_str()))
            .map(str::to_owned)
            .ok_or("this project is managed by the desktop app — create the group there")?;
        projects.push(serde_json::json!({
            "id": format!("tui-{}", uuid::Uuid::new_v4()),
            "name": name,
            "path": parent_path,
            "parent_project_id": parent_id,
            "is_folder": true,
        }));
        Ok(())
    })
}

/// Rename a shared-state plain group. Refuse project/worktree records even if
/// a stale menu somehow dispatches the action after the sidebar changed.
fn rename_group_in_app_state(project_id: &str, name: &str) -> Result<(), String> {
    unpeel_core::app_state::edit(|state| {
        let projects = state
            .get_mut("projects")
            .and_then(|v| v.as_array_mut())
            .ok_or("app-state.json has no projects array")?;
        let project = projects
            .iter_mut()
            .find(|p| p.get("id").and_then(|v| v.as_str()) == Some(project_id))
            .ok_or("this group is managed by the desktop app — rename it there")?;
        let is_group = project
            .get("parent_project_id")
            .and_then(|v| v.as_str())
            .is_some()
            && project
                .get("is_folder")
                .and_then(|v| v.as_bool())
                .unwrap_or(false)
            && project
                .get("worktree_branch")
                .and_then(|v| v.as_str())
                .is_none();
        if !is_group {
            return Err("only plain groups can be renamed here".into());
        }
        project["name"] = serde_json::Value::String(name.to_string());
        Ok(())
    })
}

/// Every hosted session currently filed under `project_id`, including rows
/// hidden by the recent-stopped window and already-archived rows omitted from
/// the app bridge payload. A valid override wins over the manifest project,
/// matching `scan_sidebar`.
fn session_ids_in_project(app: &App, project_id: &str) -> Vec<String> {
    let mut known = std::collections::HashSet::new();
    for item in &app.model.items {
        match item {
            SidebarItem::Header(name) => {
                if let Some(id) = app.project_id_for_header(name) {
                    known.insert(id);
                }
            }
            SidebarItem::WorktreeHeader { project_id, .. } => {
                known.insert(project_id.clone());
            }
            _ => {}
        }
    }

    let mut ids = Vec::new();
    if let Ok(entries) = std::fs::read_dir(unpeel_core::app_paths::app_sessions_root()) {
        for entry in entries.flatten() {
            let id = entry.file_name().to_string_lossy().into_owned();
            let Some(manifest) = unpeel_core::session_host::load_manifest(&id) else {
                continue;
            };
            let effective = unpeel_core::session_ops::project_override_marker(&id)
                .filter(|target| known.contains(target))
                .unwrap_or(manifest.session.project_id);
            if effective == project_id {
                ids.push(id);
            }
        }
    }
    ids.sort();
    ids
}

/// Where "Reveal in Finder" points: the shared file's path, or the app
/// overlay's for desktop-owned projects.
fn project_reveal_path(app: &App, project_id: &str) -> Option<String> {
    sessions::project_path(project_id).or_else(|| {
        app.overlay
            .as_ref()
            .and_then(|o| o.project_paths.get(project_id).cloned())
    })
}

/// Drop a project's record from the shared file. Plain-group callers must
/// first rehome/archive their sessions; top-level project removal retains its
/// existing forget-only behavior.
fn remove_project_from_app_state(project_id: &str) -> Result<(), String> {
    unpeel_core::app_state::edit(|state| {
        let projects = state
            .get_mut("projects")
            .and_then(|v| v.as_array_mut())
            .ok_or("app-state.json has no projects array")?;
        let before = projects.len();
        projects.retain(|p| p.get("id").and_then(|v| v.as_str()) != Some(project_id));
        if projects.len() == before {
            return Err("this project is managed by the desktop app — remove it there".into());
        }
        Ok(())
    })
}

/// Write a folder color into the app's UserDefaults — the same store the
/// desktop's color picker writes, so both UIs read one truth. The app
/// re-reads it on the state-bus ping (UnpeelStore.rescan).
fn set_project_folder_color(app: &mut App, project_id: &str, color: Option<&str>) {
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (project_id, color);
        app.info = Some("folder colors need the desktop app".into());
    }
    #[cfg(target_os = "macos")]
    {
        const DOMAIN: &str = "com.unpeel.native";
        const KEY: &str = "unpeel.native.projectFolderColors";
        let run = |args: &[&str]| {
            std::process::Command::new("defaults")
                .args(args)
                .output()
                .map(|o| o.status.success())
                .unwrap_or(false)
        };
        let ok = match color {
            Some(c) => run(&["write", DOMAIN, KEY, "-dict-add", project_id, c]),
            None => {
                // No per-key delete in `defaults`: rewrite the dict whole.
                let mut colors = app
                    .overlay
                    .as_ref()
                    .map(|o| o.project_colors.clone())
                    .unwrap_or_default();
                colors.remove(project_id);
                if colors.is_empty() {
                    // Deleting an already-absent key fails; that's still done.
                    run(&["delete", DOMAIN, KEY]);
                    true
                } else {
                    let mut args: Vec<String> =
                        vec!["write".into(), DOMAIN.into(), KEY.into(), "-dict".into()];
                    for (k, v) in &colors {
                        args.push(k.clone());
                        args.push(v.clone());
                    }
                    let refs: Vec<&str> = args.iter().map(String::as_str).collect();
                    run(&refs)
                }
            }
        };
        if !ok {
            app.info = Some("could not save the folder color".into());
            return;
        }
        // Chevron updates this frame; the ping tells the app (and any other
        // frontend) to re-read.
        if let Some(overlay) = app.overlay.as_mut() {
            match color {
                Some(c) => {
                    overlay
                        .project_colors
                        .insert(project_id.to_string(), c.to_string());
                }
                None => {
                    overlay.project_colors.remove(project_id);
                }
            }
        }
        unpeel_core::state_bus::announce(unpeel_core::state_bus::Change::AppState, app.hook_port);
    }
}

/// Run a context-menu row. The menu closed before this is called; a few
/// actions (submenus, the inline confirm) open a fresh one at the same
/// anchor.
fn activate_context_menu(
    app: &mut App,
    menu: ContextMenu,
    index: usize,
    term_w: u16,
    term_h: u16,
    results: &mpsc::Sender<VerbOutcome>,
) {
    let Some((_, action)) = menu.items.get(index) else {
        return;
    };
    let grid = preview_grid(app, term_w, term_h);
    match action {
        CtxAction::NewSession => {
            open_preset_picker_for(app, Some((menu.project_id, menu.name)));
        }
        CtxAction::StopAll => {
            let ids = app.running_ids_in_project(&menu.project_id);
            if ids.is_empty() {
                return;
            }
            let own_port = app.hook_port;
            let results = results.clone();
            app.info = None;
            app.in_flight = Some(InFlight {
                label: "stopping all".into(),
            });
            // Same verb "Stop" runs per session: archive via the app when
            // it's up, session_ops standalone — see run_verb.
            std::thread::spawn(move || {
                let mut stopped = 0usize;
                let mut failed: Option<String> = None;
                for id in &ids {
                    let outcome = match bridge::archive_session(own_port, id) {
                        Ok(()) => Ok(()),
                        Err(err) if bridge_unavailable(&err) => {
                            unpeel_core::session_ops::archive_session(id)
                        }
                        Err(err) => Err(err),
                    };
                    match outcome {
                        Ok(()) => stopped += 1,
                        Err(e) => failed = Some(e),
                    }
                }
                let message = match failed {
                    None => format!("stopped and archived {stopped}"),
                    Some(e) if stopped == 0 => e,
                    Some(e) => format!("stopped {stopped}, then: {e}"),
                };
                let _ = results.send(VerbOutcome {
                    message,
                    select: None,
                    replacement_not_applied: None,
                    clipboard: None,
                });
            });
        }
        CtxAction::Archived => {
            // Same landing as pressing `a` on the project's selection.
            app.archive_query.clear();
            app.selected_recent = None;
            app.selected_worktree_folder = None;
            app.selected_new_session = None;
            app.selected_archive = Some((menu.project_id, 0));
        }
        CtxAction::ToggleCollapse => {
            if !app.collapsed.remove(&menu.name) {
                app.collapsed.insert(menu.name);
            }
        }
        // Worktree folder rows fold by project id, not header name.
        CtxAction::ToggleWorktreeFold => {
            let id = menu.project_id;
            if !app.expanded_worktrees.remove(&id) {
                app.expanded_worktrees.insert(id);
            }
        }
        CtxAction::Reveal => {
            let Some(path) = project_reveal_path(app, &menu.project_id) else {
                return;
            };
            if std::process::Command::new("open")
                .arg(&path)
                .spawn()
                .is_err()
            {
                app.info = Some("could not open Finder".into());
            }
        }
        CtxAction::FolderColor => {
            let items = folder_color_items(app, &menu.project_id);
            app.modal = Some(Modal::Context(ContextMenu {
                title: menu.name.clone(),
                selected: 0,
                items,
                ..menu
            }));
        }
        CtxAction::SetColor(raw) => {
            let raw = *raw;
            set_project_folder_color(app, &menu.project_id, raw);
        }
        CtxAction::SortSessions => {
            let items = session_sort_items(&menu.project_id);
            app.modal = Some(Modal::Context(ContextMenu {
                title: menu.name.clone(),
                selected: 0,
                items,
                ..menu
            }));
        }
        CtxAction::SetSessionSort(date_sorted) => {
            let date_sorted = *date_sorted;
            match unpeel_core::session_ops::set_session_date_sorted(&menu.project_id, date_sorted) {
                // Rebuild NOW, like a drag commit: the mode is on disk but
                // the model only rescans on the next poll tick.
                Ok(()) => app.rescan(),
                Err(err) => app.info = Some(err),
            }
        }
        CtxAction::NewGroup => {
            app.modal = Some(Modal::GroupInput {
                project_id: menu.project_id,
                buffer: String::new(),
            });
        }
        CtxAction::RenameGroup => {
            app.modal = Some(Modal::GroupRename {
                project_id: menu.project_id,
                buffer: menu.name,
            });
        }
        CtxAction::RemoveGroup(parent_id) => {
            let parent_id = parent_id.clone();
            let count = session_ids_in_project(app, &menu.project_id).len();
            let noun = if count == 1 { "session" } else { "sessions" };
            app.modal = Some(Modal::Context(ContextMenu {
                title: format!("Remove {} and archive {count} {noun}?", menu.name),
                selected: 0,
                items: vec![
                    ("Cancel".to_string(), CtxAction::CloseMenu),
                    (
                        "Remove group".to_string(),
                        CtxAction::RemoveGroupConfirmed(parent_id),
                    ),
                ],
                ..menu
            }));
        }
        CtxAction::RemoveGroupConfirmed(parent_id) => {
            let parent_id = parent_id.clone();
            let project_id = menu.project_id.clone();
            let name = menu.name.clone();
            let ids = session_ids_in_project(app, &project_id);
            let own_port = app.hook_port;
            let results = results.clone();
            app.info = None;
            app.in_flight = Some(InFlight {
                label: "removing group".into(),
            });
            std::thread::spawn(move || {
                let bridged = bridge::post(
                    own_port,
                    "/mcp/remove-group",
                    &serde_json::json!({"project_id": project_id}),
                );
                let message = match bridged {
                    Ok(response) => {
                        let archived = response
                            .get("archived_count")
                            .and_then(|v| v.as_u64())
                            .unwrap_or(ids.len() as u64);
                        format!("removed {name} — archived {archived}")
                    }
                    Err(err) if bridge_unavailable(&err) => {
                        let mut archived = 0usize;
                        let mut failure = None;
                        for id in &ids {
                            // Pins win over archive. Explicitly unpin first so
                            // group removal really files every session away.
                            if let Err(err) = set_pin_in_app_state(id, false) {
                                failure = Some(err);
                                break;
                            }
                            // Keep the archive reachable after the group
                            // record is gone, including sessions launched
                            // directly in that group.
                            if let Err(err) =
                                unpeel_core::session_ops::set_project_override(id, &parent_id)
                            {
                                failure = Some(err);
                                break;
                            }
                            match unpeel_core::session_ops::archive_session(id) {
                                Ok(()) => archived += 1,
                                Err(err) => {
                                    failure = Some(err);
                                    break;
                                }
                            }
                        }
                        if let Some(err) = failure {
                            format!("group kept — archived {archived}, then: {err}")
                        } else {
                            match remove_project_from_app_state(&project_id) {
                                Ok(()) => format!("removed {name} — archived {archived}"),
                                Err(err) => format!("group kept — sessions archived: {err}"),
                            }
                        }
                    }
                    Err(err) => format!("group kept — {err}"),
                };
                let _ = results.send(VerbOutcome {
                    message,
                    select: None,
                    replacement_not_applied: None,
                    clipboard: None,
                });
            });
        }
        // Same popup, new rows — like the color submenu.
        CtxAction::MoveTo => {
            let Some(row) = menu
                .session_id
                .as_deref()
                .and_then(|id| app.model.rows.iter().find(|r| r.id == id))
                .cloned()
            else {
                return;
            };
            let items = move_to_items(app, &row);
            if items.is_empty() {
                return;
            }
            app.modal = Some(Modal::Context(ContextMenu {
                title: row.label.clone(),
                selected: 0,
                items,
                ..menu
            }));
        }
        CtxAction::MoveToProject(target) => {
            let target = target.clone();
            let Some(id) = menu.session_id.clone() else {
                return;
            };
            let outcome = match &target {
                Some(project_id) => unpeel_core::session_ops::set_project_override(&id, project_id),
                None => unpeel_core::session_ops::clear_project_override(&id),
            };
            match outcome {
                Ok(()) => {
                    // Show where it went: unfold the destination folder so
                    // the row doesn't just vanish from under the cursor.
                    if let Some(project_id) = target {
                        app.expanded_worktrees.insert(project_id);
                    }
                    app.info = Some("moved".into());
                }
                Err(e) => app.info = Some(e),
            }
        }
        CtxAction::RemoveProject => {
            // The desktop swaps the row for Cancel/Remove; the popup swaps
            // its rows the same way — no separate dialog to learn.
            app.modal = Some(Modal::Context(ContextMenu {
                title: format!("Remove {}?", menu.name),
                selected: 0,
                items: vec![
                    ("Cancel".to_string(), CtxAction::CloseMenu),
                    (
                        "Remove project".to_string(),
                        CtxAction::RemoveProjectConfirmed,
                    ),
                ],
                ..menu
            }));
        }
        CtxAction::RemoveProjectConfirmed => {
            app.info = Some(match remove_project_from_app_state(&menu.project_id) {
                Ok(()) => format!("removed {} — sessions keep running", menu.name),
                Err(e) => e,
            });
        }
        CtxAction::CloseMenu => {}
        // Session rows: the right-click selected the session, so the same
        // verb plumbing the keyboard uses (gates, confirms, spinner) fits
        // unchanged.
        CtxAction::RenameSession => {
            if let Some(session) = app.selected_session() {
                app.modal = Some(Modal::Rename(RenameInput::new(
                    session.id.clone(),
                    session.label.clone(),
                )));
            }
        }
        CtxAction::TogglePin => {
            let pinned = app.selected_session().map(|s| s.pinned).unwrap_or(false);
            request_verb(app, Verb::Pin(!pinned), grid, results);
        }
        CtxAction::RestartSession => request_restart(app, grid, results),
        CtxAction::CopyTranscript => {
            if menu.session_id.is_none() {
                return;
            }
            app.modal = Some(Modal::Context(ContextMenu {
                title: "Copy transcript".into(),
                selected: 0,
                items: vec![
                    (
                        "Last 20 entries".into(),
                        CtxAction::CopyTranscriptEntries(20),
                    ),
                    (
                        "Last 50 entries".into(),
                        CtxAction::CopyTranscriptEntries(50),
                    ),
                    (
                        "Whole conversation".into(),
                        CtxAction::CopyTranscriptEntries(0),
                    ),
                ],
                ..menu
            }));
        }
        CtxAction::CopyTranscriptEntries(entries) => {
            if let Some(id) = menu.session_id.clone() {
                copy_transcript(app, id, *entries, results);
            }
        }
        CtxAction::CopySessionId => {
            if let Some(id) = menu.session_id.clone() {
                let outcome = VerbOutcome {
                    message: "session ID copied".into(),
                    select: None,
                    replacement_not_applied: None,
                    clipboard: Some(format!("Unpeel Session ID: {id}")),
                };
                let _ = results.send(outcome);
            }
        }
        CtxAction::StopSession => request_verb(app, Verb::Stop, grid, results),
        CtxAction::ArchiveSession => {
            // request_verb gates Stop on running; archiving a stopped
            // session is the desktop's "Archive", so run it directly.
            if let Some(id) = menu.session_id.clone() {
                run_verb(app, Verb::Stop, id, grid, results);
            }
        }
        CtxAction::RemoveSession => request_verb(app, Verb::Remove, grid, results),
    }
}

/// Resolve the destination named by a highlighted "+ New session" row. It
/// has no selected session from which the normal picker can infer a project.
fn selected_new_session_target(app: &App) -> Option<(String, String)> {
    // A highlighted "+ New session" row names its own project — there is
    // no selected session to infer one from.
    app.selected_new_session.clone().map(|project_id| {
        let name = app
            .model
            .items
            .iter()
            .find_map(|item| match item {
                SidebarItem::NewSession { project: p, name } if *p == project_id => {
                    Some(name.clone())
                }
                _ => None,
            })
            .unwrap_or_default();
        (project_id, name)
    })
}

/// Open the centered preset dialog used by `n` and keyboard activation.
fn open_preset_picker(app: &mut App) {
    let target = selected_new_session_target(app);
    open_preset_picker_at(app, target, None)
}

/// Open the centered picker with an explicit destination, used by non-`+`
/// actions such as the project context menu.
fn open_preset_picker_for(app: &mut App, target: Option<(String, String)>) {
    open_preset_picker_at(app, target, None)
}

/// Open the mouse-first dropdown under a clicked `+` affordance.
fn open_preset_dropdown_for(app: &mut App, target: Option<(String, String)>, anchor: (u16, u16)) {
    open_preset_picker_at(app, target, Some(anchor))
}

/// Build the shared picker state. Presentation alone differs between the
/// mouse dropdown (`anchor = Some`) and keyboard dialog (`anchor = None`).
fn open_preset_picker_at(
    app: &mut App,
    target: Option<(String, String)>,
    anchor: Option<(u16, u16)>,
) {
    let presets = bridge::list_presets(app.hook_port)
        .ok()
        .filter(|p| !p.is_empty())
        .unwrap_or_else(|| sessions::fallback_presets(app.overlay.as_ref()));
    let mut presets = presets;
    presets.insert(0, ("Terminal".into(), String::new()));
    presets.push(("manage presets".into(), MANAGE_PRESETS_COMMAND.into()));
    if presets.is_empty() {
        app.info = Some("no presets found".into());
    } else {
        // Name the destination: the selected session's project header
        // and working directory.
        let mut project = String::new();
        let mut last_header = String::new();
        if let Some((project_id, name)) = target {
            let cwd = sessions::project_path(&project_id).unwrap_or_default();
            // Say when the destination is a worktree or a group — "new
            // session in worktree X" reads clearer than the bare folder
            // name.
            let folder_kind = app.model.items.iter().find_map(|item| match item {
                SidebarItem::WorktreeHeader {
                    project_id: p,
                    is_group,
                    ..
                } if *p == project_id => Some(if *is_group { "group" } else { "worktree" }),
                _ => None,
            });
            // Reveal the destination so the new session isn't hidden behind a
            // collapsed folder. A worktree/group child opens its own fold (its
            // parent header is already expanded — the folder row could not have
            // been picked otherwise); a top-level project un-collapses by name.
            if folder_kind.is_some() {
                app.expanded_worktrees.insert(project_id.clone());
            } else {
                app.collapsed.remove(&name);
            }
            app.pending_spawn_target = Some((project_id, cwd));
            project = match folder_kind {
                Some(kind) => format!("{kind} {name}"),
                None => name,
            };
        } else if let Some(selected) = app.selected_id.as_deref() {
            for item in &app.model.items {
                match item {
                    SidebarItem::Header(name) => last_header = name.clone(),
                    SidebarItem::Session(i) if app.model.rows[*i].id == selected => {
                        project = last_header.clone();
                        break;
                    }
                    _ => {}
                }
            }
            // A session living in a worktree/group child names that
            // folder, not the parent project header above it.
            if let Some(row) = app.model.rows.iter().find(|r| r.id == selected) {
                let gid = if row.group_id.is_empty() {
                    &row.project_id
                } else {
                    &row.group_id
                };
                if let Some((kind, wt)) = app.model.items.iter().find_map(|item| match item {
                    SidebarItem::WorktreeHeader {
                        project_id: p,
                        name,
                        is_group,
                        ..
                    } if p == gid => {
                        Some((if *is_group { "group" } else { "worktree" }, name.clone()))
                    }
                    _ => None,
                }) {
                    project = format!("{kind} {wt}");
                }
            }
        }
        let cwd = app
            .pending_spawn_target
            .as_ref()
            .map(|(_, cwd)| cwd.clone())
            .or_else(|| app.selected_session().map(|s| s.cwd.clone()))
            .unwrap_or_default();
        let home = std::env::var("HOME").unwrap_or_default();
        let cwd_short = if !home.is_empty() && cwd.starts_with(&home) {
            cwd.replacen(&home, "~", 1)
        } else {
            cwd
        };
        let target = if project.is_empty() {
            cwd_short
        } else if cwd_short.is_empty() {
            project
        } else {
            format!("{project} · {cwd_short}")
        };
        app.modal = Some(Modal::PresetPicker {
            presets,
            selected: 0,
            target,
            anchor,
        });
    }
}

/// Launch a preset chosen in the picker — ⏎ and a mouse click share this.
/// An explicit target (the "+ New session" row) wins over the selected
/// session's project, which is what an empty project has none of.
fn launch_picked_preset(
    app: &mut App,
    command: String,
    term_w: u16,
    term_h: u16,
    results: &mpsc::Sender<VerbOutcome>,
) {
    // The footer row is a shortcut into Settings ▸ Presets, not a launch.
    if command == MANAGE_PRESETS_COMMAND {
        app.pending_spawn_target = None;
        app.settings = Some((0, 0));
        return;
    }
    let explicit = app.pending_spawn_target.take();
    let cwd = explicit
        .as_ref()
        .map(|(_, cwd)| cwd.clone())
        .filter(|c| !c.is_empty())
        .or_else(|| app.selected_session().map(|s| s.cwd.clone()))
        .filter(|c| !c.is_empty())
        .or_else(|| std::env::var("HOME").ok())
        .unwrap_or_else(|| "/".into());
    let project_id = explicit
        .map(|(id, _)| id)
        .or_else(|| app.selected_session().map(|s| s.project_id.clone()))
        .unwrap_or_default();
    spawn_new_session(app, command, cwd, project_id, term_w, term_h, results);
}

/// Enter terminal focus: take the session's grid (explicit intent, same
/// semantics as a phone attach) and route keys to the PTY.
fn enter_terminal_focus(app: &mut App, term_w: u16, term_h: u16) {
    let Some(session) = app.selected_session() else {
        return;
    };
    if !session.running {
        app.info = Some("session is stopped — press r to resume".into());
        return;
    }
    let (cols, rows) = preview_grid(app, term_w, term_h);
    let dir = session.dir();
    // Explicit reclaim: the user chose to drive this session, so take the
    // grid back from any phone that had resized it.
    if let Ok(mut guard) = app.mobile_resizes.lock() {
        guard.remove(&session.id);
    }
    if let Err(err) = control::send_resize(&dir, cols, rows) {
        app.info = Some(err);
        return;
    }
    if let Ok(mut guard) = app.mobile_resizes.lock() {
        guard.remove(
            &dir.file_name()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default(),
        );
    }
    app.preview_scroll = 0;
    app.terminal_focus = true;
}

/// A completed sidebar click focuses without blocking the mouse loop on a
/// control-socket round trip. Selection already requested the normal async
/// pane fit on mouse-down; only a phone-owned grid needs another fit after
/// this explicit handoff clears mobile ownership.
fn focus_terminal_from_click(app: &mut App, snapshots: &SnapshotService, term_w: u16, term_h: u16) {
    let Some((session_id, running)) = app
        .selected_session()
        .map(|session| (session.id.clone(), session.running))
    else {
        return;
    };
    if !running {
        app.info = Some("session is stopped — press r to resume".into());
        return;
    }
    let mobile_owned = app.mobile_resized(&session_id);
    if let Ok(mut guard) = app.mobile_resizes.lock() {
        guard.remove(&session_id);
    }
    app.preview_scroll = 0;
    app.terminal_focus = true;
    if mobile_owned {
        resize_selected_to_pane(app, snapshots, term_w, term_h);
    }
}

/// True when a bracketed paste looks like a file drag-drop rather than
/// typed clipboard content: terminals paste a dropped file as its
/// shell-escaped absolute path (multiple files space-separated), one line.
fn looks_like_dropped_path(text: &str) -> bool {
    let trimmed = text.trim();
    if trimmed.is_empty() || trimmed.contains('\n') {
        return false;
    }
    let unquoted = trimmed.trim_start_matches(['\'', '"']);
    unquoted.starts_with('/') || unquoted.starts_with("~/") || unquoted.starts_with("file://")
}

fn forward_key_to_session(app: &mut App, key: &KeyEvent) {
    let Some(dir) = app.selected_session().map(|s| s.dir()) else {
        return;
    };
    if let Some(data) = keys::key_event_to_string(key) {
        if let Err(err) = app.input.send(&dir, data) {
            app.info = Some(err);
            app.terminal_focus = false;
        }
    }
}

/// What a key did to the rename modal. `Commit` carries the user's intent;
/// executing it is the caller's job so the local loop can route through the
/// bridge/session_ops and the remote Controller scope through its
/// `RemoteSessionBackend` — same modal, two executors (the design decision in
/// docs/plans/host-controller-transports.md: the UI never forks on scope).
pub(crate) enum RenameKeyOutcome {
    Commit { session_id: String, title: String },
    Closed,
    Pending,
}

/// Drive the rename modal with one key. The caller has already `take()`n the
/// modal; editing keys put it back, Enter/Esc leave it closed.
pub(crate) fn handle_rename_key(
    app: &mut App,
    mut input: RenameInput,
    key: KeyEvent,
) -> RenameKeyOutcome {
    match key.code {
        KeyCode::Enter => {
            let title = input.buffer.trim().to_string();
            if title.is_empty() {
                RenameKeyOutcome::Closed
            } else {
                RenameKeyOutcome::Commit {
                    session_id: input.session_id,
                    title,
                }
            }
        }
        KeyCode::Esc => RenameKeyOutcome::Closed,
        KeyCode::Backspace => {
            input.backspace();
            app.modal = Some(Modal::Rename(input));
            RenameKeyOutcome::Pending
        }
        KeyCode::Delete => {
            input.delete_forward();
            app.modal = Some(Modal::Rename(input));
            RenameKeyOutcome::Pending
        }
        KeyCode::Left => {
            input.move_left(
                key.modifiers
                    .contains(ratatui::crossterm::event::KeyModifiers::SHIFT),
            );
            app.modal = Some(Modal::Rename(input));
            RenameKeyOutcome::Pending
        }
        KeyCode::Right => {
            input.move_right(
                key.modifiers
                    .contains(ratatui::crossterm::event::KeyModifiers::SHIFT),
            );
            app.modal = Some(Modal::Rename(input));
            RenameKeyOutcome::Pending
        }
        KeyCode::Home => {
            input.move_to(
                0,
                key.modifiers
                    .contains(ratatui::crossterm::event::KeyModifiers::SHIFT),
            );
            app.modal = Some(Modal::Rename(input));
            RenameKeyOutcome::Pending
        }
        KeyCode::End => {
            input.move_to(
                input.len(),
                key.modifiers
                    .contains(ratatui::crossterm::event::KeyModifiers::SHIFT),
            );
            app.modal = Some(Modal::Rename(input));
            RenameKeyOutcome::Pending
        }
        KeyCode::Char(c)
            if !key.modifiers.intersects(
                ratatui::crossterm::event::KeyModifiers::CONTROL
                    | ratatui::crossterm::event::KeyModifiers::ALT,
            ) =>
        {
            input.insert(&c.to_string());
            app.modal = Some(Modal::Rename(input));
            RenameKeyOutcome::Pending
        }
        _ => {
            app.modal = Some(Modal::Rename(input));
            RenameKeyOutcome::Pending
        }
    }
}

/// What a key did to the preset picker: `Launch` is the picked command, to
/// be executed by the caller's backend (local spawn or remote create).
pub(crate) enum PresetPickerKeyOutcome {
    Launch(String),
    Closed,
    Pending,
}

/// Drive the preset picker with one key — navigation is shared; only the
/// launch itself belongs to the caller.
pub(crate) fn handle_preset_picker_key(
    app: &mut App,
    presets: Vec<(String, String)>,
    selected: usize,
    target: String,
    anchor: Option<(u16, u16)>,
    key: KeyEvent,
) -> PresetPickerKeyOutcome {
    match key.code {
        KeyCode::Up | KeyCode::Char('k') => {
            app.modal = Some(Modal::PresetPicker {
                presets,
                selected: selected.saturating_sub(1),
                target,
                anchor,
            });
            PresetPickerKeyOutcome::Pending
        }
        KeyCode::Down | KeyCode::Char('j') => {
            let next = (selected + 1).min(presets.len().saturating_sub(1));
            app.modal = Some(Modal::PresetPicker {
                presets,
                selected: next,
                target,
                anchor,
            });
            PresetPickerKeyOutcome::Pending
        }
        KeyCode::Enter => PresetPickerKeyOutcome::Launch(presets[selected].1.clone()),
        // Closing the picker drops any explicit target with it.
        KeyCode::Esc | KeyCode::Char('q') => {
            app.pending_spawn_target = None;
            PresetPickerKeyOutcome::Closed
        }
        _ => {
            app.modal = Some(Modal::PresetPicker {
                presets,
                selected,
                target,
                anchor,
            });
            PresetPickerKeyOutcome::Pending
        }
    }
}

/// What a key did to the open archive library. Resumable rows use the primary
/// `RestoreAndResume` action; rows whose command has no safe resume recipe are
/// only unfiled through `Restore`. `Handled` consumed the key (navigation,
/// search, the remove confirm); `NotHandled` means the archive view is closed
/// or the key falls through to the normal bindings.
pub(crate) enum ArchiveKeyOutcome {
    Restore(String),
    RestoreAndResume(String),
    Handled,
    NotHandled,
}

/// Drive the archive library with one key — shared verbatim between the
/// local loop and the remote Controller scope (the remove confirm it posts is
/// executed by whichever loop owns `app.confirm`).
pub(crate) fn handle_archive_key(
    app: &mut App,
    key: KeyEvent,
    grid: (u16, u16),
) -> ArchiveKeyOutcome {
    let Some((group, row)) = app.selected_archive.clone() else {
        return ArchiveKeyOutcome::NotHandled;
    };
    let ids: Vec<String> = app
        .archived_matches(&group)
        .iter()
        .map(|r| r.id.clone())
        .collect();
    match key.code {
        KeyCode::Up | KeyCode::Char('k') if row > 0 => {
            app.selected_archive = Some((group, row - 1));
            ArchiveKeyOutcome::Handled
        }
        KeyCode::Down | KeyCode::Char('j') if row + 1 < ids.len() => {
            app.selected_archive = Some((group, row + 1));
            ArchiveKeyOutcome::Handled
        }
        KeyCode::Enter | KeyCode::Char('r') => match ids.get(row).cloned() {
            Some(id)
                if app
                    .model
                    .rows
                    .iter()
                    .find(|session| session.id == id)
                    .is_some_and(|session| session.resume_available) =>
            {
                ArchiveKeyOutcome::RestoreAndResume(id)
            }
            Some(id) => ArchiveKeyOutcome::Restore(id),
            None => ArchiveKeyOutcome::Handled,
        },
        KeyCode::Char('x') => {
            if let Some(id) = ids.get(row).cloned() {
                let label = app
                    .model
                    .rows
                    .iter()
                    .find(|r| r.id == id)
                    .map(|r| r.label.clone())
                    .unwrap_or_default();
                app.confirm = Some(Confirm {
                    verb: Verb::Remove,
                    session_id: id,
                    grid,
                    prompt: format!("Remove \"{label}\" permanently"),
                });
            }
            ArchiveKeyOutcome::Handled
        }
        KeyCode::Esc => {
            if app.archive_query.is_empty() {
                app.selected_archive = None;
            } else {
                app.archive_query.clear();
                app.selected_archive = Some((group, 0));
            }
            ArchiveKeyOutcome::Handled
        }
        KeyCode::Backspace => {
            app.archive_query.pop();
            app.selected_archive = Some((group, 0));
            ArchiveKeyOutcome::Handled
        }
        KeyCode::Char(c)
            if !key
                .modifiers
                .contains(ratatui::crossterm::event::KeyModifiers::CONTROL) =>
        {
            app.archive_query.push(c);
            app.selected_archive = Some((group, 0));
            ArchiveKeyOutcome::Handled
        }
        _ => ArchiveKeyOutcome::NotHandled,
    }
}

fn handle_key(
    app: &mut App,
    key: KeyEvent,
    term_w: u16,
    term_h: u16,
    snapshots: &SnapshotService,
    results: &mpsc::Sender<VerbOutcome>,
) -> bool {
    // Mouse-up keeps a terminal selection visible. The next key consumes it:
    // ordinary keys retain their normal meaning, while plain Backspace can
    // delete a selection on the live input row as one edit operation.
    let terminal_selection = app.terminal_selection.take();
    if let Some(confirm) = app.confirm.take() {
        match key.code {
            KeyCode::Char('y') | KeyCode::Char('Y') => {
                run_verb(app, confirm.verb, confirm.session_id, confirm.grid, results)
            }
            _ => {}
        }
        return true;
    }
    app.info = None;
    if let Some((approval_id, _)) = app.approvals.front() {
        match key.code {
            KeyCode::Char('y') | KeyCode::Char('Y') => {
                app.approvals.answer(&approval_id, true);
                return true;
            }
            KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
                app.approvals.answer(&approval_id, false);
                return true;
            }
            _ => {}
        }
    }
    if let Some(modal) = app.modal.take() {
        match modal {
            // Any key closes help.
            Modal::Help => return true,
            Modal::FirstRun(first_run) => {
                match key.code {
                    KeyCode::Up | KeyCode::Char('k') => {
                        app.modal = Some(Modal::FirstRun(FirstRun {
                            row: first_run.row.saturating_sub(1),
                            ..first_run
                        }));
                    }
                    KeyCode::Down | KeyCode::Char('j') => {
                        let last = first_run.projects.len().saturating_sub(1);
                        app.modal = Some(Modal::FirstRun(FirstRun {
                            row: (first_run.row + 1).min(last),
                            ..first_run
                        }));
                    }
                    KeyCode::Char(' ') => {
                        let mut next = first_run;
                        if let Some(flag) = next.accepted.get_mut(next.row) {
                            *flag = !*flag;
                        }
                        app.modal = Some(Modal::FirstRun(next));
                    }
                    KeyCode::Enter => {
                        let accepted: Vec<_> = first_run
                            .projects
                            .iter()
                            .zip(first_run.accepted.iter())
                            .filter(|(_, keep)| **keep)
                            .map(|(project, _)| project.clone())
                            .collect();
                        match unpeel_core::first_run::seed_app_state(&accepted) {
                            Ok((presets, projects)) => {
                                app.info = Some(format!(
                                    "set up {} preset{} and {} project{}",
                                    presets.len(),
                                    if presets.len() == 1 { "" } else { "s" },
                                    projects.len(),
                                    if projects.len() == 1 { "" } else { "s" },
                                ));
                            }
                            Err(e) => app.info = Some(e),
                        }
                        app.overlay = overlay::load();
                        app.rescan();
                    }
                    KeyCode::Esc | KeyCode::Char('q') => {}
                    _ => app.modal = Some(Modal::FirstRun(first_run)),
                }
                return true;
            }
            Modal::Palette {
                mut query,
                selected,
            } => {
                let matches = app.palette_matches(&query);
                match key.code {
                    // Esc leaves the palette; a terminal that had focus keeps it.
                    KeyCode::Esc => {}
                    KeyCode::Up => {
                        app.modal = Some(Modal::Palette {
                            query,
                            selected: selected.saturating_sub(1),
                        });
                    }
                    KeyCode::Down => {
                        let next = (selected + 1).min(matches.len().saturating_sub(1));
                        app.modal = Some(Modal::Palette {
                            query,
                            selected: next,
                        });
                    }
                    KeyCode::Enter => {
                        if let Some(item) = matches.get(selected).cloned() {
                            match item.action {
                                palette::Action::SelectSession(id) => {
                                    app.replacement_selection.clear();
                                    app.mark_read(&id);
                                    app.selected_recent = None;
                                    app.selected_id = Some(id);
                                    app.preview_scroll = 0;
                                }
                                palette::Action::SelectProject(name) => {
                                    app.replacement_selection.clear();
                                    app.selected_recent = None;
                                    // Jump to the project's first session.
                                    let mut current = String::new();
                                    for item in &app.model.items {
                                        match item {
                                            SidebarItem::Header(header) => current = header.clone(),
                                            SidebarItem::Session(i) if current == name => {
                                                app.selected_id =
                                                    Some(app.model.rows[*i].id.clone());
                                                break;
                                            }
                                            _ => {}
                                        }
                                    }
                                    app.collapsed.remove(&name);
                                }
                                palette::Action::Launch(command)
                                | palette::Action::NewTerminal(command) => {
                                    let cwd = app
                                        .selected_session()
                                        .map(|s| s.cwd.clone())
                                        .filter(|c| !c.is_empty())
                                        .or_else(|| std::env::var("HOME").ok())
                                        .unwrap_or_else(|| "/".into());
                                    let project_id = app
                                        .selected_session()
                                        .map(|s| s.project_id.clone())
                                        .unwrap_or_default();
                                    spawn_new_session(
                                        app, command, cwd, project_id, term_w, term_h, results,
                                    );
                                }
                                palette::Action::OpenSettings => {
                                    app.selected_recent = None;
                                    app.settings = Some((0, 0));
                                }
                                palette::Action::ToggleFold => app.toggle_fold_all(),
                            }
                        }
                    }
                    KeyCode::Backspace => {
                        query.pop();
                        app.modal = Some(Modal::Palette { query, selected: 0 });
                    }
                    KeyCode::Char(c) => {
                        query.push(c);
                        app.modal = Some(Modal::Palette { query, selected: 0 });
                    }
                    _ => app.modal = Some(Modal::Palette { query, selected }),
                }
                return true;
            }
            Modal::Activity { selected } => {
                let entries = app.activity_menu_entries();
                let last = entries.len(); // footer is the final action
                match key.code {
                    KeyCode::Esc | KeyCode::Char('q') => {}
                    KeyCode::Up | KeyCode::Char('k') => {
                        app.modal = Some(Modal::Activity {
                            selected: selected.saturating_sub(1),
                        });
                    }
                    KeyCode::Down | KeyCode::Char('j') => {
                        app.modal = Some(Modal::Activity {
                            selected: (selected + 1).min(last),
                        });
                    }
                    KeyCode::Enter if selected == last => app.open_recent_activity(),
                    KeyCode::Enter => {
                        if let Some(entry) = entries.get(selected) {
                            let id = entry.session_id.clone();
                            if app.reveal_session(&id, true) {
                                resize_selected_to_pane(app, snapshots, term_w, term_h);
                            }
                        }
                    }
                    _ => {
                        app.modal = Some(Modal::Activity {
                            selected: selected.min(last),
                        })
                    }
                }
                return true;
            }
            Modal::ProjectInput(mut input) => {
                match key.code {
                    KeyCode::Enter => {
                        let expanded = input.expanded();
                        if expanded.is_empty() || !std::path::Path::new(&expanded).is_dir() {
                            // Not a directory yet — treat ⏎ as "take the highlight".
                            input.complete();
                            app.modal = Some(Modal::ProjectInput(input));
                            return true;
                        }
                        commit_add_project(app, &expanded);
                    }
                    KeyCode::Tab => {
                        input.complete();
                        app.modal = Some(Modal::ProjectInput(input));
                    }
                    KeyCode::Up => {
                        input.selected = input.selected.saturating_sub(1);
                        app.modal = Some(Modal::ProjectInput(input));
                    }
                    KeyCode::Down => {
                        input.selected =
                            (input.selected + 1).min(input.matches.len().saturating_sub(1));
                        app.modal = Some(Modal::ProjectInput(input));
                    }
                    KeyCode::Esc => {}
                    KeyCode::Backspace => {
                        input.query.pop();
                        input.refresh();
                        app.modal = Some(Modal::ProjectInput(input));
                    }
                    KeyCode::Char(c) => {
                        input.query.push(c);
                        input.refresh();
                        app.modal = Some(Modal::ProjectInput(input));
                    }
                    _ => app.modal = Some(Modal::ProjectInput(input)),
                }
                return true;
            }
            Modal::Rename(input) => {
                if let RenameKeyOutcome::Commit { session_id, title } =
                    handle_rename_key(app, input, key)
                {
                    let own_port = app.hook_port;
                    let results = results.clone();
                    app.info = Some("…".into());
                    std::thread::spawn(move || {
                        let outcome = bridge::post(
                            own_port,
                            "/mcp/organize-session",
                            &serde_json::json!({"session_id": session_id, "title": title}),
                        );
                        let message = match outcome {
                            Ok(_) => "renamed".into(),
                            Err(e) if bridge_unavailable(&e) => {
                                match unpeel_core::session_ops::set_title(&session_id, &title) {
                                    Ok(()) => "renamed".into(),
                                    Err(e2) => e2,
                                }
                            }
                            Err(e) => e,
                        };
                        let _ = results.send(VerbOutcome {
                            message,
                            select: None,
                            replacement_not_applied: None,
                            clipboard: None,
                        });
                    });
                }
                return true;
            }
            Modal::GroupInput {
                project_id,
                mut buffer,
            } => {
                match key.code {
                    KeyCode::Enter => {
                        let name = buffer.trim().to_string();
                        if !name.is_empty() {
                            app.info = Some(match add_group_to_app_state(&project_id, &name) {
                                Ok(()) => format!("group added: {name}"),
                                Err(e) => e,
                            });
                        }
                    }
                    KeyCode::Esc => {}
                    KeyCode::Backspace => {
                        buffer.pop();
                        app.modal = Some(Modal::GroupInput { project_id, buffer });
                    }
                    KeyCode::Char(c) => {
                        buffer.push(c);
                        app.modal = Some(Modal::GroupInput { project_id, buffer });
                    }
                    _ => app.modal = Some(Modal::GroupInput { project_id, buffer }),
                }
                return true;
            }
            Modal::GroupRename {
                project_id,
                mut buffer,
            } => {
                match key.code {
                    KeyCode::Enter => {
                        let name = buffer.trim().to_string();
                        if !name.is_empty() {
                            let own_port = app.hook_port;
                            let results = results.clone();
                            app.info = Some("…".into());
                            std::thread::spawn(move || {
                                let outcome = bridge::post(
                                    own_port,
                                    "/mcp/rename-group",
                                    &serde_json::json!({
                                        "project_id": project_id,
                                        "name": name,
                                    }),
                                );
                                let message = match outcome {
                                    Ok(_) => "group renamed".into(),
                                    Err(err) if bridge_unavailable(&err) => {
                                        match rename_group_in_app_state(&project_id, &name) {
                                            Ok(()) => "group renamed".into(),
                                            Err(e) => e,
                                        }
                                    }
                                    Err(err) => err,
                                };
                                let _ = results.send(VerbOutcome {
                                    message,
                                    select: None,
                                    replacement_not_applied: None,
                                    clipboard: None,
                                });
                            });
                        }
                    }
                    KeyCode::Esc => {}
                    KeyCode::Backspace => {
                        buffer.pop();
                        app.modal = Some(Modal::GroupRename { project_id, buffer });
                    }
                    KeyCode::Char(c) => {
                        buffer.push(c);
                        app.modal = Some(Modal::GroupRename { project_id, buffer });
                    }
                    _ => app.modal = Some(Modal::GroupRename { project_id, buffer }),
                }
                return true;
            }
            Modal::PresetPicker {
                presets,
                selected,
                target,
                anchor,
            } => {
                if let PresetPickerKeyOutcome::Launch(command) =
                    handle_preset_picker_key(app, presets, selected, target, anchor, key)
                {
                    launch_picked_preset(app, command, term_w, term_h, results);
                }
                return true;
            }
            Modal::Pairing { lines, code } => {
                // Passive, unlike every other modal: the QR stays up
                // while keys behave normally — 'm' toggles it closed via
                // open_pairing, and expiry auto-closes it in the run
                // loop. Put it back and fall through.
                app.modal = Some(Modal::Pairing { lines, code });
            }
            Modal::Menu { selected } => {
                match key.code {
                    // The displayed shortcuts stay live while the menu is
                    // open, just like key equivalents in a native menu.
                    KeyCode::Char(',') => activate_menu_action(app, MenuAction::OpenSettings),
                    KeyCode::Char('?') => activate_menu_action(app, MenuAction::OpenKeybindings),
                    KeyCode::Char('/') => activate_menu_action(app, MenuAction::OpenCommandPalette),
                    KeyCode::Char('k')
                        if key
                            .modifiers
                            .contains(ratatui::crossterm::event::KeyModifiers::CONTROL) =>
                    {
                        activate_menu_action(app, MenuAction::OpenCommandPalette)
                    }
                    KeyCode::Up | KeyCode::Char('k') => {
                        app.modal = Some(Modal::Menu {
                            selected: selected.saturating_sub(1),
                        });
                    }
                    KeyCode::Down | KeyCode::Char('j') => {
                        let last = MENU_ITEMS.len().saturating_sub(1);
                        app.modal = Some(Modal::Menu {
                            selected: (selected + 1).min(last),
                        });
                    }
                    KeyCode::Enter => activate_menu(app, selected),
                    KeyCode::Char('q') => activate_menu_action(app, MenuAction::Exit),
                    KeyCode::Esc => {}
                    _ => app.modal = Some(Modal::Menu { selected }),
                }
                return true;
            }
            Modal::LocalUrls { rows, selected } => {
                match key.code {
                    KeyCode::Up | KeyCode::Char('k') => {
                        app.modal = Some(Modal::LocalUrls {
                            rows,
                            selected: selected.saturating_sub(1),
                        });
                    }
                    KeyCode::Down | KeyCode::Char('j') => {
                        let last = rows.len().saturating_sub(1);
                        app.modal = Some(Modal::LocalUrls {
                            rows,
                            selected: (selected + 1).min(last),
                        });
                    }
                    KeyCode::Enter => {
                        if let Some(row) = rows.get(selected) {
                            activate_local_url_row(app, row);
                        }
                    }
                    KeyCode::Esc | KeyCode::Char('q') => {}
                    _ => app.modal = Some(Modal::LocalUrls { rows, selected }),
                }
                return true;
            }
            Modal::Context(mut menu) => {
                match key.code {
                    KeyCode::Up | KeyCode::Char('k') => {
                        menu.selected = menu.selected.saturating_sub(1);
                        app.modal = Some(Modal::Context(menu));
                    }
                    KeyCode::Down | KeyCode::Char('j') => {
                        menu.selected = (menu.selected + 1).min(menu.items.len().saturating_sub(1));
                        app.modal = Some(Modal::Context(menu));
                    }
                    KeyCode::Enter => {
                        let index = menu.selected;
                        activate_context_menu(app, menu, index, term_w, term_h, results);
                    }
                    KeyCode::Esc | KeyCode::Char('q') => {}
                    _ => app.modal = Some(Modal::Context(menu)),
                }
                return true;
            }
        }
    }
    if let Some((section, row)) = app.settings {
        const SECTIONS: usize = 6;
        // Presets ▸ the blank add row at the bottom of the list: while it's
        // selected, printable keys type into the draft (so the letter
        // shortcuts below don't fire), ⏎ commits, esc clears the draft
        // before it closes settings. Arrow keys still navigate away.
        let on_add_row = section == 0 && row == app_state_presets().len();
        // Remote ▸ Unpeel Link rows sit after the paired-device list (the
        // standalone Link tab merged into Remote, desktop parity). Editable
        // rows — the key field (not enrolled) and the display name
        // (enrolled) — type into a shared draft.
        let remote_devices_len = if section == 2 {
            paired_devices().len()
        } else {
            0
        };
        let link_licensed = section == 2 && unpeel_core::license::stored().is_some();
        let on_link_edit = section == 2
            && ((row == remote_devices_len && !link_licensed)
                || (row == remote_devices_len + 1 && link_licensed));
        match key.code {
            KeyCode::Char(c)
                if on_add_row
                    && !key
                        .modifiers
                        .contains(ratatui::crossterm::event::KeyModifiers::CONTROL) =>
            {
                app.preset_add.push(c);
            }
            KeyCode::Backspace if on_add_row => {
                app.preset_add.pop();
            }
            KeyCode::Esc if on_add_row && !app.preset_add.is_empty() => app.preset_add.clear(),
            KeyCode::Char(c)
                if on_link_edit
                    && !key
                        .modifiers
                        .contains(ratatui::crossterm::event::KeyModifiers::CONTROL) =>
            {
                app.link_input.push(c);
            }
            KeyCode::Backspace if on_link_edit => {
                app.link_input.pop();
            }
            KeyCode::Esc if on_link_edit && !app.link_input.is_empty() => app.link_input.clear(),
            KeyCode::Enter if section == 2 && row >= remote_devices_len => {
                // A blank key field falls through to the section's primary
                // action (the pairing QR) so sharing stays one keypress away
                // even when no device rows exist yet.
                if !link_licensed && app.link_input.trim().is_empty() {
                    app.settings = None;
                    open_pairing(app);
                } else {
                    link_settings_enter(app, row - remote_devices_len, link_licensed);
                }
            }
            KeyCode::Char(' ')
                if section == 2 && link_licensed && row == remote_devices_len + 2 =>
            {
                link_settings_enter(app, row - remote_devices_len, link_licensed);
            }
            KeyCode::Enter if on_add_row => {
                let command = app.preset_add.trim().to_string();
                if !command.is_empty() {
                    match add_preset(&command) {
                        Ok(()) => {
                            app.info = Some(format!("preset added: {command}"));
                            app.preset_add.clear();
                            // The list grew above the add row — follow it down.
                            app.settings = Some((section, row + 1));
                        }
                        Err(e) => app.info = Some(e),
                    }
                }
            }
            KeyCode::Esc | KeyCode::Char(',') | KeyCode::Char('q') => app.settings = None,
            KeyCode::Tab | KeyCode::Right | KeyCode::Char('l') => {
                app.settings = Some(((section + 1) % SECTIONS, 0))
            }
            KeyCode::BackTab | KeyCode::Left | KeyCode::Char('h') => {
                app.settings = Some(((section + SECTIONS - 1) % SECTIONS, 0))
            }
            KeyCode::Up | KeyCode::Char('k') => {
                app.settings = Some((section, row.saturating_sub(1)))
            }
            KeyCode::Down | KeyCode::Char('j') => {
                app.settings = Some((section, (row + 1).min(settings_row_count(app, section))));
            }
            // Presets ▸ toggle/add/remove.
            KeyCode::Enter | KeyCode::Char(' ') if section == 0 => {
                if let Err(e) = toggle_preset_enabled(row) {
                    app.info = Some(e);
                }
            }
            KeyCode::Char('+') if section == 0 => {
                // Jump straight to the blank add row.
                app.settings = Some((0, app_state_presets().len()));
            }
            // Order IS the default choice (topmost enabled preset per CLI
            // wins), so moving a row rewrites the shared list.
            KeyCode::Char('K') if section == 0 && row > 0 => {
                if move_preset(row, row - 1).is_ok() {
                    app.settings = Some((section, row - 1));
                }
            }
            KeyCode::Char('J') if section == 0 => {
                let last = app_state_presets().len().saturating_sub(1);
                if row < last && move_preset(row, row + 1).is_ok() {
                    app.settings = Some((section, row + 1));
                }
            }
            KeyCode::Char('*') if section == 0 => {
                if let Err(e) = toggle_preset_star(row) {
                    app.info = Some(e);
                }
            }
            KeyCode::Char('x') if section == 0 => {
                if let Err(e) = remove_preset_at(row) {
                    app.info = Some(e);
                }
            }
            // Access ▸ cycle the selected policy.
            KeyCode::Enter | KeyCode::Char(' ') if section == 1 => {
                if let Err(e) = cycle_access_setting(row) {
                    app.info = Some(e);
                }
            }
            // Cleanup ▸ cycle the auto-stop-and-archive cutoff.
            KeyCode::Enter | KeyCode::Char(' ') if section == 5 => {
                if let Err(e) = cycle_auto_stop_archive_setting() {
                    app.info = Some(e);
                }
            }
            // Remote ▸ share this host (pair a Controller) / unpair. ⏎ on a
            // device row shares; 'S' shares from any non-editing row
            // (capital, like 'L' — lowercase 's' stays free).
            KeyCode::Enter if section == 2 => {
                app.settings = None;
                open_pairing(app);
            }
            KeyCode::Char('S') if section == 2 => {
                app.settings = None;
                open_pairing(app);
            }
            KeyCode::Char('x') if section == 2 => {
                let devices = paired_devices();
                if let Some(device) = devices.get(row) {
                    match pairing::unpair_device(&device.id) {
                        Ok(()) => app.info = Some(format!("unpaired {}", device.name)),
                        Err(e) => app.info = Some(e),
                    }
                }
            }
            // Remote ▸ toggle the selected device between Direct-only and
            // Direct + Link ('L' — lowercase 'l' is section-switch nav). The
            // uplink revalidates every active device frame against the shared
            // authority file, so narrowing takes effect without reconnect.
            KeyCode::Char('L') if section == 2 => {
                let devices = paired_devices();
                if let Some(device) = devices.get(row) {
                    match pairing::set_device_relay_allowed(&device.id, !device.relay_allowed) {
                        Ok(()) => {
                            app.info = Some(format!(
                                "{}: {}",
                                device.name,
                                if device.relay_allowed {
                                    "direct only — Link off"
                                } else {
                                    "Link on"
                                }
                            ));
                        }
                        Err(e) => app.info = Some(e),
                    }
                }
            }
            KeyCode::Char('+') if section == 3 => {
                app.settings = None;
                app.modal = Some(Modal::ProjectInput(ProjectInput::new()));
            }
            _ => {}
        }
        return true;
    }
    if app.terminal_focus {
        let ctrl = key
            .modifiers
            .contains(ratatui::crossterm::event::KeyModifiers::CONTROL);
        // Ctrl+]: kitty terminals report Char(']'), legacy 0x1d decodes as
        // Ctrl+'5' — accept both as detach.
        let detach = ctrl && matches!(key.code, KeyCode::Char(']') | KeyCode::Char('5'));
        // Ctrl+K stays global, like the desktop's ⌘K: reaching the palette
        // shouldn't require leaving the terminal first. (The cost is the
        // shell's kill-to-end-of-line while focused; Ctrl+] then k gets it
        // back if anyone misses it.)
        let palette = ctrl && key.code == KeyCode::Char('k');
        // ^1…^9 stays global for the same reason as ^K: switching sessions
        // shouldn't require detaching from the one you're in first.
        let jump = match key.code {
            KeyCode::Char(c) if ctrl && c.is_ascii_digit() && c != '0' => app
                .quick_jump_ids()
                .get(c.to_digit(10).unwrap_or(0) as usize - 1)
                .cloned(),
            _ => None,
        };
        if detach {
            app.terminal_focus = false;
        } else if palette {
            app.modal = Some(Modal::Palette {
                query: String::new(),
                selected: 0,
            });
        } else if let Some(id) = jump {
            // Leaving focus is deliberate: the new session's grid is sized
            // for the pane, and staying attached would type into whichever
            // session the cursor lands on.
            app.terminal_focus = false;
            app.replacement_selection.clear();
            app.mark_read(&id);
            app.selected_id = Some(id);
            app.preview_scroll = 0;
            resize_selected_to_pane(app, snapshots, term_w, term_h);
        } else if key.code == KeyCode::Backspace
            && key.modifiers.is_empty()
            && terminal_selection.as_ref().is_some_and(|selection| {
                app.selected_session()
                    .is_some_and(|session| session.id == selection.session_id)
                    && snapshots
                        .get(&selection.session_id)
                        .is_some_and(|snapshot| {
                            snapshot.output_offset == selection.snapshot.output_offset
                        })
            })
        {
            let sequence = terminal_selection
                .as_ref()
                .and_then(TerminalSelection::backspace_edit_sequence);
            if let Some(data) = sequence {
                if let Some(dir) = app.selected_session().map(|session| session.dir()) {
                    if let Err(err) = app.input.send(&dir, data) {
                        app.info = Some(err);
                        app.terminal_focus = false;
                    }
                }
            } else {
                forward_key_to_session(app, &key);
            }
        } else {
            forward_key_to_session(app, &key);
        }
        return true;
    }
    // Cmd+K equivalent — must precede the plain 'k' navigation arm.
    if key.code == KeyCode::Char('k')
        && key
            .modifiers
            .contains(ratatui::crossterm::event::KeyModifiers::CONTROL)
    {
        app.modal = Some(Modal::Palette {
            query: String::new(),
            selected: 0,
        });
        return true;
    }
    // App-wide All recent page: it owns navigation while it replaces the
    // terminal preview. Removed-session history rows stay visible but simply
    // decline Enter; live rows reveal and mark read like the dropdown.
    if let Some(selected) = app.selected_recent {
        let entries = app.recent_activity_entries();
        let last = entries.len().saturating_sub(1);
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('R') => app.close_recent_activity(),
            KeyCode::Up | KeyCode::Char('k') => {
                app.selected_recent = Some(selected.saturating_sub(1))
            }
            KeyCode::Down | KeyCode::Char('j') => {
                app.selected_recent = Some((selected + 1).min(last))
            }
            KeyCode::Enter => {
                if let Some(id) = entries
                    .get(selected.min(last))
                    .and_then(|entry| entry.session_id.clone())
                {
                    if app.reveal_session(&id, true) {
                        resize_selected_to_pane(app, snapshots, term_w, term_h);
                    }
                }
            }
            _ => {}
        }
        return true;
    }
    // Archive library open: the preview lists that project's archive, so
    // the arrows walk it and the verbs act on the highlighted entry.
    match handle_archive_key(app, key, preview_grid(app, term_w, term_h)) {
        ArchiveKeyOutcome::RestoreAndResume(id) => {
            match unpeel_core::session_ops::restore_session(&id) {
                Ok(()) => {
                    app.selected_archive = None;
                    app.selected_id = Some(id.clone());
                    let grid = preview_grid(app, term_w, term_h);
                    run_verb(app, Verb::Resume, id, grid, results);
                }
                Err(e) => app.info = Some(e),
            }
            return true;
        }
        ArchiveKeyOutcome::Restore(id) => {
            match unpeel_core::session_ops::restore_session(&id) {
                Ok(()) => {
                    app.info = Some("restored from archive".into());
                    app.selected_archive = None;
                    app.selected_id = Some(id);
                }
                Err(e) => app.info = Some(e),
            }
            return true;
        }
        ArchiveKeyOutcome::Handled => return true,
        ArchiveKeyOutcome::NotHandled => {}
    }
    match key.code {
        KeyCode::Char('q') => return false,
        // Ctrl+C quits like `q` — but only here, with the sidebar focused: a
        // focused terminal forwards it to the session (interrupting the
        // agent is what Ctrl+C means there; detach is Ctrl+]).
        KeyCode::Char('c')
            if key
                .modifiers
                .contains(ratatui::crossterm::event::KeyModifiers::CONTROL) =>
        {
            return false
        }
        KeyCode::Up | KeyCode::Char('k') => {
            app.move_selection(-1);
            resize_selected_to_pane(app, snapshots, term_w, term_h);
        }
        KeyCode::Down | KeyCode::Char('j') => {
            app.move_selection(1);
            resize_selected_to_pane(app, snapshots, term_w, term_h);
        }
        KeyCode::Char('s') => {
            request_verb(app, Verb::Stop, preview_grid(app, term_w, term_h), results)
        }
        KeyCode::Char('r') => request_restart(app, preview_grid(app, term_w, term_h), results),
        KeyCode::Char('R') => app.open_recent_activity(),
        KeyCode::Char('x') => request_verb(
            app,
            Verb::Remove,
            preview_grid(app, term_w, term_h),
            results,
        ),
        // ^1…^9 jump to the Nth session of the selected session's project,
        // matching the desktop's ⌘1-9. Needs the kitty protocol: the legacy
        // encoding has no way to say "ctrl and a digit".
        KeyCode::Char(c)
            if c.is_ascii_digit()
                && c != '0'
                && key
                    .modifiers
                    .contains(ratatui::crossterm::event::KeyModifiers::CONTROL) =>
        {
            app.terminal_focus = false;
            let index = c.to_digit(10).unwrap_or(0) as usize - 1;
            if let Some(id) = app.quick_jump_ids().get(index).cloned() {
                app.replacement_selection.clear();
                app.selected_archive = None;
                app.selected_recent = None;
                app.selected_worktree_folder = None;
                app.selected_new_session = None;
                app.mark_read(&id);
                app.selected_id = Some(id);
                app.preview_scroll = 0;
                resize_selected_to_pane(app, snapshots, term_w, term_h);
            }
        }
        KeyCode::Char('n') => open_preset_picker(app),
        KeyCode::Char('m') => open_pairing(app),
        KeyCode::Char(',') => {
            app.selected_recent = None;
            app.settings = Some((0, 0));
        }
        KeyCode::Char('/') => {
            app.modal = Some(Modal::Palette {
                query: String::new(),
                selected: 0,
            })
        }
        KeyCode::Char('?') => app.modal = Some(Modal::Help),
        KeyCode::Char('v') => {
            app.selection_mode = !app.selection_mode;
        }
        KeyCode::Char('+') => app.modal = Some(Modal::ProjectInput(ProjectInput::new())),
        KeyCode::Char('-') => app.toggle_fold_all(),
        KeyCode::Char('e') => {
            if let Some(session) = app.selected_session() {
                app.modal = Some(Modal::Rename(RenameInput::new(
                    session.id.clone(),
                    session.label.clone(),
                )));
            }
        }
        KeyCode::Char('p') => {
            let pinned = app.selected_session().map(|s| s.pinned).unwrap_or(false);
            request_verb(
                app,
                Verb::Pin(!pinned),
                preview_grid(app, term_w, term_h),
                results,
            );
        }
        // The archive library for the selected session's project — the
        // sidebar's Archive footer row is gone; this key and the project
        // context menu's "Archived (N)" are the ways in.
        KeyCode::Char('a') => {
            if let Some(project_id) = app.selected_project_id() {
                if app.archived_count_in_project(&project_id) > 0 {
                    app.archive_query.clear();
                    app.selected_recent = None;
                    app.selected_worktree_folder = None;
                    app.selected_new_session = None;
                    app.selected_archive = Some((project_id, 0));
                } else {
                    app.info = Some("no archived sessions in this project".into());
                }
            }
        }
        KeyCode::Enter if app.selected_add_project => {
            app.modal = Some(Modal::ProjectInput(ProjectInput::new()));
        }
        KeyCode::Enter if app.selected_new_session.is_some() => {
            // Same path as `n`, so the picker, destination line and spawn
            // rules stay in one place.
            return handle_key(
                app,
                KeyEvent::new(KeyCode::Char('n'), key.modifiers),
                term_w,
                term_h,
                snapshots,
                results,
            );
        }
        KeyCode::Enter => match app.selected_worktree_folder.clone() {
            // ⏎ on a worktree folder row folds/unfolds it in place —
            // pure visibility, so no rescan is needed.
            Some(worktree) => {
                if !app.expanded_worktrees.remove(&worktree) {
                    app.expanded_worktrees.insert(worktree);
                }
            }
            None => enter_terminal_focus(app, term_w, term_h),
        },
        _ => {}
    }
    true
}

/// Wheel over the preview: virtual scrollback via disk-replay snapshots.
/// Never blocks the UI thread and never types into the session — clamping
/// happens against the latest replay snapshot's real history depth.
fn preview_wheel(
    app: &mut App,
    up: bool,
    col: u16,
    row: u16,
    term_width: u16,
    term_height: u16,
    snapshots: &SnapshotService,
) {
    let snapshot = app.selected_session().and_then(|r| snapshots.get(&r.id));
    if let Some(snapshot) = snapshot.as_ref() {
        // Running hosts from before mode metadata was added omit these
        // fields. Preserve their old no-scrollback fallback until that
        // session is restarted; current hosts always take the exact branches
        // below.
        if !snapshot.input_modes_known
            && snapshot.scrollback_rows == 0
            && snapshot.scroll_offset_rows == 0
            && app.preview_scroll == 0
        {
            if app.terminal_focus {
                if let Some(dir) = app.selected_session().map(|s| s.dir()) {
                    let target = ui::preview_terminal_rect(
                        preview_area(app, term_width, term_height),
                        snapshot,
                    );
                    let cx = col
                        .clamp(
                            target.x,
                            target.x.saturating_add(target.width.saturating_sub(1)),
                        )
                        .saturating_sub(target.x)
                        .saturating_add(1);
                    let cy = row
                        .clamp(
                            target.y,
                            target.y.saturating_add(target.height.saturating_sub(1)),
                        )
                        .saturating_sub(target.y)
                        .saturating_add(1);
                    let button = if up { 64 } else { 65 };
                    let _ = app.input.send(&dir, format!("\x1b[<{button};{cx};{cy}M"));
                }
            }
            return;
        }
        // Same routing Herdr gets from Ghostty's live input modes:
        // mouse-reporting children own the wheel; alternate-screen programs
        // with DEC 1007 receive cursor keys; everything else uses host
        // scrollback. Do not infer alternate screen from an empty history —
        // a fresh shell also has zero scrollback.
        if snapshot.mouse_reporting {
            if let Some(dir) = app.selected_session().map(|s| s.dir()) {
                app.preview_scroll = 0;
                let target =
                    ui::preview_terminal_rect(preview_area(app, term_width, term_height), snapshot);
                let cx = col
                    .clamp(
                        target.x,
                        target.x.saturating_add(target.width.saturating_sub(1)),
                    )
                    .saturating_sub(target.x)
                    .saturating_add(1);
                let cy = row
                    .clamp(
                        target.y,
                        target.y.saturating_add(target.height.saturating_sub(1)),
                    )
                    .saturating_sub(target.y)
                    .saturating_add(1);
                let button = if up { 64 } else { 65 };
                let _ = app.input.send(&dir, format!("\x1b[<{button};{cx};{cy}M"));
            }
            return;
        }
        if snapshot.alternate_screen && snapshot.mouse_alternate_scroll {
            if let Some(dir) = app.selected_session().map(|s| s.dir()) {
                app.preview_scroll = 0;
                let sequence = match (up, snapshot.application_cursor) {
                    (true, true) => "\x1bOA",
                    (false, true) => "\x1bOB",
                    (true, false) => "\x1b[A",
                    (false, false) => "\x1b[B",
                };
                let _ = app.input.send(&dir, sequence);
            }
            return;
        }
        if snapshot.alternate_screen {
            return;
        }
    }
    app.preview_scroll = if up {
        app.preview_scroll.saturating_add(3)
    } else {
        app.preview_scroll.saturating_sub(3)
    };
    if let Some(snapshot) = snapshot {
        app.preview_scroll = app.preview_scroll.min(snapshot.scrollback_rows);
    }
}

/// Add `expanded` (an existing directory) as a project — the add-project
/// dialog's commit, shared by ⏎ and the mouse's [ add ] chip.
fn commit_add_project(app: &mut App, expanded: &str) {
    let name = std::path::Path::new(expanded)
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| expanded.to_string());
    match add_project_to_app_state(&name, expanded) {
        Ok(AddProject::Added) => app.info = Some(format!("project added: {name}")),
        // Already a project: go to it rather than scolding —
        // the desktop's `ensureProject` does the same.
        Ok(AddProject::Existing { id, name }) => {
            app.collapsed.remove(&name);
            app.selected_archive = None;
            app.selected_worktree_folder = None;
            app.selected_add_project = false;
            // A project with sessions has no "+ New session" row to land
            // on — take its first session instead.
            let has_row = app.model.items.iter().any(
                |item| matches!(item, SidebarItem::NewSession { project, .. } if *project == id),
            );
            let first_session = app.model.rows.iter().find(|row| {
                let group = if row.group_id.is_empty() {
                    &row.project_id
                } else {
                    &row.group_id
                };
                *group == id && !row.archived
            });
            match first_session {
                Some(row) if !has_row => {
                    app.selected_new_session = None;
                    app.selected_id = Some(row.id.clone());
                }
                _ => app.selected_new_session = Some(id),
            }
            app.info = Some(format!("{name} already covers that folder"));
        }
        Err(e) => app.info = Some(e),
    }
}

/// Settings mouse map — mirrors `draw_settings`: row 0 is the panel border,
/// row 1 the back row, row 3+ the section list; the detail pane's rows line
/// up with the section's own list (Remote devices start at detail row 3).
fn handle_settings_mouse(app: &mut App, mouse: MouseEvent, divider_col: u16) -> bool {
    let Some((section, row)) = app.settings else {
        return false;
    };
    match mouse.kind {
        MouseEventKind::Down(MouseButton::Left) if mouse.column < divider_col => {
            match mouse.row {
                1 => app.settings = None, // ‹ back
                r if r >= 3 && (r - 3) < 6 => app.settings = Some(((r - 3) as usize, 0)),
                _ => {}
            }
            true
        }
        MouseEventKind::Down(MouseButton::Left) if mouse.row == 0 => {
            // The [+] sits in the detail pane's title bar, three cells wide
            // against its right edge.
            let width = crate::LAST_TERM_WIDTH.load(std::sync::atomic::Ordering::Relaxed);
            if mouse.column + 4 >= width && width > 0 {
                match section {
                    0 => app.settings = Some((0, app_state_presets().len())),
                    3 => {
                        app.settings = None;
                        app.modal = Some(Modal::ProjectInput(ProjectInput::new()));
                    }
                    _ => {}
                }
            }
            true
        }
        MouseEventKind::Down(MouseButton::Left) => {
            // Detail pane rows are selectable (and clicking a policy or
            // preset row also activates it, like the desktop toggles).
            // Screen row = 1 (the pane border) + the section's header lines
            // in ui::settings_detail (Remote: ui::draw_remote_settings) —
            // keep these in lockstep with it.
            let first_row: u16 = match section {
                0 => 3, // presets list (intro + blank)
                1 => 3, // access policies (intro + blank)
                2 => 3, // paired devices (card border, serving line, table header)
                5 => 3, // the auto-stop-and-archive cutoff (intro + blank)
                _ => u16::MAX,
            };
            if mouse.row >= first_row {
                let index = (mouse.row - first_row) as usize;
                // Remote: only the device rows sit at a fixed offset — the
                // Unpeel Link rows below them start after a variable-height
                // license block, so they stay keyboard-only.
                let clickable = if section == 2 {
                    index < paired_devices().len()
                } else {
                    index <= settings_row_count(app, section)
                };
                if clickable {
                    app.settings = Some((section, index));
                    match section {
                        // Clicking the blank add row below the presets only
                        // selects it (typing starts the draft) — a toggle
                        // there would rewrite app-state for nothing.
                        0 if index < app_state_presets().len() => {
                            let _ = toggle_preset_enabled(index);
                        }
                        1 => {
                            let _ = cycle_access_setting(index);
                        }
                        5 => {
                            let _ = cycle_auto_stop_archive_setting();
                        }
                        _ => {}
                    }
                }
            }
            true
        }
        MouseEventKind::ScrollUp => {
            app.settings = Some((section, row.saturating_sub(1)));
            true
        }
        MouseEventKind::ScrollDown => {
            app.settings = Some((section, (row + 1).min(settings_row_count(app, section))));
            true
        }
        _ => true, // swallow drags/ups while settings owns the screen
    }
}

/// SGR mouse encoding for the focused session: full-screen agent UIs (a
/// clickable "jump to bottom", menus) do their own mouse tracking, so a
/// focused pane relays presses, drags, and releases at pane-relative
/// coordinates exactly like a real terminal would.
fn forward_mouse_to_session(app: &App, mouse: &MouseEvent, target: Rect) -> bool {
    let button = match mouse.kind {
        MouseEventKind::Down(MouseButton::Left) | MouseEventKind::Up(MouseButton::Left) => 0,
        MouseEventKind::Down(MouseButton::Middle) | MouseEventKind::Up(MouseButton::Middle) => 1,
        MouseEventKind::Down(MouseButton::Right) | MouseEventKind::Up(MouseButton::Right) => 2,
        MouseEventKind::Drag(MouseButton::Left) => 32,
        MouseEventKind::Drag(MouseButton::Middle) => 33,
        MouseEventKind::Drag(MouseButton::Right) => 34,
        MouseEventKind::Moved => 35,
        _ => return false,
    };
    let modifier_bits = (if mouse
        .modifiers
        .contains(ratatui::crossterm::event::KeyModifiers::SHIFT)
    {
        4
    } else {
        0
    }) + (if mouse
        .modifiers
        .contains(ratatui::crossterm::event::KeyModifiers::ALT)
    {
        8
    } else {
        0
    }) + (if mouse
        .modifiers
        .contains(ratatui::crossterm::event::KeyModifiers::CONTROL)
    {
        16
    } else {
        0
    });
    let button = button + modifier_bits;
    let released = matches!(mouse.kind, MouseEventKind::Up(_));
    let Some(dir) = app.selected_session().map(|s| s.dir()) else {
        return false;
    };
    let col = mouse
        .column
        .clamp(
            target.x,
            target.x.saturating_add(target.width.saturating_sub(1)),
        )
        .saturating_sub(target.x)
        .saturating_add(1);
    let row = mouse
        .row
        .clamp(
            target.y,
            target.y.saturating_add(target.height.saturating_sub(1)),
        )
        .saturating_sub(target.y)
        .saturating_add(1);
    let suffix = if released { 'm' } else { 'M' };
    let sequence = format!("\x1b[<{button};{col};{row}{suffix}");
    app.input.send(&dir, sequence).is_ok()
}

/// Build and show the local-sites dropdown: open rows for every URL plus
/// Stop rows for the ones whose server resolves to a hosted session's
/// process tree. Resolution (lsof + ancestry walk, ~100ms) runs only here,
/// on the click — never on a timer.
fn open_local_urls_menu(app: &mut App, urls: Vec<String>) {
    let mut rows: Vec<LocalUrlRow> = urls.iter().cloned().map(LocalUrlRow::Open).collect();
    for url in &urls {
        if let Some(server) = unpeel_core::local_urls::server_for_url(url) {
            if server.session_id.is_some() {
                let compact = url
                    .trim_start_matches("https://")
                    .trim_start_matches("http://")
                    .split('/')
                    .next()
                    .unwrap_or(url)
                    .to_string();
                rows.push(LocalUrlRow::Stop {
                    url: url.clone(),
                    label: format!("{compact} ({})", server.command),
                });
            }
        }
    }
    app.modal = Some(Modal::LocalUrls { rows, selected: 0 });
}

/// Run one dropdown row: open the site, or stop its session-owned server
/// (resolve-and-kill in one step inside unpeel-core — a stale pid is never
/// signaled).
fn activate_local_url_row(app: &mut App, row: &LocalUrlRow) {
    match row {
        LocalUrlRow::Open(url) => open_local_url(app, url),
        LocalUrlRow::Stop { url, .. } => {
            app.info = Some(match unpeel_core::local_urls::stop_server_for_url(url) {
                Ok(server) => format!("stopped {} (pid {})", server.command, server.pid),
                Err(error) => error,
            });
        }
    }
}

/// Open a detected local site in the system browser. The URLs come from the
/// host's loopback-only detector, but re-check the scheme anyway so this can
/// never exec an opener on arbitrary manifest content.
fn open_local_url(app: &mut App, url: &str) {
    if !url.starts_with("http://") && !url.starts_with("https://") {
        return;
    }
    let opener = if cfg!(target_os = "macos") {
        "open"
    } else {
        "xdg-open"
    };
    let spawned = std::process::Command::new(opener)
        .arg(url)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .is_ok();
    app.info = Some(if spawned {
        format!("opening {url}")
    } else {
        format!("couldn't open {url}")
    });
}

fn preview_area(app: &App, term_width: u16, term_height: u16) -> Rect {
    Rect::new(
        app.sidebar_width,
        0,
        term_width.saturating_sub(app.sidebar_width),
        term_height,
    )
}

fn mouse_in_rect(mouse: &MouseEvent, rect: Rect) -> bool {
    mouse.column >= rect.x
        && mouse.column < rect.x.saturating_add(rect.width)
        && mouse.row >= rect.y
        && mouse.row < rect.y.saturating_add(rect.height)
}

fn selection_point(mouse: &MouseEvent, rect: Rect) -> (usize, u16) {
    let col = mouse
        .column
        .clamp(rect.x, rect.x.saturating_add(rect.width.saturating_sub(1)))
        .saturating_sub(rect.x);
    let row = mouse
        .row
        .clamp(rect.y, rect.y.saturating_add(rect.height.saturating_sub(1)))
        .saturating_sub(rect.y) as usize;
    (row, col)
}

/// Herdr-style pane mouse routing: a child that enabled DEC mouse tracking
/// receives its button reports; otherwise a plain left drag selects and
/// copies from the rendered terminal grid without dropping terminal focus.
fn handle_preview_terminal_mouse(
    app: &mut App,
    mouse: &MouseEvent,
    term_width: u16,
    term_height: u16,
    snapshots: &SnapshotService,
) -> bool {
    if let Some(mut selection) = app.terminal_selection.take() {
        let target = ui::preview_terminal_rect(
            preview_area(app, term_width, term_height),
            &selection.snapshot,
        );
        match mouse.kind {
            MouseEventKind::Drag(MouseButton::Left) => {
                let (row, col) = selection_point(mouse, target);
                selection.drag(row, col);
                app.terminal_selection = Some(selection);
                return true;
            }
            MouseEventKind::Up(MouseButton::Left) => {
                let (row, col) = selection_point(mouse, target);
                selection.drag(row, col);
                if let Some(text) = selection.selected_text().filter(|text| !text.is_empty()) {
                    app.info = Some(match write_terminal_clipboard(&text) {
                        Ok(()) => "selection copied".into(),
                        Err(error) => format!("could not copy selection: {error}"),
                    });
                    selection.finish();
                    app.terminal_selection = Some(selection);
                }
                return true;
            }
            MouseEventKind::Down(MouseButton::Left) => {
                // A new press replaces the old anchor below.
            }
            _ => app.terminal_selection = Some(selection),
        }
    }

    if !matches!(
        mouse.kind,
        MouseEventKind::Down(_)
            | MouseEventKind::Up(_)
            | MouseEventKind::Drag(_)
            | MouseEventKind::Moved
    ) {
        return false;
    }
    let Some((session_id, running)) = app
        .selected_session()
        .map(|session| (session.id.clone(), session.running))
    else {
        return false;
    };
    if !running {
        return false;
    }
    let Some(snapshot) = snapshots.get(&session_id) else {
        return false;
    };
    let target = ui::preview_terminal_rect(preview_area(app, term_width, term_height), &snapshot);
    if !mouse_in_rect(mouse, target) {
        return false;
    }

    if !snapshot.input_modes_known {
        if app.terminal_focus && !matches!(mouse.kind, MouseEventKind::Moved) {
            forward_mouse_to_session(app, mouse, target);
            return true;
        }
        return false;
    }

    if snapshot.mouse_reporting {
        if matches!(mouse.kind, MouseEventKind::Down(_)) && !app.terminal_focus {
            enter_terminal_focus(app, term_width, term_height);
        }
        let report = match mouse.kind {
            MouseEventKind::Drag(_) => snapshot.mouse_button_motion,
            MouseEventKind::Moved => snapshot.mouse_any_motion,
            _ => true,
        };
        if report && app.terminal_focus {
            forward_mouse_to_session(app, mouse, target);
        }
        return report || !matches!(mouse.kind, MouseEventKind::Moved);
    }

    if matches!(mouse.kind, MouseEventKind::Down(MouseButton::Left)) {
        if !app.terminal_focus {
            enter_terminal_focus(app, term_width, term_height);
        }
        if app.terminal_focus {
            let (row, col) = selection_point(mouse, target);
            app.terminal_selection =
                Some(TerminalSelection::anchor(session_id, snapshot, row, col));
        }
        return true;
    }
    false
}

fn handle_mouse(
    app: &mut App,
    mouse: MouseEvent,
    term_width: u16,
    term_height: u16,
    snapshots: &SnapshotService,
    results: &mpsc::Sender<VerbOutcome>,
) {
    // The rename dialog only captures mouse gestures that begin in its text
    // field. Clicks elsewhere still reach the sidebar, preserving the useful
    // "click another session to leave this draft" behavior.
    if matches!(app.modal, Some(Modal::Rename(_))) {
        let area = ratatui::layout::Rect::new(0, 0, term_width, term_height);
        let hit = match &app.modal {
            Some(Modal::Rename(input)) => {
                ui::rename_text_index_at(area, input, mouse.column, mouse.row)
            }
            _ => None,
        };
        let dragging = matches!(&app.modal, Some(Modal::Rename(input)) if input.dragging);
        match mouse.kind {
            MouseEventKind::Down(MouseButton::Left) if hit.is_some() => {
                if let (Some(index), Some(Modal::Rename(input))) = (hit, app.modal.as_mut()) {
                    input.begin_mouse_selection(index);
                }
                return;
            }
            MouseEventKind::Drag(MouseButton::Left) if dragging => {
                if let Some(Modal::Rename(input)) = app.modal.as_mut() {
                    let index = ui::rename_text_index_nearest(area, input, mouse.column, mouse.row);
                    input.drag_mouse_selection(index);
                }
                return;
            }
            MouseEventKind::Up(MouseButton::Left) if dragging => {
                if let Some(Modal::Rename(input)) = app.modal.as_mut() {
                    let index = ui::rename_text_index_nearest(area, input, mouse.column, mouse.row);
                    input.finish_mouse_selection(index);
                }
                return;
            }
            _ => {}
        }
    }
    // Sidebar activity popover: either reveal a live/unread Session or follow
    // the final All recent row. It owns click, hover, and wheel while open;
    // clicking off the frame dismisses without acting through it.
    if matches!(app.modal, Some(Modal::Activity { .. })) {
        let area = ratatui::layout::Rect::new(0, 0, term_width, term_height);
        match mouse.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                let selected = match &app.modal {
                    Some(Modal::Activity { selected }) => *selected,
                    _ => 0,
                };
                if let Some(index) =
                    ui::activity_menu_row_at(area, app, selected, mouse.column, mouse.row)
                {
                    let entries = app.activity_menu_entries();
                    app.modal = None;
                    if index == entries.len() {
                        app.open_recent_activity();
                    } else if let Some(entry) = entries.get(index) {
                        let id = entry.session_id.clone();
                        if app.reveal_session(&id, true) {
                            resize_selected_to_pane(app, snapshots, term_width, term_height);
                        }
                    }
                } else if !ui::activity_menu_frame_hit(area, app, mouse.column, mouse.row) {
                    app.modal = None;
                }
            }
            MouseEventKind::Moved => {
                let selected = match &app.modal {
                    Some(Modal::Activity { selected }) => *selected,
                    _ => 0,
                };
                if let Some(index) =
                    ui::activity_menu_row_at(area, app, selected, mouse.column, mouse.row)
                {
                    app.modal = Some(Modal::Activity { selected: index });
                }
            }
            MouseEventKind::ScrollUp => {
                if let Some(Modal::Activity { selected }) = app.modal.as_mut() {
                    *selected = selected.saturating_sub(1);
                }
            }
            MouseEventKind::ScrollDown => {
                let last = app.activity_menu_entries().len();
                if let Some(Modal::Activity { selected }) = app.modal.as_mut() {
                    *selected = (*selected + 1).min(last);
                }
            }
            _ => {}
        }
        return;
    }
    // An open preset picker is modal: it owns the mouse the same way it
    // owns the keyboard. Click a row to launch it, wheel to move the
    // selection, click anywhere outside to dismiss.
    if let Some(Modal::PresetPicker {
        presets,
        selected,
        target,
        anchor,
    }) = &app.modal
    {
        let count = presets.len();
        let selected = *selected;
        let anchor = *anchor;
        let area = ratatui::layout::Rect::new(0, 0, term_width, term_height);
        match mouse.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                if let Some(index) = ui::preset_picker_row_at(
                    area,
                    presets,
                    target,
                    selected,
                    anchor,
                    mouse.column,
                    mouse.row,
                ) {
                    let command = presets[index].1.clone();
                    app.modal = None;
                    launch_picked_preset(app, command, term_width, term_height, results);
                } else {
                    let rect = ui::preset_picker_rect(area, presets, target, anchor);
                    let on_frame = mouse.column >= rect.x
                        && mouse.column < rect.x + rect.width
                        && mouse.row >= rect.y
                        && mouse.row < rect.y + rect.height;
                    if !on_frame {
                        // Same as Esc: closing drops the explicit target.
                        app.modal = None;
                        app.pending_spawn_target = None;
                    }
                }
            }
            MouseEventKind::ScrollUp => {
                if let Some(Modal::PresetPicker { selected: s, .. }) = app.modal.as_mut() {
                    *s = s.saturating_sub(1);
                }
            }
            MouseEventKind::ScrollDown => {
                if let Some(Modal::PresetPicker { selected: s, .. }) = app.modal.as_mut() {
                    *s = (*s + 1).min(count.saturating_sub(1));
                }
            }
            _ => {}
        }
        return;
    }
    // The footer menu is modal the same way: click a row to follow it, wheel
    // to move, click off it to dismiss.
    if matches!(app.modal, Some(Modal::Menu { .. })) {
        let area = ratatui::layout::Rect::new(0, 0, term_width, term_height);
        match mouse.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                if let Some(index) = ui::menu_row_at(area, mouse.column, mouse.row) {
                    activate_menu(app, index);
                } else if !ui::menu_frame_hit(area, mouse.column, mouse.row) {
                    app.modal = None;
                }
            }
            MouseEventKind::ScrollUp => {
                if let Some(Modal::Menu { selected }) = app.modal.as_mut() {
                    *selected = selected.saturating_sub(1);
                }
            }
            MouseEventKind::ScrollDown => {
                if let Some(Modal::Menu { selected }) = app.modal.as_mut() {
                    *selected = (*selected + 1).min(MENU_ITEMS.len().saturating_sub(1));
                }
            }
            _ => {}
        }
        return;
    }
    // The local-sites dropdown owns the mouse the same way: click a row to
    // run it (open the URL or stop its server), wheel to move, click off it
    // to dismiss.
    if matches!(app.modal, Some(Modal::LocalUrls { .. })) {
        let area = ratatui::layout::Rect::new(0, 0, term_width, term_height);
        match mouse.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                let (row_hit, frame_hit) = match &app.modal {
                    Some(Modal::LocalUrls { rows, .. }) => {
                        let row = ui::local_urls_row_at(
                            area,
                            app.sidebar_width,
                            rows,
                            mouse.column,
                            mouse.row,
                        );
                        (
                            row.and_then(|i| rows.get(i).cloned()),
                            ui::local_urls_frame_hit(
                                area,
                                app.sidebar_width,
                                rows,
                                mouse.column,
                                mouse.row,
                            ),
                        )
                    }
                    _ => (None, false),
                };
                if let Some(row) = row_hit {
                    app.modal = None;
                    activate_local_url_row(app, &row);
                } else if !frame_hit {
                    app.modal = None;
                }
            }
            MouseEventKind::ScrollUp => {
                if let Some(Modal::LocalUrls { selected, .. }) = app.modal.as_mut() {
                    *selected = selected.saturating_sub(1);
                }
            }
            MouseEventKind::ScrollDown => {
                if let Some(Modal::LocalUrls { rows, selected }) = app.modal.as_mut() {
                    *selected = (*selected + 1).min(rows.len().saturating_sub(1));
                }
            }
            _ => {}
        }
        return;
    }
    // A project context menu owns the mouse the same way: hover moves the
    // highlight (herdr-style), a left click runs the row, a click outside
    // dismisses, and a second right-click reopens elsewhere.
    if matches!(app.modal, Some(Modal::Context(_))) {
        let area = ratatui::layout::Rect::new(0, 0, term_width, term_height);
        match mouse.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                let hit = match &app.modal {
                    Some(Modal::Context(menu)) => (
                        ui::context_menu_row_at(area, menu, mouse.column, mouse.row),
                        ui::context_menu_frame_hit(area, menu, mouse.column, mouse.row),
                    ),
                    _ => (None, false),
                };
                if let Some(index) = hit.0 {
                    if let Some(Modal::Context(menu)) = app.modal.take() {
                        activate_context_menu(app, menu, index, term_width, term_height, results);
                    }
                } else if !hit.1 {
                    app.modal = None;
                }
            }
            MouseEventKind::Moved => {
                app.mouse_pos = Some((mouse.column, mouse.row));
                let hover = match &app.modal {
                    Some(Modal::Context(menu)) => {
                        ui::context_menu_row_at(area, menu, mouse.column, mouse.row)
                    }
                    _ => None,
                };
                if let (Some(index), Some(Modal::Context(menu))) = (hover, app.modal.as_mut()) {
                    menu.selected = index;
                }
            }
            MouseEventKind::ScrollUp => {
                if let Some(Modal::Context(menu)) = app.modal.as_mut() {
                    menu.selected = menu.selected.saturating_sub(1);
                }
            }
            MouseEventKind::ScrollDown => {
                if let Some(Modal::Context(menu)) = app.modal.as_mut() {
                    menu.selected = (menu.selected + 1).min(menu.items.len().saturating_sub(1));
                }
            }
            MouseEventKind::Down(MouseButton::Right) => {
                // Reopen on whatever was right-clicked instead — close
                // first, then fall through to the opener below.
                app.modal = None;
            }
            _ => {}
        }
        if !matches!(mouse.kind, MouseEventKind::Down(MouseButton::Right)) {
            return;
        }
    }
    // The add-project dialog owns the mouse: hover tints the row under the
    // cursor (and the [ add ] chip), a click picks a folder, a double-click
    // descends into it, [ add ] commits the picked folder, the wheel moves
    // the highlight, and a click outside cancels — same as esc.
    if matches!(app.modal, Some(Modal::ProjectInput(_))) {
        let area = ratatui::layout::Rect::new(0, 0, term_width, term_height);
        match mouse.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                let (row_hit, add_hit, frame_hit) = match &app.modal {
                    Some(Modal::ProjectInput(input)) => (
                        ui::project_input_row_at(area, input, mouse.column, mouse.row),
                        ui::project_input_add_hit(area, input, mouse.column, mouse.row),
                        ui::project_input_frame_hit(area, input, mouse.column, mouse.row),
                    ),
                    _ => (None, false, false),
                };
                if add_hit {
                    if let Some(Modal::ProjectInput(input)) = app.modal.take() {
                        // The highlighted folder; with nothing to highlight
                        // (a leaf directory), the typed path itself.
                        let path = input
                            .matches
                            .get(input.selected)
                            .cloned()
                            .unwrap_or_else(|| input.expanded());
                        if !path.is_empty() && std::path::Path::new(&path).is_dir() {
                            commit_add_project(app, &path);
                        } else {
                            app.modal = Some(Modal::ProjectInput(input));
                        }
                    }
                } else if let Some(index) = row_hit {
                    if let Some(Modal::ProjectInput(input)) = app.modal.as_mut() {
                        let double = input.last_click.as_ref().is_some_and(|(prev, at)| {
                            *prev == index && at.elapsed() <= DOUBLE_CLICK
                        });
                        input.selected = index;
                        if double {
                            input.complete();
                        } else {
                            input.last_click = Some((index, Instant::now()));
                        }
                    }
                } else if !frame_hit {
                    app.modal = None;
                }
            }
            MouseEventKind::Moved => {
                app.mouse_pos = Some((mouse.column, mouse.row));
            }
            MouseEventKind::ScrollUp => {
                if let Some(Modal::ProjectInput(input)) = app.modal.as_mut() {
                    input.selected = input.selected.saturating_sub(1);
                }
            }
            MouseEventKind::ScrollDown => {
                if let Some(Modal::ProjectInput(input)) = app.modal.as_mut() {
                    input.selected =
                        (input.selected + 1).min(input.matches.len().saturating_sub(1));
                }
            }
            _ => {}
        }
        return;
    }
    let divider_col = app.sidebar_width.saturating_sub(1);
    // The update toast overlays everything below it — a click on it
    // dismisses (persisted per version) before any pane sees the press.
    if let MouseEventKind::Down(MouseButton::Left) = mouse.kind {
        if app.info.is_none() {
            if let Some(version) = app.update_available.clone() {
                let area = ratatui::layout::Rect::new(0, 0, term_width, term_height);
                if ui::update_toast_hit(area, &version, mouse.column, mouse.row) {
                    update::record_dismissed(&version);
                    app.update_available = None;
                    return;
                }
            } else if let Some(hint) = &app.env_hint {
                // A one-time environment tip dismisses the same way, and the
                // marker keeps it dismissed across launches.
                let area = ratatui::layout::Rect::new(0, 0, term_width, term_height);
                if ui::hint_toast_hit(area, &hint.text, mouse.column, mouse.row) {
                    let _ = std::fs::write(
                        unpeel_core::app_paths::unpeel_home().join(hint.marker),
                        b"dismissed",
                    );
                    app.env_hint = None;
                    return;
                }
            }
        }
    }
    if handle_settings_mouse(app, mouse, divider_col) {
        return;
    }
    // All recent replaces the preview pane, so pointer input there navigates
    // the history list and must never leak through to terminal focus/scroll.
    if let Some(selected) = app.selected_recent {
        let preview = preview_area(app, term_width, term_height);
        let entries = app.recent_activity_entries();
        let last = entries.len().saturating_sub(1);
        match mouse.kind {
            MouseEventKind::Down(MouseButton::Left) if mouse.column >= divider_col => {
                if let Some(index) =
                    ui::recent_activity_row_at(preview, app, selected, mouse.column, mouse.row)
                {
                    app.selected_recent = Some(index);
                    if let Some(id) = entries
                        .get(index)
                        .and_then(|entry| entry.session_id.clone())
                    {
                        if app.reveal_session(&id, true) {
                            resize_selected_to_pane(app, snapshots, term_width, term_height);
                        }
                    }
                }
            }
            MouseEventKind::ScrollUp if mouse.column >= divider_col => {
                app.selected_recent = Some(selected.saturating_sub(1));
            }
            MouseEventKind::ScrollDown if mouse.column >= divider_col => {
                app.selected_recent = Some((selected + 1).min(last));
            }
            _ => {}
        }
        if mouse.column >= divider_col {
            return;
        }
    }
    // Local-sites chip on the preview's top border: left-click opens a
    // single live URL directly (several drop the menu); right-click always
    // drops the menu, which is where Stop lives. Checked before generic
    // preview routing so the border click doesn't just focus the terminal.
    if let MouseEventKind::Down(button @ (MouseButton::Left | MouseButton::Right)) = mouse.kind {
        if app.settings.is_none() && app.selected_archive.is_none() && app.modal.is_none() {
            let urls = app.local_site_urls();
            let chip = if urls.is_empty() {
                None
            } else {
                let preview = preview_area(app, term_width, term_height);
                ui::local_urls_chip_rect(preview, &urls)
                    .filter(|rect| mouse_in_rect(&mouse, *rect))
                    .map(|_| urls)
            };
            if let Some(urls) = chip {
                if urls.len() == 1 && button == MouseButton::Left {
                    open_local_url(app, &urls[0]);
                } else {
                    open_local_urls_menu(app, urls);
                }
                return;
            }
        }
    }
    if handle_preview_terminal_mouse(app, &mouse, term_width, term_height, snapshots) {
        return;
    }
    match mouse.kind {
        MouseEventKind::Down(MouseButton::Left) => {
            if mouse.column == divider_col {
                app.dragging_divider = true;
                return;
            }
            if mouse.row == 0 && ui::activity_button_hit(mouse.column, divider_col) {
                app.terminal_focus = false;
                app.modal = Some(Modal::Activity { selected: 0 });
                return;
            }
            if mouse.column > divider_col {
                enter_terminal_focus(app, term_width, term_height);
                return;
            }
            // The "menu" label on the sidebar's bottom edge opens the footer
            // menu (settings, keybindings, command palette) — herdr-style.
            // Bottom-right, its counterpart: the fold-all toggle.
            if mouse.column < divider_col && mouse.row == term_height.saturating_sub(1) {
                if ui::menu_label_hit(mouse.column) {
                    app.terminal_focus = false;
                    app.modal = Some(Modal::Menu { selected: 0 });
                    return;
                }
                if ui::fold_label_hit(mouse.column, divider_col) {
                    app.terminal_focus = false;
                    app.toggle_fold_all();
                    return;
                }
            }
            if mouse.column < divider_col && mouse.row >= 1 {
                app.terminal_focus = false;
                let pos = app.sidebar_scroll + (mouse.row - 1) as usize;
                // The right-edge "+ New" on a project header ACTS, like the
                // "+ New session" row it stands in for: new session in
                // that project. The zone works even where hover never
                // painted it (terminals without motion reporting).
                if mouse.column + HEADER_ADD_ZONE >= divider_col {
                    match app.visible_items().get(pos).map(|item| (*item).clone()) {
                        Some(SidebarItem::Header(name)) => {
                            if let Some(project_id) = app.project_id_for_header(&name) {
                                open_preset_dropdown_for(
                                    app,
                                    Some((project_id, name)),
                                    (divider_col.saturating_sub(2), mouse.row.saturating_add(1)),
                                );
                                return;
                            }
                        }
                        // Worktree folder rows carry the same affordance,
                        // scoped to the worktree's own project.
                        Some(SidebarItem::WorktreeHeader {
                            project_id, name, ..
                        }) => {
                            open_preset_dropdown_for(
                                app,
                                Some((project_id, name)),
                                (divider_col.saturating_sub(2), mouse.row.saturating_add(1)),
                            );
                            return;
                        }
                        _ => {}
                    }
                }
                match app.visible_items().get(pos) {
                    Some(SidebarItem::Session(i)) => {
                        app.dragging_row = Some((app.model.rows[*i].id.clone(), pos));
                    }
                    Some(SidebarItem::Header(name)) => {
                        app.dragging_project = Some(((*name).clone(), pos, pos));
                    }
                    Some(SidebarItem::WorktreeHeader {
                        project_id, parent, ..
                    }) => {
                        app.dragging_folder = Some(FolderDrag {
                            project_id: project_id.clone(),
                            parent_id: parent.clone(),
                            start: pos,
                            drop_pos: pos,
                        });
                    }
                    _ => {}
                }
                let pos = app.sidebar_scroll + (mouse.row - 1) as usize;
                let visible = app.visible_items();
                match visible.get(pos).map(|item| (*item).clone()) {
                    // Project and inline-folder headers act on RELEASE (see
                    // the Up arm): pressing one starts a possible drag.
                    Some(SidebarItem::Header(_) | SidebarItem::WorktreeHeader { .. }) => {}
                    Some(item @ SidebarItem::Session(i)) => {
                        let id = app.model.rows[i].id.clone();
                        let double = app
                            .last_click
                            .as_ref()
                            .is_some_and(|(prev, at)| *prev == id && at.elapsed() <= DOUBLE_CLICK);
                        app.select_item(&item);
                        resize_selected_to_pane(app, snapshots, term_width, term_height);
                        if double {
                            // Double-click a session name to rename it, matching
                            // the desktop app. Uses the same dialog as `e`.
                            app.last_click = None;
                            let label = app
                                .selected_session()
                                .map(|s| s.label.clone())
                                .unwrap_or_default();
                            app.modal = Some(Modal::Rename(RenameInput::new(id, label)));
                        } else {
                            // Mouse-up classifies click vs drag and records
                            // the double-click clock after the gesture.
                            app.last_click = None;
                        }
                    }
                    // A click on "+ New session" ACTS: it reads as a
                    // button, so highlight-then-⏎ (right for the worktree
                    // rows, which are places you navigate to) would just
                    // look broken.
                    Some(item @ SidebarItem::NewSession { .. }) => {
                        app.select_item(&item);
                        let target = selected_new_session_target(app);
                        open_preset_dropdown_for(
                            app,
                            target,
                            (mouse.column, mouse.row.saturating_add(1)),
                        );
                    }
                    Some(item @ SidebarItem::AddProject) => {
                        app.select_item(&item);
                        app.modal = Some(Modal::ProjectInput(ProjectInput::new()));
                    }
                    None => {}
                }
            }
        }
        MouseEventKind::Drag(MouseButton::Left) if app.dragging_project.is_some() => {
            if mouse.row >= 1 {
                let pos = app.sidebar_scroll + (mouse.row - 1) as usize;
                if let Some((name, start, _)) = app.dragging_project.take() {
                    let last = app.visible_items().len().saturating_sub(1);
                    app.dragging_project = Some((name, start, pos.min(last)));
                }
            }
        }
        MouseEventKind::Drag(MouseButton::Left) if app.dragging_folder.is_some() => {
            if mouse.row >= 1 {
                let pos = app.sidebar_scroll + (mouse.row - 1) as usize;
                let last = app.visible_items().len().saturating_sub(1);
                if let Some(drag) = app.dragging_folder.as_mut() {
                    drag.drop_pos = pos.min(last);
                }
            }
        }
        MouseEventKind::Drag(MouseButton::Left) if app.dragging_row.is_some() => {
            if mouse.row >= 1 {
                let pos = app.sidebar_scroll + (mouse.row - 1) as usize;
                if let Some((id, _)) = app.dragging_row.take() {
                    app.dragging_row =
                        Some((id, pos.min(app.visible_items().len().saturating_sub(1))));
                }
            }
        }
        MouseEventKind::Drag(MouseButton::Left) if app.dragging_divider => {
            let max = term_width.saturating_sub(ui::MIN_SIDEBAR_WIDTH);
            app.sidebar_width =
                (mouse.column + 1).clamp(ui::MIN_SIDEBAR_WIDTH, ui::MAX_SIDEBAR_WIDTH.min(max));
        }
        MouseEventKind::Up(_) => {
            app.dragging_divider = false;
            if let Some((id, drop_pos)) = app.dragging_row.take() {
                let start_pos = app.visible_items().iter().position(|item| match item {
                    SidebarItem::Session(index) => app.model.rows[*index].id == id,
                    _ => false,
                });
                let was_click = start_pos == Some(drop_pos);
                app.commit_drag(&id, drop_pos);
                if was_click
                    && app.modal.is_none()
                    && app.selected_id.as_deref() == Some(id.as_str())
                {
                    // A Session row is an open action: once this is known to
                    // be a click (not a reorder drag), it is ready for input.
                    focus_terminal_from_click(app, snapshots, term_width, term_height);
                    app.last_click = Some((id, Instant::now()));
                } else if !was_click {
                    app.last_click = None;
                }
            }
            if let Some((name, start, drop_pos)) = app.dragging_project.take() {
                if drop_pos == start {
                    // Never moved: that was a click, so fold the project.
                    if !app.collapsed.remove(&name) {
                        app.collapsed.insert(name);
                    }
                } else {
                    app.commit_project_drag(&name, drop_pos);
                }
            }
            if let Some(drag) = app.dragging_folder.take() {
                if drag.drop_pos == drag.start {
                    // Never moved: this was a normal disclosure click.
                    if !app.expanded_worktrees.remove(&drag.project_id) {
                        app.expanded_worktrees.insert(drag.project_id.clone());
                    }
                    // Git worktrees remain selectable; groups never are.
                    if let Some(item) = app
                        .model
                        .items
                        .iter()
                        .find(|item| {
                            matches!(
                                item,
                                SidebarItem::WorktreeHeader {
                                    project_id,
                                    is_group: false,
                                    ..
                                } if project_id == &drag.project_id
                            )
                        })
                        .cloned()
                    {
                        app.select_item(&item);
                    }
                } else {
                    app.commit_folder_drag(&drag.project_id, &drag.parent_id, drag.drop_pos);
                }
            }
        }
        // Wheel is region-scoped: over the sidebar it moves the selection,
        // over the preview it scrolls the session's real scrollback.
        // The wheel scrolls the sidebar's VIEWPORT — it does not move the
        // selection. Scrolling to look around should never switch which
        // session the preview is showing (and never re-fit its PTY).
        MouseEventKind::ScrollUp => {
            if mouse.column < divider_col {
                app.scroll_sidebar(-3);
            } else {
                preview_wheel(
                    app,
                    true,
                    mouse.column,
                    mouse.row,
                    term_width,
                    term_height,
                    snapshots,
                );
            }
        }
        MouseEventKind::ScrollDown => {
            if mouse.column < divider_col {
                app.scroll_sidebar(3);
            } else {
                preview_wheel(
                    app,
                    false,
                    mouse.column,
                    mouse.row,
                    term_width,
                    term_height,
                    snapshots,
                );
            }
        }
        // Motion feeds the hover "+" on project headers. Terminals that
        // never report motion just never paint it — the click zone above
        // works either way.
        MouseEventKind::Moved => {
            app.mouse_pos = Some((mouse.column, mouse.row));
        }
        // Right-click a project header or session row: its context menu,
        // anchored at the click (the desktop's, minus what has no TUI
        // counterpart). Right-clicking a session also selects it, like the
        // desktop and macOS generally.
        MouseEventKind::Down(MouseButton::Right) => {
            if mouse.column >= divider_col || mouse.row == 0 {
                return;
            }
            let pos = app.sidebar_scroll + (mouse.row - 1) as usize;
            enum Target {
                Project(String),
                Folder {
                    project_id: String,
                    parent: String,
                    name: String,
                    is_group: bool,
                },
                Session(usize),
            }
            let visible = app.visible_items();
            let target = match visible.get(pos) {
                Some(SidebarItem::Header(name)) => Target::Project((*name).clone()),
                Some(SidebarItem::WorktreeHeader {
                    project_id,
                    parent,
                    name,
                    is_group,
                    ..
                }) => Target::Folder {
                    project_id: project_id.clone(),
                    parent: parent.clone(),
                    name: name.clone(),
                    is_group: *is_group,
                },
                Some(SidebarItem::Session(i)) => Target::Session(*i),
                _ => return,
            };
            drop(visible);
            app.terminal_focus = false;
            match target {
                Target::Project(name) => {
                    let Some(project_id) = app.project_id_for_header(&name) else {
                        return;
                    };
                    let items = project_menu_items(app, &project_id, &name);
                    app.modal = Some(Modal::Context(ContextMenu {
                        title: name.clone(),
                        project_id,
                        name,
                        session_id: None,
                        anchor: (mouse.column, mouse.row),
                        selected: 0,
                        items,
                    }));
                }
                // A worktree folder row: the project verbs scoped to the
                // worktree's own project, plus fold/unfold.
                Target::Folder {
                    project_id,
                    parent,
                    name,
                    is_group,
                } => {
                    if !is_group {
                        app.select_item(&SidebarItem::WorktreeHeader {
                            project_id: project_id.clone(),
                            parent: parent.clone(),
                            name: name.clone(),
                            branch: String::new(),
                            count: 0,
                            is_group,
                        });
                    }
                    let items = worktree_menu_items(app, &project_id, &parent, is_group);
                    app.modal = Some(Modal::Context(ContextMenu {
                        title: name.clone(),
                        project_id,
                        name,
                        session_id: None,
                        anchor: (mouse.column, mouse.row),
                        selected: 0,
                        items,
                    }));
                }
                Target::Session(i) => {
                    let row = &app.model.rows[i];
                    let items = session_menu_items(app, row);
                    let title = row.label.clone();
                    let session_id = row.id.clone();
                    let project_id = if row.group_id.is_empty() {
                        row.project_id.clone()
                    } else {
                        row.group_id.clone()
                    };
                    let name = app.project_name_for(&session_id);
                    app.select_item(&SidebarItem::Session(i));
                    resize_selected_to_pane(app, snapshots, term_width, term_height);
                    app.modal = Some(Modal::Context(ContextMenu {
                        title,
                        project_id,
                        name,
                        session_id: Some(session_id),
                        anchor: (mouse.column, mouse.row),
                        selected: 0,
                        items,
                    }));
                }
            }
        }
        _ => {}
    }
}

pub(crate) enum LinkWorkerRequest {
    Activate {
        raw_key: String,
    },
    Entitlement {
        mac_id: String,
        license_key: String,
        activation: Option<unpeel_core::license::ActivationCommit>,
    },
    DeactivateRemote {
        license_key: String,
    },
}

enum LinkWorkerOutcome {
    Activation(Result<unpeel_core::license::PendingActivation, String>),
    Entitlement {
        license_key: String,
        activation: Option<unpeel_core::license::ActivationCommit>,
        result: Result<
            unpeel_core::license::PendingRelayEntitlement,
            unpeel_core::license::RelayEntitlementError,
        >,
    },
    Deactivation(Result<(), String>),
}

fn start_link_worker() -> (
    mpsc::Sender<LinkWorkerRequest>,
    mpsc::Receiver<LinkWorkerOutcome>,
) {
    let (requests, request_rx) = mpsc::channel::<LinkWorkerRequest>();
    let (outcomes, outcome_rx) = mpsc::channel::<LinkWorkerOutcome>();
    std::thread::Builder::new()
        .name("unpeel-link-license".into())
        .spawn(move || {
            while let Ok(request) = request_rx.recv() {
                let outcome = match request {
                    LinkWorkerRequest::Activate { raw_key } => LinkWorkerOutcome::Activation(
                        unpeel_core::license::request_activation(&raw_key, "unpeel (terminal)"),
                    ),
                    LinkWorkerRequest::Entitlement {
                        mac_id,
                        license_key,
                        activation,
                    } => LinkWorkerOutcome::Entitlement {
                        result: unpeel_core::license::request_relay_entitlement_for_key(
                            &mac_id,
                            &license_key,
                        ),
                        license_key,
                        activation,
                    },
                    LinkWorkerRequest::DeactivateRemote { license_key } => {
                        LinkWorkerOutcome::Deactivation(
                            unpeel_core::license::request_deactivation_for_key(&license_key),
                        )
                    }
                };
                if outcomes.send(outcome).is_err() {
                    return;
                }
            }
        })
        .expect("spawn Link entitlement refresher");
    (requests, outcome_rx)
}

/// Keep the relay transport aligned with the current authority and serving
/// state. This deliberately does not own the LAN server: losing a license,
/// narrowing every paired device to Direct-only, or an entitlement expiring
/// must stop only Link and leave local phone control untouched.
fn suppress_current_relay_entitlement(app: &mut App) {
    app.link_blocked_entitlement =
        unpeel_core::relay_uplink::cached_entitlement_record().map(|record| record.entitlement);
    app.link_suppressed = true;
}

fn reconcile_relay_uplink(app: &mut App, mark_read: &mpsc::Sender<String>) {
    let owns_serving = tui_owns_link_authority_now(app);
    let cached = if owns_serving {
        match unpeel_core::license::allowed_cached_relay_entitlement() {
            Ok(cached) => cached,
            Err(error) => {
                app.link_suppressed = true;
                app.info = Some(format!("Unpeel Link authority could not be read: {error}"));
                None
            }
        }
    } else {
        None
    };
    if app.link_suppressed
        && cached.as_ref().is_some_and(|(entitlement, _)| {
            app.link_blocked_entitlement.as_ref() != Some(entitlement)
        })
    {
        // A different valid cache can only have arrived through a successful
        // TUI commit or while native owned serving. Never re-trust the exact
        // bearer that a relay/deactivation failure blocked.
        app.link_suppressed = false;
        app.link_blocked_entitlement = None;
    }
    let should_run = owns_serving
        && app.mobile_server.is_some()
        && !app.link_suppressed
        && relay::has_registrations()
        && cached.is_some();
    if should_run && app.relay_uplink.is_none() {
        let Some(expected_mobile_port) = app.mobile_server.as_ref().map(|server| server.port)
        else {
            return;
        };
        app.relay_uplink = Some(relay::start(
            std::sync::Arc::clone(&app.mobile_snapshot),
            mark_read.clone(),
            app.hook_port,
            std::sync::Arc::clone(&app.mobile_resizes),
            std::sync::Arc::clone(&app.approvals),
            expected_mobile_port,
        ));
        app.info = Some("serving paired phones (LAN + relay)".into());
    } else if !should_run {
        if let Some(uplink) = app.relay_uplink.take() {
            uplink.stop();
        }
    }
}

fn run(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    hook_events: Option<mpsc::Receiver<hook_listener::HookEventMessage>>,
    hook_port: Option<u16>,
    approval_hub: std::sync::Arc<approvals::ApprovalHub>,
    herdr_reporter: Option<&herdr::HerdrReporter>,
) -> io::Result<()> {
    let snapshot_service = SnapshotService::new();
    let (verb_tx, verb_rx) = mpsc::channel::<VerbOutcome>();
    let (mark_read_tx, mark_read_rx) = mpsc::channel::<String>();
    let (auto_archive_tx, auto_archive_rx) = mpsc::channel::<AutoArchiveOutcome>();
    let (link_worker_tx, link_worker_rx) = start_link_worker();
    let (event_tx, event_rx) = mpsc::channel::<AppEvent>();
    let output_wake = WakeGate::new(event_tx.clone());
    let mouse_motion = MouseMotionGate::new(event_tx.clone());
    let scroll_gate = ScrollGate::new(event_tx.clone());
    let pending_input = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    // Terminal events come off a dedicated reader thread so the loop can
    // block on one channel that BOTH input and live session output wake —
    // `event::poll` alone would sleep through streamed bytes for a full tick.
    {
        let input_tx = event_tx.clone();
        let mouse_motion = mouse_motion.clone();
        let scroll_gate = scroll_gate.clone();
        let pending_input = std::sync::Arc::clone(&pending_input);
        std::thread::spawn(move || loop {
            match event::read() {
                Ok(ev) => {
                    if let Event::Mouse(mouse) = &ev {
                        if matches!(
                            mouse.kind,
                            MouseEventKind::ScrollUp | MouseEventKind::ScrollDown
                        ) {
                            if !scroll_gate.push(*mouse) {
                                return;
                            }
                            continue;
                        }
                    }
                    // Every non-wheel event is an ordering barrier. A later
                    // wheel burst gets its own FIFO marker behind this event
                    // instead of merging into scrolling that happened before
                    // a key, click, paste, drag, or resize.
                    scroll_gate.seal();
                    if let Event::Mouse(mouse) = &ev {
                        if matches!(mouse.kind, MouseEventKind::Moved) {
                            if !mouse_motion.push(*mouse) {
                                return;
                            }
                            continue;
                        }
                    }
                    let priority = terminal_event_is_priority(&ev);
                    if priority {
                        pending_input.fetch_add(1, std::sync::atomic::Ordering::Release);
                    }
                    if input_tx.send(AppEvent::Term(ev)).is_err() {
                        if priority {
                            pending_input.fetch_sub(1, std::sync::atomic::Ordering::Release);
                        }
                        return;
                    }
                }
                Err(_) => return,
            }
        });
    }
    let sidebar_feed = bridge::start_sidebar_poller(hook_port);
    let update_slot = update::spawn_check();
    let mut app = App::new(hook_port, sidebar_feed, approval_hub);
    let startup_mascot = mascot::StartupMascot::new();
    app.link_worker = Some(link_worker_tx.clone());
    if herdr_reporter.is_some() {
        app.env_hint = herdr::right_click_hint();
    }
    // Herdr observes the same fully-derived model the TUI renders. An
    // immediate rescan also gives bridge mode a chance to replace the disk
    // fallback before the reporter publishes its initial state.
    app.rescan();
    if let Some(reporter) = herdr_reporter {
        reporter.update(&app.model);
    }
    let mut last_scan = Instant::now();
    // `pair --serve` carries an exact endpoint handoff into this process.
    // Run the ownership/start block on the first loop turn instead of leaving
    // the freshly paired Controller offline until the periodic rescan.
    let mut dirty = true;
    let mut last_selected: Option<String> = None;
    let mut mouse_released = false;
    // Snapshot fetches are the expensive path: each one makes the host
    // render and serialize a full styled grid over its control socket. At
    // one per frame that pins a busy full-screen session (claude, opencode)
    // and the TUI both. Refetch only when something can have changed —
    // new output, a different session/grid/scroll — plus a slow heartbeat.
    let mut last_snapshot_key: Option<(String, u16, u16, u32, u64)> = None;
    let mut last_snapshot_at = Instant::now() - Duration::from_secs(1);
    // Live streamed VT for the selected session (stream.rs): the app-parity
    // typing path. `last_live_publish` keys what the last published frame
    // showed (session, scroll, grid) so scroll/resize republish immediately.
    let mut live: Option<stream::LiveStream> = None;
    let mut last_live_publish: Option<(String, u32, u16, u16)> = None;
    let mut last_live_publish_at = Instant::now() - Duration::from_secs(1);
    // Burst coalescing (QUIET_FLUSH / BURST_MAX_HOLD): when the wait for a
    // quiet gap began, so continuous output is force-published at the cap.
    let mut burst_hold_since: Option<Instant> = None;
    // Toast lifecycle: `app.info` is set all over (verb outcomes, errors) —
    // rather than threading a timestamp through every site, the loop notices
    // the message *changing* here and expires it TOAST_TTL later. A keypress
    // still dismisses it immediately (handle_key clears `info` up front).
    let mut toast_seen: Option<String> = None;
    let mut toast_since = Instant::now();
    let mut link_refresh_in_flight = false;
    let mut link_refresh_retry_at = Instant::now();
    let mut link_refresh_forced = false;
    let mut last_link_maintenance = Instant::now() - Duration::from_secs(1);

    loop {
        if let Some(events) = &hook_events {
            while let Ok(message) = events.try_recv() {
                // A shared-state ping is not a hook event: it means another
                // frontend changed something on disk, so just refresh.
                if message.is_state_change() {
                    app.overlay_loaded_at = None;
                    dirty = true;
                    continue;
                }
                let canonical = activity::normalize_event_name(&message.event_name);
                let is_stop = matches!(canonical.as_str(), "Stop" | "StopFailure");
                let runtime_metadata = runtime_launch_metadata_on_disk(&message.session_id);
                let accepted = if let Some(current_generation) = runtime_metadata.0 {
                    app.engine.apply_hook_event_for_runtime(
                        &message.session_id,
                        &canonical,
                        message.tool_name.as_deref(),
                        message.received_at,
                        message.runtime_generation,
                        current_generation,
                        runtime_metadata.1,
                    )
                } else {
                    app.engine.apply_hook_event(
                        &message.session_id,
                        &canonical,
                        message.tool_name.as_deref(),
                        message.received_at,
                    );
                    true
                };
                if !accepted {
                    // Stale generation-tagged hooks and ambiguous legacy Stops
                    // change neither status nor downstream history/unread.
                    continue;
                }
                if matches!(canonical.as_str(), "Start" | "UserPromptSubmit") {
                    app.deferred_stop_effects.remove(&message.session_id);
                } else if is_stop {
                    app.defer_stop_effects_until_runtime_generation_settles(
                        &message.session_id,
                        message.runtime_generation.or(runtime_metadata.0),
                    );
                }
                dirty = true;
            }
        }
        while let Ok(outcome) = verb_rx.try_recv() {
            app.in_flight = None;
            let mut message = outcome.message;
            if let Some(text) = outcome.clipboard {
                if let Err(error) = write_terminal_clipboard(&text) {
                    message = format!("could not copy to clipboard: {error}");
                }
            }
            app.info = Some(message);
            if outcome.select.is_some() {
                app.replacement_selection.clear();
                app.pending_select = outcome.select;
            }
            if let Some(source_id) = outcome.replacement_not_applied {
                app.replacement_selection.clear_if_source(&source_id);
            }
            dirty = true;
        }
        if matches!(app.modal, Some(Modal::Pairing { .. })) && !app.pairing.is_open() {
            app.modal = None;
            app.info = Some("pairing complete or expired".into());
        }
        while let Ok(session_id) = mark_read_rx.try_recv() {
            app.mark_read(&session_id);
            dirty = true;
        }
        while let Ok(outcome) = auto_archive_rx.try_recv() {
            let now_ms = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|duration| duration.as_millis() as u64)
                .unwrap_or(0);
            apply_auto_archive_outcome(
                &mut app.auto_archive_issued,
                &mut app.auto_archive_retry_after_ms,
                outcome,
                now_ms,
            );
            // Automatic cleanup is best-effort background work. Its retry
            // state keeps transient failures self-healing without replacing
            // a user-action toast for every Session; manual Stop/Archive
            // still report their errors through the normal verb outcome.
            // Success changed the manifest/marker; failure changed retry
            // eligibility. Rebuild once before evaluating the next sweep.
            dirty = true;
        }
        while let Ok(outcome) = link_worker_rx.try_recv() {
            match outcome {
                LinkWorkerOutcome::Activation(result) => {
                    app.link_activation_in_flight = false;
                    match result {
                        Err(error) => app.info = Some(error),
                        Ok(pending) if !tui_owns_link_authority_now(&app) => {
                            // The service already assigned the seat. Roll it
                            // back off-thread so dropping this late response
                            // cannot strand an activation with no local key.
                            let _ = link_worker_tx.send(LinkWorkerRequest::DeactivateRemote {
                                license_key: pending.key,
                            });
                            app.info = Some(
                                "the Unpeel app took ownership; Link activation was not saved"
                                    .into(),
                            );
                        }
                        Ok(pending) => match unpeel_core::license::commit_activation(&pending) {
                            Err(error) => {
                                let _ = link_worker_tx.send(LinkWorkerRequest::DeactivateRemote {
                                    license_key: pending.key,
                                });
                                app.info =
                                    Some(format!("Link activation could not be saved: {error}"));
                            }
                            Ok(activation) => match ensure_mac_id() {
                                Err(error) => {
                                    app.link_suppressed = true;
                                    app.info = Some(error);
                                }
                                Ok(mac_id) => {
                                    app.link_input.clear();
                                    app.link_suppressed = true;
                                    app.link_blocked_entitlement = None;
                                    if link_worker_tx
                                        .send(LinkWorkerRequest::Entitlement {
                                            mac_id,
                                            license_key: pending.key,
                                            activation: Some(activation),
                                        })
                                        .is_ok()
                                    {
                                        link_refresh_in_flight = true;
                                        app.info = Some(format!(
                                            "Unpeel Link active for {} — authorizing relay…",
                                            pending.payload.email
                                        ));
                                    } else {
                                        app.info = Some(
                                                "Link activated, but authorization is busy; retrying shortly"
                                                    .into(),
                                            );
                                        link_refresh_retry_at = Instant::now();
                                    }
                                }
                            },
                        },
                    }
                }
                LinkWorkerOutcome::Deactivation(result) => {
                    if let Err(error) = result {
                        app.info = Some(format!(
                            "Link stopped locally, but its paid seat was not released; manage activations in your Unpeel account: {error}"
                        ));
                    }
                }
                LinkWorkerOutcome::Entitlement {
                    license_key,
                    activation,
                    result,
                } => {
                    link_refresh_in_flight = false;
                    let owns_serving = tui_owns_link_authority_now(&app);
                    let request_is_current = owns_serving
                        && unpeel_core::license::stored()
                            .is_some_and(|(key, _)| key == license_key);
                    match result {
                        Ok(pending) if request_is_current => {
                            let committed = match activation.as_ref() {
                                Some(activation) => {
                                    unpeel_core::license::commit_relay_entitlement_for_activation(
                                        &license_key,
                                        &pending,
                                        activation,
                                    )
                                }
                                None => unpeel_core::license::commit_relay_entitlement_for_key(
                                    &license_key,
                                    &pending,
                                ),
                            };
                            match committed {
                                Ok(()) => {
                                    app.link_suppressed = false;
                                    app.link_blocked_entitlement = None;
                                    link_refresh_forced = false;
                                    link_refresh_retry_at = Instant::now();
                                }
                                Err(error) => {
                                    link_refresh_retry_at =
                                        Instant::now() + Duration::from_secs(60);
                                    app.info = Some(format!(
                                        "Unpeel Link could not save authorization: {error}"
                                    ));
                                }
                            }
                        }
                        Err(error) if error.is_rejected() && request_is_current => {
                            suppress_current_relay_entitlement(&mut app);
                            link_refresh_forced = false;
                            if let Some(uplink) = app.relay_uplink.take() {
                                uplink.stop();
                            }
                            link_refresh_retry_at = Instant::now() + Duration::from_secs(15 * 60);
                            app.info =
                                Some(match unpeel_core::license::reject_relay_entitlement() {
                                    Ok(()) => {
                                        format!("Unpeel Link authorization rejected: {error}")
                                    }
                                    Err(remove_error) => format!(
                                        "Unpeel Link authorization rejected; durable suppression could not finish: {remove_error}"
                                    ),
                                });
                        }
                        Err(error) if request_is_current => {
                            link_refresh_retry_at = Instant::now() + Duration::from_secs(60);
                            match ensure_mac_id() {
                                Ok(mac_id)
                                    if unpeel_core::relay_uplink::cached_entitlement(&mac_id)
                                        .is_none() =>
                                {
                                    app.info = Some(format!("Unpeel Link unavailable: {error}"));
                                }
                                Err(identity_error) => app.info = Some(identity_error),
                                _ => {}
                            }
                        }
                        _ => {
                            // Deactivation, key replacement, or native-app
                            // ownership won the race. Request workers never
                            // write shared authority by themselves.
                            link_refresh_retry_at = Instant::now();
                        }
                    }
                }
            }
            dirty = true;
        }
        if dirty || last_scan.elapsed() >= RESCAN_INTERVAL {
            app.rescan();
            // While the app serves the sidebar it also runs this sweep, with
            // knowledge of its own selection/unread this process lacks.
            let mobile_authority_ambiguous = app.legacy_mobile_handoff_latched
                || app
                    .mobile_server
                    .as_ref()
                    .is_some_and(|server| !server.owns_configured_endpoint());
            if tui_owns_auto_archive_sweep(
                app.bridge_mode,
                app.feed_note,
                mobile_authority_ambiguous,
            ) {
                run_auto_stop_archive_sweep(&mut app, &auto_archive_tx);
            }
            app.announce_new_local_urls();
            if let Some(reporter) = herdr_reporter {
                reporter.update(&app.model);
            }
            last_scan = Instant::now();
            dirty = false;
            let paired_device_count = mobile::paired_device_count();
            if paired_device_count == 0 {
                // Revoking the final device is an authorization event, not
                // merely a UI change: close LAN sockets, __remote__ WSS, and
                // the Relay uplink so no established transport outlives the
                // authority record.
                if let Some(uplink) = app.relay_uplink.take() {
                    // Revoke remote dispatch before releasing the LAN lease;
                    // a new owner may bind as soon as MobileServer::stop joins
                    // its accept loop.
                    uplink.stop();
                }
                if let Some(server) = app.mobile_server.take() {
                    server.stop();
                    if let Ok(mut guard) = app.mobile_resizes.lock() {
                        guard.clear();
                    }
                    app.info = Some("phone serving stopped — no paired devices".into());
                }
                app.legacy_mobile_handoff_latched = false;
                app.legacy_mobile_handoff_classified = false;
                app.legacy_mobile_fallback_port = None;
                app.legacy_mobile_mismatch_observed_at = None;
            }
            // Any positively identified native app owns Link/entitlement
            // authority, including the released build that predates sidebar.
            // A connected-but-busy hook candidate is also fail-closed until
            // it resolves. Revoke dispatch before touching the LAN listener.
            let configured_endpoint_replaced = app
                .mobile_server
                .as_ref()
                .is_some_and(|server| !server.owns_configured_endpoint());
            let native_without_exact_handoff =
                app.legacy_bridge_mode || (app.bridge_mode && !app.bridge_mobile_endpoint_handoff);
            if configured_endpoint_replaced {
                if !app.legacy_mobile_handoff_latched {
                    app.legacy_mobile_mismatch_observed_at = Some(Instant::now());
                }
                app.legacy_mobile_handoff_latched = true;
                if app.legacy_mobile_fallback_port.is_none() {
                    app.legacy_mobile_fallback_port =
                        mobile::canonical_server_port().filter(|published| {
                            app.mobile_server
                                .as_ref()
                                .is_some_and(|server| *published != server.port)
                        });
                }
            }
            if app.legacy_mobile_handoff_latched && native_without_exact_handoff {
                app.legacy_mobile_handoff_classified = true;
            }
            let fallback_listener_is_gone = app
                .legacy_mobile_fallback_port
                .is_none_or(|port| !mobile::local_endpoint_is_listening(port));
            let classified_legacy_owner_is_gone = app.legacy_mobile_handoff_latched
                && app.legacy_mobile_handoff_classified
                && !app.bridge_mode
                && !app.legacy_bridge_mode
                && !app.bridge_unresolved
                && app.feed_note == "app offline"
                && fallback_listener_is_gone;
            if classified_legacy_owner_is_gone {
                app.legacy_mobile_handoff_latched = false;
                app.legacy_mobile_handoff_classified = false;
                app.legacy_mobile_fallback_port = None;
                app.legacy_mobile_mismatch_observed_at = None;
            }
            let unclassified_stale_rewrite_is_gone = app.legacy_mobile_handoff_latched
                && !app.legacy_mobile_handoff_classified
                && !app.bridge_mode
                && !app.legacy_bridge_mode
                && !app.bridge_unresolved
                && app.feed_note == "app offline"
                && fallback_listener_is_gone
                && app
                    .legacy_mobile_mismatch_observed_at
                    .is_some_and(|observed| observed.elapsed() >= Duration::from_secs(3));
            let native_link_authority = app.bridge_mode
                || app.legacy_bridge_mode
                || app.bridge_unresolved
                || configured_endpoint_replaced
                || app.legacy_mobile_handoff_latched;
            if native_link_authority {
                if let Some(uplink) = app.relay_uplink.take() {
                    uplink.stop();
                }
            }

            // Capability-aware native retries the exact persisted endpoint,
            // so it is safe to release Direct. Released native instead falls
            // back A→random B and overwrites server-port; keep serving the
            // paired phone on A and atomically repair that legacy rewrite.
            let yield_lan_to_native = app.bridge_mode && app.bridge_mobile_endpoint_handoff;
            if yield_lan_to_native {
                if let Some(server) = app.mobile_server.take() {
                    server.stop();
                    if let Ok(mut guard) = app.mobile_resizes.lock() {
                        guard.clear();
                    }
                    app.info = Some("phone serving handed to the Mac app".into());
                }
                app.legacy_mobile_handoff_latched = false;
                app.legacy_mobile_handoff_classified = false;
                app.legacy_mobile_fallback_port = None;
                app.legacy_mobile_mismatch_observed_at = None;
            } else if configured_endpoint_replaced {
                if native_without_exact_handoff || unclassified_stale_rewrite_is_gone {
                    let restored = app
                        .mobile_server
                        .as_ref()
                        .is_some_and(mobile::MobileServer::restore_legacy_configured_endpoint);
                    if !restored {
                        app.info = Some(
                            "could not restore the paired Direct endpoint after legacy app handoff"
                                .into(),
                        );
                    } else if unclassified_stale_rewrite_is_gone {
                        app.legacy_mobile_handoff_latched = false;
                        app.legacy_mobile_handoff_classified = false;
                        app.legacy_mobile_fallback_port = None;
                        app.legacy_mobile_mismatch_observed_at = None;
                    }
                } else {
                    // Keep A serving, but leave Link denied and B untouched
                    // until the hook probe positively classifies who rewrote
                    // it. Guessing here can race an old app and reauthorize a
                    // duplicate uplink against a stale offline poll.
                    app.info = Some(
                        "paired Direct endpoint changed — waiting for frontend ownership".into(),
                    );
                }
            }
            // Every local frontend may contend for the endpoint. Exact bind
            // is the arbiter. A capability-aware native gets priority after
            // its validated sidebar asks us to yield; an older native simply
            // loses this bind and keeps its own platform/Link authority.
            if app.mobile_server.is_none() && !yield_lan_to_native && paired_device_count > 0 {
                app.mobile_server = mobile::start(
                    std::sync::Arc::clone(&app.mobile_snapshot),
                    mark_read_tx.clone(),
                    app.hook_port,
                    std::sync::Arc::clone(&app.mobile_resizes),
                    std::sync::Arc::clone(&app.approvals),
                    std::sync::Arc::clone(&app.pairing),
                );
                if let Some(server) = &app.mobile_server {
                    app.info = Some(format!("serving paired phones on port {}", server.port));
                }
            }
        }

        // License verification and shared-file reads do not belong on the
        // 100ms render tick. More importantly, this entire block is owned by
        // a resolved standalone TUI: while the native bridge is reachable it
        // alone may refresh or invalidate the shared entitlement cache.
        if last_link_maintenance.elapsed() >= Duration::from_secs(1) {
            last_link_maintenance = Instant::now();
            let owns_serving = tui_owns_link_authority_now(&app);
            if owns_serving {
                if app
                    .relay_uplink
                    .as_ref()
                    .is_some_and(relay::RelayUplink::take_authorization_rejected)
                {
                    suppress_current_relay_entitlement(&mut app);
                    link_refresh_forced = true;
                    link_refresh_retry_at = Instant::now();
                    if let Some(uplink) = app.relay_uplink.take() {
                        uplink.stop();
                    }
                    if let Err(error) = unpeel_core::license::reject_relay_entitlement() {
                        app.info = Some(format!(
                            "Unpeel Link authorization was refused; durable suppression could not finish: {error}"
                        ));
                    }
                }

                // A malformed headless key must never inherit an old app-
                // issued entitlement. Absence is different: cached native
                // entitlements stay compatible while the app is closed.
                let key_file_exists = unpeel_core::license::stored_file_exists();
                let stored_license = unpeel_core::license::stored();
                if key_file_exists && stored_license.is_none() {
                    suppress_current_relay_entitlement(&mut app);
                    if let Some(uplink) = app.relay_uplink.take() {
                        uplink.stop();
                    }
                    if let Err(error) = unpeel_core::license::reject_invalid_stored_key() {
                        app.info = Some(format!(
                            "invalid Link key; durable suppression could not finish: {error}"
                        ));
                    }
                }

                let tombstone = unpeel_core::license::link_tombstone_reason();
                match tombstone {
                    Err(error) => {
                        app.link_suppressed = true;
                        if let Some(uplink) = app.relay_uplink.take() {
                            uplink.stop();
                        }
                        app.info = Some(format!(
                            "Unpeel Link disable state could not be read: {error}"
                        ));
                    }
                    Ok(reason) => {
                        let suppression_requires_refresh = matches!(
                            reason,
                            Some(
                                unpeel_core::license::LinkTombstoneReason::ActivationPending
                                    | unpeel_core::license::LinkTombstoneReason::AuthorizationRejected
                            )
                        );
                        if reason.is_some() {
                            app.link_suppressed = true;
                            if let Some(uplink) = app.relay_uplink.take() {
                                uplink.stop();
                            }
                        }
                        // A user disable has no recovery permission. A
                        // durable activation-pending generation or an
                        // authorization rejection may use the stored key,
                        // but never runs the denied cache while refreshing.
                        match unpeel_core::license::link_tombstone_allows_refresh() {
                            Err(error) => {
                                app.link_suppressed = true;
                                app.info = Some(format!(
                                    "Unpeel Link recovery state could not be read: {error}"
                                ));
                            }
                            Ok(false) => {}
                            Ok(true) => match ensure_mac_id() {
                                Ok(mac_id) => {
                                    let cache_state =
                                        unpeel_core::relay_uplink::entitlement_cache_state(&mac_id);
                                    if !link_refresh_in_flight
                                        && Instant::now() >= link_refresh_retry_at
                                        && stored_license.is_some()
                                        && (link_refresh_forced
                                            || suppression_requires_refresh
                                            || cache_state
                                                != unpeel_core::relay_uplink::EntitlementCacheState::Fresh)
                                    {
                                        let license_key = stored_license
                                            .map(|(key, _)| key)
                                            .unwrap_or_default();
                                        if link_worker_tx
                                            .send(LinkWorkerRequest::Entitlement {
                                                mac_id,
                                                license_key,
                                                activation: None,
                                            })
                                            .is_ok()
                                        {
                                            link_refresh_in_flight = true;
                                        }
                                    }
                                }
                                Err(error) => {
                                    app.link_suppressed = true;
                                    app.info = Some(error);
                                }
                            },
                        }
                    }
                }
            } else {
                // A response already in flight is harmless (request-only),
                // and its outcome will be ignored while native owns serving.
                link_refresh_forced = false;
            }
            reconcile_relay_uplink(&mut app, &mark_read_tx);
        }

        // Selection mode hands the mouse back to the terminal emulator so
        // its native drag-select and copy work; re-grab when it ends.
        if app.selection_mode != mouse_released {
            mouse_released = app.selection_mode;
            if mouse_released {
                let _ = execute!(io::stdout(), DisableMouseCapture);
            } else {
                let _ = execute!(io::stdout(), EnableMouseCapture);
            }
        }
        let size = terminal.size()?;
        LAST_TERM_WIDTH.store(size.width, std::sync::atomic::Ordering::Relaxed);
        // Keep the session's PTY matching the pane on EVERY tick, not just
        // when the selection changes: the window can be resized, the sidebar
        // dragged, or a phone can hand the grid back — all of which change
        // the pane without touching the selection. The call is a no-op when
        // the grid already matches, and a phone that currently owns the grid
        // keeps it (taking it back is explicit, via selecting or focusing).
        if app
            .selected_id
            .as_deref()
            .is_some_and(|id| !app.mobile_resized(id))
        {
            resize_selected_to_pane(&app, &snapshot_service, size.width, size.height);
        }
        // Selection changed (including auto-select of a fresh spawn): make
        // sure the session's PTY matches the pane and repaint from scratch —
        // a newly spawned CLI's startup escape soup can leave diff artifacts.
        if app.selected_id != last_selected {
            last_selected = app.selected_id.clone();
            // Moving to a different session closes an open rename dialog — a
            // half-typed name for the session you just left is never what you
            // want to save. Guarded on the id so a double-click, which selects
            // a session and *then* opens rename on that same one, is not
            // treated as navigating away from it.
            let stale_rename = matches!(
                &app.modal,
                Some(Modal::Rename(input))
                    if Some(&input.session_id) != app.selected_id.as_ref()
            );
            if stale_rename {
                app.modal = None;
            }
            resize_selected_to_pane(&app, &snapshot_service, size.width, size.height);
            // Only wipe the screen when there's nothing cached to draw for
            // the new session (a fresh spawn, whose startup escape soup can
            // otherwise leave diff artifacts). Switching between sessions we
            // already have snapshots for repaints by diff — clearing there
            // is what made the whole terminal blink.
            let has_snapshot = app
                .selected_id
                .as_deref()
                .is_some_and(|id| snapshot_service.get(id).is_some());
            if !has_snapshot {
                // Inside a synchronized update: the wipe and the repaint that
                // follows this pass must land as one presented frame. Setting
                // the mode twice (again before the draw below) is harmless —
                // it is a mode, not a counter — and one End closes it.
                let _ = execute!(io::stdout(), BeginSynchronizedUpdate);
                // Ratatui 0.30's Terminal::clear snapshots the cursor with a
                // CPR query first. Hosted/controller PTYs intentionally do
                // not synthesize that terminal reply, so use the fullscreen
                // resize/reset path: it clears the viewport and invalidates
                // the diff buffer without waiting for stdin.
                terminal.resize(size.into())?;
            }
        }
        app.clamp_scroll(size.height.saturating_sub(2) as usize);
        // Live stream lifecycle: exactly one, for the selected running
        // session while the TUI owns its grid. A phone-resized grid keeps
        // the socket-snapshot path (which letterboxes the host's true grid
        // instead of assuming the pane's); a dead stream retries with a 1s
        // backoff, falling back to socket snapshots in between.
        let want_live = app
            .selected_session()
            .filter(|s| s.running)
            .map(|s| s.id.clone())
            .filter(|id| !app.mobile_resized(id));
        let (pane_cols, pane_rows) = preview_grid(&app, size.width, size.height);
        let need_new = match (&live, &want_live) {
            (_, None) => false,
            (None, Some(_)) => true,
            (Some(current), Some(id)) => {
                &current.session_id != id
                    || (current.is_dead() && current.started.elapsed() >= current.retry_delay())
            }
        };
        if let Some(id) = &want_live {
            if need_new {
                live = Some(stream::LiveStream::start(
                    id.clone(),
                    pane_cols,
                    pane_rows,
                    output_wake.clone(),
                ));
                last_live_publish = None;
            }
        } else {
            live = None;
        }
        // Publish a frame from the live VT when bytes arrived or the scroll/
        // grid changed — capped at ~60fps during output storms; a lone echo
        // publishes immediately. While the stream is healthy the socket
        // snapshot poller below is skipped entirely.
        let mut live_streaming = false;
        let mut throttled_until: Option<Instant> = None;
        if let Some(current) = live.as_ref().filter(|l| !l.is_dead()) {
            live_streaming = true;
            current.resize(pane_cols, pane_rows);
            let key = (
                current.session_id.clone(),
                app.preview_scroll,
                pane_cols,
                pane_rows,
            );
            let key_changed = last_live_publish.as_ref() != Some(&key);
            if key_changed || current.is_dirty() {
                let now = Instant::now();
                // Coalesce bursts: while bytes are still streaming in, the
                // grid may hold a half-applied repaint. An app that brackets
                // repaints in DEC 2026 markers says exactly when: hold while
                // a sync block is open, publish freely between frames — the
                // gap heuristic (and its force-publish cap, which caught
                // mid-repaint grids under sustained output) never applies.
                // Without markers, wait for a quiet gap, capped so
                // continuous output still flows. Geometry/scroll changes
                // bypass (the grid is stale either way; a re-publish follows
                // when the burst settles).
                let quiet = current.since_last_feed();
                let sync_framed = current.sync_frames_active();
                let hold = !key_changed
                    && if sync_framed {
                        current
                            .sync_open_elapsed()
                            .is_some_and(|open| open < SYNC_MAX_HOLD)
                    } else {
                        // Tiny feeds are keystroke echo, not a repaint
                        // mid-burst — never delay those.
                        current.last_feed_len() >= QUIET_FLUSH_MIN_FEED_BYTES
                            && quiet < QUIET_FLUSH
                            && burst_hold_since
                                .is_none_or(|since| now.duration_since(since) < BURST_MAX_HOLD)
                    };
                if !current.has_fed() {
                    // Brand-new stream, tail replay not yet in the VT: the
                    // grid is empty, and the reset publish key would push
                    // that emptiness to the pane — the restart blink. Keep
                    // the previous frame; the replay's feed wakes the loop.
                } else if hold {
                    if burst_hold_since.is_none() {
                        burst_hold_since = Some(now);
                    }
                    // Leave `dirty` set and wake when the hold would lapse;
                    // the reader wakes the loop sooner on the close marker
                    // or the next quiet-gap feed.
                    let recheck = if sync_framed {
                        now + current
                            .sync_open_elapsed()
                            .map_or(SYNC_MAX_HOLD, |open| SYNC_MAX_HOLD.saturating_sub(open))
                    } else {
                        now + (QUIET_FLUSH - quiet)
                    };
                    throttled_until = Some(throttled_until.map_or(recheck, |t| t.min(recheck)));
                } else if key_changed || last_live_publish_at.elapsed() >= LIVE_FRAME_MIN_INTERVAL {
                    current.take_dirty();
                    if let Some(snapshot) = current.snapshot(app.preview_scroll) {
                        snapshot_service.publish(current.session_id.clone(), snapshot);
                        last_live_publish = Some(key);
                        last_live_publish_at = now;
                        burst_hold_since = None;
                    }
                } else {
                    throttled_until = Some(last_live_publish_at + LIVE_FRAME_MIN_INTERVAL);
                }
            }
        }
        if let Some(session_id) = app.selected_session().map(|s| s.id.clone()) {
            if app.preview_scroll > 0 {
                if let Some(snapshot) = snapshot_service.get(&session_id) {
                    app.preview_scroll = app.preview_scroll.min(snapshot.scrollback_rows);
                }
            }
            // When a phone owns the grid, request the session's ACTUAL size
            // (cols=0/rows=0 → "current") so the preview letterboxes the
            // phone's grid rather than re-wrapping it to the pane. Otherwise
            // render at the pane, which we also keep the PTY sized to.
            let phone_owns = app.mobile_resized(&session_id);
            let (cols, rows) = if phone_owns {
                (0u16, 0u16)
            } else {
                (
                    size.width.saturating_sub(app.sidebar_width + 2).max(4),
                    size.height.saturating_sub(3).max(2),
                )
            };
            let output_len = std::fs::metadata(
                unpeel_core::app_paths::app_sessions_root()
                    .join(&session_id)
                    .join("output.bin"),
            )
            .map(|meta| meta.len())
            .unwrap_or(0);
            let key = (
                session_id.clone(),
                cols,
                rows,
                app.preview_scroll,
                output_len,
            );
            let stale = last_snapshot_at.elapsed() >= Duration::from_millis(1_000);
            if !live_streaming
                && (last_snapshot_key.as_ref() != Some(&key) || stale)
                && last_snapshot_at.elapsed() >= Duration::from_millis(120)
            {
                last_snapshot_key = Some(key);
                last_snapshot_at = Instant::now();
                snapshot_service.request(vec![SnapshotRequest {
                    session_id,
                    cols,
                    rows,
                    scroll_offset: app.preview_scroll,
                }]);
            }
        }
        // A finished update check drops its result here; the persistent
        // toast draws whenever no transient `info` toast is up.
        if app.update_available.is_none() {
            if let Some(version) = update_slot.lock().ok().and_then(|mut slot| slot.take()) {
                app.update_available = Some(version);
            }
        }
        // Notice a new toast, expire a stale one (see `toast_seen` above).
        match (&app.info, &toast_seen) {
            (Some(message), previous) if previous.as_ref() != Some(message) => {
                toast_seen = Some(message.clone());
                toast_since = Instant::now();
            }
            (Some(_), _) if toast_since.elapsed() >= TOAST_TTL => {
                app.info = None;
                toast_seen = None;
            }
            (None, Some(_)) => toast_seen = None,
            _ => {}
        }
        // Synchronized update (DEC mode 2026): the whole frame — which for a
        // busy session is a full rewrite of the preview pane, tens of KB —
        // is presented by the outer terminal atomically instead of racing
        // its renderer mid-write. Without this, Ghostty can paint a display
        // frame between our erase and our repaint of the pane: the embedded
        // terminal "randomly blinks" while the small-diff chrome never does.
        // Terminals without 2026 ignore both sequences.
        let _ = execute!(io::stdout(), BeginSynchronizedUpdate);
        let draw_result = terminal.draw(|f| {
            ui::draw(f, &app, &snapshot_service);
            startup_mascot.draw(f);
        });
        let _ = execute!(io::stdout(), EndSynchronizedUpdate);
        draw_result?;

        // Sleep until input, a live-output wake, or the tick — sooner when a
        // throttled live frame is pending. Then drain everything queued so a
        // burst of keys applies before the next draw, not one key per frame.
        let mut timeout = TICK;
        if let Some(deadline) = throttled_until {
            timeout = timeout.min(deadline.saturating_duration_since(Instant::now()));
        }
        let first = match event_rx.recv_timeout(timeout) {
            Ok(ev) => ev,
            Err(mpsc::RecvTimeoutError::Timeout) => continue,
            // The reader thread only dies with the terminal itself.
            Err(mpsc::RecvTimeoutError::Disconnected) => return Ok(()),
        };
        let input_started = Instant::now();
        for ev in queued_app_events(first, &event_rx) {
            if matches!(&ev, AppEvent::Term(event) if terminal_event_is_priority(event)) {
                pending_input.fetch_sub(1, std::sync::atomic::Ordering::AcqRel);
            }
            match ev {
                AppEvent::Wake => output_wake.consumed(),
                AppEvent::Term(Event::Key(key)) if key.kind != KeyEventKind::Release => {
                    if !handle_key(
                        &mut app,
                        key,
                        size.width,
                        size.height,
                        &snapshot_service,
                        &verb_tx,
                    ) {
                        save_layout(&app);
                        if let Some(uplink) = app.relay_uplink.take() {
                            uplink.stop();
                        }
                        if let Some(server) = app.mobile_server.take() {
                            server.stop();
                        }
                        return Ok(());
                    }
                }
                AppEvent::Term(Event::Paste(text)) => {
                    if let Some(Modal::Rename(input)) = app.modal.as_mut() {
                        input.insert(&text);
                        if input_started.elapsed() >= INPUT_DRAIN_BUDGET {
                            break;
                        }
                        continue;
                    }
                    // A file dragged from Finder onto the window arrives as a
                    // bracketed paste of its shell-escaped path. Without
                    // terminal focus that used to be dropped on the floor —
                    // the user had to click into the terminal first for the
                    // drop to take. A pasted path is explicit intent to feed
                    // the selected session: enter terminal focus (same as ⏎)
                    // and forward, as long as no overlay owns the keyboard.
                    // Plain text pastes still require focus so a stray ⌘V
                    // can't type into an agent unnoticed.
                    if !app.terminal_focus
                        && looks_like_dropped_path(&text)
                        && app.modal.is_none()
                        && app.confirm.is_none()
                        && app.settings.is_none()
                        && app.selected_archive.is_none()
                        && app.approvals.front().is_none()
                    {
                        enter_terminal_focus(&mut app, size.width, size.height);
                    }
                    if app.terminal_focus {
                        if let Some(dir) = app.selected_session().map(|s| s.dir()) {
                            let _ = app.input.send(&dir, text);
                        }
                    }
                }
                AppEvent::Term(Event::Mouse(mouse)) => handle_mouse(
                    &mut app,
                    mouse,
                    size.width,
                    size.height,
                    &snapshot_service,
                    &verb_tx,
                ),
                AppEvent::MouseMoved => {
                    if let Some(mouse) = mouse_motion.take() {
                        handle_mouse(
                            &mut app,
                            mouse,
                            size.width,
                            size.height,
                            &snapshot_service,
                            &verb_tx,
                        );
                    }
                }
                AppEvent::ScrollBurst => {
                    if let Some(run) = scroll_gate.take() {
                        for _ in 0..run.count {
                            handle_mouse(
                                &mut app,
                                run.event,
                                size.width,
                                size.height,
                                &snapshot_service,
                                &verb_tx,
                            );
                        }
                    }
                }
                AppEvent::Term(Event::Resize(w, h)) => {
                    if app.terminal_focus {
                        if let Some(dir) = app.selected_session().map(|s| s.dir()) {
                            let (cols, rows) = preview_grid(&app, w, h);
                            let _ = control::send_resize(&dir, cols, rows);
                        }
                    }
                }
                AppEvent::Term(_) => {}
            }
            if app.exit_requested {
                save_layout(&app);
                if let Some(uplink) = app.relay_uplink.take() {
                    uplink.stop();
                }
                if let Some(server) = app.mobile_server.take() {
                    server.stop();
                }
                return Ok(());
            }
            // Redraw traffic and precision-scroll bursts may consume the
            // ordinary frame budget, but once the reader has already seen a
            // key/paste, keep draining FIFO until it has been forwarded. This
            // preserves click/focus ordering without letting another slow draw
            // bury keyboard input behind low-priority events.
            if input_started.elapsed() >= INPUT_DRAIN_BUDGET
                && pending_input.load(std::sync::atomic::Ordering::Acquire) == 0
            {
                break;
            }
        }
    }
}

// `unpeel presets ...` and the settings panel edit the shared
// `~/.unpeel/app-state.json` contract — since the app's overlay migration
// (`native_preset_overlay_migrated`) the file is the single preset truth and
// a running app picks up edits live via FSEvents. Only un-migrated installs
// still show overlay-held presets, read-only.

/// Last selectable row index per settings section.
pub fn settings_row_count(app: &App, section: usize) -> usize {
    let _ = app;
    match section {
        // The blank add row sits one past the last preset (and is row 0 of
        // an empty list).
        0 => app_state_presets().len(),
        1 => ACCESS_SETTINGS.len() - 1,
        // Remote: the paired devices, then the Unpeel Link rows — enrolled
        // -> deactivate / nickname / avatar; otherwise the key field.
        2 => {
            let link_rows = if unpeel_core::license::stored().is_some() {
                3
            } else {
                1
            };
            paired_devices().len() + link_rows - 1
        }
        _ => 0,
    }
}

/// App-wide access policies, mirroring the desktop's Settings toggles. Each
/// entry is (app-state.json key, label, cycle of values).
pub const ACCESS_SETTINGS: [(&str, &str, &[&str]); 5] = [
    (
        "browser_default_access",
        "Browser access",
        &["on", "ask", "off"],
    ),
    (
        "mcp_nonchild_write_access",
        "Sessions MCP writes across groups",
        &["ask", "allow", "deny"],
    ),
    ("computer_access", "Computer use", &["ask", "allow", "off"]),
    (
        "mcp_worktree_access",
        "Agents may create worktrees",
        &["false", "true"],
    ),
    (
        "mcp_auto_add_browser_screenshots",
        "Browser screenshots go to gallery",
        &["true", "false"],
    ),
];

/// The auto-stop-and-archive knob, shared with the desktop app
/// (UnpeelStore's `auto_stop_archive_minutes`): sessions continuously idle
/// this long get the same treatment as the Stop verb — host stopped, row
/// filed into the archive library, nothing deleted. Absent key = on at the
/// default cutoff (opt-out feature); explicit 0 = off.
pub const AUTO_STOP_ARCHIVE_KEY: &str = "auto_stop_archive_minutes";
pub const AUTO_STOP_ARCHIVE_MINUTE_OPTIONS: [u64; 7] = [0, 30, 60, 120, 240, 480, 1440];
pub const DEFAULT_AUTO_STOP_ARCHIVE_MINUTES: u64 = 1440;

pub fn auto_stop_archive_minutes() -> u64 {
    let state: serde_json::Value = std::fs::read(unpeel_core::app_paths::app_state_path())
        .ok()
        .and_then(|raw| serde_json::from_slice(&raw).ok())
        .unwrap_or_default();
    match state.get(AUTO_STOP_ARCHIVE_KEY).and_then(|v| v.as_u64()) {
        Some(minutes) if AUTO_STOP_ARCHIVE_MINUTE_OPTIONS.contains(&minutes) => minutes,
        // Junk never silently *shortens* the cutoff — it reads as off.
        Some(_) => 0,
        None => DEFAULT_AUTO_STOP_ARCHIVE_MINUTES,
    }
}

pub fn auto_stop_archive_label(minutes: u64) -> String {
    match minutes {
        0 => "Never".into(),
        m if m < 60 => format!("After {m} minutes"),
        60 => "After 1 hour".into(),
        1440 => "After 1 day".into(),
        m => format!("After {} hours", m / 60),
    }
}

/// ⏎ on the Cleanup row: advance the cutoff to the next option and persist
/// it explicitly — absent means "default on", so Never (0) must be a
/// written value, not a removed key.
fn cycle_auto_stop_archive_setting() -> Result<(), String> {
    let current = auto_stop_archive_minutes();
    let position = AUTO_STOP_ARCHIVE_MINUTE_OPTIONS
        .iter()
        .position(|m| *m == current)
        .unwrap_or(0);
    let next =
        AUTO_STOP_ARCHIVE_MINUTE_OPTIONS[(position + 1) % AUTO_STOP_ARCHIVE_MINUTE_OPTIONS.len()];
    update_app_state(|state| {
        state.insert(
            AUTO_STOP_ARCHIVE_KEY.to_string(),
            serde_json::Value::from(next),
        );
    })
}

/// Auto-stop and archive inactive terminals — the desktop sweep's twin
/// (UnpeelStore.runAutoStopArchiveIfNeeded), for when the TUI is the only
/// frontend (headless host, app closed). While the app serves the sidebar —
/// or the bridge is still resolving during app startup/rebuild — its sweep
/// owns the policy because it knows selection/unread state this process
/// cannot see. The caller gates on that ownership.
///
/// "Inactive" is deliberately NOT "old": only an unbroken idle stretch
/// qualifies. Its clock is the same canonical lifecycle timestamp used by
/// Recently updated — hook-owned sessions therefore ignore raw terminal
/// repaints, while non-hook sessions use parsed-screen changes (and the
/// legacy output fallback) through that timestamp. Archiving is the same
/// non-destructive verb as Stop: the row files into the archive library and
/// Restore + Resume continues the conversation.
#[derive(Debug)]
struct AutoArchiveOutcome {
    session_id: String,
    error: Option<String>,
}

fn auto_archive_attempt_blocked(
    issued: &HashSet<String>,
    retry_after_ms: &HashMap<String, u64>,
    session_id: &str,
    now_ms: u64,
) -> bool {
    issued.contains(session_id)
        || retry_after_ms
            .get(session_id)
            .is_some_and(|retry_at| now_ms < *retry_at)
}

/// Cleanup ownership is resolved by the sidebar bridge, not merely by the
/// current model source. Startup `connecting…` and an older reachable app
/// both remain app-owned; only a resolved unreachable app makes this TUI the
/// standalone cleanup owner.
fn tui_owns_auto_archive_sweep(
    bridge_mode: bool,
    feed_note: &str,
    mobile_authority_ambiguous: bool,
) -> bool {
    !bridge_mode && feed_note == "app offline" && !mobile_authority_ambiguous
}

/// Link cache and uplink authority follows the exact /mobile endpoint claim,
/// not the shared hook-port registry. Multiple TUIs appear in that registry
/// and older frontends may 404 `/mcp/sidebar`; neither signal can safely pick
/// a single Host owner, while the listener bind can.
fn tui_owns_link_authority(
    has_exact_endpoint_claim: bool,
    native_bridge_reachable_or_unresolved: bool,
) -> bool {
    has_exact_endpoint_claim && !native_bridge_reachable_or_unresolved
}

/// Re-check both the durable endpoint and the live poll slot at the cache
/// commit/use boundary. A native sidebar response can arrive while an
/// entitlement request is in flight, before the next frame folds it into the
/// App fields; that response must revoke TUI Link authority immediately.
fn tui_owns_link_authority_now(app: &App) -> bool {
    let native_bridge_reachable_or_unresolved = app
        .sidebar_feed
        .lock()
        .ok()
        .and_then(|latest| latest.clone())
        .is_some_and(|result| match result {
            Ok(_) => true,
            Err(error) => {
                error.contains("predate this route") || error.contains("bridge is still resolving")
            }
        })
        || app.legacy_mobile_handoff_latched;
    tui_owns_link_authority(
        app.mobile_server
            .as_ref()
            .is_some_and(mobile::MobileServer::owns_configured_endpoint),
        native_bridge_reachable_or_unresolved,
    )
}

/// Keep automatic cleanup cancellable between Sessions. In particular, a
/// desktop app returning during a rebuild must prevent the TUI from working
/// through a previously captured backlog after ownership has moved back.
fn auto_archive_worker_available(issued: &HashSet<String>) -> bool {
    issued.is_empty()
}

/// A stop request can succeed while the Host's final manifest publication
/// arrives after `archive_session`'s bounded wait. That row is stopped on the
/// next rescan, but it still needs one idempotent retry to write archived.json.
fn stopped_auto_archive_retry_due(
    row: &SessionRow,
    issued: &HashSet<String>,
    retry_after_ms: &HashMap<String, u64>,
    now_ms: u64,
) -> bool {
    !row.running
        && !row.archived
        && !row.pinned
        && !issued.contains(&row.id)
        && retry_after_ms
            .get(&row.id)
            .is_some_and(|retry_at| now_ms >= *retry_at)
}

/// Release one in-flight id and either clear its retry state on success or
/// arm a bounded retry delay on failure. Keeping this state transition on the
/// render thread means a worker can never race the next sweep's maps.
fn apply_auto_archive_outcome(
    issued: &mut HashSet<String>,
    retry_after_ms: &mut HashMap<String, u64>,
    outcome: AutoArchiveOutcome,
    now_ms: u64,
) {
    let AutoArchiveOutcome { session_id, error } = outcome;
    issued.remove(&session_id);
    match error {
        Some(_) => {
            retry_after_ms.insert(
                session_id,
                now_ms.saturating_add(AUTO_ARCHIVE_RETRY_DELAY_MS),
            );
        }
        None => {
            retry_after_ms.remove(&session_id);
        }
    }
}

fn run_auto_stop_archive_sweep(app: &mut App, outcomes: &mpsc::Sender<AutoArchiveOutcome>) {
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);
    // Maintain the continuously-idle map first. A session first observed
    // idle seeds from its last recorded activity, clamped to now — a TUI
    // restart must not reset hours of already-accumulated idleness.
    for row in &app.model.rows {
        if row.running && row.status == Status::Idle {
            app.idle_since_ms
                .entry(row.id.clone())
                .and_modify(|idle_since| {
                    *idle_since = observed_idle_since(Some(*idle_since), row.activity_at, now_ms)
                })
                .or_insert_with(|| observed_idle_since(None, row.activity_at, now_ms));
        } else {
            app.idle_since_ms.remove(&row.id);
        }
    }
    app.idle_since_ms
        .retain(|id, _| app.model.rows.iter().any(|r| r.id == *id));
    app.auto_archive_issued
        .retain(|id| app.model.rows.iter().any(|r| r.id == *id));
    app.auto_archive_retry_after_ms
        .retain(|id, _| app.model.rows.iter().any(|r| r.id == *id));

    // One worker at a time: a bridge ownership change can stop the backlog
    // between Sessions instead of leaving an uncancellable queue behind.
    if !auto_archive_worker_available(&app.auto_archive_issued) {
        return;
    }

    let minutes = auto_stop_archive_minutes();
    if minutes == 0 {
        return;
    }
    let threshold_ms = minutes * 60_000;

    let mut due: Option<String> = None;
    for row in &app.model.rows {
        // Finish a previously accepted stop whose exited manifest missed the
        // bounded wait. archive_session is idempotent for an exited Host: it
        // observes the exited manifest immediately and writes the marker.
        if stopped_auto_archive_retry_due(
            row,
            &app.auto_archive_issued,
            &app.auto_archive_retry_after_ms,
            now_ms,
        ) {
            due = Some(row.id.clone());
            break;
        }
        if !row.running || row.status != Status::Idle {
            continue;
        }
        // Plain shells are exempt: a quiet long-lived process looks idle
        // for days, and a shell has no conversation to resume.
        if row.command.is_empty() {
            continue;
        }
        if row.pinned || row.archived {
            continue;
        }
        if app.selected_id.as_deref() == Some(row.id.as_str()) {
            continue;
        }
        // Settled while unobserved: the user hasn't seen the result yet.
        if app.unread_ids.contains(&row.id) {
            continue;
        }
        if auto_archive_attempt_blocked(
            &app.auto_archive_issued,
            &app.auto_archive_retry_after_ms,
            &row.id,
            now_ms,
        ) {
            continue;
        }
        let Some(&idle_since) = app.idle_since_ms.get(&row.id) else {
            continue;
        };
        if now_ms.saturating_sub(idle_since) < threshold_ms {
            continue;
        }
        due = Some(row.id.clone());
        break;
    }
    let Some(id) = due else {
        return;
    };
    app.auto_archive_issued.insert(id.clone());
    // archive_session waits on the host's exited manifest — never on the
    // render thread. The marker lands on disk, so the next rescan (or the
    // lifecycle state-bus ping) refreshes every frontend.
    let outcomes = outcomes.clone();
    std::thread::spawn(move || {
        let error = unpeel_core::session_ops::archive_session(&id).err();
        let _ = outcomes.send(AutoArchiveOutcome {
            session_id: id,
            error,
        });
    });
}

/// Fold a new canonical lifecycle observation into the continuous-idle
/// clock. Raw output time is intentionally not an argument: for a hook-owned
/// agent an idle repaint can be newer than the last real turn by days.
fn observed_idle_since(existing: Option<u64>, lifecycle_at: u64, now_ms: u64) -> u64 {
    let observed = lifecycle_at.min(now_ms);
    match existing {
        Some(anchor) => anchor.max(observed),
        None if lifecycle_at == 0 => now_ms,
        None => observed,
    }
}

#[cfg(test)]
mod auto_stop_archive_tests {
    use super::{
        apply_auto_archive_outcome, auto_archive_attempt_blocked, auto_archive_worker_available,
        observed_idle_since, stopped_auto_archive_retry_due, tui_owns_auto_archive_sweep,
        tui_owns_link_authority, AutoArchiveOutcome, SessionRow, Status,
        AUTO_ARCHIVE_RETRY_DELAY_MS,
    };
    use std::collections::{HashMap, HashSet};

    #[test]
    fn idle_clock_advances_only_with_the_canonical_lifecycle_stamp() {
        let first = observed_idle_since(None, 1_000, 2_000);
        assert_eq!(first, 1_000);

        // A later sweep after any number of raw terminal repaints sees the
        // same hook lifecycle stamp, so the one-day clock stays anchored.
        let after_repaints = observed_idle_since(Some(first), 1_000, 90_000_000);
        assert_eq!(after_repaints, 1_000);

        // A real subsequent lifecycle event moves the clock forward.
        let after_turn = observed_idle_since(Some(after_repaints), 80_000_000, 90_000_000);
        assert_eq!(after_turn, 80_000_000);

        // Future-skewed filesystem timestamps cannot postpone cleanup past
        // the current observation time.
        assert_eq!(
            observed_idle_since(None, 100_000_000, 90_000_000),
            90_000_000
        );
    }

    #[test]
    fn missing_lifecycle_starts_a_fresh_idle_clock_instead_of_epoch() {
        assert_eq!(observed_idle_since(None, 0, 90_000_000), 90_000_000);
    }

    #[test]
    fn failed_archive_attempt_releases_in_flight_and_retries_after_backoff() {
        let mut issued = HashSet::from(["session-1".to_string()]);
        let mut retry_after = HashMap::new();
        let now_ms = 1_000;

        apply_auto_archive_outcome(
            &mut issued,
            &mut retry_after,
            AutoArchiveOutcome {
                session_id: "session-1".to_string(),
                error: Some("temporary host failure".to_string()),
            },
            now_ms,
        );
        assert!(!issued.contains("session-1"));
        assert!(auto_archive_attempt_blocked(
            &issued,
            &retry_after,
            "session-1",
            now_ms + AUTO_ARCHIVE_RETRY_DELAY_MS - 1,
        ));
        assert!(!auto_archive_attempt_blocked(
            &issued,
            &retry_after,
            "session-1",
            now_ms + AUTO_ARCHIVE_RETRY_DELAY_MS,
        ));

        issued.insert("session-1".to_string());
        assert!(auto_archive_attempt_blocked(
            &issued,
            &retry_after,
            "session-1",
            u64::MAX,
        ));
        apply_auto_archive_outcome(
            &mut issued,
            &mut retry_after,
            AutoArchiveOutcome {
                session_id: "session-1".to_string(),
                error: None,
            },
            u64::MAX,
        );
        assert!(!issued.contains("session-1"));
        assert!(!retry_after.contains_key("session-1"));
    }

    #[test]
    fn late_exited_session_retries_marker_after_backoff_without_running_again() {
        let row = SessionRow {
            id: "session-1".to_string(),
            project_id: "project-1".to_string(),
            label: "Session".to_string(),
            command: "claude".to_string(),
            active_runtime_id: None,
            resume_available: true,
            resume_agent_available: false,
            running: false,
            status: Status::Exited,
            created_at: 1,
            pinned: false,
            archived: false,
            unread: false,
            cwd: "/tmp".to_string(),
            activity_at: 2,
            group_id: "project-1".to_string(),
            detected_local_urls: Vec::new(),
        };
        let retry_after = HashMap::from([("session-1".to_string(), 2_000)]);

        let mut issued = HashSet::new();
        assert!(!stopped_auto_archive_retry_due(
            &row,
            &issued,
            &retry_after,
            1_999
        ));
        assert!(stopped_auto_archive_retry_due(
            &row,
            &issued,
            &retry_after,
            2_000
        ));

        issued.insert("session-1".to_string());
        assert!(!stopped_auto_archive_retry_due(
            &row,
            &issued,
            &retry_after,
            2_000
        ));
        issued.clear();

        let mut archived = row.clone();
        archived.archived = true;
        assert!(!stopped_auto_archive_retry_due(
            &archived,
            &issued,
            &retry_after,
            2_000
        ));

        let mut pinned = row;
        pinned.pinned = true;
        assert!(!stopped_auto_archive_retry_due(
            &pinned,
            &issued,
            &retry_after,
            2_000
        ));
    }

    #[test]
    fn auto_archive_ownership_waits_for_resolved_standalone_mode() {
        assert!(!tui_owns_auto_archive_sweep(false, "", false));
        assert!(!tui_owns_auto_archive_sweep(false, "connecting…", false));
        assert!(!tui_owns_auto_archive_sweep(
            false,
            "app update pending",
            false
        ));
        assert!(!tui_owns_auto_archive_sweep(true, "", false));
        assert!(!tui_owns_auto_archive_sweep(false, "app offline", true));
        assert!(tui_owns_auto_archive_sweep(false, "app offline", false));
    }

    #[test]
    fn link_ownership_requires_exact_mobile_claim_without_native_authority() {
        assert!(!tui_owns_link_authority(false, false));
        assert!(tui_owns_link_authority(true, false));
        assert!(!tui_owns_link_authority(true, true));
    }

    #[test]
    fn auto_archive_dispatch_allows_only_one_in_flight_session() {
        let mut issued = HashSet::new();
        assert!(auto_archive_worker_available(&issued));
        issued.insert("session-1".to_string());
        assert!(!auto_archive_worker_available(&issued));
    }
}

/// Avatar choices for the Unpeel Apps identity (Settings ▸ Remote ▸
/// Unpeel Link).
pub const LINK_AVATARS: [&str; 8] = ["🦊", "🐙", "🦉", "🐢", "🐝", "🦁", "🐬", "🌵"];

/// A string field from shared app-state.json (profile_display_name etc.).
pub fn profile_value(key: &str) -> String {
    let state: serde_json::Value = std::fs::read(unpeel_core::app_paths::app_state_path())
        .ok()
        .and_then(|raw| serde_json::from_slice(&raw).ok())
        .unwrap_or_default();
    state
        .get(key)
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string()
}

/// ⏎ on a Settings ▸ Remote ▸ Unpeel Link row (row is relative to the
/// first Link row): activate / deactivate / save name / cycle avatar.
/// Service requests run on the Link worker; this handler only changes local
/// fail-closed state and queues work, so Settings remains responsive offline.
fn link_settings_enter(app: &mut App, row: usize, licensed: bool) {
    if (!licensed || row == 0) && !tui_owns_link_authority_now(app) {
        app.info = Some("the Unpeel app currently owns Link settings".into());
        return;
    }
    if !licensed {
        let key = app.link_input.trim().to_string();
        if key.is_empty() {
            return;
        }
        if app.link_activation_in_flight {
            app.info = Some("Link activation is already in progress".into());
            return;
        }
        let queued = app.link_worker.as_ref().is_some_and(|worker| {
            worker
                .send(LinkWorkerRequest::Activate { raw_key: key })
                .is_ok()
        });
        if queued {
            app.link_activation_in_flight = true;
            app.info = Some("activating Unpeel Link…".into());
        } else {
            app.info = Some("Link activation worker is busy; try again".into());
        }
        return;
    }
    match row {
        0 => {
            suppress_current_relay_entitlement(app);
            if let Some(uplink) = app.relay_uplink.take() {
                uplink.stop();
            }
            app.info = Some(match unpeel_core::license::deactivate_local() {
                Ok(Some(license_key)) => {
                    let _ = app.link_worker.as_ref().and_then(|worker| {
                        worker
                            .send(LinkWorkerRequest::DeactivateRemote { license_key })
                            .ok()
                    });
                    "Unpeel Link deactivated on this machine".into()
                }
                Ok(None) => "Unpeel Link deactivated on this machine".into(),
                Err(error) => {
                    format!("Unpeel Link stopped, but local deactivation could not finish: {error}")
                }
            });
        }
        1 => {
            let name = app.link_input.trim().to_string();
            let result = update_app_state(|state| {
                state.insert(
                    "profile_display_name".to_string(),
                    serde_json::Value::String(name.clone()),
                );
            });
            app.info = Some(match result {
                Ok(()) => "display name saved".into(),
                Err(e) => e,
            });
            app.link_input.clear();
        }
        2 => {
            let current = profile_value("profile_avatar");
            let position = LINK_AVATARS
                .iter()
                .position(|a| **a == *current)
                .unwrap_or(LINK_AVATARS.len() - 1);
            let next = LINK_AVATARS[(position + 1) % LINK_AVATARS.len()];
            let result = update_app_state(|state| {
                state.insert(
                    "profile_avatar".to_string(),
                    serde_json::Value::String(next.to_string()),
                );
            });
            if let Err(e) = result {
                app.info = Some(e);
            }
        }
        _ => {}
    }
}

pub fn access_setting_value(key: &str, cycle: &[&str]) -> String {
    let state: serde_json::Value = std::fs::read(unpeel_core::app_paths::app_state_path())
        .ok()
        .and_then(|raw| serde_json::from_slice(&raw).ok())
        .unwrap_or_default();
    match state.get(key) {
        Some(serde_json::Value::String(s)) => s.clone(),
        Some(serde_json::Value::Bool(b)) => b.to_string(),
        _ => cycle[0].to_string(),
    }
}

/// Cycle one policy to its next value and persist it (the MCP host re-reads
/// these per call, so changes apply live to running sessions).
fn cycle_access_setting(index: usize) -> Result<(), String> {
    let Some((key, _, cycle)) = ACCESS_SETTINGS.get(index) else {
        return Ok(());
    };
    let current = access_setting_value(key, cycle);
    let position = cycle.iter().position(|v| *v == current).unwrap_or(0);
    let next = cycle[(position + 1) % cycle.len()];
    update_app_state(|state| {
        let value = match next {
            "true" => serde_json::Value::Bool(true),
            "false" => serde_json::Value::Bool(false),
            other => serde_json::Value::String(other.to_string()),
        };
        state.insert(key.to_string(), value);
    })
}

/// Move a preset within the shared list — the desktop's drag-to-reorder,
/// which is also how the default preset per CLI is chosen.
fn move_preset(from: usize, to: usize) -> Result<(), String> {
    update_app_state(|state| {
        if let Some(list) = state.get_mut("presets").and_then(|v| v.as_array_mut()) {
            if from < list.len() && to < list.len() {
                let item = list.remove(from);
                list.insert(to, item);
            }
        }
    })
}

/// Toggle `quick_launch` — the desktop's star (sidebar quick-launch chips).
fn toggle_preset_star(index: usize) -> Result<(), String> {
    update_app_state(|state| {
        if let Some(preset) = state
            .get_mut("presets")
            .and_then(|v| v.as_array_mut())
            .and_then(|list| list.get_mut(index))
            .and_then(|p| p.as_object_mut())
        {
            let starred = preset
                .get("quick_launch")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            preset.insert("quick_launch".into(), (!starred).into());
        }
    })
}

pub fn app_state_presets() -> Vec<(String, String, bool, bool)> {
    let state: serde_json::Value = std::fs::read(unpeel_core::app_paths::app_state_path())
        .ok()
        .and_then(|raw| serde_json::from_slice(&raw).ok())
        .unwrap_or_default();
    state
        .get("presets")
        .and_then(|v| v.as_array())
        .map(|list| {
            list.iter()
                .filter_map(|p| {
                    Some((
                        p.get("label")?.as_str()?.to_string(),
                        p.get("command")?.as_str()?.to_string(),
                        p.get("enabled").and_then(|v| v.as_bool()).unwrap_or(true),
                        p.get("quick_launch")
                            .and_then(|v| v.as_bool())
                            .unwrap_or(false),
                    ))
                })
                .collect()
        })
        .unwrap_or_default()
}

fn toggle_preset_enabled(index: usize) -> Result<(), String> {
    update_app_state(|state| {
        if let Some(preset) = state
            .get_mut("presets")
            .and_then(|v| v.as_array_mut())
            .and_then(|list| list.get_mut(index))
            .and_then(|p| p.as_object_mut())
        {
            let enabled = preset
                .get("enabled")
                .and_then(|v| v.as_bool())
                .unwrap_or(true);
            preset.insert("enabled".into(), (!enabled).into());
        }
    })
}

/// Add a preset to the shared list. The label defaults to the command, the
/// same shape `unpeel presets add` writes.
fn add_preset(command: &str) -> Result<(), String> {
    update_app_state(|state| {
        let preset = serde_json::json!({
            "id": format!("tui-{}", uuid::Uuid::new_v4()),
            "label": command,
            "command": command,
            "project_id": serde_json::Value::Null,
            "enabled": true,
            "quick_launch": false,
        });
        if let Some(list) = state
            .entry("presets".to_string())
            .or_insert_with(|| serde_json::json!([]))
            .as_array_mut()
        {
            list.push(preset);
        }
    })
}

fn remove_preset_at(index: usize) -> Result<(), String> {
    update_app_state(|state| {
        if let Some(list) = state.get_mut("presets").and_then(|v| v.as_array_mut()) {
            if index < list.len() {
                list.remove(index);
            }
        }
    })
}

/// Read-modify-write app-state.json through the guarded core helper, which
/// refuses to clobber a file it couldn't parse and refuses to drop keys the
/// desktop owns.
fn update_app_state(
    edit: impl FnOnce(&mut serde_json::Map<String, serde_json::Value>),
) -> Result<(), String> {
    unpeel_core::app_state::edit(|object| {
        edit(object);
        Ok(())
    })
}

/// Open (or close) the pairing window and render its QR.
fn open_pairing(app: &mut App) {
    if matches!(app.modal, Some(Modal::Pairing { .. })) {
        app.pairing.cancel();
        app.modal = None;
        return;
    }
    let Some(server) = &app.mobile_server else {
        app.info = Some("phone serving is off — close the Unpeel app to pair here".into());
        return;
    };
    let mac_id = match ensure_mac_id() {
        Ok(mac_id) => mac_id,
        Err(error) => {
            app.info = Some(error);
            return;
        }
    };
    let host = mobile::preferred_lan_address();
    match app.pairing.begin(&host, server.port, &mac_id) {
        Some((code, _)) => {
            app.modal = Some(Modal::Pairing {
                lines: pairing::qr_lines(&code),
                code,
            })
        }
        None => app.info = Some("could not open pairing".into()),
    }
}

/// Paired phones from the shared devices.json: (id, name, platform).
/// One paired Controller from `~/.unpeel/mobile/devices.json` — the same
/// record the app's Settings ▸ Remote devices list renders.
pub struct PairedDevice {
    pub id: String,
    pub name: String,
    pub platform: String,
    pub app_version: Option<String>,
    pub last_seen_unix_ms: Option<u64>,
    /// False = Direct-only: the relay uplink never announces this device.
    /// Absent in the file means allowed (the app writes the same key).
    pub relay_allowed: bool,
}

pub fn paired_devices() -> Vec<PairedDevice> {
    let path = unpeel_core::app_paths::unpeel_home()
        .join("mobile")
        .join("devices.json");
    std::fs::read(path)
        .ok()
        .and_then(|raw| serde_json::from_slice::<serde_json::Value>(&raw).ok())
        .and_then(|v| v.get("devices").and_then(|d| d.as_array()).cloned())
        .unwrap_or_default()
        .iter()
        .filter_map(|d| {
            Some(PairedDevice {
                id: d.get("id")?.as_str()?.to_string(),
                name: d
                    .get("name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("phone")
                    .to_string(),
                platform: d
                    .get("platform")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
                app_version: d
                    .get("appVersion")
                    .and_then(|v| v.as_str())
                    .map(|v| v.to_string()),
                last_seen_unix_ms: d.get("lastSeenAtUnixMs").and_then(|v| v.as_u64()),
                relay_allowed: d.get("relayAllowed").and_then(|v| v.as_bool()) != Some(false),
            })
        })
        .collect()
}

/// Select the latest settled activity under the stable launch command's
/// authority. An observed runtime is presentation-only: a blank shell must
/// never bind a stale hook seed left by a previous agent invocation.
fn resolve_settled_at(
    launch_command: &str,
    hook_event_at: Option<u64>,
    screen_changed_at: Option<u64>,
    output_at: Option<u64>,
) -> Option<u64> {
    if unpeel_core::integrations::uses_hook_port(unpeel_core::integrations::command_head(
        launch_command,
    )) {
        hook_event_at
    } else {
        screen_changed_at.or(output_at)
    }
}

/// When a session last settled (finished a turn): the durable hook seed's
/// mtime only for a hook-capable launch command, else the host's parsed-screen
/// change stamp (idle repaint loops never advance it), else the output tail's
/// mtime for sessions hosted by older builds.
fn settled_at(session_id: &str, launch_command: &str) -> Option<u64> {
    let dir = unpeel_core::app_paths::app_sessions_root().join(session_id);
    let stamp = |name: &str| -> Option<u64> {
        let modified = std::fs::metadata(dir.join(name)).ok()?.modified().ok()?;
        Some(
            modified
                .duration_since(std::time::UNIX_EPOCH)
                .ok()?
                .as_millis() as u64,
        )
    };
    resolve_settled_at(
        launch_command,
        stamp("last-hook-event.json"),
        unpeel_core::session_ops::screen_changed_at_ms(session_id),
        stamp("output.bin"),
    )
}

#[cfg(test)]
mod settled_at_tests {
    use super::resolve_settled_at;

    #[test]
    fn hook_capable_launch_uses_only_hook_activity() {
        assert_eq!(
            resolve_settled_at("claude", Some(30), Some(20), Some(10)),
            Some(30)
        );
        assert_eq!(resolve_settled_at("claude", None, Some(20), Some(10)), None);
    }

    #[test]
    fn blank_launch_ignores_stale_hook_activity() {
        assert_eq!(
            resolve_settled_at("", Some(30), Some(20), Some(10)),
            Some(20)
        );
        assert_eq!(resolve_settled_at("", Some(30), None, Some(10)), Some(10));
    }
}

/// TUI-owned layout prefs (`~/.unpeel/tui-layout.json`): sidebar width,
/// collapsed folders, last selection — the desktop persists the same three.
fn layout_path() -> std::path::PathBuf {
    unpeel_core::app_paths::unpeel_home().join("tui-layout.json")
}

fn load_layout() -> (u16, HashSet<String>, Option<String>) {
    let value: serde_json::Value = std::fs::read(layout_path())
        .ok()
        .and_then(|raw| serde_json::from_slice(&raw).ok())
        .unwrap_or_default();
    let width = value
        .get("sidebar_width")
        .and_then(|v| v.as_u64())
        .map(|w| (w as u16).clamp(ui::MIN_SIDEBAR_WIDTH, ui::MAX_SIDEBAR_WIDTH))
        .unwrap_or(36);
    let collapsed = value
        .get("collapsed")
        .and_then(|v| v.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default();
    let selected = value
        .get("selected")
        .and_then(|v| v.as_str())
        .map(str::to_owned);
    (width, collapsed, selected)
}

fn save_layout(app: &App) {
    let value = serde_json::json!({
        "sidebar_width": app.sidebar_width,
        "collapsed": app.collapsed.iter().cloned().collect::<Vec<_>>(),
        "selected": app.selected_id,
    });
    if let Ok(body) = serde_json::to_vec_pretty(&value) {
        let path = layout_path();
        let tmp = path.with_extension("json.tmp");
        if std::fs::write(&tmp, body).is_ok() {
            let _ = std::fs::rename(&tmp, &path);
        }
    }
}

/// The Host identity phones pair against (`~/.unpeel/mobile/mac-id`). The
/// native app uses the sibling `mac-id.lock`, so Rust must take that same
/// cross-process lock and re-read before publishing an atomic private file.
pub fn ensure_mac_id() -> Result<String, String> {
    unpeel_core::relay_uplink::ensure_host_id()
}

/// Pin/unpin via the shared app-state.json `pinned_sessions` contract — the
/// desktop reads this file, so TUI pins show up there too.
fn set_pin_in_app_state(session_id: &str, pinned: bool) -> Result<(), String> {
    let project_id = sessions::scan_project_of(session_id).unwrap_or_default();
    unpeel_core::app_state::edit(|state| {
        let pins = state
            .get_mut("pinned_sessions")
            .and_then(|v| v.as_object_mut())
            .ok_or("app-state.json has no pinned_sessions")?;
        if pinned {
            let entry = serde_json::json!({
                "key": format!("session:{session_id}"),
                "project_id": project_id,
                "session_id": session_id,
                "pinned_at": std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_millis() as u64,
            });
            pins.entry(project_id.clone())
                .or_insert_with(|| serde_json::json!([]))
                .as_array_mut()
                .ok_or("pinned_sessions entry is not a list")?
                .push(entry);
        } else {
            for list in pins.values_mut() {
                if let Some(array) = list.as_array_mut() {
                    array.retain(|p| {
                        p.get("session_id").and_then(|v| v.as_str()) != Some(session_id)
                    });
                }
            }
        }
        Ok(())
    })
}

/// Append a project to the shared app-state.json contract (atomic write).
/// Compare project paths the way a user means them: trailing slashes and
/// symlinks are not a different folder.
fn same_folder(a: &str, b: &str) -> bool {
    let norm = |p: &str| {
        let trimmed = p.trim_end_matches('/');
        std::fs::canonicalize(trimmed)
            .map(|c| c.to_string_lossy().into_owned())
            .unwrap_or_else(|_| trimmed.to_string())
    };
    !a.is_empty() && norm(a) == norm(b)
}

/// What adding a folder did. A folder that is already a project is not an
/// error — the desktop just takes you to the one you have (`ensureProject`),
/// and the terminal should not be pricklier about it.
pub enum AddProject {
    Added,
    Existing { id: String, name: String },
}

pub fn add_project_to_app_state(name: &str, path: &str) -> Result<AddProject, String> {
    // The app's own projects live in ITS UserDefaults, not in app-state.json,
    // so checking the file alone cheerfully adds a second "unpeel" pointing
    // at the same folder — which then shows up in the desktop as an empty
    // duplicate of a project full of sessions.
    if let Some(overlay) = crate::overlay::load() {
        if let Some((id, existing)) = overlay
            .project_paths
            .iter()
            .find(|(_, existing)| same_folder(existing, path))
        {
            let label = overlay
                .projects
                .iter()
                .find(|(pid, _)| pid == id)
                .map(|(_, name)| name.clone())
                .unwrap_or_else(|| existing.clone());
            return Ok(AddProject::Existing {
                id: id.clone(),
                name: label,
            });
        }
    }
    unpeel_core::app_state::edit(|state| {
        let projects = state
            .entry("projects".to_string())
            .or_insert_with(|| serde_json::json!([]))
            .as_array_mut()
            .ok_or("app-state.json has no projects array")?;
        if let Some(existing) = projects.iter().find(|p| {
            p.get("path")
                .and_then(|v| v.as_str())
                .is_some_and(|existing| same_folder(existing, path))
        }) {
            return Ok(AddProject::Existing {
                id: existing
                    .get("id")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default()
                    .to_string(),
                name: existing
                    .get("name")
                    .and_then(|v| v.as_str())
                    .unwrap_or(name)
                    .to_string(),
            });
        }
        projects.push(serde_json::json!({
            "id": format!("tui-{}", uuid::Uuid::new_v4()),
            "name": name,
            "path": path,
        }));
        Ok(AddProject::Added)
    })
}

pub fn presets_cli(args: &[String]) -> Result<(), String> {
    let load = unpeel_core::app_state::load_for_edit;
    let save = unpeel_core::app_state::save;
    match args.first().map(String::as_str) {
        Some("list") | None => {
            let mut shown = 0usize;
            let mut overlay_superseded = false;
            if let Ok(state) = load() {
                overlay_superseded = state
                    .get("native_preset_overlay_migrated")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                if let Some(presets) = state.get("presets").and_then(|v| v.as_array()) {
                    for p in presets {
                        let enabled = p.get("enabled").and_then(|v| v.as_bool()).unwrap_or(true);
                        println!(
                            "{} {:24} {}",
                            if enabled { " " } else { "x" },
                            p.get("label").and_then(|v| v.as_str()).unwrap_or(""),
                            p.get("command").and_then(|v| v.as_str()).unwrap_or(""),
                        );
                        shown += 1;
                    }
                }
            }
            // Presets still held in the app's UserDefaults overlay: read-only
            // until the app runs once and folds them into app-state.json.
            if !overlay_superseded {
                if let Some(overlay) = overlay::load() {
                    for (label, command) in &overlay.presets {
                        println!("- {label:24} {command}   (in the app — open it once to migrate)");
                        shown += 1;
                    }
                }
            }
            if shown == 0 {
                println!("no presets -- add one: unpeel presets add <label> <command>");
            }
            Ok(())
        }
        Some("add") => {
            let (Some(label), Some(command)) = (args.get(1), args.get(2)) else {
                return Err("usage: unpeel presets add <label> <command>".into());
            };
            let mut state = load().unwrap_or_else(|_| {
                serde_json::json!({
                    "projects": [], "active_project_id": null, "presets": [],
                    "active_tabs": {}, "pinned_sessions": {}
                })
            });
            let preset = serde_json::json!({
                "id": format!("tui-{}", uuid::Uuid::new_v4()),
                "label": label,
                "command": command,
                "project_id": null,
                "enabled": true,
                "quick_launch": false,
            });
            state
                .get_mut("presets")
                .and_then(|v| v.as_array_mut())
                .ok_or("app-state.json has no presets array")?
                .push(preset);
            save(&state)?;
            println!("added: {label} -- {command}");
            Ok(())
        }
        Some("remove") => {
            let Some(needle) = args.get(1) else {
                return Err("usage: unpeel presets remove <label>".into());
            };
            let mut state = load()?;
            let presets = state
                .get_mut("presets")
                .and_then(|v| v.as_array_mut())
                .ok_or("app-state.json has no presets array")?;
            let before = presets.len();
            presets.retain(|p| p.get("label").and_then(|v| v.as_str()) != Some(needle.as_str()));
            if presets.len() == before {
                return Err(format!(
                    "no preset labelled {needle:?} in app-state.json (a preset still held \
                     in the app's overlay migrates on the app's next launch)"
                ));
            }
            save(&state)?;
            println!("removed: {needle}");
            Ok(())
        }
        Some(other) => Err(format!("unknown presets subcommand: {other}")),
    }
}

/// One-shot probe for the terminal's background so the selection bar can
/// pick white-on-dark vs black-on-light (`ui::set_light_background`).
/// Raw mode must already be on (the reply arrives unbuffered on stdin).
/// Never blocks past ~150ms: terminals that don't answer OSC 11 fall back
/// to COLORFGBG, then to dark.
fn detect_light_background() -> bool {
    if let Some(light) = osc11_reports_light() {
        return light;
    }
    // COLORFGBG is "<fg>;<bg>" (sometimes "<fg>;default;<bg>"); 7/15 are
    // the light palette backgrounds.
    std::env::var("COLORFGBG")
        .ok()
        .and_then(|v| v.split(';').next_back()?.parse::<u8>().ok())
        .map(|bg| matches!(bg, 7 | 15))
        .unwrap_or(false)
}

/// Ask via OSC 11 and read the `rgb:RRRR/GGGG/BBBB` reply straight off the
/// stdin fd — this runs before crossterm's event reader exists, so nothing
/// else is consuming the tty.
fn osc11_reports_light() -> Option<bool> {
    use std::io::Write;
    use std::os::fd::AsRawFd;

    let mut out = io::stdout();
    out.write_all(b"\x1b]11;?\x1b\\").ok()?;
    out.flush().ok()?;

    let fd = io::stdin().as_raw_fd();
    let deadline = Instant::now() + Duration::from_millis(150);
    let mut buf: Vec<u8> = Vec::new();
    loop {
        let remaining = deadline.checked_duration_since(Instant::now())?;
        let mut pfd = libc::pollfd {
            fd,
            events: libc::POLLIN,
            revents: 0,
        };
        let ready = unsafe { libc::poll(&mut pfd, 1, remaining.as_millis() as i32) };
        if ready <= 0 {
            return None;
        }
        let mut chunk = [0u8; 64];
        let read = unsafe { libc::read(fd, chunk.as_mut_ptr().cast(), chunk.len()) };
        if read <= 0 {
            return None;
        }
        buf.extend_from_slice(&chunk[..read as usize]);
        // Terminated by BEL or ST depending on the terminal.
        if buf.contains(&0x07) || buf.windows(2).any(|w| w == b"\x1b\\") {
            break;
        }
        if buf.len() > 256 {
            return None;
        }
    }

    let text = String::from_utf8_lossy(&buf);
    let mut channels = text.split("rgb:").nth(1)?.split('/').take(3).map(|part| {
        let hex: String = part.chars().take_while(char::is_ascii_hexdigit).collect();
        let value = u32::from_str_radix(&hex, 16).ok()?;
        let max = 16u32.checked_pow(hex.len() as u32)? - 1;
        Some(value as f32 / max.max(1) as f32)
    });
    let r = channels.next()??;
    let g = channels.next()??;
    let b = channels.next()??;
    Some(0.2126 * r + 0.7152 * g + 0.0722 * b > 0.55)
}

#[cfg(test)]
mod terminal_selection_tests {
    use super::*;
    use unpeel_core::terminal_viewport::TerminalViewportRow;

    fn snapshot(rows: &[(&str, bool)], cols: u16) -> TerminalViewportSnapshot {
        TerminalViewportSnapshot {
            cols,
            rows: rows.len() as u16,
            output_offset: 0,
            truncated: false,
            cursor_row: 0,
            cursor_col: 0,
            scrollback_rows: 0,
            viewport_start_row: 0,
            scroll_offset_rows: 0,
            input_modes_known: true,
            mouse_reporting: false,
            mouse_button_motion: false,
            mouse_any_motion: false,
            alternate_screen: false,
            mouse_alternate_scroll: false,
            application_cursor: false,
            viewport_rows: rows
                .iter()
                .map(|(text, wrapped)| TerminalViewportRow {
                    text: (*text).into(),
                    styles: Vec::new(),
                    wrapped: *wrapped,
                })
                .collect(),
        }
    }

    #[test]
    fn copied_selection_unwraps_soft_wraps_but_keeps_hard_lines() {
        let mut soft = TerminalSelection::anchor(
            "s".into(),
            snapshot(&[("abcd", true), ("efgh", false)], 4),
            0,
            0,
        );
        soft.drag(1, 3);
        assert_eq!(soft.selected_text().as_deref(), Some("abcdefgh"));

        let mut hard = TerminalSelection::anchor(
            "s".into(),
            snapshot(&[("abcd", false), ("efgh", false)], 4),
            0,
            0,
        );
        hard.drag(1, 3);
        assert_eq!(hard.selected_text().as_deref(), Some("abcd\nefgh"));
    }

    #[test]
    fn copied_selection_uses_terminal_cell_width_for_wide_text() {
        let mut selection =
            TerminalSelection::anchor("s".into(), snapshot(&[("a🙂b", false)], 4), 0, 1);
        selection.drag(0, 2);
        assert_eq!(selection.selected_text().as_deref(), Some("🙂"));
    }

    #[test]
    fn backspace_edit_moves_to_selection_end_and_deletes_graphemes() {
        let mut view = snapshot(&[("› hello world", false)], 13);
        view.cursor_col = 13;
        let mut selection = TerminalSelection::anchor("s".into(), view, 0, 2);
        selection.drag(0, 6);
        assert_eq!(
            selection.backspace_edit_sequence().as_deref(),
            Some("\x1b[D\x1b[D\x1b[D\x1b[D\x1b[D\x1b[D\x7f\x7f\x7f\x7f\x7f")
        );
    }

    #[test]
    fn backspace_edit_never_treats_transcript_rows_as_input() {
        let mut view = snapshot(&[("old output", false), ("› draft", false)], 10);
        view.cursor_row = 1;
        view.cursor_col = 7;
        let mut selection = TerminalSelection::anchor("s".into(), view, 0, 0);
        selection.drag(0, 2);
        assert!(selection.backspace_edit_sequence().is_none());
    }
}

fn main() -> io::Result<()> {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    // `--workspace` re-homes the whole process (UNPEEL_HOME), so it must be
    // claimed before any dispatch touches state — spawned hosts inherit it.
    match workspaces::claim_workspace_flag(&mut args) {
        Ok(Some(reference)) => {
            if let Err(error) = workspaces::enter(&reference) {
                eprintln!("unpeel: {error}");
                std::process::exit(2);
            }
        }
        Ok(None) => {}
        Err(error) => {
            eprintln!("unpeel: {error}");
            std::process::exit(2);
        }
    }
    match remote_scope::parse_host_target(&args) {
        Ok(Some(target)) => return remote_scope::run(target),
        Ok(None) => {}
        Err(error) => {
            eprintln!("unpeel: {error}");
            std::process::exit(2);
        }
    }
    if let Some(code) = cli::run(&args) {
        // One-shot verbs exit immediately; wait for their change pings to
        // actually reach the other frontends first.
        unpeel_core::state_bus::flush();
        std::process::exit(code);
    }

    // Upgrade maintenance belongs to both Host frontends. Keep it off the
    // render-start critical path; per-Session lifecycle locks make concurrent
    // desktop/TUI passes harmless and live pre-v4 Hosts stay untouched.
    let _ = std::thread::Builder::new()
        .name("unpeel-output-journal-maintenance".into())
        .spawn(|| {
            let _ = unpeel_core::session_host::compact_exited_output_journals();
        });

    // One aggregate pane authority belongs to the interactive TUI only.
    // The lifecycle guard releases it on every ordinary return or unwind.
    let herdr_reporter = herdr::HerdrReporter::from_env();

    let approval_hub = std::sync::Arc::new(approvals::ApprovalHub::default());
    let listener = hook_listener::start(std::sync::Arc::clone(&approval_hub));
    let (hook_events, hook_port) = match listener {
        Ok(l) => (Some(l.events), Some(l.port)),
        Err(_) => (None, None),
    };

    // Restore the terminal and the port registry even if we panic mid-draw.
    let default_hook = std::panic::take_hook();
    let panic_port = hook_port;
    std::panic::set_hook(Box::new(move |info| {
        let _ = execute!(
            io::stdout(),
            ratatui::crossterm::event::PopKeyboardEnhancementFlags
        );
        let _ = disable_raw_mode();
        let _ = execute!(io::stdout(), LeaveAlternateScreen, DisableMouseCapture);
        if let Some(port) = panic_port {
            hook_listener::unregister_port(port);
        }
        default_hook(info);
    }));

    enable_raw_mode()?;
    ui::set_light_background(detect_light_background());
    execute!(
        io::stdout(),
        EnterAlternateScreen,
        EnableMouseCapture,
        EnableBracketedPaste
    )?;
    // Ctrl+digit does not exist in the legacy encoding — a terminal sends a
    // plain "1" for ctrl+1 — so ^1…^9 needs the kitty keyboard protocol,
    // which Ghostty speaks. Where it is missing those keys simply never
    // arrive and nothing else changes.
    //
    // DISAMBIGUATE only. REPORT_ALL_KEYS_AS_ESCAPE_CODES would also report
    // bare modifier presses (which would let the list preview ^1…^9 while
    // ctrl is held), but it makes crossterm report the UNSHIFTED key: on a
    // layout where `?` is shift and `+` share a key, pressing `?` arrives as
    // `+` and opens the new-project dialog. crossterm 0.28 has no
    // REPORT_ASSOCIATED_TEXT support to recover the real character, so
    // correct typing wins over the preview.
    // Pushed unconditionally, NOT gated on `supports_keyboard_enhancement()`:
    // that query blocks for up to two seconds waiting for a reply, reading
    // stdin while it waits — so on any terminal that doesn't answer, startup
    // stalls and the user's first keystrokes are swallowed. A terminal
    // without the protocol simply ignores this sequence.
    let _ = execute!(
        io::stdout(),
        ratatui::crossterm::event::PushKeyboardEnhancementFlags(
            ratatui::crossterm::event::KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES
        )
    );
    let mut terminal = Terminal::new(CrosstermBackend::new(io::stdout()))?;
    if let Some(port) = hook_port {
        unpeel_core::session_ops::set_own_listener_port(port);
    }
    let result = run(
        &mut terminal,
        hook_events,
        hook_port,
        approval_hub,
        herdr_reporter.as_ref(),
    );
    let _ = execute!(
        io::stdout(),
        ratatui::crossterm::event::PopKeyboardEnhancementFlags
    );
    disable_raw_mode()?;
    execute!(
        io::stdout(),
        LeaveAlternateScreen,
        DisableMouseCapture,
        DisableBracketedPaste
    )?;
    if let Some(port) = hook_port {
        hook_listener::unregister_port(port);
    }
    result
}
