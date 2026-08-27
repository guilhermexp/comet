//! Pure Controller mode for `unpeel --host ssh://…`.
//!
//! This path is selected before the local CLI, hook listener, state scan,
//! bridge, phone server, or relay can start. It renders the SAME UI as the
//! local loop — shared sidebar model, modals, and confirm flows — with every
//! verb routed through `RemoteSessionBackend`; nothing can fall through to
//! local session state, and the only visible difference is the green Host
//! name on the sidebar's bottom edge (host-controller-transports.md).

use std::collections::{HashMap, HashSet, VecDeque};
use std::io;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc};
use std::time::{Duration, Instant};

use ratatui::crossterm::event::{
    self, DisableBracketedPaste, DisableMouseCapture, EnableBracketedPaste, EnableMouseCapture,
    Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use ratatui::crossterm::execute;
use ratatui::crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, BeginSynchronizedUpdate, EndSynchronizedUpdate,
    EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::prelude::CrosstermBackend;
use ratatui::Terminal;

use unpeel_core::remote_session_backend::{
    RemoteActivityState, RemoteBootstrapSnapshot, RemoteDesktopResize, RemoteEffectFailure,
    RemoteEffectFailureKind, RemotePresetSummary, RemoteProjectSummary, RemoteSessionBackend,
    RemoteSessionCreateRequest, RemoteSessionStatus, RemoteSessionSummary,
    REMOTE_CAPABILITY_INPUT_WRITE, REMOTE_CAPABILITY_MARK_READ, REMOTE_CAPABILITY_OUTPUT_READ,
    REMOTE_CAPABILITY_RESIZE_DESKTOP, REMOTE_CAPABILITY_RESTART, REMOTE_CAPABILITY_RESUME_AGENT,
    REMOTE_DESKTOP_RESIZE_MAX_COLUMNS, REMOTE_DESKTOP_RESIZE_MAX_ROWS,
    REMOTE_DESKTOP_RESIZE_MIN_COLUMNS, REMOTE_DESKTOP_RESIZE_MIN_ROWS,
    REMOTE_TERMINAL_WRITE_MAX_BYTES,
};
use unpeel_core::ssh_connection::{SshHostConnection, SshTarget};

use crate::activity::ActivityEngine;
use crate::remote_preview::RemoteLiveStream;
use crate::sessions::{MobileSnapshot, ScanCache, SessionRow, SidebarItem, SidebarModel, Status};
use crate::snapshots::SnapshotService;
use crate::{
    approvals, control, gate_verb, handle_archive_key, handle_preset_picker_key, handle_rename_key,
    overlay, pairing, resume_unavailable_message, selected_restart_verb, ui, App,
    ArchiveKeyOutcome, InFlight, Modal, PresetPickerKeyOutcome, RenameInput, RenameKeyOutcome,
    Verb,
};

const REMOTE_TICK: Duration = Duration::from_millis(100);
const BOOTSTRAP_REFRESH: Duration = Duration::from_secs(2);
const REMOTE_SIDEBAR_WIDTH: u16 = 36;
const REQUIRED_INTERACTIVE_CAPABILITIES: [&str; 3] = [
    REMOTE_CAPABILITY_OUTPUT_READ,
    REMOTE_CAPABILITY_INPUT_WRITE,
    REMOTE_CAPABILITY_RESIZE_DESKTOP,
];

/// Parse only the first shipped Controller selector. Any appearance of
/// `--host` is claimed here so malformed remote intent can never fall through
/// into the local CLI or local interactive UI.
pub fn parse_host_target(args: &[String]) -> Result<Option<SshTarget>, String> {
    let host_args: Vec<(usize, &str)> = args
        .iter()
        .enumerate()
        .filter_map(|(index, arg)| {
            (arg == "--host" || arg.starts_with("--host=")).then_some((index, arg.as_str()))
        })
        .collect();
    if host_args.is_empty() {
        return Ok(None);
    }
    if host_args.len() != 1 {
        return Err("--host may be specified only once".to_owned());
    }

    let (index, host_arg) = host_args[0];
    if index != 0 {
        return Err("remote Host scope is interactive: use `unpeel --host ssh://HOST`".to_owned());
    }

    let target = if let Some(value) = host_arg.strip_prefix("--host=") {
        if args.len() != 1 {
            return Err(
                "remote Host scope does not accept a local command or extra arguments".to_owned(),
            );
        }
        value
    } else {
        if args.len() < 2 || args[1].starts_with("--") {
            return Err("--host requires an ssh://HOST target".to_owned());
        }
        if args.len() != 2 {
            return Err(
                "remote Host scope does not accept a local command or extra arguments".to_owned(),
            );
        }
        args[1].as_str()
    };
    if target.is_empty() {
        return Err("--host requires an ssh://HOST target".to_owned());
    }
    SshTarget::parse(target)
        .map(Some)
        .map_err(|error| error.to_string())
}

pub fn run(target: SshTarget) -> io::Result<()> {
    let target_uri = target.uri().to_owned();
    let backend = RemoteSessionBackend::new(Arc::new(ssh_host_connection(target)?));
    let initial = backend.bootstrap().map_err(|error| {
        io::Error::new(
            io::ErrorKind::ConnectionRefused,
            format!("connect to {target_uri}: {error}"),
        )
    })?;
    require_interactive_capabilities(&initial.snapshot)?;

    // The host indicator prefers the Host's own advertised name, falling
    // back to the SSH target alias the user typed.
    let host_label = initial
        .snapshot
        .host_name
        .clone()
        .filter(|name| !name.trim().is_empty())
        .unwrap_or_else(|| target_alias(&target_uri));
    let model = model_from_bootstrap(&initial.snapshot);
    let mut app = remote_app(model, &host_label);
    let mut terminal_restore = TerminalRestore::enter()?;
    let mut terminal = Terminal::new(CrosstermBackend::new(io::stdout()))?;
    let (event_tx, event_rx) = mpsc::channel();
    let output_wake = RemoteWake::new(event_tx.clone());
    start_terminal_reader(event_tx.clone());
    let stop = Arc::new(AtomicBool::new(false));
    start_bootstrap_refresh(backend.clone(), Arc::clone(&stop), event_tx.clone());
    let (effect_tx, effect_rx) = mpsc::channel();
    let effects = RemoteEffectQueue {
        sender: effect_tx,
        halted: Arc::new(AtomicBool::new(false)),
        owned_fits: Arc::new(std::sync::Mutex::new(HashSet::new())),
    };
    start_effect_worker(
        backend.clone(),
        effect_rx,
        event_tx.clone(),
        Arc::clone(&effects.halted),
        Arc::clone(&effects.owned_fits),
    );

    let snapshots = SnapshotService::new();
    let controller = RemoteController {
        backend: backend.clone(),
        events: event_tx.clone(),
        presets: initial.snapshot.presets.clone(),
        projects: initial.snapshot.projects.clone(),
        sessions: initial.snapshot.sessions.clone(),
        picker_preset_ids: HashMap::new(),
        archived_overlay: None,
        replacement_selection: RemoteReplacementSelectionState::default(),
    };
    let result = run_remote_loop(
        &mut terminal,
        &mut app,
        &snapshots,
        backend.clone(),
        effects.clone(),
        controller,
        initial.snapshot.supports(REMOTE_CAPABILITY_MARK_READ),
        output_wake,
        event_rx,
    );

    stop.store(true, Ordering::Release);
    let (done_tx, done_rx) = mpsc::channel();
    let _ = effects
        .sender
        .send(RemoteEffect::Shutdown { done: done_tx });
    let _ = done_rx.recv_timeout(Duration::from_secs(2));
    backend.disconnect();
    terminal_restore.restore();
    result
}

fn ssh_host_connection(target: SshTarget) -> io::Result<SshHostConnection> {
    // The black-box PTY suite cannot rely on an sshd. Debug test processes
    // may inject an absolute fake-SSH executable that still runs the real
    // Host stdio gateway. Release builds compile this branch out completely;
    // product callers always use the fixed `/usr/bin/ssh` path.
    #[cfg(debug_assertions)]
    if std::env::var("UNPEEL_TEST").as_deref() == Ok("1") {
        if let Some(program) = std::env::var_os("UNPEEL_TEST_SSH_PROGRAM") {
            return SshHostConnection::with_ssh_program(target, program).map_err(|error| {
                io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!("invalid test SSH program: {error}"),
                )
            });
        }
    }
    Ok(SshHostConnection::new(target))
}

fn require_interactive_capabilities(snapshot: &RemoteBootstrapSnapshot) -> io::Result<()> {
    let missing: Vec<&str> = REQUIRED_INTERACTIVE_CAPABILITIES
        .iter()
        .copied()
        .filter(|capability| !snapshot.supports(capability))
        .collect();
    if missing.is_empty() {
        Ok(())
    } else {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            format!(
                "Host cannot serve an interactive TUI yet (missing {})",
                missing.join(", ")
            ),
        ))
    }
}

struct TerminalRestore {
    active: bool,
}

impl TerminalRestore {
    fn enter() -> io::Result<Self> {
        enable_raw_mode()?;
        let mut restore = Self { active: true };
        ui::set_light_background(crate::detect_light_background());
        if let Err(error) = execute!(
            io::stdout(),
            EnterAlternateScreen,
            EnableMouseCapture,
            EnableBracketedPaste
        ) {
            restore.restore();
            return Err(error);
        }
        let _ = execute!(
            io::stdout(),
            ratatui::crossterm::event::PushKeyboardEnhancementFlags(
                ratatui::crossterm::event::KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES
            )
        );
        Ok(restore)
    }

    fn restore(&mut self) {
        if !self.active {
            return;
        }
        self.active = false;
        let _ = execute!(
            io::stdout(),
            ratatui::crossterm::event::PopKeyboardEnhancementFlags
        );
        let _ = disable_raw_mode();
        let _ = execute!(
            io::stdout(),
            LeaveAlternateScreen,
            DisableMouseCapture,
            DisableBracketedPaste
        );
    }
}

impl Drop for TerminalRestore {
    fn drop(&mut self) {
        self.restore();
    }
}

enum RemoteUiEvent {
    Terminal(Event),
    Wake,
    Bootstrap(Result<RemoteBootstrapSnapshot, String>),
    Fatal(String),
    Notice(String),
    /// A lifecycle verb finished on its worker thread: outcome toast,
    /// optional session to select once the sidebar lists it, and an id to
    /// drop from the fetched archive list (a successful remove). A failed
    /// replacement carries its source id so the exact-selection latch can be
    /// released without disturbing a newer replacement request.
    VerbDone {
        message: String,
        select: Option<String>,
        archive_prune: Option<String>,
        failed_replacement_source: Option<String>,
    },
    /// `list_archived_sessions` returned for the archive library.
    ArchivedSessions {
        project_id: String,
        result: Result<Vec<RemoteSessionSummary>, String>,
    },
}

/// Controller-side verb plumbing for the shared interaction flows: the
/// backend every verb routes through, the latest bootstrap catalog the
/// preset picker and archive library read, and the per-picker command→preset
/// map. Lives on the loop, not in `App` — the shared UI state stays
/// scope-agnostic.
struct RemoteController {
    backend: RemoteSessionBackend,
    events: mpsc::Sender<RemoteUiEvent>,
    presets: Vec<RemotePresetSummary>,
    projects: Vec<RemoteProjectSummary>,
    /// Latest Host summaries, retained so a replacement selection intent can
    /// capture provider/worktree identity before either lifecycle effect.
    sessions: Vec<RemoteSessionSummary>,
    /// command → preset id for the open picker; the picker rows keep real
    /// commands so the shared renderer paints them exactly like local.
    picker_preset_ids: HashMap<String, String>,
    /// Archived Sessions fetched for the open archive library, re-merged
    /// into the model after every bootstrap so the view survives refreshes.
    archived_overlay: Option<(String, Vec<RemoteSessionSummary>)>,
    /// Replacement Resume mints a new Session id but preserves stable launch
    /// identity. Hold it across bootstraps so selection follows the exact
    /// replacement rather than the first unrelated visible row.
    replacement_selection: RemoteReplacementSelectionState,
}

const REPLACEMENT_BOOTSTRAP_OBSERVATIONS: u8 = 30;

#[derive(Default)]
struct RemoteReplacementSelectionState {
    pending: Option<PendingReplacementSelection>,
    /// Ambiguity or expiry clears `pending` but deliberately keeps this set.
    /// Only an explicit user choice may restore ordinary default selection.
    suppress_default: bool,
}

impl RemoteReplacementSelectionState {
    fn begin(&mut self, pending: PendingReplacementSelection) {
        self.pending = Some(pending);
        self.suppress_default = true;
    }

    fn clear(&mut self) {
        self.pending = None;
        self.suppress_default = false;
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct PendingReplacementSelection {
    source_id: String,
    project_id: String,
    created_at_unix_ms: i64,
    runtime_id: Option<String>,
    worktree_path: Option<String>,
    worktree_branch: Option<String>,
    /// IDs already present when Resume began can never be its replacement,
    /// even if old/corrupt data happens to reuse the stable timestamp.
    baseline_session_ids: HashSet<String>,
    bootstrap_observations_remaining: u8,
}

enum ReplacementSelectionResolution {
    Wait(PendingReplacementSelection),
    Select(String),
    Cancel,
}

/// The alias part of `ssh://[user@]host` — the host indicator fallback when
/// the Host does not advertise a name.
fn target_alias(target_uri: &str) -> String {
    let destination = target_uri.strip_prefix("ssh://").unwrap_or(target_uri);
    destination
        .rsplit('@')
        .next()
        .unwrap_or(destination)
        .to_owned()
}

#[derive(Clone)]
struct RemoteWake {
    sender: mpsc::Sender<RemoteUiEvent>,
    queued: Arc<AtomicBool>,
}

impl RemoteWake {
    fn new(sender: mpsc::Sender<RemoteUiEvent>) -> Self {
        Self {
            sender,
            queued: Arc::new(AtomicBool::new(false)),
        }
    }

    fn wake(&self) {
        if self
            .queued
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
            && self.sender.send(RemoteUiEvent::Wake).is_err()
        {
            self.queued.store(false, Ordering::Release);
        }
    }

    fn consumed(&self) {
        self.queued.store(false, Ordering::Release);
    }
}

fn start_terminal_reader(events: mpsc::Sender<RemoteUiEvent>) {
    std::thread::Builder::new()
        .name("unpeel-remote-input".to_owned())
        .spawn(move || loop {
            match event::read() {
                // Hover reports are lossy presentation data and can arrive
                // faster than a frame. The remote UI has no hover-only
                // affordance, so do not let them queue ahead of typing.
                Ok(Event::Mouse(mouse)) if matches!(mouse.kind, MouseEventKind::Moved) => {}
                Ok(event) => {
                    if events.send(RemoteUiEvent::Terminal(event)).is_err() {
                        return;
                    }
                }
                Err(_) => return,
            }
        })
        .expect("spawn remote terminal input reader");
}

fn start_bootstrap_refresh(
    backend: RemoteSessionBackend,
    stop: Arc<AtomicBool>,
    events: mpsc::Sender<RemoteUiEvent>,
) {
    std::thread::Builder::new()
        .name("unpeel-remote-bootstrap".to_owned())
        .spawn(move || {
            while !wait_for_stop(&stop, BOOTSTRAP_REFRESH) {
                if events.send(bootstrap_event(&backend)).is_err() {
                    return;
                }
            }
        })
        .expect("spawn remote Host refresh");
}

/// One bootstrap round trip, mapped exactly like the periodic refresh —
/// verb workers use this to force the sidebar current after an effect.
fn bootstrap_event(backend: &RemoteSessionBackend) -> RemoteUiEvent {
    match backend.bootstrap() {
        Ok(bootstrap) => match require_interactive_capabilities(&bootstrap.snapshot) {
            Ok(()) => RemoteUiEvent::Bootstrap(Ok(bootstrap.snapshot)),
            Err(error) => RemoteUiEvent::Fatal(error.to_string()),
        },
        Err(error) => RemoteUiEvent::Bootstrap(Err(error.to_string())),
    }
}

fn wait_for_stop(stop: &AtomicBool, duration: Duration) -> bool {
    let deadline = Instant::now() + duration;
    while Instant::now() < deadline {
        if stop.load(Ordering::Acquire) {
            return true;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    stop.load(Ordering::Acquire)
}

enum RemoteEffect {
    Write {
        session_id: String,
        data: String,
    },
    Resize {
        session_id: String,
        resize: RemoteDesktopResize,
    },
    MarkRead {
        session_id: String,
    },
    Shutdown {
        done: mpsc::Sender<()>,
    },
}

#[derive(Clone)]
struct RemoteEffectQueue {
    sender: mpsc::Sender<RemoteEffect>,
    halted: Arc<AtomicBool>,
    owned_fits: Arc<std::sync::Mutex<HashSet<String>>>,
}

impl RemoteEffectQueue {
    fn enqueue(&self, effect: RemoteEffect) -> Result<(), ()> {
        if self.halted.load(Ordering::Acquire) {
            return Err(());
        }
        self.sender.send(effect).map_err(|_| ())
    }

    fn is_halted(&self) -> bool {
        self.halted.load(Ordering::Acquire)
    }
}

fn start_effect_worker(
    backend: RemoteSessionBackend,
    effects: mpsc::Receiver<RemoteEffect>,
    events: mpsc::Sender<RemoteUiEvent>,
    halted: Arc<AtomicBool>,
    owned_fits: Arc<std::sync::Mutex<HashSet<String>>>,
) {
    std::thread::Builder::new()
        .name("unpeel-remote-effects".to_owned())
        .spawn(move || run_effect_worker(backend, effects, events, halted, owned_fits))
        .expect("spawn remote effect worker");
}

fn run_effect_worker(
    backend: RemoteSessionBackend,
    effects: mpsc::Receiver<RemoteEffect>,
    events: mpsc::Sender<RemoteUiEvent>,
    halted_state: Arc<AtomicBool>,
    owned_fits: Arc<std::sync::Mutex<HashSet<String>>>,
) {
    let mut queued = VecDeque::new();
    let mut halted = false;
    loop {
        let effect = match queued.pop_front() {
            Some(effect) => effect,
            None => match effects.recv() {
                Ok(effect) => effect,
                Err(_) => return,
            },
        };

        if let RemoteEffect::Shutdown { done } = effect {
            if !halted {
                let fitted: Vec<String> = owned_fits
                    .lock()
                    .map(|fits| fits.iter().cloned().collect())
                    .unwrap_or_default();
                for session_id in fitted {
                    match backend.resize_desktop(&session_id, RemoteDesktopResize::Clear) {
                        Ok(_) => {
                            if let Ok(mut fits) = owned_fits.lock() {
                                fits.remove(&session_id);
                            }
                        }
                        Err(failure)
                            if failure.kind() == RemoteEffectFailureKind::OutcomeUnknown =>
                        {
                            halted_state.store(true, Ordering::Release);
                            break;
                        }
                        Err(_) => {}
                    }
                }
            }
            let _ = done.send(());
            return;
        }

        if halted {
            continue;
        }

        let effect = batch_writes(effect, &effects, &mut queued);
        let fit_change = match &effect {
            RemoteEffect::Resize {
                session_id,
                resize: RemoteDesktopResize::Fit { .. },
            } => Some((session_id.clone(), true)),
            RemoteEffect::Resize {
                session_id,
                resize: RemoteDesktopResize::Clear,
            } => Some((session_id.clone(), false)),
            _ => None,
        };
        let outcome = match &effect {
            RemoteEffect::Write { session_id, data } => {
                backend.write_terminal(session_id, data).map(|_| ())
            }
            RemoteEffect::Resize { session_id, resize } => {
                backend.resize_desktop(session_id, *resize).map(|_| ())
            }
            RemoteEffect::MarkRead { session_id } => {
                backend.mark_session_read(session_id).map(|_| ())
            }
            RemoteEffect::Shutdown { .. } => unreachable!(),
        };

        if outcome.is_ok() {
            if let Some((session_id, fitted)) = fit_change {
                if let Ok(mut owned) = owned_fits.lock() {
                    if fitted {
                        owned.insert(session_id);
                    } else {
                        owned.remove(&session_id);
                    }
                }
            }
        } else if let Err(failure) = outcome {
            let must_halt = effect_failure_requires_halt(failure.kind());
            let message = if must_halt {
                format!(
                    "{failure}; input paused to preserve command order — quit and reconnect before sending more"
                )
            } else {
                failure.to_string()
            };
            if must_halt {
                halted_state.store(true, Ordering::Release);
            }
            let _ = events.send(RemoteUiEvent::Notice(message));
            halted = must_halt;
        }
    }
}

fn effect_failure_requires_halt(failure: RemoteEffectFailureKind) -> bool {
    failure == RemoteEffectFailureKind::OutcomeUnknown
}

fn batch_writes(
    first: RemoteEffect,
    effects: &mpsc::Receiver<RemoteEffect>,
    queued: &mut VecDeque<RemoteEffect>,
) -> RemoteEffect {
    let RemoteEffect::Write {
        session_id,
        mut data,
    } = first
    else {
        return first;
    };
    while let Ok(next) = effects.try_recv() {
        match next {
            RemoteEffect::Write {
                session_id: next_session,
                data: next_data,
            } if next_session == session_id
                && data.len().saturating_add(next_data.len())
                    <= REMOTE_TERMINAL_WRITE_MAX_BYTES =>
            {
                data.push_str(&next_data);
            }
            other => {
                queued.push_back(other);
                break;
            }
        }
    }
    RemoteEffect::Write { session_id, data }
}

#[allow(clippy::too_many_arguments)]
fn run_remote_loop(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut App,
    snapshots: &SnapshotService,
    backend: RemoteSessionBackend,
    effects: RemoteEffectQueue,
    mut controller: RemoteController,
    mut mark_read_supported: bool,
    output_wake: RemoteWake,
    events: mpsc::Receiver<RemoteUiEvent>,
) -> io::Result<()> {
    let wake_gate = output_wake.clone();
    let wake: Arc<dyn Fn() + Send + Sync> = Arc::new(move || {
        wake_gate.wake();
    });
    let mut stream: Option<RemoteLiveStream> = None;
    let mut active_session: Option<String> = None;
    let mut last_fit: Option<(String, u16, u16)> = None;
    let mut last_frame: Option<(String, u32, u16, u16, u64)> = None;
    let mut last_stream_error: Option<String> = None;
    let mut observed_selection: Option<String> = None;
    let mut bootstrap_notice: Option<String> = None;
    let mut stream_notice: Option<String> = None;
    let mut connection_notice: Option<String> = None;
    let mut halt_notice: Option<String> = None;
    let startup_mascot = crate::mascot::StartupMascot::new();

    loop {
        if effects.is_halted() {
            app.terminal_focus = false;
            let message = halt_notice.get_or_insert_with(|| {
                "A remote effect did not complete safely; input is paused until you quit and reconnect"
                    .to_owned()
            });
            app.info = Some(message.clone());
        }
        let size = terminal.size()?;
        crate::LAST_TERM_WIDTH.store(size.width, Ordering::Relaxed);
        app.clamp_scroll(size.height.saturating_sub(2) as usize);
        let selected = app
            .selected_session()
            .filter(|session| session.running)
            .map(|session| session.id.clone());
        let (columns, rows) = remote_preview_grid(app, size.width, size.height);

        if selected != active_session {
            if let Some(previous) = active_session.take() {
                let _ = effects.enqueue(RemoteEffect::Resize {
                    session_id: previous,
                    resize: RemoteDesktopResize::Clear,
                });
            }
            stream = None;
            last_fit = None;
            last_frame = None;
            last_stream_error = None;
            stream_notice = None;
            update_connection_notice(
                app,
                halt_notice.as_ref(),
                &bootstrap_notice,
                &stream_notice,
                &mut connection_notice,
            );
            active_session = selected.clone();
            if let Some(session_id) = selected.clone() {
                stream = Some(RemoteLiveStream::start(
                    backend.clone(),
                    session_id,
                    columns,
                    rows,
                    Arc::clone(&wake),
                ));
            }
            let _ = execute!(io::stdout(), BeginSynchronizedUpdate);
            // Ratatui 0.30's Terminal::clear snapshots the cursor with a
            // CPR query first. The SSH/controller PTY does not owe the TUI
            // that terminal reply, so clear and reset the fullscreen diff
            // buffers through resize instead.
            terminal.resize(size.into())?;
        }

        if app.selected_id != observed_selection {
            observed_selection = app.selected_id.clone();
            if let Some(session_id) = observed_selection.clone() {
                if mark_read_supported
                    && effects
                        .enqueue(RemoteEffect::MarkRead {
                            session_id: session_id.clone(),
                        })
                        .is_ok()
                {
                    app.unread_ids.remove(&session_id);
                }
            }
        }

        if let (Some(session_id), Some(stream)) = (selected.as_ref(), stream.as_ref()) {
            stream.resize(columns, rows);
            let fit = (session_id.clone(), columns, rows);
            if last_fit.as_ref() != Some(&fit) {
                let _ = effects.enqueue(RemoteEffect::Resize {
                    session_id: session_id.clone(),
                    resize: RemoteDesktopResize::Fit { columns, rows },
                });
                last_fit = Some(fit);
            }

            if stream.is_dirty()
                || last_frame
                    .as_ref()
                    .is_none_or(|(id, scroll, cols, frame_rows, _)| {
                        id != session_id
                            || *scroll != app.preview_scroll
                            || *cols != columns
                            || *frame_rows != rows
                    })
            {
                if let Some(snapshot) = stream.snapshot(app.preview_scroll) {
                    app.preview_scroll = app.preview_scroll.min(snapshot.scrollback_rows);
                    let key = (
                        session_id.clone(),
                        app.preview_scroll,
                        columns,
                        rows,
                        snapshot.output_offset,
                    );
                    snapshots.publish(session_id.clone(), snapshot);
                    last_frame = Some(key);
                }
            }

            let stream_error = stream.last_error();
            if stream_error != last_stream_error {
                last_stream_error = stream_error.clone();
                if let Some(error) = stream_error {
                    let message = format!("Host reconnecting: {error}");
                    stream_notice = Some(message);
                } else if stream.is_connected() {
                    stream_notice = None;
                }
                update_connection_notice(
                    app,
                    halt_notice.as_ref(),
                    &bootstrap_notice,
                    &stream_notice,
                    &mut connection_notice,
                );
            }
        }

        let _ = execute!(io::stdout(), BeginSynchronizedUpdate);
        let draw = terminal.draw(|frame| {
            ui::draw(frame, app, snapshots);
            startup_mascot.draw(frame);
        });
        let _ = execute!(io::stdout(), EndSynchronizedUpdate);
        draw?;

        let event = match events.recv_timeout(REMOTE_TICK) {
            Ok(event) => event,
            Err(mpsc::RecvTimeoutError::Timeout) => continue,
            Err(mpsc::RecvTimeoutError::Disconnected) => return Ok(()),
        };
        match event {
            RemoteUiEvent::Wake => output_wake.consumed(),
            RemoteUiEvent::Notice(message) => {
                if effects.is_halted() {
                    app.terminal_focus = false;
                    halt_notice = Some(message.clone());
                }
                app.info = Some(message);
            }
            RemoteUiEvent::Fatal(message) => {
                return Err(io::Error::new(io::ErrorKind::Unsupported, message));
            }
            RemoteUiEvent::Bootstrap(Ok(snapshot)) => {
                let next_mark_read_supported = snapshot.supports(REMOTE_CAPABILITY_MARK_READ);
                if next_mark_read_supported && !mark_read_supported {
                    observed_selection = None;
                }
                mark_read_supported = next_mark_read_supported;
                controller.presets = snapshot.presets.clone();
                controller.projects = snapshot.projects.clone();
                controller.sessions = snapshot.sessions.clone();
                apply_bootstrap(app, &snapshot, &mut controller.replacement_selection);
                if app.selected_archive.is_none() {
                    controller.archived_overlay = None;
                }
                merge_archived_overlay(app, controller.archived_overlay.as_ref());
                bootstrap_notice = None;
                update_connection_notice(
                    app,
                    halt_notice.as_ref(),
                    &bootstrap_notice,
                    &stream_notice,
                    &mut connection_notice,
                );
            }
            RemoteUiEvent::VerbDone {
                message,
                select,
                archive_prune,
                failed_replacement_source,
            } => {
                app.in_flight = None;
                app.info = Some(message);
                if select.is_some() {
                    controller.replacement_selection.clear();
                    app.pending_select = select;
                }
                if let Some(source_id) = failed_replacement_source {
                    if controller
                        .replacement_selection
                        .pending
                        .as_ref()
                        .is_some_and(|pending| pending.source_id == source_id)
                    {
                        controller.replacement_selection.clear();
                    }
                }
                if let Some(id) = archive_prune {
                    if let Some((_, sessions)) = controller.archived_overlay.as_mut() {
                        sessions.retain(|session| session.id != id);
                    }
                }
            }
            RemoteUiEvent::ArchivedSessions { project_id, result } => {
                app.in_flight = None;
                match result {
                    Ok(sessions) => {
                        controller.archived_overlay = Some((project_id.clone(), sessions));
                        merge_archived_overlay(app, controller.archived_overlay.as_ref());
                        app.archive_query.clear();
                        app.selected_worktree_folder = None;
                        app.selected_new_session = None;
                        app.selected_archive = Some((project_id, 0));
                    }
                    Err(error) => app.info = Some(error),
                }
            }
            RemoteUiEvent::Bootstrap(Err(error)) => {
                let message = format!("Host unavailable: {error}");
                bootstrap_notice = Some(message);
                update_connection_notice(
                    app,
                    halt_notice.as_ref(),
                    &bootstrap_notice,
                    &stream_notice,
                    &mut connection_notice,
                );
            }
            RemoteUiEvent::Terminal(Event::Key(key)) if key.kind != KeyEventKind::Release => {
                if !handle_remote_key(app, key, &effects, &mut controller, size.width, size.height)
                {
                    return Ok(());
                }
            }
            RemoteUiEvent::Terminal(Event::Paste(text)) => {
                if app.terminal_focus {
                    send_remote_input(app, &effects, text);
                }
            }
            RemoteUiEvent::Terminal(Event::Mouse(mouse)) => {
                handle_remote_mouse(
                    app,
                    mouse,
                    size.width,
                    size.height,
                    snapshots,
                    &effects,
                    mark_read_supported,
                    &mut controller.replacement_selection,
                );
            }
            RemoteUiEvent::Terminal(Event::Resize(_, _)) => {}
            RemoteUiEvent::Terminal(_) => {}
        }
    }
}

fn update_connection_notice(
    app: &mut App,
    halt_notice: Option<&String>,
    bootstrap_notice: &Option<String>,
    stream_notice: &Option<String>,
    displayed: &mut Option<String>,
) {
    if halt_notice.is_some() {
        return;
    }
    let next = bootstrap_notice
        .as_ref()
        .or(stream_notice.as_ref())
        .cloned();
    if next.is_some() || app.info.as_ref() == displayed.as_ref() {
        app.info = next.clone();
    }
    *displayed = next;
}

/// The remote key loop mirrors the local `handle_key` order exactly —
/// confirm, modals, terminal focus, archive library, then the sidebar
/// bindings — with every commit routed through `RemoteSessionBackend`
/// instead of local session ops. The modals themselves are the shared ones
/// rendered by `ui::draw`.
fn handle_remote_key(
    app: &mut App,
    key: KeyEvent,
    effects: &RemoteEffectQueue,
    controller: &mut RemoteController,
    term_w: u16,
    term_h: u16,
) -> bool {
    let halted = effects.is_halted();
    if halted {
        app.terminal_focus = false;
    }
    if let Some(confirm) = app.confirm.take() {
        if matches!(key.code, KeyCode::Char('y') | KeyCode::Char('Y')) {
            run_remote_verb(app, controller, confirm.verb, confirm.session_id);
        }
        return true;
    }
    if !halted {
        app.info = None;
    }
    if let Some(modal) = app.modal.take() {
        match modal {
            // Any key closes help, like local.
            Modal::Help => {}
            Modal::Rename(input) => {
                if let RenameKeyOutcome::Commit { session_id, title } =
                    handle_rename_key(app, input, key)
                {
                    run_remote_rename(app, controller, session_id, title);
                }
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
                    run_remote_create(app, controller, command);
                }
            }
            // No other modal can open in remote scope; a stray one closes.
            _ => {}
        }
        return true;
    }
    if app.terminal_focus {
        let control = key.modifiers.contains(KeyModifiers::CONTROL);
        if control && matches!(key.code, KeyCode::Char(']') | KeyCode::Char('5')) {
            app.terminal_focus = false;
            return true;
        }
        if let Some(data) = crate::keys::key_event_to_string(&key) {
            send_remote_input(app, effects, data);
        }
        return true;
    }
    let grid = remote_preview_grid(app, term_w, term_h);
    match handle_archive_key(app, key, grid) {
        ArchiveKeyOutcome::RestoreAndResume(id) => {
            let Some(pending) = pending_replacement_selection(app, controller, &id) else {
                app.info = Some("the archived session is no longer available".to_owned());
                return true;
            };
            app.selected_archive = None;
            app.selected_id = Some(id.clone());
            app.terminal_focus = false;
            app.terminal_selection = None;
            app.preview_scroll = 0;
            controller.replacement_selection.begin(pending.clone());
            run_remote_restore_and_resume(app, controller, id, pending);
            return true;
        }
        ArchiveKeyOutcome::Restore(id) => {
            app.selected_archive = None;
            run_remote_restore(app, controller, id);
            return true;
        }
        ArchiveKeyOutcome::Handled => return true,
        ArchiveKeyOutcome::NotHandled => {}
    }

    match key.code {
        KeyCode::Char('q') => return false,
        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => return false,
        KeyCode::Up | KeyCode::Char('k') => {
            controller.replacement_selection.clear();
            app.move_selection_silent(-1);
        }
        KeyCode::Down | KeyCode::Char('j') => {
            controller.replacement_selection.clear();
            app.move_selection_silent(1);
        }
        KeyCode::PageUp => scroll_remote_preview(app, true),
        KeyCode::PageDown => scroll_remote_preview(app, false),
        KeyCode::Char('-') => app.toggle_fold_all(),
        KeyCode::Char('n') => open_remote_preset_picker(app, controller),
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
            let verb = Verb::Pin(!pinned);
            if let Some(id) = gate_verb(app, verb, grid) {
                run_remote_verb(app, controller, verb, id);
            }
        }
        KeyCode::Char('s') => {
            if let Some(id) = gate_verb(app, Verb::Stop, grid) {
                run_remote_verb(app, controller, Verb::Stop, id);
            }
        }
        KeyCode::Char('r') => {
            let Some(verb) = selected_restart_verb(app) else {
                app.info = app
                    .selected_session()
                    .map(|session| resume_unavailable_message(session).to_owned());
                return true;
            };
            if let Some(id) = gate_verb(app, verb, grid) {
                run_remote_verb(app, controller, verb, id);
            }
        }
        KeyCode::Char('x') => {
            if let Some(id) = gate_verb(app, Verb::Remove, grid) {
                run_remote_verb(app, controller, Verb::Remove, id);
            }
        }
        KeyCode::Char('a') => open_remote_archive(app, controller),
        // The one intentionally missing verb: the protocol has no Host
        // settings operations yet.
        KeyCode::Char(',') => {
            app.info = Some("Host settings aren't editable over this connection yet".to_owned());
        }
        KeyCode::Char('?') => app.modal = Some(Modal::Help),
        KeyCode::Enter if app.selected_new_session.is_some() => {
            open_remote_preset_picker(app, controller)
        }
        KeyCode::Enter => match app.selected_worktree_folder.clone() {
            Some(worktree) => {
                if !app.expanded_worktrees.remove(&worktree) {
                    app.expanded_worktrees.insert(worktree);
                }
            }
            None => enter_remote_terminal_focus(app, halted),
        },
        _ => {}
    }
    true
}

/// Execute a sidebar verb against the Host on a worker thread. Effects are
/// never auto-retried: a `NotApplied` failure surfaces its message and stops;
/// `OutcomeUnknown` additionally forces a bootstrap so the sidebar shows what
/// actually happened before the user retries by hand.
fn run_remote_verb(
    app: &mut App,
    controller: &mut RemoteController,
    verb: Verb,
    session_id: String,
) {
    let pending_replacement = if matches!(verb, Verb::Resume) {
        let Some(pending) = pending_replacement_selection(app, controller, &session_id) else {
            app.info = Some("the stopped session is no longer available".to_owned());
            return;
        };
        controller.replacement_selection.begin(pending.clone());
        Some(pending)
    } else {
        None
    };
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
    let backend = controller.backend.clone();
    let events = controller.events.clone();
    std::thread::spawn(move || {
        let outcome = match verb {
            // `s` matches the local Stop verb exactly: stop AND file away
            // (the local loop routes `Verb::Stop` through archive too).
            Verb::Stop => backend
                .archive_session(&session_id)
                .map(|_| ("stopped and archived".to_owned(), None, None)),
            Verb::Resume => backend
                .restart_session(&session_id)
                .map(|_| ("resuming session".to_owned(), None, None)),
            Verb::ResumeAgent => backend
                .resume_agent(&session_id)
                .map(|_| ("resuming agent".to_owned(), None, None)),
            Verb::Remove => backend
                .remove_session(&session_id)
                .map(|_| ("removed".to_owned(), None, Some(session_id.clone()))),
            Verb::Pin(pinned) => backend.set_session_pinned(&session_id, pinned).map(|_| {
                (
                    if pinned { "pinned" } else { "unpinned" }.to_owned(),
                    None,
                    None,
                )
            }),
        };
        let failed_replacement_source = outcome
            .as_ref()
            .err()
            .filter(|failure| failure.kind() == RemoteEffectFailureKind::NotApplied)
            .and_then(|_| pending_replacement.map(|pending| pending.source_id));
        conclude_remote_effect(&backend, &events, outcome, failed_replacement_source);
    });
}

fn run_remote_rename(
    app: &mut App,
    controller: &RemoteController,
    session_id: String,
    title: String,
) {
    app.info = Some("…".into());
    let backend = controller.backend.clone();
    let events = controller.events.clone();
    std::thread::spawn(move || {
        let outcome = backend
            .set_session_title(&session_id, &title)
            .map(|_| ("renamed".to_owned(), None, None));
        conclude_remote_effect(&backend, &events, outcome, None);
    });
}

fn run_remote_restore(app: &mut App, controller: &RemoteController, session_id: String) {
    app.info = None;
    app.in_flight = Some(InFlight {
        label: "restoring".into(),
    });
    let backend = controller.backend.clone();
    let events = controller.events.clone();
    std::thread::spawn(move || {
        let outcome = backend.restore_session(&session_id).map(|_| {
            (
                "restored from archive".to_owned(),
                Some(session_id.clone()),
                Some(session_id.clone()),
            )
        });
        conclude_remote_effect(&backend, &events, outcome, None);
    });
}

/// The archive library's primary action for a resumable row: unfile it, then
/// ask the Host to recreate its stopped terminal with the saved resume recipe.
/// Each mutation remains an explicit, non-replayed effect; if the second one
/// fails the Session is safely left restored in the stopped list.
fn run_remote_restore_and_resume(
    app: &mut App,
    controller: &RemoteController,
    session_id: String,
    pending_replacement: PendingReplacementSelection,
) {
    app.info = None;
    app.in_flight = Some(InFlight {
        label: "restoring and resuming".into(),
    });
    let backend = controller.backend.clone();
    let events = controller.events.clone();
    std::thread::spawn(move || {
        let source_id = pending_replacement.source_id;
        let outcome = backend
            .restore_session(&session_id)
            .and_then(|_| backend.restart_session(&session_id))
            .map(|_| ("restored and resuming".to_owned(), None, Some(session_id)));
        let failed_replacement_source = outcome
            .as_ref()
            .err()
            .filter(|failure| failure.kind() == RemoteEffectFailureKind::NotApplied)
            .map(|_| source_id);
        conclude_remote_effect(&backend, &events, outcome, failed_replacement_source);
    });
}

/// Launch a preset picked in the shared picker as a Host-side create. The
/// footer "manage presets" row maps to the one intentionally missing verb.
fn run_remote_create(app: &mut App, controller: &mut RemoteController, command: String) {
    if command == crate::MANAGE_PRESETS_COMMAND {
        app.pending_spawn_target = None;
        app.info = Some("Host settings aren't editable over this connection yet".to_owned());
        return;
    }
    controller.replacement_selection.clear();
    let explicit = app.pending_spawn_target.take().map(|(id, _)| id);
    let Some(project_id) =
        explicit.or_else(|| app.selected_session().map(|s| s.project_id.clone()))
    else {
        app.info = Some("select a project first".into());
        return;
    };
    let request = match controller.picker_preset_ids.get(&command) {
        Some(preset_id) => RemoteSessionCreateRequest::from_preset(project_id, preset_id.clone()),
        None => RemoteSessionCreateRequest::from_command(project_id, command),
    };
    app.info = None;
    app.in_flight = Some(InFlight {
        label: "starting session".into(),
    });
    let backend = controller.backend.clone();
    let events = controller.events.clone();
    std::thread::spawn(move || {
        let outcome = backend.create_session(&request).map(|created| {
            (
                "new session started".to_owned(),
                Some(created.session_id),
                None,
            )
        });
        conclude_remote_effect(&backend, &events, outcome, None);
    });
}

/// Common verb-thread tail: publish the outcome toast (the
/// `RemoteEffectFailure` Display text verbatim on failure) and refresh the
/// bootstrap after success or an ambiguous outcome. Never retries.
fn conclude_remote_effect(
    backend: &RemoteSessionBackend,
    events: &mpsc::Sender<RemoteUiEvent>,
    outcome: Result<(String, Option<String>, Option<String>), RemoteEffectFailure>,
    failed_replacement_source: Option<String>,
) {
    let (message, select, archive_prune, refresh) = match outcome {
        Ok((message, select, archive_prune)) => (message, select, archive_prune, true),
        Err(failure) => {
            let refresh = failure.kind() == RemoteEffectFailureKind::OutcomeUnknown;
            (failure.to_string(), None, None, refresh)
        }
    };
    let _ = events.send(RemoteUiEvent::VerbDone {
        message,
        select,
        archive_prune,
        failed_replacement_source,
    });
    if refresh {
        let _ = events.send(bootstrap_event(backend));
    }
}

/// `n`: the shared preset picker, fed from the Host bootstrap's preset
/// catalog instead of local `app-state.json`.
fn open_remote_preset_picker(app: &mut App, controller: &mut RemoteController) {
    let target_project = app
        .selected_new_session
        .clone()
        .or_else(|| app.selected_project_id());
    let Some(project_id) = target_project else {
        app.info = Some("no presets found".into());
        return;
    };
    controller.picker_preset_ids.clear();
    let mut presets: Vec<(String, String)> = vec![("Terminal".into(), String::new())];
    for preset in controller.presets.iter().filter(|preset| preset.enabled) {
        presets.push((preset.label.clone(), preset.command.clone()));
        controller
            .picker_preset_ids
            .entry(preset.command.clone())
            .or_insert_with(|| preset.id.clone());
    }
    presets.push((
        "manage presets".into(),
        crate::MANAGE_PRESETS_COMMAND.into(),
    ));

    let summary = controller
        .projects
        .iter()
        .find(|project| project.id == project_id);
    let name = summary.map(|p| p.name.clone()).unwrap_or_default();
    // Host paths are display-only here — never probed on the Controller.
    let path = summary.map(|p| p.path.clone()).unwrap_or_default();
    let folder_kind = app.model.items.iter().find_map(|item| match item {
        SidebarItem::WorktreeHeader {
            project_id: p,
            is_group,
            ..
        } if *p == project_id => Some(if *is_group { "group" } else { "worktree" }),
        _ => None,
    });
    // Reveal the destination, exactly like the local picker.
    if folder_kind.is_some() {
        app.expanded_worktrees.insert(project_id.clone());
    } else {
        app.collapsed.remove(&name);
    }
    let project_label = match folder_kind {
        Some(kind) => format!("{kind} {name}"),
        None => name,
    };
    let target = if project_label.is_empty() {
        path
    } else if path.is_empty() {
        project_label
    } else {
        format!("{project_label} · {path}")
    };
    app.pending_spawn_target = Some((project_id, String::new()));
    app.modal = Some(Modal::PresetPicker {
        presets,
        selected: 0,
        target,
        anchor: None,
    });
}

/// `a`: fetch the project's archive from the Host, then open the shared
/// archive library over it.
fn open_remote_archive(app: &mut App, controller: &RemoteController) {
    let Some(project_id) = app.selected_project_id() else {
        return;
    };
    if app.archived_count_in_project(&project_id) == 0 {
        app.info = Some("no archived sessions in this project".into());
        return;
    }
    app.info = None;
    app.in_flight = Some(InFlight {
        label: "loading archive".into(),
    });
    let backend = controller.backend.clone();
    let events = controller.events.clone();
    std::thread::spawn(move || {
        let result = backend
            .list_archived_sessions(&project_id)
            .map_err(|error| error.to_string());
        let _ = events.send(RemoteUiEvent::ArchivedSessions { project_id, result });
    });
}

/// Keep the fetched archive visible across bootstrap refreshes: append any
/// archived Session the rebuilt model does not already carry. Append-only —
/// `items` indexes `rows`, so rows are never removed here.
fn merge_archived_overlay(app: &mut App, overlay: Option<&(String, Vec<RemoteSessionSummary>)>) {
    let Some((_, sessions)) = overlay else {
        return;
    };
    for summary in sessions {
        if !app.model.rows.iter().any(|row| row.id == summary.id) {
            app.model.rows.push(session_row(summary));
        }
    }
}

fn send_remote_input(app: &mut App, effects: &RemoteEffectQueue, data: String) {
    if effects.is_halted() {
        app.info =
            Some("Remote input is paused — quit and reconnect before sending more".to_owned());
        app.terminal_focus = false;
        return;
    }
    let Some(session_id) = app.selected_session().map(|session| session.id.clone()) else {
        return;
    };
    let mut start = 0;
    while start < data.len() {
        let mut end = (start + REMOTE_TERMINAL_WRITE_MAX_BYTES).min(data.len());
        while end > start && !data.is_char_boundary(end) {
            end -= 1;
        }
        if end == start
            || effects
                .enqueue(RemoteEffect::Write {
                    session_id: session_id.clone(),
                    data: data[start..end].to_owned(),
                })
                .is_err()
        {
            app.info = Some("remote input worker stopped".to_owned());
            app.terminal_focus = false;
            return;
        }
        start = end;
    }
}

fn select_remote_session(app: &mut App, session_id: String) {
    let Some(index) = app
        .model
        .rows
        .iter()
        .position(|session| session.id == session_id)
    else {
        return;
    };
    // Always clear alternate views, even when this id was already selected
    // underneath one. Otherwise input could focus a hidden terminal while
    // Archive or a folder/action row still owned the screen.
    app.select_item_silent(&SidebarItem::Session(index));
    app.terminal_focus = false;
    app.unread_ids.remove(&session_id);
}

fn enter_remote_terminal_focus(app: &mut App, halted: bool) {
    if halted {
        app.info =
            Some("Remote input is paused — quit and reconnect before driving a Session".to_owned());
        return;
    }
    if let Some(session) = app.selected_session() {
        if session.running {
            app.preview_scroll = 0;
            app.terminal_focus = true;
        } else {
            app.info = Some("session is stopped — press r to resume".into());
        }
    }
}

fn scroll_remote_preview(app: &mut App, up: bool) {
    app.preview_scroll = if up {
        app.preview_scroll.saturating_add(3)
    } else {
        app.preview_scroll.saturating_sub(3)
    };
}

// Keep the remote input authorities explicit at this dispatch boundary.
#[allow(clippy::too_many_arguments)]
fn handle_remote_mouse(
    app: &mut App,
    mouse: MouseEvent,
    term_width: u16,
    term_height: u16,
    snapshots: &SnapshotService,
    effects: &RemoteEffectQueue,
    mark_read_supported: bool,
    replacement_selection: &mut RemoteReplacementSelectionState,
) {
    // Modals and confirms are keyboard-driven in remote scope; a click
    // outside simply closes the floating layer instead of acting through it.
    if app.confirm.is_some() {
        return;
    }
    if app.modal.is_some() {
        if matches!(mouse.kind, MouseEventKind::Down(_)) {
            app.modal = None;
            app.pending_spawn_target = None;
        }
        return;
    }
    let divider = app.sidebar_width.saturating_sub(1);
    let terminal_hit = remote_terminal_target(app, term_width, term_height, snapshots)
        .is_some_and(|target| mouse_in_rect(&mouse, target));
    if effects.is_halted() {
        app.terminal_focus = false;
    }
    match mouse.kind {
        MouseEventKind::Down(MouseButton::Left) if mouse.column == divider => {
            app.dragging_divider = true;
        }
        MouseEventKind::Drag(MouseButton::Left) if app.dragging_divider => {
            let max_width = ui::MAX_SIDEBAR_WIDTH
                .min(term_width.saturating_sub(10))
                .max(ui::MIN_SIDEBAR_WIDTH);
            app.sidebar_width = mouse.column.clamp(ui::MIN_SIDEBAR_WIDTH, max_width);
        }
        MouseEventKind::Up(MouseButton::Left) if app.dragging_divider => {
            app.dragging_divider = false;
        }
        MouseEventKind::ScrollUp if mouse.column < divider => {
            app.scroll_sidebar(-3);
        }
        MouseEventKind::ScrollDown if mouse.column < divider => {
            app.scroll_sidebar(3);
        }
        MouseEventKind::ScrollUp if terminal_hit => remote_preview_wheel(
            app,
            true,
            mouse,
            term_width,
            term_height,
            snapshots,
            effects,
        ),
        MouseEventKind::ScrollDown if terminal_hit => remote_preview_wheel(
            app,
            false,
            mouse,
            term_width,
            term_height,
            snapshots,
            effects,
        ),
        // The sidebar's bottom-right fold-all toggle works in remote scope
        // too — collapse state is plain local UI state.
        MouseEventKind::Down(MouseButton::Left)
            if mouse.column < divider
                && mouse.row == term_height.saturating_sub(1)
                && ui::fold_label_hit(mouse.column, divider) =>
        {
            app.toggle_fold_all();
        }
        MouseEventKind::Down(MouseButton::Left) if mouse.column < divider && mouse.row >= 1 => {
            app.terminal_focus = false;
            let position = app.sidebar_scroll + (mouse.row - 1) as usize;
            let item = app
                .visible_items()
                .get(position)
                .map(|item| (*item).clone());
            match item {
                Some(SidebarItem::Session(index)) => {
                    replacement_selection.clear();
                    let id = app.model.rows[index].id.clone();
                    let label = app.model.rows[index].label.clone();
                    let reobserved = app.selected_id.as_deref() == Some(id.as_str());
                    let now = Instant::now();
                    // Second click on the selected row within the local
                    // double-click window opens the shared rename dialog.
                    let double = app.last_click.as_ref().is_some_and(|(last, at)| {
                        *last == id && now.duration_since(*at) < crate::DOUBLE_CLICK
                    });
                    app.last_click = Some((id.clone(), now));
                    if double {
                        app.modal = Some(Modal::Rename(RenameInput::new(id, label)));
                    } else {
                        select_remote_session(app, id.clone());
                        if reobserved && mark_read_supported {
                            let _ = effects.enqueue(RemoteEffect::MarkRead { session_id: id });
                        }
                        // Keep remote scope behavior identical to local: a
                        // running Session is ready for input on the click
                        // that opens it.
                        enter_remote_terminal_focus(app, effects.is_halted());
                    }
                }
                Some(item @ SidebarItem::NewSession { .. }) => {
                    replacement_selection.clear();
                    app.select_item_silent(&item);
                }
                Some(SidebarItem::Header(name)) => {
                    if !app.collapsed.remove(&name) {
                        app.collapsed.insert(name);
                    }
                }
                Some(SidebarItem::WorktreeHeader { project_id, .. }) => {
                    if !app.expanded_worktrees.remove(&project_id) {
                        app.expanded_worktrees.insert(project_id);
                    }
                }
                _ => {}
            }
        }
        MouseEventKind::Down(_) if terminal_hit && !effects.is_halted() => {
            if app
                .selected_session()
                .is_some_and(|session| session.running)
            {
                app.terminal_focus = true;
                forward_remote_mouse(app, &mouse, term_width, term_height, snapshots, effects);
            }
        }
        MouseEventKind::Down(_) | MouseEventKind::Up(_) | MouseEventKind::Drag(_)
            if terminal_hit && app.terminal_focus =>
        {
            forward_remote_mouse(app, &mouse, term_width, term_height, snapshots, effects);
        }
        _ => {}
    }
}

fn remote_preview_wheel(
    app: &mut App,
    up: bool,
    mouse: MouseEvent,
    term_width: u16,
    term_height: u16,
    snapshots: &SnapshotService,
    effects: &RemoteEffectQueue,
) {
    let Some(session) = app.selected_session() else {
        return;
    };
    let Some(snapshot) = snapshots.get(&session.id) else {
        scroll_remote_preview(app, up);
        return;
    };
    let target =
        ui::preview_terminal_rect(crate::preview_area(app, term_width, term_height), &snapshot);
    if !mouse_in_rect(&mouse, target) {
        return;
    }
    if snapshot.mouse_reporting && app.terminal_focus {
        let button = if up { 64 } else { 65 };
        let (column, row) = mouse_position(&mouse, target);
        send_remote_input(app, effects, format!("\x1b[<{button};{column};{row}M"));
    } else if snapshot.alternate_screen && snapshot.mouse_alternate_scroll {
        let sequence = match (up, snapshot.application_cursor) {
            (true, true) => "\x1bOA",
            (false, true) => "\x1bOB",
            (true, false) => "\x1b[A",
            (false, false) => "\x1b[B",
        };
        send_remote_input(app, effects, sequence.to_owned());
    } else if !snapshot.alternate_screen {
        scroll_remote_preview(app, up);
        app.preview_scroll = app.preview_scroll.min(snapshot.scrollback_rows);
    }
}

fn forward_remote_mouse(
    app: &mut App,
    mouse: &MouseEvent,
    term_width: u16,
    term_height: u16,
    snapshots: &SnapshotService,
    effects: &RemoteEffectQueue,
) -> bool {
    let Some(session) = app.selected_session() else {
        return false;
    };
    let Some(snapshot) = snapshots.get(&session.id) else {
        return false;
    };
    if !snapshot.mouse_reporting {
        return false;
    }
    let target =
        ui::preview_terminal_rect(crate::preview_area(app, term_width, term_height), &snapshot);
    if !mouse_in_rect(mouse, target) {
        return false;
    }
    if matches!(mouse.kind, MouseEventKind::Drag(_)) && !snapshot.mouse_button_motion {
        return false;
    }
    let button = match mouse.kind {
        MouseEventKind::Down(MouseButton::Left) | MouseEventKind::Up(MouseButton::Left) => 0,
        MouseEventKind::Down(MouseButton::Middle) | MouseEventKind::Up(MouseButton::Middle) => 1,
        MouseEventKind::Down(MouseButton::Right) | MouseEventKind::Up(MouseButton::Right) => 2,
        MouseEventKind::Drag(MouseButton::Left) => 32,
        MouseEventKind::Drag(MouseButton::Middle) => 33,
        MouseEventKind::Drag(MouseButton::Right) => 34,
        _ => return false,
    };
    let modifier = (if mouse.modifiers.contains(KeyModifiers::SHIFT) {
        4
    } else {
        0
    }) + (if mouse.modifiers.contains(KeyModifiers::ALT) {
        8
    } else {
        0
    }) + (if mouse.modifiers.contains(KeyModifiers::CONTROL) {
        16
    } else {
        0
    });
    let (column, row) = mouse_position(mouse, target);
    let suffix = if matches!(mouse.kind, MouseEventKind::Up(_)) {
        'm'
    } else {
        'M'
    };
    send_remote_input(
        app,
        effects,
        format!("\x1b[<{};{column};{row}{suffix}", button + modifier),
    );
    true
}

fn remote_terminal_target(
    app: &App,
    term_width: u16,
    term_height: u16,
    snapshots: &SnapshotService,
) -> Option<ratatui::layout::Rect> {
    let session = app.selected_session()?;
    let snapshot = snapshots.get(&session.id)?;
    Some(ui::preview_terminal_rect(
        crate::preview_area(app, term_width, term_height),
        &snapshot,
    ))
}

fn mouse_in_rect(mouse: &MouseEvent, rect: ratatui::layout::Rect) -> bool {
    mouse.column >= rect.x
        && mouse.column < rect.x.saturating_add(rect.width)
        && mouse.row >= rect.y
        && mouse.row < rect.y.saturating_add(rect.height)
}

fn remote_preview_grid(app: &App, term_width: u16, term_height: u16) -> (u16, u16) {
    let (columns, rows) = crate::preview_grid(app, term_width, term_height);
    (
        columns.clamp(
            REMOTE_DESKTOP_RESIZE_MIN_COLUMNS,
            REMOTE_DESKTOP_RESIZE_MAX_COLUMNS,
        ),
        rows.clamp(
            REMOTE_DESKTOP_RESIZE_MIN_ROWS,
            REMOTE_DESKTOP_RESIZE_MAX_ROWS,
        ),
    )
}

fn mouse_position(mouse: &MouseEvent, target: ratatui::layout::Rect) -> (u16, u16) {
    let column = mouse
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
    (column, row)
}

fn remote_app(model: SidebarModel, host_label: &str) -> App {
    let selected_id = first_visible_session(&model);
    let unread_ids = model
        .rows
        .iter()
        .filter(|row| row.unread)
        .map(|row| row.id.clone())
        .collect();
    let expanded_worktrees = model
        .items
        .iter()
        .filter_map(|item| match item {
            SidebarItem::WorktreeHeader { project_id, .. } => Some(project_id.clone()),
            _ => None,
        })
        .collect();
    // App lives for the rest of this process, so the scope label can use the
    // existing static feed-note slot without introducing a second App model.
    // ui.rs strips the "remote:" prefix and renders the Host's name green on
    // the sidebar's bottom edge — the one visible difference from Local.
    let scope_label: &'static str = Box::leak(format!("remote:{host_label}").into_boxed_str());
    App {
        model,
        engine: ActivityEngine::default(),
        scan_cache: ScanCache::default(),
        selected_id,
        selected_archive: None,
        selected_recent: None,
        // Native hides Recent outside Local until activity history becomes a
        // Host protocol capability. Keep this store empty: remote scope must
        // never read the Controller's local activity-log.jsonl.
        activity_log: unpeel_core::activity_log::ActivityLogStore::default(),
        deferred_stop_effects: HashMap::new(),
        model_runtime_generations: HashMap::new(),
        pending_spawn_target: None,
        last_selection_key: String::new(),
        selected_new_session: None,
        selected_add_project: false,
        dragging_project: None,
        dragging_folder: None,
        archive_query: String::new(),
        modal: None,
        exit_requested: false,
        selected_worktree_folder: None,
        expanded_worktrees,
        sidebar_scroll: 0,
        sidebar_width: REMOTE_SIDEBAR_WIDTH,
        mouse_pos: None,
        dragging_divider: false,
        collapsed: HashSet::new(),
        confirm: None,
        info: None,
        local_url_verdicts: Default::default(),
        local_url_checks_in_flight: Default::default(),
        announced_local_urls: HashSet::new(),
        update_available: None,
        env_hint: None,
        hook_port: None,
        sidebar_feed: Arc::new(std::sync::Mutex::new(None)),
        feed_note: scope_label,
        bridge_mode: false,
        bridge_mobile_endpoint_handoff: false,
        legacy_bridge_mode: false,
        bridge_unresolved: false,
        preview_scroll: 0,
        terminal_focus: false,
        terminal_selection: None,
        input: control::InteractiveInput::new(),
        local_unread: HashSet::new(),
        unread_ids,
        overlay: None::<overlay::NativeOverlay>,
        overlay_loaded_at: None,
        pending_select: None,
        replacement_selection: crate::LocalReplacementSelectionState::default(),
        mobile_snapshot: Arc::new(std::sync::Mutex::new(MobileSnapshot::default())),
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
        mobile_resizes: Arc::new(std::sync::Mutex::new(HashMap::new())),
        approvals: Arc::new(approvals::ApprovalHub::default()),
        pairing: Arc::new(pairing::PairingWindow::default()),
        settings: None,
        preset_add: String::new(),
        link_input: String::new(),
        selection_mode: false,
        dragging_row: None,
        last_click: None,
        in_flight: None,
        // Remote scope never sweeps: sessions live on the Host, and the
        // local auto-stop-and-archive policy must not touch them.
        idle_since_ms: std::collections::HashMap::new(),
        auto_archive_issued: HashSet::new(),
        auto_archive_retry_after_ms: std::collections::HashMap::new(),
    }
}

fn apply_bootstrap(
    app: &mut App,
    snapshot: &RemoteBootstrapSnapshot,
    replacement_selection: &mut RemoteReplacementSelectionState,
) {
    let previous_selection = app.selected_id.clone();
    app.model = model_from_bootstrap(snapshot);
    app.unread_ids = app
        .model
        .rows
        .iter()
        .filter(|row| row.unread)
        .map(|row| row.id.clone())
        .collect();
    for item in &app.model.items {
        if let SidebarItem::WorktreeHeader { project_id, .. } = item {
            app.expanded_worktrees.insert(project_id.clone());
        }
    }
    // Replacement Resume has no new id in its receipt. Resolve the bounded
    // pre-effect intent against successful bootstraps; ambiguity and expiry
    // permanently cancel auto-adoption while keeping default selection
    // suppressed until an explicit user choice.
    let replacement = replacement_selection.pending.take().and_then(|pending| {
        match replacement_selection_resolution(&pending, snapshot) {
            ReplacementSelectionResolution::Wait(updated) => {
                replacement_selection.pending = Some(updated);
                None
            }
            ReplacementSelectionResolution::Select(id) => {
                replacement_selection.suppress_default = false;
                Some(id)
            }
            ReplacementSelectionResolution::Cancel => None,
        }
    });

    // A verb that returned an exact id (create/plain restore) selects it as
    // soon as the Host's sidebar lists it — same contract as the local loop's
    // `pending_select`.
    let pending = app
        .pending_select
        .clone()
        .filter(|id| model_lists_session(&app.model, id));
    if pending.is_some() {
        replacement_selection.clear();
        app.pending_select = None;
    }
    let next_selection = pending
        .or(replacement)
        .or_else(|| {
            previous_selection
                .clone()
                .filter(|id| model_lists_session(&app.model, id))
        })
        .or_else(|| {
            if replacement_selection.pending.is_some() || replacement_selection.suppress_default {
                None
            } else {
                first_visible_session(&app.model)
            }
        });
    let next_is_running = next_selection.as_ref().is_some_and(|id| {
        app.model
            .rows
            .iter()
            .any(|row| row.id == *id && row.running)
    });
    if next_selection != previous_selection || !next_is_running {
        app.terminal_focus = false;
        app.terminal_selection = None;
        app.preview_scroll = 0;
    }
    app.selected_id = next_selection;
}

fn pending_replacement_selection(
    app: &App,
    controller: &RemoteController,
    source_id: &str,
) -> Option<PendingReplacementSelection> {
    let source = controller
        .sessions
        .iter()
        .find(|session| session.id == source_id)
        .or_else(|| {
            controller
                .archived_overlay
                .as_ref()?
                .1
                .iter()
                .find(|session| session.id == source_id)
        })?;
    let baseline_session_ids = controller
        .sessions
        .iter()
        .map(|session| session.id.clone())
        .chain(app.model.rows.iter().map(|session| session.id.clone()))
        .collect();
    Some(replacement_selection_intent(source, baseline_session_ids))
}

fn replacement_selection_intent(
    source: &RemoteSessionSummary,
    baseline_session_ids: HashSet<String>,
) -> PendingReplacementSelection {
    PendingReplacementSelection {
        source_id: source.id.clone(),
        project_id: source.project_id.clone(),
        created_at_unix_ms: source.created_at_unix_ms,
        runtime_id: remote_runtime_id(source),
        worktree_path: source.worktree_path.clone(),
        worktree_branch: source.worktree_branch.clone(),
        baseline_session_ids,
        bootstrap_observations_remaining: REPLACEMENT_BOOTSTRAP_OBSERVATIONS,
    }
}

fn remote_runtime_id(session: &RemoteSessionSummary) -> Option<String> {
    session
        .provider_id
        .clone()
        .or_else(|| crate::runtime_presentation::legacy_slug(&session.command).map(str::to_owned))
}

fn replacement_selection_resolution(
    pending: &PendingReplacementSelection,
    snapshot: &RemoteBootstrapSnapshot,
) -> ReplacementSelectionResolution {
    let source_still_exists = snapshot
        .sessions
        .iter()
        .any(|session| session.id == pending.source_id);
    let candidates: Vec<&RemoteSessionSummary> = snapshot
        .sessions
        .iter()
        .filter(|session| {
            session.status == RemoteSessionStatus::Running
                && !session.archived
                && session.project_id == pending.project_id
                && session.created_at_unix_ms == pending.created_at_unix_ms
                && session.worktree_path == pending.worktree_path
                && session.worktree_branch == pending.worktree_branch
                && !pending.baseline_session_ids.contains(&session.id)
                && pending.runtime_id.as_ref().is_none_or(|runtime_id| {
                    remote_runtime_id(session).as_ref() == Some(runtime_id)
                })
        })
        .collect();
    if candidates.len() > 1 {
        return ReplacementSelectionResolution::Cancel;
    }
    if !source_still_exists {
        if let Some(candidate) = candidates.first() {
            return ReplacementSelectionResolution::Select(candidate.id.clone());
        }
    }
    if pending.bootstrap_observations_remaining <= 1 {
        return ReplacementSelectionResolution::Cancel;
    }
    let mut waiting = pending.clone();
    waiting.bootstrap_observations_remaining -= 1;
    ReplacementSelectionResolution::Wait(waiting)
}

fn first_visible_session(model: &SidebarModel) -> Option<String> {
    model.items.iter().find_map(|item| match item {
        SidebarItem::Session(index) => Some(model.rows[*index].id.clone()),
        _ => None,
    })
}

fn model_lists_session(model: &SidebarModel, session_id: &str) -> bool {
    model.items.iter().any(|item| match item {
        SidebarItem::Session(index) => model.rows[*index].id == session_id,
        _ => false,
    })
}

pub(crate) fn model_from_bootstrap(snapshot: &RemoteBootstrapSnapshot) -> SidebarModel {
    let mut projects: Vec<(usize, &RemoteProjectSummary)> =
        snapshot.projects.iter().enumerate().collect();
    projects.sort_by(|(left_index, left), (right_index, right)| {
        left.sort_order
            .unwrap_or(*left_index as i64)
            .cmp(&right.sort_order.unwrap_or(*right_index as i64))
            .then_with(|| left_index.cmp(right_index))
    });
    let known: HashSet<&str> = projects
        .iter()
        .map(|(_, project)| project.id.as_str())
        .collect();
    let mut rows = Vec::with_capacity(snapshot.sessions.len());
    for session in &snapshot.sessions {
        let mut row = session_row(session);
        row.resume_available &= snapshot.supports(REMOTE_CAPABILITY_RESTART);
        row.resume_agent_available &= snapshot.supports(REMOTE_CAPABILITY_RESUME_AGENT);
        rows.push(row);
    }

    let mut items = Vec::new();
    let mut archived_counts = HashMap::new();
    let top_level: Vec<&RemoteProjectSummary> = projects
        .iter()
        .filter_map(|(_, project)| {
            project
                .parent_project_id
                .as_deref()
                .filter(|parent| known.contains(parent))
                .is_none()
                .then_some(*project)
        })
        .collect();
    for project in top_level {
        items.push(SidebarItem::Header(project.name.clone()));
        // Same rule as the local sidebar: only an empty project gets the
        // "+ New session" row (create is a supported Host verb).
        let own_count = rows
            .iter()
            .filter(|row| row.group_id == project.id && row_listed(row))
            .count();
        if own_count == 0 {
            items.push(SidebarItem::NewSession {
                project: project.id.clone(),
                name: project.name.clone(),
            });
        }
        for (_, child) in projects
            .iter()
            .copied()
            .filter(|(_, child)| child.parent_project_id.as_deref() == Some(project.id.as_str()))
        {
            let child_count = rows
                .iter()
                .filter(|row| row.group_id == child.id && row_listed(row))
                .count();
            items.push(SidebarItem::WorktreeHeader {
                project_id: child.id.clone(),
                parent: project.id.clone(),
                name: child.name.clone(),
                branch: child
                    .worktree_branch
                    .clone()
                    .or_else(|| child.git_branch.clone())
                    .unwrap_or_default(),
                count: child_count,
                is_group: child.is_group.unwrap_or(false),
            });
            if child_count == 0 {
                items.push(SidebarItem::NewSession {
                    project: child.id.clone(),
                    name: child.name.clone(),
                });
            }
            push_group_sessions(&child.id, &rows, &mut items);
            insert_archive_count(child, &rows, &mut archived_counts);
        }
        push_group_sessions(&project.id, &rows, &mut items);
        insert_archive_count(project, &rows, &mut archived_counts);
    }

    let rendered: HashSet<usize> = items
        .iter()
        .filter_map(|item| match item {
            SidebarItem::Session(index) => Some(*index),
            _ => None,
        })
        .collect();
    let orphan_indices: Vec<usize> = rows
        .iter()
        .enumerate()
        .filter(|(index, row)| row_listed(row) && !rendered.contains(index))
        .map(|(index, _)| index)
        .collect();
    if !orphan_indices.is_empty() {
        items.push(SidebarItem::Header("Other".to_owned()));
        items.extend(orphan_indices.into_iter().map(SidebarItem::Session));
    }

    SidebarModel {
        rows,
        items,
        archived_counts,
    }
}

/// Whether a session renders in the sidebar list — pins win over archive,
/// same doctrine as the local builder.
fn row_listed(row: &SessionRow) -> bool {
    !row.archived || row.pinned
}

fn push_group_sessions(group_id: &str, rows: &[SessionRow], items: &mut Vec<SidebarItem>) {
    let mut indices: Vec<usize> = rows
        .iter()
        .enumerate()
        .filter(|(_, row)| row.group_id == group_id && row_listed(row))
        .map(|(index, _)| index)
        .collect();
    indices.sort_by_key(|index| !rows[*index].pinned);
    items.extend(indices.into_iter().map(SidebarItem::Session));
}

fn insert_archive_count(
    project: &RemoteProjectSummary,
    rows: &[SessionRow],
    archived_counts: &mut HashMap<String, usize>,
) {
    let actual = rows
        .iter()
        .filter(|row| row.group_id == project.id && row.archived)
        .count();
    let advertised = project.archived_session_count.unwrap_or(0) as usize;
    let count = actual.max(advertised);
    if count > 0 {
        archived_counts.insert(project.id.clone(), count);
    }
}

/// One Host session summary as a shared sidebar row.
fn session_row(session: &RemoteSessionSummary) -> SessionRow {
    let running = session.status == RemoteSessionStatus::Running;
    let resume_available = !running
        && session.capabilities.as_ref().map_or_else(
            || {
                session.command.trim().is_empty()
                    || unpeel_core::resume::can_resume(&session.command)
            },
            |capabilities| capabilities.restart,
        );
    let resume_agent_available = running
        && !session.runtime_launch_pending
        && session.activity != RemoteActivityState::Starting
        && session
            .capabilities
            .as_ref()
            .is_some_and(|capabilities| capabilities.resume_agent);
    SessionRow {
        id: session.id.clone(),
        project_id: session.project_id.clone(),
        label: session.title.clone(),
        command: session.command.clone(),
        active_runtime_id: session.active_runtime_id.clone(),
        resume_available,
        resume_agent_available,
        running,
        status: remote_status(session.status, session.activity),
        created_at: nonnegative_millis(session.created_at_unix_ms),
        pinned: session.pinned,
        archived: session.archived,
        unread: session.unread,
        // A Host path must never be inspected as a path on the
        // Controller (ui.rs otherwise probes `.git/HEAD`).
        cwd: String::new(),
        activity_at: nonnegative_millis(
            session
                .updated_at_unix_ms
                .unwrap_or(session.created_at_unix_ms),
        ),
        group_id: session.project_id.clone(),
        // A remote Host's loopback is unreachable from this Controller;
        // never offer its local sites here.
        detected_local_urls: Vec::new(),
    }
}

fn remote_status(status: RemoteSessionStatus, activity: RemoteActivityState) -> Status {
    if status == RemoteSessionStatus::Exited {
        return Status::Exited;
    }
    match activity {
        RemoteActivityState::Starting => Status::Starting,
        RemoteActivityState::Working => Status::Busy,
        RemoteActivityState::Blocked => Status::Attention,
        RemoteActivityState::Done | RemoteActivityState::Idle | RemoteActivityState::Unknown => {
            Status::Idle
        }
    }
}

fn nonnegative_millis(value: i64) -> u64 {
    value.max(0) as u64
}

#[cfg(test)]
mod tests {
    use super::*;
    use unpeel_core::controller_protocol::HostProtocolDescriptor;
    use unpeel_core::remote_session_backend::{RemoteProjectFolderSummary, RemoteSessionSummary};
    use unpeel_core::terminal_viewport::TerminalViewportState;

    fn strings(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| (*value).to_owned()).collect()
    }

    fn test_effects() -> (RemoteEffectQueue, mpsc::Receiver<RemoteEffect>) {
        let (sender, receiver) = mpsc::channel();
        (
            RemoteEffectQueue {
                sender,
                halted: Arc::new(AtomicBool::new(false)),
                owned_fits: Arc::new(std::sync::Mutex::new(HashSet::new())),
            },
            receiver,
        )
    }

    fn mouse(kind: MouseEventKind, column: u16, row: u16) -> MouseEvent {
        MouseEvent {
            kind,
            column,
            row,
            modifiers: KeyModifiers::NONE,
        }
    }

    fn remote_mouse_fixture(modes: &[u8]) -> (App, SnapshotService) {
        let initial = snapshot();
        let app = remote_app(model_from_bootstrap(&initial), "ssh://studio");
        let selected = app.selected_id.clone().expect("remote Session selection");
        let snapshots = SnapshotService::new();
        let mut viewport = TerminalViewportState::new(20, 5);
        viewport.feed(modes);
        snapshots.publish(selected, viewport.snapshot(0, None));
        (app, snapshots)
    }

    #[test]
    fn host_parser_claims_remote_intent_before_local_dispatch() {
        assert!(parse_host_target(&[]).unwrap().is_none());
        let target = parse_host_target(&strings(&["--host", "ssh://studio"]))
            .unwrap()
            .unwrap();
        assert_eq!(target.destination(), "studio");
        let target = parse_host_target(&strings(&["--host=ssh://tommy@studio"]))
            .unwrap()
            .unwrap();
        assert_eq!(target.destination(), "tommy@studio");

        for args in [
            strings(&["--host"]),
            strings(&["--host", "--json"]),
            strings(&["--host", "http://studio"]),
            strings(&["--host", "ssh://studio", "ls"]),
            strings(&["ls", "--host", "ssh://studio"]),
            strings(&["--host", "ssh://a", "--host", "ssh://b"]),
        ] {
            assert!(parse_host_target(&args).is_err(), "accepted {args:?}");
        }
    }

    fn snapshot() -> RemoteBootstrapSnapshot {
        RemoteBootstrapSnapshot {
            protocol_version: 1,
            host_protocol: Some(HostProtocolDescriptor {
                major_version: 1,
                minor_version: 0,
                capabilities: Vec::new(),
            }),
            host_id: Some("host-1".to_owned()),
            host_name: Some("Studio".to_owned()),
            folders: Vec::<RemoteProjectFolderSummary>::new(),
            projects: vec![
                RemoteProjectSummary {
                    id: "root".to_owned(),
                    name: "Main".to_owned(),
                    path: "/host/main".to_owned(),
                    folder_id: None,
                    parent_project_id: None,
                    worktree_branch: None,
                    is_group: Some(false),
                    color_id: None,
                    git_branch: Some("main".to_owned()),
                    mcp_blocked: false,
                    sort_order: Some(1),
                    archived_session_count: Some(2),
                    date_sorted: Some(true),
                },
                RemoteProjectSummary {
                    id: "child".to_owned(),
                    name: "Experiment".to_owned(),
                    path: "/host/experiment".to_owned(),
                    folder_id: None,
                    parent_project_id: Some("root".to_owned()),
                    worktree_branch: Some("experiment".to_owned()),
                    is_group: Some(false),
                    color_id: None,
                    git_branch: Some("experiment".to_owned()),
                    mcp_blocked: false,
                    sort_order: Some(2),
                    archived_session_count: None,
                    date_sorted: None,
                },
            ],
            presets: Vec::new(),
            sessions: vec![
                session("busy", "root", RemoteActivityState::Working, true, false),
                session(
                    "blocked",
                    "child",
                    RemoteActivityState::Blocked,
                    false,
                    false,
                ),
                session("archived", "root", RemoteActivityState::Idle, false, true),
            ],
            captured_at_unix_ms: 10,
            remote_server_port: None,
            remote_server_certificate_fingerprint: None,
            experimental_worktrees_enabled: None,
            pro_entitled: None,
            pending_approvals: None,
        }
    }

    fn session(
        id: &str,
        project_id: &str,
        activity: RemoteActivityState,
        pinned: bool,
        archived: bool,
    ) -> RemoteSessionSummary {
        RemoteSessionSummary {
            id: id.to_owned(),
            project_id: project_id.to_owned(),
            active_runtime_id: None,
            provider_id: None,
            title: id.to_owned(),
            command: "codex".to_owned(),
            created_at_unix_ms: 5,
            updated_at_unix_ms: Some(8),
            runtime_launch_pending: false,
            status: RemoteSessionStatus::Running,
            activity,
            unread: true,
            pinned,
            worktree_path: None,
            worktree_branch: None,
            parent_session_id: None,
            last_output_preview: None,
            notify_when_done: false,
            terminal_background_hex: None,
            capabilities: None,
            archived,
            spinner_color_hex: None,
        }
    }

    #[test]
    fn remote_summary_maps_active_runtime_without_rewriting_the_launch_command() {
        let mut summary = session(
            "blank-shell",
            "project-a",
            RemoteActivityState::Working,
            false,
            false,
        );
        summary.command.clear();
        summary.active_runtime_id = Some("claude".into());

        let row = session_row(&summary);
        assert_eq!(row.command, "");
        assert_eq!(row.active_runtime_id.as_deref(), Some("claude"));
        assert_eq!(row.presentation_command(), "claude");
    }

    #[test]
    fn remote_resume_rows_require_host_and_session_capabilities() {
        let mut bootstrap = snapshot();
        bootstrap.host_protocol.as_mut().unwrap().capabilities = vec![
            REMOTE_CAPABILITY_RESTART.into(),
            REMOTE_CAPABILITY_RESUME_AGENT.into(),
        ];
        let capabilities = unpeel_core::remote_session_backend::RemoteSessionCapabilities {
            restart: false,
            resume_agent: true,
            restart_agent: false,
            fork: false,
            append_system_context: false,
            notify_when_done: false,
            archive: true,
        };
        bootstrap.sessions[0].capabilities = Some(capabilities.clone());
        bootstrap.sessions[0].active_runtime_id = None;
        bootstrap.sessions[1].status = RemoteSessionStatus::Exited;
        bootstrap.sessions[1].active_runtime_id = None;
        bootstrap.sessions[1].capabilities = Some(
            unpeel_core::remote_session_backend::RemoteSessionCapabilities {
                restart: true,
                resume_agent: false,
                restart_agent: false,
                ..capabilities
            },
        );

        let model = model_from_bootstrap(&bootstrap);
        let live = model.rows.iter().find(|row| row.id == "busy").unwrap();
        assert!(live.resume_agent_available);
        assert!(!live.resume_available);
        let exited = model.rows.iter().find(|row| row.id == "blocked").unwrap();
        assert!(!exited.resume_agent_available);
        assert!(exited.resume_available);

        bootstrap.sessions[0].activity = RemoteActivityState::Starting;
        let model = model_from_bootstrap(&bootstrap);
        assert!(
            !model
                .rows
                .iter()
                .find(|row| row.id == "busy")
                .unwrap()
                .resume_agent_available
        );
        bootstrap.sessions[0].activity = RemoteActivityState::Working;

        bootstrap.sessions[0].runtime_launch_pending = true;
        let model = model_from_bootstrap(&bootstrap);
        assert!(
            !model
                .rows
                .iter()
                .find(|row| row.id == "busy")
                .unwrap()
                .resume_agent_available
        );
        bootstrap.sessions[0].runtime_launch_pending = false;

        bootstrap
            .host_protocol
            .as_mut()
            .unwrap()
            .capabilities
            .retain(|capability| capability != REMOTE_CAPABILITY_RESUME_AGENT);
        let model = model_from_bootstrap(&bootstrap);
        assert!(
            !model
                .rows
                .iter()
                .find(|row| row.id == "busy")
                .unwrap()
                .resume_agent_available
        );
    }

    #[test]
    fn legacy_restart_agent_decode_never_surfaces_a_resume_action() {
        let mut bootstrap = snapshot();
        bootstrap.host_protocol.as_mut().unwrap().capabilities = vec![
            // Old Hosts may still decode and publish this capability, but the
            // current Controller must never turn it into a visible action.
            "session.runtime.restart".into(),
        ];
        bootstrap.sessions[0].capabilities = Some(
            unpeel_core::remote_session_backend::RemoteSessionCapabilities {
                restart: false,
                resume_agent: false,
                restart_agent: true,
                fork: false,
                append_system_context: false,
                notify_when_done: false,
                archive: true,
            },
        );

        let model = model_from_bootstrap(&bootstrap);
        let live = model.rows.iter().find(|row| row.id == "busy").unwrap();
        assert!(!live.resume_agent_available);
        assert!(!live.resume_available);
    }

    #[test]
    fn bootstrap_mapping_is_host_only_and_matches_local_sidebar_rules() {
        let model = model_from_bootstrap(&snapshot());
        assert_eq!(model.rows.len(), 3);
        assert!(model.rows.iter().all(|row| row.cwd.is_empty()));
        assert_eq!(model.rows[0].status, Status::Busy);
        assert_eq!(model.rows[1].status, Status::Attention);
        assert_eq!(model.archived_counts.get("root"), Some(&2));
        // Populated projects get no "+ New session" row (same rule as
        // local), and project creation is never offered against a Host.
        assert!(!model.items.iter().any(|item| matches!(
            item,
            SidebarItem::NewSession { .. } | SidebarItem::AddProject
        )));
        assert!(model.items.iter().any(|item| matches!(
            item,
            SidebarItem::WorktreeHeader { project_id, branch, .. }
                if project_id == "child" && branch == "experiment"
        )));
        assert!(!model.items.iter().any(|item| matches!(
            item,
            SidebarItem::Session(index) if model.rows[*index].id == "archived"
        )));
    }

    #[test]
    fn bootstrap_project_order_is_the_hosts_advertised_order() {
        // Hosts emit projects in their DISPLAY order with sortOrder equal to
        // the array rank (both agree). The Controller must render exactly
        // that order — never a local or re-derived one.
        let mut bootstrap = snapshot();
        bootstrap.sessions.clear();
        let mut second = bootstrap.projects[0].clone();
        second.id = "second".to_owned();
        second.name = "Second".to_owned();
        bootstrap.projects[0].sort_order = Some(0);
        bootstrap.projects[1].sort_order = Some(1);
        second.sort_order = Some(2);
        bootstrap.projects.push(second);

        let headers = |model: &SidebarModel| -> Vec<String> {
            model
                .items
                .iter()
                .filter_map(|item| match item {
                    SidebarItem::Header(name) => Some(name.clone()),
                    _ => None,
                })
                .collect()
        };
        let model = model_from_bootstrap(&bootstrap);
        assert_eq!(headers(&model), vec!["Main", "Second"]);

        // A Host-side drag flips the advertised order; the Controller
        // follows without consulting anything local. Absent sort_order
        // falls back to array position, so both agreeing forms hold.
        let reordered: Vec<_> = vec![
            bootstrap.projects[2].clone(),
            bootstrap.projects[1].clone(),
            bootstrap.projects[0].clone(),
        ];
        bootstrap.projects = reordered;
        for (index, project) in bootstrap.projects.iter_mut().enumerate() {
            project.sort_order = Some(index as i64);
        }
        let model = model_from_bootstrap(&bootstrap);
        assert_eq!(headers(&model), vec!["Second", "Main"]);

        for project in &mut bootstrap.projects {
            project.sort_order = None;
        }
        let model = model_from_bootstrap(&bootstrap);
        assert_eq!(headers(&model), vec!["Second", "Main"]);
    }

    #[test]
    fn empty_host_projects_offer_the_new_session_row_but_never_add_project() {
        let mut bootstrap = snapshot();
        bootstrap.sessions.clear();
        let model = model_from_bootstrap(&bootstrap);
        assert!(model.items.iter().any(|item| matches!(
            item,
            SidebarItem::NewSession { project, .. } if project == "root"
        )));
        assert!(model.items.iter().any(|item| matches!(
            item,
            SidebarItem::NewSession { project, .. } if project == "child"
        )));
        assert!(!model
            .items
            .iter()
            .any(|item| matches!(item, SidebarItem::AddProject)));
    }

    #[test]
    fn every_unarchived_host_session_remains_visible_at_unsupported_depth() {
        let mut bootstrap = snapshot();
        let mut grandchild = bootstrap.projects[1].clone();
        grandchild.id = "grandchild".to_owned();
        grandchild.name = "Nested".to_owned();
        grandchild.parent_project_id = Some("child".to_owned());
        grandchild.sort_order = Some(3);
        bootstrap.projects.push(grandchild);
        bootstrap.sessions.push(session(
            "nested-session",
            "grandchild",
            RemoteActivityState::Idle,
            false,
            false,
        ));

        let model = model_from_bootstrap(&bootstrap);
        assert!(model.items.iter().any(|item| matches!(
            item,
            SidebarItem::Session(index) if model.rows[*index].id == "nested-session"
        )));
        assert!(model
            .items
            .iter()
            .any(|item| matches!(item, SidebarItem::Header(name) if name == "Other")));
    }

    #[test]
    fn interactive_scope_requires_advertised_effect_capabilities() {
        let mut bootstrap = snapshot();
        let error = require_interactive_capabilities(&bootstrap).unwrap_err();
        assert!(error.to_string().contains("session.input.write"));

        bootstrap.host_protocol = Some(HostProtocolDescriptor::headless_v1());
        require_interactive_capabilities(&bootstrap).unwrap();
    }

    #[test]
    fn only_outcome_unknown_effects_halt_remote_input() {
        assert!(!effect_failure_requires_halt(
            RemoteEffectFailureKind::NotApplied
        ));
        assert!(effect_failure_requires_halt(
            RemoteEffectFailureKind::OutcomeUnknown
        ));
    }

    #[test]
    fn output_wake_is_edge_triggered() {
        let (sender, receiver) = mpsc::channel();
        let wake = RemoteWake::new(sender);
        for _ in 0..10_000 {
            wake.wake();
        }
        assert!(matches!(receiver.try_recv(), Ok(RemoteUiEvent::Wake)));
        assert!(matches!(
            receiver.try_recv(),
            Err(mpsc::TryRecvError::Empty)
        ));
        wake.consumed();
        wake.wake();
        assert!(matches!(receiver.try_recv(), Ok(RemoteUiEvent::Wake)));
    }

    #[test]
    fn bootstrap_change_detaches_before_selecting_another_agent() {
        let mut initial = snapshot();
        let mut app = remote_app(model_from_bootstrap(&initial), "ssh://studio");
        app.selected_id = Some("busy".to_owned());
        app.terminal_focus = true;
        initial.sessions.retain(|session| session.id != "busy");
        let mut replacement_selection = RemoteReplacementSelectionState::default();

        apply_bootstrap(&mut app, &initial, &mut replacement_selection);

        assert_ne!(app.selected_id.as_deref(), Some("busy"));
        assert!(!app.terminal_focus);
        assert_eq!(app.preview_scroll, 0);
    }

    #[test]
    fn replacement_selection_requires_one_new_exact_identity() {
        let mut initial = snapshot();
        initial.sessions[0].status = RemoteSessionStatus::Exited;
        initial.sessions[0].created_at_unix_ms = 42;
        let mut existing_collision = session(
            "existing-collision",
            "root",
            RemoteActivityState::Idle,
            false,
            false,
        );
        existing_collision.created_at_unix_ms = 42;
        initial.sessions.push(existing_collision);
        let mut app = remote_app(model_from_bootstrap(&initial), "ssh://studio");
        app.selected_id = Some("busy".to_owned());
        let baseline = initial
            .sessions
            .iter()
            .map(|session| session.id.clone())
            .collect();
        let intent = replacement_selection_intent(&initial.sessions[0], baseline);
        let mut replacement_selection = RemoteReplacementSelectionState::default();
        replacement_selection.begin(intent);

        let mut changed = initial.clone();
        changed.sessions.retain(|session| session.id != "busy");
        apply_bootstrap(&mut app, &changed, &mut replacement_selection);
        assert!(replacement_selection.pending.is_some());
        assert_eq!(app.selected_id, None, "a baseline collision must not win");

        let mut wrong_runtime = session(
            "wrong-runtime",
            "root",
            RemoteActivityState::Starting,
            false,
            false,
        );
        wrong_runtime.created_at_unix_ms = 42;
        wrong_runtime.command = "claude".to_owned();
        changed.sessions.push(wrong_runtime);
        apply_bootstrap(&mut app, &changed, &mut replacement_selection);
        assert!(replacement_selection.pending.is_some());
        assert_eq!(app.selected_id, None, "a different runtime must not win");
        changed
            .sessions
            .retain(|session| session.id != "wrong-runtime");

        let mut replacement_a = session(
            "replacement-a",
            "root",
            RemoteActivityState::Starting,
            false,
            false,
        );
        replacement_a.created_at_unix_ms = 42;
        changed.sessions.push(replacement_a);
        apply_bootstrap(&mut app, &changed, &mut replacement_selection);
        assert!(replacement_selection.pending.is_none());
        assert!(!replacement_selection.suppress_default);
        assert_eq!(app.selected_id.as_deref(), Some("replacement-a"));
    }

    #[test]
    fn ambiguous_replacement_permanently_cancels_auto_adoption() {
        let mut initial = snapshot();
        initial.sessions[0].status = RemoteSessionStatus::Exited;
        initial.sessions[0].created_at_unix_ms = 42;
        let mut app = remote_app(model_from_bootstrap(&initial), "ssh://studio");
        app.selected_id = Some("busy".to_owned());
        let baseline = initial
            .sessions
            .iter()
            .map(|session| session.id.clone())
            .collect();
        let intent = replacement_selection_intent(&initial.sessions[0], baseline);
        let mut replacement_selection = RemoteReplacementSelectionState::default();
        replacement_selection.begin(intent);

        let mut changed = initial.clone();
        changed.sessions.retain(|session| session.id != "busy");
        let mut replacement_a = session(
            "replacement-a",
            "root",
            RemoteActivityState::Starting,
            false,
            false,
        );
        replacement_a.created_at_unix_ms = 42;
        let mut replacement_b = replacement_a.clone();
        replacement_b.id = "replacement-b".to_owned();
        changed.sessions.push(replacement_a);
        changed.sessions.push(replacement_b);
        apply_bootstrap(&mut app, &changed, &mut replacement_selection);
        assert!(replacement_selection.pending.is_none());
        assert!(replacement_selection.suppress_default);
        assert_eq!(app.selected_id, None);

        changed
            .sessions
            .retain(|session| session.id != "replacement-b");
        apply_bootstrap(&mut app, &changed, &mut replacement_selection);
        assert_eq!(
            app.selected_id, None,
            "resolving a prior ambiguity must not resurrect auto-selection"
        );
    }

    #[test]
    fn replacement_selection_expiry_keeps_default_fallback_suppressed() {
        let mut initial = snapshot();
        initial.sessions[0].status = RemoteSessionStatus::Exited;
        initial.sessions[0].created_at_unix_ms = 42;
        let mut app = remote_app(model_from_bootstrap(&initial), "ssh://studio");
        app.selected_id = Some("busy".to_owned());
        let baseline = initial
            .sessions
            .iter()
            .map(|session| session.id.clone())
            .collect();
        let mut intent = replacement_selection_intent(&initial.sessions[0], baseline);
        intent.bootstrap_observations_remaining = 1;
        let mut replacement_selection = RemoteReplacementSelectionState::default();
        replacement_selection.begin(intent);

        initial.sessions.retain(|session| session.id != "busy");
        apply_bootstrap(&mut app, &initial, &mut replacement_selection);
        assert!(replacement_selection.pending.is_none());
        assert!(replacement_selection.suppress_default);
        assert_eq!(app.selected_id, None);
    }

    #[test]
    fn bootstrap_recovery_keeps_an_active_stream_error_visible() {
        let initial = snapshot();
        let mut app = remote_app(model_from_bootstrap(&initial), "ssh://studio");
        let mut displayed = None;
        let mut bootstrap = Some("Host unavailable".to_owned());
        let mut stream = Some("Host reconnecting".to_owned());

        update_connection_notice(&mut app, None, &bootstrap, &stream, &mut displayed);
        assert_eq!(app.info.as_deref(), Some("Host unavailable"));

        bootstrap = None;
        update_connection_notice(&mut app, None, &bootstrap, &stream, &mut displayed);
        assert_eq!(app.info.as_deref(), Some("Host reconnecting"));

        stream = None;
        update_connection_notice(&mut app, None, &bootstrap, &stream, &mut displayed);
        assert!(app.info.is_none());
    }

    #[test]
    fn remote_grid_matches_the_host_resize_contract_at_large_window_sizes() {
        let initial = snapshot();
        let app = remote_app(model_from_bootstrap(&initial), "ssh://studio");

        assert_eq!(
            remote_preview_grid(&app, u16::MAX, u16::MAX),
            (
                REMOTE_DESKTOP_RESIZE_MAX_COLUMNS,
                REMOTE_DESKTOP_RESIZE_MAX_ROWS
            )
        );
        assert_eq!(remote_preview_grid(&app, 1, 1), (4, 2));
    }

    #[test]
    fn sidebar_session_click_focuses_without_leaking_input_or_scroll_selection() {
        let (mut app, snapshots) = remote_mouse_fixture(b"\x1b[?1000h");
        let selected = app.selected_id.clone();
        let mut expected_unread = app.unread_ids.clone();
        if let Some(selected) = selected.as_ref() {
            expected_unread.remove(selected);
        }
        let (effects, receiver) = test_effects();
        let mut replacement_selection = RemoteReplacementSelectionState::default();
        let source = session(
            selected.as_deref().unwrap(),
            "root",
            RemoteActivityState::Working,
            false,
            false,
        );
        replacement_selection.begin(replacement_selection_intent(&source, HashSet::new()));
        app.terminal_focus = true;
        app.selected_archive = Some(("p".to_owned(), 0));
        let selected_row = app
            .visible_items()
            .iter()
            .position(|item| match item {
                SidebarItem::Session(index) => {
                    Some(app.model.rows[*index].id.as_str()) == selected.as_deref()
                }
                _ => false,
            })
            .unwrap() as u16
            + 1;

        handle_remote_mouse(
            &mut app,
            mouse(MouseEventKind::Down(MouseButton::Left), 2, selected_row),
            150,
            45,
            &snapshots,
            &effects,
            true,
            &mut replacement_selection,
        );
        assert!(replacement_selection.pending.is_none());
        assert!(!replacement_selection.suppress_default);
        handle_remote_mouse(
            &mut app,
            mouse(MouseEventKind::Up(MouseButton::Left), 2, selected_row),
            150,
            45,
            &snapshots,
            &effects,
            true,
            &mut replacement_selection,
        );
        handle_remote_mouse(
            &mut app,
            mouse(MouseEventKind::ScrollDown, 2, 3),
            150,
            45,
            &snapshots,
            &effects,
            true,
            &mut replacement_selection,
        );

        assert!(app.terminal_focus);
        assert!(app.selected_archive.is_none());
        assert_eq!(app.selected_id, selected);
        assert_eq!(app.unread_ids, expected_unread);
        assert!(matches!(
            receiver.try_recv(),
            Ok(RemoteEffect::MarkRead { session_id })
                if Some(session_id.as_str()) == selected.as_deref()
        ));
        assert!(receiver.try_recv().is_err());
    }

    #[test]
    fn remote_drag_obeys_button_motion_and_first_right_click_focuses() {
        let (mut app, snapshots) = remote_mouse_fixture(b"\x1b[?1000h");
        let (effects, receiver) = test_effects();
        let mut replacement_selection = RemoteReplacementSelectionState::default();
        let target = remote_terminal_target(&app, 150, 45, &snapshots).unwrap();
        app.terminal_focus = true;

        handle_remote_mouse(
            &mut app,
            mouse(MouseEventKind::Drag(MouseButton::Left), target.x, target.y),
            150,
            45,
            &snapshots,
            &effects,
            true,
            &mut replacement_selection,
        );
        assert!(receiver.try_recv().is_err());

        let mut viewport = TerminalViewportState::new(20, 5);
        viewport.feed(b"\x1b[?1002h");
        snapshots.publish(app.selected_id.clone().unwrap(), viewport.snapshot(0, None));
        handle_remote_mouse(
            &mut app,
            mouse(MouseEventKind::Drag(MouseButton::Left), target.x, target.y),
            150,
            45,
            &snapshots,
            &effects,
            true,
            &mut replacement_selection,
        );
        match receiver.try_recv().unwrap() {
            RemoteEffect::Write { data, .. } => assert_eq!(data, "\x1b[<32;1;1M"),
            _ => panic!("drag did not enqueue terminal input"),
        }

        app.terminal_focus = false;
        handle_remote_mouse(
            &mut app,
            mouse(
                MouseEventKind::Down(MouseButton::Right),
                target.x + 1,
                target.y + 1,
            ),
            150,
            45,
            &snapshots,
            &effects,
            true,
            &mut replacement_selection,
        );
        assert!(app.terminal_focus);
        match receiver.try_recv().unwrap() {
            RemoteEffect::Write { data, .. } => assert_eq!(data, "\x1b[<2;2;2M"),
            _ => panic!("right click did not enqueue terminal input"),
        }
    }
}
