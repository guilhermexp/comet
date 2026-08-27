//! Terminal viewport rendering over libghostty-vt.
//!
//! The host keeps a live parsed viewport per session (see `session_host`) and
//! renders one-shot replays for remote clients. Both paths run on the vendored
//! libghostty-vt engine (`vendor/ghostty-vt/`) — the exact same VT
//! implementation that renders the terminal on desktop and phone (GhosttyKit)
//! — so `read_screen`, `wait_for_text`, and menu-prompt detection see the
//! screen the user sees, not an approximation. The previous hand-rolled
//! parser lived here until 2026-07-09.
//!
//! The JSON snapshot shape (`TerminalViewportSnapshot`) is a wire contract
//! with remote clients and the `__viewport__` CLI; keep it stable.

use crate::ghostty_vt as vt;
use crate::session_host::{output_path, read_output_chunk, request_terminal_viewport_snapshot};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

const DEFAULT_VIEWPORT_REPLAY_MAX_BYTES: usize = 512 * 1024;
const MAX_VIEWPORT_REPLAY_MAX_BYTES: usize = 4 * 1024 * 1024;

/// Scrollback page-storage budget per virtual terminal. ghostty's
/// `max_scrollback` is in **bytes** (rounded up to page granularity; the
/// header comment saying "lines" is an upstream doc bug — see
/// `Screen.zig`), so this bounds memory directly. Replaces the old
/// parser's 2000-row cap; at typical widths this retains a comparable
/// few thousand rows of history.
const MAX_VIEWPORT_SCROLLBACK_BYTES: usize = 4 * 1024 * 1024;

/// Bytes of raw output retained per live viewport so `snapshot_resized` can
/// re-render at a different grid size with real reflow (ghostty reflow is not
/// reversible in place, so resized snapshots replay into a fresh terminal).
const RESIZE_REPLAY_MAX_BYTES: usize = DEFAULT_VIEWPORT_REPLAY_MAX_BYTES;

/// Nominal cell pixel size passed to ghostty resize (only image protocols and
/// size reports observe it; text rendering does not).
const CELL_WIDTH_PX: u32 = 8;
const CELL_HEIGHT_PX: u32 = 16;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalViewportStyleRun {
    pub start: u16,
    pub len: u16,
    pub fg: Option<String>,
    pub bg: Option<String>,
    pub bold: bool,
    pub inverse: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalViewportRow {
    pub text: String,
    pub styles: Vec<TerminalViewportStyleRun>,
    /// This physical row continues on the next row without a hard newline.
    /// Clipboard extraction uses it to unwrap terminal line wrapping.
    #[serde(default)]
    pub wrapped: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalViewportSnapshot {
    pub cols: u16,
    pub rows: u16,
    pub output_offset: u64,
    pub truncated: bool,
    pub cursor_row: u16,
    pub cursor_col: u16,
    pub scrollback_rows: u32,
    pub viewport_start_row: u32,
    pub scroll_offset_rows: u32,
    /// False only when decoding a snapshot from a pre-input-modes host.
    #[serde(default)]
    pub input_modes_known: bool,
    /// Whether the child currently owns mouse button reports (DEC 9, 1000,
    /// 1002, or 1003). TUI clients use this to distinguish a clickable
    /// full-screen app from ordinary terminal text that should drag-select.
    #[serde(default)]
    pub mouse_reporting: bool,
    #[serde(default)]
    pub mouse_button_motion: bool,
    #[serde(default)]
    pub mouse_any_motion: bool,
    /// Input modes needed to route wheel gestures like a real terminal.
    #[serde(default)]
    pub alternate_screen: bool,
    #[serde(default)]
    pub mouse_alternate_scroll: bool,
    #[serde(default)]
    pub application_cursor: bool,
    pub viewport_rows: Vec<TerminalViewportRow>,
}

/// Per-cell style in the snapshot's string encoding: `ansi:N` (palette 0-15),
/// `ansi256:N`, or `rgb:r,g,b`. Matches what the old parser emitted.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct CellStyle {
    fg: Option<String>,
    bg: Option<String>,
    bold: bool,
    inverse: bool,
}

/// Owned libghostty-vt handles for one virtual terminal. Not thread-safe by
/// itself; every user wraps it in a `Mutex` (the live per-session viewport,
/// the replay cache).
struct VtTerminal {
    term: vt::GhosttyTerminal,
    render: vt::GhosttyRenderState,
    row_iter: vt::GhosttyRenderStateRowIterator,
    cells: vt::GhosttyRenderStateRowCells,
    cols: u16,
    rows: u16,
}

// The raw handles are only ever touched through &mut self (or &self getters
// that ghostty documents as pure reads), and all shared use is Mutex-guarded.
unsafe impl Send for VtTerminal {}

impl Drop for VtTerminal {
    fn drop(&mut self) {
        unsafe {
            vt::ghostty_render_state_row_cells_free(self.cells);
            vt::ghostty_render_state_row_iterator_free(self.row_iter);
            vt::ghostty_render_state_free(self.render);
            vt::ghostty_terminal_free(self.term);
        }
    }
}

impl VtTerminal {
    fn new(cols: u16, rows: u16) -> Self {
        let cols = cols.max(1);
        let rows = rows.max(1);
        let options = vt::GhosttyTerminalOptions {
            cols,
            rows,
            max_scrollback: MAX_VIEWPORT_SCROLLBACK_BYTES,
        };
        unsafe {
            let mut term: vt::GhosttyTerminal = std::ptr::null_mut();
            assert_eq!(
                vt::ghostty_terminal_new(std::ptr::null(), &mut term, options),
                vt::GHOSTTY_SUCCESS,
                "libghostty-vt terminal allocation failed"
            );
            let mut render: vt::GhosttyRenderState = std::ptr::null_mut();
            assert_eq!(
                vt::ghostty_render_state_new(std::ptr::null(), &mut render),
                vt::GHOSTTY_SUCCESS,
                "libghostty-vt render state allocation failed"
            );
            let mut row_iter: vt::GhosttyRenderStateRowIterator = std::ptr::null_mut();
            assert_eq!(
                vt::ghostty_render_state_row_iterator_new(std::ptr::null(), &mut row_iter),
                vt::GHOSTTY_SUCCESS,
                "libghostty-vt row iterator allocation failed"
            );
            let mut cells: vt::GhosttyRenderStateRowCells = std::ptr::null_mut();
            assert_eq!(
                vt::ghostty_render_state_row_cells_new(std::ptr::null(), &mut cells),
                vt::GHOSTTY_SUCCESS,
                "libghostty-vt row cells allocation failed"
            );
            Self {
                term,
                render,
                row_iter,
                cells,
                cols,
                rows,
            }
        }
    }

    fn write(&mut self, data: &[u8]) {
        unsafe { vt::ghostty_terminal_vt_write(self.term, data.as_ptr(), data.len()) }
    }

    fn resize(&mut self, cols: u16, rows: u16) {
        let cols = cols.max(1);
        let rows = rows.max(1);
        if cols == self.cols && rows == self.rows {
            return;
        }
        unsafe {
            vt::ghostty_terminal_resize(self.term, cols, rows, CELL_WIDTH_PX, CELL_HEIGHT_PX);
        }
        self.cols = cols;
        self.rows = rows;
    }

    fn get_usize(&self, data: std::os::raw::c_int) -> usize {
        let mut out: usize = 0;
        unsafe {
            vt::ghostty_terminal_get(self.term, data, &mut out as *mut usize as *mut _);
        }
        out
    }

    fn get_u16(&self, data: std::os::raw::c_int) -> u16 {
        let mut out: u16 = 0;
        unsafe {
            vt::ghostty_terminal_get(self.term, data, &mut out as *mut u16 as *mut _);
        }
        out
    }

    fn scrollback_rows(&self) -> usize {
        self.get_usize(vt::GHOSTTY_TERMINAL_DATA_SCROLLBACK_ROWS)
    }

    fn cursor(&self) -> (u16, u16) {
        (
            self.get_u16(vt::GHOSTTY_TERMINAL_DATA_CURSOR_X),
            self.get_u16(vt::GHOSTTY_TERMINAL_DATA_CURSOR_Y),
        )
    }

    /// True while the fed stream sits inside a DEC 2026 synchronized update
    /// (`?2026h` seen, `?2026l` not yet) — the app has declared the grid a
    /// work-in-progress. Ghostty tracks the mode; unknown-mode errors read
    /// as false.
    fn synchronized_output(&self) -> bool {
        let mut value = false;
        let rc = unsafe { vt::ghostty_terminal_mode_get(self.term, 2026, &mut value) };
        rc == vt::GHOSTTY_SUCCESS && value
    }

    /// DEC mouse tracking modes. Encoding modes such as 1006 only describe
    /// reports; one of these tracking modes must be active for the child to
    /// own button gestures.
    fn mouse_reporting(&self) -> bool {
        [9, 1000, 1002, 1003].into_iter().any(|mode| {
            let mut value = false;
            let rc = unsafe { vt::ghostty_terminal_mode_get(self.term, mode, &mut value) };
            rc == vt::GHOSTTY_SUCCESS && value
        })
    }

    fn mode(&self, mode: u16) -> bool {
        let mut value = false;
        let rc = unsafe { vt::ghostty_terminal_mode_get(self.term, mode, &mut value) };
        rc == vt::GHOSTTY_SUCCESS && value
    }

    fn alternate_screen(&self) -> bool {
        let mut screen = vt::GHOSTTY_TERMINAL_SCREEN_PRIMARY;
        let rc = unsafe {
            vt::ghostty_terminal_get(
                self.term,
                vt::GHOSTTY_TERMINAL_DATA_ACTIVE_SCREEN,
                &mut screen as *mut _ as *mut _,
            )
        };
        rc == vt::GHOSTTY_SUCCESS && screen == vt::GHOSTTY_TERMINAL_SCREEN_ALTERNATE
    }

    fn scroll_to_row(&mut self, row: usize) {
        unsafe {
            vt::ghostty_terminal_scroll_viewport(
                self.term,
                vt::GhosttyTerminalScrollViewport::row(row),
            );
        }
    }

    fn scroll_bottom(&mut self) {
        unsafe {
            vt::ghostty_terminal_scroll_viewport(
                self.term,
                vt::GhosttyTerminalScrollViewport::bottom(),
            );
        }
    }

    /// Render the current viewport and return its rows.
    fn viewport_rows(&mut self, with_styles: bool) -> Vec<TerminalViewportRow> {
        unsafe {
            vt::ghostty_render_state_update(self.render, self.term);
            if vt::ghostty_render_state_get(
                self.render,
                vt::GHOSTTY_RENDER_STATE_DATA_ROW_ITERATOR,
                &mut self.row_iter as *mut _ as *mut _,
            ) != vt::GHOSTTY_SUCCESS
            {
                return Vec::new();
            }

            let mut rows = Vec::with_capacity(self.rows as usize);
            while vt::ghostty_render_state_row_iterator_next(self.row_iter) {
                let mut wrapped = false;
                let mut raw_row: vt::GhosttyRow = 0;
                if vt::ghostty_render_state_row_get(
                    self.row_iter,
                    vt::GHOSTTY_RENDER_STATE_ROW_DATA_RAW,
                    &mut raw_row as *mut _ as *mut _,
                ) == vt::GHOSTTY_SUCCESS
                {
                    let _ = vt::ghostty_row_get(
                        raw_row,
                        vt::GHOSTTY_ROW_DATA_WRAP,
                        &mut wrapped as *mut _ as *mut _,
                    );
                }
                if vt::ghostty_render_state_row_get(
                    self.row_iter,
                    vt::GHOSTTY_RENDER_STATE_ROW_DATA_CELLS,
                    &mut self.cells as *mut _ as *mut _,
                ) != vt::GHOSTTY_SUCCESS
                {
                    rows.push(TerminalViewportRow {
                        text: String::new(),
                        styles: Vec::new(),
                        wrapped,
                    });
                    continue;
                }
                let mut row = read_row_cells(self.cells, with_styles);
                row.wrapped = wrapped;
                rows.push(row);
            }
            rows
        }
    }
}

/// One rendered cell: its text piece (empty for wide-char spacers, a space
/// for blank cells) and its style.
fn read_cell(
    cells: vt::GhosttyRenderStateRowCells,
    with_styles: bool,
    previous_style: &CellStyle,
) -> (String, CellStyle) {
    unsafe {
        let mut buf = [0u8; 64];
        let mut gbuf = vt::GhosttyBuffer {
            ptr: buf.as_mut_ptr(),
            cap: buf.len(),
            len: 0,
        };
        let result = vt::ghostty_render_state_row_cells_get(
            cells,
            vt::GHOSTTY_RENDER_STATE_ROW_CELLS_DATA_GRAPHEMES_UTF8,
            &mut gbuf as *mut _ as *mut _,
        );

        let text = if result == vt::GHOSTTY_SUCCESS && gbuf.len > 0 {
            String::from_utf8_lossy(&buf[..gbuf.len]).into_owned()
        } else if result == vt::GHOSTTY_OUT_OF_SPACE {
            // Oversized grapheme cluster; retry with a heap buffer.
            let mut heap = vec![0u8; gbuf.len.max(64)];
            let mut retry = vt::GhosttyBuffer {
                ptr: heap.as_mut_ptr(),
                cap: heap.len(),
                len: 0,
            };
            if vt::ghostty_render_state_row_cells_get(
                cells,
                vt::GHOSTTY_RENDER_STATE_ROW_CELLS_DATA_GRAPHEMES_UTF8,
                &mut retry as *mut _ as *mut _,
            ) == vt::GHOSTTY_SUCCESS
            {
                String::from_utf8_lossy(&heap[..retry.len]).into_owned()
            } else {
                " ".to_string()
            }
        } else {
            // No text: blank cell, or the spacer of a wide character. The
            // spacer must not add a column of text; wide-char continuation
            // keeps the head cell's style so style runs span both columns.
            let mut raw: vt::GhosttyCell = 0;
            let mut wide: std::os::raw::c_int = 0;
            if vt::ghostty_render_state_row_cells_get(
                cells,
                vt::GHOSTTY_RENDER_STATE_ROW_CELLS_DATA_RAW,
                &mut raw as *mut _ as *mut _,
            ) == vt::GHOSTTY_SUCCESS
                && vt::ghostty_cell_get(
                    raw,
                    vt::GHOSTTY_CELL_DATA_WIDE,
                    &mut wide as *mut _ as *mut _,
                ) == vt::GHOSTTY_SUCCESS
                && (wide == vt::GHOSTTY_CELL_WIDE_SPACER_TAIL
                    || wide == vt::GHOSTTY_CELL_WIDE_SPACER_HEAD)
            {
                return (String::new(), previous_style.clone());
            }
            " ".to_string()
        };

        if !with_styles {
            return (text, CellStyle::default());
        }

        let mut cell_style = CellStyle::default();
        let mut has_styling = false;
        if vt::ghostty_render_state_row_cells_get(
            cells,
            vt::GHOSTTY_RENDER_STATE_ROW_CELLS_DATA_HAS_STYLING,
            &mut has_styling as *mut _ as *mut _,
        ) == vt::GHOSTTY_SUCCESS
            && has_styling
        {
            let mut style = vt::GhosttyStyle::zeroed_sized();
            if vt::ghostty_render_state_row_cells_get(
                cells,
                vt::GHOSTTY_RENDER_STATE_ROW_CELLS_DATA_STYLE,
                &mut style as *mut _ as *mut _,
            ) == vt::GHOSTTY_SUCCESS
            {
                cell_style = CellStyle {
                    fg: style_color_string(&style.fg_color),
                    bg: style_color_string(&style.bg_color),
                    bold: style.bold,
                    inverse: style.inverse,
                };
            }
        }

        // EL/ECH store their SGR background directly on each erased cell's
        // content tag, not in its style. `HAS_STYLING` is therefore false for
        // the blank tail even though Ghostty renders it with a background.
        // Ask the resolved-color API when the explicit style had no bg so
        // full-width TUI fills survive viewport serialization.
        if cell_style.bg.is_none() {
            let mut rgb = vt::GhosttyColorRgb::default();
            if vt::ghostty_render_state_row_cells_get(
                cells,
                vt::GHOSTTY_RENDER_STATE_ROW_CELLS_DATA_BG_COLOR,
                &mut rgb as *mut _ as *mut _,
            ) == vt::GHOSTTY_SUCCESS
            {
                cell_style.bg = Some(format!("rgb:{},{},{}", rgb.r, rgb.g, rgb.b));
            }
        }

        (text, cell_style)
    }
}

fn style_color_string(color: &vt::GhosttyStyleColor) -> Option<String> {
    match color.tag {
        vt::GHOSTTY_STYLE_COLOR_PALETTE => {
            let index = unsafe { color.value.palette };
            if index < 16 {
                Some(format!("ansi:{index}"))
            } else {
                Some(format!("ansi256:{index}"))
            }
        }
        vt::GHOSTTY_STYLE_COLOR_RGB => {
            let rgb = unsafe { color.value.rgb };
            Some(format!("rgb:{},{},{}", rgb.r, rgb.g, rgb.b))
        }
        _ => None,
    }
}

fn read_row_cells(cells: vt::GhosttyRenderStateRowCells, with_styles: bool) -> TerminalViewportRow {
    let mut pieces: Vec<String> = Vec::new();
    let mut cell_styles: Vec<CellStyle> = Vec::new();
    let mut previous_style = CellStyle::default();

    while unsafe { vt::ghostty_render_state_row_cells_next(cells) } {
        let (text, style) = read_cell(cells, with_styles, &previous_style);
        previous_style = style.clone();
        pieces.push(text);
        cell_styles.push(style);
    }

    // Trailing trim mirrors the old parser: drop trailing blank cells (and
    // any spacer pieces) from the text payload, keep everything before.
    let text_end = pieces
        .iter()
        .rposition(|piece| !piece.is_empty() && piece != " ")
        .map(|index| index + 1)
        .unwrap_or(0);
    let text: String = pieces[..text_end].concat();

    let styles = if with_styles {
        style_runs(&cell_styles)
    } else {
        Vec::new()
    };
    TerminalViewportRow {
        text,
        styles,
        wrapped: false,
    }
}

fn style_runs(cell_styles: &[CellStyle]) -> Vec<TerminalViewportStyleRun> {
    let default_style = CellStyle::default();
    let mut styles = Vec::new();
    let mut run_start: Option<usize> = None;
    let mut run_style: Option<&CellStyle> = None;

    for (index, style) in cell_styles.iter().enumerate() {
        if style == &default_style {
            if let (Some(start), Some(style)) = (run_start.take(), run_style.take()) {
                styles.push(viewport_style_run(start, index - start, style));
            }
            continue;
        }

        if run_style.is_some_and(|run| run == style) {
            continue;
        }

        if let (Some(start), Some(style)) = (run_start.replace(index), run_style.replace(style)) {
            styles.push(viewport_style_run(start, index - start, style));
        }
    }

    if let (Some(start), Some(style)) = (run_start, run_style) {
        styles.push(viewport_style_run(start, cell_styles.len() - start, style));
    }

    styles
}

fn viewport_style_run(start: usize, len: usize, style: &CellStyle) -> TerminalViewportStyleRun {
    TerminalViewportStyleRun {
        start: start.min(u16::MAX as usize) as u16,
        len: len.min(u16::MAX as usize) as u16,
        fg: style.fg.clone(),
        bg: style.bg.clone(),
        bold: style.bold,
        inverse: style.inverse,
    }
}

/// Render a snapshot window from a terminal. Scrolls the terminal viewport to
/// cover the requested window (possibly in multiple passes when the window is
/// taller than the grid) and pins it back to the bottom afterwards.
fn snapshot_terminal(
    term: &mut VtTerminal,
    output_offset: u64,
    truncated: bool,
    scroll_offset_rows: u32,
    viewport_row_count: Option<u16>,
) -> TerminalViewportSnapshot {
    let grid_rows = term.rows as usize;
    let grid_cols = term.cols;
    let scrollback_rows = term.scrollback_rows();
    let total_rows = scrollback_rows + grid_rows;
    let viewport_row_count = viewport_row_count
        .map(|value| value.max(1) as usize)
        .unwrap_or(grid_rows)
        .min(total_rows.max(1));
    let max_scroll_offset = total_rows.saturating_sub(viewport_row_count) as u32;
    let scroll_offset_rows = scroll_offset_rows.min(max_scroll_offset);
    let viewport_start_row = total_rows
        .saturating_sub(viewport_row_count)
        .saturating_sub(scroll_offset_rows as usize);

    let end_row = viewport_start_row + viewport_row_count;
    let mut viewport_rows = Vec::with_capacity(viewport_row_count);
    let mut absolute_row = viewport_start_row;
    while absolute_row < end_row {
        // Any absolute row is reachable from a viewport position in
        // [0, scrollback_rows]; skip inside the rendered pass when the
        // position had to clamp.
        let position = absolute_row.min(scrollback_rows);
        term.scroll_to_row(position);
        let skip = absolute_row - position;
        let rendered = term.viewport_rows(true);
        if skip >= rendered.len() {
            break;
        }
        for row in rendered.into_iter().skip(skip) {
            if absolute_row >= end_row {
                break;
            }
            viewport_rows.push(row);
            absolute_row += 1;
        }
    }
    term.scroll_bottom();

    let (cursor_col, cursor_row) = term.cursor();
    TerminalViewportSnapshot {
        cols: grid_cols,
        rows: grid_rows as u16,
        output_offset,
        truncated,
        cursor_row: cursor_row.min((grid_rows as u16).saturating_sub(1)),
        cursor_col: cursor_col.min(grid_cols.saturating_sub(1)),
        scrollback_rows: scrollback_rows as u32,
        viewport_start_row: viewport_start_row as u32,
        scroll_offset_rows,
        input_modes_known: true,
        mouse_reporting: term.mouse_reporting(),
        mouse_button_motion: term.mode(1002) || term.mode(1003),
        mouse_any_motion: term.mode(1003),
        alternate_screen: term.alternate_screen(),
        mouse_alternate_scroll: term.mode(1007),
        application_cursor: term.mode(1),
        viewport_rows,
    }
}

pub struct TerminalViewportState {
    term: VtTerminal,
    /// Bounded raw-output tail for `snapshot_resized` replays.
    resize_replay: Vec<u8>,
    output_offset: u64,
    history_truncated: bool,
}

impl TerminalViewportState {
    pub fn new(cols: u16, rows: u16) -> Self {
        Self {
            term: VtTerminal::new(cols, rows),
            resize_replay: Vec::new(),
            output_offset: 0,
            history_truncated: false,
        }
    }

    /// Absolute byte offset immediately after the output parsed so far.
    pub fn output_offset(&self) -> u64 {
        self.output_offset
    }

    /// Replace the parsed terminal before feeding a rebased output stream.
    ///
    /// Remote renderers use this when the Host cannot continue from the
    /// Controller's committed cursor. `output_offset` is the absolute offset
    /// of the first byte that will be fed after the reset.
    pub fn reset_at_output_offset(
        &mut self,
        cols: u16,
        rows: u16,
        output_offset: u64,
        history_truncated: bool,
    ) {
        self.term = VtTerminal::new(cols, rows);
        self.resize_replay.clear();
        self.output_offset = output_offset;
        self.history_truncated = history_truncated;
    }

    pub fn feed(&mut self, data: &[u8]) {
        self.term.write(data);
        self.resize_replay.extend_from_slice(data);
        if self.resize_replay.len() > RESIZE_REPLAY_MAX_BYTES {
            let overflow = self.resize_replay.len() - RESIZE_REPLAY_MAX_BYTES;
            self.resize_replay.drain(..overflow);
        }
        self.output_offset = self.output_offset.saturating_add(data.len() as u64);
    }

    /// Current cursor position, 0-based (row, col) — the host's terminal
    /// probe responder reports it for CPR (`CSI 6 n`) answers while no
    /// surface is attached.
    pub fn cursor_position(&self) -> (u16, u16) {
        let (col, row) = self.term.cursor();
        (row, col)
    }

    /// True while the fed stream is inside a DEC 2026 synchronized update —
    /// renderers must not snapshot the grid (it is a declared mid-repaint,
    /// typically erased but not yet redrawn).
    pub fn synchronized_output_active(&self) -> bool {
        self.term.synchronized_output()
    }

    /// The currently visible screen (the `rows`×`cols` grid, excluding
    /// scrollback) as newline-separated text with trailing blanks trimmed per
    /// row. Mirrors what the phone's `readViewportText()` returns; used to scan
    /// for agent-drawn select-menu footers (see `crate::menu_prompt`).
    pub fn current_screen_text(&mut self) -> String {
        self.term.scroll_bottom();
        let rows = self.term.viewport_rows(false);
        let mut out = String::with_capacity(rows.len() * (self.term.cols as usize + 1));
        for row in rows {
            out.push_str(row.text.trim_end());
            out.push('\n');
        }
        out
    }

    pub fn resize(&mut self, cols: u16, rows: u16) {
        self.term.resize(cols, rows);
    }

    pub fn snapshot(
        &mut self,
        scroll_offset_rows: u32,
        viewport_row_count: Option<u16>,
    ) -> TerminalViewportSnapshot {
        snapshot_terminal(
            &mut self.term,
            self.output_offset,
            self.history_truncated,
            scroll_offset_rows,
            viewport_row_count,
        )
    }

    /// Non-perturbing snapshot at a different grid size: replays the retained
    /// output tail into a fresh terminal so remote/mobile clients cannot
    /// disturb the live viewport that desktop attach owns via explicit Resize.
    pub fn snapshot_resized(
        &self,
        cols: u16,
        rows: u16,
        scroll_offset_rows: u32,
        viewport_row_count: Option<u16>,
    ) -> TerminalViewportSnapshot {
        let mut replay = VtTerminal::new(cols, rows);
        replay.write(&self.resize_replay);
        snapshot_terminal(
            &mut replay,
            self.output_offset,
            self.history_truncated,
            scroll_offset_rows,
            viewport_row_count,
        )
    }
}

struct TerminalViewportReplayCache {
    cols: u16,
    rows: u16,
    max_bytes: usize,
    output_offset: u64,
    truncated: bool,
    term: VtTerminal,
}

static REPLAY_CACHE: OnceLock<Mutex<HashMap<String, TerminalViewportReplayCache>>> =
    OnceLock::new();

fn replay_cache() -> &'static Mutex<HashMap<String, TerminalViewportReplayCache>> {
    REPLAY_CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn cached_replay_snapshot(
    session_id: &str,
    cols: u16,
    rows: u16,
    max_bytes: usize,
    output_offset: u64,
    scroll_offset_rows: u32,
    viewport_row_count: Option<u16>,
) -> Option<TerminalViewportSnapshot> {
    let mut guard = replay_cache().lock().ok()?;
    let cache = guard.get_mut(session_id)?;
    if cache.cols == cols
        && cache.rows == rows
        && cache.max_bytes == max_bytes
        && cache.output_offset == output_offset
    {
        let truncated = cache.truncated;
        return Some(snapshot_terminal(
            &mut cache.term,
            output_offset,
            truncated,
            scroll_offset_rows,
            viewport_row_count,
        ));
    }
    None
}

/// Cap on distinct session ids retained in the replay cache. Each entry holds a
/// full virtual terminal (up to `MAX_VIEWPORT_SCROLLBACK_BYTES` of history), so
/// the map is bounded rather than growing once-per-session-id forever. Only ever
/// populated by the one-shot `__viewport__` CLI today, but the bound keeps it
/// safe if a long-lived process ever calls the replay entry points.
const REPLAY_CACHE_MAX_ENTRIES: usize = 64;

fn store_replay_cache(
    session_id: String,
    cols: u16,
    rows: u16,
    max_bytes: usize,
    output_offset: u64,
    truncated: bool,
    term: VtTerminal,
) {
    if let Ok(mut guard) = replay_cache().lock() {
        // Evict when full and this is a new session id. Without access-time
        // tracking a precise LRU isn't worth it here; dropping one arbitrary
        // entry keeps the map bounded (a re-request just re-renders).
        if guard.len() >= REPLAY_CACHE_MAX_ENTRIES && !guard.contains_key(&session_id) {
            if let Some(evict) = guard.keys().next().cloned() {
                guard.remove(&evict);
            }
        }
        guard.insert(
            session_id,
            TerminalViewportReplayCache {
                cols,
                rows,
                max_bytes,
                output_offset,
                truncated,
                term,
            },
        );
    }
}

#[cfg(test)]
pub fn render_terminal_viewport_from_bytes(
    data: &[u8],
    cols: u16,
    rows: u16,
    output_offset: u64,
    truncated: bool,
    scroll_offset_rows: u32,
    viewport_row_count: Option<u16>,
) -> TerminalViewportSnapshot {
    let mut term = VtTerminal::new(cols, rows);
    term.write(data);
    snapshot_terminal(
        &mut term,
        output_offset,
        truncated,
        scroll_offset_rows,
        viewport_row_count,
    )
}

/// Reject session ids that could escape the app-sessions directory. The viewport
/// entry points build `output_path`/`session.sock` paths straight from the id,
/// so an id containing `/`, `..`, `\`, or an absolute path must never be
/// accepted (matches `transcripts::load_safe_manifest` and `mcp_host`).
fn ensure_safe_session_id(session_id: &str) -> Result<(), String> {
    if session_id.is_empty()
        || session_id.contains('/')
        || session_id.contains("..")
        || session_id.contains('\\')
    {
        return Err("Invalid session id".into());
    }
    Ok(())
}

pub fn read_terminal_viewport_snapshot(
    session_id: String,
    cols: u16,
    rows: u16,
    max_bytes: Option<usize>,
    scroll_offset_rows: Option<u32>,
    viewport_rows: Option<u16>,
) -> Result<TerminalViewportSnapshot, String> {
    ensure_safe_session_id(&session_id)?;
    if cols == 0 || rows == 0 {
        return Err("Terminal viewport dimensions must be greater than zero".into());
    }

    let scroll_offset_rows = scroll_offset_rows.unwrap_or(0);
    if let Ok(snapshot) = request_terminal_viewport_snapshot(
        &session_id,
        cols,
        rows,
        scroll_offset_rows,
        viewport_rows,
    ) {
        return Ok(snapshot);
    }

    let max_bytes = max_bytes
        .unwrap_or(DEFAULT_VIEWPORT_REPLAY_MAX_BYTES)
        .clamp(1, MAX_VIEWPORT_REPLAY_MAX_BYTES);

    replay_snapshot_from_disk(
        session_id,
        cols,
        rows,
        max_bytes,
        scroll_offset_rows,
        viewport_rows,
    )
}

pub fn replay_terminal_viewport_snapshot(
    session_id: String,
    cols: u16,
    rows: u16,
    max_bytes: Option<usize>,
    scroll_offset_rows: Option<u32>,
    viewport_rows: Option<u16>,
) -> Result<TerminalViewportSnapshot, String> {
    ensure_safe_session_id(&session_id)?;
    if cols == 0 || rows == 0 {
        return Err("Terminal viewport dimensions must be greater than zero".into());
    }

    let scroll_offset_rows = scroll_offset_rows.unwrap_or(0);
    let max_bytes = max_bytes
        .unwrap_or(DEFAULT_VIEWPORT_REPLAY_MAX_BYTES)
        .clamp(1, MAX_VIEWPORT_REPLAY_MAX_BYTES);

    replay_snapshot_from_disk(
        session_id,
        cols,
        rows,
        max_bytes,
        scroll_offset_rows,
        viewport_rows,
    )
}

fn replay_snapshot_from_disk(
    session_id: String,
    cols: u16,
    rows: u16,
    max_bytes: usize,
    scroll_offset_rows: u32,
    viewport_rows: Option<u16>,
) -> Result<TerminalViewportSnapshot, String> {
    if let Ok(metadata) = std::fs::metadata(output_path(&session_id)) {
        if let Some(snapshot) = cached_replay_snapshot(
            &session_id,
            cols,
            rows,
            max_bytes,
            metadata.len(),
            scroll_offset_rows,
            viewport_rows,
        ) {
            return Ok(snapshot);
        }
    }

    let chunk = read_output_chunk(&session_id, None, Some(max_bytes), Some(max_bytes))?;
    let start_offset = chunk.next_offset.saturating_sub(chunk.data.len() as u64);
    let truncated = start_offset > 0;
    let mut term = VtTerminal::new(cols, rows);
    term.write(&chunk.data);
    let snapshot = snapshot_terminal(
        &mut term,
        chunk.next_offset,
        truncated,
        scroll_offset_rows,
        viewport_rows,
    );
    if chunk.exists {
        store_replay_cache(
            session_id,
            cols,
            rows,
            max_bytes,
            chunk.next_offset,
            truncated,
            term,
        );
    }
    Ok(snapshot)
}

pub fn run_cli(args: &[String]) -> Result<(), String> {
    let mode = args.first().map(String::as_str).unwrap_or("snapshot");
    let session_id = args.get(1).ok_or(
        "usage: unpeel-host __viewport__ snapshot <session-id> --cols N --rows N [--max-bytes N] [--scroll-offset-rows N] [--viewport-rows N]",
    )?;
    if mode != "snapshot" {
        return Err(
            "usage: unpeel-host __viewport__ snapshot <session-id> --cols N --rows N [--max-bytes N] [--scroll-offset-rows N] [--viewport-rows N]"
                .to_string(),
        );
    }

    let cols = flag_u16(args, "--cols").unwrap_or(120);
    let rows = flag_u16(args, "--rows").unwrap_or(31);
    let max_bytes = flag_usize(args, "--max-bytes");
    let scroll_offset_rows = flag_u32(args, "--scroll-offset-rows");
    let viewport_rows = flag_u16(args, "--viewport-rows");
    let snapshot = replay_terminal_viewport_snapshot(
        session_id.to_string(),
        cols,
        rows,
        max_bytes,
        scroll_offset_rows,
        viewport_rows,
    )?;
    let body = serde_json::to_string_pretty(&snapshot)
        .map_err(|e| format!("Failed to serialize viewport snapshot: {e}"))?;
    println!("{body}");
    Ok(())
}

fn flag_string(args: &[String], name: &str) -> Option<String> {
    args.windows(2)
        .find(|pair| pair[0] == name)
        .map(|pair| pair[1].clone())
}

fn flag_usize(args: &[String], name: &str) -> Option<usize> {
    flag_string(args, name)?.parse().ok()
}

fn flag_u32(args: &[String], name: &str) -> Option<u32> {
    flag_string(args, name)?.parse().ok()
}

fn flag_u16(args: &[String], name: &str) -> Option<u16> {
    flag_string(args, name)?.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::{render_terminal_viewport_from_bytes, TerminalViewportState, VtTerminal};

    // Leak-regression: the replay cache is bounded — inserting many distinct
    // session ids evicts old entries instead of growing forever (F3).
    #[test]
    fn replay_cache_is_bounded_by_max_entries() {
        let cap = super::REPLAY_CACHE_MAX_ENTRIES;
        for i in 0..(cap * 3) {
            super::store_replay_cache(
                format!("leak-cache-session-{i}"),
                80,
                24,
                4096,
                i as u64,
                false,
                VtTerminal::new(80, 24),
            );
        }
        let len = super::replay_cache().lock().unwrap().len();
        assert!(len <= cap, "replay cache grew to {len}, cap is {cap}");
    }

    #[test]
    fn viewport_snapshot_rejects_traversal_session_ids() {
        for bad in ["../../etc", "a/b", "..", "", "x\\y"] {
            assert!(
                super::read_terminal_viewport_snapshot(bad.to_string(), 80, 24, None, None, None)
                    .is_err(),
                "expected {bad:?} to be rejected"
            );
            assert!(super::replay_terminal_viewport_snapshot(
                bad.to_string(),
                80,
                24,
                None,
                None,
                None
            )
            .is_err());
        }
    }

    fn rows(snapshot: &super::TerminalViewportSnapshot) -> Vec<String> {
        snapshot
            .viewport_rows
            .iter()
            .map(|row| row.text.clone())
            .collect()
    }

    fn numbered_lines(count: usize) -> Vec<u8> {
        let mut output = String::new();
        for index in 0..count {
            if index > 0 {
                output.push_str("\r\n");
            }
            output.push_str(&format!("line{index:03}"));
        }
        output.into_bytes()
    }

    #[test]
    fn viewport_renders_basic_text_and_scrolls() {
        let snapshot =
            render_terminal_viewport_from_bytes(b"one\r\ntwo\r\nthree", 8, 2, 15, false, 0, None);
        assert_eq!(rows(&snapshot), vec!["two", "three"]);
        assert_eq!(snapshot.scrollback_rows, 1);
        let history =
            render_terminal_viewport_from_bytes(b"one\r\ntwo\r\nthree", 8, 2, 15, false, 1, None);
        assert_eq!(rows(&history), vec!["one", "two"]);
        assert_eq!(snapshot.cursor_row, 1);
        assert_eq!(snapshot.cursor_col, 5);
    }

    #[test]
    fn viewport_returns_requested_history_window() {
        let output = numbered_lines(10);
        let snapshot = render_terminal_viewport_from_bytes(
            &output,
            8,
            3,
            output.len() as u64,
            false,
            2,
            Some(5),
        );

        assert_eq!(snapshot.scrollback_rows, 7);
        assert_eq!(snapshot.scroll_offset_rows, 2);
        assert_eq!(snapshot.viewport_start_row, 3);
        assert_eq!(
            rows(&snapshot),
            vec!["line003", "line004", "line005", "line006", "line007"]
        );
    }

    #[test]
    fn viewport_clamps_scroll_offset_to_available_history() {
        let output = numbered_lines(4);
        let snapshot = render_terminal_viewport_from_bytes(
            &output,
            8,
            2,
            output.len() as u64,
            false,
            999,
            None,
        );

        assert_eq!(snapshot.scroll_offset_rows, 2);
        assert_eq!(snapshot.viewport_start_row, 0);
        assert_eq!(rows(&snapshot), vec!["line000", "line001"]);
    }

    #[test]
    fn viewport_clamps_requested_window_to_available_rows() {
        let output = numbered_lines(3);
        let snapshot = render_terminal_viewport_from_bytes(
            &output,
            8,
            2,
            output.len() as u64,
            false,
            0,
            Some(99),
        );

        assert_eq!(snapshot.viewport_rows.len(), 3);
        assert_eq!(snapshot.viewport_start_row, 0);
        assert_eq!(rows(&snapshot), vec!["line000", "line001", "line002"]);
    }

    #[test]
    fn viewport_handles_cursor_position_and_erase_line() {
        let snapshot = render_terminal_viewport_from_bytes(
            b"abcdef\x1b[1;3HXY\x1b[K",
            8,
            2,
            14,
            false,
            0,
            None,
        );
        assert_eq!(rows(&snapshot), vec!["abXY", ""]);
        assert_eq!(snapshot.cursor_row, 0);
        assert_eq!(snapshot.cursor_col, 4);
    }

    #[test]
    fn viewport_tracks_sgr_styles() {
        let snapshot =
            render_terminal_viewport_from_bytes(b"\x1b[1;31mR\x1b[0mN", 4, 1, 12, false, 0, None);
        assert_eq!(rows(&snapshot), vec!["RN"]);
        assert_eq!(snapshot.viewport_rows[0].styles.len(), 1);
        let style = &snapshot.viewport_rows[0].styles[0];
        assert_eq!(style.start, 0);
        assert_eq!(style.len, 1);
        assert_eq!(style.fg.as_deref(), Some("ansi:1"));
        assert!(style.bold);
    }

    #[test]
    fn viewport_merges_adjacent_style_cells_and_omits_default_gaps() {
        let snapshot = render_terminal_viewport_from_bytes(
            b"\x1b[31mAB\x1b[0mC\x1b[31mD",
            6,
            1,
            18,
            false,
            0,
            None,
        );

        assert_eq!(rows(&snapshot), vec!["ABCD"]);
        assert_eq!(snapshot.viewport_rows[0].styles.len(), 2);
        assert_eq!(snapshot.viewport_rows[0].styles[0].start, 0);
        assert_eq!(snapshot.viewport_rows[0].styles[0].len, 2);
        assert_eq!(
            snapshot.viewport_rows[0].styles[0].fg.as_deref(),
            Some("ansi:1")
        );
        assert_eq!(snapshot.viewport_rows[0].styles[1].start, 3);
        assert_eq!(snapshot.viewport_rows[0].styles[1].len, 1);
    }

    #[test]
    fn viewport_style_runs_cover_inverse_spaces() {
        let snapshot =
            render_terminal_viewport_from_bytes(b"\x1b[7m A \x1b[0mZ", 5, 1, 13, false, 0, None);

        assert_eq!(rows(&snapshot), vec![" A Z"]);
        assert_eq!(snapshot.viewport_rows[0].styles.len(), 1);
        assert_eq!(snapshot.viewport_rows[0].styles[0].start, 0);
        assert_eq!(snapshot.viewport_rows[0].styles[0].len, 3);
        assert!(snapshot.viewport_rows[0].styles[0].inverse);
    }

    #[test]
    fn viewport_style_runs_cover_erased_background_to_the_edge() {
        // Codex paints each composer row by setting a background and using
        // EL. Ghostty stores those erased cells as background-only content
        // tags rather than ordinary styled cells.
        let snapshot = render_terminal_viewport_from_bytes(
            b"\x1b[48;2;53;53;57m\x1b[K",
            8,
            1,
            21,
            false,
            0,
            None,
        );

        assert_eq!(snapshot.viewport_rows[0].text, "");
        assert_eq!(snapshot.viewport_rows[0].styles.len(), 1);
        let style = &snapshot.viewport_rows[0].styles[0];
        assert_eq!(style.start, 0);
        assert_eq!(style.len, 8);
        assert_eq!(style.bg.as_deref(), Some("rgb:53,53,57"));
    }

    #[test]
    fn viewport_trims_default_trailing_spaces_from_payload() {
        let snapshot = render_terminal_viewport_from_bytes(b"x", 80, 2, 1, false, 0, None);

        assert_eq!(snapshot.viewport_rows.len(), 2);
        assert_eq!(snapshot.viewport_rows[0].text, "x");
        assert_eq!(snapshot.viewport_rows[1].text, "");
    }

    #[test]
    fn viewport_tracks_extended_sgr_colors() {
        let snapshot = render_terminal_viewport_from_bytes(
            b"\x1b[38;5;196mA\x1b[48;2;1;2;3mB",
            4,
            1,
            28,
            false,
            0,
            None,
        );

        assert_eq!(rows(&snapshot), vec!["AB"]);
        assert_eq!(snapshot.viewport_rows[0].styles.len(), 2);
        assert_eq!(
            snapshot.viewport_rows[0].styles[0].fg.as_deref(),
            Some("ansi256:196")
        );
        assert_eq!(
            snapshot.viewport_rows[0].styles[1].bg.as_deref(),
            Some("rgb:1,2,3")
        );
    }

    #[test]
    fn viewport_handles_save_restore_cursor() {
        let snapshot =
            render_terminal_viewport_from_bytes(b"ab\x1b7cd\x1b8Z", 6, 1, 9, false, 0, None);

        assert_eq!(rows(&snapshot), vec!["abZd"]);
        assert_eq!(snapshot.cursor_row, 0);
        assert_eq!(snapshot.cursor_col, 3);
    }

    #[test]
    fn viewport_ignores_osc_payloads() {
        let snapshot =
            render_terminal_viewport_from_bytes(b"a\x1b]0;title\x07b", 4, 1, 12, false, 0, None);
        assert_eq!(rows(&snapshot), vec!["ab"]);
    }

    #[test]
    fn viewport_state_handles_split_csi_sequences() {
        let mut state = TerminalViewportState::new(4, 1);
        state.feed(b"\x1b[31");
        state.feed(b"mR\x1b[0");
        state.feed(b"mN");
        let snapshot = state.snapshot(0, None);

        assert_eq!(rows(&snapshot), vec!["RN"]);
        assert_eq!(snapshot.viewport_rows[0].styles.len(), 1);
        assert_eq!(
            snapshot.viewport_rows[0].styles[0].fg.as_deref(),
            Some("ansi:1")
        );
    }

    #[test]
    fn viewport_state_handles_split_osc_sequences() {
        let mut state = TerminalViewportState::new(6, 1);
        state.feed(b"a\x1b]0;ti");
        state.feed(b"tle\x07b");
        let snapshot = state.snapshot(0, None);

        assert_eq!(rows(&snapshot), vec!["ab"]);
    }

    #[test]
    fn viewport_state_tracks_synchronized_output_mode() {
        let mut state = TerminalViewportState::new(8, 2);
        assert!(!state.synchronized_output_active());
        state.feed(b"\x1b[?2026h\x1b[2Jrepainting");
        assert!(state.synchronized_output_active());
        // Split across a chunk boundary, like the socket delivers it.
        state.feed(b"\x1b[?20");
        assert!(state.synchronized_output_active());
        state.feed(b"26l");
        assert!(!state.synchronized_output_active());
    }

    #[test]
    fn viewport_snapshot_reports_when_the_child_owns_mouse_buttons() {
        let off = render_terminal_viewport_from_bytes(b"plain text", 8, 2, 10, false, 0, None);
        assert!(!off.mouse_reporting);

        let on = render_terminal_viewport_from_bytes(
            b"\x1b[?1002h\x1b[?1006hclickable",
            8,
            2,
            20,
            false,
            0,
            None,
        );
        assert!(on.input_modes_known);
        assert!(on.mouse_reporting);
        assert!(on.mouse_button_motion);
        assert!(!on.mouse_any_motion);
        assert!(!on.alternate_screen);

        let reset = render_terminal_viewport_from_bytes(
            b"\x1b[?1003h\x1b[?1003lplain",
            8,
            2,
            20,
            false,
            0,
            None,
        );
        assert!(!reset.mouse_reporting);

        let alternate = render_terminal_viewport_from_bytes(
            b"\x1b[?1049h\x1b[?1007h\x1b[?1hfull screen",
            8,
            2,
            30,
            false,
            0,
            None,
        );
        assert!(alternate.alternate_screen);
        assert!(alternate.mouse_alternate_scroll);
        assert!(alternate.application_cursor);
    }

    #[test]
    fn viewport_rows_preserve_soft_wrap_for_clipboard_unwrapping() {
        let snapshot = render_terminal_viewport_from_bytes(b"abcdefgh", 4, 2, 8, false, 0, None);
        assert_eq!(snapshot.viewport_rows[0].text, "abcd");
        assert!(snapshot.viewport_rows[0].wrapped);
        assert_eq!(snapshot.viewport_rows[1].text, "efgh");
        assert!(!snapshot.viewport_rows[1].wrapped);
    }

    #[test]
    fn viewport_state_incremental_feed_matches_batch_parse() {
        let chunks = [
            b"one\r\n".as_slice(),
            b"\x1b[31".as_slice(),
            b"mtwo\x1b[0m\r\nthr".as_slice(),
            b"ee".as_slice(),
        ];
        let mut state = TerminalViewportState::new(8, 2);
        for chunk in chunks {
            state.feed(chunk);
        }
        let incremental = state.snapshot(0, None);
        let batch = render_terminal_viewport_from_bytes(
            b"one\r\n\x1b[31mtwo\x1b[0m\r\nthree",
            8,
            2,
            24,
            false,
            0,
            None,
        );

        assert_eq!(rows(&incremental), rows(&batch));
        assert_eq!(
            incremental.viewport_rows[0].styles,
            batch.viewport_rows[0].styles
        );
    }

    #[test]
    fn viewport_state_tracks_absolute_output_offset() {
        let mut state = TerminalViewportState::new(8, 2);
        assert_eq!(state.output_offset(), 0);

        state.feed(b"one");
        state.feed(b" two");

        assert_eq!(state.output_offset(), 7);
        assert_eq!(state.snapshot(0, None).output_offset, 7);
    }

    #[test]
    fn viewport_state_reset_rebases_and_reports_truncated_history() {
        let mut state = TerminalViewportState::new(8, 2);
        state.feed(b"stale output");

        state.reset_at_output_offset(6, 1, 40, true);
        assert_eq!(state.output_offset(), 40);
        state.feed(b"fresh");

        let snapshot = state.snapshot(0, None);
        assert_eq!(snapshot.cols, 6);
        assert_eq!(snapshot.rows, 1);
        assert_eq!(snapshot.output_offset, 45);
        assert!(snapshot.truncated);
        assert_eq!(rows(&snapshot), vec!["fresh"]);

        let resized = state.snapshot_resized(10, 2, 0, None);
        assert_eq!(resized.output_offset, 45);
        assert!(resized.truncated);
        assert_eq!(rows(&resized), vec!["fresh", ""]);
    }

    #[test]
    fn viewport_state_resize_preserves_bottom_rows() {
        let mut state = TerminalViewportState::new(8, 2);
        state.feed(&numbered_lines(4));
        state.resize(8, 3);
        let snapshot = state.snapshot(0, None);

        assert_eq!(rows(&snapshot), vec!["line001", "line002", "line003"]);
        assert_eq!(snapshot.scrollback_rows, 1);
    }

    #[test]
    fn viewport_state_resize_smaller_moves_trimmed_rows_to_scrollback() {
        let mut state = TerminalViewportState::new(8, 4);
        state.feed(&numbered_lines(4));
        state.resize(8, 2);

        let bottom = state.snapshot(0, None);
        assert_eq!(rows(&bottom), vec!["line002", "line003"]);
        assert_eq!(bottom.scrollback_rows, 2);

        let history = state.snapshot(2, None);
        assert_eq!(rows(&history), vec!["line000", "line001"]);
    }

    #[test]
    fn viewport_state_snapshot_resized_does_not_mutate_state() {
        let mut state = TerminalViewportState::new(8, 4);
        state.feed(&numbered_lines(4));

        let smaller = state.snapshot_resized(8, 2, 0, None);
        assert_eq!(rows(&smaller), vec!["line002", "line003"]);

        let original = state.snapshot(0, None);
        assert_eq!(
            rows(&original),
            vec!["line000", "line001", "line002", "line003"]
        );
        assert_eq!(original.rows, 4);
    }

    #[test]
    fn current_screen_text_feeds_the_menu_detector() {
        use crate::menu_prompt::viewport_has_menu_prompt;

        let mut state = TerminalViewportState::new(60, 6);
        state.feed(b"\x1b[2J\x1b[H"); // clear + home
        state.feed(b"  1. Switch to $59/year\r\n");
        state.feed(b"  2. Keep perpetual one-time\r\n");
        state.feed(b"\r\n");
        state.feed(b"Enter to select \xc2\xb7 \xe2\x86\x91/\xe2\x86\x93 to navigate \xc2\xb7 Esc to cancel");

        let screen = state.current_screen_text();
        assert!(screen.contains("1. Switch to $59/year"));
        assert!(viewport_has_menu_prompt(&screen));

        // Answering the menu redraws the screen; the footer is gone, so the
        // detector must clear.
        state.feed(b"\x1b[2J\x1b[HWorking on it\xe2\x80\xa6");
        assert!(!viewport_has_menu_prompt(&state.current_screen_text()));
    }

    #[test]
    fn viewport_retains_bounded_scrollback() {
        // Feed well past the byte budget twice; scrollback must trim to a
        // steady state instead of growing with input (ghostty trims with
        // page granularity, so exact row counts are not asserted).
        let mut state = TerminalViewportState::new(80, 4);
        let block = format!("{}\r\n", "x".repeat(78)).repeat(100_000);

        state.feed(block.as_bytes());
        let first = state.snapshot(0, Some(1)).scrollback_rows;
        assert!(
            (first as usize) < 100_000,
            "scrollback {first} was never trimmed"
        );

        state.feed(block.as_bytes());
        let second = state.snapshot(0, Some(1)).scrollback_rows;
        assert!(
            second <= first.saturating_add(first / 4),
            "scrollback kept growing: {first} -> {second}"
        );
    }

    #[test]
    fn viewport_handles_wide_characters() {
        // "日本" is two wide chars (4 columns); the spacer cells must not
        // inject extra columns into the text payload.
        let snapshot =
            render_terminal_viewport_from_bytes("日本x".as_bytes(), 8, 1, 7, false, 0, None);
        assert_eq!(rows(&snapshot), vec!["日本x"]);
    }
}
