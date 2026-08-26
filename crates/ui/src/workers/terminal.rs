use std::collections::HashMap;
use std::time::Duration;

use gpui::{
    ClipboardItem, Context, FocusHandle, IntoElement, KeyDownEvent, MouseButton, MouseDownEvent,
    MouseMoveEvent, MouseUpEvent, Pixels, Render, Task, Window, div, prelude::*, px,
};
use zeron_workers_unpeel::{
    LocalWorkersClient, WorkersOutput, WorkersViewport, WorkersViewportInputModes,
};

use crate::terminal::emulator::{Emulator, GridPoint, SelectionType, Side};
use crate::terminal::panel::{GridGeometry, GridSnapshot};
use crate::terminal::scroll::{
    MouseProtocol, SCROLLBAR_HIT_WIDTH, SCROLLBAR_HOVER_THUMB_WIDTH, SCROLLBAR_THUMB_WIDTH,
    SCROLLBAR_TRACK_INSET, ScrollbarMetrics, TerminalScrollAction, TerminalScrollGesture,
    TerminalScrollModes, scrollbar_metrics, terminal_scroll_action,
};
use crate::terminal::view::{
    COALESCE_MS, InputCoalescer, SELECTION_DRAG_THRESHOLD, TERM_LINE_HEIGHT, TerminalElement,
    cell_at, keystroke_bytes, paste_bytes,
};

#[derive(Debug, Clone, Copy)]
struct SelectionDrag {
    origin: gpui::Point<Pixels>,
    armed: bool,
}

#[derive(Debug, Clone, Copy)]
struct ScrollbarDrag {
    grab_offset: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MouseReportKind {
    Down,
    Up,
    Drag,
    Move,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TerminalRefresh {
    Snapshot,
    Incremental,
    Idle,
}

fn terminal_refresh(viewport_dirty: bool, truncated: bool, has_data: bool) -> TerminalRefresh {
    if viewport_dirty || truncated {
        TerminalRefresh::Snapshot
    } else if has_data {
        TerminalRefresh::Incremental
    } else {
        TerminalRefresh::Idle
    }
}

fn viewport_has_tui_jump_hint(text: &str) -> bool {
    let lines = text.lines().collect::<Vec<_>>();
    lines.iter().rev().take(15).any(|line| {
        if line.contains("Jump to bottom (ctrl+End)") {
            return true;
        }
        [" new message (ctrl+End)", " new messages (ctrl+End)"]
            .into_iter()
            .any(|marker| {
                line.find(marker).is_some_and(|index| {
                    line[..index]
                        .split_whitespace()
                        .next_back()
                        .is_some_and(|count| count.parse::<u64>().is_ok())
                })
            })
    })
}

fn emulator_mouse_protocol(emulator: &Emulator) -> MouseProtocol {
    if emulator.sgr_mouse_mode() {
        MouseProtocol::Sgr
    } else if emulator.utf8_mouse_mode() {
        MouseProtocol::Utf8
    } else {
        MouseProtocol::Normal
    }
}

fn mouse_report_bytes(
    kind: MouseReportKind,
    column: usize,
    row: usize,
    modifiers: gpui::Modifiers,
) -> Vec<u8> {
    let base = match kind {
        MouseReportKind::Down | MouseReportKind::Up => 0,
        MouseReportKind::Drag => 32,
        MouseReportKind::Move => 35,
    };
    let modifier_bits = if modifiers.shift { 4 } else { 0 }
        + if modifiers.alt { 8 } else { 0 }
        + if modifiers.control { 16 } else { 0 };
    let suffix = if kind == MouseReportKind::Up {
        'm'
    } else {
        'M'
    };
    format!(
        "\x1b[<{};{};{}{suffix}",
        base + modifier_bits,
        column.saturating_add(1),
        row.saturating_add(1)
    )
    .into_bytes()
}

fn scroll_action(
    modes: WorkersViewportInputModes,
    mouse_protocol: MouseProtocol,
    steps: i32,
    column: usize,
    row: usize,
) -> TerminalScrollAction {
    terminal_scroll_action(
        TerminalScrollModes {
            mouse_reporting: modes.mouse_reporting,
            mouse_protocol,
            alternate_screen: modes.alternate_screen,
            mouse_alternate_scroll: modes.mouse_alternate_scroll,
            application_cursor: modes.application_cursor,
        },
        steps,
        column,
        row,
    )
}

#[derive(Default)]
struct RemoteGridTracker {
    grids: HashMap<String, (u16, u16)>,
}

impl RemoteGridTracker {
    fn record_resize(&mut self, session_id: &str, cols: u16, rows: u16) -> bool {
        let next = (cols, rows);
        if self.grids.get(session_id) == Some(&next) {
            return false;
        }
        self.grids.insert(session_id.to_owned(), next);
        true
    }

    fn invalidate(&mut self, session_id: &str, cols: u16, rows: u16) {
        if self.grids.get(session_id) == Some(&(cols, rows)) {
            self.grids.remove(session_id);
        }
    }
}

#[derive(Default)]
struct HistoricalReplay {
    active: bool,
    grid_ready: bool,
    chunks: u32,
}

/// Teto do catch-up silencioso, medido em chunks de `OUTPUT_MAX_BYTES`
/// (256 KB) do host: 128 MB de backlog escondido. Dimensionado pelos dois
/// lados — o maior scrollback retido que vimos tem 28 MB (~112 chunks, cabe
/// folgado), e nenhuma sessao emite 128 MB AO VIVO no intervalo entre abrir e
/// drenar. O teto existe porque uma sessao que nunca drena deixaria a tela
/// coberta pra sempre; estourado, volta a pintar chunk a chunk, entao o pior
/// caso vira o comportamento antigo e nunca algo pior.
const MAX_SILENT_REPLAY_CHUNKS: u32 = 512;

impl HistoricalReplay {
    fn start(&mut self) {
        self.active = true;
        self.grid_ready = false;
        self.chunks = 0;
    }

    fn observe_geometry(&mut self) {
        self.grid_ready = true;
    }

    fn can_consume_output(&self) -> bool {
        !self.active || self.grid_ready
    }

    /// Enquanto o backlog nao drena, o terminal alimenta o emulador sem
    /// pintar: cada chunk pintado era um quadro do terminal rolando do topo,
    /// e quem abre a sessao quer o fim dela, nao o filme.
    fn is_catching_up(&self) -> bool {
        self.active && self.chunks < MAX_SILENT_REPLAY_CHUNKS
    }

    fn observe_output(&mut self, had_data: bool) {
        if had_data {
            self.chunks = self.chunks.saturating_add(1);
        } else {
            self.active = false;
        }
    }
}

/// Se este refresh deve virar um quadro na tela. Pintar so quando o catch-up
/// acabou (ou quando ele acabou AGORA, que e o quadro que o usuario pediu:
/// a ultima interacao).
fn should_paint(
    was_catching_up: bool,
    catching_up: bool,
    had_data: bool,
    viewport_dirty: bool,
) -> bool {
    !catching_up && (had_data || viewport_dirty || was_catching_up)
}

#[derive(Default)]
struct ResizeSync {
    epoch: u64,
    pending: bool,
}

impl ResizeSync {
    fn start(&mut self) -> u64 {
        self.epoch = self.epoch.wrapping_add(1);
        self.pending = true;
        self.epoch
    }

    fn complete(&mut self, epoch: u64) -> bool {
        if self.epoch != epoch {
            return false;
        }
        self.pending = false;
        true
    }

    fn epoch(&self) -> u64 {
        self.epoch
    }

    fn pending(&self) -> bool {
        self.pending
    }
}

/// A UI attachment to an already-running worker session. Dropping or
/// detaching this value owns only the local view; worker lifecycle authority
/// remains exclusively with [`WorkersModel`](crate::workers::model::WorkersModel).
pub(crate) struct WorkersTerminalView<T> {
    session_id: String,
    terminal: T,
}

impl<T> WorkersTerminalView<T> {
    pub(crate) fn new(session_id: impl Into<String>, terminal: T) -> Self {
        Self {
            session_id: session_id.into(),
            terminal,
        }
    }

    pub(crate) fn session_id(&self) -> &str {
        &self.session_id
    }

    pub(crate) fn terminal(&self) -> &T {
        &self.terminal
    }

    pub(crate) fn detach(self) {
        drop(self.terminal);
    }
}

struct WorkersTerminalState {
    emulator: Emulator,
    offset: u64,
    viewport_dirty: bool,
    modes_from_snapshot: bool,
    historical_replay: HistoricalReplay,
    resize_sync: ResizeSync,
    input_modes: WorkersViewportInputModes,
    selection_drag: Option<SelectionDrag>,
    scroll_gesture: TerminalScrollGesture,
    resize_error: Option<String>,
    resize_retry_blocked: bool,
    tui_jump_suppressed: bool,
    mouse_protocol: MouseProtocol,
}

impl WorkersTerminalState {
    fn new(cols: u16, rows: u16) -> Self {
        Self {
            emulator: Emulator::new(cols, rows),
            offset: 0,
            viewport_dirty: true,
            modes_from_snapshot: false,
            historical_replay: HistoricalReplay::default(),
            resize_sync: ResizeSync::default(),
            input_modes: WorkersViewportInputModes::default(),
            selection_drag: None,
            scroll_gesture: TerminalScrollGesture::default(),
            resize_error: None,
            resize_retry_blocked: false,
            tui_jump_suppressed: false,
            mouse_protocol: MouseProtocol::Sgr,
        }
    }

    fn has_tui_jump_hint(&self) -> bool {
        let text = (0..self.emulator.rows())
            .map(|row| self.emulator.row_text(row))
            .collect::<Vec<_>>()
            .join("\n");
        viewport_has_tui_jump_hint(&text)
    }

    fn apply_refresh(&mut self, output: WorkersOutput, viewport: Option<WorkersViewport>) -> bool {
        let had_data = !output.data.is_empty();
        let truncated = output.truncated;
        if truncated {
            let (cols, rows) = viewport
                .as_ref()
                .map(|viewport| (viewport.cols, viewport.rows))
                .unwrap_or((self.emulator.cols() as u16, self.emulator.rows() as u16));
            self.emulator = Emulator::new(cols, rows);
            self.modes_from_snapshot = true;
        }
        if had_data {
            let _ = self.emulator.feed(&output.data);
            if !truncated {
                self.mouse_protocol = emulator_mouse_protocol(&self.emulator);
            }
        }
        self.offset = output.next_offset;
        if let Some(viewport) = viewport {
            self.input_modes = viewport.input_modes;
            if output.truncated && !had_data {
                let _ = self.emulator.feed(&viewport.ansi);
            }
        } else if had_data && !self.modes_from_snapshot {
            self.input_modes = WorkersViewportInputModes {
                known: true,
                mouse_reporting: self.emulator.mouse_reporting_mode(),
                mouse_button_motion: self.emulator.mouse_button_motion_mode(),
                mouse_any_motion: self.emulator.mouse_any_motion_mode(),
                alternate_screen: self.emulator.alternate_screen_mode(),
                mouse_alternate_scroll: self.emulator.mouse_alternate_scroll_mode(),
                application_cursor: self.emulator.app_cursor_mode(),
            };
        }
        self.viewport_dirty = had_data && self.modes_from_snapshot;
        self.historical_replay.observe_output(had_data);
        if !self.has_tui_jump_hint() {
            self.tui_jump_suppressed = false;
        }
        had_data
    }
}

#[derive(Default)]
struct RetainedWorkerTerminals {
    active_id: Option<String>,
    states: HashMap<String, WorkersTerminalState>,
}

impl RetainedWorkerTerminals {
    fn select(&mut self, session_id: Option<String>, cols: u16, rows: u16) -> bool {
        let mut inserted = false;
        if let Some(session_id) = session_id.as_ref() {
            if !self.states.contains_key(session_id) {
                self.states
                    .insert(session_id.clone(), WorkersTerminalState::new(cols, rows));
                inserted = true;
            }
        }
        self.active_id = session_id;
        inserted
    }

    fn active(&self) -> Option<&WorkersTerminalState> {
        self.states.get(self.active_id.as_deref()?)
    }

    fn active_mut(&mut self) -> Option<&mut WorkersTerminalState> {
        self.states.get_mut(self.active_id.as_deref()?)
    }

    fn jump_to_bottom(&mut self) -> bool {
        let Some(state) = self.active_mut() else {
            return false;
        };
        if state.emulator.display_offset() == 0 {
            return false;
        }
        state.emulator.scroll_to_bottom();
        true
    }

    fn complete_resize(&mut self, session_id: &str, epoch: u64) -> bool {
        self.states
            .get_mut(session_id)
            .is_some_and(|state| state.resize_sync.complete(epoch))
    }

    fn shed_scrollback(&mut self, include_active: bool) {
        let active_id = self.active_id.clone();
        for (session_id, state) in &mut self.states {
            if active_id.as_deref() == Some(session_id.as_str()) {
                continue;
            }
            state.emulator.clear_scrollback();
            state.selection_drag = None;
        }
        if include_active && let Some(state) = self.active_mut() {
            state.emulator.clear_scrollback();
            state.selection_drag = None;
        }
    }
}

pub struct WorkersTerminal {
    client: LocalWorkersClient,
    session_id: Option<String>,
    terminals: RetainedWorkerTerminals,
    generation: u64,
    error: Option<String>,
    geometry: Option<GridGeometry>,
    remote_grids: RemoteGridTracker,
    focus_handle: FocusHandle,
    focus_pending: bool,
    coalescer: InputCoalescer,
    flush_task: Option<Task<()>>,
    scrollbar_drag: Option<ScrollbarDrag>,
    terminal_hovered: bool,
    scrollbar_hovered: bool,
    _poll_task: Task<()>,
}

impl WorkersTerminal {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let poll_task = cx.spawn(async move |this, cx| {
            let mut error_backoff_ms = 0_u64;
            loop {
                let Ok((
                    session_id,
                    offset,
                    generation,
                    client,
                    can_consume_output,
                    geometry,
                    viewport_dirty,
                    resize_epoch,
                )) = this.update(cx, |terminal, _| {
                    let state = terminal.active_state();
                    (
                        terminal.session_id.clone(),
                        state.map_or(0, |state| state.offset),
                        terminal.generation,
                        terminal.client.clone(),
                        state.is_some_and(|state| {
                            state.historical_replay.can_consume_output()
                                && !state.resize_sync.pending()
                        }),
                        terminal.geometry,
                        state.is_some_and(|state| state.viewport_dirty),
                        state.map_or(0, |state| state.resize_sync.epoch()),
                    )
                })
                else {
                    break;
                };
                let Some(session_id) = session_id else {
                    cx.background_executor()
                        .timer(Duration::from_millis(80))
                        .await;
                    continue;
                };
                if !can_consume_output {
                    cx.background_executor()
                        .timer(Duration::from_millis(16))
                        .await;
                    continue;
                }
                let request_session_id = session_id.clone();
                let result = cx
                    .background_executor()
                    .spawn(async move {
                        let output = client.read_output(&request_session_id, Some(offset), 180)?;
                        let refresh = terminal_refresh(
                            viewport_dirty,
                            output.truncated,
                            !output.data.is_empty(),
                        );
                        let viewport = if refresh == TerminalRefresh::Snapshot {
                            let geometry = geometry.expect("historical replay waits for geometry");
                            Some(client.read_viewport(
                                &request_session_id,
                                geometry.cols,
                                geometry.rows,
                            )?)
                        } else {
                            None
                        };
                        Ok::<_, zeron_workers_unpeel::WorkersError>((output, viewport))
                    })
                    .await;
                let failed = result.is_err();
                if this
                    .update(cx, |terminal, cx| {
                        if terminal.generation != generation
                            || terminal.session_id.as_deref() != Some(session_id.as_str())
                        {
                            return;
                        }
                        let Some(state) = terminal.active_state_mut() else {
                            return;
                        };
                        if state.resize_sync.epoch() != resize_epoch {
                            return;
                        }
                        match result {
                            Ok((output, viewport)) => {
                                let was_catching_up = state.historical_replay.is_catching_up();
                                let had_data = state.apply_refresh(output, viewport);
                                let catching_up = state.historical_replay.is_catching_up();
                                terminal.error = None;
                                if should_paint(
                                    was_catching_up,
                                    catching_up,
                                    had_data,
                                    viewport_dirty,
                                ) {
                                    cx.notify();
                                }
                            }
                            Err(error) => {
                                terminal.error = Some(error.to_string());
                                cx.notify();
                            }
                        }
                    })
                    .is_err()
                {
                    break;
                }
                if failed {
                    error_backoff_ms = if error_backoff_ms == 0 {
                        250
                    } else {
                        (error_backoff_ms * 2).min(2_000)
                    };
                    cx.background_executor()
                        .timer(Duration::from_millis(error_backoff_ms))
                        .await;
                } else {
                    error_backoff_ms = 0;
                }
            }
        });
        Self {
            client: LocalWorkersClient::new(),
            session_id: None,
            terminals: RetainedWorkerTerminals::default(),
            generation: 0,
            error: None,
            geometry: None,
            remote_grids: RemoteGridTracker::default(),
            focus_handle: cx.focus_handle(),
            focus_pending: false,
            coalescer: InputCoalescer::default(),
            flush_task: None,
            scrollbar_drag: None,
            terminal_hovered: false,
            scrollbar_hovered: false,
            _poll_task: poll_task,
        }
    }

    pub fn set_session(&mut self, session_id: Option<String>, cx: &mut Context<Self>) {
        if self.session_id == session_id {
            return;
        }
        self.focus_pending = session_id.is_some();
        self.session_id = session_id.clone();
        let (cols, rows) = self
            .geometry
            .map(|geometry| (geometry.cols, geometry.rows))
            .unwrap_or((80, 24));
        let inserted = self.terminals.select(session_id, cols, rows);
        if inserted && let Some(state) = self.terminals.active_mut() {
            state.historical_replay.start();
        }
        self.generation = self.generation.wrapping_add(1);
        self.error = None;
        self.coalescer.take();
        cx.notify();
    }

    fn active_state(&self) -> Option<&WorkersTerminalState> {
        self.terminals.active()
    }

    fn active_state_mut(&mut self) -> Option<&mut WorkersTerminalState> {
        self.terminals.active_mut()
    }

    pub fn focus(&mut self, cx: &mut Context<Self>) {
        if self.session_id.is_some() {
            self.focus_pending = true;
            cx.notify();
        }
    }

    pub fn on_grid_metrics(&mut self, geometry: GridGeometry, cx: &mut Context<Self>) {
        self.client.remember_grid(geometry.cols, geometry.rows);
        let dimensions_changed = self.geometry.is_none_or(|previous| {
            previous.cols != geometry.cols || previous.rows != geometry.rows
        });
        self.geometry = Some(geometry);
        if let Some(state) = self.active_state_mut() {
            state.historical_replay.observe_geometry();
            if state.emulator.cols() != geometry.cols as usize
                || state.emulator.rows() != geometry.rows as usize
            {
                state.emulator.resize(geometry.cols, geometry.rows);
            }
        }
        let Some(session_id) = self.session_id.clone() else {
            return;
        };
        if self
            .active_state()
            .is_some_and(|state| state.resize_retry_blocked)
        {
            return;
        }
        if !self
            .remote_grids
            .record_resize(&session_id, geometry.cols, geometry.rows)
        {
            if dimensions_changed {
                if let Some(state) = self.active_state_mut() {
                    state.viewport_dirty = true;
                }
                cx.notify();
            }
            return;
        }
        let Some(state) = self.active_state_mut() else {
            return;
        };
        let resize_epoch = state.resize_sync.start();
        let client = self.client.clone();
        let cols = geometry.cols;
        let rows = geometry.rows;
        let request_session_id = session_id.clone();
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { client.resize(&request_session_id, cols, rows) })
                .await;
            let failed = result.is_err();
            let _ = this.update(cx, |terminal, cx| {
                if !terminal
                    .terminals
                    .complete_resize(&session_id, resize_epoch)
                {
                    return;
                }
                if let Some(state) = terminal.terminals.states.get_mut(&session_id) {
                    state.viewport_dirty = true;
                }
                match result {
                    Ok(()) => {
                        if let Some(state) = terminal.terminals.states.get_mut(&session_id) {
                            state.resize_error = None;
                            state.resize_retry_blocked = false;
                        }
                        if terminal.session_id.as_deref() == Some(session_id.as_str()) {
                            terminal.error = None;
                        }
                    }
                    Err(error) => {
                        tracing::warn!(%error, "workers terminal resize failed");
                        terminal.remote_grids.invalidate(&session_id, cols, rows);
                        if let Some(state) = terminal.terminals.states.get_mut(&session_id) {
                            state.resize_error = Some(error.to_string());
                            state.resize_retry_blocked = true;
                        }
                    }
                }
                cx.notify();
            });
            if failed {
                cx.background_executor()
                    .timer(Duration::from_millis(500))
                    .await;
                let _ = this.update(cx, |terminal, cx| {
                    if let Some(state) = terminal.terminals.states.get_mut(&session_id) {
                        state.resize_retry_blocked = false;
                        cx.notify();
                    }
                });
            }
        })
        .detach();
    }

    pub fn active_grid_snapshot(&self) -> Option<GridSnapshot> {
        let state = self.active_state()?;
        Some(GridSnapshot {
            lines: state.emulator.lines(),
            cursor: state.emulator.cursor(),
        })
    }

    /// Release the local terminal history while preserving the hosted PTY and session.
    /// The next poll asks the worker host for a fresh viewport, so no agent is stopped.
    pub fn shed_scrollback(&mut self, include_active: bool, cx: &mut Context<Self>) {
        self.terminals.shed_scrollback(include_active);
        cx.notify();
    }

    fn queue_input(&mut self, bytes: &[u8], cx: &mut Context<Self>) {
        if self.session_id.is_none() || !self.coalescer.push(bytes) {
            return;
        }
        self.flush_task = Some(Self::schedule_flush(cx));
    }

    pub fn insert_text(&mut self, text: &str, cx: &mut Context<Self>) {
        self.queue_input(text.as_bytes(), cx);
    }

    fn schedule_flush(cx: &mut Context<Self>) -> Task<()> {
        cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(COALESCE_MS))
                .await;
            let _ = this.update(cx, |terminal, cx| terminal.flush_input(cx));
        })
    }

    fn flush_input(&mut self, cx: &mut Context<Self>) {
        let Some(session_id) = self.session_id.clone() else {
            return;
        };
        let generation = self.generation;
        let bytes = self.coalescer.take();
        if bytes.is_empty() {
            return;
        }
        let data = String::from_utf8_lossy(&bytes).into_owned();
        let client = self.client.clone();
        let request_session_id = session_id.clone();
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { client.write(&request_session_id, &data) })
                .await;
            if let Err(error) = result {
                let _ = this.update(cx, |terminal, cx| {
                    if terminal.generation != generation
                        || terminal.session_id.as_deref() != Some(session_id.as_str())
                    {
                        return;
                    }
                    terminal.error = Some(error.to_string());
                    cx.notify();
                });
            }
        })
        .detach();
    }

    fn on_key_down(&mut self, event: &KeyDownEvent, _window: &mut Window, cx: &mut Context<Self>) {
        let keystroke = &event.keystroke;
        let modifiers = &keystroke.modifiers;
        if keystroke.key == "v" && (modifiers.platform || (modifiers.control && modifiers.shift)) {
            if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
                let bracketed = self
                    .active_state()
                    .is_some_and(|state| state.emulator.bracketed_paste_mode());
                let bytes = paste_bytes(&text, bracketed);
                self.queue_input(&bytes, cx);
                cx.stop_propagation();
            }
            return;
        }
        if keystroke.key == "c"
            && (modifiers.platform || (modifiers.control && modifiers.shift))
            && self.copy_selection(cx)
        {
            cx.stop_propagation();
            return;
        }
        // Incremental output is fed directly into the emulator between full
        // viewport snapshots, so its cursor mode is the freshest source.
        let application_cursor = self
            .active_state()
            .is_some_and(|state| state.emulator.app_cursor_mode());
        if let Some(bytes) = keystroke_bytes(
            &keystroke.key,
            keystroke.key_char.as_deref(),
            modifiers,
            application_cursor,
        ) {
            self.queue_input(&bytes, cx);
            cx.stop_propagation();
        }
    }

    fn scroll(&mut self, lines: i32, cx: &mut Context<Self>) {
        if let Some(state) = self.active_state_mut() {
            state.emulator.scroll(lines);
            cx.notify();
        }
    }

    fn active_scrollbar_metrics(&self) -> Option<ScrollbarMetrics> {
        let geometry = self.geometry?;
        let state = self.active_state()?;
        scrollbar_metrics(
            geometry.bounds,
            state.emulator.rows(),
            state.emulator.history_lines(),
            state.emulator.display_offset(),
        )
    }

    fn scrollbar_to_pointer(
        &mut self,
        pointer_y: Pixels,
        grab_offset: f32,
        cx: &mut Context<Self>,
    ) {
        let Some(metrics) = self.active_scrollbar_metrics() else {
            return;
        };
        let offset = metrics.offset_for_pointer(pointer_y, grab_offset);
        if let Some(state) = self.active_state_mut() {
            state.emulator.scroll_to_offset(offset);
            cx.notify();
        }
    }

    fn on_scrollbar_mouse_down(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(metrics) = self.active_scrollbar_metrics() else {
            return;
        };
        window.focus(&self.focus_handle, cx);
        let pointer_on_track = f32::from(event.position.y) - metrics.track_top;
        let grab_offset = if (metrics.thumb_top..=metrics.thumb_top + metrics.thumb_height)
            .contains(&pointer_on_track)
        {
            pointer_on_track - metrics.thumb_top
        } else {
            metrics.thumb_height / 2.0
        };
        self.scrollbar_drag = Some(ScrollbarDrag { grab_offset });
        self.scrollbar_to_pointer(event.position.y, grab_offset, cx);
        cx.stop_propagation();
    }

    fn on_terminal_hover(&mut self, hovered: &bool, _window: &mut Window, cx: &mut Context<Self>) {
        if self.terminal_hovered != *hovered {
            self.terminal_hovered = *hovered;
            if !*hovered {
                self.scrollbar_hovered = false;
            }
            cx.notify();
        }
    }

    fn jump_to_bottom(&mut self, cx: &mut Context<Self>) {
        let retry_tui_jump = self.tui_jump_hint_active();
        if retry_tui_jump {
            if let Some(state) = self.active_state_mut() {
                state.tui_jump_suppressed = true;
            }
            self.send_tui_jump_key(cx);
        }
        if self.terminals.jump_to_bottom() {
            cx.notify();
        }
        if retry_tui_jump {
            let Some(jump_session_id) = self.session_id.clone() else {
                return;
            };
            let jump_generation = self.generation;
            cx.spawn(async move |this, cx| {
                cx.background_executor()
                    .timer(Duration::from_millis(500))
                    .await;
                let _ = this.update(cx, |terminal, cx| {
                    let hint_remains = terminal
                        .terminals
                        .states
                        .get_mut(&jump_session_id)
                        .is_some_and(|state| {
                            state.tui_jump_suppressed = false;
                            state.has_tui_jump_hint()
                        });
                    if hint_remains
                        && terminal.generation == jump_generation
                        && terminal.session_id.as_deref() == Some(jump_session_id.as_str())
                    {
                        terminal.send_tui_jump_key(cx);
                    }
                    cx.notify();
                });
            })
            .detach();
        }
    }

    fn tui_jump_hint_active(&self) -> bool {
        self.active_state()
            .is_some_and(WorkersTerminalState::has_tui_jump_hint)
    }

    fn send_tui_jump_key(&mut self, cx: &mut Context<Self>) {
        let application_cursor = self
            .active_state()
            .is_some_and(|state| state.emulator.app_cursor_mode());
        let modifiers = gpui::Modifiers {
            control: true,
            ..gpui::Modifiers::default()
        };
        if let Some(bytes) = keystroke_bytes("end", None, &modifiers, application_cursor) {
            self.queue_input(&bytes, cx);
        }
    }

    fn render_scrollbar(
        &self,
        theme: &crate::theme::Theme,
        cx: &mut Context<Self>,
    ) -> Option<gpui::AnyElement> {
        if !self.terminal_hovered {
            return None;
        }
        let metrics = self.active_scrollbar_metrics()?;
        let thumb_width = if self.scrollbar_hovered {
            SCROLLBAR_HOVER_THUMB_WIDTH
        } else {
            SCROLLBAR_THUMB_WIDTH
        };
        Some(
            div()
                .id("workers-terminal-scrollbar")
                .absolute()
                .top(px(0.0))
                .bottom(px(0.0))
                .right(px(0.0))
                .w(px(SCROLLBAR_HIT_WIDTH))
                .cursor_pointer()
                .on_hover(cx.listener(|this, hovered: &bool, _, cx| {
                    if this.scrollbar_hovered != *hovered {
                        this.scrollbar_hovered = *hovered;
                        cx.notify();
                    }
                }))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(Self::on_scrollbar_mouse_down),
                )
                .child(
                    div()
                        .absolute()
                        .top(px(SCROLLBAR_TRACK_INSET + metrics.thumb_top))
                        .right(px(2.0))
                        .w(px(thumb_width))
                        .h(px(metrics.thumb_height))
                        .rounded(px(thumb_width / 2.0))
                        .bg(theme.text_faint.opacity(0.52)),
                )
                .into_any_element(),
        )
    }

    fn render_jump_to_bottom(&self, cx: &mut Context<Self>) -> Option<gpui::AnyElement> {
        let scrolled_up = self
            .active_state()
            .is_some_and(|state| state.emulator.display_offset() > 0);
        let tui_jump_visible = self
            .active_state()
            .is_some_and(|state| state.has_tui_jump_hint() && !state.tui_jump_suppressed);
        if !scrolled_up && !tui_jump_visible {
            return None;
        }
        let theme = crate::theme::Theme::of(cx).clone();
        Some(
            div()
                .id("workers-terminal-jump-to-bottom")
                .absolute()
                .bottom(px(14.0))
                .left_0()
                .right_0()
                .flex()
                .justify_center()
                .child(
                    div()
                        .id("workers-terminal-jump-pill")
                        .h(px(30.0))
                        .px(px(12.0))
                        .rounded_full()
                        .border_1()
                        .border_color(theme.border)
                        .shadow_md()
                        .cursor_pointer()
                        .bg(theme.surface_raised)
                        .flex()
                        .items_center()
                        .gap(px(6.0))
                        .text_size(px(12.0))
                        .text_color(theme.text_muted)
                        .on_click(cx.listener(|this, _, _, cx| this.jump_to_bottom(cx)))
                        .child(
                            crate::icons::icon(crate::icons::ARROW_DOWN)
                                .size(px(14.0))
                                .text_color(theme.text_muted),
                        )
                        .child("Scroll to bottom"),
                )
                .into_any_element(),
        )
    }

    fn cell_hit_at(&self, position: gpui::Point<Pixels>) -> Option<crate::terminal::view::CellHit> {
        let geometry = self.geometry?;
        Some(cell_at(
            f32::from(position.x - geometry.origin.x),
            f32::from(position.y - geometry.origin.y),
            geometry.cell_w,
            geometry.line_h,
            geometry.cols as usize,
            geometry.rows as usize,
        ))
    }

    fn grid_point_at(&mut self, position: gpui::Point<Pixels>) -> Option<(GridPoint, Side)> {
        let hit = self.cell_hit_at(position)?;
        Some((
            self.active_state_mut()?
                .emulator
                .grid_point(hit.row, hit.col),
            hit.side,
        ))
    }

    fn on_mouse_down(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        window.focus(&self.focus_handle, cx);
        let input_modes = self
            .active_state()
            .map_or(WorkersViewportInputModes::default(), |state| {
                state.input_modes
            });
        if input_modes.known && input_modes.mouse_reporting {
            let Some(hit) = self.cell_hit_at(event.position) else {
                return;
            };
            if let Some(state) = self.active_state_mut() {
                state.selection_drag = None;
            }
            self.queue_input(
                &mouse_report_bytes(MouseReportKind::Down, hit.col, hit.row, event.modifiers),
                cx,
            );
            return;
        }
        let Some((point, side)) = self.grid_point_at(event.position) else {
            return;
        };
        let ty = match event.click_count {
            0 => return,
            1 => SelectionType::Simple,
            2 => SelectionType::Semantic,
            _ => SelectionType::Lines,
        };
        let Some(state) = self.active_state_mut() else {
            return;
        };
        if ty == SelectionType::Simple {
            if event.modifiers.shift && state.emulator.has_selection() {
                state.emulator.update_selection(point, side);
                state.selection_drag = Some(SelectionDrag {
                    origin: event.position,
                    armed: true,
                });
            } else {
                state.emulator.clear_selection();
                state.selection_drag = Some(SelectionDrag {
                    origin: event.position,
                    armed: false,
                });
            }
        } else {
            state.emulator.start_selection(ty, point, side);
            state.selection_drag = Some(SelectionDrag {
                origin: event.position,
                armed: true,
            });
        }
        cx.notify();
    }

    fn on_mouse_move(
        &mut self,
        event: &MouseMoveEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(drag) = self.scrollbar_drag {
            if event.dragging() {
                self.scrollbar_to_pointer(event.position.y, drag.grab_offset, cx);
            } else {
                self.scrollbar_drag = None;
            }
            return;
        }
        let input_modes = self
            .active_state()
            .map_or(WorkersViewportInputModes::default(), |state| {
                state.input_modes
            });
        if input_modes.known && input_modes.mouse_reporting {
            let kind = match event.pressed_button {
                Some(MouseButton::Left) if input_modes.mouse_button_motion => MouseReportKind::Drag,
                None if input_modes.mouse_any_motion => MouseReportKind::Move,
                _ => return,
            };
            let Some(hit) = self.cell_hit_at(event.position) else {
                return;
            };
            self.queue_input(
                &mouse_report_bytes(kind, hit.col, hit.row, event.modifiers),
                cx,
            );
            return;
        }
        if !event.dragging() {
            return;
        }
        let Some(drag) = self.active_state().and_then(|state| state.selection_drag) else {
            return;
        };
        if !drag.armed {
            let dx = f32::from(event.position.x - drag.origin.x);
            let dy = f32::from(event.position.y - drag.origin.y);
            if dx.hypot(dy) < SELECTION_DRAG_THRESHOLD {
                return;
            }
            let Some((anchor, side)) = self.grid_point_at(drag.origin) else {
                return;
            };
            if let Some(state) = self.active_state_mut() {
                state
                    .emulator
                    .start_selection(SelectionType::Simple, anchor, side);
                state.selection_drag = Some(SelectionDrag {
                    armed: true,
                    ..drag
                });
            }
        }
        let Some((point, side)) = self.grid_point_at(event.position) else {
            return;
        };
        if let Some(state) = self.active_state_mut() {
            state.emulator.update_selection(point, side);
        }
        cx.notify();
    }

    fn on_mouse_up(&mut self, event: &MouseUpEvent, _window: &mut Window, cx: &mut Context<Self>) {
        let input_modes = self
            .active_state()
            .map_or(WorkersViewportInputModes::default(), |state| {
                state.input_modes
            });
        if input_modes.known && input_modes.mouse_reporting {
            if let Some(hit) = self.cell_hit_at(event.position) {
                self.queue_input(
                    &mouse_report_bytes(MouseReportKind::Up, hit.col, hit.row, event.modifiers),
                    cx,
                );
            }
        }
        if let Some(state) = self.active_state_mut() {
            state.selection_drag = None;
        }
        self.scrollbar_drag = None;
    }

    fn on_scroll_wheel(&mut self, event: &gpui::ScrollWheelEvent, cx: &mut Context<Self>) {
        let Some(hit) = self.cell_hit_at(event.position) else {
            return;
        };
        let Some(state) = self.active_state_mut() else {
            return;
        };
        let steps =
            state
                .scroll_gesture
                .steps(event.delta, event.touch_phase, px(TERM_LINE_HEIGHT));
        if steps == 0 {
            return;
        }
        let action = scroll_action(
            state.input_modes,
            state.mouse_protocol,
            steps,
            hit.col,
            hit.row,
        );
        match action {
            TerminalScrollAction::Write(bytes) => self.queue_input(&bytes, cx),
            TerminalScrollAction::Scrollback => self.scroll(steps, cx),
        }
    }

    fn copy_selection(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(text) = self
            .active_state_mut()
            .and_then(|state| state.emulator.selection_text())
        else {
            return false;
        };
        cx.write_to_clipboard(ClipboardItem::new_string(text));
        true
    }
}

impl Render for WorkersTerminal {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if std::mem::take(&mut self.focus_pending) {
            window.focus(&self.focus_handle, cx);
        }
        let focused = self.focus_handle.is_focused(window);
        let error = self
            .active_state()
            .and_then(|state| state.resize_error.clone())
            .or_else(|| self.error.clone());
        let theme = crate::theme::Theme::of(cx).clone();
        // Suprimir o notify do replay nao basta: qualquer outro repaint da
        // janela pinta o emulador no meio do backlog, e o usuario ve o
        // terminal rolando do topo. A grade fica coberta ate o catch-up
        // drenar — o TerminalElement continua montado porque e ele que mede a
        // geometria, e o replay so consome output depois que a grade existe.
        let catching_up = self
            .active_state()
            .is_some_and(|state| state.historical_replay.is_catching_up());
        let scrollbar = self.render_scrollbar(&theme, cx);
        let jump_to_bottom = self.render_jump_to_bottom(cx);
        div()
            .id("workers-terminal")
            .role(gpui::Role::Terminal)
            .size_full()
            .relative()
            .key_context("Terminal")
            .track_focus(&self.focus_handle)
            .on_hover(cx.listener(Self::on_terminal_hover))
            .on_key_down(cx.listener(Self::on_key_down))
            .on_mouse_down(MouseButton::Left, cx.listener(Self::on_mouse_down))
            .on_mouse_move(cx.listener(Self::on_mouse_move))
            .on_mouse_up_out(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .on_scroll_wheel(cx.listener(|this, event, _, cx| this.on_scroll_wheel(event, cx)))
            .child(TerminalElement::new_workers(cx.entity(), focused))
            .children(scrollbar)
            .children(jump_to_bottom)
            .when(catching_up, |el| {
                el.child(
                    div()
                        .absolute()
                        .inset_0()
                        .bg(crate::terminal::view::terminal_panel_bg(&theme))
                        .flex()
                        .items_center()
                        .justify_center()
                        .text_size(px(11.0))
                        .text_color(theme.text_faint)
                        .child("Loading history…"),
                )
            })
            .when_some(error, |el, error| {
                el.child(
                    div()
                        .absolute()
                        .top(px(8.0))
                        .right(px(8.0))
                        .max_w(px(420.0))
                        .px(px(8.0))
                        .py(px(5.0))
                        .rounded(px(7.0))
                        .bg(crate::theme::ink(0.82))
                        .text_size(px(10.0))
                        .text_color(crate::theme::Theme::of(cx).danger_muted)
                        .child(format!("Worker terminal disconnected: {error}")),
                )
            })
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };

    use gpui::Modifiers;
    use zeron_workers_unpeel::WorkersViewportInputModes;

    use crate::terminal::view::paste_bytes;

    use super::{
        HistoricalReplay, MAX_SILENT_REPLAY_CHUNKS, MouseProtocol, MouseReportKind,
        RemoteGridTracker, ResizeSync, RetainedWorkerTerminals, TerminalRefresh,
        TerminalScrollAction, WorkersTerminalView, mouse_report_bytes, scroll_action, should_paint,
        terminal_refresh, viewport_has_tui_jump_hint,
    };

    #[test]
    fn retained_session_switch_preserves_history_and_view_position() {
        let mut terminals = RetainedWorkerTerminals::default();

        terminals.select(Some("a".into()), 8, 2);
        let a = terminals.active_mut().unwrap();
        a.emulator.feed(b"one\ntwo\nthree\nfour\n");
        a.emulator.scroll(2);
        let a_history = a.emulator.history_lines();
        let a_offset = a.emulator.display_offset();

        terminals.select(Some("b".into()), 8, 2);
        terminals
            .active_mut()
            .unwrap()
            .emulator
            .feed(b"other\nterminal\n");
        terminals.select(Some("a".into()), 8, 2);

        let a = terminals.active().unwrap();
        assert_eq!(a.emulator.history_lines(), a_history);
        assert_eq!(a.emulator.display_offset(), a_offset);
    }

    #[test]
    fn retained_resize_and_new_output_do_not_steal_scrolled_view() {
        let mut terminals = RetainedWorkerTerminals::default();
        terminals.select(Some("a".into()), 8, 2);
        let a = terminals.active_mut().unwrap();
        a.emulator.feed(b"one\ntwo\nthree\nfour\n");
        a.emulator.scroll(2);
        let offset = a.emulator.display_offset();

        a.emulator.resize(12, 3);
        a.emulator.feed(b"five\n");

        assert!(a.emulator.history_lines() > 0);
        assert_eq!(a.emulator.display_offset(), offset);
    }

    #[test]
    fn jump_to_bottom_restores_live_tail_for_only_the_active_session() {
        let mut terminals = RetainedWorkerTerminals::default();
        terminals.select(Some("a".into()), 8, 2);
        terminals
            .active_mut()
            .unwrap()
            .emulator
            .feed(b"one\ntwo\nthree\nfour\n");
        terminals.active_mut().unwrap().emulator.scroll(2);

        terminals.select(Some("b".into()), 8, 2);
        terminals
            .active_mut()
            .unwrap()
            .emulator
            .feed(b"five\nsix\nseven\neight\n");
        terminals.active_mut().unwrap().emulator.scroll(1);
        assert!(terminals.jump_to_bottom());
        assert_eq!(terminals.active().unwrap().emulator.display_offset(), 0);

        terminals.select(Some("a".into()), 8, 2);
        assert_eq!(terminals.active().unwrap().emulator.display_offset(), 2);
    }

    #[test]
    fn retained_stream_bootstrap_keeps_history_beyond_the_visible_snapshot() {
        let mut state = super::WorkersTerminalState::new(8, 2);
        let output = zeron_workers_unpeel::WorkersOutput {
            offset: 0,
            next_offset: 24,
            data: b"one\ntwo\nthree\nfour\nfive\n".to_vec(),
            truncated: false,
        };
        let viewport = zeron_workers_unpeel::WorkersViewport {
            // The host snapshot can race ahead of the exact output chunk.
            // Its screen is useful for modes, but it must never advance the
            // byte cursor past data that was not fed into the emulator.
            output_offset: 40,
            cols: 8,
            rows: 2,
            ansi: b"four\nfive".to_vec(),
            input_modes: WorkersViewportInputModes {
                known: true,
                ..WorkersViewportInputModes::default()
            },
        };

        state.apply_refresh(output, Some(viewport));

        assert!(state.emulator.history_lines() >= 3);
        assert_eq!(state.offset, 24);
        assert!(state.input_modes.known);
    }

    #[test]
    fn truncated_rebase_preserves_the_last_known_mouse_protocol() {
        let mut state = super::WorkersTerminalState::new(8, 2);
        state.apply_refresh(
            zeron_workers_unpeel::WorkersOutput {
                offset: 0,
                next_offset: 10,
                data: b"\x1b[?1000h\x1b[?1005h".to_vec(),
                truncated: false,
            },
            None,
        );
        assert_eq!(state.mouse_protocol, MouseProtocol::Utf8);

        state.apply_refresh(
            zeron_workers_unpeel::WorkersOutput {
                offset: 20,
                next_offset: 21,
                data: b"x".to_vec(),
                truncated: true,
            },
            Some(zeron_workers_unpeel::WorkersViewport {
                output_offset: 21,
                cols: 8,
                rows: 2,
                ansi: Vec::new(),
                input_modes: WorkersViewportInputModes {
                    known: true,
                    mouse_reporting: true,
                    ..WorkersViewportInputModes::default()
                },
            }),
        );

        assert_eq!(state.mouse_protocol, MouseProtocol::Utf8);
    }

    #[test]
    fn resize_completion_releases_the_original_session_after_a_switch() {
        let mut terminals = RetainedWorkerTerminals::default();
        terminals.select(Some("a".into()), 8, 2);
        let epoch = terminals.active_mut().unwrap().resize_sync.start();

        terminals.select(Some("b".into()), 8, 2);
        assert!(terminals.complete_resize("a", epoch));

        terminals.select(Some("a".into()), 8, 2);
        assert!(!terminals.active().unwrap().resize_sync.pending());
    }

    #[test]
    fn tui_jump_hint_matches_only_the_reference_tail_shapes() {
        assert!(viewport_has_tui_jump_hint(
            "answer\nJump to bottom (ctrl+End)"
        ));
        assert!(viewport_has_tui_jump_hint(
            "answer\n3 new messages (ctrl+End) ↓"
        ));
        assert!(viewport_has_tui_jump_hint(
            "answer\n1 new message (ctrl+End)"
        ));

        let quoted_above_tail = format!(
            "Jump to bottom (ctrl+End)\n{}",
            (0..16).map(|_| "ordinary").collect::<Vec<_>>().join("\n")
        );
        assert!(!viewport_has_tui_jump_hint(&quoted_above_tail));
        assert!(!viewport_has_tui_jump_hint("ctrl+End"));
    }

    #[test]
    fn normal_memory_trim_preserves_the_active_scrollback() {
        let mut terminals = RetainedWorkerTerminals::default();
        for session in ["a", "b"] {
            terminals.select(Some(session.into()), 8, 2);
            terminals
                .active_mut()
                .unwrap()
                .emulator
                .feed(b"one\ntwo\nthree\nfour\n");
        }
        terminals.select(Some("a".into()), 8, 2);

        terminals.shed_scrollback(false);

        assert!(terminals.states["a"].emulator.history_lines() > 0);
        assert_eq!(terminals.states["b"].emulator.history_lines(), 0);

        terminals.shed_scrollback(true);
        assert_eq!(terminals.states["a"].emulator.history_lines(), 0);
    }

    struct ViewDropProbe {
        view_drops: Arc<AtomicUsize>,
        _lifecycle_mutations: Arc<AtomicUsize>,
    }

    impl Drop for ViewDropProbe {
        fn drop(&mut self) {
            self.view_drops.fetch_add(1, Ordering::SeqCst);
        }
    }

    #[test]
    fn workers_terminal_detach_is_view_only() {
        let view_drops = Arc::new(AtomicUsize::new(0));
        let lifecycle_mutations = Arc::new(AtomicUsize::new(0));
        let view = WorkersTerminalView::new(
            "session-1",
            ViewDropProbe {
                view_drops: Arc::clone(&view_drops),
                _lifecycle_mutations: Arc::clone(&lifecycle_mutations),
            },
        );

        view.detach();

        assert_eq!(view_drops.load(Ordering::SeqCst), 1);
        assert_eq!(lifecycle_mutations.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn terminal_refresh_reuses_the_existing_emulator_for_incremental_output() {
        assert_eq!(
            terminal_refresh(false, false, true),
            TerminalRefresh::Incremental
        );
        assert_eq!(terminal_refresh(false, false, false), TerminalRefresh::Idle);
        assert_eq!(
            terminal_refresh(true, false, true),
            TerminalRefresh::Snapshot
        );
        assert_eq!(
            terminal_refresh(false, true, true),
            TerminalRefresh::Snapshot
        );
    }

    #[test]
    fn terminal_generation_rejects_a_previous_session_cycle() {
        let a_first = 1_u64;
        let b = a_first.wrapping_add(1);
        let a_second = b.wrapping_add(1);
        assert_ne!(a_first, a_second);
    }

    #[test]
    fn returning_to_a_session_on_the_same_grid_does_not_resize_its_pty_again() {
        let mut grids = RemoteGridTracker::default();

        assert!(grids.record_resize("session-a", 180, 48));
        assert!(grids.record_resize("session-b", 180, 48));
        assert!(!grids.record_resize("session-a", 180, 48));
        assert!(grids.record_resize("session-a", 200, 48));
    }

    #[test]
    fn failed_remote_resize_can_retry_the_same_grid() {
        let mut grids = RemoteGridTracker::default();

        assert!(grids.record_resize("session-a", 180, 48));
        grids.invalidate("session-a", 180, 48);

        assert!(grids.record_resize("session-a", 180, 48));
    }

    #[test]
    fn historical_replay_waits_for_the_visible_grid_before_consuming_output() {
        let mut replay = HistoricalReplay::default();

        replay.start();
        assert!(!replay.can_consume_output());

        replay.observe_geometry();
        assert!(replay.can_consume_output());

        replay.start();
        assert!(!replay.can_consume_output());
    }

    /// Abrir uma sessao longa pintava cada chunk do backlog: o usuario via o
    /// terminal rolar do topo ate o fim antes de chegar onde ele queria.
    #[test]
    fn a_long_backlog_paints_one_frame_at_the_end_not_the_whole_scroll() {
        let mut replay = HistoricalReplay::default();
        replay.start();
        replay.observe_geometry();

        // Chunks do backlog: alimentam o emulador, nenhum vira quadro.
        for _ in 0..5 {
            let was = replay.is_catching_up();
            replay.observe_output(true);
            assert!(was && replay.is_catching_up());
            assert!(!should_paint(was, replay.is_catching_up(), true, false));
        }

        // A leitura vazia drena o backlog: ESTE e o quadro que o usuario pediu.
        let was = replay.is_catching_up();
        replay.observe_output(false);
        assert!(!replay.is_catching_up());
        assert!(should_paint(was, replay.is_catching_up(), false, false));

        // Dali em diante, output ao vivo pinta normalmente.
        replay.observe_output(true);
        assert!(should_paint(false, replay.is_catching_up(), true, false));
    }

    /// Uma sessao viva e tagarela pode nunca drenar; sem teto a tela ficaria
    /// vazia pra sempre.
    #[test]
    fn a_backlog_that_never_drains_starts_painting_again() {
        let mut replay = HistoricalReplay::default();
        replay.start();
        replay.observe_geometry();

        for _ in 0..MAX_SILENT_REPLAY_CHUNKS {
            assert!(replay.is_catching_up());
            replay.observe_output(true);
        }

        assert!(!replay.is_catching_up(), "o teto solta o paint");
        assert!(should_paint(false, replay.is_catching_up(), true, false));
    }

    #[test]
    fn resize_sync_releases_only_the_latest_resize() {
        let mut sync = ResizeSync::default();
        let resize = sync.start();

        assert!(sync.pending());
        assert!(sync.complete(resize));
        assert!(!sync.pending());
    }

    #[test]
    fn stale_resize_completion_does_not_release_a_newer_resize() {
        let mut sync = ResizeSync::default();
        let stale_resize = sync.start();
        let current_resize = sync.start();

        assert!(!sync.complete(stale_resize));
        assert!(sync.pending());
        assert!(sync.complete(current_resize));
        assert!(!sync.pending());
    }

    #[test]
    fn sgr_mouse_encoder_uses_one_based_cells_and_modifiers() {
        let modifiers = Modifiers {
            shift: true,
            alt: true,
            control: true,
            ..Modifiers::default()
        };

        assert_eq!(
            mouse_report_bytes(MouseReportKind::Down, 0, 0, modifiers),
            b"\x1b[<28;1;1M"
        );
        assert_eq!(
            mouse_report_bytes(MouseReportKind::Drag, 7, 4, Modifiers::default()),
            b"\x1b[<32;8;5M"
        );
        assert_eq!(
            mouse_report_bytes(MouseReportKind::Up, 7, 4, Modifiers::default()),
            b"\x1b[<0;8;5m"
        );
    }

    #[test]
    fn wheel_routing_repeats_every_step_and_never_swallows_history() {
        let mut modes = WorkersViewportInputModes {
            known: true,
            mouse_reporting: true,
            ..WorkersViewportInputModes::default()
        };
        assert_eq!(
            scroll_action(modes, MouseProtocol::Sgr, 3, 3, 2),
            TerminalScrollAction::Write(b"\x1b[<64;4;3M\x1b[<64;4;3M\x1b[<64;4;3M".to_vec())
        );

        modes.mouse_reporting = false;
        modes.alternate_screen = true;
        modes.mouse_alternate_scroll = true;
        modes.application_cursor = true;
        assert_eq!(
            scroll_action(modes, MouseProtocol::Sgr, -3, 3, 2),
            TerminalScrollAction::Write(b"\x1bOB\x1bOB\x1bOB".to_vec())
        );

        modes.mouse_alternate_scroll = false;
        assert_eq!(
            scroll_action(modes, MouseProtocol::Sgr, 2, 3, 2),
            TerminalScrollAction::Scrollback
        );

        modes.alternate_screen = false;
        assert_eq!(
            scroll_action(modes, MouseProtocol::Sgr, 2, 3, 2),
            TerminalScrollAction::Scrollback
        );
    }

    #[test]
    fn bracketed_paste_preserves_multiline_text_exactly() {
        assert_eq!(
            paste_bytes("first\nsecond\n", true),
            b"\x1b[200~first\nsecond\n\x1b[201~"
        );
    }
}
